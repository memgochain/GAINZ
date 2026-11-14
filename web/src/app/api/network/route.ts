import { NextResponse } from "next/server";
import { networkSnapshot } from "@/lib/near";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

/** Live read of the deployed GuardVault on NEAR testnet. No keys, no cache. */
export async function GET() {
  try {
    const snapshot = await networkSnapshot();
    // Short edge cache: a repeat visitor never waits on a cold function, and a
    // 15s-old block height is indistinguishable from a live one in this UI.
    return NextResponse.json(snapshot, {
      headers: { "cache-control": "public, s-maxage=15, stale-while-revalidate=60" },
    });
  } catch (error) {
    const message = error instanceof Error ? error.message : "network read failed";
    return NextResponse.json({ error: message }, { status: 502 });
  }
}
