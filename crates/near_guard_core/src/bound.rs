//! The analytic loss bound.
//!
//! This is the number a policy owner actually needs: not "what does my policy
//! allow" but "how much can leave the vault in 24 hours if every single action
//! is legal". It is a closed-form upper bound under stated assumptions, not a
//! proof, and the report says so.

use serde::{Deserialize, Serialize};

use crate::policy::{CompiledPolicy, WindowModel};

/// Worst-case value at risk for one policy, in micro-NEAR.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LossBound {
    /// The number the owner wrote in `policy.json`.
    pub declared_daily_cap_micro: u128,
    pub per_tx_cap_micro: u128,
    /// How much the rate limit alone would allow in 24h.
    pub rate_ceiling_micro: u128,
    /// Value that can leave inside the spending window.
    pub spend_bound_micro: u128,
    /// Value destroyed by slippage on allowed swaps inside that window.
    pub slippage_leakage_micro: u128,
    /// `spend_bound + slippage_leakage`: total worst-case loss.
    pub total_value_at_risk_micro: u128,
    pub window_model: WindowModel,
    /// 2 for a calendar window (two day buckets fit in one 24h window), else 1.
    pub day_boundary_multiplier: u32,
    pub notes: Vec<String>,
}

impl LossBound {
    /// Does the policy actually bound losses to the number it advertises?
    pub fn honours_declared_cap(&self) -> bool {
        self.total_value_at_risk_micro <= self.declared_daily_cap_micro
    }
}

/// Compute the bound. Assumptions are listed in `notes` and repeated in the
/// report, because a bound without assumptions is marketing.
pub fn analytic_bound(policy: &CompiledPolicy) -> LossBound {
    let rate_ceiling = (policy.rate_limit_per_hour as u128)
        .saturating_mul(policy.per_tx_cap_micro)
        .saturating_mul(24);

    let window_multiplier: u32 = match policy.window_model {
        WindowModel::Calendar => 2,
        WindowModel::Rolling => 1,
    };

    let mut spend_bound = policy.daily_cap_micro.min(rate_ceiling);
    if spend_bound == 0 {
        // Rate ceiling below one action: still one action can slip through.
        spend_bound = policy.per_tx_cap_micro.min(policy.daily_cap_micro);
    }
    spend_bound = spend_bound.saturating_mul(window_multiplier as u128);

    let slippage_leakage =
        spend_bound.saturating_mul(policy.max_slippage_bps as u128) / 10_000;

    let mut notes = vec![
        format!(
            "Enforcement ceiling: min(daily_cap, rate_limit_per_hour x per_tx_cap x 24) = min({}, {} x {} x 24) micro-NEAR.",
            policy.daily_cap_micro, policy.rate_limit_per_hour, policy.per_tx_cap_micro
        ),
        match policy.window_model {
            WindowModel::Calendar => "Calendar-day window: a 24h interval can contain two full day buckets, so the bound doubles.".to_string(),
            WindowModel::Rolling => "Rolling 24h window: a sliding window can hold at most daily_cap, so the bound does not double.".to_string(),
        },
        format!(
            "Slippage: allowed swaps can burn up to {} bps of the spend bound.",
            policy.max_slippage_bps
        ),
        "Assumes the onchain GuardVault enforces exactly this policy and the agent holds no other key.".to_string(),
        "Assumes allowlisted recipients do not forward funds onward; forwarding is a trust assumption, not something the chain can see.".to_string(),
        "Excludes asset price movement: this bounds policy leakage, not market risk.".to_string(),
    ];
    if policy.max_slippage_bps == 0 {
        notes.push("Swaps disabled by policy: slippage leakage is zero.".to_string());
    }

    LossBound {
        declared_daily_cap_micro: policy.daily_cap_micro,
        per_tx_cap_micro: policy.per_tx_cap_micro,
        rate_ceiling_micro: rate_ceiling,
        spend_bound_micro: spend_bound,
        slippage_leakage_micro: slippage_leakage,
        total_value_at_risk_micro: spend_bound.saturating_add(slippage_leakage),
        window_model: policy.window_model,
        day_boundary_multiplier: window_multiplier,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::Policy;

    fn compiled(window: WindowModel, per_tx: f64, daily: f64, rate: u32, bps: u32) -> CompiledPolicy {
        Policy {
            version: 1,
            owner_id: "owner.testnet".into(),
            vault_id: "vault.testnet".into(),
            agent_id: "agent.testnet".into(),
            per_tx_cap: per_tx,
            daily_cap: daily,
            rate_limit_per_hour: rate,
            window_model: window,
            allowlist: vec!["api.testnet".into()],
            allowed_assets: vec!["NEAR".into()],
            max_slippage_bps: bps,
            require_zk: true,
        }
        .compile()
        .unwrap()
    }

    #[test]
    fn calendar_window_doubles_the_bound() {
        let calendar = analytic_bound(&compiled(WindowModel::Calendar, 5.0, 10.0, 4, 100));
        let rolling = analytic_bound(&compiled(WindowModel::Rolling, 5.0, 10.0, 4, 100));
        assert_eq!(calendar.spend_bound_micro, 20_000_000);
        assert_eq!(rolling.spend_bound_micro, 10_000_000);
        assert_eq!(calendar.slippage_leakage_micro, 200_000);
        assert_eq!(calendar.total_value_at_risk_micro, 20_200_000);
        assert!(!calendar.honours_declared_cap());
        assert!(!rolling.honours_declared_cap(), "100 bps slippage still leaks 0.1 NEAR");
    }

    #[test]
    fn rate_limit_can_bind_before_the_daily_cap() {
        let b = analytic_bound(&compiled(WindowModel::Rolling, 1.0, 100.0, 2, 0));
        assert_eq!(b.rate_ceiling_micro, 48_000_000);
        assert_eq!(b.spend_bound_micro, 48_000_000);
    }
}
