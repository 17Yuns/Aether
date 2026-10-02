use std::collections::BTreeMap;

use aether_contracts::{
    ExecutionPlan, ExecutionTimeouts, RequestBody, EXECUTION_REQUEST_FOLLOW_REDIRECTS_HEADER,
};
use axum::{
    body::Body,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

use super::super::{
    payload::{provider_query_extract_model, provider_query_extract_provider_id},
    response::build_admin_provider_query_bad_request_response,
};
use crate::{
    handlers::admin::request::{AdminAppState, AdminGatewayProviderTransportSnapshot},
    GatewayError,
};

const IMPOSSIBLE_CHANNEL: &str = "aether-clinepass-probe-no-such-provider";

pub(crate) async fn probe_clinepass_channels(
    state: &AdminAppState<'_>,
    payload: &Value,
) -> Result<Response<Body>, GatewayError> {
    let Some(provider_id) = provider_query_extract_provider_id(payload) else {
        return Ok(build_admin_provider_query_bad_request_response(
            "provider_id is required",
        ));
    };
    let Some(model) = provider_query_extract_model(payload)
        .filter(|model| model.len() <= 256 && !model.chars().any(char::is_control))
    else {
        return Ok(build_admin_provider_query_bad_request_response(
            "model is required",
        ));
    };
    let provider_ids = vec![provider_id.clone()];
    let provider = state
        .read_provider_catalog_providers_by_ids(&provider_ids)
        .await?
        .into_iter()
        .next();
    if !provider.is_some_and(|provider| provider.provider_type == "clinepass") {
        return Ok(build_admin_provider_query_bad_request_response(
            "Only ClinePass supports this channel probe",
        ));
    }
    let endpoints = state
        .app()
        .list_provider_catalog_endpoints_by_provider_ids(&provider_ids)
        .await?;
    let keys = state
        .app()
        .list_provider_catalog_keys_by_provider_ids(&provider_ids)
        .await?;
    let endpoint = endpoints
        .iter()
        .find(|endpoint| endpoint.is_active && endpoint.api_format == "openai:chat");
    let key = keys.iter().find(|key| {
        key.is_active
            && payload
                .get("api_key_id")
                .and_then(Value::as_str)
                .is_none_or(|id| id == key.id)
    });
    let (Some(endpoint), Some(key)) = (endpoint, key) else {
        return Ok(build_admin_provider_query_bad_request_response(
            "ClinePass requires an active Chat endpoint and API key",
        ));
    };
    let Some(transport) = state
        .read_provider_transport_snapshot(&provider_id, &endpoint.id, &key.id)
        .await?
    else {
        return Ok(build_admin_provider_query_bad_request_response(
            "ClinePass transport is unavailable",
        ));
    };
    let first = request(state, &transport, json!({"model":model,"messages":[{"role":"user","content":"hi"}],"max_tokens":256,"stream":false})).await?;
    if first
        .get("choices")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
    {
        return Ok(Json(json!({"success":false,"error":"ClinePass 模型探测未成功，请检查模型名称、密钥和套餐额度"})).into_response());
    }
    let pipeline = if routing_metadata(&first).is_some() {
        "planner"
    } else if first.get("provider").and_then(Value::as_str).is_some() {
        "direct"
    } else {
        ""
    };
    let mut second_body = json!({"model":model,"messages":[{"role":"user","content":"hi"}],"max_tokens":16,"stream":false});
    if pipeline == "planner" {
        second_body["providerOptions"] = json!({"gateway":{"only":[IMPOSSIBLE_CHANNEL]}});
    } else if pipeline == "direct" {
        second_body["provider"] = json!({"only":[IMPOSSIBLE_CHANNEL]});
    }
    let second = if pipeline.is_empty() {
        Value::Null
    } else {
        request(state, &transport, second_body).await?
    };
    Ok(
        Json(json!({"success":true,"clinepass_probe":parse_probe(&first, &second, pipeline)}))
            .into_response(),
    )
}

async fn request(
    state: &AdminAppState<'_>,
    transport: &AdminGatewayProviderTransportSnapshot,
    body: Value,
) -> Result<Value, GatewayError> {
    let plan = ExecutionPlan {
        request_id: format!("clinepass-probe:{}", uuid::Uuid::new_v4()),
        candidate_id: None,
        provider_name: Some("ClinePass".to_string()),
        provider_id: transport.provider.id.clone(),
        endpoint_id: transport.endpoint.id.clone(),
        key_id: transport.key.id.clone(),
        method: "POST".to_string(),
        url: format!(
            "{}/chat/completions",
            transport.endpoint.base_url.trim_end_matches('/')
        ),
        headers: BTreeMap::from([
            (
                "authorization".to_string(),
                format!("Bearer {}", transport.key.decrypted_api_key.trim()),
            ),
            ("accept".to_string(), "application/json".to_string()),
            (
                EXECUTION_REQUEST_FOLLOW_REDIRECTS_HEADER.to_string(),
                "false".to_string(),
            ),
        ]),
        content_type: Some("application/json".to_string()),
        content_encoding: None,
        body: RequestBody::from_json(body),
        stream: false,
        client_api_format: "openai:chat".to_string(),
        provider_api_format: "openai:chat".to_string(),
        model_name: None,
        proxy: state
            .resolve_transport_proxy_snapshot_with_tunnel_affinity(transport)
            .await,
        transport_profile: state.resolve_transport_profile(transport),
        timeouts: Some(ExecutionTimeouts {
            total_ms: Some(60_000),
            ..ExecutionTimeouts::default()
        }),
    };
    let result = state
        .execute_execution_runtime_sync_plan(Some(&plan.request_id), &plan)
        .await?;
    Ok(super::model_test::provider_query_execution_json_body(&result).unwrap_or(Value::Null))
}

fn routing_metadata(value: &Value) -> Option<&Value> {
    value
        .pointer("/provider_metadata/gateway/routing")
        .or_else(|| {
            value
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| {
                    choices.iter().find_map(|choice| {
                        choice.pointer("/message/provider_metadata/gateway/routing")
                    })
                })
        })
}

fn add_channel(channels: &mut Vec<String>, value: &str) {
    let value = value.trim().trim_matches(['\'', '"']);
    let slug = aether_provider_transport::clinepass::normalize_clinepass_channel(value);
    if !value.is_empty()
        && !slug.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-/ ".contains(&byte))
        && !channels.iter().any(|channel| channel == &slug)
    {
        channels.push(slug);
    }
}

fn parse_probe(first: &Value, second: &Value, pipeline: &str) -> Value {
    let mut channels = Vec::new();
    let routing = routing_metadata(first);
    if let Some(routing) = routing {
        for field in ["finalProvider", "resolvedProvider"] {
            if let Some(value) = routing.get(field).and_then(Value::as_str) {
                add_channel(&mut channels, value);
            }
        }
        if let Some(fallbacks) = routing.get("fallbacksAvailable").and_then(Value::as_array) {
            for value in fallbacks.iter().filter_map(Value::as_str) {
                add_channel(&mut channels, value);
            }
        }
        if let Some(plan) = routing.get("planningReasoning").and_then(Value::as_str) {
            let lower = plan.to_ascii_lowercase();
            for (prefix, separator) in [
                ("total execution order:", "→"),
                ("system credentials planned for:", ","),
            ] {
                if let Some(index) = lower.find(prefix) {
                    let segment = plan[index + prefix.len()..]
                        .lines()
                        .next()
                        .unwrap_or("")
                        .replace("->", "→");
                    for token in segment.split(separator) {
                        let token = token
                            .split('(')
                            .next()
                            .unwrap_or("")
                            .trim()
                            .trim_matches(['.', ';', '\'', '"']);
                        if token
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
                        {
                            add_channel(&mut channels, token);
                        }
                    }
                    break;
                }
            }
        }
    }
    if let Some(channel) = first.get("provider").and_then(Value::as_str) {
        add_channel(&mut channels, channel);
    }
    let completed = second
        .get("choices")
        .and_then(Value::as_array)
        .is_some_and(|choices| !choices.is_empty());
    let message = second
        .pointer("/error/message")
        .and_then(Value::as_str)
        .or_else(|| second.get("error").and_then(Value::as_str))
        .unwrap_or("");
    let structured = message
        .find('{')
        .and_then(|offset| serde_json::from_str::<Value>(&message[offset..]).ok());
    let available = second
        .pointer("/error/metadata/available_providers")
        .or_else(|| {
            structured
                .as_ref()
                .and_then(|value| value.pointer("/error/metadata/available_providers"))
        })
        .and_then(Value::as_array);
    if let Some(available) = available {
        for channel in available.iter().filter_map(Value::as_str) {
            add_channel(&mut channels, channel);
        }
    }
    let lower = message.to_ascii_lowercase();
    if let Some(index) = lower.find("available providers are:") {
        let text = &message[index + "available providers are:".len()..];
        for channel in text.split(',') {
            add_channel(&mut channels, channel.trim().trim_end_matches('.'));
        }
    }
    let rejected = available.is_some()
        || lower.contains("no allowed providers")
        || lower.contains("no available providers")
        || lower.contains("provider.only")
        || lower.contains("requested_providers")
        || lower.contains("no provider matches")
        || lower.contains("no endpoints found that match")
        || lower.contains("no providers match");
    let pinnable = !pipeline.is_empty() && !completed && rejected && !channels.is_empty();
    let reason = if pinnable {
        None
    } else if pipeline.is_empty() {
        Some("上游未返回可识别的渠道商路由")
    } else if completed {
        Some("该模型忽略渠道商筛选参数")
    } else {
        Some("未能验证渠道商筛选支持，请重新探测")
    };
    json!({"available_channels":channels,"pipeline":pipeline,"pinnable":pinnable,"pin_reason":reason})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_distinguishes_ignored_preferences_and_harvests_rejection_channels() {
        let first = json!({"choices":[{"message":{"provider_metadata":{"gateway":{"routing":{"finalProvider":"baseten","fallbacksAvailable":["deepseek"]}}}}}]});
        let rejected = json!({"error":{"message":"No allowed providers. Available providers are: baseten, deepseek, google."}});
        let result = parse_probe(&first, &rejected, "planner");
        assert_eq!(
            result["available_channels"],
            json!(["baseten", "deepseek", "google"])
        );
        assert_eq!(result["pinnable"], true);
        assert_eq!(
            parse_probe(&first, &json!({"choices":[{}]}), "planner")["pinnable"],
            false
        );
        assert_eq!(
            parse_probe(
                &first,
                &json!({"error":{"message":"Invalid API key"}}),
                "planner"
            )["pinnable"],
            false
        );
    }
}
