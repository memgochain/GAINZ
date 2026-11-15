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
  /** `null` until the check answers; the two error fields say why it failed. */
  const [build, setBuild] = useState<{
    verified: boolean;
    chainError: string | null;
    artifactError: string | null;
  } | null>(null);

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
    const failed = (message: string) => ({
      verified: false,
      chainError: message,
      artifactError: null,
    });
    fetch("/api/contract-verification", { cache: "no-store" })
      .then((response) => (response.ok ? response.json() : null))
      .then((payload) => {
        if (!active) return;
        if (!payload) {
          setBuild(failed("contract-verification request failed"));
          return;
        }
        setBuild({
          verified: payload.verified === true,
          chainError: payload.chainError ?? null,
          artifactError: payload.artifactError ?? null,
        });
      })
      .catch((cause) => {
        if (active) {
          setBuild(
            failed(cause instanceof Error ? cause.message : "contract-verification unreachable"),
          );
        }
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
        [
          error ? `NEAR RPC error: ${error}` : `Block ${height ?? "…"} · RPC ${latency ?? "…"}ms`,
          build === null
            ? "build: checking"
            : build.chainError
              ? `build: unchecked, chain read failed (${build.chainError})`
              : build.artifactError
                ? `build: unverified, ${build.artifactError}`
                : `build: ${build.verified ? "verified" : "unknown"}`,
          "click to refresh, opens the contract on nearblocks",
        ].join(" · ")
      }
    >
      <span className={error ? "dot dot-bad" : "dot"} aria-hidden />
      {error
        ? "rpc down"
        : loading && height === null
          ? "connecting…"
          : `testnet · ${height === null ? "…" : height.toLocaleString("en-US")}`}
      {build === null ? null : build.chainError ? (
        <span className="build-error" title={`NEAR RPC error: ${build.chainError}`}>
          {" · build unchecked · rpc error"}
        </span>
      ) : build.artifactError ? (
        <span className="build-error" title={build.artifactError}>
          {" · build unverified · artifact missing"}
        </span>
      ) : (
        <span className={build.verified ? "build-ok" : "build-unknown"}>
          {` · ${build.verified ? "verified build" : "unknown build"}`}
        </span>
      )}
    </a>
  );
}
