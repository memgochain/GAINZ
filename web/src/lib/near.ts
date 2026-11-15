import { ANCHORED_REPORTS, NETWORK, type NetworkSnapshot } from "./network";

/**
 * Read-only NEAR RPC client.
 *
 * The MVP needs no keys to show live state: `status`, `gas_price`, `view_account`
 * and `call_function` with `finality: final` are all public. Write operations
 * (paying through the vault, anchoring a report) require the owner or agent key
 * and are intentionally not exposed by this deployment.
 */

const RPC_URL = process.env.NEAR_RPC_URL ?? NETWORK.rpcUrl;
const VAULT = process.env.NEAR_VAULT_ACCOUNT ?? NETWORK.vault;

const TIMEOUT_MS = 9000;

/** NEAR reports this base58 hash for an account with no contract deployed. */
const EMPTY_CODE_HASH = "11111111111111111111111111111111";

async function rpc<T>(method: string, params: unknown): Promise<T> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);
  try {
    const response = await fetch(RPC_URL, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: "fenceline", method, params }),
      signal: controller.signal,
      cache: "no-store",
    });
    const body = (await response.json()) as {
      result?: T;
      error?: { message?: string; data?: unknown };
    };
    if (body.error) {
      throw new Error(body.error.message ?? "near rpc error");
    }
    if (body.result === undefined) {
      throw new Error(`near rpc returned no result for ${method}`);
    }
    return body.result;
  } finally {
    clearTimeout(timer);
  }
}

/**
 * The deployed bytecode hash of the vault account, or `null` when the chain
 * cannot answer. The verification route compares it with a local build.
 */
export async function accountCodeHash(): Promise<string | null> {
  try {
    const account = await rpc<RpcAccount>("query", {
      request_type: "view_account",
      finality: "final",
      account_id: VAULT,
    });
    return account.code_hash && account.code_hash !== EMPTY_CODE_HASH
      ? account.code_hash
      : null;
  } catch {
    return null;
  }
}

/** Call a view method on the vault and JSON-decode its result. */
export async function viewCall<T>(methodName: string, args: unknown = {}): Promise<T> {
  const result = await rpc<{ result: number[]; logs: string[] }>("query", {
    request_type: "call_function",
    finality: "final",
    account_id: VAULT,
    method_name: methodName,
    args_base64: Buffer.from(JSON.stringify(args)).toString("base64"),
  });
  const text = Buffer.from(result.result).toString("utf8");
  return JSON.parse(text) as T;
}

interface RpcAccount {
  amount: string;
  code_hash: string;
  storage_usage: number;
}

function nearFromYocto(yocto: string | number): string {
  const value = BigInt(yocto);
  const whole = value / 10n ** 24n;
  const fraction = value % 10n ** 24n;
  if (fraction === 0n) return whole.toString();
  const digits = fraction.toString().padStart(24, "0").replace(/0+$/, "");
  return `${whole}.${digits}`;
}

interface VaultConfigView {
  owner_id: string;
  agent_id: string;
  max_tx_amount: string;
  daily_limit: string;
  spent_in_window: string;
  remaining_daily: string;
  window_model: string;
  whitelist: string[];
  policy_commitment: string;
  audit_log_count: number;
  report_count: number;
}

interface ReportAnchorView {
  report_hash: string;
  policy_commitment: string;
  declared_cap: string;
  analytic_bound: string;
  observed_loss: string;
  counterexample_count: number;
  verdict: string;
  anchored_at_ns: string;
  anchored_by: string;
}

interface AuditLogView {
  timestamp: string;
  recipient: string;
  amount: string;
  ipld_cid: string;
  zk_proof_hash: string;
  policy_commitment: string;
}

/** Snapshot cache: a hit is instant, and 15s of staleness is invisible on a
 *  block explorer. Without it every page load paid the full RPC round trip. */
const SNAPSHOT_TTL_MS = 15_000;
let cachedSnapshot: { at: number; value: NetworkSnapshot } | null = null;

/** Every read goes out in a single parallel batch. Four sequential rounds took
 *  ~12s on a cold Vercel function, which is what made the site look dead. */
export async function networkSnapshot(): Promise<NetworkSnapshot> {
  if (cachedSnapshot && Date.now() - cachedSnapshot.at < SNAPSHOT_TTL_MS) {
    return cachedSnapshot.value;
  }

  const started = Date.now();
  const errors: string[] = [];
  const track = async <T,>(label: string, promise: Promise<T>): Promise<T | null> => {
    try {
      return await promise;
    } catch (error) {
      errors.push(`${label}: ${(error as Error).message}`);
      return null;
    }
  };

  const [status, gas, account, config, logs, weakAnchor, hardenedAnchor] = await Promise.all([
    track(
      "status",
      rpc<{ chain_id: string; sync_info: { latest_block_height: number; latest_block_hash: string } }>(
        "status",
        [],
      ),
    ),
    track("gas_price", rpc<{ gas_price: string }>("gas_price", [null])),
    track(
      "view_account",
      rpc<RpcAccount>("query", {
        request_type: "view_account",
        finality: "final",
        account_id: VAULT,
      }),
    ),
    track("get_config", viewCall<VaultConfigView>("get_config")),
    track("get_recent_audit_logs", viewCall<AuditLogView[]>("get_recent_audit_logs", { limit: 5 })),
    track(
      "get_report(weak)",
      viewCall<ReportAnchorView | null>("get_report", {
        report_hash: ANCHORED_REPORTS.weak.reportHash,
      }),
    ),
    track(
      "get_report(hardened)",
      viewCall<ReportAnchorView | null>("get_report", {
        report_hash: ANCHORED_REPORTS.hardened.reportHash,
      }),
    ),
  ]);

  const latencyMs = Date.now() - started;

  const anchors: NetworkSnapshot["anchors"] = [];
  const anchorPairs = [
    ["weak", weakAnchor],
    ["hardened", hardenedAnchor],
  ] as const;
  for (const [key, anchor] of anchorPairs) {
    if (!anchor) continue;
    anchors.push({
      key,
      reportHash: anchor.report_hash,
      verdict: anchor.verdict,
      declaredCapNear: nearFromYocto(anchor.declared_cap),
      analyticBoundNear: nearFromYocto(anchor.analytic_bound),
      observedLossNear: nearFromYocto(anchor.observed_loss),
      counterexamples: anchor.counterexample_count,
      tx: ANCHORED_REPORTS[key].tx,
      anchoredAtNs: anchor.anchored_at_ns,
    });
  }

  const value: NetworkSnapshot = {
    chainId: status?.chain_id ?? "testnet",
    blockHeight: status?.sync_info.latest_block_height ?? 0,
    blockHash: status?.sync_info.latest_block_hash ?? "",
    gasPrice: gas?.gas_price ?? "",
    latencyMs,
    account: {
      accountId: VAULT,
      balanceNear: account ? nearFromYocto(account.amount) : "0",
      storageUsed: account?.storage_usage ?? 0,
      hasContract: !!account && account.code_hash !== EMPTY_CODE_HASH,
    },
    vault: config
      ? {
          ownerId: config.owner_id,
          agentId: config.agent_id,
          perTxCapNear: nearFromYocto(config.max_tx_amount),
          dailyLimitNear: nearFromYocto(config.daily_limit),
          spentInWindowNear: nearFromYocto(config.spent_in_window),
          remainingDailyNear: nearFromYocto(config.remaining_daily),
          windowModel: config.window_model,
          whitelist: config.whitelist,
          policyCommitment: config.policy_commitment,
          auditLogCount: config.audit_log_count,
          reportCount: config.report_count,
        }
      : {
          ownerId: "",
          agentId: "",
          perTxCapNear: "0",
          dailyLimitNear: "0",
          spentInWindowNear: "0",
          remainingDailyNear: "0",
          windowModel: "",
          whitelist: [],
          policyCommitment: "",
          auditLogCount: 0,
          reportCount: 0,
        },
    anchors,
    reports: anchors.map((anchor) => ({
      reportHash: anchor.reportHash,
      verdict: anchor.verdict,
      tx: anchor.tx,
    })),
    recentLogs: (logs ?? []).map((log) => ({
      recipient: log.recipient,
      amountNear: nearFromYocto(log.amount),
      ipldCid: log.ipld_cid,
      zkProofHash: log.zk_proof_hash,
      timestamp: log.timestamp,
    })),
    errors,
    fetchedAt: new Date().toISOString(),
  };

  cachedSnapshot = { at: Date.now(), value };
  return value;
}
