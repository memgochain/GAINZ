import { cidV1DagCbor } from "./cid";
import { analyticBound } from "./bound";
import { runRange } from "./fuzz";
import { formatNear } from "./amount";
import { policyCommitment } from "./policy";
import type { CompiledPolicy, FuzzConfig, LossBound, RangeReport, RangeResult } from "./types";

export const DISCLAIMER =
  "This report is an analytic upper bound plus an empirical adversarial search, not a formal proof. 'No counterexample found' does not mean none exists: it means the strategies in this build did not find one within the budget. The bound assumes the onchain GuardVault enforces exactly this policy, that the agent holds no other signing key, and that allowlisted recipients do not forward funds. Asset price risk is out of scope.";

/** Canonical bytes for the report hash — the clock is deliberately excluded. */
export function canonicalReportBytes(
  policy: CompiledPolicy,
  config: FuzzConfig,
  bound: LossBound,
  range: RangeResult,
): string {
  const kinds = range.counterexamples.map((c) => c.kind).join("+");
  return `near-agent-guard/report/v1|commitment=${policyCommitment(policy)}|seed=${config.seed}|sequences=${config.sequences}|horizon_hours=${config.horizon_hours}|steps=${config.steps_per_sequence}|declared=${bound.declared_daily_cap_micro}|bound_total=${bound.total_value_at_risk_micro}|observed=${range.observed_max_loss_micro}|approved=${range.approved}|blocked=${range.blocked}|actions=${range.actions_simulated}|counterexamples=${kinds}|bound_violations=${range.bound_violations}`;
}

export function buildReport(
  policy: CompiledPolicy,
  config: FuzzConfig,
  generatedAtMs: number,
): RangeReport {
  const bound = analyticBound(policy);
  const range = runRange(policy, config);
  const canonical = canonicalReportBytes(policy, range.config, bound, range);
  const reportHash = cidV1DagCbor(canonical);

  const verdict: RangeReport["verdict"] =
    range.bound_violations > 0
      ? "bound_violation"
      : range.counterexamples.length === 0
        ? "bounded"
        : "under_bounded";

  const verdictDetail =
    verdict === "bound_violation"
      ? `INTERNAL: the range realised ${range.observed_max_loss_micro} micro-NEAR of loss, above the computed bound of ${bound.total_value_at_risk_micro} micro-NEAR. The bound calculator is wrong; do not rely on this report.`
      : verdict === "under_bounded"
        ? `The policy advertises a ${formatNear(bound.declared_daily_cap_micro)} NEAR daily cap but the range realised ${formatNear(range.observed_max_loss_micro)} NEAR of loss in a single 24h window. Worst case from the analytic bound: ${formatNear(bound.total_value_at_risk_micro)} NEAR.`
        : `No legal sequence exceeded the declared ${formatNear(bound.declared_daily_cap_micro)} NEAR cap across ${range.sequences_run} sequences and ${range.actions_simulated} simulated actions. Worst case from the analytic bound: ${formatNear(bound.total_value_at_risk_micro)} NEAR.`;

  return {
    schema: "near-agent-guard/range-report/v1",
    generated_at_ms: generatedAtMs,
    policy_id: `${policy.source.vault_id}::${policy.source.agent_id}`,
    policy_commitment: policyCommitment(policy),
    report_hash: reportHash,
    report_cid: reportHash,
    declared_daily_cap_micro: bound.declared_daily_cap_micro,
    bound,
    range,
    verdict,
    verdict_detail: verdictDetail,
    disclaimer: DISCLAIMER,
  };
}
