"use client";

import { useCallback, useEffect, useState } from "react";
import { NETWORK, explorerAccount } from "@/lib/network";

/**
 * Live chain badge for the top bar: real block height and RPC latency from
 * `https://rpc.testnet.fastnear.com`, refreshed on click and every 30s, plus a
 * once-per-load check that the deployed contract matches this repo's wasm.
 */
export default function NetworkStatus() {
  const [height, setHeight] = useState<number | null>(null);
  const [latency, setLatency] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [buildVerified, setBuildVerified] = useState<boolean | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const response = await fetch("/api/network", { cache: "no-store" });
      const payload = await response.json();
      if (!response.ok) throw new Error(payload.error ?? "RPC unreachable");
      setHeight(payload.blockHeight as number);
      setLatency(payload.latencyMs as number);
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "RPC unreachable");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 30_000);
    return () => clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    let active = true;
    fetch("/api/contract-verification", { cache: "no-store" })
      .then((response) => (response.ok ? response.json() : null))
      .then((payload) => {
        if (active) setBuildVerified(payload?.verified === true);
      })
      .catch(() => {
        if (active) setBuildVerified(false);
      });
    return () => {
      active = false;
    };
  }, []);

  return (
    <a
      className={`chip ${error ? "chip-bad" : "chip-live"}`}
      href={explorerAccount(NETWORK.vault)}
      target="_blank"
      rel="noreferrer"
      onClick={(event) => {
        // A click refreshes the number; the link still opens the contract page.
        event.preventDefault();
        void refresh();
      }}
      title={
        error
          ? `NEAR RPC error: ${error}`
          : `Block ${height ?? "…"} · RPC ${latency ?? "…"}ms · build ${
              buildVerified === null ? "checking" : buildVerified ? "verified" : "unknown"
            } · click to refresh, opens the contract on nearblocks`
      }
    >
      <span className={error ? "dot dot-bad" : "dot"} aria-hidden />
      {error
        ? "rpc down"
        : loading && height === null
          ? "connecting…"
          : `testnet · ${height === null ? "…" : height.toLocaleString("en-US")}`}
      {buildVerified === null ? null : (
        <span className={buildVerified ? "build-ok" : "build-unknown"}>
          {` · ${buildVerified ? "verified build" : "unknown build"}`}
        </span>
      )}
    </a>
  );
}
