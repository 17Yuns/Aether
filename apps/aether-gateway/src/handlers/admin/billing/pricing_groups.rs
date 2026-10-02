use super::build_admin_billing_bad_request_response;
use crate::{
    handlers::admin::request::{AdminAppState, AdminRequestContext},
    GatewayError,
};
use aether_billing::groups::{PricingGroupsConfig, PRICING_GROUPS_CONFIG_KEY};
use axum::{
    body::{Body, Bytes},
    http,
    response::{IntoResponse, Response},
    Json,
};

pub(super) async fn maybe_build_pricing_groups_response(
    state: &AdminAppState<'_>,
    context: &AdminRequestContext<'_>,
    body: Option<&Bytes>,
) -> Result<Option<Response<Body>>, GatewayError> {
    if context.path().trim_end_matches('/') != "/api/admin/billing/pricing-groups" {
        return Ok(None);
    }
    if context.method() == http::Method::GET {
        let config = PricingGroupsConfig::from_value(
            state
                .read_system_config_json_value_strong(PRICING_GROUPS_CONFIG_KEY)
                .await?,
        )
        .map_err(GatewayError::Internal)?;
        if let Some(model_id) = crate::handlers::admin::shared::query_param_value(
            context.query_string(),
            "global_model_id",
        ) {
            let Some(model) = state
                .app()
                .data
                .get_admin_global_model_by_id(&model_id)
                .await
                .map_err(|error| GatewayError::Internal(error.to_string()))?
            else {
                return Ok(Some(build_admin_billing_bad_request_response("模型不存在")));
            };
            let base = serde_json::json!({
                "default_tiered_pricing": model.default_tiered_pricing,
                "default_price_per_request": model.default_price_per_request,
                "config": model.config,
            });
            let quotes = config
                .groups
                .iter()
                .filter(|_| config.enabled)
                .map(|group| {
                    let available = group.allows_model(&model_id);
                    let pricing = if available {
                        aether_billing::groups::scale_model_sale_prices(&base, group.multiplier)
                            .ok_or_else(|| GatewayError::Internal("分组价格无效".to_string()))?
                    } else {
                        serde_json::json!({})
                    };
                    let mut quote = pricing;
                    quote["id"] = serde_json::json!(group.id);
                    quote["name"] = serde_json::json!(group.name);
                    quote["multiplier"] = serde_json::json!(group.multiplier);
                    quote["is_visible"] = serde_json::json!(group.is_visible);
                    quote["is_available"] = serde_json::json!(available);
                    Ok(quote)
                })
                .collect::<Result<Vec<_>, GatewayError>>()?;
            return Ok(Some(
                Json(serde_json::json!({
                    "enabled": config.enabled, "base_pricing": base, "group_prices": quotes,
                }))
                .into_response(),
            ));
        }
        return Ok(Some(Json(config).into_response()));
    }
    if context.method() == http::Method::PUT {
        let Some(body) = body else {
            return Ok(Some(build_admin_billing_bad_request_response(
                "请求体不能为空",
            )));
        };
        let config = match serde_json::from_slice::<PricingGroupsConfig>(body) {
            Ok(config) => config,
            Err(_) => {
                return Ok(Some(build_admin_billing_bad_request_response(
                    "分组定价配置格式无效",
                )))
            }
        };
        if let Err(error) = config.validate() {
            return Ok(Some(build_admin_billing_bad_request_response(error)));
        }
        let mut checked = std::collections::BTreeMap::new();
        for group in &config.groups {
            for model in group.model_access.iter().flatten() {
                if !checked.contains_key(&model.global_model_id) {
                    if state
                        .app()
                        .data
                        .get_admin_global_model_by_id(&model.global_model_id)
                        .await
                        .map_err(|error| GatewayError::Internal(error.to_string()))?
                        .is_none()
                    {
                        return Ok(Some(build_admin_billing_bad_request_response(
                            "分组包含不存在的模型，请刷新模型列表",
                        )));
                    }
                    let providers = state
                        .app()
                        .data
                        .list_admin_provider_models_by_global_model_id(&model.global_model_id)
                        .await
                        .map_err(|error| GatewayError::Internal(error.to_string()))?
                        .into_iter()
                        .map(|model| model.provider_id)
                        .collect::<std::collections::BTreeSet<_>>();
                    checked.insert(model.global_model_id.clone(), providers);
                }
                if model
                    .provider_ids
                    .iter()
                    .any(|id| !checked[&model.global_model_id].contains(id))
                {
                    return Ok(Some(build_admin_billing_bad_request_response(
                        "分组选择的供应商未关联该模型，请刷新供应商列表",
                    )));
                }
            }
        }
        let value = serde_json::to_value(&config)
            .map_err(|error| GatewayError::Internal(error.to_string()))?;
        state
            .upsert_system_config_json_value(
                PRICING_GROUPS_CONFIG_KEY,
                &value,
                Some("API Key 分组倍率及供应商模型调用范围"),
            )
            .await?;
        return Ok(Some(Json(config).into_response()));
    }
    Ok(None)
}
