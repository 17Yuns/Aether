use super::shared::{
    build_provider_quota_execution_plan, build_quota_snapshot_payload,
    default_provider_quota_execution_timeouts, execute_provider_quota_plan,
    persist_credential_fenced_provider_quota_refresh_state, ProviderQuotaExecutionOutcome,
};
use crate::handlers::admin::request::{AdminAppState, AdminGatewayProviderTransportSnapshot};
use crate::GatewayError;
use aether_contracts::ProxySnapshot;
use aether_data_contracts::repository::provider_catalog::{
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
) -> Result<Option<Value>, GatewayError> {
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
    match execute_provider_quota_plan(state, transport, plan, "clinepass").await? {
        ProviderQuotaExecutionOutcome::Response(response) if response.status_code == 200 => {
            Ok(response.body.and_then(|body| body.json_body))
        }
        _ => Ok(None),
    }
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
        if let Some(transport) = state
            .read_provider_transport_snapshot(&provider.id, &endpoint.id, &key.id)
            .await?
        {
            let Some(fence) = state
                .app()
                .capture_provider_transport_credential_fence(&transport)
                .await?
            else {
                results.push(result);
                continue;
            };
            let secret = transport.key.decrypted_api_key.trim();
            if !secret.is_empty() {
                let authorization = ("authorization".to_string(), format!("Bearer {secret}"));
                let (plan, limits) = tokio::try_join!(
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
                )?;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_secs())
                    .unwrap_or(0);
                if let Some(metadata) = plan
                    .as_ref()
                    .and_then(|plan| parse_clinepass_quota(plan, limits.as_ref(), now))
                {
                    let update = json!({"clinepass":metadata});
                    if persist_credential_fenced_provider_quota_refresh_state(
                        state,
                        &key.id,
                        Some(&update),
                        None,
                        None,
                        None,
                        &fence,
                    )
                    .await?
                    {
                        success += 1;
                        result = json!({"key_id":key.id,"key_name":key.name,"status":"success","metadata":metadata,
                            "quota_snapshot":build_quota_snapshot_payload("clinepass", key.status_snapshot.as_ref(), Some(&update))});
                    }
                }
            }
        }
        results.push(result);
    }
    Ok(Some(
        json!({"success":success,"failed":results.len()-success,"total":results.len(),"results":results,"auto_removed":0,"message":"ClinePass 套餐额度已更新"}),
    ))
}
