use std::collections::BTreeMap;

use aether_contracts::{ExecutionPlan, ExecutionTimeouts, RequestBody};
use aether_provider_transport::{resolve_transport_profile, GatewayProviderTransportSnapshot};
use serde_json::{json, Value};

use crate::transport::ModelFetchTransportRuntime;

pub(crate) const CLINEPASS_CATALOG_URL: &str = "https://models.dev/api.json";

pub(crate) async fn fetch_clinepass_models(
    runtime: &(impl ModelFetchTransportRuntime + ?Sized),
    transport: &GatewayProviderTransportSnapshot,
) -> Result<Vec<Value>, String> {
    let plan = ExecutionPlan {
        request_id: format!("clinepass-models:{}", transport.provider.id),
        candidate_id: None,
        provider_name: Some("ClinePass".to_string()),
        provider_id: transport.provider.id.clone(),
        endpoint_id: transport.endpoint.id.clone(),
        key_id: transport.key.id.clone(),
        method: "GET".to_string(),
        url: CLINEPASS_CATALOG_URL.to_string(),
        // This public directory must never receive the account's API key or header rules.
        headers: BTreeMap::from([("accept".to_string(), "application/json".to_string())]),
        content_type: None,
        content_encoding: None,
        body: RequestBody {
            json_body: None,
            body_bytes_b64: None,
            body_ref: None,
        },
        stream: false,
        client_api_format: "openai:chat".to_string(),
        provider_api_format: "clinepass:catalog".to_string(),
        model_name: Some("models".to_string()),
        proxy: runtime.resolve_model_fetch_proxy(transport).await,
        transport_profile: resolve_transport_profile(transport),
        timeouts: Some(ExecutionTimeouts {
            total_ms: Some(30_000),
            ..Default::default()
        }),
    };
    let result = runtime.execute_model_fetch_execution_plan(&plan).await?;
    let body = crate::strategy::execution_result_json_body(&result)?;
    parse_clinepass_catalog(&body)
}

fn parse_clinepass_catalog(body: &Value) -> Result<Vec<Value>, String> {
    let models = body
        .pointer("/providers/cline-pass/models")
        .or_else(|| body.pointer("/cline-pass/models"))
        .and_then(Value::as_object)
        .ok_or_else(|| "models.dev did not return the ClinePass model directory".to_string())?;
    let mut cards = BTreeMap::new();
    for (raw_id, model) in models {
        let raw_id = raw_id.trim().to_ascii_lowercase();
        if raw_id.is_empty() || raw_id.len() > 240 || raw_id.chars().any(char::is_control) {
            continue;
        }
        let id = if raw_id.starts_with("cline-pass/") {
            raw_id
        } else {
            format!("cline-pass/{raw_id}")
        };
        if id == "cline-pass/" {
            continue;
        }
        cards.insert(id.clone(), json!({
            "id": id, "object": "model", "owned_by": "clinepass",
            "display_name": model.get("name").and_then(Value::as_str).unwrap_or(&id),
            "api_formats": ["openai:chat"], "api_format": "openai:chat",
            "context_window": model.pointer("/limit/context"), "max_output_tokens": model.pointer("/limit/output"),
            "supports_extended_thinking": model.get("reasoning"),
            "supports_function_calling": model.get("tool_call"),
            "supports_vision": model.pointer("/modalities/input").and_then(Value::as_array).is_some_and(|inputs| inputs.iter().any(|input| input == "image")),
        }));
    }
    if cards.is_empty() {
        return Err("models.dev returned an empty ClinePass model directory".to_string());
    }
    Ok(cards.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_only_uses_cline_pass_models_and_preserves_prefix_and_capabilities() {
        for wrapped in [false, true] {
            let directory = json!({"cline-pass":{"models":{
                "deepseek-v4.1-flash":{"name":"DeepSeek V4.1 Flash","reasoning":true,"tool_call":true,"limit":{"context":1000000,"output":65536}},
                "cline-pass/glm-5.3":{},
            }},"openrouter":{"models":{"deepseek/deepseek-v4.1-flash":{}}}});
            let body = if wrapped {
                json!({"providers":directory})
            } else {
                directory
            };
            let cards = parse_clinepass_catalog(&body).unwrap();
            assert_eq!(cards.len(), 2);
            assert_eq!(cards[0]["id"], "cline-pass/deepseek-v4.1-flash");
            assert_eq!(cards[0]["context_window"], 1000000);
            assert_eq!(cards[1]["id"], "cline-pass/glm-5.3");
        }
        assert!(parse_clinepass_catalog(&json!({"openrouter":{"models":{}}})).is_err());
    }
}
