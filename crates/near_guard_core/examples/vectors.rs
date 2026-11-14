//! Prints the reference vectors that `fixtures/policy_vectors.json` asserts on,
//! so the Rust engine and the TypeScript mirror in `web/` can be checked against
//! the same numbers.
//!
//!     cargo run -q -p near_guard_core --example vectors

use near_guard_core::{
    analytic_bound, format_near, micro_from_near, policy_commitment,
    report::build_report, Action, FuzzConfig, Policy, VaultState,
};
use serde_json::json;

fn main() {
    let weak = Policy::compile_json(include_str!("../../../policy/policy.json")).unwrap();
    let hardened =
        Policy::compile_json(include_str!("../../../policy/policy-hardened.json")).unwrap();

    let config = FuzzConfig {
        seed: 1337,
        sequences: 60,
        horizon_hours: 48,
        steps_per_sequence: 48,
    };

    let basic = Action::transfer("api-provider.testnet", micro_from_near(2.0), "NEAR", "data");
    let ts = 20_001u64 * near_guard_core::MS_PER_DAY + 60_000;
    let (verdict, _) = near_guard_core::evaluate(
        &weak,
        &VaultState::default(),
        &basic,
        ts,
        Some("noir-policy-proof"),
    );

    let weak_report = build_report(&weak, &config, 1_700_000_000_000);
    let hardened_report = build_report(&hardened, &config, 1_700_000_000_000);

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "policy_commitment": {
                "weak": policy_commitment(&weak),
                "hardened": policy_commitment(&hardened)
            },
            "audit_cid": {
                "action": "2 NEAR -> api-provider.testnet",
                "ts_ms": ts,
                "cid": verdict.ipld_cid,
                "proof_hash": verdict.proof_hash
            },
            "bounds": {
                "weak": analytic_bound(&weak),
                "hardened": analytic_bound(&hardened)
            },
            "range": {
                "config": config,
                "weak": {
                    "verdict": weak_report.verdict,
                    "report_hash": weak_report.report_hash,
                    "observed_max_loss_near": format_near(weak_report.range.observed_max_loss_micro),
                    "counterexample_kinds": weak_report.range.counterexamples.iter().map(|c| c.kind.clone()).collect::<Vec<_>>(),
                    "approved": weak_report.range.approved,
                    "blocked": weak_report.range.blocked
                },
                "hardened": {
                    "verdict": hardened_report.verdict,
                    "report_hash": hardened_report.report_hash,
                    "observed_max_loss_near": format_near(hardened_report.range.observed_max_loss_micro),
                    "counterexample_kinds": hardened_report.range.counterexamples.iter().map(|c| c.kind.clone()).collect::<Vec<_>>(),
                    "approved": hardened_report.range.approved,
                    "blocked": hardened_report.range.blocked
                }
            }
        }))
        .unwrap()
    );
}
