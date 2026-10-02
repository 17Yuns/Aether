use super::AppState;
use crate::GatewayError;
use aether_billing::groups::{PricingGroupsConfig, PRICING_GROUPS_CONFIG_KEY};

impl AppState {
    /// Resolve exact provider/model pairs once for the catalog, intersecting
    /// group membership with the caller's provider policy and live mappings.
    pub(crate) async fn pricing_group_available_model_ids(
        &self,
        config: &PricingGroupsConfig,
        allowed_providers: Option<&[String]>,
    ) -> Result<std::collections::BTreeMap<String, std::collections::BTreeSet<String>>, GatewayError>
    {
        use std::collections::{BTreeMap, BTreeSet};
        let mut available = BTreeMap::new();
        if !config.enabled
            || !config
                .groups
                .iter()
                .any(|group| group.model_access.is_some())
        {
            return Ok(available);
        }
        let providers = self.list_provider_catalog_providers(true).await?;
        let allowed_ids = providers
            .iter()
            .filter(|provider| {
                allowed_providers.is_none_or(|allowed| {
                    allowed.iter().any(|value| {
                        aether_scheduler_core::provider_matches_allowed_value(
                            value,
                            &provider.id,
                            &provider.name,
                            &provider.provider_type,
                        )
                    })
                })
            })
            .map(|provider| provider.id.clone())
            .collect::<BTreeSet<_>>();
        let provider_ids = config
            .groups
            .iter()
            .flat_map(|group| group.model_access.iter().flatten())
            .flat_map(|model| model.provider_ids.iter())
            .filter(|id| allowed_ids.contains(*id))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let refs = if provider_ids.is_empty() {
            Vec::new()
        } else {
            self.list_active_global_model_ids_by_provider_ids(&provider_ids)
                .await?
        };
        for group in &config.groups {
            if group.model_access.is_some() {
                available.insert(
                    group.id.clone(),
                    refs.iter()
                        .filter(|model| group.allows(&model.global_model_id, &model.provider_id))
                        .map(|model| model.global_model_id.clone())
                        .collect(),
                );
            }
        }
        Ok(available)
    }

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
