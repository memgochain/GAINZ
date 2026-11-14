export type WindowModel = "calendar" | "rolling";

/** The file an owner writes by hand. Mirrors `Policy` in near_guard_core. */
export interface Policy {
  version: number;
  owner_id: string;
  vault_id: string;
  agent_id: string;
  per_tx_cap: number;
  daily_cap: number;
  rate_limit_per_hour: number;
  window_model: WindowModel;
  allowlist: string[];
  allowed_assets: string[];
  max_slippage_bps: number;
  require_zk: boolean;
}

/** A validated policy with integer limits. Mirrors `CompiledPolicy`. */
export interface CompiledPolicy {
  source: Policy;
  per_tx_cap_micro: number;
  daily_cap_micro: number;
  rate_limit_per_hour: number;
  window_model: WindowModel;
  allowlist: string[];
  allowed_assets: string[];
  max_slippage_bps: number;
  require_zk: boolean;
}

export type Action =
  | {
      kind: "transfer";
      recipient: string;
      amount: number;
      asset?: string;
      reason?: string;
    }
  | {
      kind: "swap";
      token_in: string;
      token_out: string;
      amount_in: number;
      min_out: number;
      quoted_out?: number;
      reason?: string;
    };

export interface HistoryEntry {
  ts_ms: number;
  amount_micro: number;
  loss_micro: number;
}

export interface VaultState {
  history: HistoryEntry[];
}

export type Rule =
  | "non_positive_amount"
  | "self_transfer"
  | "missing_zk_proof"
  | "asset_not_allowed"
  | "recipient_not_whitelisted"
  | "per_tx_cap"
  | "slippage_exceeded"
  | "daily_cap"
  | "rate_limit";

/** The result of one policy evaluation. Mirrors `Evaluation`. */
export interface Decision {
  approved: boolean;
  rule: Rule | null;
  rule_code: string;
  detail: string;
  ts_ms: number;
  kind: string;
  action: string;
  target: string;
  asset: string;
  amount_micro: number;
  loss_micro: number;
  spent_in_window_micro: number;
  tx_in_hour: number;
  remaining_daily_micro: number;
  ipld_cid: string;
  proof_hash: string;
  policy_commitment: string;
  /** Present after a swap: the slippage the quote implies. */
  slippage_bps?: number;
}

export interface LossBound {
  declared_daily_cap_micro: number;
  per_tx_cap_micro: number;
  rate_ceiling_micro: number;
  spend_bound_micro: number;
  slippage_leakage_micro: number;
  total_value_at_risk_micro: number;
  window_model: WindowModel;
  day_boundary_multiplier: number;
  notes: string[];
}

export interface CexStep {
  ts_ms: number;
  action: string;
  approved: boolean;
  rule_code: string;
  amount_micro: number;
  loss_micro: number;
  ipld_cid: string;
}

export interface Counterexample {
  kind: string;
  title: string;
  declared_cap_micro: number;
  observed_loss_micro: number;
  window_start_ms: number;
  window_end_ms: number;
  approved_steps: number;
  blocked_steps: number;
  steps: CexStep[];
  explanation: string;
  recommended_fix: string;
}

export interface TimelineBucket {
  hour: number;
  loss_micro: number;
  approved_in_bucket: number;
  blocked_in_bucket: number;
}

export interface FuzzConfig {
  seed: number;
  sequences: number;
  horizon_hours: number;
  steps_per_sequence: number;
}

export interface RangeResult {
  config: FuzzConfig;
  sequences_run: number;
  actions_simulated: number;
  approved: number;
  blocked: number;
  observed_max_loss_micro: number;
  observed_window_start_ms: number;
  observed_window_end_ms: number;
  counterexamples: Counterexample[];
  bound_violations: number;
  rules_triggered: Record<string, number>;
  strategy_hits: Record<string, number>;
  loss_timeline: TimelineBucket[];
}

export interface RangeReport {
  schema: string;
  generated_at_ms: number;
  policy_id: string;
  policy_commitment: string;
  report_hash: string;
  report_cid: string;
  declared_daily_cap_micro: number;
  bound: LossBound;
  range: RangeResult;
  verdict: "bounded" | "under_bounded" | "bound_violation";
  verdict_detail: string;
  disclaimer: string;
}
