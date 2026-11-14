"use client";

import { useCallback, useEffect, useState } from "react";
import {
  ANCHORED_REPORTS,
  NETWORK,
  explorerAccount,
  explorerTx,
  type NetworkSnapshot,
} from "@/lib/network";

interface Props {
  /** Hash of the report currently shown in the range panel. */
  reportHash?: string;
}

const short = (value: string, head = 10, tail = 6) =>
  value.length <= head + tail ? value : `${value.slice(0, head)}…${value.slice(-tail)}`;

/**
 * Everything here comes from the contract that is actually deployed on NEAR
 * testnet: `get_config`, `get_report`, `view_account` and `status`. No fixtures.
 */
export default function LiveOnNear({ reportHash }: Props) {
  const [snapshot, setSnapshot] = useState<NetworkSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const response = await fetch("/api/network", { cache: "no-store" });
      const payload = await response.json();
      if (!response.ok) throw new Error(payload.error ?? "RPC unreachable");
      setSnapshot(payload as NetworkSnapshot);
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "RPC unreachable");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const matches = snapshot
    ? snapshot.anchors.find((anchor) => anchor.reportHash === reportHash)
    : undefined;

  return (
    <section className="panel live" aria-label="Live NEAR testnet state">
      <header className="panel-head">
        <h2>Live on NEAR testnet</h2>
        <div className="live-head-right">
          <span className="tag">{snapshot ? snapshot.chainId : "connecting"}</span>
          <button type="button" className="ghost" onClick={() => void load()} disabled={loading}>
            {loading ? "reading…" : "refresh chain state"}
          </button>
        </div>
      </header>

      {error ? (
        <p className="banner banner-bad" role="alert">
          NEAR RPC error: {error}
        </p>
      ) : null}

      {!snapshot && loading ? <p className="empty">Querying rpc.testnet.fastnear.com…</p> : null}

      {snapshot ? (
        <>
          <div className="live-grid">
            <article className="live-card">
              <h3>GuardVault contract</h3>
              <a
                className="live-value"
                href={explorerAccount(snapshot.account.accountId)}
                target="_blank"
                rel="noreferrer"
              >
                {snapshot.account.accountId} ↗
              </a>
              <span className="live-sub">
                {snapshot.account.hasContract ? "wasm deployed" : "no code"} ·{" "}
                {snapshot.account.balanceNear} NEAR balance
              </span>
            </article>

            <article className="live-card">
              <h3>Enforced onchain</h3>
              <span className="live-value">
                {snapshot.vault.perTxCapNear} / {snapshot.vault.dailyLimitNear} NEAR
              </span>
              <span className="live-sub">
                per-tx cap / {snapshot.vault.windowModel} window · {snapshot.vault.remainingDailyNear}{" "}
                NEAR left in window
              </span>
            </article>

            <article className="live-card">
              <h3>Allowlist</h3>
              <span className="live-value">{snapshot.vault.whitelist.length || 0}</span>
              <span className="live-sub">
                {snapshot.vault.whitelist.length
                  ? snapshot.vault.whitelist.map((entry) => (
                      <code key={entry} className="live-allow">
                        {entry}
                      </code>
                    ))
                  : "no recipients whitelisted"}
              </span>
            </article>

            <article className="live-card">
              <h3>Policy commitment</h3>
              <code className="live-value live-hash">
                {snapshot.vault.policyCommitment ? short(snapshot.vault.policyCommitment, 12, 8) : "—"}
              </code>
              <span className="live-sub">
                sha256 of the canonical policy · {snapshot.vault.auditLogCount} audit entries ·{" "}
                {snapshot.vault.reportCount} anchored reports
              </span>
            </article>
          </div>

          <div className="live-anchors">
            {(Object.keys(ANCHORED_REPORTS) as Array<keyof typeof ANCHORED_REPORTS>).map((key) => {
              const anchor = snapshot.anchors.find((entry) => entry.key === key);
              const meta = ANCHORED_REPORTS[key];
              if (!anchor) {
                return (
                  <div className="live-anchor" key={key}>
                    <span className="pill pill-bad">unread</span>
                    <code>{short(meta.reportHash, 18, 8)}</code>
                    <span className="live-sub">anchor not returned by get_report</span>
                  </div>
                );
              }
              return (
                <div className="live-anchor" key={key}>
                  <span className={`pill ${anchor.verdict === "bounded" ? "pill-ok" : "pill-bad"}`}>
                    {anchor.verdict}
                  </span>
                  <code>
                    {short(anchor.reportHash, 18, 8)} · declared {anchor.declaredCapNear} · bound{" "}
                    {anchor.analyticBoundNear} · observed {anchor.observedLossNear} NEAR ·{" "}
                    {anchor.counterexamples} cex
                  </code>
                  <a href={explorerTx(anchor.tx)} target="_blank" rel="noreferrer">
                    anchor tx ↗
                  </a>
                </div>
              );
            })}
          </div>

          <div className="live-foot">
            <p className={matches ? "anchor-ok" : "live-sub"}>
              {matches ? (
                <>
                  <span className="dot" aria-hidden /> This exact report CID is anchored in{" "}
                  <code>{short(matches.reportHash, 16, 6)}</code> ·{" "}
                  <a href={explorerTx(matches.tx)} target="_blank" rel="noreferrer">
                    verify the transaction ↗
                  </a>
                </>
              ) : (
                <>
                  The report in the panel is not anchored onchain yet. Run the range with the default
                  seed and sequences to reproduce an anchored CID, or anchor a new one from the owner
                  account {short(NETWORK.vault, 12, 8)}.
                </>
              )}
            </p>
            <span className="live-sub">
              block {snapshot.blockHeight.toLocaleString("en-US")} · RPC {snapshot.latencyMs}ms ·
              read at {new Date(snapshot.fetchedAt).toLocaleTimeString("en-US")}
            </span>
          </div>

          {snapshot.errors.length ? (
            <p className="live-sub live-warn">partial read: {snapshot.errors.join(" · ")}</p>
          ) : null}
        </>
      ) : null}
    </section>
  );
}
