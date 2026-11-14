import { NETWORK } from "./network";

/**
 * The networks a NEAR wallet can sign into. NEAR exposes two to wallets:
 * mainnet and testnet. GuardVault is deployed on testnet, so mainnet is a
 * read-only connection here — the UI says so rather than pretending otherwise.
 */
export type WalletNetworkId = "testnet" | "mainnet";

export interface WalletNetwork {
  id: WalletNetworkId;
  label: string;
  nodeUrl: string;
  helperUrl: string;
  explorerUrl: string;
  vault: string | null;
}

export const WALLET_NETWORKS: Record<WalletNetworkId, WalletNetwork> = {
  testnet: {
    id: "testnet",
    label: "testnet",
    nodeUrl: "https://rpc.testnet.near.org",
    helperUrl: "https://helper.testnet.near.org",
    explorerUrl: "https://testnet.nearblocks.io",
    vault: NETWORK.vault,
  },
  mainnet: {
    id: "mainnet",
    label: "mainnet",
    nodeUrl: "https://rpc.mainnet.near.org",
    helperUrl: "https://helper.mainnet.near.org",
    explorerUrl: "https://nearblocks.io",
    vault: null,
  },
};

export const WALLET_NETWORK_LIST: WalletNetwork[] = Object.values(WALLET_NETWORKS);

export const explorerAccountOn = (network: WalletNetwork, accountId: string) =>
  `${network.explorerUrl}/address/${accountId}`;

/** Public `view_account` read — no keys, works on either network. */
export async function accountBalanceNear(
  network: WalletNetwork,
  accountId: string,
): Promise<string | null> {
  try {
    const response = await fetch(network.nodeUrl, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: "gainz",
        method: "query",
        params: { request_type: "view_account", finality: "final", account_id: accountId },
      }),
      cache: "no-store",
    });
    if (!response.ok) return null;
    const body = (await response.json()) as { result?: { amount?: string } };
    if (!body.result?.amount) return null;
    return yoctoToNear(body.result.amount);
  } catch {
    return null;
  }
}

function yoctoToNear(yocto: string): string {
  const value = BigInt(yocto);
  const whole = value / 10n ** 24n;
  const fraction = value % 10n ** 24n;
  if (fraction === 0n) return whole.toString();
  return `${whole}.${fraction.toString().padStart(24, "0").replace(/0+$/, "")}`;
}
