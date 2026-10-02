use super::super::{
    any, build_router_with_state, build_state_with_execution_runtime_override, hash_api_key,
    sample_bound_key, sample_currently_usable_auth_snapshot, sample_endpoint, sample_provider,
    start_server, AppState, GatewayDataState, InMemoryAuthApiKeySnapshotRepository,
    InMemoryProviderCatalogReadRepository, Json, Router,
};
use aether_contracts::{ExecutionPlan, ExecutionResult, ResponseBody};
use aether_crypto::DEVELOPMENT_ENCRYPTION_KEY;
use aether_data::repository::candidate_selection::InMemoryMinimalCandidateSelectionReadRepository;
use aether_data_contracts::repository::candidate_selection::StoredMinimalCandidateSelectionRow;
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};

fn run_systemone_test<F, Fut>(name: &'static str, make_future: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    let handle = std::thread::Builder::new()
        .name(name.into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(make_future())
        })
        .unwrap();
    if let Err(payload) = handle.join() {
        std::panic::resume_unwind(payload);
    }
}

fn systemone_success_state(execution_runtime_url: String) -> AppState {
    let mut snapshot =
        sample_currently_usable_auth_snapshot("key-systemone-success", "user-systemone-success");
    snapshot.user_allowed_providers = None;
    snapshot.api_key_allowed_providers = None;
    snapshot.user_allowed_api_formats = Some(vec!["typesafe:systemone".to_string()]);
    snapshot.api_key_allowed_api_formats = Some(vec!["typesafe:systemone".to_string()]);
    snapshot.user_allowed_models = Some(vec!["jev-latest".to_string()]);
    snapshot.api_key_allowed_models = Some(vec!["jev-latest".to_string()]);
    let auth_repository = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![(
        Some(hash_api_key("sk-systemone-success")),
        snapshot,
    )]));
    let candidate_repository =
        Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(vec![
            systemone_candidate_row(),
        ]));
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![{
            let mut provider = sample_provider("provider-systemone", "Jev", 1);
            provider.provider_type = "jev".to_string();
            provider
        }],
        vec![sample_endpoint(
            "endpoint-systemone",
            "provider-systemone",
            "typesafe:systemone",
            "https://api.typesafe.ai/v1",
        )],
        vec![sample_bound_key(
            "key-upstream-systemone",
            "provider-systemone",
            "typesafe:systemone",
            "sk-upstream-systemone",
        )],
    ));
    let data_state =
        GatewayDataState::with_provider_catalog_and_minimal_candidate_selection_for_tests(
            provider_catalog_repository,
            candidate_repository,
        )
        .with_auth_api_key_reader(auth_repository)
        .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY);

    build_state_with_execution_runtime_override(execution_runtime_url)
        .with_data_state_for_tests(data_state)
}

fn systemone_candidate_row() -> StoredMinimalCandidateSelectionRow {
    StoredMinimalCandidateSelectionRow {
        provider_id: "provider-systemone".to_string(),
        provider_name: "Jev".to_string(),
        provider_type: "jev".to_string(),
        provider_priority: 1,
        provider_is_active: true,
        endpoint_id: "endpoint-systemone".to_string(),
        endpoint_api_format: "typesafe:systemone".to_string(),
        endpoint_api_family: Some("typesafe".to_string()),
        endpoint_kind: Some("systemone".to_string()),
        endpoint_is_active: true,
        key_id: "key-upstream-systemone".to_string(),
        key_name: "default".to_string(),
        key_auth_type: "api_key".to_string(),
        key_is_active: true,
        key_api_formats: Some(vec!["typesafe:systemone".to_string()]),
        key_allowed_models: None,
        key_capabilities: None,
        key_internal_priority: 50,
        key_global_priority_by_format: None,
        model_id: "model-systemone-base".to_string(),
        global_model_id: "global-systemone-base".to_string(),
        global_model_name: "jev-latest".to_string(),
        global_model_mappings: None,
        global_model_supports_streaming: Some(false),
        model_provider_model_name: "jev-preview".to_string(),
        model_provider_model_mappings: None,
        model_supports_streaming: Some(false),
        model_is_active: true,
        model_is_available: true,
    }
}

fn systemone_request() -> Value {
    json!({"model":"jev-latest","state":{"message":"Please refund the payment","amount":25},"questions":{
        "urgent":{"type":"noul","instructions":["Is this urgent?"],"criteria":{"true":{"meaning":"Requires action"},"false":"Can wait"}},
        "department":{"type":"choice","instructions":{"task":"Select department"},"criteria":{"billing":null,"support":["Technical issue"]}},
        "priority":{"type":"score","instructions":"Score priority","criteria":["Low",{"level":"High"}]}
    }})
}

fn systemone_response() -> Value {
    json!({"model":"jev-1.13.0","answers":{
        "urgent":{"type":"noul","noul":0.95},
        "department":{"type":"choice","choice":"billing","probabilities":{"billing":0.88,"support":0.12},"confidence":0.81},
        "priority":{"type":"score","score":0.9,"legend":{"0":"Low","1":"High"},"probabilities":{"0":0.1,"1":0.9},"confidence":0.92}
    },"usage":{"input_tokens":318,"output_tokens":34}})
}

fn systemone_runtime(status_code: u16, response_body: Value) -> Router {
    Router::new().route(
        "/v1/execute/sync",
        any(move |Json(plan): Json<ExecutionPlan>| {
            let response_body = response_body.clone();
            async move {
                assert_eq!(plan.client_api_format, "typesafe:systemone");
                assert_eq!(plan.provider_api_format, "typesafe:systemone");
                assert_eq!(plan.method, "POST");
                assert_eq!(plan.url, "https://api.typesafe.ai/v1/systemone");
                assert_eq!(
                    plan.headers.get("authorization").map(String::as_str),
                    Some("Bearer sk-upstream-systemone")
                );
                assert!(!plan.headers.contains_key("x-api-key"));
                let mut expected = systemone_request();
                expected["model"] = json!("jev-preview");
                assert_eq!(plan.body.json_body.as_ref(), Some(&expected));
                Json(ExecutionResult {
                    request_id: plan.request_id,
                    candidate_id: plan.candidate_id,
                    status_code,
                    headers: BTreeMap::from([
                        ("content-type".into(), "application/json".into()),
                        ("retry-after".into(), "2".into()),
                    ]),
                    response_observation: None,
                    body: Some(ResponseBody {
                        json_body: Some(response_body),
                        body_bytes_b64: None,
                    }),
                    telemetry: None,
                    error: None,
                })
            }
        }),
    )
}

#[test]
fn systemone_proxies_all_question_types_and_preserves_native_answers() {
    run_systemone_test("systemone_native_proxy", || async {
        let (runtime_url, runtime_handle) =
            start_server(systemone_runtime(200, systemone_response())).await;
        let (gateway_url, gateway_handle) = start_server(build_router_with_state(
            systemone_success_state(runtime_url),
        ))
        .await;
        for path in ["/v1/systemone", "/jev/v1/systemone"] {
            let response = reqwest::Client::new()
                .post(format!("{gateway_url}{path}"))
                .bearer_auth("sk-systemone-success")
                .json(&systemone_request())
                .send()
                .await
                .unwrap();
            let status = response.status();
            let body = response.text().await.unwrap();
            assert_eq!(status, http::StatusCode::OK, "{body}");
            assert_eq!(
                serde_json::from_str::<Value>(&body).unwrap(),
                systemone_response()
            );
        }
        gateway_handle.abort();
        runtime_handle.abort();
    });
}

#[test]
fn systemone_rejects_malformed_requests_and_requires_authentication() {
    run_systemone_test("systemone_validation", || async {
        let (runtime_url, runtime_handle) =
            start_server(systemone_runtime(200, systemone_response())).await;
        let (gateway_url, gateway_handle) = start_server(build_router_with_state(
            systemone_success_state(runtime_url),
        ))
        .await;
        for request in [
            json!({"model":"jev-latest","messages":[]}),
            {
                let mut request = systemone_request();
                request["stream"] = json!(true);
                request
            },
            {
                let mut request = systemone_request();
                request["questions"]["priority"]["criteria"] = json!(["one"]);
                request
            },
        ] {
            let response = reqwest::Client::new()
                .post(format!("{gateway_url}/v1/systemone"))
                .bearer_auth("sk-systemone-success")
                .json(&request)
                .send()
                .await
                .unwrap();
            let status = response.status();
            let body = response.json::<Value>().await.unwrap();
            assert_eq!(status, http::StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            assert!(body.get("detail").is_some(), "{body}");
        }
        let response = reqwest::Client::new()
            .post(format!("{gateway_url}/v1/systemone"))
            .json(&systemone_request())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
        assert!(response.json::<Value>().await.unwrap()["detail"].is_string());
        for (key, model, expected_status) in [
            (
                "sk-systemone-invalid",
                "jev-latest",
                http::StatusCode::UNAUTHORIZED,
            ),
            (
                "sk-systemone-success",
                "jev-private",
                http::StatusCode::FORBIDDEN,
            ),
        ] {
            let mut request = systemone_request();
            request["model"] = json!(model);
            let response = reqwest::Client::new()
                .post(format!("{gateway_url}/jev/v1/systemone"))
                .bearer_auth(key)
                .json(&request)
                .send()
                .await
                .unwrap();
            let status = response.status();
            let body = response.json::<Value>().await.unwrap();
            assert_eq!(status, expected_status, "{body}");
            assert!(body["detail"].is_string(), "{body}");
            assert!(body.get("error").is_none(), "{body}");
        }
        gateway_handle.abort();
        runtime_handle.abort();
    });
}

#[test]
fn systemone_lists_only_authorized_models_in_native_catalog_shape() {
    run_systemone_test("systemone_models", || async {
        let (runtime_url, runtime_handle) =
            start_server(systemone_runtime(200, systemone_response())).await;
        let state = systemone_success_state(runtime_url);
        state.runtime_state().kv_set("upstream_models:provider-systemone:key-upstream-systemone",json!([
            {"id":"jev-preview","description":"Structured decisions","release_date":"2026-09-01"},
            {"id":"secret-model","description":"Private","release_date":"2026-09-02"}
        ]).to_string(),None).await.unwrap();
        let (gateway_url, gateway_handle) = start_server(build_router_with_state(state)).await;
        let response = reqwest::Client::new()
            .get(format!("{gateway_url}/jev/v1/models"))
            .bearer_auth("sk-systemone-success")
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = response.json::<Value>().await.unwrap();
        assert_eq!(status, http::StatusCode::OK, "{body}");
        assert_eq!(
            body,
            json!({"models":[{"name":"jev-latest","description":"Structured decisions","release_date":"2026-09-01"}]})
        );
        let response = reqwest::Client::new()
            .get(format!("{gateway_url}/jev/v1/models"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
        gateway_handle.abort();
        runtime_handle.abort();
    });
}

#[test]
fn systemone_preserves_upstream_validation_and_retryable_errors() {
    run_systemone_test("systemone_errors", || async {
        for status in [422, 429, 529] {
            let expected = if status == 422 {
                json!({"detail":[{"type":"value_error","loc":["body","questions"],"msg":"Invalid question","input":{}}]})
            } else {
                json!({"detail":"Native upstream error"})
            };
            let (runtime_url, runtime_handle) =
                start_server(systemone_runtime(status, expected.clone())).await;
            let (gateway_url, gateway_handle) = start_server(build_router_with_state(
                systemone_success_state(runtime_url),
            ))
            .await;
            let response = reqwest::Client::new()
                .post(format!("{gateway_url}/v1/systemone"))
                .bearer_auth("sk-systemone-success")
                .json(&systemone_request())
                .send()
                .await
                .unwrap();
            let actual_status = response.status();
            let headers = response.headers().clone();
            let body = response.json::<Value>().await.unwrap();
            assert_eq!(actual_status.as_u16(), status, "{body}");
            assert_eq!(body, expected);
            if status != 422 {
                assert_eq!(headers.get("retry-after").unwrap(), "2");
            }
            gateway_handle.abort();
            runtime_handle.abort();
        }
    });
}
