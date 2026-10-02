use aether_data_contracts::repository::candidate_selection::StoredMinimalCandidateSelectionRow;
use axum::{
    body::Body,
    http,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

const PUBLIC_MODELS_OWNER: &str = "aether";

pub(super) async fn build_typesafe_models_list_response(
    state: &crate::AppState,
    rows: &[StoredMinimalCandidateSelectionRow],
) -> Response<Body> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut catalogs = std::collections::BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut models = Vec::new();
    for row in rows {
        if !seen.insert(row.global_model_name.clone()) {
            continue;
        }
        let target = (row.provider_id.clone(), row.key_id.clone());
        if !catalogs.contains_key(&target) {
            let cache_key = format!("upstream_models:{}:{}", row.provider_id, row.key_id);
            let mut raw = state
                .runtime_state()
                .kv_get(&cache_key)
                .await
                .ok()
                .flatten();
            if raw.is_none() && tokio::time::Instant::now() < deadline {
                let _ = tokio::time::timeout_at(
                    deadline,
                    crate::model_fetch::perform_model_fetch_for_key(
                        state,
                        &row.provider_id,
                        &row.key_id,
                    ),
                )
                .await;
                raw = state
                    .runtime_state()
                    .kv_get(&cache_key)
                    .await
                    .ok()
                    .flatten();
            }
            let cards = raw
                .and_then(|raw| serde_json::from_str::<Vec<serde_json::Value>>(&raw).ok())
                .unwrap_or_default();
            catalogs.insert(target.clone(), cards);
        }
        let upstream_model =
            aether_scheduler_core::select_provider_model_name(row, "typesafe:systemone");
        let card = catalogs.get(&target).and_then(|cards| {
            cards.iter().find(|card| {
                card.get("id").and_then(serde_json::Value::as_str) == Some(&upstream_model)
            })
        });
        models.push(json!({
            "name":row.global_model_name,
            "description":card.and_then(|card| card.get("description")).and_then(serde_json::Value::as_str).unwrap_or(&upstream_model),
            "release_date":card.and_then(|card| card.get("release_date")).and_then(serde_json::Value::as_str).unwrap_or(""),
        }));
    }
    Json(json!({"models":models})).into_response()
}

pub(crate) fn build_models_auth_error_response(api_format: &str) -> Response<Body> {
    match api_format {
        "typesafe:systemone" => (http::StatusCode::UNAUTHORIZED, Json(json!({"detail":"Invalid API key provided"}))).into_response(),
        "claude:messages" => (
            http::StatusCode::UNAUTHORIZED,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "authentication_error",
                    "message": "Invalid API key provided",
                },
            })),
        )
            .into_response(),
        "gemini:generate_content" => (
            http::StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": {
                    "code": 401,
                    "message": "API key not valid. Please pass a valid API key.",
                    "status": "UNAUTHENTICATED",
                }
            })),
        )
            .into_response(),
        _ => (
            http::StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": {
                    "message": "Incorrect API key provided. You can find your API key at https://platform.openai.com/account/api-keys.",
                    "type": "invalid_request_error",
                    "param": null,
                    "code": "invalid_api_key",
                }
            })),
        )
            .into_response(),
    }
}

pub(super) fn build_models_not_found_response(model_id: &str, api_format: &str) -> Response<Body> {
    match api_format {
        "claude:messages" => (
            http::StatusCode::NOT_FOUND,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "not_found_error",
                    "message": format!("Model '{model_id}' not found"),
                },
            })),
        )
            .into_response(),
        "gemini:generate_content" => (
            http::StatusCode::NOT_FOUND,
            Json(json!({
                "error": {
                    "code": 404,
                    "message": format!("models/{model_id} is not found"),
                    "status": "NOT_FOUND",
                }
            })),
        )
            .into_response(),
        _ => (
            http::StatusCode::NOT_FOUND,
            Json(json!({
                "error": {
                    "message": format!("The model '{model_id}' does not exist"),
                    "type": "invalid_request_error",
                    "param": "model",
                    "code": "model_not_found",
                }
            })),
        )
            .into_response(),
    }
}

pub(super) fn build_empty_models_list_response(api_format: &str) -> Response<Body> {
    match api_format {
        "typesafe:systemone" => Json(json!({"models":[]})).into_response(),
        "openai:responses" => Json(json!({ "models": [] })).into_response(),
        "claude:messages" => Json(json!({
            "data": [],
            "has_more": false,
            "first_id": serde_json::Value::Null,
            "last_id": serde_json::Value::Null,
        }))
        .into_response(),
        "gemini:generate_content" => Json(json!({ "models": [] })).into_response(),
        _ => Json(json!({ "object": "list", "data": [] })).into_response(),
    }
}

pub(super) fn build_codex_models_list_response(
    models: Vec<serde_json::Value>,
    etag: Option<&str>,
) -> Response<Body> {
    let mut response = Json(json!({ "models": models })).into_response();
    if let Some(etag) = etag.and_then(|value| http::HeaderValue::from_str(value).ok()) {
        response.headers_mut().insert(http::header::ETAG, etag);
    }
    response
}

pub(super) fn build_openai_models_list_response(
    rows: &[StoredMinimalCandidateSelectionRow],
) -> Response<Body> {
    Json(json!({
        "object": "list",
        "data": rows.iter().map(|row| {
            json!({
                "id": row.global_model_name,
                "object": "model",
                "created": 0,
                "owned_by": PUBLIC_MODELS_OWNER,
            })
        }).collect::<Vec<_>>(),
    }))
    .into_response()
}

pub(super) fn build_openai_model_detail_response(
    row: &StoredMinimalCandidateSelectionRow,
) -> Response<Body> {
    Json(json!({
        "id": row.global_model_name,
        "object": "model",
        "created": 0,
        "owned_by": PUBLIC_MODELS_OWNER,
    }))
    .into_response()
}

pub(super) fn build_claude_models_list_response(
    rows: &[StoredMinimalCandidateSelectionRow],
    before_id: Option<&str>,
    after_id: Option<&str>,
    limit: usize,
) -> Response<Body> {
    let model_data = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.global_model_name,
                "type": "model",
                "display_name": row.global_model_name,
                "created_at": serde_json::Value::Null,
            })
        })
        .collect::<Vec<_>>();

    let mut start_idx = 0usize;
    if let Some(after_id) = after_id {
        if let Some(index) = model_data.iter().position(|item| item["id"] == after_id) {
            start_idx = index.saturating_add(1);
        }
    }
    let mut end_idx = model_data.len();
    if let Some(before_id) = before_id {
        if let Some(index) = model_data.iter().position(|item| item["id"] == before_id) {
            end_idx = index;
        }
    }
    let window = &model_data[start_idx.min(end_idx)..end_idx];
    let paginated = window.iter().take(limit).cloned().collect::<Vec<_>>();
    let first_id = paginated
        .first()
        .and_then(|item| item.get("id"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let last_id = paginated
        .last()
        .and_then(|item| item.get("id"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    Json(json!({
        "data": paginated,
        "has_more": window.len() > limit,
        "first_id": first_id,
        "last_id": last_id,
    }))
    .into_response()
}

pub(super) fn build_claude_model_detail_response(
    row: &StoredMinimalCandidateSelectionRow,
) -> Response<Body> {
    Json(json!({
        "id": row.global_model_name,
        "type": "model",
        "display_name": row.global_model_name,
        "created_at": serde_json::Value::Null,
    }))
    .into_response()
}

fn build_gemini_model_value(row: &StoredMinimalCandidateSelectionRow) -> serde_json::Value {
    json!({
        "name": format!("models/{}", row.global_model_name),
        "baseModelId": row.global_model_name,
        "version": "001",
        "displayName": row.global_model_name,
        "description": format!("Model {}", row.global_model_name),
        "inputTokenLimit": 128000,
        "outputTokenLimit": 8192,
        "supportedGenerationMethods": ["generateContent", "countTokens"],
        "temperature": 1.0,
        "maxTemperature": 2.0,
        "topP": 0.95,
        "topK": 64,
    })
}

pub(super) fn build_gemini_models_list_response(
    rows: &[StoredMinimalCandidateSelectionRow],
    page_size: usize,
    page_token: Option<&str>,
) -> Response<Body> {
    let start_idx = page_token
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let end_idx = start_idx.saturating_add(page_size);
    let window = rows
        .iter()
        .skip(start_idx)
        .take(page_size)
        .map(build_gemini_model_value)
        .collect::<Vec<_>>();
    let mut payload = json!({ "models": window });
    if end_idx < rows.len() {
        payload["nextPageToken"] = serde_json::Value::String(end_idx.to_string());
    }
    Json(payload).into_response()
}

pub(super) fn build_gemini_model_detail_response(
    row: &StoredMinimalCandidateSelectionRow,
) -> Response<Body> {
    Json(build_gemini_model_value(row)).into_response()
}
