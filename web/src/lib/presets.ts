import { NETWORK } from "./network";
import type { Policy } from "./types";

/**
 * The policy the live testnet vault was initialised with: owner and agent are the
 * real accounts, and the only allowlisted recipient is the API provider account
 * that is whitelisted onchain. Edit the fields below and the report CID changes,
 * which is exactly what the anchor check detects.
 */
export const WEAK_POLICY: Policy = {
  version: 1,
  owner_id: NETWORK.vault,
  vault_id: NETWORK.vault,
  agent_id: NETWORK.agent,
  per_tx_cap: 5,
  daily_cap: 10,
  rate_limit_per_hour: 4,
  window_model: "calendar",
  allowlist: [NETWORK.apiProvider],
  allowed_assets: ["NEAR", "USDC"],
  max_slippage_bps: 100,
  require_zk: true,
};

export const HARDENED_POLICY: Policy = {
  ...WEAK_POLICY,
  window_model: "rolling",
};

export type PresetId = "weak" | "hardened";

export const PRESETS: Record<PresetId, { label: string; blurb: string; policy: Policy }> = {
  weak: {
    label: "Calendar window",
    blurb: "The default everyone ships: a daily cap that resets at 00:00 UTC.",
    policy: WEAK_POLICY,
  },
  hardened: {
    label: "Rolling window",
    blurb: "Same limits, measured over any 24h window instead of a calendar day.",
    policy: HARDENED_POLICY,
  },
};

export const DEFAULT_CONFIG = {
  seed: 1337,
  sequences: 120,
  horizon_hours: 48,
  steps_per_sequence: 48,
};
