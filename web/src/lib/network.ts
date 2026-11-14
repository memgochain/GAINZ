/**
 * The NEAR testnet deployment this MVP talks to.
 *
 * These accounts were created through the official testnet faucet and the
 * `GuardVault` contract is live on them; everything the UI shows on the "Live on
 * NEAR" panel is read from the chain at request time. Kept in one place so the
 * policy, the engine and the explorer links cannot drift apart.
 */
export const NETWORK = {
  networkId: "testnet",
  rpcUrl: "https://rpc.testnet.fastnear.com",
  vault: "fenceline-vault-8t1bk.testnet",
  agent: "fenceline-agent-8t1bk.testnet",
  apiProvider: "fenceline-api-8t1bk.testnet",
  /** Recipient used by the "prompt injection" button; deliberately not allowlisted. */
  attacker: "drain-7.testnet",
  explorer: "https://testnet.nearblocks.io",
  deployTx: "F1oMn9bNWNc74TwQRTr85seZozkeCbdtU253ztN9opbQ",
  initTx: "FLwDCrLVTmVDUtKHunreqX1Ze1o6XL6KC7zKCNTUTEuN",
} as const;

export const explorerAccount = (accountId: string) =>
  `${NETWORK.explorer}/address/${accountId}`;

export const explorerTx = (txHash: string) => `${NETWORK.explorer}/txns/${txHash}`;

/** Reports anchored onchain during the deployment step. */
export const ANCHORED_REPORTS = {
  weak: {
    reportHash: "bafyreiam7if3ugmzvk5hz4ig73gt3akp6jtkb7wvzbswod3jm5n2ejkhwy",
    verdict: "under_bounded",
    tx: "J3CBhfRuZ9jZUPXN6xvdbESH5QxLBkaAFv5ZaGboiwPA",
  },
  hardened: {
    reportHash: "bafyreickps35otu3w6dcegoi5md3zinfvdhxj4v4aooqhbjxfpf67tddcy",
    verdict: "bounded",
    tx: "DSaD1sNiDPJntqeVJ4iMDvYm2WBJ4nBL2YWm9xoXG7M1",
  },
} as const;

export interface NetworkSnapshot {
  chainId: string;
  blockHeight: number;
  blockHash: string;
  gasPrice: string;
  latencyMs: number;
  account: {
    accountId: string;
    balanceNear: string;
    storageUsed: number;
    hasContract: boolean;
  };
  vault: {
    ownerId: string;
    agentId: string;
    perTxCapNear: string;
    dailyLimitNear: string;
    spentInWindowNear: string;
    remainingDailyNear: string;
    windowModel: string;
    whitelist: string[];
    policyCommitment: string;
    auditLogCount: number;
    reportCount: number;
  };
  anchors: Array<{
    key: "weak" | "hardened";
    reportHash: string;
    verdict: string;
    declaredCapNear: string;
    analyticBoundNear: string;
    observedLossNear: string;
    counterexamples: number;
    tx: string;
    anchoredAtNs: string;
  }>;
  reports: Array<{ reportHash: string; verdict: string; tx: string }>;
  recentLogs: Array<{
    recipient: string;
    amountNear: string;
    ipldCid: string;
    zkProofHash: string;
    timestamp: string;
  }>;
  errors: string[];
  fetchedAt: string;
}
