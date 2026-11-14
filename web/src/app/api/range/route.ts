import { NextResponse } from "next/server";
import { DEFAULT_CONFIG } from "@/lib/presets";
import { compilePolicy } from "@/lib/policy";
import { buildReport } from "@/lib/report";
import type { FuzzConfig, Policy } from "@/lib/types";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

export async function POST(request: Request) {
  try {
    const body = (await request.json()) as { policy?: Policy; config?: Partial<FuzzConfig> };
    if (!body.policy) {
      return NextResponse.json({ error: "policy is required" }, { status: 400 });
    }
    const policy = compilePolicy(body.policy);
    const config: FuzzConfig = {
      seed: Number(body.config?.seed ?? DEFAULT_CONFIG.seed),
      sequences: Number(body.config?.sequences ?? DEFAULT_CONFIG.sequences),
      horizon_hours: Number(body.config?.horizon_hours ?? DEFAULT_CONFIG.horizon_hours),
      steps_per_sequence: Number(
        body.config?.steps_per_sequence ?? DEFAULT_CONFIG.steps_per_sequence,
      ),
    };
    const report = buildReport(policy, config, Date.now());
    return NextResponse.json({ report });
  } catch (error) {
    const message = error instanceof Error ? error.message : "range failed";
    return NextResponse.json({ error: message }, { status: 400 });
  }
}
