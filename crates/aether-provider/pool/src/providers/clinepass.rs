use std::collections::BTreeMap;

use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogEndpoint;
use chrono::DateTime;
use serde_json::{json, Value};

use crate::capability::ProviderPoolCapabilities;
use crate::provider::{
    provider_pool_endpoint_format_matches, provider_pool_matching_endpoint, ProviderPoolAdapter,
};
use crate::quota_refresh::ProviderPoolQuotaRequestSpec;

#[derive(Debug, Clone, Default)]
pub struct ClinePassProviderPoolAdapter;

impl ProviderPoolAdapter for ClinePassProviderPoolAdapter {
    fn provider_type(&self) -> &'static str {
        "clinepass"
    }

    fn capabilities(&self) -> ProviderPoolCapabilities {
        ProviderPoolCapabilities {
            plan_tier: false,
            quota_reset: true,
            quota_refresh: true,
        }
    }

    fn quota_refresh_endpoint(
        &self,
        endpoints: &[StoredProviderCatalogEndpoint],
        include_inactive: bool,
    ) -> Option<StoredProviderCatalogEndpoint> {
        provider_pool_matching_endpoint(endpoints, include_inactive, |endpoint| {
            provider_pool_endpoint_format_matches(endpoint, "openai:chat")
        })
    }
}

pub fn build_clinepass_pool_quota_request(
    key_id: &str,
    base_url: &str,
    authorization: (String, String),
    limits: bool,
) -> ProviderPoolQuotaRequestSpec {
    let suffix = if limits {
        "users/me/plan/usage-limits"
    } else {
        "users/me/plan"
    };
    ProviderPoolQuotaRequestSpec {
        request_id: format!(
            "clinepass-quota:{key_id}:{}",
            if limits { "limits" } else { "plan" }
        ),
        provider_name: "ClinePass".to_string(),
        quota_kind: "clinepass".to_string(),
        method: "GET".to_string(),
        url: format!("{}/{suffix}", base_url.trim_end_matches('/')),
        headers: BTreeMap::from([
            authorization,
            ("accept".to_string(), "application/json".to_string()),
        ]),
        content_type: None,
        json_body: None,
        client_api_format: "openai:chat".to_string(),
        provider_api_format: "clinepass:quota".to_string(),
        model_name: None,
    }
}

pub fn parse_clinepass_quota(
    plan_response: &Value,
    limits_response: Option<&Value>,
    now: u64,
) -> Option<Value> {
    if plan_response.get("success").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let data = plan_response.get("data")?;
    let plan = data.get("plan")?.as_object()?;
    let name = plan
        .get("displayName")
        .or_else(|| plan.get("name"))
        .and_then(Value::as_str);
    let caps = plan
        .get("entitlements")
        .and_then(|entitlements| {
            entitlements
                .get("cline_pass")
                .or_else(|| entitlements.get("clinePass"))
        })
        .and_then(|pass| pass.get("inferenceCapThreshold"));
    let limits = limits_response
        .filter(|response| response.get("success").and_then(Value::as_bool) != Some(false))
        .and_then(|response| response.pointer("/data/limits"))
        .and_then(Value::as_array);
    let mut windows = Vec::new();
    for (kind, label, minutes, cap_field) in [
        ("five_hour", "5h", 300, "last5HoursUsageCostUSDPerUser"),
        ("weekly", "7d", 10_080, "last7daysUsageCostUSDPerUser"),
        ("monthly", "30d", 43_200, "last30daysUsageCostUSDPerUser"),
    ] {
        let limit = limits.and_then(|limits| {
            limits
                .iter()
                .find(|limit| limit.get("type").and_then(Value::as_str) == Some(kind))
        });
        let used_percent = limit
            .and_then(|limit| limit.get("percentUsed"))
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value >= 0.0);
        let reset_at = limit
            .and_then(|limit| limit.get("resetsAt"))
            .and_then(Value::as_str)
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .and_then(|value| u64::try_from(value.timestamp()).ok());
        let expired = reset_at.is_some_and(|reset| reset <= now);
        let exhausted = !expired && used_percent.is_some_and(|percent| percent >= 100.0);
        let reset_at = if exhausted {
            Some(
                reset_at
                    .filter(|reset| *reset <= now.saturating_add(32 * 86_400))
                    .unwrap_or(now.saturating_add(300)),
            )
        } else {
            reset_at
        };
        windows.push(json!({
            "code": kind, "label": label, "scope": "account", "unit": "percent", "window_minutes": minutes,
            "used_ratio": used_percent.map(|percent| if expired { 0.0 } else { percent.clamp(0.0, 100.0) / 100.0 }),
            "remaining_ratio": used_percent.map(|percent| if expired { 1.0 } else { (100.0 - percent).clamp(0.0, 100.0) / 100.0 }),
            "is_exhausted": exhausted, "reset_at": reset_at,
            "cap_usd": caps.and_then(|caps| caps.get(cap_field)).and_then(Value::as_f64).map(|cap| cap / 100_000_000.0),
        }));
    }
    let exhausted = windows.iter().any(|window| window["is_exhausted"] == true);
    let reset_at = windows
        .iter()
        .filter(|window| window["is_exhausted"] == true)
        .filter_map(|window| window["reset_at"].as_u64())
        .max();
    let usage_ratio = windows
        .iter()
        .filter_map(|window| window["used_ratio"].as_f64())
        .reduce(f64::max);
    Some(json!({
        "plan_type": name, "active": plan.get("isActive"), "current_period_end": data.get("currentPeriodEnd"),
        "windows": windows, "exhausted": exhausted, "usage_ratio": usage_ratio, "reset_at": reset_at,
        "updated_at": now, "observed_at": now, "provider_type": "clinepass", "source": "quota_api",
        "code": if exhausted { "exhausted" } else if usage_ratio.is_some() { "ok" } else { "unknown" },
        "freshness": if usage_ratio.is_some() { "fresh" } else { "unknown" },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;

    #[test]
    fn clinepass_pool_excludes_any_exhausted_window_until_its_reset() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut key = StoredProviderCatalogKey::new(
            "key".to_string(),
            "provider".to_string(),
            "account".to_string(),
            "api_key".to_string(),
            None,
            true,
        )
        .unwrap();
        key.status_snapshot = Some(
            json!({"quota":{"provider_type":"clinepass","plan_type":"monthly","exhausted":true,"windows":[
                {"code":"five_hour","scope":"account","used_ratio":0.1,"is_exhausted":false},
                {"code":"weekly","scope":"account","used_ratio":1.0,"is_exhausted":true,"reset_at":now+3600},
                {"code":"monthly","scope":"account","used_ratio":0.2,"is_exhausted":false}
            ]}}),
        );
        fn input(key: &StoredProviderCatalogKey) -> crate::ProviderPoolMemberInput<'_> {
            crate::ProviderPoolMemberInput {
                provider_type: "clinepass",
                key,
                auth_config: None,
                provider_model_name: Some("test"),
            }
        }
        assert!(ClinePassProviderPoolAdapter.quota_exhausted(&input(&key)));
        assert!(ClinePassProviderPoolAdapter
            .member_signals(&input(&key))
            .plan_tier
            .is_some());
        key.status_snapshot.as_mut().unwrap()["quota"]["windows"][1]["reset_at"] = json!(now - 1);
        assert!(!ClinePassProviderPoolAdapter.quota_exhausted(&input(&key)));
    }

    #[test]
    fn clinepass_missing_reset_uses_bounded_retry_and_elapsed_windows_recover() {
        let plan = json!({"data":{"plan":{"displayName":"Monthly"}}});
        let limits = json!({"data":{"limits":[{"type":"five_hour","percentUsed":100},{"type":"weekly","percentUsed":100,"resetsAt":"2020-01-01T00:00:00Z"}]}});
        let quota = parse_clinepass_quota(&plan, Some(&limits), 1_790_985_600).unwrap();
        assert_eq!(quota["windows"][0]["reset_at"], 1_790_985_900u64);
        assert_eq!(quota["windows"][0]["is_exhausted"], true);
        assert_eq!(quota["windows"][1]["remaining_ratio"], 1.0);
        assert_eq!(quota["windows"][1]["is_exhausted"], false);
        assert!(quota["windows"][2]["remaining_ratio"].is_null());
    }

    #[test]
    fn parses_independent_windows_caps_and_subscription_expiry() {
        let plan = json!({"success":true,"data":{"currentPeriodEnd":"2026-10-14T00:00:00Z","plan":{"displayName":"Cline Pass (Monthly)","isActive":true,"entitlements":{"cline_pass":{"inferenceCapThreshold":{"last5HoursUsageCostUSDPerUser":1000000000}}}}}});
        let limits = json!({"data":{"limits":[{"type":"five_hour","percentUsed":10},{"type":"weekly","percentUsed":100,"resetsAt":"2026-10-04T00:00:00Z"},{"type":"monthly","percentUsed":35}]}});
        let parsed = parse_clinepass_quota(&plan, Some(&limits), 1_790_985_600).unwrap();
        assert_eq!(parsed["windows"][0]["remaining_ratio"], 0.9);
        assert_eq!(parsed["windows"][0]["cap_usd"], 10.0);
        assert_eq!(parsed["windows"][1]["remaining_ratio"], 0.0);
        assert_eq!(parsed["windows"][2]["remaining_ratio"], 0.65);
        assert_eq!(parsed["exhausted"], true);
        assert_eq!(parsed["current_period_end"], "2026-10-14T00:00:00Z");
        let unknown = parse_clinepass_quota(&plan, None, 1).unwrap();
        assert!(unknown["windows"][0]["remaining_ratio"].is_null());
        assert_eq!(unknown["exhausted"], false);
    }
}
