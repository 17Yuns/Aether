use async_trait::async_trait;

use crate::repository::auth::{
    normalize_api_key_billing_multiplier, ApiKeyBillingSourceMode,
    DEFAULT_API_KEY_BILLING_MULTIPLIER,
};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UsageSettlementInput {
    pub request_id: String,
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    #[serde(default)]
    pub api_key_is_standalone: bool,
    pub provider_id: Option<String>,
    pub status: String,
    pub billing_status: String,
    pub total_cost_usd: f64,
    pub actual_total_cost_usd: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_actual_total_cost_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_source_mode: Option<ApiKeyBillingSourceMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallet_billing_multiplier: Option<f64>,
    pub finalized_at_unix_secs: Option<u64>,
}

impl UsageSettlementInput {
    pub fn validate(&self) -> Result<(), crate::DataLayerError> {
        if self.request_id.trim().is_empty() {
            return Err(crate::DataLayerError::InvalidInput(
                "settlement request_id cannot be empty".to_string(),
            ));
        }
        if self.status.trim().is_empty() || self.billing_status.trim().is_empty() {
            return Err(crate::DataLayerError::InvalidInput(
                "settlement status cannot be empty".to_string(),
            ));
        }
        if !self.total_cost_usd.is_finite()
            || !self.actual_total_cost_usd.is_finite()
            || self
                .provider_actual_total_cost_usd
                .is_some_and(|value| !value.is_finite())
            || self
                .wallet_billing_multiplier
                .is_some_and(|value| !value.is_finite())
        {
            return Err(crate::DataLayerError::InvalidInput(
                "settlement cost must be finite".to_string(),
            ));
        }
        normalize_api_key_billing_multiplier(self.wallet_billing_multiplier)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StoredUsageSettlement {
    pub request_id: String,
    pub wallet_id: Option<String>,
    pub billing_status: String,
    pub wallet_balance_before: Option<f64>,
    pub wallet_balance_after: Option<f64>,
    pub wallet_recharge_balance_before: Option<f64>,
    pub wallet_recharge_balance_after: Option<f64>,
    pub wallet_gift_balance_before: Option<f64>,
    pub wallet_gift_balance_after: Option<f64>,
    pub provider_monthly_used_usd: Option<f64>,
    pub finalized_at_unix_secs: Option<u64>,
}

#[async_trait]
pub trait SettlementWriteRepository: Send + Sync {
    async fn settle_usage(
        &self,
        input: UsageSettlementInput,
    ) -> Result<Option<StoredUsageSettlement>, crate::DataLayerError>;
}

pub trait SettlementRepository: SettlementWriteRepository + Send + Sync {}

impl<T> SettlementRepository for T where T: SettlementWriteRepository + Send + Sync {}

pub const SETTLEMENT_EPSILON_USD: f64 = 0.000_000_01;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DailyQuotaBillingCapacity {
    pub remaining_usd: f64,
    pub billing_multiplier: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceAwareSettlementPlan {
    pub quota_debits_usd: Vec<f64>,
    pub package_provider_cost_usd: f64,
    pub package_billed_cost_usd: f64,
    pub wallet_provider_cost_usd: f64,
    pub wallet_debit_usd: f64,
    pub total_billed_cost_usd: f64,
    pub insufficient: bool,
}

pub fn source_aware_settlement_cost_usd(input: &UsageSettlementInput) -> f64 {
    input
        .provider_actual_total_cost_usd
        .unwrap_or(input.actual_total_cost_usd)
        .max(0.0)
}

pub fn source_aware_wallet_billing_multiplier(input: &UsageSettlementInput) -> f64 {
    if input.provider_actual_total_cost_usd.is_none() {
        return DEFAULT_API_KEY_BILLING_MULTIPLIER;
    }
    input
        .wallet_billing_multiplier
        .unwrap_or(DEFAULT_API_KEY_BILLING_MULTIPLIER)
}

pub fn plan_source_aware_settlement(
    provider_cost_usd: f64,
    wallet_billing_multiplier: f64,
    billing_source_mode: ApiKeyBillingSourceMode,
    quota_capacities: &[DailyQuotaBillingCapacity],
    wallet_available_usd: Option<f64>,
    wallet_can_overdraft: bool,
) -> Result<SourceAwareSettlementPlan, crate::DataLayerError> {
    if !provider_cost_usd.is_finite() || provider_cost_usd < 0.0 {
        return Err(crate::DataLayerError::InvalidInput(
            "source-aware settlement cost must be finite and non-negative".to_string(),
        ));
    }
    let wallet_billing_multiplier =
        normalize_api_key_billing_multiplier(Some(wallet_billing_multiplier))?;
    if wallet_available_usd.is_some_and(|value| !value.is_finite() || value < 0.0) {
        return Err(crate::DataLayerError::InvalidInput(
            "source-aware wallet capacity must be finite and non-negative".to_string(),
        ));
    }

    let mut quota_debits_usd = vec![0.0; quota_capacities.len()];
    let mut remaining_provider_cost_usd = provider_cost_usd;
    if billing_source_mode != ApiKeyBillingSourceMode::Wallet {
        for (index, capacity) in quota_capacities.iter().enumerate() {
            if !capacity.remaining_usd.is_finite() || capacity.remaining_usd < 0.0 {
                return Err(crate::DataLayerError::InvalidInput(
                    "daily quota capacity must be finite and non-negative".to_string(),
                ));
            }
            let multiplier =
                normalize_api_key_billing_multiplier(Some(capacity.billing_multiplier))?;
            if remaining_provider_cost_usd <= SETTLEMENT_EPSILON_USD {
                break;
            }
            if capacity.remaining_usd <= SETTLEMENT_EPSILON_USD {
                continue;
            }
            if multiplier <= SETTLEMENT_EPSILON_USD {
                remaining_provider_cost_usd = 0.0;
                break;
            }

            let covered_provider_cost_usd = remaining_provider_cost_usd
                .min(capacity.remaining_usd / multiplier)
                .max(0.0);
            let debit_usd = quantize_settlement_cost(covered_provider_cost_usd * multiplier)
                .min(capacity.remaining_usd)
                .max(0.0);
            quota_debits_usd[index] = debit_usd;
            remaining_provider_cost_usd =
                (remaining_provider_cost_usd - covered_provider_cost_usd).max(0.0);
        }
    }

    let package_provider_cost_usd = (provider_cost_usd - remaining_provider_cost_usd).max(0.0);
    let package_billed_cost_usd = quantize_settlement_cost(quota_debits_usd.iter().sum::<f64>());
    let package_only_insufficient = billing_source_mode == ApiKeyBillingSourceMode::Package
        && remaining_provider_cost_usd > SETTLEMENT_EPSILON_USD;
    let wallet_provider_cost_usd = if billing_source_mode == ApiKeyBillingSourceMode::Package {
        0.0
    } else {
        remaining_provider_cost_usd
    };
    let wallet_debit_usd =
        quantize_settlement_cost(wallet_provider_cost_usd * wallet_billing_multiplier);
    let wallet_insufficient = wallet_debit_usd > SETTLEMENT_EPSILON_USD
        && !wallet_can_overdraft
        && wallet_available_usd
            .is_some_and(|available| available + SETTLEMENT_EPSILON_USD < wallet_debit_usd);
    let insufficient = package_only_insufficient || wallet_insufficient;

    Ok(SourceAwareSettlementPlan {
        quota_debits_usd,
        package_provider_cost_usd,
        package_billed_cost_usd,
        wallet_provider_cost_usd,
        wallet_debit_usd,
        total_billed_cost_usd: quantize_settlement_cost(package_billed_cost_usd + wallet_debit_usd),
        insufficient,
    })
}

fn quantize_settlement_cost(value: f64) -> f64 {
    const FACTOR: f64 = 100_000_000.0;
    (value * FACTOR).round() / FACTOR
}

#[derive(Debug, Clone, Copy)]
pub struct WalletDebitPlan {
    pub recharge_deduction: f64,
    pub gift_deduction: f64,
    pub recharge_overdraft: f64,
}

impl WalletDebitPlan {
    pub fn after_balances(self, recharge_balance: f64, gift_balance: f64) -> (f64, f64) {
        (
            recharge_balance - self.recharge_deduction - self.recharge_overdraft,
            gift_balance - self.gift_deduction,
        )
    }
}

pub fn finite_wallet_available_usd(recharge_balance: f64, gift_balance: f64) -> f64 {
    recharge_balance.max(0.0) + gift_balance.max(0.0)
}

pub fn plan_finite_wallet_debit(
    recharge_balance: f64,
    gift_balance: f64,
    requested_usd: f64,
) -> WalletDebitPlan {
    let requested_usd = requested_usd.max(0.0);
    let recharge_deduction = recharge_balance.max(0.0).min(requested_usd);
    let after_recharge_remaining = (requested_usd - recharge_deduction).max(0.0);
    let gift_deduction = gift_balance.max(0.0).min(after_recharge_remaining);
    let recharge_overdraft = (after_recharge_remaining - gift_deduction).max(0.0);
    WalletDebitPlan {
        recharge_deduction,
        gift_deduction,
        recharge_overdraft,
    }
}

pub fn settlement_billing_status_for_usage_status(status: &str) -> &'static str {
    match status {
        "completed" | "cancelled" => "settled",
        _ => "void",
    }
}

pub fn settlement_billable_cost_usd(input: &UsageSettlementInput) -> f64 {
    input.actual_total_cost_usd.max(0.0)
}

pub fn settlement_provider_usage_cost_usd(input: &UsageSettlementInput) -> f64 {
    input
        .provider_actual_total_cost_usd
        .unwrap_or(input.actual_total_cost_usd)
        .max(0.0)
}

#[cfg(test)]
mod tests {
    use super::{
        plan_source_aware_settlement, settlement_provider_usage_cost_usd,
        DailyQuotaBillingCapacity, UsageSettlementInput,
    };
    use crate::repository::auth::ApiKeyBillingSourceMode;

    #[test]
    fn rejects_invalid_settlement_input() {
        let input = UsageSettlementInput {
            request_id: "".to_string(),
            user_id: None,
            api_key_id: None,
            api_key_is_standalone: false,
            provider_id: None,
            status: "completed".to_string(),
            billing_status: "pending".to_string(),
            total_cost_usd: 0.1,
            actual_total_cost_usd: 0.1,
            provider_actual_total_cost_usd: None,
            billing_source_mode: None,
            wallet_billing_multiplier: None,
            finalized_at_unix_secs: None,
        };
        assert!(input.validate().is_err());
    }

    #[test]
    fn provider_usage_cost_prefers_explicit_provider_cost_and_falls_back() {
        let mut input = UsageSettlementInput {
            request_id: "request-1".to_string(),
            user_id: None,
            api_key_id: None,
            api_key_is_standalone: false,
            provider_id: Some("provider-1".to_string()),
            status: "completed".to_string(),
            billing_status: "pending".to_string(),
            total_cost_usd: 1.0,
            actual_total_cost_usd: 2.0,
            provider_actual_total_cost_usd: None,
            billing_source_mode: None,
            wallet_billing_multiplier: None,
            finalized_at_unix_secs: None,
        };
        assert_eq!(settlement_provider_usage_cost_usd(&input), 2.0);

        input.provider_actual_total_cost_usd = Some(0.75);
        assert_eq!(settlement_provider_usage_cost_usd(&input), 0.75);
    }

    #[test]
    fn source_aware_plan_splits_package_and_wallet_with_distinct_multipliers() {
        let plan = plan_source_aware_settlement(
            10.0,
            1.5,
            ApiKeyBillingSourceMode::Auto,
            &[DailyQuotaBillingCapacity {
                remaining_usd: 4.0,
                billing_multiplier: 0.5,
            }],
            Some(10.0),
            false,
        )
        .expect("allocation should build");

        assert_eq!(plan.quota_debits_usd, vec![4.0]);
        assert_eq!(plan.package_provider_cost_usd, 8.0);
        assert_eq!(plan.wallet_provider_cost_usd, 2.0);
        assert_eq!(plan.wallet_debit_usd, 3.0);
        assert_eq!(plan.total_billed_cost_usd, 7.0);
        assert!(!plan.insufficient);
    }

    #[test]
    fn package_source_is_all_or_nothing() {
        let plan = plan_source_aware_settlement(
            10.0,
            1.0,
            ApiKeyBillingSourceMode::Package,
            &[DailyQuotaBillingCapacity {
                remaining_usd: 4.0,
                billing_multiplier: 1.0,
            }],
            Some(100.0),
            true,
        )
        .expect("allocation should build");

        assert!(plan.insufficient);
        assert_eq!(plan.wallet_debit_usd, 0.0);
    }
}
