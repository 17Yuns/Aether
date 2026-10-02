use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{quantize_cost, BillingComputation, BillingModelPricingSnapshot};

pub const PRICING_GROUPS_CONFIG_KEY: &str = "pricing_groups";
pub const PRICING_GROUP_ID_SETTING: &str = "pricing_group_id";
pub const PRICING_GROUP_SNAPSHOT_KEY: &str = "pricing_group";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PricingGroupModelAccess {
    pub global_model_id: String,
    pub provider_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PricingGroup {
    pub id: String,
    pub name: String,
    pub multiplier: f64,
    #[serde(default = "default_visible")]
    pub is_visible: bool,
    /// Missing/null preserves existing groups. An empty list permits no models.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_access: Option<Vec<PricingGroupModelAccess>>,
}

impl PricingGroup {
    pub fn allows_model(&self, global_model_id: &str) -> bool {
        self.model_access.as_ref().is_none_or(|models| {
            models
                .iter()
                .any(|model| model.global_model_id == global_model_id)
        })
    }

    pub fn allows(&self, global_model_id: &str, provider_id: &str) -> bool {
        self.model_access.as_ref().is_none_or(|models| {
            models.iter().any(|model| {
                model.global_model_id == global_model_id
                    && model.provider_ids.iter().any(|id| id == provider_id)
            })
        })
    }

    /// Pricing snapshots need the rate, not the complete routing allowlist.
    pub fn billing_snapshot(&self) -> Self {
        Self {
            model_access: None,
            ..self.clone()
        }
    }
}

fn default_visible() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PricingGroupsConfig {
    #[serde(default)]
    pub enabled: bool,
    pub default_group_id: String,
    pub groups: Vec<PricingGroup>,
}

impl Default for PricingGroupsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_group_id: "default".to_string(),
            groups: vec![PricingGroup {
                id: "default".to_string(),
                name: "默认分组".to_string(),
                multiplier: 1.0,
                is_visible: true,
                model_access: None,
            }],
        }
    }
}

impl PricingGroupsConfig {
    pub fn from_value(value: Option<Value>) -> Result<Self, String> {
        let config = match value {
            Some(value) => {
                serde_json::from_value(value).map_err(|_| "分组定价配置格式无效".to_string())?
            }
            None => Self::default(),
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.groups.is_empty() || self.groups.len() > 100 {
            return Err("定价分组数量必须在 1 到 100 之间".to_string());
        }
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for group in &self.groups {
            if group.id.trim().is_empty() || group.id != group.id.trim() || group.id.len() > 100 {
                return Err("分组 ID 必须为不超过 100 字节的非空字符串".to_string());
            }
            if group.name.trim().is_empty()
                || group.name != group.name.trim()
                || group.name.len() > 200
            {
                return Err("分组名称必须为不超过 200 字节的非空字符串".to_string());
            }
            if !ids.insert(&group.id) || !names.insert(group.name.to_lowercase()) {
                return Err("分组 ID 和名称不能重复".to_string());
            }
            if !group.multiplier.is_finite() || group.multiplier < 0.0 {
                return Err("分组倍率必须是大于等于 0 的有限数值".to_string());
            }
            if let Some(models) = &group.model_access {
                if models.len() > 10000 {
                    return Err("每个分组最多配置 10000 个模型".to_string());
                }
                let mut model_ids = BTreeSet::new();
                for model in models {
                    if model.global_model_id.trim().is_empty()
                        || model.global_model_id != model.global_model_id.trim()
                        || model.global_model_id.len() > 200
                        || !model_ids.insert(&model.global_model_id)
                    {
                        return Err("分组模型 ID 无效或重复".to_string());
                    }
                    let mut provider_ids = BTreeSet::new();
                    if model.provider_ids.is_empty() || model.provider_ids.len() > 10000 {
                        return Err("每个分组模型必须至少选择一个供应商".to_string());
                    }
                    for id in &model.provider_ids {
                        if id.trim().is_empty()
                            || id != id.trim()
                            || id.len() > 200
                            || !provider_ids.insert(id)
                        {
                            return Err("分组供应商 ID 无效或重复".to_string());
                        }
                    }
                }
            }
        }
        let default = self
            .groups
            .iter()
            .find(|group| group.id == self.default_group_id)
            .ok_or_else(|| "默认定价分组不存在".to_string())?;
        if !default.is_visible {
            return Err("默认定价分组必须对用户可见".to_string());
        }
        Ok(())
    }

    pub fn resolve(&self, id: Option<&str>) -> Result<Option<PricingGroup>, String> {
        if !self.enabled {
            return Ok(None);
        }
        let id = id.unwrap_or(&self.default_group_id);
        self.groups
            .iter()
            .find(|group| group.id == id)
            .cloned()
            .map(Some)
            .ok_or_else(|| "定价分组不存在，请重新选择 API Key 的定价分组".to_string())
    }

    pub fn visible_groups(&self) -> Vec<PricingGroup> {
        if !self.enabled {
            return Vec::new();
        }
        self.groups
            .iter()
            .filter(|group| group.is_visible)
            .cloned()
            .collect()
    }
}

pub fn pricing_group_id(settings: Option<&Value>) -> Result<Option<&str>, String> {
    match settings.and_then(|settings| settings.get(PRICING_GROUP_ID_SETTING)) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(id)) if !id.trim().is_empty() => Ok(Some(id)),
        Some(_) => Err("pricing_group_id 必须为非空字符串".to_string()),
    }
}

/// Only the validated top-level selection can change a user's pricing group.
/// Generic feature updates preserve an administrator-assigned hidden group.
pub fn user_key_group_settings(
    config: &PricingGroupsConfig,
    mut settings: Option<Value>,
    selection: Option<Option<&str>>,
    existing: Option<&Value>,
    creating: bool,
) -> Result<Option<Value>, String> {
    let current = pricing_group_id(existing)?;
    let selected = match selection {
        Some(id) => {
            if !config.enabled && id.is_some() {
                return Err("分组定价尚未启用".to_string());
            }
            let group = config.resolve(id)?;
            if let Some(group) = &group {
                if !group.is_visible && current != Some(group.id.as_str()) {
                    return Err("该定价分组不对用户开放".to_string());
                }
            }
            group.map(|group| group.id)
        }
        None if creating => config.resolve(None)?.map(|group| group.id),
        None => current.map(ToOwned::to_owned),
    };
    if let Some(object) = settings.as_mut().and_then(Value::as_object_mut) {
        object.remove(PRICING_GROUP_ID_SETTING);
    }
    if let Some(id) = selected {
        let object = settings
            .get_or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| "feature_settings 必须为对象".to_string())?;
        object.insert(PRICING_GROUP_ID_SETTING.to_string(), Value::String(id));
    }
    Ok(settings)
}

/// Administrators may assign hidden groups to both user and standalone keys.
pub fn admin_key_group_settings(
    config: &PricingGroupsConfig,
    mut settings: Option<Value>,
    selection: Option<Option<&str>>,
    existing: Option<&Value>,
    creating: bool,
) -> Result<Option<Value>, String> {
    let setting_id = pricing_group_id(settings.as_ref())?;
    let current = pricing_group_id(existing)?;
    let chosen = selection.or_else(|| setting_id.map(Some));
    let selected = match chosen {
        Some(id) => {
            if !config.enabled {
                if id.is_some() && (creating || id != current) {
                    return Err("分组定价尚未启用".to_string());
                }
                id.map(ToOwned::to_owned)
            } else {
                config.resolve(id)?.map(|group| group.id)
            }
        }
        None if creating => config.resolve(None)?.map(|group| group.id),
        None => pricing_group_id(existing)?.map(ToOwned::to_owned),
    };
    if let Some(object) = settings.as_mut().and_then(Value::as_object_mut) {
        object.remove(PRICING_GROUP_ID_SETTING);
    }
    if let Some(id) = selected {
        settings
            .get_or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| "feature_settings 必须为对象".to_string())?
            .insert(PRICING_GROUP_ID_SETTING.to_string(), Value::String(id));
    }
    Ok(settings)
}

/// Materialize display prices without changing context bands, TTLs, or ratios.
pub fn scale_group_pricing(pricing: &Value, multiplier: f64) -> Option<Value> {
    let mut result = crate::pricing::multiply_pricing_catalog(pricing, multiplier, "group")?;
    if let Some(overlays) = pricing.get("processing_tiers").and_then(Value::as_object) {
        let mut scaled = serde_json::Map::new();
        for (name, overlay) in overlays {
            scaled.insert(
                name.clone(),
                if overlay.get("price_multiplier").is_some()
                    && ![
                        "tiers",
                        "image_output_prices",
                        "image_output_price_default",
                        "image_output_price_ranges",
                    ]
                    .iter()
                    .any(|key| overlay.get(*key).is_some())
                {
                    overlay.clone()
                } else {
                    scale_group_pricing(overlay, multiplier)?
                },
            );
        }
        result
            .as_object_mut()?
            .insert("processing_tiers".to_string(), Value::Object(scaled));
    }
    Some(result)
}

pub fn scale_model_sale_prices(pricing: &Value, multiplier: f64) -> Option<Value> {
    if !multiplier.is_finite() || multiplier < 0.0 || !pricing.is_object() {
        return None;
    }
    let mut model = pricing.clone();
    if let Some(video) = model
        .get_mut("config")
        .and_then(|config| config.get_mut("billing"))
        .and_then(|billing| billing.get_mut("video"))
    {
        if let Some(prices) = video
            .get_mut("price_per_second_by_resolution")
            .and_then(Value::as_object_mut)
        {
            for price in prices.values_mut() {
                if let Some(value) = price.as_f64() {
                    let value = value * multiplier;
                    if !value.is_finite() || value < 0.0 {
                        return None;
                    }
                    *price = serde_json::json!(value);
                }
            }
        }
        if let Some(price) = video.get_mut("price_per_second") {
            if let Some(value) = price.as_f64() {
                let value = value * multiplier;
                if !value.is_finite() || value < 0.0 {
                    return None;
                }
                *price = serde_json::json!(value);
            }
        }
    }
    if let Some(price) = model["default_price_per_request"].as_f64() {
        let price = price * multiplier;
        if !price.is_finite() {
            return None;
        }
        model["default_price_per_request"] = serde_json::json!(price);
    }
    if !model["default_tiered_pricing"].is_null() {
        let pricing = scale_group_pricing(&model["default_tiered_pricing"], multiplier)?;
        model["default_tiered_pricing"] = pricing;
    }
    Some(model)
}

/// Group billing uses the model's base sale catalog. Provider overrides and
/// upstream discounts remain provider configuration, not customer sale prices.
pub fn customer_pricing_snapshot(
    mut pricing: BillingModelPricingSnapshot,
    group: Option<&PricingGroup>,
) -> BillingModelPricingSnapshot {
    if group.is_some() {
        pricing.provider_billing_type = None;
        pricing.model_config = None;
        pricing.model_price_per_request = None;
        pricing.model_tiered_pricing = None;
        pricing.provider_api_key_rate_multipliers = None;
    }
    pricing
}

pub fn apply_group_multiplier(
    computation: &mut BillingComputation,
    group: Option<&PricingGroup>,
) -> Result<(), String> {
    let Some(group) = group else {
        return Ok(());
    };
    let cost = quantize_cost(computation.cost_result.cost * group.multiplier);
    if !group.multiplier.is_finite() || group.multiplier < 0.0 || !cost.is_finite() || cost < 0.0 {
        return Err("分组计费金额无效".to_string());
    }
    computation.actual_total_cost = cost;
    computation.rate_multiplier = group.multiplier;
    computation.is_free_tier = false;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn group_membership_is_an_allowlist_of_exact_provider_model_pairs() {
        let config = PricingGroupsConfig::from_value(Some(json!({
            "enabled": true, "default_group_id": "deepseek", "groups": [
                {"id":"deepseek", "name":"DeepSeek", "multiplier":0.1, "is_visible":true,
                 "model_access":[{"global_model_id":"deepseek", "provider_ids":["a","b"]}]},
                {"id":"claude", "name":"Claude", "multiplier":1.1, "is_visible":true,
                 "model_access":[{"global_model_id":"claude", "provider_ids":["a"]}]},
                {"id":"shared", "name":"Shared", "multiplier":0.5, "is_visible":true,
                 "model_access":[{"global_model_id":"deepseek", "provider_ids":["b"]}]}
            ]
        })))
        .unwrap();
        let deepseek = config.resolve(Some("deepseek")).unwrap().unwrap();
        let claude = config.resolve(Some("claude")).unwrap().unwrap();
        let shared = config.resolve(Some("shared")).unwrap().unwrap();
        assert!(deepseek.allows("deepseek", "a"));
        assert!(deepseek.allows("deepseek", "b"));
        assert!(!deepseek.allows("claude", "a"));
        assert!(!deepseek.allows("deepseek", "c"));
        assert!(claude.allows("claude", "a"));
        assert!(!claude.allows("claude", "b"));
        assert!(shared.allows("deepseek", "b"));
        assert_eq!(deepseek.billing_snapshot().multiplier, 0.1);
        assert!(deepseek.billing_snapshot().model_access.is_none());
    }

    #[test]
    fn empty_access_denies_every_model_and_missing_access_preserves_existing_groups() {
        let mut group = PricingGroupsConfig::default().groups.remove(0);
        assert!(group.allows("any-model", "any-provider"));
        group.model_access = Some(Vec::new());
        assert!(!group.allows("any-model", "any-provider"));
        assert!(!group.allows_model("any-model"));
        let config = PricingGroupsConfig {
            groups: vec![group],
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn group_membership_rejects_empty_providers_and_duplicate_pairs() {
        for models in [
            json!([{"global_model_id":"model", "provider_ids":[]}]),
            json!([{"global_model_id":"model", "provider_ids":["a","a"]}]),
            json!([{"global_model_id":"model", "provider_ids":["a"]},
                   {"global_model_id":"model", "provider_ids":["b"]}]),
        ] {
            let mut config = serde_json::to_value(PricingGroupsConfig::default()).unwrap();
            config["groups"][0]["model_access"] = models;
            assert!(PricingGroupsConfig::from_value(Some(config)).is_err());
        }
    }

    #[test]
    fn administrator_can_assign_hidden_groups_and_preserve_them_while_disabled() {
        let mut config = PricingGroupsConfig {
            enabled: true,
            ..Default::default()
        };
        let mut hidden = config.groups[0].clone();
        hidden.id = "private".into();
        hidden.name = "专属".into();
        hidden.is_visible = false;
        config.groups.push(hidden);
        let assigned = admin_key_group_settings(&config, None, Some(Some("private")), None, true)
            .unwrap()
            .unwrap();
        assert_eq!(assigned["pricing_group_id"], "private");
        assert!(
            admin_key_group_settings(&config, None, Some(Some("missing")), None, true).is_err()
        );
        config.enabled = false;
        let preserved = admin_key_group_settings(
            &config,
            Some(assigned.clone()),
            None,
            Some(&assigned),
            false,
        )
        .unwrap()
        .unwrap();
        assert_eq!(preserved["pricing_group_id"], "private");
        assert!(admin_key_group_settings(
            &config,
            None,
            Some(Some("default")),
            Some(&assigned),
            false
        )
        .is_err());
    }

    #[test]
    fn materialized_group_prices_scale_video_and_preserve_small_and_zero_prices() {
        let base = json!({"default_price_per_request":0.02,
            "default_tiered_pricing":{"tiers":[{"up_to":200000,"input_price_per_1m":1,
                "output_price_per_1m":0,"cache_read_price_per_1m":0.000001,
                "cache_ttl_pricing":[{"ttl_minutes":60,"cache_creation_price_per_1m":2}]}]},
            "config":{"billing":{"video":{"price_per_second":0.2,
                "price_per_second_by_resolution":{"1080p":0.3}}}}});
        let prices = scale_model_sale_prices(&base, 0.1).unwrap();
        assert_eq!(prices["default_price_per_request"], 0.002);
        assert_eq!(
            prices["default_tiered_pricing"]["tiers"][0]["up_to"],
            200000
        );
        assert_eq!(
            prices["default_tiered_pricing"]["tiers"][0]["output_price_per_1m"].as_f64(),
            Some(0.0)
        );
        assert_eq!(
            prices["default_tiered_pricing"]["tiers"][0]["cache_ttl_pricing"][0]["ttl_minutes"],
            60
        );
        assert!(
            prices["default_tiered_pricing"]["tiers"][0]["cache_read_price_per_1m"]
                .as_f64()
                .unwrap()
                > 0.0
        );
        assert!(
            (prices["config"]["billing"]["video"]["price_per_second"]
                .as_f64()
                .unwrap()
                - 0.02)
                .abs()
                < 1e-12
        );
        assert!(scale_model_sale_prices(&base, -1.0).is_none());
        assert_eq!(base["default_price_per_request"], 0.02);
    }

    #[test]
    fn defaults_preserve_legacy_billing_until_enabled() {
        let mut config = PricingGroupsConfig::default();
        assert_eq!(config.resolve(None).unwrap(), None);
        assert!(config.visible_groups().is_empty());
        config.enabled = true;
        assert_eq!(config.resolve(None).unwrap().unwrap().multiplier, 1.0);
    }

    #[test]
    fn hidden_groups_remain_billable_but_are_not_advertised() {
        let mut config = PricingGroupsConfig {
            enabled: true,
            ..Default::default()
        };
        config.groups.push(PricingGroup {
            id: "private".into(),
            name: "专属".into(),
            multiplier: 0.5,
            is_visible: false,
            model_access: None,
        });
        config.validate().unwrap();
        assert_eq!(config.visible_groups().len(), 1);
        assert_eq!(
            config.resolve(Some("private")).unwrap().unwrap().multiplier,
            0.5
        );
        assert!(config.resolve(Some("missing")).is_err());
    }

    #[test]
    fn rejects_invalid_ratios_duplicates_and_hidden_defaults() {
        for multiplier in [-1.0, f64::INFINITY, f64::NAN] {
            let mut config = PricingGroupsConfig::default();
            config.groups[0].multiplier = multiplier;
            assert!(config.validate().is_err());
        }
        let mut config = PricingGroupsConfig::default();
        config.groups[0].multiplier = 0.0;
        config.validate().unwrap();
        config.groups.push(config.groups[0].clone());
        assert!(config.validate().is_err());
        config.groups.pop();
        config.groups[0].is_visible = false;
        assert!(config.validate().is_err());
    }

    #[test]
    fn scales_prices_and_keeps_processing_ratios_and_context_thresholds() {
        let pricing = json!({
            "tiers": [{"up_to": 200000, "input_price_per_1m": 2.0, "output_price_per_1m": 10.0,
                "cache_ttl_pricing": [{"ttl_minutes": 5, "cache_creation_price_per_1m": 3.0}]},
                {"up_to": null, "input_price_per_1m": 4.0}],
            "processing_tiers": {"priority": {"price_multiplier": 2.0},
                "flex": {"tiers": [{"input_price_per_1m": 1.0}]}},
            "image_output_price_default": 0.04
        });
        let scaled = scale_group_pricing(&pricing, 0.5).unwrap();
        assert_eq!(scaled["tiers"][0]["input_price_per_1m"], 1.0);
        assert_eq!(scaled["tiers"][0]["up_to"], 200000);
        assert_eq!(scaled["tiers"][1]["input_price_per_1m"], 2.0);
        assert_eq!(scaled["tiers"][0]["cache_ttl_pricing"][0]["ttl_minutes"], 5);
        assert_eq!(
            scaled["tiers"][0]["cache_ttl_pricing"][0]["cache_creation_price_per_1m"],
            1.5
        );
        assert_eq!(
            scaled["processing_tiers"]["priority"]["price_multiplier"],
            2.0
        );
        assert_eq!(
            scaled["processing_tiers"]["flex"]["tiers"][0]["input_price_per_1m"],
            0.5
        );
        assert_eq!(scaled["image_output_price_default"], 0.02);
        assert_eq!(pricing["tiers"][0]["input_price_per_1m"], 2.0);
    }
    #[test]
    fn key_selection_rejects_hidden_groups_and_ignores_feature_injection() {
        let mut config = PricingGroupsConfig {
            enabled: true,
            ..Default::default()
        };
        config.groups.push(PricingGroup {
            id: "private".into(),
            name: "专属".into(),
            multiplier: 0.5,
            is_visible: false,
            model_access: None,
        });
        let injected = Some(json!({"pricing_group_id": "private", "other": true}));
        let settings = user_key_group_settings(&config, injected, None, None, true)
            .unwrap()
            .unwrap();
        assert_eq!(settings["pricing_group_id"], "default");
        assert_eq!(settings["other"], true);
        assert!(user_key_group_settings(&config, None, Some(Some("private")), None, true).is_err());
        let current = json!({"pricing_group_id": "private"});
        assert_eq!(
            user_key_group_settings(&config, None, None, Some(&current), false).unwrap(),
            Some(current.clone())
        );
        assert_eq!(
            user_key_group_settings(&config, None, Some(Some("private")), Some(&current), false)
                .unwrap(),
            Some(current)
        );
        assert_eq!(
            user_key_group_settings(&config, None, Some(None), None, false)
                .unwrap()
                .unwrap()["pricing_group_id"],
            "default"
        );
    }

    #[test]
    fn group_charge_uses_base_prices_without_provider_overrides_or_discounts() {
        let pricing: BillingModelPricingSnapshot = serde_json::from_value(json!({
            "provider_id": "provider", "provider_billing_type": "free_tier", "global_model_id": "model",
            "global_model_name": "model", "default_price_per_request": 2,
            "model_price_per_request": 99, "provider_api_key_rate_multipliers": {"openai:chat": 0.1},
        })).unwrap();
        let mut input = crate::BillingUsageInput::new("chat");
        input.api_format = Some("openai:chat".to_string());
        let group = PricingGroup {
            id: "vip".into(),
            name: "VIP".into(),
            multiplier: 0.5,
            is_visible: true,
            model_access: None,
        };
        let mut legacy = crate::BillingService::new()
            .calculate(&pricing, &input)
            .unwrap();
        apply_group_multiplier(&mut legacy, None).unwrap();
        assert_eq!(legacy.actual_total_cost, 0.0);
        for format in [
            "openai:chat",
            "openai:responses",
            "claude:messages",
            "future:format",
        ] {
            input.api_format = Some(format.to_string());
            let base = customer_pricing_snapshot(pricing.clone(), Some(&group));
            let mut calculation = crate::BillingService::new()
                .calculate(&base, &input)
                .unwrap();
            assert_eq!(calculation.cost_result.cost, 2.0);
            apply_group_multiplier(&mut calculation, Some(&group)).unwrap();
            assert_eq!(calculation.actual_total_cost, 1.0);
            assert_eq!(calculation.rate_multiplier, 0.5);
        }
    }

    #[test]
    fn explicit_processing_catalog_is_scaled_even_with_a_legacy_multiplier() {
        let pricing = json!({"tiers": [{"up_to": null, "input_price_per_1m": 4}],
            "processing_tiers": {"priority": {"price_multiplier": 999, "tiers": [{"up_to": null, "input_price_per_1m": 6}]}}});
        let result = scale_group_pricing(&pricing, 0.5).unwrap();
        assert_eq!(
            result["processing_tiers"]["priority"]["tiers"][0]["input_price_per_1m"],
            3.0
        );
        assert_eq!(
            result["processing_tiers"]["priority"]["price_multiplier"],
            999
        );
    }
}
