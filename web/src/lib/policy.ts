import { microFromNear } from "./amount";
import { sha256Hex } from "./cid";
import type { CompiledPolicy, Policy } from "./types";

/**
 * Validate a policy and freeze its limits — the same checks `Policy::compile`
 * performs in Rust, in the same order.
 */
export function compilePolicy(input: Policy): CompiledPolicy {
  const policy: Policy = {
    version: input.version ?? 1,
    owner_id: input.owner_id?.trim() ?? "",
    vault_id: input.vault_id?.trim() ?? "",
    agent_id: input.agent_id?.trim() ?? "",
    per_tx_cap: input.per_tx_cap,
    daily_cap: input.daily_cap,
    rate_limit_per_hour: input.rate_limit_per_hour,
    window_model: input.window_model ?? "rolling",
    allowlist: input.allowlist ?? [],
    allowed_assets: input.allowed_assets ?? [],
    max_slippage_bps: input.max_slippage_bps ?? 0,
    require_zk: input.require_zk ?? true,
  };

  if (!policy.owner_id) throw new Error("owner_id must be set");
  if (!policy.agent_id) throw new Error("agent_id must be set");
  if (policy.agent_id === policy.owner_id)
    throw new Error("agent_id must differ from owner_id: the agent must not be the owner");
  if (!Number.isFinite(policy.per_tx_cap) || policy.per_tx_cap <= 0)
    throw new Error("per_tx_cap must be greater than zero (0 would mean unlimited)");
  if (!Number.isFinite(policy.daily_cap) || policy.daily_cap <= 0)
    throw new Error("daily_cap must be greater than zero (0 would mean unlimited)");
  if (policy.daily_cap < policy.per_tx_cap) throw new Error("daily_cap must be at least per_tx_cap");
  if (!Number.isInteger(policy.rate_limit_per_hour) || policy.rate_limit_per_hour < 1)
    throw new Error("rate_limit_per_hour must be at least 1");
  if (policy.max_slippage_bps < 0 || policy.max_slippage_bps > 10_000)
    throw new Error("max_slippage_bps must be <= 10000");
  if (policy.allowlist.length === 0)
    throw new Error("allowlist must contain at least one recipient");
  if (policy.allowed_assets.length === 0)
    throw new Error("allowed_assets must contain at least one asset");

  const perTxMicro = microFromNear(policy.per_tx_cap);
  const dailyMicro = microFromNear(policy.daily_cap);
  if (perTxMicro === 0 || dailyMicro === 0) throw new Error("caps must be at least 1 micro-NEAR");
  if (perTxMicro > dailyMicro) throw new Error("daily_cap must be at least per_tx_cap");

  const allowlist = [...new Set(policy.allowlist.map((a) => a.trim()))].sort();
  const allowedAssets = [...new Set(policy.allowed_assets.map((a) => a.trim().toUpperCase()))].sort();

  return {
    source: { ...policy, allowlist, allowed_assets: allowedAssets },
    per_tx_cap_micro: perTxMicro,
    daily_cap_micro: dailyMicro,
    rate_limit_per_hour: policy.rate_limit_per_hour,
    window_model: policy.window_model,
    allowlist,
    allowed_assets: allowedAssets,
    max_slippage_bps: policy.max_slippage_bps,
    require_zk: policy.require_zk,
  };
}

export function allowlistCsv(policy: CompiledPolicy): string {
  return policy.allowlist.join(",");
}

export function assetsCsv(policy: CompiledPolicy): string {
  return policy.allowed_assets.join(",");
}

export function canonicalPolicyBytes(policy: CompiledPolicy): string {
  return `near-agent-guard/policy/v1|vault=${policy.source.vault_id}|agent=${policy.source.agent_id}|per_tx_micro=${policy.per_tx_cap_micro}|daily_micro=${policy.daily_cap_micro}|rate_per_hour=${policy.rate_limit_per_hour}|window=${policy.window_model}|slippage_bps=${policy.max_slippage_bps}|zk=${policy.require_zk}|assets=${assetsCsv(policy)}|allowlist=${allowlistCsv(policy)}`;
}

export function policyCommitment(policy: CompiledPolicy): string {
  return sha256Hex(canonicalPolicyBytes(policy));
}
