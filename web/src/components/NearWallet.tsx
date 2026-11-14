"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import type { WalletSelector } from "@near-wallet-selector/core";
import {
  WALLET_NETWORKS,
  WALLET_NETWORK_LIST,
  accountBalanceNear,
  explorerAccountOn,
  type WalletNetworkId,
} from "@/lib/wallet";

/**
 * NEAR wallet connection for both wallet networks (testnet + mainnet).
 *
 * The selector is imported lazily so it never runs during server rendering, and
 * a session is bound to one network: switching networks drops it, because a
 * testnet key is not a mainnet key.
 */
export default function NearWallet() {
  const [networkId, setNetworkId] = useState<WalletNetworkId>("testnet");
  const [accountId, setAccountId] = useState<string | null>(null);
  const [balance, setBalance] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  const selectorRef = useRef<WalletSelector | null>(null);
  const subscriptionRef = useRef<{ unsubscribe: () => void } | null>(null);

  const network = WALLET_NETWORKS[networkId];

  const resetSelector = useCallback(() => {
    subscriptionRef.current?.unsubscribe();
    subscriptionRef.current = null;
    selectorRef.current = null;
    setAccountId(null);
    setBalance(null);
  }, []);

  useEffect(() => resetSelector, [resetSelector]);

  useEffect(() => {
    if (!accountId) return;
    let active = true;
    void accountBalanceNear(network, accountId).then((value) => {
      if (active) setBalance(value);
    });
    return () => {
      active = false;
    };
  }, [accountId, network]);

  const switchNetwork = (next: WalletNetworkId) => {
    if (next === networkId) return;
    resetSelector();
    setNote(null);
    setNetworkId(next);
  };

  const connect = async () => {
    setBusy(true);
    setNote(null);
    try {
      const [{ setupWalletSelector }, { setupMyNearWallet }, { setupModal }] = await Promise.all([
        import("@near-wallet-selector/core"),
        import("@near-wallet-selector/my-near-wallet"),
        import("@near-wallet-selector/modal-ui"),
      ]);

      let selector = selectorRef.current;
      if (!selector) {
        // Passing the network id alone uses the selector's built-in RPC and
        // indexer endpoints for that chain — the same public ones we read from.
        selector = await setupWalletSelector({
          network: network.id,
          modules: [setupMyNearWallet()],
        });
        selectorRef.current = selector;
        subscriptionRef.current = selector.store.observable.subscribe((state) => {
          setAccountId(state.accounts[0]?.accountId ?? null);
        });
      }

      if (selector.isSignedIn()) {
        setBusy(false);
        return;
      }

      setupModal(selector, {
        contractId: network.vault ?? undefined,
        theme: "dark",
        description: network.vault
          ? "Sign in to see your account next to the live GuardVault state."
          : "GuardVault is deployed on testnet. Mainnet connects read-only.",
      }).show();
    } catch (error) {
      setNote(error instanceof Error ? error.message : "wallet setup failed");
    } finally {
      setBusy(false);
    }
  };

  const disconnect = async () => {
    const selector = selectorRef.current;
    resetSelector();
    if (!selector) return;
    const wallet = await selector.wallet().catch(() => null);
    await wallet?.signOut().catch(() => undefined);
  };

  return (
    <div className="wallet">
      <div className="wallet-networks" role="group" aria-label="NEAR network">
        {WALLET_NETWORK_LIST.map((option) => (
          <button
            key={option.id}
            type="button"
            className={`wallet-net ${option.id === networkId ? "wallet-net-on" : ""}`}
            onClick={() => switchNetwork(option.id)}
          >
            {option.label}
          </button>
        ))}
      </div>

      {accountId ? (
        <div className="wallet-account">
          <a
            className="chip chip-live"
            href={explorerAccountOn(network, accountId)}
            target="_blank"
            rel="noreferrer"
            title={`${accountId} on ${network.label}`}
          >
            {accountId}
            {balance ? ` · ${balance} NEAR` : ""}
          </a>
          <button type="button" className="wallet-disconnect" onClick={() => void disconnect()}>
            disconnect
          </button>
        </div>
      ) : (
        <button type="button" className="chip chip-live wallet-connect" onClick={() => void connect()}>
          {busy ? "opening…" : "connect wallet"}
        </button>
      )}

      {note ? <span className="wallet-note">{note}</span> : null}
    </div>
  );
}
