//! # GuardVault
//!
//! An onchain treasury vault for an autonomous AI agent.
//!
//! The agent gets a **function-call access key** scoped to this contract and
//! nothing else: it can call `execute_agent_transfer`, and every call is checked
//! against hard limits that the agent cannot change, bypass or reset.
//!
//! Enforced onchain, in this order:
//!   1. caller is the registered agent,
//!   2. recipient is allowlisted,
//!   3. amount is positive and within the per-transaction cap,
//!   4. a non-empty Noir policy proof is attached,
//!   5. the spend fits the 24h window (rolling or calendar-day),
//!   6. an IPLD audit CID and the ZK proof hash are recorded forever.
//!
//! The owner can additionally anchor a **Fenceline loss-bound report** hash, so
//! the claim "this policy bounds the loss to X" is timestamped on NEAR and can
//! be re-verified by anyone holding the report.

use near_sdk::store::{IterableMap, IterableSet};
use near_sdk::{env, near, AccountId, NearToken, PanicOnDefault, Promise};

/// One day in nanoseconds, as used by `env::block_timestamp()`.
pub const DAY_IN_NS: u64 = 86_400_000_000_000;

/// How the daily spending window is measured.
#[near(serializers = [json, borsh])]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum WindowModel {
    /// Budget refills at 00:00 UTC. Two buckets fit in one 24h interval.
    Calendar,
    /// Any 24h sliding window. Costs a short spend history, cannot be doubled.
    Rolling,
}

/// A spend that still counts against the window.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Spend {
    pub ts_ns: u64,
    pub amount: NearToken,
}

/// Immutable record of one approved agent action.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct AuditLog {
    pub timestamp: u64,
    pub recipient: AccountId,
    pub amount: NearToken,
    pub ipld_cid: String,
    pub zk_proof_hash: String,
    pub policy_commitment: String,
}

/// Anchored Fenceline report: a timestamped claim about the policy's worst case.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct ReportAnchor {
    pub report_hash: String,
    pub policy_commitment: String,
    pub declared_cap: NearToken,
    pub analytic_bound: NearToken,
    pub observed_loss: NearToken,
    pub counterexample_count: u32,
    pub verdict: String,
    pub anchored_at_ns: u64,
    pub anchored_by: AccountId,
}

/// Public snapshot of the vault configuration.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct VaultConfig {
    pub owner_id: AccountId,
    pub agent_id: AccountId,
    pub max_tx_amount: NearToken,
    pub daily_limit: NearToken,
    pub spent_in_window: NearToken,
    pub remaining_daily: NearToken,
    pub window_model: WindowModel,
    pub whitelist: Vec<AccountId>,
    pub policy_commitment: String,
    pub audit_log_count: u64,
    pub report_count: u64,
}

/// Does a spend at `entry_ts` still count at `now_ts` under `model`?
pub fn within_window(entry_ts: u64, now_ts: u64, model: WindowModel) -> bool {
    match model {
        WindowModel::Rolling => entry_ts <= now_ts && now_ts.saturating_sub(entry_ts) < DAY_IN_NS,
        WindowModel::Calendar => entry_ts / DAY_IN_NS == now_ts / DAY_IN_NS,
    }
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct GuardVault {
    pub owner_id: AccountId,
    pub agent_id: AccountId,
    pub max_tx_amount: NearToken,
    pub daily_limit: NearToken,
    pub window_model: WindowModel,
    pub spends: Vec<Spend>,
    pub whitelist: IterableSet<AccountId>,
    pub audit_logs: IterableMap<u64, AuditLog>,
    pub log_count: u64,
    /// sha256 of the canonical policy file, published instead of the numbers.
    pub policy_commitment: String,
    pub reports: IterableMap<String, ReportAnchor>,
    pub report_count: u64,
}

#[near]
impl GuardVault {
    #[init]
    pub fn new(
        owner_id: AccountId,
        agent_id: AccountId,
        max_tx_amount: NearToken,
        daily_limit: NearToken,
        window_model: Option<WindowModel>,
    ) -> Self {
        assert!(
            max_tx_amount.as_yoctonear() > 0,
            "max_tx_amount must be > 0 (0 would mean unlimited)"
        );
        assert!(
            daily_limit.as_yoctonear() >= max_tx_amount.as_yoctonear(),
            "daily_limit must be >= max_tx_amount"
        );
        assert_ne!(owner_id, agent_id, "owner and agent must be different accounts");

        Self {
            owner_id,
            agent_id,
            max_tx_amount,
            daily_limit,
            // Rolling by default: the safe choice, because a calendar window can
            // be spent twice inside one 24h interval.
            window_model: window_model.unwrap_or(WindowModel::Rolling),
            spends: Vec::new(),
            whitelist: IterableSet::new(b"w"),
            audit_logs: IterableMap::new(b"a"),
            log_count: 0,
            policy_commitment: String::new(),
            reports: IterableMap::new(b"r"),
            report_count: 0,
        }
    }

    // ---------------------------------------------------------------- owner ops

    pub fn add_whitelisted_recipient(&mut self, recipient: AccountId) {
        self.assert_owner();
        self.whitelist.insert(recipient);
    }

    pub fn remove_whitelisted_recipient(&mut self, recipient: AccountId) {
        self.assert_owner();
        self.whitelist.remove(&recipient);
    }

    pub fn set_limits(&mut self, max_tx_amount: NearToken, daily_limit: NearToken) {
        self.assert_owner();
        assert!(max_tx_amount.as_yoctonear() > 0, "max_tx_amount must be > 0");
        assert!(
            daily_limit.as_yoctonear() >= max_tx_amount.as_yoctonear(),
            "daily_limit must be >= max_tx_amount"
        );
        self.max_tx_amount = max_tx_amount;
        self.daily_limit = daily_limit;
    }

    /// Publish the hash of the policy the owner believes is enforced.
    pub fn set_policy_commitment(&mut self, policy_commitment: String) {
        self.assert_owner();
        assert_eq!(
            policy_commitment.len(),
            64,
            "policy_commitment must be a 64 character sha256 hex digest"
        );
        self.policy_commitment = policy_commitment;
    }

    #[payable]
    pub fn deposit(&mut self) {}

    // --------------------------------------------------------- agent execution

    pub fn execute_agent_transfer(
        &mut self,
        recipient: AccountId,
        amount: NearToken,
        ipld_cid: String,
        zk_proof: Vec<u8>,
    ) -> Promise {
        assert_eq!(
            env::predecessor_account_id(),
            self.agent_id,
            "Unauthorized: only the registered AI agent can execute"
        );
        assert!(
            self.whitelist.contains(&recipient),
            "Policy Violation: recipient {} is not whitelisted",
            recipient
        );
        assert!(
            amount.as_yoctonear() > 0,
            "Policy Violation: amount must be positive"
        );
        assert!(
            amount.as_yoctonear() <= self.max_tx_amount.as_yoctonear(),
            "Policy Violation: amount exceeds the per-transaction cap"
        );
        assert!(
            !zk_proof.is_empty(),
            "ZK Verification Failed: empty Noir policy proof"
        );
        assert!(
            ipld_cid.starts_with("bafy"),
            "Audit Failed: ipld_cid must be a CIDv1 (bafy...)"
        );

        let now = env::block_timestamp();
        self.prune_spends(now);
        let spent = self.spent_in_window();
        let new_spent = spent.saturating_add(amount);
        assert!(
            new_spent.as_yoctonear() <= self.daily_limit.as_yoctonear(),
            "Policy Violation: daily spending limit exceeded ({} + {} > {})",
            spent,
            amount,
            self.daily_limit
        );

        let proof_hash = hex::encode(env::sha256(&zk_proof));
        self.spends.push(Spend {
            ts_ns: now,
            amount,
        });

        let log_entry = AuditLog {
            timestamp: now,
            recipient: recipient.clone(),
            amount,
            ipld_cid: ipld_cid.clone(),
            zk_proof_hash: proof_hash.clone(),
            policy_commitment: self.policy_commitment.clone(),
        };
        self.audit_logs.insert(self.log_count, log_entry);
        self.log_count += 1;

        env::log_str(&format!(
            "AGENT_TX_APPROVED | to={} | amount={} | ipld={} | zk={} | spent_window={}",
            recipient, amount, ipld_cid, proof_hash, new_spent
        ));

        Promise::new(recipient).transfer(amount)
    }

    // ------------------------------------------------------------- report anchor

    /// Timestamp a Fenceline loss-bound report hash onchain.
    ///
    /// The contract cannot re-run the range, so it anchors the claim and its
    /// inputs. Anyone with the report file can recompute the hash and compare.
    #[allow(clippy::too_many_arguments)]
    pub fn anchor_report(
        &mut self,
        report_hash: String,
        policy_commitment: String,
        declared_cap: NearToken,
        analytic_bound: NearToken,
        observed_loss: NearToken,
        counterexample_count: u32,
        verdict: String,
    ) -> ReportAnchor {
        self.assert_owner();
        assert!(
            report_hash.starts_with("bafy") && report_hash.len() >= 40,
            "report_hash must be a CIDv1 (bafy...)"
        );
        assert!(
            !self.reports.contains_key(&report_hash),
            "Report already anchored"
        );

        let anchor = ReportAnchor {
            report_hash: report_hash.clone(),
            policy_commitment,
            declared_cap,
            analytic_bound,
            observed_loss,
            counterexample_count,
            verdict,
            anchored_at_ns: env::block_timestamp(),
            anchored_by: env::predecessor_account_id(),
        };
        self.reports.insert(report_hash, anchor.clone());
        self.report_count += 1;

        env::log_str(&format!(
            "REPORT_ANCHORED | hash={} | declared={} | bound={} | observed={} | counterexamples={} | verdict={}",
            anchor.report_hash, declared_cap, analytic_bound, observed_loss, counterexample_count, anchor.verdict
        ));

        anchor
    }

    // -------------------------------------------------------------------- views

    pub fn get_config(&self) -> VaultConfig {
        let now = env::block_timestamp();
        let spent = self.spent_in_window_at(now);
        VaultConfig {
            owner_id: self.owner_id.clone(),
            agent_id: self.agent_id.clone(),
            max_tx_amount: self.max_tx_amount,
            daily_limit: self.daily_limit,
            spent_in_window: spent,
            remaining_daily: self.daily_limit.saturating_sub(spent),
            window_model: self.window_model,
            whitelist: self.whitelist.iter().cloned().collect(),
            policy_commitment: self.policy_commitment.clone(),
            audit_log_count: self.log_count,
            report_count: self.report_count,
        }
    }

    pub fn get_remaining_daily_budget(&self) -> NearToken {
        self.daily_limit
            .saturating_sub(self.spent_in_window_at(env::block_timestamp()))
    }

    pub fn get_log_count(&self) -> u64 {
        self.log_count
    }

    pub fn get_audit_log(&self, index: u64) -> Option<AuditLog> {
        self.audit_logs.get(&index).cloned()
    }

    /// Most recent logs first, capped by `limit`.
    pub fn get_recent_audit_logs(&self, limit: u32) -> Vec<AuditLog> {
        let limit = limit.min(self.log_count as u32) as u64;
        (0..limit)
            .filter_map(|i| {
                let idx = self.log_count.checked_sub(i + 1)?;
                self.audit_logs.get(&idx).cloned()
            })
            .collect()
    }

    pub fn get_report(&self, report_hash: String) -> Option<ReportAnchor> {
        self.reports.get(&report_hash).cloned()
    }

    pub fn get_report_hashes(&self) -> Vec<String> {
        self.reports.keys().cloned().collect()
    }

    // ------------------------------------------------------------------ internal

    fn assert_owner(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner_id,
            "Unauthorized: only the owner can do that"
        );
    }

    fn prune_spends(&mut self, now: u64) {
        let model = self.window_model;
        self.spends.retain(|s| within_window(s.ts_ns, now, model));
    }

    fn spent_in_window(&self) -> NearToken {
        self.spends
            .iter()
            .fold(NearToken::from_yoctonear(0), |acc, s| {
                acc.saturating_add(s.amount)
            })
    }

    fn spent_in_window_at(&self, now: u64) -> NearToken {
        self.spends
            .iter()
            .filter(|s| within_window(s.ts_ns, now, self.window_model))
            .fold(NearToken::from_yoctonear(0), |acc, s| {
                acc.saturating_add(s.amount)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_window_remembers_across_midnight() {
        let day_start = 20_001 * DAY_IN_NS;
        let entry = day_start - 60_000_000_000; // 23:59 of the previous day
        let now = day_start + 60_000_000_000; // 00:01
        assert!(within_window(entry, now, WindowModel::Rolling));
        assert!(!within_window(entry, now, WindowModel::Calendar));
    }

    #[test]
    fn rolling_window_expires_after_a_day() {
        let now = 20_001 * DAY_IN_NS;
        assert!(within_window(now - DAY_IN_NS + 1, now, WindowModel::Rolling));
        assert!(!within_window(now - DAY_IN_NS, now, WindowModel::Rolling));
    }

    #[test]
    fn calendar_window_is_the_documented_weakness() {
        let day_start = 20_001 * DAY_IN_NS;
        assert!(within_window(
            day_start + 1,
            day_start + DAY_IN_NS - 1,
            WindowModel::Calendar
        ));
        assert!(!within_window(
            day_start + 1,
            day_start + DAY_IN_NS,
            WindowModel::Calendar
        ));
    }
}
