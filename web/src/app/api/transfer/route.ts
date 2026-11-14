import { NextResponse } from "next/server";
import { evaluate } from "@/lib/engine";
import { compilePolicy } from "@/lib/policy";
import type { Action, Policy, VaultState } from "@/lib/types";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

interface Body {
  policy?: Policy;
  state?: VaultState;
  action?: Action;
  now_ms?: number;
}

export async function POST(request: Request) {
  try {
    const body = (await request.json()) as Body;
    if (!body.policy) return NextResponse.json({ error: "policy is required" }, { status: 400 });
    if (!body.action) return NextResponse.json({ error: "action is required" }, { status: 400 });
    if (body.action.kind !== "transfer" && body.action.kind !== "swap") {
      return NextResponse.json({ error: "action.kind must be transfer or swap" }, { status: 400 });
    }

    const policy = compilePolicy(body.policy);
    const state: VaultState = {
      history: (body.state?.history ?? []).map((entry) => ({
        ts_ms: Number(entry.ts_ms),
        amount_micro: Number(entry.amount_micro),
        loss_micro: Number(entry.loss_micro ?? 0),
      })),
    };
    const now = Number(body.now_ms ?? Date.now());

    const { decision, state: nextState } = evaluate(
      policy,
      state,
      body.action,
      now,
      "noir-policy-proof",
    );

    return NextResponse.json({ decision, state: nextState });
  } catch (error) {
    const message = error instanceof Error ? error.message : "evaluation failed";
    return NextResponse.json({ error: message }, { status: 400 });
  }
}
