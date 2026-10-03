use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use aether_contracts::{ExecutionPlan, ExecutionResult, ResponseBody};
use aether_crypto::DEVELOPMENT_ENCRYPTION_KEY;
use aether_data::repository::global_models::InMemoryGlobalModelReadRepository;
use aether_data::repository::provider_catalog::InMemoryProviderCatalogReadRepository;
use aether_data_contracts::repository::provider_catalog::ProviderCatalogReadRepository;
use axum::{routing::post, Json, Router};
use http::StatusCode;
use serde_json::{json, Value};

use super::super::{
    build_router_with_state, build_state_with_execution_runtime_override,
    sample_admin_provider_model, sample_bound_key, sample_endpoint, sample_provider, start_server,
    AppState,
};
use crate::constants::{
    GATEWAY_HEADER, TRUSTED_ADMIN_SESSION_ID_HEADER, TRUSTED_ADMIN_USER_ID_HEADER,
    TRUSTED_ADMIN_USER_ROLE_HEADER,
};
use crate::data::GatewayDataState;

fn admin_post(url: &str) -> reqwest::RequestBuilder {
    reqwest::Client::new()
        .post(url)
        .header(GATEWAY_HEADER, "rust-phase3b")
        .header(TRUSTED_ADMIN_USER_ID_HEADER, "admin-user-123")
        .header(TRUSTED_ADMIN_USER_ROLE_HEADER, "admin")
        .header(TRUSTED_ADMIN_SESSION_ID_HEADER, "session-123")
}

#[test]
fn gateway_provider_regressions_jev_create_and_manual_native_endpoint() {
    crate::tests::run_async_test_on_large_stack(
        "jev_create_and_endpoint",
        16 * 1024 * 1024,
        || async {
            let mut existing = sample_provider("jev-existing", "existing-jev", 0);
            existing.provider_type = "jev".to_string();
            let repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
                vec![existing],
                vec![],
                vec![],
            ));
            let gateway =
                build_router_with_state(AppState::new().unwrap().with_data_state_for_tests(
                    GatewayDataState::with_provider_catalog_repository_for_tests(
                        repository.clone(),
                    ),
                ));
            let (url, handle) = start_server(gateway).await;
            for name in ["first-jev", "second-jev"] {
                let response = admin_post(&format!("{url}/api/admin/providers/"))
                    .json(&json!({"name": name, "provider_type": "jev"}))
                    .send()
                    .await
                    .unwrap();
                let status = response.status();
                let body: Value = response.json().await.unwrap();
                assert_eq!(status, StatusCode::OK, "{body}");
                assert_eq!(body["name"], name);
                let providers = repository.list_providers(false).await.unwrap();
                let created = providers
                    .iter()
                    .find(|provider| provider.name == name)
                    .unwrap();
                let endpoints = repository
                    .list_endpoints_by_provider_ids(&[created.id.clone()])
                    .await
                    .unwrap();
                assert_eq!(endpoints.len(), 1);
                assert_eq!(endpoints[0].api_format, "typesafe:systemone");
                assert!(endpoints[0].custom_path.is_none());
            }
            let response = admin_post(&format!("{url}/api/admin/endpoints/providers/jev-existing/endpoints"))
                .json(&json!({"provider_id":"jev-existing", "api_format":"typesafe:systemone", "base_url":"https://api.typesafe.ai/v1", "custom_path":"/systemone", "is_active":true}))
            .send().await.unwrap();
            let status = response.status();
            let body: Value = response.json().await.unwrap();
            assert_eq!(status, StatusCode::OK, "{body}");
            let endpoints = repository
                .list_endpoints_by_provider_ids(&["jev-existing".to_string()])
                .await
                .unwrap();
            assert_eq!(endpoints.len(), 1);
            assert_eq!(endpoints[0].api_family.as_deref(), Some("typesafe"));
            assert_eq!(endpoints[0].endpoint_kind.as_deref(), Some("systemone"));
            handle.abort();
        },
    );
}

#[test]
fn gateway_provider_regressions_clinepass_api_key_quota_without_oauth() {
    crate::tests::run_async_test_on_large_stack(
        "clinepass_api_key_quota",
        16 * 1024 * 1024,
        || async {
            let seen = Arc::new(Mutex::new(Vec::<String>::new()));
            let capture = seen.clone();
            let runtime = Router::new().route("/v1/execute/sync", post(move |Json(plan): Json<ExecutionPlan>| {
            let capture = capture.clone();
            async move {
                assert_eq!(plan.method, "GET");
                assert_eq!(plan.headers.get("authorization").map(String::as_str), Some("Bearer sk-clinepass"));
                capture.lock().unwrap().push(plan.url.clone());
                let body = if plan.url.ends_with("/usage-limits") {
                    json!({"success":true,"data":{"limits":[{"type":"five_hour","percentUsed":10},{"type":"weekly","percentUsed":20},{"type":"monthly","percentUsed":35}]}})
                } else {
                    json!({"success":true,"data":{"plan":{"displayName":"Cline Pass (Monthly)","isActive":true,"entitlements":{"cline_pass":{"inferenceCapThreshold":{"last5HoursUsageCostUSDPerUser":1000000000}}}}}})
                };
                Json(ExecutionResult {request_id:plan.request_id,candidate_id:plan.candidate_id,status_code:200,headers:BTreeMap::new(),response_observation:None,
                    body:Some(ResponseBody{json_body:Some(body),body_bytes_b64:None}),telemetry:None,error:None})
            }
        }));
            let (runtime_url, runtime_handle) = start_server(runtime).await;
            let mut provider = sample_provider("clinepass", "ClinePass", 0);
            provider.provider_type = "clinepass".to_string();
            let key = sample_bound_key("cline-key", "clinepass", "openai:chat", "sk-clinepass");
            assert!(key.encrypted_auth_config.is_none());
            let repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
                vec![provider],
                vec![sample_endpoint(
                    "cline-endpoint",
                    "clinepass",
                    "openai:chat",
                    "https://api.cline.bot/api/v1",
                )],
                vec![key],
            ));
            let state = build_state_with_execution_runtime_override(runtime_url)
                .with_data_state_for_tests(
                    GatewayDataState::with_provider_catalog_repository_for_tests(
                        repository.clone(),
                    )
                    .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY),
                );
            let (url, handle) = start_server(build_router_with_state(state)).await;
            let response = admin_post(&format!(
                "{url}/api/admin/endpoints/providers/clinepass/refresh-quota"
            ))
            .send()
            .await
            .unwrap();
            let status = response.status();
            let body: Value = response.json().await.unwrap();
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(body["success"], 1, "{body}");
            assert_eq!(body["failed"], 0);
            let keys = repository
                .list_keys_by_provider_ids(&["clinepass".to_string()])
                .await
                .unwrap();
            assert!(keys[0].encrypted_auth_config.is_none());
            let windows = &keys[0].status_snapshot.as_ref().unwrap()["quota"]["windows"];
            assert_eq!(windows[0]["remaining_ratio"], 0.9);
            assert_eq!(windows[0]["cap_usd"], 10.0);
            assert_eq!(windows[1]["remaining_ratio"], 0.8);
            assert_eq!(windows[2]["remaining_ratio"], 0.65);
            let mut urls = seen.lock().unwrap().clone();
            urls.sort();
            assert_eq!(
                urls,
                vec![
                    "https://api.cline.bot/api/v1/users/me/plan",
                    "https://api.cline.bot/api/v1/users/me/plan/usage-limits"
                ]
            );
            handle.abort();
            runtime_handle.abort();
        },
    );
}

#[test]
fn gateway_provider_regressions_clinepass_mapped_test_manual_model_and_probe() {
    crate::tests::run_async_test_on_large_stack(
        "clinepass_model_test_and_probe",
        16 * 1024 * 1024,
        || async {
            let seen = Arc::new(Mutex::new(Vec::<String>::new()));
            let capture = seen.clone();
            let runtime = Router::new().route("/v1/execute/sync", post(move |Json(plan): Json<ExecutionPlan>| {
            let capture = capture.clone();
            async move {
                let request = plan.body.json_body.as_ref().unwrap();
                let model = request["model"].as_str().unwrap();
                capture.lock().unwrap().push(model.to_string());
                Json(ExecutionResult {request_id:plan.request_id,candidate_id:plan.candidate_id,status_code:200,headers:BTreeMap::new(),response_observation:None,
                    body:Some(ResponseBody{json_body:Some(json!({"id":"chatcmpl-cline","object":"chat.completion","model":model,"provider":"baseten",
                        "choices":[{"index":0,"message":{"role":"assistant","content":"Hi"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}})),body_bytes_b64:None}),telemetry:None,error:None})
            }
        }));
            let (runtime_url, runtime_handle) = start_server(runtime).await;
            let mut provider = sample_provider("clinepass", "ClinePass", 0);
            provider.provider_type = "clinepass".to_string();
            let repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
                vec![provider],
                vec![sample_endpoint(
                    "cline-endpoint",
                    "clinepass",
                    "openai:chat",
                    "https://api.cline.bot/api/v1",
                )],
                vec![sample_bound_key(
                    "cline-key",
                    "clinepass",
                    "openai:chat",
                    "sk-clinepass",
                )],
            ));
            let mut model = sample_admin_provider_model(
                "model-cline",
                "clinepass",
                "global-deepseek",
                "deepseek-v4.1-flash",
            );
            model.global_model_name = Some("deepseek-v4.1-flash".to_string());
            model.provider_model_mappings =
                Some(json!([{"name":"cline-pass/deepseek-v4.1-flash","priority":1}]));
            let models = Arc::new(
                InMemoryGlobalModelReadRepository::seed(vec![])
                    .with_admin_provider_models(vec![model]),
            );
            let state = build_state_with_execution_runtime_override(runtime_url)
                .with_data_state_for_tests(
                    GatewayDataState::with_provider_transport_reader_for_tests(
                        repository,
                        DEVELOPMENT_ENCRYPTION_KEY.to_string(),
                    )
                    .with_global_model_repository_for_tests(models),
                );
            let (url, handle) = start_server(build_router_with_state(state)).await;
            for manual in [false, true] {
                let request_model = if manual {
                    "cline-pass/glm-5.3"
                } else {
                    "cline-pass/deepseek-v4.1-flash"
                };
                let response = admin_post(&format!("{url}/api/admin/provider-query/test-model-failover"))
                .json(&json!({"provider_id":"clinepass","model_name":"deepseek-v4.1-flash","mode":"global","apply_model_mapping":true,
                    "endpoint_id":"cline-endpoint","api_format":"openai:chat",
                    "failover_models":["deepseek-v4.1-flash"],"request_body":{"model":request_model,"messages":[{"role":"user","content":"Hi"}]}}))
                .send().await.unwrap();
                let status = response.status();
                let body: Value = response.json().await.unwrap();
                assert_eq!(status, StatusCode::OK, "{body}");
                assert_eq!(body["success"], true, "{body}");
                let expected = if manual {
                    request_model
                } else {
                    "cline-pass/deepseek-v4.1-flash"
                };
                assert_eq!(body["attempts"][0]["request_body"]["model"], expected);
                assert_eq!(body["attempts"][0]["effective_model"], expected);
            }
            let response = admin_post(&format!("{url}/api/admin/provider-query/test-model"))
            .json(&json!({"provider_id":"clinepass","model_name":"deepseek-v4.1-flash","clinepass_probe":true}))
            .send().await.unwrap();
            let body: Value = response.json().await.unwrap();
            assert_eq!(body["success"], true, "{body}");
            assert_eq!(
                *seen.lock().unwrap(),
                vec![
                    "cline-pass/deepseek-v4.1-flash",
                    "cline-pass/glm-5.3",
                    "cline-pass/deepseek-v4.1-flash",
                    "cline-pass/deepseek-v4.1-flash"
                ]
            );
            handle.abort();
            runtime_handle.abort();
        },
    );
}
