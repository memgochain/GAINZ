"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import type { WalletSelector } from "@near-wallet-selector/core";
import {
  WALLET_NETWORKS,
  WALLET_NETWORK_LIST,
  accountBalanceNear,
  explorerAccountOn,
  type WalletNetwork,
  type WalletNetworkId,
} from "@/lib/wallet";

const NETWORK_KEY = "gainz:wallet-network";

/** MyNearWallet writes `signedAccountId` on connect. */
function signedInTrace(): boolean {
  try {
    return Boolean(window.localStorage.getItem("signedAccountId"));
  } catch {
    return false;
  }
}

/** A testnet account is not a mainnet session: never show one on the other chain. */
function onNetwork(accountId: string, networkId: WalletNetworkId): boolean {
  const isTestnet = accountId.endsWith(".testnet");
  return networkId === "testnet" ? isTestnet : !isTestnet;
}

/**
 * The connector's own `signOut()` does not reliably clear its keys, which made
 * disconnect come back after a reload. Clear them explicitly.
 */
function clearWalletStorage() {
  try {
    const keys: string[] = ["signedAccountId", "near_app_wallet_auth_key", "functionCallKey"];
    for (let i = 0; i < window.localStorage.length; i += 1) {
      const key = window.localStorage.key(i);
      if (key?.startsWith("near-api-js:keystore:")) keys.push(key);
    }
    for (const key of keys) window.localStorage.removeItem(key);
  } catch {
    // storage blocked: the in-memory state is already cleared
  }
}

function storedNetwork(): WalletNetworkId {
  try {
    return window.localStorage.getItem(NETWORK_KEY) === "mainnet" ? "mainnet" : "testnet";
  } catch {
    return "testnet";
  }
}

/**
 * NEAR wallet connection for both networks wallets support (testnet + mainnet).
 *
 * The selector is initialised on mount when there is a session to restore or a
 * redirect to finish — otherwise a user coming back from MyNearWallet would
 * land on a page that still looked signed out. Modules are imported lazily so
 * nothing runs during server rendering.
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

  const ensureSelector = useCallback(async (target: WalletNetwork) => {
    if (selectorRef.current) return selectorRef.current;
    const [{ setupWalletSelector }, { setupMyNearWallet }] = await Promise.all([
      import("@near-wallet-selector/core"),
      import("@near-wallet-selector/my-near-wallet"),
    ]);
    // The network id alone uses the selector's built-in RPC endpoints.
    const selector = await setupWalletSelector({
      network: target.id,
      modules: [setupMyNearWallet()],
    });
    selectorRef.current = selector;
    subscriptionRef.current = selector.store.observable.subscribe((state) => {
      const id = state.accounts[0]?.accountId;
      setAccountId(id && onNetwork(id, target.id) ? id : null);
    });
    const initial = selector.store.getState().accounts[0]?.accountId;
    setAccountId(initial && onNetwork(initial, target.id) ? initial : null);
    return selector;
  }, []);

  useEffect(() => {
    // Always initialise the selector (cheap, lazy modules) and, when the wallet
    // left a trace, ask the module for its accounts. `selector.wallet()` is what
    // instantiates MyNearWallet — which is also where it consumes the redirect
    // parameters — so without it a returning user still looked signed out.
    const restored = storedNetwork();
    setNetworkId(restored);
    void (async () => {
      try {
        const selector = await ensureSelector(WALLET_NETWORKS[restored]);
        if (!signedInTrace()) return;
        const wallet = await selector.wallet("my-near-wallet");
        const account = (await wallet.getAccounts()).find(
          (entry) => entry.accountId && onNetwork(entry.accountId, restored),
        );
        if (account) setAccountId(account.accountId);
      } catch {
        setNote("could not restore the wallet session");
      }
    })();
  }, [ensureSelector]);

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
    try {
      window.localStorage.setItem(NETWORK_KEY, next);
    } catch {
      // storage can be blocked; the switch still works for this page view
    }
  };

  const connect = async () => {
    if (!network.vault) {
      setNote("GuardVault is on testnet — no key is requested on mainnet.");
      return;
    }
    setBusy(true);
    setNote(null);
    try {
      const selector = await ensureSelector(network);
      if (selector.isSignedIn()) {
        setBusy(false);
        return;
      }
      const { setupModal } = await import("@near-wallet-selector/modal-ui");
      setupModal(selector, {
        // Scoped to the vault: the wallet grants a function-call key, never full access.
        contractId: network.vault,
        theme: "dark",
        description: "Sign in to see your account next to the live GuardVault state.",
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
    setNote(null);
    if (selector) {
      const wallet = await selector.wallet().catch(() => null);
      await wallet?.signOut().catch(() => undefined);
    }
    clearWalletStorage();
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
        <button
          type="button"
          className="chip chip-live wallet-connect"
          onClick={() => void connect()}
          disabled={busy}
        >
          {busy ? "opening…" : "connect wallet"}
        </button>
      )}

      {note ? <span className="wallet-note">{note}</span> : null}
    </div>
  );
}
