//! Model Context Protocol server for Guard AI NEAR ZK.
//!
//! The server owns the policy and the spending state. An agent connected over
//! stdio (Claude Desktop, Claude Code, Cursor, or any MCP client) can only reach
//! the wallet through these tools, and every tool answer is a policy verdict with
//! an audit CID attached.
//!
//! Transport lives in `main.rs`; everything testable lives here.

use near_guard_core::{
    analytic_bound, format_near, micro_from_near, policy_commitment, Action, FuzzConfig,
    GuardedVault, Policy, RangeReport,
};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub const PROTOCOL_VERSION: &str = "2024-11-05";
pub const SERVER_NAME: &str = "near-agent-guard";
pub const SERVER_VERSION: &str = "0.1.0";

/// Wall clock in milliseconds, the unit the policy engine uses.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The policy-governed vault plus the last range report it produced.
pub struct GuardServer {
    pub vault: GuardedVault,
    pub last_report: Option<RangeReport>,
    pub clock_offset_ms: u64,
}

impl GuardServer {
    pub fn new(policy: Policy) -> Result<Self, String> {
        Ok(Self {
            vault: GuardedVault::new(policy)?,
            last_report: None,
            clock_offset_ms: 0,
        })
    }

    pub fn from_policy_json(text: &str) -> Result<Self, String> {
        Self::new(Policy::from_json(text)?)
    }

    /// Test hook: shift the clock so window-boundary behaviour can be exercised.
    pub fn with_clock_offset(mut self, offset_ms: u64) -> Self {
        self.clock_offset_ms = offset_ms;
        self
    }

    pub fn now(&self) -> u64 {
        now_ms() + self.clock_offset_ms
    }

    // ------------------------------------------------------------- MCP plumbing

    /// Parse one JSON-RPC line and produce the response line, if any.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let request: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(err) => {
                let resp = error_response(None, -32700, &format!("Parse error: {err}"));
                return Some(resp.to_string());
            }
        };
        self.handle(&request).map(|v| v.to_string())
    }

    /// Dispatch a JSON-RPC request. `None` means "notification, do not answer".
    pub fn handle(&mut self, request: &Value) -> Option<Value> {
        let id = request.get("id").cloned();
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        if method.is_empty() {
            return Some(error_response(id, -32600, "Invalid Request: missing method"));
        }
        if id.is_none() {
            // notifications/initialized, notifications/cancelled, ...: no reply.
            return None;
        }

        match method.as_str() {
            "initialize" => Some(ok_response(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
                    "instructions": "Programmable spending firewall for NEAR agent wallets. Route every payment through near_safe_transfer, and run near_run_range before funding an agent."
                }),
            )),
            "ping" => Some(ok_response(id, json!({}))),
            "shutdown" => Some(ok_response(id, Value::Null)),
            "tools/list" => Some(ok_response(id, json!({ "tools": tools() }))),
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                match self.call_tool(&name, &args) {
                    Ok((payload, is_error)) => Some(ok_response(
                        id,
                        json!({
                            "content": [{
                                "type": "text",
                                "text": serde_json::to_string_pretty(&payload).unwrap_or_default()
                            }],
                            "isError": is_error
                        }),
                    )),
                    Err(message) => Some(error_response(id, -32602, &message)),
                }
            }
            other => Some(error_response(
                id,
                -32601,
                &format!("Method not found: {other}"),
            )),
        }
    }

    // ------------------------------------------------------------------- tools

    fn call_tool(&mut self, name: &str, args: &Value) -> Result<(Value, bool), String> {
        match name {
            "near_safe_transfer" => Ok(self.safe_transfer(args)),
            "near_policy_status" => Ok((self.policy_status(), false)),
            "near_run_range" => Ok((self.run_range(args), false)),
            "near_audit_trail" => Ok((self.audit_trail(args), false)),
            "near_report_anchor_payload" => Ok((self.anchor_payload(), false)),
            other => Err(format!("Unknown tool: {other}")),
        }
    }

    fn safe_transfer(&mut self, args: &Value) -> (Value, bool) {
        let kind = args
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("transfer");
        let reason = args
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("unspecified");

        let action = if kind == "swap" {
            let token_in = args
                .get("token_in")
                .and_then(Value::as_str)
                .unwrap_or("NEAR");
            let token_out = args
                .get("token_out")
                .and_then(Value::as_str)
                .unwrap_or("USDC");
            let amount_near = args
                .get("amount_near")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let slippage_bps = args
                .get("slippage_bps")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            Action::swap(
                token_in,
                token_out,
                micro_from_near(amount_near),
                slippage_bps,
                reason,
            )
        } else {
            let recipient = args
                .get("recipient")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let amount_near = args
                .get("amount_near")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let asset = args
                .get("asset")
                .and_then(Value::as_str)
                .unwrap_or("NEAR");
            Action::transfer(
                recipient,
                micro_from_near(amount_near),
                asset,
                reason,
            )
        };

        let verdict = self.vault.submit(action, self.now());
        let payload = json!({
            "approved": verdict.approved,
            "rule": verdict.rule_code,
            "detail": verdict.detail,
            "network": "near-testnet",
            "vault": self.vault.policy.source.vault_id,
            "action": verdict.action,
            "amount_near": format_near(verdict.amount_micro),
            "remaining_daily_near": format_near(verdict.remaining_daily_micro),
            "ipld_cid": verdict.ipld_cid,
            "noir_proof_hash": verdict.proof_hash,
            "policy_commitment": verdict.policy_commitment,
            "note": if verdict.approved {
                "Approved. The transfer is signed through the GuardVault function-call key and the audit CID is anchored onchain."
            } else {
                "Refused by the policy. Do NOT retry with a different recipient, a split amount, or an alternative asset."
            }
        });
        (payload, !verdict.approved)
    }

    fn policy_status(&self) -> Value {
        let policy = &self.vault.policy;
        let bound = analytic_bound(policy);
        json!({
            "vault": policy.source.vault_id,
            "owner": policy.source.owner_id,
            "agent": policy.source.agent_id,
            "per_tx_cap_near": format_near(policy.per_tx_cap_micro),
            "daily_cap_near": format_near(policy.daily_cap_micro),
            "rate_limit_per_hour": policy.rate_limit_per_hour,
            "window_model": policy.window_model.as_str(),
            "allowlist": policy.allowlist.iter().cloned().collect::<Vec<_>>(),
            "allowed_assets": policy.allowed_assets.iter().cloned().collect::<Vec<_>>(),
            "max_slippage_bps": policy.max_slippage_bps,
            "require_zk": policy.require_zk,
            "remaining_daily_near": format_near(self.vault.remaining_daily(self.now())),
            "declared_daily_cap_near": format_near(bound.declared_daily_cap_micro),
            "analytic_bound_near": format_near(bound.total_value_at_risk_micro),
            "bound_notes": bound.notes,
            "policy_commitment": policy_commitment(policy),
            "actions_this_session": self.vault.audit.len(),
            "rules_triggered": self.vault.rule_counts()
        })
    }

    fn run_range(&mut self, args: &Value) -> Value {
        let defaults = FuzzConfig::default();
        let config = FuzzConfig {
            seed: args
                .get("seed")
                .and_then(Value::as_u64)
                .unwrap_or(defaults.seed),
            sequences: args
                .get("sequences")
                .and_then(Value::as_u64)
                .unwrap_or(defaults.sequences as u64) as u32,
            horizon_hours: args
                .get("horizon_hours")
                .and_then(Value::as_u64)
                .unwrap_or(defaults.horizon_hours as u64) as u32,
            steps_per_sequence: args
                .get("steps_per_sequence")
                .and_then(Value::as_u64)
                .unwrap_or(defaults.steps_per_sequence as u64) as u32,
        };
        let report = near_guard_core::report::build_report(&self.vault.policy, &config, self.now());
        let payload = json!({
            "verdict": report.verdict,
            "verdict_detail": report.verdict_detail,
            "declared_daily_cap_near": format_near(report.bound.declared_daily_cap_micro),
            "analytic_bound_near": format_near(report.bound.total_value_at_risk_micro),
            "slippage_leakage_near": format_near(report.bound.slippage_leakage_micro),
            "observed_max_loss_near": format_near(report.range.observed_max_loss_micro),
            "sequences_run": report.range.sequences_run,
            "actions_simulated": report.range.actions_simulated,
            "approved": report.range.approved,
            "blocked": report.range.blocked,
            "counterexamples": report.range.counterexamples.iter().map(|c| json!({
                "kind": c.kind,
                "title": c.title,
                "observed_loss_near": format_near(c.observed_loss_micro),
                "explanation": c.explanation,
                "recommended_fix": c.recommended_fix,
                "approved_steps": c.approved_steps,
                "blocked_steps": c.blocked_steps
            })).collect::<Vec<_>>(),
            "rules_triggered": report.range.rules_triggered,
            "report_hash": report.report_hash,
            "report_cid": report.report_cid,
            "disclaimer": report.disclaimer,
            "next_step": if report.range.counterexamples.is_empty() {
                "Policy bounded its declared cap. Anchor the report hash with near_report_anchor_payload."
            } else {
                "Policy is under-bounded. Show the counterexample to the owner and fix the policy before funding the agent."
            }
        });
        self.last_report = Some(report);
        payload
    }

    fn audit_trail(&self, args: &Value) -> Value {
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(10) as usize;
        let entries: Vec<Value> = self
            .vault
            .audit
            .iter()
            .rev()
            .take(limit)
            .map(|v| {
                json!({
                    "approved": v.approved,
                    "rule": v.rule_code,
                    "action": v.action,
                    "amount_near": format_near(v.amount_micro),
                    "ipld_cid": v.ipld_cid,
                    "ts_ms": v.ts_ms
                })
            })
            .collect();
        json!({
            "entries": entries,
            "total": self.vault.audit.len(),
            "note": "Approved entries carry the IPLD CID that GuardVault anchors onchain."
        })
    }

    fn anchor_payload(&mut self) -> Value {
        if self.last_report.is_none() {
            let config = FuzzConfig::default();
            let report =
                near_guard_core::report::build_report(&self.vault.policy, &config, self.now());
            self.last_report = Some(report);
        }
        let report = self.last_report.as_ref().expect("report was just created");
        let yocto = |micro: u128| micro.saturating_mul(1_000_000_000_000_000_000) / 1_000_000;
        json!({
            "contract": self.vault.policy.source.vault_id,
            "method": "anchor_report",
            "args": {
                "report_hash": report.report_hash,
                "policy_commitment": report.policy_commitment,
                "declared_cap": format!("{} yoctoNEAR", yocto(report.bound.declared_daily_cap_micro)),
                "analytic_bound": format!("{} yoctoNEAR", yocto(report.bound.total_value_at_risk_micro)),
                "observed_loss": format!("{} yoctoNEAR", yocto(report.range.observed_max_loss_micro)),
                "counterexample_count": report.range.counterexamples.len(),
                "verdict": report.verdict
            },
            "note": "Owner-only call. Submit it with the owner key; the agent key cannot anchor reports."
        })
    }
}

fn ok_response(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": result })
}

fn error_response(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": { "code": code, "message": message }
    })
}

/// Tool catalogue advertised to MCP clients.
pub fn tools() -> Value {
    json!([
        {
            "name": "near_safe_transfer",
            "description": "Execute a policy-enforced NEAR transfer or swap. Returns an approve/block verdict with an IPLD audit CID and a Noir policy proof hash. A blocked verdict is final.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["transfer", "swap"], "default": "transfer" },
                    "recipient": { "type": "string", "description": "NEAR account id (transfer only)" },
                    "amount_near": { "type": "number", "description": "Amount in NEAR" },
                    "asset": { "type": "string", "default": "NEAR" },
                    "token_in": { "type": "string", "description": "Swap input asset" },
                    "token_out": { "type": "string", "description": "Swap output asset" },
                    "slippage_bps": { "type": "integer", "description": "Swap slippage in basis points" },
                    "reason": { "type": "string", "description": "Human readable purpose, hashed into the audit CID" }
                },
                "required": ["amount_near", "reason"]
            }
        },
        {
            "name": "near_policy_status",
            "description": "Show the active policy, the remaining daily budget, the analytic worst-case loss bound and the policy commitment hash.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "near_run_range",
            "description": "Run the GAINZ red-team range against the active policy: analytic loss bound, deterministic adversarial search, counterexamples with fixes, and a reproducible report hash.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "seed": { "type": "integer" },
                    "sequences": { "type": "integer" },
                    "horizon_hours": { "type": "integer" },
                    "steps_per_sequence": { "type": "integer" }
                }
            }
        },
        {
            "name": "near_audit_trail",
            "description": "List recent decisions with rule codes and IPLD CIDs, newest first.",
            "inputSchema": {
                "type": "object",
                "properties": { "limit": { "type": "integer", "default": 10 } }
            }
        },
        {
            "name": "near_report_anchor_payload",
            "description": "Produce the owner-only GuardVault.anchor_report call for the latest report, so the loss-bound claim is timestamped on NEAR.",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}
