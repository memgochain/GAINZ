//! The enforcement engine.
//!
//! `evaluate` is the single source of truth for "may this agent do this, right
//! now". The onchain `GuardVault` contract implements the same rules and the
//! TypeScript mirror in `web/src/lib/engine.ts` is byte-for-byte equivalent on
//! the shared fixtures — so the report cannot describe a policy the chain does
//! not actually enforce.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::amount::{day_index, format_near, near_from_micro, MS_PER_DAY, MS_PER_HOUR};
use crate::cid::{audit_cid, policy_commitment, proof_hash};
use crate::policy::{CompiledPolicy, Policy, WindowModel};

/// What the agent is asking for.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Action {
    /// Send tokens to a recipient.
    Transfer {
        recipient: String,
        amount: f64,
        #[serde(default)]
        asset: Option<String>,
        #[serde(default)]
        reason: String,
    },
    /// Swap one allowed asset into another (the NEAR Intents shape: an amount,
    /// a quoted output and a minimum acceptable output).
    Swap {
        token_in: String,
        token_out: String,
        amount_in: f64,
        min_out: f64,
        #[serde(default)]
        quoted_out: f64,
        #[serde(default)]
        reason: String,
    },
}

impl Action {
    pub fn transfer(recipient: &str, amount_micro: u128, asset: &str, reason: &str) -> Self {
        Action::Transfer {
            recipient: recipient.to_string(),
            amount: near_from_micro(amount_micro),
            asset: Some(asset.to_string()),
            reason: reason.to_string(),
        }
    }

    pub fn swap(
        token_in: &str,
        token_out: &str,
        amount_micro: u128,
        slippage_bps: u32,
        reason: &str,
    ) -> Self {
        let quoted = 1_000_000.0_f64;
        let min_out = quoted * (1.0 - slippage_bps as f64 / 10_000.0);
        Action::Swap {
            token_in: token_in.to_string(),
            token_out: token_out.to_string(),
            amount_in: near_from_micro(amount_micro),
            min_out,
            quoted_out: quoted,
            reason: reason.to_string(),
        }
    }

    /// Amount expressed in micro-NEAR (or micro-units of the swapped asset).
    pub fn amount_micro(&self) -> u128 {
        match self {
            Action::Transfer { amount, .. } => crate::amount::micro_from_near(*amount),
            Action::Swap { amount_in, .. } => crate::amount::micro_from_near(*amount_in),
        }
    }

    /// The asset that leaves the vault.
    pub fn asset(&self) -> String {
        match self {
            Action::Transfer { asset, .. } => asset
                .clone()
                .unwrap_or_else(|| "NEAR".to_string())
                .to_uppercase(),
            Action::Swap { token_in, .. } => token_in.to_uppercase(),
        }
    }

    /// Destination of the value: a recipient, or the token being bought.
    pub fn target(&self) -> String {
        match self {
            Action::Transfer { recipient, .. } => recipient.clone(),
            Action::Swap { token_out, .. } => token_out.clone(),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Action::Transfer { .. } => "transfer",
            Action::Swap { .. } => "swap",
        }
    }

    pub fn reason(&self) -> &str {
        match self {
            Action::Transfer { reason, .. } => reason,
            Action::Swap { reason, .. } => reason,
        }
    }

    /// Realised slippage implied by the quote, in basis points.
    pub fn slippage_bps(&self) -> u32 {
        match self {
            Action::Transfer { .. } => 0,
            Action::Swap {
                min_out, quoted_out, ..
            } => {
                if *quoted_out <= 0.0 || *min_out >= *quoted_out {
                    return 0;
                }
                (((*quoted_out - *min_out) / *quoted_out) * 10_000.0).round() as u32
            }
        }
    }

    /// One-line human description used in reports and counterexamples.
    pub fn label(&self) -> String {
        match self {
            Action::Transfer {
                recipient, amount, ..
            } => format!("{} NEAR -> {}", trim_float(*amount), recipient),
            Action::Swap {
                token_in,
                token_out,
                amount_in,
                ..
            } => format!(
                "swap {} {} -> {} @ {} bps",
                trim_float(*amount_in),
                token_in.to_uppercase(),
                token_out.to_uppercase(),
                self.slippage_bps()
            ),
        }
    }
}

fn trim_float(value: f64) -> String {
    let s = format!("{:.6}", value);
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s.is_empty() {
        "0".to_string()
    } else {
        s
    }
}

/// Every way an action can be refused. Codes are stable: reports, tests and the
/// onchain logs all reference them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    NonPositiveAmount,
    SelfTransfer,
    MissingZkProof,
    AssetNotAllowed,
    RecipientNotWhitelisted,
    PerTxCap,
    SlippageExceeded,
    DailyCap,
    RateLimit,
}

impl Rule {
    pub fn code(&self) -> &'static str {
        match self {
            Rule::NonPositiveAmount => "non_positive_amount",
            Rule::SelfTransfer => "self_transfer",
            Rule::MissingZkProof => "missing_zk_proof",
            Rule::AssetNotAllowed => "asset_not_allowed",
            Rule::RecipientNotWhitelisted => "recipient_not_whitelisted",
            Rule::PerTxCap => "per_tx_cap",
            Rule::SlippageExceeded => "slippage_exceeded",
            Rule::DailyCap => "daily_cap",
            Rule::RateLimit => "rate_limit",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Rule::NonPositiveAmount => "Non-positive amount",
            Rule::SelfTransfer => "Transfer to the vault itself",
            Rule::MissingZkProof => "No Noir policy proof",
            Rule::AssetNotAllowed => "Asset not allowed by policy",
            Rule::RecipientNotWhitelisted => "Recipient not on the allowlist",
            Rule::PerTxCap => "Single transaction cap exceeded",
            Rule::SlippageExceeded => "Swap slippage above the policy limit",
            Rule::DailyCap => "Daily spending cap exceeded",
            Rule::RateLimit => "Hourly action rate limit exceeded",
        }
    }
}

/// One approved action, kept for window accounting.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub ts_ms: u64,
    pub amount_micro: u128,
    #[serde(default)]
    pub loss_micro: u128,
}

/// Minimal state the enforcement needs. In production this lives onchain inside
/// `GuardVault`; here it is an in-memory accumulator with the same semantics.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VaultState {
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
}

impl VaultState {
    /// Sum of approved amounts inside the policy's spending window.
    pub fn spent_in_window(&self, now_ms: u64, window: WindowModel) -> u128 {
        let day = day_index(now_ms);
        self.history
            .iter()
            .filter(|e| match window {
                WindowModel::Calendar => day_index(e.ts_ms) == day,
                WindowModel::Rolling => {
                    e.ts_ms <= now_ms && now_ms.saturating_sub(e.ts_ms) < MS_PER_DAY
                }
            })
            .fold(0u128, |acc, e| acc.saturating_add(e.amount_micro))
    }

    /// Approved actions inside the last rolling hour.
    pub fn tx_in_hour(&self, now_ms: u64) -> u32 {
        self.history
            .iter()
            .filter(|e| e.ts_ms <= now_ms && now_ms.saturating_sub(e.ts_ms) < MS_PER_HOUR)
            .count() as u32
    }

    fn push(&self, entry: HistoryEntry) -> VaultState {
        let mut next = self.clone();
        next.history.push(entry);
        next
    }
}

/// The result of one policy evaluation, rich enough to render and to audit.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evaluation {
    pub approved: bool,
    pub rule: Option<Rule>,
    pub rule_code: String,
    pub detail: String,
    pub ts_ms: u64,
    pub kind: String,
    pub action: String,
    pub target: String,
    pub asset: String,
    pub amount_micro: u128,
    pub loss_micro: u128,
    pub spent_in_window_micro: u128,
    pub tx_in_hour: u32,
    pub remaining_daily_micro: u128,
    pub ipld_cid: String,
    pub proof_hash: String,
    pub policy_commitment: String,
}

fn blocked(
    policy: &CompiledPolicy,
    state: &VaultState,
    action: &Action,
    ts_ms: u64,
    rule: Rule,
    detail: String,
    spent: u128,
    commitment: &str,
) -> Evaluation {
    Evaluation {
        approved: false,
        rule: Some(rule),
        rule_code: rule.code().to_string(),
        detail,
        ts_ms,
        kind: action.kind().to_string(),
        action: action.label(),
        target: action.target(),
        asset: action.asset(),
        amount_micro: action.amount_micro(),
        loss_micro: 0,
        spent_in_window_micro: spent,
        tx_in_hour: state.tx_in_hour(ts_ms),
        remaining_daily_micro: policy.daily_cap_micro.saturating_sub(spent),
        ipld_cid: String::new(),
        proof_hash: String::new(),
        policy_commitment: commitment.to_string(),
    }
}

/// Evaluate one action against one policy at one instant.
///
/// Returns the verdict and the next vault state (unchanged when blocked).
pub fn evaluate(
    policy: &CompiledPolicy,
    state: &VaultState,
    action: &Action,
    ts_ms: u64,
    zk_proof: Option<&str>,
) -> (Evaluation, VaultState) {
    let commitment = policy_commitment(policy);
    let amount = action.amount_micro();
    let spent = state.spent_in_window(ts_ms, policy.window_model);

    if amount == 0 {
        return (
            blocked(
                policy,
                state,
                action,
                ts_ms,
                Rule::NonPositiveAmount,
                format!("Amount {} must be positive", format_near(amount)),
                spent,
                &commitment,
            ),
            state.clone(),
        );
    }

    if let Action::Transfer { recipient, .. } = action {
        if recipient == &policy.source.vault_id || recipient == &policy.source.agent_id {
            return (
                blocked(
                    policy,
                    state,
                    action,
                    ts_ms,
                    Rule::SelfTransfer,
                    format!("Recipient {recipient} is the vault or the agent itself"),
                    spent,
                    &commitment,
                ),
                state.clone(),
            );
        }
    }

    if policy.require_zk && zk_proof.map(|p| p.is_empty()).unwrap_or(true) {
        return (
            blocked(
                policy,
                state,
                action,
                ts_ms,
                Rule::MissingZkProof,
                "Policy requires a Noir proof of policy compliance and none was supplied".into(),
                spent,
                &commitment,
            ),
            state.clone(),
        );
    }

    let asset = action.asset();
    if !policy.asset_allowed(&asset) {
        return (
            blocked(
                policy,
                state,
                action,
                ts_ms,
                Rule::AssetNotAllowed,
                format!(
                    "Asset {asset} is not in the allowed list [{}]",
                    policy.assets_csv()
                ),
                spent,
                &commitment,
            ),
            state.clone(),
        );
    }
    if let Action::Swap { token_out, .. } = action {
        if !policy.asset_allowed(token_out) {
            return (
                blocked(
                    policy,
                    state,
                    action,
                    ts_ms,
                    Rule::AssetNotAllowed,
                    format!("Swap target {token_out} is not in the allowed list"),
                    spent,
                    &commitment,
                ),
                state.clone(),
            );
        }
    }

    if let Action::Transfer { recipient, .. } = action {
        if !policy.recipient_allowed(recipient) {
            return (
                blocked(
                    policy,
                    state,
                    action,
                    ts_ms,
                    Rule::RecipientNotWhitelisted,
                    format!(
                        "Recipient {recipient} is not on the allowlist [{}]",
                        policy.allowlist_csv()
                    ),
                    spent,
                    &commitment,
                ),
                state.clone(),
            );
        }
    }

    if amount > policy.per_tx_cap_micro {
        return (
            blocked(
                policy,
                state,
                action,
                ts_ms,
                Rule::PerTxCap,
                format!(
                    "Amount {} NEAR exceeds the per-transaction cap of {} NEAR",
                    format_near(amount),
                    format_near(policy.per_tx_cap_micro)
                ),
                spent,
                &commitment,
            ),
            state.clone(),
        );
    }

    if action.slippage_bps() > policy.max_slippage_bps {
        return (
            blocked(
                policy,
                state,
                action,
                ts_ms,
                Rule::SlippageExceeded,
                format!(
                    "Slippage {} bps exceeds the policy limit of {} bps",
                    action.slippage_bps(),
                    policy.max_slippage_bps
                ),
                spent,
                &commitment,
            ),
            state.clone(),
        );
    }

    if spent.saturating_add(amount) > policy.daily_cap_micro {
        return (
            blocked(
                policy,
                state,
                action,
                ts_ms,
                Rule::DailyCap,
                format!(
                    "{} NEAR would push the {} window to {} NEAR, over the {} NEAR daily cap",
                    format_near(amount),
                    match policy.window_model {
                        WindowModel::Calendar => "calendar-day",
                        WindowModel::Rolling => "rolling 24h",
                    },
                    format_near(spent.saturating_add(amount)),
                    format_near(policy.daily_cap_micro)
                ),
                spent,
                &commitment,
            ),
            state.clone(),
        );
    }

    if state.tx_in_hour(ts_ms) + 1 > policy.rate_limit_per_hour {
        return (
            blocked(
                policy,
                state,
                action,
                ts_ms,
                Rule::RateLimit,
                format!(
                    "{} actions already approved in this hour; policy allows {}",
                    state.tx_in_hour(ts_ms),
                    policy.rate_limit_per_hour
                ),
                spent,
                &commitment,
            ),
            state.clone(),
        );
    }

    // Approved: build the audit record, the policy proof hash and the next state.
    let loss = match action {
        Action::Transfer { .. } => amount,
        Action::Swap { .. } => amount.saturating_mul(action.slippage_bps() as u128) / 10_000,
    };
    let cid = audit_cid(
        &policy.source.vault_id,
        &policy.source.agent_id,
        action.kind(),
        &action.target(),
        &action.asset(),
        amount,
        ts_ms,
        action.reason(),
        &commitment,
    );
    let proof = proof_hash(&commitment, &action.asset(), amount, ts_ms);
    let next_spent = spent.saturating_add(amount);
    let next_state = state.push(HistoryEntry {
        ts_ms,
        amount_micro: amount,
        loss_micro: loss,
    });

    (
        Evaluation {
            approved: true,
            rule: None,
            rule_code: String::new(),
            detail: format!(
                "Approved within {} window: {} of {} NEAR daily cap used",
                match policy.window_model {
                    WindowModel::Calendar => "calendar-day",
                    WindowModel::Rolling => "rolling 24h",
                },
                format_near(next_spent),
                format_near(policy.daily_cap_micro)
            ),
            ts_ms,
            kind: action.kind().to_string(),
            action: action.label(),
            target: action.target(),
            asset: action.asset(),
            amount_micro: amount,
            loss_micro: loss,
            remaining_daily_micro: policy.daily_cap_micro.saturating_sub(next_spent),
            spent_in_window_micro: next_spent,
            tx_in_hour: next_state.tx_in_hour(ts_ms),
            ipld_cid: cid,
            proof_hash: proof,
            policy_commitment: commitment,
        },
        next_state,
    )
}

/// A policy plus the state it governs and everything it has decided so far.
/// The MCP server and the CLI demo both drive the vault through this type.
#[derive(Clone, Debug)]
pub struct GuardedVault {
    pub policy: CompiledPolicy,
    pub state: VaultState,
    pub audit: Vec<Evaluation>,
}

impl GuardedVault {
    pub fn new(policy: Policy) -> Result<Self, String> {
        Ok(Self {
            policy: policy.compile()?,
            state: VaultState::default(),
            audit: Vec::new(),
        })
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        Self::new(Policy::from_json(text)?)
    }

    /// Submit an action. The guard always attaches the Noir proof hash, which is
    /// exactly what an honest agent runtime does before calling the contract.
    pub fn submit(&mut self, action: Action, ts_ms: u64) -> Evaluation {
        let (verdict, next) = evaluate(
            &self.policy,
            &self.state,
            &action,
            ts_ms,
            Some("noir-policy-proof"),
        );
        if verdict.approved {
            self.state = next;
        }
        self.audit.push(verdict.clone());
        verdict
    }

    pub fn remaining_daily(&self, now_ms: u64) -> u128 {
        self.policy
            .daily_cap_micro
            .saturating_sub(self.state.spent_in_window(now_ms, self.policy.window_model))
    }

    /// Counters for the UI / demo output.
    pub fn rule_counts(&self) -> BTreeMap<String, u64> {
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        for verdict in &self.audit {
            if !verdict.approved {
                *counts.entry(verdict.rule_code.clone()).or_insert(0) += 1;
            }
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amount::micro_from_near;

    fn policy() -> CompiledPolicy {
        Policy {
            version: 1,
            owner_id: "owner.testnet".into(),
            vault_id: "vault.testnet".into(),
            agent_id: "agent.testnet".into(),
            per_tx_cap: 5.0,
            daily_cap: 10.0,
            rate_limit_per_hour: 4,
            window_model: WindowModel::Rolling,
            allowlist: vec!["api.testnet".into()],
            allowed_assets: vec!["NEAR".into(), "USDC".into()],
            max_slippage_bps: 100,
            require_zk: true,
        }
        .compile()
        .unwrap()
    }

    #[test]
    fn approves_and_updates_state() {
        let p = policy();
        let state = VaultState::default();
        let action = Action::transfer("api.testnet", micro_from_near(2.0), "NEAR", "data");
        let (verdict, next) = evaluate(&p, &state, &action, 1_000, Some("proof"));
        assert!(verdict.approved);
        assert_eq!(verdict.remaining_daily_micro, micro_from_near(8.0));
        assert!(verdict.ipld_cid.starts_with("b"));
        assert_eq!(next.history.len(), 1);
    }

    #[test]
    fn blocks_every_rule() {
        let p = policy();
        let state = VaultState::default();
        let cases: Vec<(Action, Option<&str>, Rule)> = vec![
            (
                Action::transfer("api.testnet", 0, "NEAR", ""),
                Some("proof"),
                Rule::NonPositiveAmount,
            ),
            (
                Action::transfer("vault.testnet", micro_from_near(1.0), "NEAR", ""),
                Some("proof"),
                Rule::SelfTransfer,
            ),
            (
                Action::transfer("api.testnet", micro_from_near(1.0), "NEAR", ""),
                None,
                Rule::MissingZkProof,
            ),
            (
                Action::transfer("api.testnet", micro_from_near(1.0), "DOGE", ""),
                Some("proof"),
                Rule::AssetNotAllowed,
            ),
            (
                Action::transfer("drain.testnet", micro_from_near(1.0), "NEAR", ""),
                Some("proof"),
                Rule::RecipientNotWhitelisted,
            ),
            (
                Action::transfer("api.testnet", micro_from_near(6.0), "NEAR", ""),
                Some("proof"),
                Rule::PerTxCap,
            ),
            (
                Action::swap("NEAR", "USDC", micro_from_near(4.0), 150, ""),
                Some("proof"),
                Rule::SlippageExceeded,
            ),
        ];
        for (action, proof, expected) in cases {
            let (verdict, _) = evaluate(&p, &state, &action, 1_000, proof);
            assert!(!verdict.approved, "{action:?} should be blocked");
            assert_eq!(verdict.rule, Some(expected), "{action:?}");
        }
    }

    #[test]
    fn rolling_window_survives_a_day_boundary() {
        let p = policy();
        let day = 20_001 * MS_PER_DAY;
        let mut state = VaultState::default();
        let first = Action::transfer("api.testnet", micro_from_near(5.0), "NEAR", "a");
        let (v1, s1) = evaluate(&p, &state, &first, day - 60_000, Some("proof"));
        assert!(v1.approved);
        state = s1;
        let (v2, s2) = evaluate(&p, &state, &first, day - 30_000, Some("proof"));
        assert!(v2.approved);
        state = s2;
        // 60s later, a new calendar day: a rolling policy still remembers.
        let (v3, _) = evaluate(&p, &state, &first, day + 30_000, Some("proof"));
        assert!(!v3.approved);
        assert_eq!(v3.rule, Some(Rule::DailyCap));
    }

    #[test]
    fn calendar_window_forgets_a_day_boundary() {
        let mut p = policy();
        p.source.window_model = WindowModel::Calendar;
        p.window_model = WindowModel::Calendar;
        let day = 20_001 * MS_PER_DAY;
        let mut state = VaultState::default();
        let action = Action::transfer("api.testnet", micro_from_near(5.0), "NEAR", "a");
        for ts in [day - 60_000, day - 30_000] {
            let (v, next) = evaluate(&p, &state, &action, ts, Some("proof"));
            assert!(v.approved);
            state = next;
        }
        let (v, _) = evaluate(&p, &state, &action, day + 30_000, Some("proof"));
        assert!(v.approved, "calendar window refilled at 00:00: this is the bug");
    }
}
