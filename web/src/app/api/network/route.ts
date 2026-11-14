import { NextResponse } from "next/server";
import { networkSnapshot } from "@/lib/near";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

/** Live read of the deployed GuardVault on NEAR testnet. No keys, no cache. */
export async function GET() {
  try {
    const snapshot = await networkSnapshot();
    return NextResponse.json(snapshot, { headers: { "cache-control": "no-store" } });
  } catch (error) {
    const message = error instanceof Error ? error.message : "network read failed";
    return NextResponse.json({ error: message }, { status: 502 });
  }
}
