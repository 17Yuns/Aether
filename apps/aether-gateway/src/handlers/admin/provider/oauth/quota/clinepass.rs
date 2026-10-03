use super::shared::{
    build_provider_quota_execution_plan, build_quota_snapshot_payload,
    default_provider_quota_execution_timeouts, execute_provider_quota_plan,
    ProviderQuotaExecutionOutcome,
};
use crate::handlers::admin::request::{AdminAppState, AdminGatewayProviderTransportSnapshot};
use crate::handlers::shared::sync_provider_key_quota_status_snapshot;
use crate::GatewayError;
use aether_contracts::ProxySnapshot;
use aether_data_contracts::repository::provider_catalog::{
    ProviderCatalogKeyOAuthCredentialFence, ProviderCatalogKeyRuntimeMetadataUpdate,
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use aether_provider_pool::{build_clinepass_pool_quota_request, parse_clinepass_quota};
use serde_json::{json, Value};

async fn fetch_quota(
    state: &AdminAppState<'_>,
    transport: &AdminGatewayProviderTransportSnapshot,
    authorization: &(String, String),
    limits: bool,
    proxy_override: Option<&ProxySnapshot>,
) -> Result<Value, String> {
    let proxy = match proxy_override {
        Some(proxy) => Some(proxy.clone()),
        None => {
            state
                .resolve_transport_proxy_snapshot_with_tunnel_affinity(transport)
                .await
        }
    };
    let timeouts = Some(default_provider_quota_execution_timeouts(proxy.as_ref()));
    let spec = build_clinepass_pool_quota_request(
        &transport.key.id,
        &transport.endpoint.base_url,
        authorization.clone(),
        limits,
    );
    let plan = build_provider_quota_execution_plan(
        transport,
        spec,
        proxy,
        state.resolve_transport_profile(transport),
        timeouts,
    );
    match execute_provider_quota_plan(state, transport, plan, "clinepass").await {
        Ok(ProviderQuotaExecutionOutcome::Response(response)) if response.status_code == 200 => {
            response
                .body
                .and_then(|body| body.json_body)
                .filter(|body| body.get("success").and_then(Value::as_bool) != Some(false))
                .ok_or_else(|| "ClinePass 未返回有效的额度数据".to_string())
        }
        Ok(ProviderQuotaExecutionOutcome::Response(response)) => Err(format!(
            "ClinePass 额度接口返回 HTTP {}",
            response.status_code
        )),
        Ok(ProviderQuotaExecutionOutcome::Failure(_)) | Err(_) => {
            Err("ClinePass 额度请求失败，请检查密钥、上游连接及代理".to_string())
        }
    }
}

async fn persist_api_key_quota(
    state: &AdminAppState<'_>,
    key_id: &str,
    metadata: &Value,
    credential: &ProviderCatalogKeyOAuthCredentialFence,
    now: u64,
) -> Result<bool, GatewayError> {
    let Some(latest) = state
        .app()
        .list_provider_catalog_keys_by_ids_strong(&[key_id.to_string()])
        .await?
        .into_iter()
        .next()
    else {
        return Ok(false);
    };
    let update = json!({"clinepass": metadata});
    let snapshot = sync_provider_key_quota_status_snapshot(
        latest.status_snapshot.as_ref(),
        "clinepass",
        Some(&update),
        "refresh_api",
    );
    state.app().update_provider_catalog_key_runtime_metadata(&ProviderCatalogKeyRuntimeMetadataUpdate {
        key_id: key_id.to_string(),
        namespace: "clinepass".to_string(),
        expected_credential: Some(credential.clone()),
        expected_upstream_metadata_value: latest.upstream_metadata.as_ref()
            .and_then(|metadata| metadata.get("clinepass")).cloned(),
        upstream_metadata_value: metadata.clone(),
        status_snapshot_patch: json!({"quota": snapshot.as_ref().and_then(|snapshot| snapshot.get("quota"))}),
        updated_at_unix_secs: Some(now),
    }).await
}

pub(crate) async fn refresh_clinepass_provider_quota_locally(
    state: &AdminAppState<'_>,
    provider: &StoredProviderCatalogProvider,
    endpoint: &StoredProviderCatalogEndpoint,
    keys: Vec<StoredProviderCatalogKey>,
    proxy_override: Option<ProxySnapshot>,
) -> Result<Option<Value>, GatewayError> {
    let mut results = Vec::new();
    let mut success = 0;
    for key in keys {
        let mut result = json!({"key_id":key.id,"key_name":key.name,"status":"error","message":"ClinePass 套餐读取失败"});
        let Some(transport) = state
            .read_provider_transport_snapshot(&provider.id, &endpoint.id, &key.id)
            .await?
        else {
            result["message"] = json!("ClinePass 密钥或端点不可用");
            results.push(result);
            continue;
        };
        let Some(stored) = state
            .app()
            .list_provider_catalog_keys_by_ids_strong(&[key.id.clone()])
            .await?
            .into_iter()
            .next()
        else {
            results.push(result);
            continue;
        };
        // API-key accounts have no OAuth auth_config. Fence the actual key ciphertext instead.
        let secret = transport.key.decrypted_api_key.trim();
        let current_secret = state.app().decrypt_provider_catalog_key_api_key(&stored)?;
        if secret.is_empty()
            || current_secret.as_deref().map(str::trim) != Some(secret)
            || stored.provider_id != provider.id
            || stored.auth_type != transport.key.auth_type
        {
            result["message"] = json!("密钥已变更，请重新刷新额度");
            results.push(result);
            continue;
        }
        let credential = ProviderCatalogKeyOAuthCredentialFence {
            encrypted_api_key: stored.encrypted_api_key.clone(),
            auth_type: stored.auth_type.clone(),
            provider_id: provider.id.clone(),
            provider_type: provider.provider_type.clone(),
        };
        let authorization = ("authorization".to_string(), format!("Bearer {secret}"));
        let (plan, limits) = tokio::join!(
            fetch_quota(
                state,
                &transport,
                &authorization,
                false,
                proxy_override.as_ref()
            ),
            fetch_quota(
                state,
                &transport,
                &authorization,
                true,
                proxy_override.as_ref()
            ),
        );
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        match plan {
            Ok(plan) => {
                if let Some(mut metadata) = parse_clinepass_quota(&plan, limits.as_ref().ok(), now)
                {
                    if let Err(error) = limits.as_ref() {
                        metadata["usage_read_error"] = json!(error);
                    }
                    if persist_api_key_quota(state, &key.id, &metadata, &credential, now).await? {
                        success += 1;
                        let update = json!({"clinepass": metadata});
                        result = json!({"key_id":key.id,"key_name":key.name,"status":"success","metadata":metadata,
                            "quota_snapshot":build_quota_snapshot_payload("clinepass", key.status_snapshot.as_ref(), Some(&update)),
                            "message": limits.as_ref().err().map(|error| format!("套餐已读取，使用量读取失败：{error}"))});
                    } else {
                        result["message"] = json!("密钥或额度数据已变更，请重新刷新");
                    }
                } else {
                    result["message"] =
                        json!("ClinePass 未返回套餐信息，请确认此 Key 所属账号已开通套餐");
                }
            }
            Err(error) => result["message"] = json!(error),
        }
        results.push(result);
    }
    let failed = results.len() - success;
    Ok(Some(
        json!({"success":success,"failed":failed,"total":results.len(),"results":results,"auto_removed":0,
        "message": if failed == 0 { "ClinePass 套餐额度已更新" } else { "部分 ClinePass 套餐读取失败，请查看各密钥结果" }}),
    ))
}
