//! Policy file: the thing an owner writes by hand.
//!
//! Deliberately small and boring. A policy that a human cannot read is a policy
//! a human cannot verify, and the whole product is about verification.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::amount::{micro_from_near, MICRO_PER_NEAR};

/// How the daily spending window is measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowModel {
    /// The budget refills at 00:00 UTC. Cheap to implement onchain, and the
    /// source of the double-spend counterexample this project demonstrates.
    Calendar,
    /// Any 24h sliding window. Costs a short history onchain, cannot be doubled
    /// at a day boundary.
    Rolling,
}

impl Default for WindowModel {
    fn default() -> Self {
        WindowModel::Rolling
    }
}

impl WindowModel {
    pub fn as_str(&self) -> &'static str {
        match self {
            WindowModel::Calendar => "calendar",
            WindowModel::Rolling => "rolling",
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_version() -> u32 {
    1
}

/// Authored policy, deserialized straight from `policy/policy.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    #[serde(default = "default_version")]
    pub version: u32,
    pub owner_id: String,
    pub vault_id: String,
    pub agent_id: String,
    /// Hard cap for a single action, in NEAR.
    pub per_tx_cap: f64,
    /// The number printed on the tin: "the agent can spend at most X per day".
    pub daily_cap: f64,
    /// Maximum number of approved actions in any rolling hour.
    pub rate_limit_per_hour: u32,
    #[serde(default)]
    pub window_model: WindowModel,
    /// Recipients the agent is allowed to pay. An empty allowlist denies
    /// everything, which is the safe failure mode.
    pub allowlist: Vec<String>,
    /// Assets the agent may move or swap into.
    pub allowed_assets: Vec<String>,
    /// Maximum tolerated swap slippage in basis points (100 = 1%).
    #[serde(default)]
    pub max_slippage_bps: u32,
    #[serde(default = "default_true")]
    pub require_zk: bool,
}

/// A policy that passed validation, with integer limits precomputed.
#[derive(Clone, Debug, Serialize)]
pub struct CompiledPolicy {
    pub source: Policy,
    pub per_tx_cap_micro: u128,
    pub daily_cap_micro: u128,
    pub rate_limit_per_hour: u32,
    pub window_model: WindowModel,
    pub allowlist: BTreeSet<String>,
    pub allowed_assets: BTreeSet<String>,
    pub max_slippage_bps: u32,
    pub require_zk: bool,
}

impl Policy {
    /// Parse a policy from JSON text.
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str::<Policy>(text)
            .map_err(|e| format!("policy.json is not valid: {e}"))
    }

    /// Parse and validate in one step.
    pub fn compile_json(text: &str) -> Result<CompiledPolicy, String> {
        Policy::from_json(text)?.compile()
    }

    /// Validate the policy and freeze its numeric limits.
    ///
    /// Every rule here exists because a policy that violates it cannot be
    /// bounded: a zero cap means "unlimited" in far too many implementations,
    /// and an empty allowlist means "anyone".
    pub fn compile(&self) -> Result<CompiledPolicy, String> {
        if self.owner_id.trim().is_empty() {
            return Err("owner_id must be set".into());
        }
        if self.agent_id.trim().is_empty() {
            return Err("agent_id must be set".into());
        }
        if self.agent_id == self.owner_id {
            return Err("agent_id must differ from owner_id: the agent must not be the owner".into());
        }
        if !self.per_tx_cap.is_finite() || self.per_tx_cap <= 0.0 {
            return Err("per_tx_cap must be greater than zero (0 would mean unlimited)".into());
        }
        if !self.daily_cap.is_finite() || self.daily_cap <= 0.0 {
            return Err("daily_cap must be greater than zero (0 would mean unlimited)".into());
        }
        if self.daily_cap < self.per_tx_cap {
            return Err("daily_cap must be at least per_tx_cap".into());
        }
        if self.rate_limit_per_hour == 0 {
            return Err("rate_limit_per_hour must be at least 1".into());
        }
        if self.max_slippage_bps > 10_000 {
            return Err("max_slippage_bps must be <= 10000".into());
        }
        if self.allowlist.is_empty() {
            return Err("allowlist must contain at least one recipient".into());
        }
        if self.allowed_assets.is_empty() {
            return Err("allowed_assets must contain at least one asset".into());
        }

        let per_tx_cap_micro = micro_from_near(self.per_tx_cap);
        let daily_cap_micro = micro_from_near(self.daily_cap);
        if per_tx_cap_micro == 0 || daily_cap_micro == 0 {
            return Err("caps must be at least 1 micro-NEAR".into());
        }
        if daily_cap_micro % MICRO_PER_NEAR == 0 && daily_cap_micro < per_tx_cap_micro {
            return Err("daily_cap must be at least per_tx_cap".into());
        }

        Ok(CompiledPolicy {
            per_tx_cap_micro,
            daily_cap_micro,
            rate_limit_per_hour: self.rate_limit_per_hour,
            window_model: self.window_model,
            allowlist: self.allowlist.iter().map(|a| a.trim().to_string()).collect(),
            allowed_assets: self.allowed_assets.iter().map(|a| a.trim().to_uppercase()).collect(),
            max_slippage_bps: self.max_slippage_bps,
            require_zk: self.require_zk,
            source: self.clone(),
        })
    }
}

impl CompiledPolicy {
    /// Is this recipient payable?
    pub fn recipient_allowed(&self, recipient: &str) -> bool {
        self.allowlist.contains(recipient.trim())
    }

    /// Is this asset movable?
    pub fn asset_allowed(&self, asset: &str) -> bool {
        self.allowed_assets.contains(&asset.trim().to_uppercase())
    }

    /// Sorted allowlist as used by the policy commitment.
    pub fn allowlist_csv(&self) -> String {
        self.allowlist.iter().cloned().collect::<Vec<_>>().join(",")
    }

    /// Sorted asset list as used by the policy commitment.
    pub fn assets_csv(&self) -> String {
        self.allowed_assets.iter().cloned().collect::<Vec<_>>().join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{
        "owner_id": "owner.testnet", "vault_id": "vault.testnet", "agent_id": "agent.testnet",
        "per_tx_cap": 5.0, "daily_cap": 10.0, "rate_limit_per_hour": 4,
        "window_model": "rolling", "allowlist": ["api.testnet"], "allowed_assets": ["near"],
        "max_slippage_bps": 100
    }"#;

    #[test]
    fn compiles_valid_policy() {
        let p = Policy::compile_json(GOOD).expect("policy should compile");
        assert_eq!(p.daily_cap_micro, 10_000_000);
        assert!(p.asset_allowed("NEAR"));
        assert!(p.recipient_allowed("api.testnet"));
        assert!(p.require_zk, "require_zk defaults to true");
    }

    #[test]
    fn rejects_unbounded_policies() {
        for (needle, replacement) in [
            ("\"per_tx_cap\": 5.0", "\"per_tx_cap\": 0"),
            ("\"daily_cap\": 10.0", "\"daily_cap\": 0"),
            ("\"rate_limit_per_hour\": 4", "\"rate_limit_per_hour\": 0"),
            ("[\"api.testnet\"]", "[]"),
            ("\"agent_id\": \"agent.testnet\"", "\"agent_id\": \"owner.testnet\""),
        ] {
            let broken = GOOD.replace(needle, replacement);
            assert!(Policy::compile_json(&broken).is_err(), "should reject {broken}");
        }
    }

    #[test]
    fn rejects_unknown_fields() {
        let broken = GOOD.replace("\"rate_limit_per_hour\"", "\"rate_limit_per_day\"");
        assert!(Policy::compile_json(&broken).is_err());
    }
}
