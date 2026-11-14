import { formatNear, microFromNear, MS_PER_DAY, MS_PER_HOUR } from "./amount";
import { auditCid, proofHash } from "./cid";
import { policyCommitment } from "./policy";
import type { Action, CompiledPolicy, Decision, Rule, VaultState, WindowModel } from "./types";

const RULE_TITLES: Record<Rule, string> = {
  non_positive_amount: "Non-positive amount",
  self_transfer: "Transfer to the vault itself",
  missing_zk_proof: "No Noir policy proof",
  asset_not_allowed: "Asset not allowed by policy",
  recipient_not_whitelisted: "Recipient not on the allowlist",
  per_tx_cap: "Single transaction cap exceeded",
  slippage_exceeded: "Swap slippage above the policy limit",
  daily_cap: "Daily spending cap exceeded",
  rate_limit: "Hourly action rate limit exceeded",
};

export function ruleTitle(rule: Rule): string {
  return RULE_TITLES[rule];
}

function trimFloat(value: number): string {
  return String(Number(value.toFixed(6)));
}

export function actionAmountMicro(action: Action): number {
  return action.kind === "transfer" ? microFromNear(action.amount) : microFromNear(action.amount_in);
}

export function actionAsset(action: Action): string {
  return (action.kind === "transfer" ? (action.asset ?? "NEAR") : action.token_in).toUpperCase();
}

export function actionTarget(action: Action): string {
  return action.kind === "transfer" ? action.recipient : action.token_out;
}

export function actionReason(action: Action): string {
  return action.reason ?? "";
}

export function actionSlippageBps(action: Action): number {
  if (action.kind !== "swap") return 0;
  const quoted = action.quoted_out ?? 0;
  if (quoted <= 0 || action.min_out >= quoted) return 0;
  return Math.round(((quoted - action.min_out) / quoted) * 10_000);
}

export function actionLabel(action: Action): string {
  if (action.kind === "transfer") {
    return `${trimFloat(action.amount)} NEAR -> ${action.recipient}`;
  }
  return `swap ${trimFloat(action.amount_in)} ${action.token_in.toUpperCase()} -> ${action.token_out.toUpperCase()} @ ${actionSlippageBps(action)} bps`;
}

export function spentInWindow(state: VaultState, nowMs: number, window: WindowModel): number {
  const day = Math.floor(nowMs / MS_PER_DAY);
  return state.history
    .filter((entry) =>
      window === "calendar"
        ? Math.floor(entry.ts_ms / MS_PER_DAY) === day
        : entry.ts_ms <= nowMs && nowMs - entry.ts_ms < MS_PER_DAY,
    )
    .reduce((acc, entry) => acc + entry.amount_micro, 0);
}

export function txInHour(state: VaultState, nowMs: number): number {
  return state.history.filter((e) => e.ts_ms <= nowMs && nowMs - e.ts_ms < MS_PER_HOUR).length;
}

export const EMPTY_STATE: VaultState = { history: [] };

interface BlockedArgs {
  policy: CompiledPolicy;
  state: VaultState;
  action: Action;
  tsMs: number;
  rule: Rule;
  detail: string;
  spent: number;
  commitment: string;
}

function blocked({
  policy,
  state,
  action,
  tsMs,
  rule,
  detail,
  spent,
  commitment,
}: BlockedArgs): Decision {
  return {
    approved: false,
    rule,
    rule_code: rule,
    detail,
    ts_ms: tsMs,
    kind: action.kind,
    action: actionLabel(action),
    target: actionTarget(action),
    asset: actionAsset(action),
    amount_micro: actionAmountMicro(action),
    loss_micro: 0,
    spent_in_window_micro: spent,
    tx_in_hour: txInHour(state, tsMs),
    remaining_daily_micro: Math.max(0, policy.daily_cap_micro - spent),
    ipld_cid: "",
    proof_hash: "",
    policy_commitment: commitment,
  };
}

/**
 * Evaluate one action. Byte-for-byte the same rule order as
 * `near_guard_core::engine::evaluate`, which is why both implementations agree
 * on every fixture vector.
 */
export function evaluate(
  policy: CompiledPolicy,
  state: VaultState,
  action: Action,
  tsMs: number,
  zkProof: string | null = "noir-policy-proof",
): { decision: Decision; state: VaultState } {
  const commitment = policyCommitment(policy);
  const amount = actionAmountMicro(action);
  const spent = spentInWindow(state, tsMs, policy.window_model);

  if (amount === 0) {
    return {
      decision: blocked({
        policy,
        state,
        action,
        tsMs,
        rule: "non_positive_amount",
        detail: `Amount ${formatNear(amount)} must be positive`,
        spent,
        commitment,
      }),
      state,
    };
  }

  if (
    action.kind === "transfer" &&
    (action.recipient === policy.source.vault_id || action.recipient === policy.source.agent_id)
  ) {
    return {
      decision: blocked({
        policy,
        state,
        action,
        tsMs,
        rule: "self_transfer",
        detail: `Recipient ${action.recipient} is the vault or the agent itself`,
        spent,
        commitment,
      }),
      state,
    };
  }

  if (policy.require_zk && !zkProof) {
    return {
      decision: blocked({
        policy,
        state,
        action,
        tsMs,
        rule: "missing_zk_proof",
        detail:
          "Policy requires a Noir proof of policy compliance and none was supplied",
        spent,
        commitment,
      }),
      state,
    };
  }

  const asset = actionAsset(action);
  if (!policy.allowed_assets.includes(asset)) {
    return {
      decision: blocked({
        policy,
        state,
        action,
        tsMs,
        rule: "asset_not_allowed",
        detail: `Asset ${asset} is not in the allowed list [${assetsCsvSafe(policy)}]`,
        spent,
        commitment,
      }),
      state,
    };
  }
  if (action.kind === "swap" && !policy.allowed_assets.includes(action.token_out.toUpperCase())) {
    return {
      decision: blocked({
        policy,
        state,
        action,
        tsMs,
        rule: "asset_not_allowed",
        detail: `Swap target ${action.token_out} is not in the allowed list`,
        spent,
        commitment,
      }),
      state,
    };
  }

  if (action.kind === "transfer" && !policy.allowlist.includes(action.recipient.trim())) {
    return {
      decision: blocked({
        policy,
        state,
        action,
        tsMs,
        rule: "recipient_not_whitelisted",
        detail: `Recipient ${action.recipient} is not on the allowlist [${policy.allowlist.join(",")}]`,
        spent,
        commitment,
      }),
      state,
    };
  }

  if (amount > policy.per_tx_cap_micro) {
    return {
      decision: blocked({
        policy,
        state,
        action,
        tsMs,
        rule: "per_tx_cap",
        detail: `Amount ${formatNear(amount)} NEAR exceeds the per-transaction cap of ${formatNear(policy.per_tx_cap_micro)} NEAR`,
        spent,
        commitment,
      }),
      state,
    };
  }

  if (actionSlippageBps(action) > policy.max_slippage_bps) {
    return {
      decision: blocked({
        policy,
        state,
        action,
        tsMs,
        rule: "slippage_exceeded",
        detail: `Slippage ${actionSlippageBps(action)} bps exceeds the policy limit of ${policy.max_slippage_bps} bps`,
        spent,
        commitment,
      }),
      state,
    };
  }

  if (spent + amount > policy.daily_cap_micro) {
    return {
      decision: blocked({
        policy,
        state,
        action,
        tsMs,
        rule: "daily_cap",
        detail: `${formatNear(amount)} NEAR would push the ${policy.window_model === "calendar" ? "calendar-day" : "rolling 24h"} window to ${formatNear(spent + amount)} NEAR, over the ${formatNear(policy.daily_cap_micro)} NEAR daily cap`,
        spent,
        commitment,
      }),
      state,
    };
  }

  if (txInHour(state, tsMs) + 1 > policy.rate_limit_per_hour) {
    return {
      decision: blocked({
        policy,
        state,
        action,
        tsMs,
        rule: "rate_limit",
        detail: `${txInHour(state, tsMs)} actions already approved in this hour; policy allows ${policy.rate_limit_per_hour}`,
        spent,
        commitment,
      }),
      state,
    };
  }

  const loss =
    action.kind === "transfer"
      ? amount
      : Math.floor((amount * actionSlippageBps(action)) / 10_000);
  const cid = auditCid(
    policy.source.vault_id,
    policy.source.agent_id,
    action.kind,
    actionTarget(action),
    actionAsset(action),
    amount,
    tsMs,
    actionReason(action),
    commitment,
  );
  const proof = proofHash(commitment, actionAsset(action), amount, tsMs);
  const nextSpent = spent + amount;
  const nextState: VaultState = {
    history: [...state.history, { ts_ms: tsMs, amount_micro: amount, loss_micro: loss }],
  };

  return {
    decision: {
      approved: true,
      rule: null,
      rule_code: "",
      detail: `Approved within ${policy.window_model === "calendar" ? "calendar-day" : "rolling 24h"} window: ${formatNear(nextSpent)} of ${formatNear(policy.daily_cap_micro)} NEAR daily cap used`,
      ts_ms: tsMs,
      kind: action.kind,
      action: actionLabel(action),
      target: actionTarget(action),
      asset: actionAsset(action),
      amount_micro: amount,
      loss_micro: loss,
      spent_in_window_micro: nextSpent,
      tx_in_hour: txInHour(nextState, tsMs),
      remaining_daily_micro: Math.max(0, policy.daily_cap_micro - nextSpent),
      ipld_cid: cid,
      proof_hash: proof,
      policy_commitment: commitment,
      slippage_bps: action.kind === "swap" ? actionSlippageBps(action) : undefined,
    },
    state: nextState,
  };
}

function assetsCsvSafe(policy: CompiledPolicy): string {
  return policy.allowed_assets.join(",");
}
