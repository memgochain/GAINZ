"use client";

import { useCallback, useEffect, useState } from "react";
import { NETWORK, explorerAccount } from "@/lib/network";

/**
 * Live chain badge for the top bar: real block height and RPC latency from
 * `https://rpc.testnet.near.org`, refreshed on click and every 30s.
 */
export default function NetworkStatus() {
  const [height, setHeight] = useState<number | null>(null);
  const [latency, setLatency] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

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
          : `Block ${height ?? "…"} · RPC ${latency ?? "…"}ms · click to refresh, opens the contract on nearblocks`
      }
    >
      <span className={error ? "dot dot-bad" : "dot"} aria-hidden />
      {error
        ? "rpc down"
        : loading && height === null
          ? "connecting…"
          : `testnet · ${height === null ? "…" : height.toLocaleString("en-US")}`}
    </a>
  );
}
