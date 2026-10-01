use super::AppState;
use crate::GatewayError;
use aether_billing::groups::{PricingGroupsConfig, PRICING_GROUPS_CONFIG_KEY};

impl AppState {
    pub(crate) async fn read_pricing_groups_config(
        &self,
    ) -> Result<PricingGroupsConfig, GatewayError> {
        PricingGroupsConfig::from_value(
            self.read_system_config_json_value_strong(PRICING_GROUPS_CONFIG_KEY)
                .await?,
        )
        .map_err(GatewayError::Internal)
    }
}
