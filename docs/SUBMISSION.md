# NEAR HACKATHON 2026 — submission pack

**Project:** GAINZ — Guard AI NEAR ZK
**Tracks:** `security` (lead), `agents`, `business`
**Repo:** <https://github.com/memgochain/GAINZ>
**Live MVP:** <https://gainz-near-agent-guard.vercel.app>
**Live contract (NEAR testnet):** <https://testnet.nearblocks.io/address/fenceline-vault-8t1bk.testnet>
**Video:** `{{VIDEO_URL}}`
**X post:** `{{X_POST_URL}}`

---

## 1. The one-liner

A red-team range and loss-bound report for AI agent wallets on NEAR: it attacks the agent's spending
policy, finds the sequence of *allowed* actions that breaks it, and tells the owner the maximum they
can lose — then anchors that claim onchain.

## 2. What did you build? (form field, ~1 800 characters)

```
GAINZ — Guard AI NEAR ZK is a security layer for AI agents that hold NEAR.

THE PROBLEM
Giving an LLM a wallet means one hallucination or prompt injection can drain it. The industry's
answer is spending policies: per-transaction caps, daily caps, allowlists. Those enforce rules, but
nobody tells the owner whether the policy actually bounds the loss. Every action can be individually
legal while a sequence of legal actions still empties the vault.

WHAT WE BUILT
1. GuardVault (Rust / near-sdk): an onchain treasury vault for a single agent. It enforces nine
rules — positive amount, no self transfer, mandatory Noir proof, asset allowlist, recipient
allowlist, per-transaction cap, slippage tolerance, 24h window cap, hourly rate limit. The agent
holds only a function-call access key scoped to this contract, cannot edit the policy, and cannot
anchor reports. Approved actions are logged with an IPLD CIDv1 and the Noir proof hash.

2. GAINZ range (Rust): an analytic worst-case loss bound (closed form, integer micro-NEAR) plus
a deterministic adversarial search with six strategies — split sweeps, day-boundary double spends,
maximum-slippage swaps, allowlist fanout, injection probes, mixed sequences. It only proposes
actions the policy allows, so anything it finds is a policy bug. Output: declared cap vs analytic
bound vs observed loss, counterexamples replayed step by step, and a recommended fix.

3. Reproducible reports: the report hash excludes the wall clock, so same policy + same seed gives
the same CID on every machine. GuardVault.anchor_report timestamps that hash on NEAR.

4. Agent surface: a Rust MCP server (stdio) for Claude Desktop / Claude Code / Cursor with five
tools, optional Cerebras inference, and a one-command offline demo.

5. Zero-knowledge policy: a Noir circuit proves spent + tx <= secret_limit against a public policy
commitment without publishing the limits.

RESULT (real numbers, seed 1337, 9 600 simulated actions)
A "10 NEAR per day" calendar-window policy loses 20 NEAR in one 24h window, because two day buckets
fit inside it. Switch the same policy to a rolling window: observed loss returns to 10 NEAR and the
range reports "bounded".

The web MVP runs the same engine in TypeScript; a shared fixture suite proves both implementations
produce identical policy commitments, audit CIDs and report hashes.
```

## 3. Inspiration

Agent wallets are shipping everywhere and every one of them advertises "spending limits". Limits are
an enforcement mechanism, not evidence. A $10/day cap that can lose $20/day is worse than no
documentation at all, because it buys false confidence. We wanted the missing artifact: a report you
can hand to a user, a judge, or a treasury committee that says *this is the worst case, here is the
sequence that achieves it, here is the fix, and the hash is onchain*.

## 4. How it works

1. Owner writes `policy.json` and deploys `GuardVault` with those limits; the agent gets a scoped
   function-call key.
2. `near_guard_core` compiles the policy into integer limits and computes the analytic bound.
3. The range runs six adversarial strategies over a 48h horizon, evaluating every action with the
   same rule engine the contract enforces.
4. Counterexamples come back with timestamps, rule codes, CIDs and a recommended fix.
5. The owner anchors the report CID with `anchor_report`, so the claim is timestamped and
   re-verifiable by recomputing the hash from the report file.
6. In production the agent talks to the vault only through the MCP server, which attaches the reason
   text, the audit CID and the ZK proof hash to every payment.

## 5. Challenges

- **Making two implementations agree.** The contract is Rust, the MVP is TypeScript. We ported the
  engine and the range's xorshift64\* PRNG bit-for-bit and enforce parity with one shared fixture
  file; the first version of the report hash differed by exactly one integer division and the test
  caught it.
- **Bounds that are honest.** It would be easy to print `daily_cap` and call it a bound. The real
  work was enumerating where the number leaks: calendar windows doubling, slippage amplifying within
  the cap, rate limits binding before the daily cap, allowlisted recipients forwarding funds.
- **Determinism.** A hash you cannot reproduce is a screenshot. Removing the wall clock from the
  canonical bytes was the fix.
- **Refusing to overclaim.** The report says "analytic bound plus empirical search, not a proof" in
  the UI, the CLI, the MCP response and the JSON.

## 6. Accomplishments

- Found a real, reproducible policy bug in the most common implementation of "daily limits" and
  shipped the fix as a one-line policy change with a measurable before/after.
- Nine enforcement rules implemented twice (Rust and TypeScript) with byte-identical outputs.
- 33 Rust tests (engine, bounds, range, MCP protocol, contract window logic, cross-language parity) plus 9 TypeScript tests, all green; the contract compiles to wasm.
- A 90-second demo that runs with no network and no keys.

## 7. What's next

- Onchain verification of a real Noir proof (BN254 verifier inside `GuardVault`) instead of a
  non-empty-proof + hash check.
- A public counterexample corpus: every policy tested contributes its attack sequences, so the
  benchmark grows and the range gets better.
- Live NEAR Intents quoting for the slippage model (mainnet, tiny amounts).
- Continuous monitoring: re-run the range on every policy change and fail CI if the verdict regresses.

## 8. Tech stack

Rust (`near-sdk` 5, `near-workspaces`, `tokio`, `serde`, `sha2`), Noir, Next.js 15 / React 19 /
TypeScript on Vercel, IPLD CIDv1, MCP (JSON-RPC over stdio), optional Cerebras inference.

---

## 9. X / Twitter post (copy, replace the placeholders)

```
Just built GAINZ at @NEARProtocol #NEARHackathon 🛡️🤖

Everyone is shipping AI agents with wallets. Almost nobody can tell you the worst case.

GAINZ attacks your agent's spending policy, finds the sequence of ALLOWED actions that breaks
it, and reports the maximum you can lose.

Demo: a "10 NEAR/day" policy that actually loses 20 NEAR in 24h — because its daily budget resets
at 00:00 UTC and two day buckets fit in one day.

What's inside:
🔹 GuardVault (Rust/near-sdk): 9 onchain rules, function-call keys, per-tx + 24h caps, allowlist,
   slippage, IPLD audit CIDs
🔹 Red-team range: 6 attack strategies, counterexample replays, recommended fixes
🔹 Analytic loss bound + report CID anchored on NEAR — same policy + seed = same hash
🔹 Noir ZK: proves compliance without revealing the limits
🔹 Rust MCP server for Claude Desktop / Cursor, plus a Next.js MVP on Vercel

Repo: https://github.com/memgochain/GAINZ
Video: {{VIDEO_URL}}

#NEAR #AIAgents #Security #ZK
```

## 10. Submission checklist

- [x] Public repository with a README a judge can run in under five minutes
- [x] `cargo run -p near_guard_mcp -- --demo` works offline
- [x] Contract builds to wasm: `cargo build -p guard_vault --target wasm32-unknown-unknown --release`
- [x] Web MVP deployed to Vercel (no env vars required) — <https://gainz-near-agent-guard.vercel.app>
- [x] `GuardVault` deployed to NEAR testnet (`fenceline-vault-8t1bk.testnet`) and two report CIDs anchored; the MVP reads the contract live via `/api/network`
- [ ] Video recorded (script in `docs/DEMO_SCRIPT.md`) and uploaded → paste into `{{VIDEO_URL}}`
- [ ] X post published with the repo and video links → paste into `{{X_POST_URL}}`
- [ ] Form submitted at <https://nearhackathon.com/#apply> before 14 Nov 2026 23:59 UTC
- [ ] Team lead name, email and NEAR payout account entered in the form

---

## NEAR Hackathon submission checklist

Live demo: <https://gainz-near-agent-guard.vercel.app>
Repository: <https://github.com/memgochain/GAINZ> (public, MIT, `LICENSE` in the root)
Demo video: [`docs/gainz-demo.mp4`](gainz-demo.mp4) — 47 s, 1920x1080, no audio

Requirements from <https://nearhackathon.com/HACKATHON.md>:

- [x] Repository public before 2026-11-14T23:59:00Z
- [x] Reproducible README with "do X, see Y" steps
- [x] LICENSE file (MIT)
- [x] Hosted demo that stays up through judging — `https://gainz-near-agent-guard.vercel.app`
- [x] Demo video recorded from the live site
- [ ] Public demo post on X: says the project was built at NEAR HACKATHON, tags
      `@NEARProtocol` and `#NEARHackathon`, attaches the video — **needs the owner's X account**
- [ ] Submission at <https://nearhackathon.com/#apply> — needs the demo post link and a
      NEAR wallet address for the prize payout

The submitter must fill in: project name, description (20–2,000 characters), team lead,
email, team size, tracks, payout wallet address, repository link, demo post link.

### Why the build claim is checkable

`GET /api/contract-verification` hashes `web/contracts/guard_vault.wasm` and compares it with
the `code_hash` the vault account reports on chain. Both are `CwuPXnTyGhqZ2kN1V2mVJ1fcd5QtBaZsBnGNwVQd2rYS`,
so the page shows `verified build` instead of `unknown build`. Regenerate the artifact with:

```bash
cargo build -p guard_vault --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/guard_vault.wasm web/contracts/guard_vault.wasm
```
