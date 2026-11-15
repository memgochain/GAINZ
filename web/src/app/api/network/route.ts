import { NextResponse } from "next/server";
import { networkSnapshot } from "@/lib/near";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

/** Live read of the deployed GuardVault on NEAR testnet. No keys, no cache. */
export async function GET() {
  try {
    const snapshot = await networkSnapshot();
    // networkSnapshot swallows per-call failures so a partial read still
    // renders. When nothing at all came back, say so instead of serving zeros.
    if (snapshot.blockHeight === 0 && snapshot.errors.length > 0) {
      return NextResponse.json(
        { error: snapshot.errors.join(" · ") },
        { status: 502, headers: { "cache-control": "no-store" } },
      );
    }
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
