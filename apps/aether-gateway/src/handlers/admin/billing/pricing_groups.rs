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
        let value = serde_json::to_value(&config)
            .map_err(|error| GatewayError::Internal(error.to_string()))?;
        state
            .upsert_system_config_json_value(
                PRICING_GROUPS_CONFIG_KEY,
                &value,
                Some("模型基础售价与 API Key 定价分组倍率"),
            )
            .await?;
        return Ok(Some(Json(config).into_response()));
    }
    Ok(None)
}
