//! End-to-end sandbox test: spin up a local NEAR node, deploy `GuardVault`, and
//! try to break it with the exact scenarios the project claims to stop.
//!
//! This lives in its own crate because `near-workspaces` needs a native TLS
//! stack; keeping it out of `guard_vault` means the contract's own unit tests
//! run anywhere, while this suite needs network access on first run (it
//! downloads a sandbox node and compiles the contract to wasm).
//!
//!     cargo test -p guard_vault_sandbox_test -- --nocapture

use serde_json::{json, Value};

/// `NearToken` JSON form is a decimal yoctoNEAR string.
fn yocto(near: u64) -> String {
    format!("{}{}", near, "0".repeat(24))
}

fn ipld_cid(label: &str) -> String {
    // A CIDv1 dag-cbor string is `bafyrei` + 52 base32 chars.
    let body = format!("{label:0<52}");
    format!("bafyrei{}", &body[..52])
}

async fn view(contract: &near_workspaces::Contract, method: &str, args: Value) -> Value {
    contract
        .view(method)
        .args_json(args)
        .await
        .unwrap_or_else(|e| panic!("view {method} failed: {e}"))
        .json()
        .unwrap_or_else(|e| panic!("view {method} returned bad JSON: {e}"))
}

#[tokio::test]
async fn guard_vault_enforces_policy_and_anchors_reports() -> Result<(), Box<dyn std::error::Error>>
{
    let worker = near_workspaces::sandbox().await?;
    let wasm = near_workspaces::compile_project(guard_vault_sandbox_test::contract_dir()).await?;
    let contract = worker.dev_deploy(&wasm).await?;

    let owner = worker.dev_create_account().await?;
    let agent = worker.dev_create_account().await?;
    let api_provider = worker.dev_create_account().await?;
    let attacker = worker.dev_create_account().await?;

    let init = contract
        .call("new")
        .args_json(json!({
            "owner_id": owner.id(),
            "agent_id": agent.id(),
            "max_tx_amount": yocto(5),
            "daily_limit": yocto(10),
            "window_model": "rolling",
        }))
        .transact()
        .await?;
    assert!(init.is_success(), "init failed: {init:?}");

    let deposit = owner
        .call(contract.id(), "deposit")
        .deposit(near_workspaces::types::NearToken::from_near(20))
        .transact()
        .await?;
    assert!(deposit.is_success());

    let whitelisted = owner
        .call(contract.id(), "add_whitelisted_recipient")
        .args_json(json!({ "recipient": api_provider.id() }))
        .transact()
        .await?;
    assert!(whitelisted.is_success());

    // The owner publishes the hash of the policy it believes is enforced.
    let commitment = owner
        .call(contract.id(), "set_policy_commitment")
        .args_json(json!({ "policy_commitment": "a".repeat(64) }))
        .transact()
        .await?;
    assert!(commitment.is_success());

    // --- Scenario 1: legitimate API payment, 2 NEAR -------------------------
    let valid = agent
        .call(contract.id(), "execute_agent_transfer")
        .args_json(json!({
            "recipient": api_provider.id(),
            "amount": yocto(2),
            "ipld_cid": ipld_cid("valid"),
            "zk_proof": vec![1u8, 2, 3, 4, 5, 6, 7, 8],
        }))
        .transact()
        .await?;
    assert!(valid.is_success(), "valid payment must succeed: {valid:?}");

    let logs = view(&contract, "get_recent_audit_logs", json!({ "limit": 5 })).await;
    assert_eq!(logs.as_array().map(|a| a.len()), Some(1));
    assert!(logs[0]["ipld_cid"].as_str().unwrap().starts_with("bafy"));

    // --- Scenario 2: prompt-injection drain to an unknown account -----------
    let drain = agent
        .call(contract.id(), "execute_agent_transfer")
        .args_json(json!({
            "recipient": attacker.id(),
            "amount": yocto(5),
            "ipld_cid": ipld_cid("malicious"),
            "zk_proof": vec![1u8, 2, 3],
        }))
        .transact()
        .await?;
    assert!(
        drain.is_failure(),
        "SECURITY: non-whitelisted recipient must be blocked"
    );

    // --- Scenario 3: over the per-transaction cap ---------------------------
    let over_cap = agent
        .call(contract.id(), "execute_agent_transfer")
        .args_json(json!({
            "recipient": api_provider.id(),
            "amount": yocto(6),
            "ipld_cid": ipld_cid("overlimit"),
            "zk_proof": vec![1u8, 2, 3],
        }))
        .transact()
        .await?;
    assert!(
        over_cap.is_failure(),
        "SECURITY: per-transaction cap must be enforced"
    );

    // --- Scenario 4: split sweep past the daily window ----------------------
    let second = agent
        .call(contract.id(), "execute_agent_transfer")
        .args_json(json!({
            "recipient": api_provider.id(),
            "amount": yocto(5),
            "ipld_cid": ipld_cid("second"),
            "zk_proof": vec![9u8, 9, 9],
        }))
        .transact()
        .await?;
    assert!(second.is_success(), "2 + 5 = 7 NEAR is inside the 10 NEAR cap");

    let third = agent
        .call(contract.id(), "execute_agent_transfer")
        .args_json(json!({
            "recipient": api_provider.id(),
            "amount": yocto(5),
            "ipld_cid": ipld_cid("third"),
            "zk_proof": vec![7u8, 7, 7],
        }))
        .transact()
        .await?;
    assert!(
        third.is_failure(),
        "SECURITY: 7 + 5 = 12 NEAR must exceed the 10 NEAR window cap"
    );

    // --- Scenario 5: the agent cannot make itself the owner -----------------
    let escalation = agent
        .call(contract.id(), "add_whitelisted_recipient")
        .args_json(json!({ "recipient": attacker.id() }))
        .transact()
        .await?;
    assert!(
        escalation.is_failure(),
        "SECURITY: the agent must not be able to edit the allowlist"
    );

    let config = view(&contract, "get_config", json!({})).await;
    assert_eq!(config["audit_log_count"], 2);
    assert_eq!(config["policy_commitment"], "a".repeat(64));
    assert_eq!(config["window_model"], "rolling");
    assert_eq!(config["remaining_daily"], yocto(3));

    // --- Scenario 6: anchor a Fenceline report and read it back -------------
    let report_hash = ipld_cid("report");
    let anchor = owner
        .call(contract.id(), "anchor_report")
        .args_json(json!({
            "report_hash": report_hash,
            "policy_commitment": "a".repeat(64),
            "declared_cap": yocto(10),
            "analytic_bound": yocto(10),
            "observed_loss": yocto(7),
            "counterexample_count": 0,
            "verdict": "bounded",
        }))
        .transact()
        .await?;
    assert!(anchor.is_success(), "owner must be able to anchor: {anchor:?}");

    let stored = view(&contract, "get_report", json!({ "report_hash": report_hash })).await;
    assert_eq!(stored["verdict"], "bounded");
    assert_eq!(stored["counterexample_count"], 0);
    assert_eq!(stored["anchored_by"], owner.id().as_str());

    let duplicate = owner
        .call(contract.id(), "anchor_report")
        .args_json(json!({
            "report_hash": report_hash,
            "policy_commitment": "a".repeat(64),
            "declared_cap": yocto(10),
            "analytic_bound": yocto(10),
            "observed_loss": yocto(7),
            "counterexample_count": 0,
            "verdict": "bounded",
        }))
        .transact()
        .await?;
    assert!(
        duplicate.is_failure(),
        "anchoring the same report twice must fail"
    );

    let agent_anchor = agent
        .call(contract.id(), "anchor_report")
        .args_json(json!({
            "report_hash": ipld_cid("agent-report"),
            "policy_commitment": "a".repeat(64),
            "declared_cap": yocto(10),
            "analytic_bound": yocto(10),
            "observed_loss": yocto(0),
            "counterexample_count": 0,
            "verdict": "bounded",
        }))
        .transact()
        .await?;
    assert!(
        agent_anchor.is_failure(),
        "SECURITY: the agent must not be able to anchor reports"
    );

    Ok(())
}
