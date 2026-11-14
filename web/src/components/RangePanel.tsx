"use client";

import { useState } from "react";
import RangeChart from "@/components/RangeChart";
import { formatNear } from "@/lib/amount";
import { PRESETS } from "@/lib/presets";
import { ANCHORED_REPORTS, explorerTx } from "@/lib/network";
import type { RangeReport } from "@/lib/types";

interface Props {
  report: RangeReport | null;
  running: boolean;
}

function Metric({
  label,
  value,
  unit,
  sub,
  tone,
}: {
  label: string;
  value: string;
  unit: string;
  sub: string;
  tone: "muted" | "good" | "bad";
}) {
  return (
    <div className={`metric metric-${tone}`}>
      <span className="metric-label">{label}</span>
      <span className="metric-value">
        {value}
        <em>{unit}</em>
      </span>
      <span className="metric-sub">{sub}</span>
    </div>
  );
}

function Share({
  label,
  micro,
  total,
  tone,
}: {
  label: string;
  micro: number;
  total: number;
  tone: string;
}) {
  const width = Math.max(2, Math.min(100, (micro / Math.max(total, 1)) * 100));
  return (
    <div className="share-row">
      <span className="share-label">{label}</span>
      <div className="share-track">
        <div className={`share-fill share-${tone}`} style={{ width: `${width}%` }} />
      </div>
      <span className="share-value">{formatNear(micro)} NEAR</span>
    </div>
  );
}

export default function RangePanel({ report, running }: Props) {
  const [copied, setCopied] = useState(false);
  const [swapped, setSwapped] = useState(false);
  const [mismatch, setMismatch] = useState(false);

  if (!report) {
    return (
      <section className="panel panel-range" aria-label="Range">
        <header className="panel-head">
          <h2>Fenceline range</h2>
          <span className="tag">idle</span>
        </header>
        <div className="skeleton">
          <p>{running ? "Running adversarial sequences…" : "Pick a policy and run the range."}</p>
        </div>
      </section>
    );
  }

  const breach = report.range.observed_max_loss_micro > report.declared_daily_cap_micro;
  const counterexample = report.range.counterexamples[0];
  const anchor = Object.values(ANCHORED_REPORTS).find(
    (entry) => entry.reportHash === report.report_hash,
  );

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(report.report_hash);
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    } catch {
      setCopied(false);
    }
  };

  const swapPolicy = async () => {
    setSwapped(true);
    const swappedPolicy = {
      ...PRESETS.weak.policy,
      window_model: "rolling",
    };
    const res = await fetch("/api/range", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ policy: swappedPolicy, config: { seed: 1337, sequences: 120, horizon_hours: 48, steps_per_sequence: 48 } }),
    });
    const data = await res.json();
    if (res.ok) {
      const originalAnchoredCid = ANCHORED_REPORTS.weak.reportHash;
      const mismatched = data.report.report_hash !== originalAnchoredCid;
      setMismatch(mismatched);
      setSwapped(true);
    } else {
      setSwapped(false);
    }
  };

  return (
    <section className="panel panel-range" aria-label="Range">
      <header className="panel-head">
        <h2>Fenceline range</h2>
        <span className={`tag ${breach ? "tag-bad" : "tag-good"}`}>{report.verdict}</span>
      </header>

      <div className="metric-row">
        <Metric
          label="Declared cap"
          value={formatNear(report.declared_daily_cap_micro)}
          unit="NEAR"
          sub="what the owner wrote in policy.json"
          tone="muted"
        />
        <Metric
          label="Observed max loss"
          value={formatNear(report.range.observed_max_loss_micro)}
          unit="NEAR"
          sub="best legal sequence the range found, per 24h"
          tone={breach ? "bad" : "good"}
        />
        <Metric
          label="Analytic bound"
          value={formatNear(report.bound.total_value_at_risk_micro)}
          unit="NEAR"
          sub="worst case from the policy, incl. slippage"
          tone="good"
        />
      </div>

      <div className="share-block">
        <Share
          label="declared"
          micro={report.declared_daily_cap_micro}
          total={report.bound.total_value_at_risk_micro}
          tone="muted"
        />
        <Share
          label="observed"
          micro={report.range.observed_max_loss_micro}
          total={report.bound.total_value_at_risk_micro}
          tone={breach ? "bad" : "good"}
        />
        <Share
          label="bound"
          micro={report.bound.total_value_at_risk_micro}
          total={report.bound.total_value_at_risk_micro}
          tone="good"
        />
      </div>

      <p className={`verdict-detail ${breach ? "verdict-detail-bad" : ""}`}>
        {report.verdict_detail}
      </p>

      <RangeChart
        timeline={report.range.loss_timeline}
        declaredMicro={report.declared_daily_cap_micro}
        observedMicro={report.range.observed_max_loss_micro}
      />

      {counterexample ? (
        <article className="cex">
          <header className="cex-head">
            <span className="cex-warn">counterexample</span>
            <h3>{counterexample.title}</h3>
            <span className="tag tag-bad">{counterexample.kind}</span>
          </header>
          <p className="cex-text">{counterexample.explanation}</p>
          <div className="cex-table">
            <div className="cex-row cex-row-head">
              <span>t+</span>
              <span>action</span>
              <span>status</span>
              <span>amount</span>
            </div>
            {counterexample.steps.map((step, index) => (
              <div className="cex-row" key={`${step.ts_ms}-${index}`}>
                <span className="mono">
                  {Math.round((step.ts_ms - counterexample.window_start_ms) / 1000)}s
                </span>
                <span className="cex-action">{step.action}</span>
                <span className={step.approved ? "pill pill-ok" : "pill pill-bad"}>
                  {step.approved ? "approved" : step.rule_code}
                </span>
                <span className="mono">{formatNear(step.amount_micro)}</span>
              </div>
            ))}
          </div>
          <p className="cex-fix">
            <strong>Fix:</strong> {counterexample.recommended_fix}
          </p>
        </article>
      ) : (
        <article className="cex cex-clean">
          <header className="cex-head">
            <span className="cex-warn">no counterexample</span>
            <h3>Every legal sequence stayed inside the declared cap</h3>
          </header>
          <p className="cex-text">
            {report.range.sequences_run} sequences and {report.range.actions_simulated} simulated
            actions produced {report.range.approved} approvals and {report.range.blocked} blocks.
            Worst observed 24h loss: {formatNear(report.range.observed_max_loss_micro)} NEAR.
          </p>
        </article>
      )}

      <div className="report-foot">
        <div className="hash-row">
          <span className="field-label">Report CID</span>
          <code>{report.report_hash}</code>
          <button type="button" className="ghost" onClick={copy}>
            {copied ? "copied" : "copy"}
          </button>
        </div>
        {(!swapped) && (
          <button type="button" className="ghost" onClick={swapPolicy}>
            Swap window model
          </button>
        )}
        {anchor ? (
          <p className="anchor-hint anchor-ok">
            <span className="dot" aria-hidden /> Anchored on NEAR testnet ({anchor.verdict}) ·{" "}<a href={explorerTx(anchor.tx)} target="_blank" rel="noreferrer">view the anchor transaction ↗</a>
          </p>
        ) : (
          <p className="anchor-hint">
            This CID is not anchored. Editing the policy or the seed changes it — that is how the
            anchor catches a swapped report. Anchoring needs the owner key, which never ships to the
            browser.
          </p>
        )}
        {mismatch && (
          <p style={{ color: "var(--red)" }}>⚠️ Anchor mismatch: the report CID changed after swapping the window model. The original anchored transaction is <a href={explorerTx(ANCHORED_REPORTS.weak.tx)}>here</a>.</p>
        )}
        <div className="stat-chips">
          <span className="chip chip-quiet">{report.range.sequences_run} sequences</span>
          <span className="chip chip-quiet">{report.range.actions_simulated} actions</span>
          <span className="chip chip-quiet">{report.range.approved} approved</span>
          <span className="chip chip-quiet">{report.range.blocked} blocked</span>
          {Object.entries(report.range.rules_triggered).map(([rule, count]) => (
            <span className="chip chip-rule" key={rule}>
              {rule} × {count}
            </span>
          ))}
        </div>
      </div>
    </section>
  );
}
