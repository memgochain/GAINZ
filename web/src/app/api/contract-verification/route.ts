import { NextResponse } from "next/server";
import { createHash } from "crypto";
import fs from "fs";
import path from "path";

const WASM_PATH =
  "/workspace/chats/b3099967b06c4e3caede1df2/target/wasm32-unknown-unknown/release/guard_vault.wasm";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

async function fetchCodeHash(rpcUrl: string, vaultId: string): Promise<string | null> {
  try {
    const res = await fetch(rpcUrl, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: "fenceline",
        method: "query",
        params: {
          request_type: "view_account",
          finality: "final",
          account_id: vaultId,
        },
      }),
    });
    if (!res.ok) return null;
    const data = (await res.json()) as any;
    // code_hash is a field on the account response
    return data.result?.code_hash ?? null;
  } catch {
    return null;
  }
}

export async function GET() {
  try {
    const rpc = process.env.NEAR_RPC_URL ?? "https://rpc.testnet.near.org";
    const vault = process.env.NEAR_VAULT_ACCOUNT ?? "fenceline-vault-8t1bk.testnet";

    const onChainCodeHash = await fetchCodeHash(rpc, vault);

    // Read local wasm and compute SHA256
    let expectedHash = "";
    try {
      const buf = fs.readFileSync(WASM_PATH);
      expectedHash = createHash("sha256").update(buf).digest("hex");
    } catch {
      // wasm not present – we cannot verify
      expectedHash = "";
    }

    const verified = onChainCodeHash !== null && onChainCodeHash === expectedHash;

    return NextResponse.json({
      verified,
      codeHash: onChainCodeHash ?? "—",
      expectedHash: expectedHash ?? "—",
    });
  } catch (err: any) {
    return NextResponse.json({ error: err.message }, { status: 500 });
  }
}