"use client";

import { useState } from "react";
import RangeChart from "@/components/RangeChart";
import { formatNear } from "@/lib/amount";
import { DEFAULT_CONFIG, PRESETS } from "@/lib/presets";
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
  const [swappedReport, setSwappedReport] = useState<RangeReport | null>(null);
  const [swapFailed, setSwapFailed] = useState(false);

  if (!report) {
    return (
      <section className="panel panel-range" aria-label="Range">
        <header className="panel-head">
          <h2>GAINZ range</h2>
          <span className="tag">idle</span>
        </header>
        <div className="skeleton">
          <p>{running ? "Running adversarial sequences…" : "Pick a policy and run the range."}</p>
        </div>
      </section>
    );
  }

  // After a swap the panel shows the new report; the anchor lookup below then
  // fails, which is exactly the evidence that the policy change is visible.
  const shown = swappedReport ?? report;
  const breach = shown.range.observed_max_loss_micro > shown.declared_daily_cap_micro;
  const counterexample = shown.range.counterexamples[0];
  const anchor = Object.values(ANCHORED_REPORTS).find(
    (entry) => entry.reportHash === shown.report_hash,
  );
  const swapUnanchored = swappedReport !== null && !anchor;

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(shown.report_hash);
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    } catch {
      setCopied(false);
    }
  };

  const swapPolicy = async () => {
    setSwapFailed(false);
    try {
      const response = await fetch("/api/range", {
        method: "POST",
        headers: { "content-type": "application/json" },
        // Same limits, same seed — only the window model changes.
        body: JSON.stringify({ policy: PRESETS.hardened.policy, config: DEFAULT_CONFIG }),
      });
      if (!response.ok) throw new Error("range request failed");
      const payload = await response.json();
      setSwappedReport(payload.report as RangeReport);
    } catch {
      setSwapFailed(true);
    }
  };

  return (
    <section className="panel panel-range" aria-label="Range">
      <header className="panel-head">
        <h2>GAINZ range</h2>
        <span className={`tag ${breach ? "tag-bad" : "tag-good"}`}>{shown.verdict}</span>
      </header>

      <div className="metric-row">
        <Metric
          label="Declared cap"
          value={formatNear(shown.declared_daily_cap_micro)}
          unit="NEAR"
          sub="what the owner wrote in policy.json"
          tone="muted"
        />
        <Metric
          label="Observed max loss"
          value={formatNear(shown.range.observed_max_loss_micro)}
          unit="NEAR"
          sub="best legal sequence the range found, per 24h"
          tone={breach ? "bad" : "good"}
        />
        <Metric
          label="Analytic bound"
          value={formatNear(shown.bound.total_value_at_risk_micro)}
          unit="NEAR"
          sub="worst case from the policy, incl. slippage"
          tone="good"
        />
      </div>

      <div className="share-block">
        <Share
          label="declared"
          micro={shown.declared_daily_cap_micro}
          total={shown.bound.total_value_at_risk_micro}
          tone="muted"
        />
        <Share
          label="observed"
          micro={shown.range.observed_max_loss_micro}
          total={shown.bound.total_value_at_risk_micro}
          tone={breach ? "bad" : "good"}
        />
        <Share
          label="bound"
          micro={shown.bound.total_value_at_risk_micro}
          total={shown.bound.total_value_at_risk_micro}
          tone="good"
        />
      </div>

      <p className={`verdict-detail ${breach ? "verdict-detail-bad" : ""}`}>
        {shown.verdict_detail}
      </p>

      <RangeChart
        timeline={shown.range.loss_timeline}
        declaredMicro={shown.declared_daily_cap_micro}
        observedMicro={shown.range.observed_max_loss_micro}
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
            {shown.range.sequences_run} sequences and {shown.range.actions_simulated} simulated
            actions produced {shown.range.approved} approvals and {shown.range.blocked} blocks.
            Worst observed 24h loss: {formatNear(shown.range.observed_max_loss_micro)} NEAR.
          </p>
        </article>
      )}

      <div className="report-foot">
        <div className="hash-row">
          <span className="field-label">Report CID</span>
          <code>{shown.report_hash}</code>
          <button type="button" className="ghost" onClick={copy}>
            {copied ? "copied" : "copy"}
          </button>
        </div>

        <button
          type="button"
          className="ghost"
          onClick={swappedReport ? () => setSwappedReport(null) : swapPolicy}
        >
          {swappedReport ? "Restore original report" : "Swap window model"}
        </button>
        {swapFailed ? (
          <p className="anchor-hint anchor-bad">Could not run the swapped policy. Try again.</p>
        ) : null}

        {anchor ? (
          <p className="anchor-hint anchor-ok">
            <span className="dot" aria-hidden /> Anchored on NEAR testnet ({anchor.verdict}) ·{" "}
            <a href={explorerTx(anchor.tx)} target="_blank" rel="noreferrer">
              view the anchor transaction ↗
            </a>
          </p>
        ) : (
          <p className="anchor-hint">
            This CID is not anchored. Editing the policy or the seed changes it — that is how the
            anchor catches a swapped report. Anchoring needs the owner key, which never ships to the
            browser.
          </p>
        )}

        {swapUnanchored ? (
          <p className="anchor-hint anchor-bad">
            <span className="dot dot-bad" aria-hidden /> Anchor mismatch: after swapping the window
            model this policy hashes to <code>{shown.report_hash}</code>, which is not the CID
            anchored on NEAR testnet. The original anchor is{" "}
            <a href={explorerTx(ANCHORED_REPORTS.weak.tx)} target="_blank" rel="noreferrer">
              still on chain ↗
            </a>
          </p>
        ) : null}

        <div className="stat-chips">
          <span className="chip chip-quiet">{shown.range.sequences_run} sequences</span>
          <span className="chip chip-quiet">{shown.range.actions_simulated} actions</span>
          <span className="chip chip-quiet">{shown.range.approved} approved</span>
          <span className="chip chip-quiet">{shown.range.blocked} blocked</span>
          {Object.entries(shown.range.rules_triggered).map(([rule, count]) => (
            <span className="chip chip-rule" key={rule}>
              {rule} × {count}
            </span>
          ))}
        </div>
      </div>
    </section>
  );
}
