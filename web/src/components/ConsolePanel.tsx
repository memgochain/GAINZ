"use client";

import { formatNear } from "@/lib/amount";
import type { Decision } from "@/lib/types";

interface Props {
  decisions: Decision[];
  busy: boolean;
  onPay: () => void;
  onSweep: () => void;
  onInject: () => void;
  onReset: () => void;
}

export default function ConsolePanel({ decisions, busy, onPay, onSweep, onInject, onReset }: Props) {
  const remaining = decisions[0]?.remaining_daily_micro;

  return (
    <section className="panel panel-console" aria-label="Enforcement console">
      <header className="panel-head">
        <h2>Enforcement console</h2>
        <span className="tag">guard_vault</span>
      </header>

      <p className="console-note">
        Every click runs the same policy engine the MCP server uses: nine rules, the live allowlist
        and the caps of <code>GuardVault</code>. An approval returns an IPLD audit CID and a Noir
        proof hash; the onchain limits are read from testnet below.
      </p>

      <div className="console-actions">
        <button type="button" className="secondary" onClick={onPay} disabled={busy}>
          Pay 1.5 NEAR invoice
        </button>
        <button type="button" className="secondary" onClick={onSweep} disabled={busy}>
          Split sweep ×3
        </button>
        <button type="button" className="secondary secondary-bad" onClick={onInject} disabled={busy}>
          Prompt injection
        </button>
      </div>

      <div className="console-budget">
        <span className="field-label">Remaining in window</span>
        <span className="console-budget-value">
          {remaining === undefined ? "—" : `${formatNear(remaining)} NEAR`}
        </span>
      </div>

      <div className="decision-list">
        {decisions.length === 0 ? (
          <p className="empty">No actions yet. Try a payment, then try the injection.</p>
        ) : (
          decisions.map((decision, index) => (
            <div className="decision" key={`${decision.ts_ms}-${index}`}>
              <div className="decision-top">
                <span className={decision.approved ? "pill pill-ok" : "pill pill-bad"}>
                  {decision.approved ? "approved" : decision.rule_code}
                </span>
                <span className="decision-action">{decision.action}</span>
              </div>
              {decision.approved ? (
                <div className="decision-meta">
                  <code title={decision.ipld_cid}>{decision.ipld_cid.slice(0, 26)}…</code>
                  <span>zk {decision.proof_hash.slice(0, 10)}…</span>
                </div>
              ) : (
                <p className="decision-detail">{decision.detail}</p>
              )}
            </div>
          ))
        )}
      </div>

      <button type="button" className="ghost ghost-wide" onClick={onReset}>
        reset session state
      </button>
    </section>
  );
}
