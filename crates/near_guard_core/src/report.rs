//! The loss-bound report: what the owner actually buys.
//!
//! `report_hash` is deliberately independent of the wall clock, so the same
//! policy and seed always produce the same hash. That is what allows the hash to
//! be anchored on NEAR and re-verified by a third party later.

use serde::{Deserialize, Serialize};

use crate::bound::{analytic_bound, LossBound};
use crate::cid::{cid_v1_dag_cbor, policy_commitment};
use crate::fuzz::{run_range, FuzzConfig, RangeResult};
use crate::policy::CompiledPolicy;

pub const DISCLAIMER: &str = "This report is an analytic upper bound plus an empirical adversarial search, not a formal proof. 'No counterexample found' does not mean none exists: it means the strategies in this build did not find one within the budget. The bound assumes the onchain GuardVault enforces exactly this policy, that the agent holds no other signing key, and that allowlisted recipients do not forward funds. Asset price risk is out of scope.";

/// Everything the range found, plus the bound it is compared against.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RangeReport {
    pub schema: String,
    pub generated_at_ms: u64,
    pub policy_id: String,
    pub policy_commitment: String,
    pub report_hash: String,
    pub report_cid: String,
    pub declared_daily_cap_micro: u128,
    pub bound: LossBound,
    pub range: RangeResult,
    /// `bounded` | `under_bounded` | `bound_violation`
    pub verdict: String,
    pub verdict_detail: String,
    pub disclaimer: String,
}

/// Canonical bytes for the report hash. Excludes `generated_at_ms` on purpose.
pub fn canonical_report_bytes(
    policy: &CompiledPolicy,
    config: &FuzzConfig,
    bound: &LossBound,
    range: &RangeResult,
) -> String {
    let kinds: Vec<String> = range
        .counterexamples
        .iter()
        .map(|c| c.kind.clone())
        .collect();
    format!(
        "near-agent-guard/report/v1|commitment={}|seed={}|sequences={}|horizon_hours={}|steps={}|declared={}|bound_total={}|observed={}|approved={}|blocked={}|actions={}|counterexamples={}|bound_violations={}",
        policy_commitment(policy),
        config.seed,
        config.sequences,
        config.horizon_hours,
        config.steps_per_sequence,
        bound.declared_daily_cap_micro,
        bound.total_value_at_risk_micro,
        range.observed_max_loss_micro,
        range.approved,
        range.blocked,
        range.actions_simulated,
        kinds.join("+"),
        range.bound_violations
    )
}

/// Run the range and wrap it in a report with a reproducible hash.
pub fn build_report(
    policy: &CompiledPolicy,
    config: &FuzzConfig,
    generated_at_ms: u64,
) -> RangeReport {
    let bound = analytic_bound(policy);
    let range = run_range(policy, config);
    let canonical = canonical_report_bytes(policy, &range.config, &bound, &range);
    let report_hash = cid_v1_dag_cbor(canonical.as_bytes());

    let verdict = if range.bound_violations > 0 {
        "bound_violation"
    } else if range.counterexamples.is_empty() {
        "bounded"
    } else {
        "under_bounded"
    };

    let verdict_detail = match verdict {
        "bound_violation" => format!(
            "INTERNAL: the range realised {} micro-NEAR of loss, above the computed bound of {} micro-NEAR. The bound calculator is wrong; do not rely on this report.",
            range.observed_max_loss_micro, bound.total_value_at_risk_micro
        ),
        "under_bounded" => format!(
            "The policy advertises a {} NEAR daily cap but the range realised {} NEAR of loss in a single 24h window. Worst case from the analytic bound: {} NEAR.",
            crate::format_near(bound.declared_daily_cap_micro),
            crate::format_near(range.observed_max_loss_micro),
            crate::format_near(bound.total_value_at_risk_micro)
        ),
        _ => format!(
            "No legal sequence exceeded the declared {} NEAR cap across {} sequences and {} simulated actions. Worst case from the analytic bound: {} NEAR.",
            crate::format_near(bound.declared_daily_cap_micro),
            range.sequences_run,
            range.actions_simulated,
            crate::format_near(bound.total_value_at_risk_micro)
        ),
    };

    RangeReport {
        schema: "near-agent-guard/range-report/v1".to_string(),
        generated_at_ms,
        policy_id: format!("{}::{}", policy.source.vault_id, policy.source.agent_id),
        policy_commitment: policy_commitment(policy),
        report_cid: report_hash.clone(),
        report_hash,
        declared_daily_cap_micro: bound.declared_daily_cap_micro,
        bound,
        range,
        verdict: verdict.to_string(),
        verdict_detail,
        disclaimer: DISCLAIMER.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::Policy;

    fn policy(window: &str) -> CompiledPolicy {
        Policy::from_json(&format!(
            r#"{{
                "owner_id": "owner.testnet", "vault_id": "vault.testnet", "agent_id": "agent.testnet",
                "per_tx_cap": 5.0, "daily_cap": 10.0, "rate_limit_per_hour": 4,
                "window_model": "{window}",
                "allowlist": ["api.testnet"], "allowed_assets": ["NEAR", "USDC"],
                "max_slippage_bps": 100
            }}"#
        ))
        .unwrap()
        .compile()
        .unwrap()
    }

    #[test]
    fn hash_is_reproducible_across_runs_and_clocks() {
        let p = policy("calendar");
        let a = build_report(&p, &FuzzConfig::default(), 1);
        let b = build_report(&p, &FuzzConfig::default(), 999_999);
        assert_eq!(a.report_hash, b.report_hash);
        assert!(a.report_hash.starts_with("bafyrei"));
    }

    #[test]
    fn verdict_reflects_the_window_model() {
        let weak = build_report(&policy("calendar"), &FuzzConfig::default(), 1);
        let hardened = build_report(&policy("rolling"), &FuzzConfig::default(), 1);
        assert_eq!(weak.verdict, "under_bounded");
        assert_eq!(hardened.verdict, "bounded");
        assert_ne!(weak.report_hash, hardened.report_hash);
    }
}
