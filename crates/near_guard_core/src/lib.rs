//! # near_guard_core
//!
//! The shared brain of **GAINZ — Guard AI NEAR ZK**.
//!
//! Two responsibilities that must stay in lock-step:
//!
//! 1. **Enforcement** — compile a `policy.json` into hard limits and evaluate every
//!    agent action against it (`engine`). This mirrors, rule for rule, what the
//!    onchain `GuardVault` contract enforces, so the CLI, the MCP server and the
//!    contract can never disagree about what "allowed" means.
//! 2. **Verification** — compute the analytic worst-case loss for that policy
//!    (`bound`) and try to break it with a deterministic adversarial search
//!    (`fuzz`). This is the part nobody else ships: enforcement is common,
//!    evidence that the policy actually bounds the loss is not.
//!
//! Everything is deterministic. Same policy + same seed => same report hash,
//! which is what makes the report anchorable on NEAR.

pub mod amount;
pub mod bound;
pub mod cid;
pub mod engine;
pub mod fuzz;
pub mod policy;
pub mod report;

pub use amount::{format_near, micro_from_near, MS_PER_DAY, MS_PER_HOUR};
pub use bound::{analytic_bound, LossBound};
pub use cid::{audit_cid, cid_v1_dag_cbor, policy_commitment, sha256_hex};
pub use engine::{evaluate, Action, Evaluation, GuardedVault, Rule, VaultState};
pub use fuzz::{run_range, Counterexample, FuzzConfig, RangeResult};
pub use policy::{CompiledPolicy, Policy, WindowModel};
pub use report::RangeReport;
