import type { CompiledPolicy, LossBound } from "./types";

/**
 * Closed-form worst case: how much can leave the vault in one 24h window if
 * every single action is legal. Mirrors `near_guard_core::bound::analytic_bound`.
 */
export function analyticBound(policy: CompiledPolicy): LossBound {
  const rateCeiling = policy.rate_limit_per_hour * policy.per_tx_cap_micro * 24;
  const windowMultiplier = policy.window_model === "calendar" ? 2 : 1;

  let spendBound = Math.min(policy.daily_cap_micro, rateCeiling);
  if (spendBound === 0) spendBound = Math.min(policy.per_tx_cap_micro, policy.daily_cap_micro);
  spendBound *= windowMultiplier;

  const slippageLeakage = Math.floor((spendBound * policy.max_slippage_bps) / 10_000);

  const notes = [
    `Enforcement ceiling: min(daily_cap, rate_limit_per_hour x per_tx_cap x 24) = min(${policy.daily_cap_micro}, ${policy.rate_limit_per_hour} x ${policy.per_tx_cap_micro} x 24) micro-NEAR.`,
    policy.window_model === "calendar"
      ? "Calendar-day window: a 24h interval can contain two full day buckets, so the bound doubles."
      : "Rolling 24h window: a sliding window can hold at most daily_cap, so the bound does not double.",
    `Slippage: allowed swaps can burn up to ${policy.max_slippage_bps} bps of the spend bound.`,
    "Assumes the onchain GuardVault enforces exactly this policy and the agent holds no other key.",
    "Assumes allowlisted recipients do not forward funds onward; forwarding is a trust assumption, not something the chain can see.",
    "Excludes asset price movement: this bounds policy leakage, not market risk.",
  ];
  if (policy.max_slippage_bps === 0) {
    notes.push("Swaps disabled by policy: slippage leakage is zero.");
  }

  return {
    declared_daily_cap_micro: policy.daily_cap_micro,
    per_tx_cap_micro: policy.per_tx_cap_micro,
    rate_ceiling_micro: rateCeiling,
    spend_bound_micro: spendBound,
    slippage_leakage_micro: slippageLeakage,
    total_value_at_risk_micro: spendBound + slippageLeakage,
    window_model: policy.window_model,
    day_boundary_multiplier: windowMultiplier,
    notes,
  };
}
