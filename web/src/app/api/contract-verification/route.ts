import { createHash } from "crypto";
import { readFileSync } from "fs";
import { NextResponse } from "next/server";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

const VAULT_ID = process.env.NEAR_VAULT_ACCOUNT ?? "fenceline-vault-8t1bk.testnet";
const RPC_URL = process.env.NEAR_RPC_URL ?? "https://rpc.testnet.fastnear.com";

/**
 * Candidate locations of the release wasm, relative to the Next.js app root
 * (`web/`). The committed release artifact comes first because it is what a
 * Vercel deployment actually ships; the cargo output is the local fallback.
 */
const WASM_CANDIDATES = [
  process.env.GUARD_VAULT_WASM,
  "contracts/guard_vault.wasm",
  "../target/wasm32-unknown-unknown/release/guard_vault.wasm",
  "target/wasm32-unknown-unknown/release/guard_vault.wasm",
].filter((candidate): candidate is string => Boolean(candidate));

const BASE58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/** NEAR exposes `code_hash` as base58(sha256(wasm)), not as hex. */
function base58(bytes: Uint8Array): string {
  let value = 0n;
  for (const byte of bytes) value = (value << 8n) | BigInt(byte);
  let encoded = "";
  while (value > 0n) {
    encoded = BASE58[Number(value % 58n)] + encoded;
    value /= 58n;
  }
  for (const byte of bytes) {
    if (byte !== 0) break;
    encoded = "1" + encoded;
  }
  return encoded || "1";
}

async function onChainCodeHash(): Promise<string | null> {
  try {
    const response = await fetch(RPC_URL, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: "gainz",
        method: "query",
        params: { request_type: "view_account", finality: "final", account_id: VAULT_ID },
      }),
    });
    if (!response.ok) return null;
    const payload = (await response.json()) as { result?: { code_hash?: string } };
    return payload.result?.code_hash ?? null;
  } catch {
    return null;
  }
}

function localCodeHash(): string | null {
  for (const candidate of WASM_CANDIDATES) {
    try {
      return base58(createHash("sha256").update(readFileSync(candidate)).digest());
    } catch {
      // artifact not at this path — try the next candidate
    }
  }
  return null;
}

export async function GET() {
  const [codeHash, expectedHash] = [await onChainCodeHash(), localCodeHash()];
  return NextResponse.json({
    verified: codeHash !== null && codeHash === expectedHash,
    codeHash,
    expectedHash,
  });
}
