import { describe, expect, it } from "vitest";
import vectors from "../../../fixtures/policy_vectors.json";
import { analyticBound } from "./bound";
import { EMPTY_STATE, evaluate } from "./engine";
import { compilePolicy, policyCommitment } from "./policy";
import { buildReport } from "./report";
import type { Action, FuzzConfig, Policy, VaultState } from "./types";

const data = vectors as unknown as {
  policies: Record<string, Policy>;
  commitments: Record<string, string>;
  audit_cid_case: { policy: string; ts_ms: number; action: Action; cid: string; proof_hash: string };
  bounds: Record<string, Record<string, number>>;
  transfers: Array<{
    name: string;
    policy: string;
    now_ms: number;
    history: Array<{ ts_ms: number; amount_micro: number }>;
    zk_proof: string | null;
    action: Action;
    expect: { approved: boolean; rule: string | null };
  }>;
  range: {
    config: FuzzConfig;
    weak: Record<string, unknown> & { counterexample_kinds: string[] };
    hardened: Record<string, unknown> & { counterexample_kinds: string[] };
  };
};

const compile = (name: string) => compilePolicy(data.policies[name]);

describe("parity with the Rust engine", () => {
  it("computes identical policy commitments", () => {
    expect(policyCommitment(compile("weak"))).toBe(data.commitments.weak);
    expect(policyCommitment(compile("hardened"))).toBe(data.commitments.hardened);
  });

  it("produces the same IPLD audit CID and proof hash", () => {
    const policy = compile(data.audit_cid_case.policy);
    const { decision } = evaluate(
      policy,
      EMPTY_STATE,
      data.audit_cid_case.action,
      data.audit_cid_case.ts_ms,
      "noir-policy-proof",
    );
    expect(decision.approved).toBe(true);
    expect(decision.ipld_cid).toBe(data.audit_cid_case.cid);
    expect(decision.proof_hash).toBe(data.audit_cid_case.proof_hash);
  });

  it("computes identical loss bounds", () => {
    for (const name of ["weak", "hardened"]) {
      const bound = analyticBound(compile(name));
      expect(bound.declared_daily_cap_micro).toBe(data.bounds[name].declared_daily_cap_micro);
      expect(bound.rate_ceiling_micro).toBe(data.bounds[name].rate_ceiling_micro);
      expect(bound.spend_bound_micro).toBe(data.bounds[name].spend_bound_micro);
      expect(bound.slippage_leakage_micro).toBe(data.bounds[name].slippage_leakage_micro);
      expect(bound.total_value_at_risk_micro).toBe(data.bounds[name].total_value_at_risk_micro);
      expect(bound.day_boundary_multiplier).toBe(data.bounds[name].day_boundary_multiplier);
    }
  });

  it("returns the same verdict and rule for every transfer vector", () => {
    for (const testCase of data.transfers) {
      const policy = compile(testCase.policy);
      const state: VaultState = { history: testCase.history.map((h) => ({ ...h, loss_micro: 0 })) };
      const { decision } = evaluate(policy, state, testCase.action, testCase.now_ms, testCase.zk_proof);
      expect(decision.approved, testCase.name).toBe(testCase.expect.approved);
      expect(decision.rule, testCase.name).toBe(testCase.expect.rule);
    }
  });

  it("produces byte-identical report hashes and counters", () => {
    for (const name of ["weak", "hardened"] as const) {
      const report = buildReport(compile(name), data.range.config, 1_700_000_000_000);
      expect(report.report_hash, name).toBe(data.range[name].report_hash);
      expect(report.verdict, name).toBe(data.range[name].verdict);
      expect(report.range.observed_max_loss_micro, name).toBe(
        data.range[name].observed_max_loss_micro,
      );
      expect(report.range.approved, name).toBe(data.range[name].approved);
      expect(report.range.blocked, name).toBe(data.range[name].blocked);
      expect(report.range.counterexamples.map((c) => c.kind), name).toEqual(
        data.range[name].counterexample_kinds,
      );
    }
  });
});

describe("range guarantees", () => {
  it("never realises more than the analytic bound", () => {
    for (const name of ["weak", "hardened"]) {
      const policy = compile(name);
      const report = buildReport(policy, data.range.config, 1_700_000_000_000);
      expect(report.range.observed_max_loss_micro).toBeLessThanOrEqual(
        report.bound.total_value_at_risk_micro,
      );
      expect(report.range.bound_violations).toBe(0);
    }
  });

  it("finds the day-boundary counterexample for a calendar window and not for a rolling one", () => {
    const weak = buildReport(compile("weak"), data.range.config, 1_700_000_000_000);
    const hardened = buildReport(compile("hardened"), data.range.config, 1_700_000_000_000);
    expect(weak.range.counterexamples[0]?.kind).toBe("window_boundary_double_spend");
    expect(weak.range.observed_max_loss_micro).toBeGreaterThan(weak.declared_daily_cap_micro);
    expect(hardened.range.counterexamples).toHaveLength(0);
    expect(hardened.range.observed_max_loss_micro).toBeLessThanOrEqual(
      hardened.declared_daily_cap_micro,
    );
  });

  it("is deterministic for a fixed seed", () => {
    const config = { ...data.range.config, seed: 4242 };
    const a = buildReport(compile("weak"), config, 1);
    const b = buildReport(compile("weak"), config, 999_999);
    expect(a.report_hash).toBe(b.report_hash);
  });

  it("rejects policies that cannot be bounded", () => {
    const base = { ...data.policies.weak };
    expect(() => compilePolicy({ ...base, daily_cap: 0 })).toThrow(/daily_cap/);
    expect(() => compilePolicy({ ...base, allowlist: [] })).toThrow(/allowlist/);
    expect(() => compilePolicy({ ...base, rate_limit_per_hour: 0 })).toThrow(/rate_limit/);
    expect(() => compilePolicy({ ...base, agent_id: base.owner_id })).toThrow(/agent_id/);
  });
});
