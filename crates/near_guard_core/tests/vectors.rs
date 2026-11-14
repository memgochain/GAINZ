//! Parity test: the shared vectors in `fixtures/policy_vectors.json` are also
//! asserted by the TypeScript mirror in `web/src/lib/engine.test.ts`. If this
//! test fails, the CLI, the MCP server and the web app disagree about what a
//! policy means — which would make every report a lie.

use near_guard_core::{
    analytic_bound, micro_from_near, policy_commitment, report::build_report, Action, FuzzConfig,
    Policy, VaultState,
};
use serde_json::Value;

fn fixtures() -> Value {
    serde_json::from_str(include_str!("../../../fixtures/policy_vectors.json"))
        .expect("fixtures must be valid JSON")
}

fn compile(policy_name: &str) -> near_guard_core::CompiledPolicy {
    let data = fixtures();
    let policy: Policy =
        serde_json::from_value(data["policies"][policy_name].clone()).expect("policy parses");
    policy.compile().expect("policy compiles")
}

#[test]
fn commitments_match_the_shared_vectors() {
    let data = fixtures();
    for name in ["weak", "hardened"] {
        assert_eq!(
            policy_commitment(&compile(name)),
            data["commitments"][name].as_str().unwrap(),
            "commitment for {name}"
        );
    }
}

#[test]
fn audit_cid_and_proof_hash_match() {
    let data = fixtures();
    let case = &data["audit_cid_case"];
    let policy = compile(case["policy"].as_str().unwrap());
    let action: Action = serde_json::from_value(case["action"].clone()).unwrap();
    let ts = case["ts_ms"].as_u64().unwrap();
    let (verdict, _) = near_guard_core::evaluate(
        &policy,
        &VaultState::default(),
        &action,
        ts,
        Some("noir-policy-proof"),
    );
    assert!(verdict.approved);
    assert_eq!(verdict.ipld_cid, case["cid"].as_str().unwrap());
    assert_eq!(verdict.proof_hash, case["proof_hash"].as_str().unwrap());
}

#[test]
fn bounds_match_the_shared_vectors() {
    let data = fixtures();
    for name in ["weak", "hardened"] {
        let bound = analytic_bound(&compile(name));
        let expected = &data["bounds"][name];
        assert_eq!(
            bound.declared_daily_cap_micro as u64,
            expected["declared_daily_cap_micro"].as_u64().unwrap(),
            "{name} declared"
        );
        assert_eq!(
            bound.per_tx_cap_micro as u64,
            expected["per_tx_cap_micro"].as_u64().unwrap()
        );
        assert_eq!(
            bound.rate_ceiling_micro as u64,
            expected["rate_ceiling_micro"].as_u64().unwrap()
        );
        assert_eq!(
            bound.spend_bound_micro as u64,
            expected["spend_bound_micro"].as_u64().unwrap()
        );
        assert_eq!(
            bound.slippage_leakage_micro as u64,
            expected["slippage_leakage_micro"].as_u64().unwrap()
        );
        assert_eq!(
            bound.total_value_at_risk_micro as u64,
            expected["total_value_at_risk_micro"].as_u64().unwrap()
        );
        assert_eq!(
            bound.day_boundary_multiplier,
            expected["day_boundary_multiplier"].as_u64().unwrap() as u32
        );
    }
}

#[test]
fn transfer_decisions_match_the_shared_vectors() {
    let data = fixtures();
    let cases = data["transfers"].as_array().expect("transfers array");
    assert!(cases.len() >= 13);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let policy = compile(case["policy"].as_str().unwrap());
        let action: Action = serde_json::from_value(case["action"].clone())
            .unwrap_or_else(|e| panic!("{name}: action must parse: {e}"));
        let state: VaultState = serde_json::from_value(serde_json::json!({
            "history": case["history"].clone()
        }))
        .unwrap_or_else(|e| panic!("{name}: history must parse: {e}"));
        let now = case["now_ms"].as_u64().unwrap();
        let proof = case["zk_proof"].as_str();
        let (verdict, _) = near_guard_core::evaluate(&policy, &state, &action, now, proof);

        let expected = &case["expect"];
        assert_eq!(
            verdict.approved,
            expected["approved"].as_bool().unwrap(),
            "{name}: approval mismatch"
        );
        let expected_rule = expected["rule"].as_str();
        assert_eq!(
            verdict.rule.map(|r| r.code().to_string()).as_deref(),
            expected_rule,
            "{name}: rule mismatch ({} vs {:?})",
            verdict.rule_code,
            expected_rule
        );
        if verdict.approved {
            assert!(verdict.ipld_cid.starts_with("bafy"), "{name}: missing CID");
        }
    }
}

#[test]
fn range_reports_match_the_shared_vectors() {
    let data = fixtures();
    for name in ["weak", "hardened"] {
        let policy = compile(name);
        let config: FuzzConfig = serde_json::from_value(data["range"]["config"].clone()).unwrap();
        let report = build_report(&policy, &config, 1_700_000_000_000);
        let expected = &data["range"][name];
        assert_eq!(
            report.report_hash,
            expected["report_hash"].as_str().unwrap(),
            "{name}: report hash mismatch"
        );
        assert_eq!(
            report.verdict,
            expected["verdict"].as_str().unwrap(),
            "{name}: verdict"
        );
        assert_eq!(
            report.range.observed_max_loss_micro as u64,
            expected["observed_max_loss_micro"].as_u64().unwrap(),
            "{name}: observed loss"
        );
        assert_eq!(
            report.range.approved as u64,
            expected["approved"].as_u64().unwrap(),
            "{name}: approved count"
        );
        assert_eq!(
            report.range.blocked as u64,
            expected["blocked"].as_u64().unwrap(),
            "{name}: blocked count"
        );
        let kinds: Vec<&str> = report
            .range
            .counterexamples
            .iter()
            .map(|c| c.kind.as_str())
            .collect();
        let expected_kinds: Vec<&str> = expected["counterexample_kinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(kinds, expected_kinds, "{name}: counterexample kinds");
    }
}

#[test]
fn every_transfer_amount_is_representable_in_micro_near() {
    // Guards the shared fixtures themselves: a value like 1.2345678 NEAR would
    // round differently in the two implementations and silently desynchronise them.
    let data = fixtures();
    for case in data["transfers"].as_array().unwrap() {
        let action: Action = serde_json::from_value(case["action"].clone()).unwrap();
        let micro = match &action {
            Action::Transfer { amount, .. } => micro_from_near(*amount),
            Action::Swap { amount_in, .. } => micro_from_near(*amount_in),
        };
        assert_eq!(
            micro as f64,
            match &action {
                Action::Transfer { amount, .. } => (amount * 1_000_000.0).round(),
                Action::Swap { amount_in, .. } => (amount_in * 1_000_000.0).round(),
            },
            "fixture amounts must be exact in micro-NEAR"
        );
    }
}
