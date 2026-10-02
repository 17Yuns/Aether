use std::collections::{BTreeMap, BTreeSet};

use aether_data_contracts::repository::global_models::{
    PublicCatalogModelListQuery, PublicGlobalModelQuery, StoredPublicGlobalModel,
    StoredPublicGlobalModelPage,
};
use axum::{
    body::Body,
    http,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use super::{
    build_admin_endpoint_health_status_payload, build_auth_error_response, query_param_value,
    resolve_authenticated_local_user, sanitize_public_model_capabilities,
    sanitize_public_model_config_for_user, sanitize_public_tiered_pricing, AppState,
    GatewayPublicRequestContext, USERS_ME_AVAILABLE_MODELS_FETCH_LIMIT,
};

const USERS_ME_MODEL_CATALOG_UNAVAILABLE_DETAIL: &str = "用户模型目录暂不可用";
const USERS_ME_PROVIDER_CATALOG_UNAVAILABLE_DETAIL: &str = "用户提供商目录暂不可用";
const USERS_ME_ENDPOINT_STATUS_UNAVAILABLE_DETAIL: &str = "用户端点健康数据暂不可用";

fn build_users_me_available_model_payload(
    model: StoredPublicGlobalModel,
    hide_mapping_config: bool,
) -> serde_json::Value {
    let (default_tiered_pricing, supported_capabilities, config) = if hide_mapping_config {
        (
            sanitize_public_tiered_pricing(model.default_tiered_pricing),
            sanitize_public_model_capabilities(model.supported_capabilities),
            sanitize_public_model_config_for_user(model.config),
        )
    } else {
        (
            model.default_tiered_pricing,
            model.supported_capabilities,
            model.config,
        )
    };
    json!({
        "id": model.id,
        "name": model.name,
        "display_name": model.display_name,
        "is_active": model.is_active,
        "default_price_per_request": model.default_price_per_request,
        "default_tiered_pricing": default_tiered_pricing,
        "supported_capabilities": supported_capabilities,
        "config": config,
        "usage_count": model.usage_count,
    })
}

fn parse_users_me_available_models_query(query: Option<&str>) -> (usize, usize, Option<String>) {
    let skip = query_param_value(query, "skip")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let limit = query_param_value(query, "limit")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (1..=1000).contains(value))
        .unwrap_or(100);
    let search = query_param_value(query, "search");
    (skip, limit, search)
}

fn users_me_allowed_provider_names(
    allowed_providers: Option<&[String]>,
) -> Option<BTreeSet<String>> {
    allowed_providers.map(|providers| {
        providers
            .iter()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect::<BTreeSet<_>>()
    })
}

async fn resolve_users_me_allowed_global_model_ids(
    state: &AppState,
    allowed_providers: Option<&[String]>,
) -> Result<Option<BTreeSet<String>>, Response<Body>> {
    let Some(allowed_providers) = allowed_providers else {
        return Ok(None);
    };
    if allowed_providers.is_empty() {
        return Ok(Some(BTreeSet::new()));
    }

    if !state.has_provider_catalog_data_reader() {
        return Err(build_auth_error_response(
            http::StatusCode::SERVICE_UNAVAILABLE,
            USERS_ME_PROVIDER_CATALOG_UNAVAILABLE_DETAIL,
            false,
        ));
    }

    let allowed_provider_names: BTreeSet<String> = allowed_providers
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();
    let providers = match state.list_provider_catalog_providers(true).await {
        Ok(value) => value,
        Err(err) => {
            return Err(build_auth_error_response(
                http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("user provider lookup failed: {err:?}"),
                false,
            ))
        }
    };
    let provider_ids = providers
        .into_iter()
        .filter(|provider| {
            allowed_provider_names.contains(&provider.id.to_ascii_lowercase())
                || allowed_provider_names.contains(&provider.name.to_ascii_lowercase())
                || allowed_provider_names.contains(&provider.provider_type.to_ascii_lowercase())
        })
        .map(|provider| provider.id)
        .collect::<Vec<_>>();
    if provider_ids.is_empty() {
        return Ok(Some(BTreeSet::new()));
    }

    let refs = match state
        .list_active_global_model_ids_by_provider_ids(&provider_ids)
        .await
    {
        Ok(value) => value,
        Err(err) => {
            return Err(build_auth_error_response(
                http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("user provider model lookup failed: {err:?}"),
                false,
            ))
        }
    };
    Ok(Some(
        refs.into_iter()
            .map(|entry| entry.global_model_id)
            .collect::<BTreeSet<_>>(),
    ))
}

fn public_pricing_group_payload(group: &aether_billing::groups::PricingGroup) -> serde_json::Value {
    json!({"id": group.id, "name": group.name, "multiplier": group.multiplier,
        "is_visible": group.is_visible})
}

fn scale_model_group_prices(
    model: &mut serde_json::Value,
    multiplier: f64,
) -> Result<(), &'static str> {
    *model =
        aether_billing::groups::scale_model_sale_prices(model, multiplier).ok_or("分组价格无效")?;
    Ok(())
}

pub(super) async fn handle_users_me_pricing_groups(
    state: &AppState,
    context: &GatewayPublicRequestContext,
    headers: &http::HeaderMap,
) -> Response<Body> {
    if let Err(response) = resolve_authenticated_local_user(state, context, headers).await {
        return response;
    }
    match state.read_pricing_groups_config().await {
        Ok(config) => Json(json!({ "enabled": config.enabled,
            "default_group_id": config.default_group_id, "groups": config.visible_groups().iter()
                .map(public_pricing_group_payload).collect::<Vec<_>>() }))
        .into_response(),
        Err(error) => build_auth_error_response(
            http::StatusCode::SERVICE_UNAVAILABLE,
            error.into_message(),
            false,
        ),
    }
}

pub(super) async fn handle_users_me_available_models(
    state: &AppState,
    request_context: &GatewayPublicRequestContext,
    headers: &http::HeaderMap,
) -> Response<Body> {
    if !state.has_global_model_data_reader() {
        return build_auth_error_response(
            http::StatusCode::SERVICE_UNAVAILABLE,
            USERS_ME_MODEL_CATALOG_UNAVAILABLE_DETAIL,
            false,
        );
    }

    let auth = match resolve_authenticated_local_user(state, request_context, headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let pricing_config = match state.read_pricing_groups_config().await {
        Ok(config) => config,
        Err(error) => {
            return build_auth_error_response(
                http::StatusCode::SERVICE_UNAVAILABLE,
                error.into_message(),
                false,
            )
        }
    };
    let selected_group_id = query_param_value(
        request_context.request_query_string.as_deref(),
        "pricing_group_id",
    );
    let group = match pricing_config.resolve(selected_group_id.as_deref()) {
        Ok(Some(group)) if !group.is_visible => {
            return build_auth_error_response(
                http::StatusCode::FORBIDDEN,
                "该定价分组不对用户开放",
                false,
            )
        }
        Ok(group) => group,
        Err(error) => {
            return build_auth_error_response(http::StatusCode::BAD_REQUEST, error, false)
        }
    };
    let (skip, limit, search) =
        parse_users_me_available_models_query(request_context.request_query_string.as_deref());

    let effective_policies = if auth.user.role.eq_ignore_ascii_case("admin") {
        None
    } else {
        match state
            .data
            .resolve_user_effective_list_policies(&auth.user)
            .await
        {
            Ok(value) => Some(value),
            Err(err) => {
                return build_auth_error_response(
                    http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("user policy lookup failed: {err:?}"),
                    false,
                )
            }
        }
    };
    let mut provider_model_ids = if auth.user.role.eq_ignore_ascii_case("admin") {
        None
    } else {
        match resolve_users_me_allowed_global_model_ids(
            state,
            effective_policies
                .as_ref()
                .and_then(|policies| policies.allowed_providers.as_deref()),
        )
        .await
        {
            Ok(value) => value,
            Err(response) => return response,
        }
    };
    let group_model_ids = match state
        .pricing_group_available_model_ids(
            &pricing_config,
            effective_policies
                .as_ref()
                .and_then(|policies| policies.allowed_providers.as_deref()),
        )
        .await
    {
        Ok(ids) => ids,
        Err(error) => {
            return build_auth_error_response(
                http::StatusCode::SERVICE_UNAVAILABLE,
                error.into_message(),
                false,
            )
        }
    };
    if let Some(allowed) = group
        .as_ref()
        .and_then(|group| group_model_ids.get(&group.id))
    {
        provider_model_ids = Some(match provider_model_ids {
            Some(existing) => existing.intersection(allowed).cloned().collect(),
            None => allowed.clone(),
        });
    }
    let allowed_models: Option<BTreeSet<String>> = if auth.user.role.eq_ignore_ascii_case("admin") {
        None
    } else {
        effective_policies
            .as_ref()
            .and_then(|policies| policies.allowed_models.as_ref())
            .map(|models: &Vec<String>| {
                models
                    .iter()
                    .map(|value: &String| value.trim().to_ascii_lowercase())
                    .filter(|value: &String| !value.is_empty())
                    .collect::<BTreeSet<_>>()
            })
    };

    let hide_mapping_config = !auth.user.role.eq_ignore_ascii_case("admin");

    let allowed_models: Option<BTreeSet<String>> = allowed_models.map(|models| {
        models
            .into_iter()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect::<BTreeSet<_>>()
    });

    let page = if provider_model_ids.is_none() && allowed_models.is_none() {
        match state
            .list_public_global_models(&PublicGlobalModelQuery {
                offset: skip,
                limit,
                is_active: Some(true),
                search,
            })
            .await
        {
            Ok(value) => value,
            Err(err) => {
                return build_auth_error_response(
                    http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("available model lookup failed: {err:?}"),
                    false,
                )
            }
        }
    } else {
        let page = match state
            .list_public_global_models(&PublicGlobalModelQuery {
                offset: 0,
                limit: USERS_ME_AVAILABLE_MODELS_FETCH_LIMIT,
                is_active: Some(true),
                search,
            })
            .await
        {
            Ok(value) => value,
            Err(err) => {
                return build_auth_error_response(
                    http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("available model lookup failed: {err:?}"),
                    false,
                )
            }
        };

        let filtered = page
            .items
            .into_iter()
            .filter(|model| {
                allowed_models
                    .as_ref()
                    .is_none_or(|allowed: &BTreeSet<String>| {
                        allowed.contains(&model.name.to_ascii_lowercase())
                    })
            })
            .filter(|model| {
                provider_model_ids
                    .as_ref()
                    .is_none_or(|allowed: &BTreeSet<String>| allowed.contains(&model.id))
            })
            .collect::<Vec<_>>();
        let total = filtered.len();
        let items = filtered
            .into_iter()
            .skip(skip)
            .take(limit)
            .collect::<Vec<_>>();
        StoredPublicGlobalModelPage { items, total }
    };

    let mut models = page
        .items
        .into_iter()
        .map(|model| build_users_me_available_model_payload(model, hide_mapping_config))
        .collect::<Vec<_>>();
    for model in &mut models {
        let base = model.clone();
        let mut quotes = Vec::new();
        for visible_group in pricing_config
            .groups
            .iter()
            .filter(|group| pricing_config.enabled && group.is_visible)
        {
            let available = group_model_ids
                .get(&visible_group.id)
                .is_none_or(|allowed| base["id"].as_str().is_some_and(|id| allowed.contains(id)));
            let mut quote = public_pricing_group_payload(visible_group);
            quote["is_available"] = json!(available);
            if available {
                let mut priced = base.clone();
                if let Err(error) = scale_model_group_prices(&mut priced, visible_group.multiplier)
                {
                    return build_auth_error_response(
                        http::StatusCode::SERVICE_UNAVAILABLE,
                        error,
                        false,
                    );
                }
                quote["default_tiered_pricing"] = priced["default_tiered_pricing"].clone();
                quote["default_price_per_request"] = priced["default_price_per_request"].clone();
                quote["config"] = priced["config"].clone();
            }
            quotes.push(quote);
        }
        if let Some(group) = &group {
            if let Err(error) = scale_model_group_prices(model, group.multiplier) {
                return build_auth_error_response(
                    http::StatusCode::SERVICE_UNAVAILABLE,
                    error,
                    false,
                );
            }
        }
        model["base_pricing"] = json!({
            "default_tiered_pricing": base["default_tiered_pricing"],
            "default_price_per_request": base["default_price_per_request"], "config": base["config"],
        });
        model["group_prices"] = json!(quotes);
        model["pricing_group"] = group
            .as_ref()
            .map(public_pricing_group_payload)
            .unwrap_or(serde_json::Value::Null);
    }
    Json(json!({
        "models": models, "total": page.total, "pricing_group": group.as_ref().map(public_pricing_group_payload),
    }))
    .into_response()
}

pub(super) async fn handle_users_me_providers_get(
    state: &AppState,
    request_context: &GatewayPublicRequestContext,
    headers: &http::HeaderMap,
) -> Response<Body> {
    if !state.has_provider_catalog_data_reader() {
        return build_auth_error_response(
            http::StatusCode::SERVICE_UNAVAILABLE,
            USERS_ME_PROVIDER_CATALOG_UNAVAILABLE_DETAIL,
            false,
        );
    }

    let auth = match resolve_authenticated_local_user(state, request_context, headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let expose_provider_details = auth.user.role.eq_ignore_ascii_case("admin");
    let allowed_provider_names = if expose_provider_details {
        None
    } else {
        let effective_policies = match state
            .data
            .resolve_user_effective_list_policies(&auth.user)
            .await
        {
            Ok(value) => value,
            Err(err) => {
                return build_auth_error_response(
                    http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("user policy lookup failed: {err:?}"),
                    false,
                )
            }
        };
        users_me_allowed_provider_names(effective_policies.allowed_providers.as_deref())
    };

    let mut providers = match state.list_provider_catalog_providers(true).await {
        Ok(value) => value,
        Err(err) => {
            return build_auth_error_response(
                http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("user provider lookup failed: {err:?}"),
                false,
            )
        }
    };
    if let Some(allowed_provider_names) = allowed_provider_names.as_ref() {
        providers.retain(|provider| {
            allowed_provider_names.contains(&provider.id.to_ascii_lowercase())
                || allowed_provider_names.contains(&provider.name.to_ascii_lowercase())
                || allowed_provider_names.contains(&provider.provider_type.to_ascii_lowercase())
        });
    }
    providers.sort_by(|left, right| {
        left.provider_priority
            .cmp(&right.provider_priority)
            .then_with(|| left.name.cmp(&right.name))
    });

    let provider_ids = providers
        .iter()
        .map(|provider| provider.id.clone())
        .collect::<Vec<_>>();
    let endpoints = match state
        .list_provider_catalog_endpoints_by_provider_ids(&provider_ids)
        .await
    {
        Ok(value) => value,
        Err(err) => {
            return build_auth_error_response(
                http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("user provider endpoint lookup failed: {err:?}"),
                false,
            )
        }
    };
    let mut endpoints_by_provider = BTreeMap::<String, Vec<serde_json::Value>>::new();
    for endpoint in endpoints {
        let mut endpoint_payload = json!({
            "id": endpoint.id,
            "api_format": endpoint.api_format,
            "is_active": endpoint.is_active,
        });
        if expose_provider_details {
            endpoint_payload["base_url"] = json!(endpoint.base_url);
        }
        endpoints_by_provider
            .entry(endpoint.provider_id)
            .or_default()
            .push(endpoint_payload);
    }

    let mut models_by_provider = BTreeMap::<String, Vec<serde_json::Value>>::new();
    if state.has_global_model_data_reader() {
        for provider_id in &provider_ids {
            let models = match state
                .list_public_catalog_models(&PublicCatalogModelListQuery {
                    provider_id: Some(provider_id.clone()),
                    offset: 0,
                    limit: 1000,
                })
                .await
            {
                Ok(value) => value,
                Err(err) => {
                    return build_auth_error_response(
                        http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!("user provider model lookup failed: {err:?}"),
                        false,
                    )
                }
            };
            models_by_provider.insert(
                provider_id.clone(),
                models
                    .into_iter()
                    .map(|model| {
                        json!({
                            "id": model.id,
                            "name": model.name,
                            "display_name": model.display_name,
                            "input_price_per_1m": model.input_price_per_1m,
                            "output_price_per_1m": model.output_price_per_1m,
                            "cache_creation_price_per_1m": model.cache_creation_price_per_1m,
                            "cache_read_price_per_1m": model.cache_read_price_per_1m,
                            "supports_vision": model.supports_vision,
                            "supports_function_calling": model.supports_function_calling,
                            "supports_streaming": model.supports_streaming,
                            "supports_embedding": model.supports_embedding,
                        })
                    })
                    .collect::<Vec<_>>(),
            );
        }
    }

    Json(
        providers
            .into_iter()
            .map(|provider| {
                let provider_id = provider.id.clone();
                let mut payload = json!({
                    "id": provider_id.clone(),
                    "provider_priority": provider.provider_priority,
                    "endpoints": endpoints_by_provider.remove(&provider_id).unwrap_or_default(),
                    "models": models_by_provider.remove(&provider_id).unwrap_or_default(),
                });
                if expose_provider_details {
                    let description = provider
                        .config
                        .as_ref()
                        .and_then(|value| value.get("description"))
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned);
                    payload["name"] = json!(provider.name);
                    payload["description"] = json!(description);
                }
                payload
            })
            .collect::<Vec<_>>(),
    )
    .into_response()
}

pub(super) async fn handle_users_me_endpoint_status_get(
    state: &AppState,
    request_context: &GatewayPublicRequestContext,
    headers: &http::HeaderMap,
) -> Response<Body> {
    match resolve_authenticated_local_user(state, request_context, headers).await {
        Ok(_) => {}
        Err(response) => return response,
    };

    let Some(payload) =
        build_admin_endpoint_health_status_payload(&crate::admin_api::AdminAppState::new(state), 6)
            .await
    else {
        return build_auth_error_response(
            http::StatusCode::SERVICE_UNAVAILABLE,
            USERS_ME_ENDPOINT_STATUS_UNAVAILABLE_DETAIL,
            false,
        );
    };
    let Some(items) = payload.as_array() else {
        return build_auth_error_response(
            http::StatusCode::INTERNAL_SERVER_ERROR,
            "endpoint status payload malformed",
            false,
        );
    };

    Json(serde_json::Value::Array(
        items.iter()
            .map(|item| {
                json!({
                    "api_format": item.get("api_format").cloned().unwrap_or(serde_json::Value::Null),
                    "display_name": item.get("display_name").cloned().unwrap_or(serde_json::Value::Null),
                    "health_score": item.get("health_score").cloned().unwrap_or(serde_json::Value::Null),
                    "timeline": item.get("timeline").cloned().unwrap_or_else(|| json!([])),
                    "time_range_start": item.get("time_range_start").cloned().unwrap_or(serde_json::Value::Null),
                    "time_range_end": item.get("time_range_end").cloned().unwrap_or(serde_json::Value::Null),
                })
            })
            .collect(),
    ))
    .into_response()
}
