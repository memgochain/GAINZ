# GAINZ — Guard AI NEAR ZK

**Give an AI agent a wallet without giving it the treasury. Then prove the policy actually bounds the loss.**

Built for **NEAR HACKATHON 2026** — tracks: `security` (lead), `agents`, `business`.

- **Live MVP:** <https://fenceline-near-agent-guard.vercel.app> (GAINZ edition)
- **Repository:** <https://github.com/memgochain/near-agent-guard> (GAINZ edition)
- **Enforcement:** `GuardVault`, a Rust `near-sdk` contract with hard per-tx / 24h / allowlist / slippage limits
- **Verification:** an analytic loss bound plus a deterministic red-team range that finds the *legal* sequence which breaks the policy
- **Agent surface:** a Rust MCP server (Claude Desktop / Claude Code / Cursor) that routes every payment through the vault
- **NEAR AI Cloud integration:** agents can use `NEAR_AI_API_KEY` to connect to https://near.ai with model `fireworks::accounts/fireworks/models/llama-v3p1-70b-instruct` for intent generation and policy recommendations. Falls back to `CEREBRAS_API_KEY` or local simulation.
- **Privacy:** a Noir circuit proves `spent + tx <= limit` without publishing the limit
- **Audit:** every action and every report is content-addressed as a real CIDv1 and anchorable on NEAR

## Why GAINZ Is Different

Unlike centralized Python/API proxies that keep treasury private keys on a server, GAINZ enforces all limits, whitelists, and Noir ZK-proofs natively on-chain through the `GuardVault` contract and Function Call Access Keys. Every action is verified in the blockchain, no trusted third party required.

> Enforcement is a solved-looking problem with many vendors. **Verification is not.** GAINZ sells the evidence, not another gate.

---

## 1. The problem, in one example

`policy.json`:

```json
{ "per_tx_cap": 5, "daily_cap": 10, "rate_limit_per_hour": 4, "window_model": "calendar" }
```

Read that as a human and you conclude: *"the agent can lose at most 10 NEAR a day."*

The range disagrees:

```
declared cap        10 NEAR
observed max loss   18.110838 NEAR
analytic bound      20.2 NEAR
verdict             under_bounded
counterexample      window_boundary_double_spend
```

Every action in the counterexample is allowed by the policy. The bug is that a **calendar-day**
budget refills at 00:00 UTC, so two buckets fit inside one 24-hour interval. Switch the same
policy to a **rolling 24h** window and the loss drops back to the declared cap:

```
declared cap        10 NEAR
observed max loss   10 NEAR
analytic bound      10.1 NEAR   (0.1 NEAR is the allowed 100 bps of swap slippage)
verdict             bounded
```

That before/after is the product.

---

## 2. Quick start (under 5 minutes, no keys, no network)

```bash
# 1. the end-to-end demo: approve a bill, block an injection, find a policy bug, fix it
cargo run -p near_guard_mcp -- --demo

# 2. all Rust tests: policy engine, bound calculator, range, MCP protocol, contract helpers
cargo test -p near_guard_core -p near_guard_mcp -p guard_vault --lib

# 3. the web MVP locally
cd web && npm install && npm run dev      # http://localhost:3000
npm test                                  # parity tests against the shared fixtures
```

The contract's onchain test (needs network on first run, it downloads a NEAR sandbox node and
compiles the contract to wasm):

```bash
cargo test -p guard_vault_sandbox_test -- --nocapture
```

No `nargo`? The Noir circuit in `circuits/policy_check` is optional; the rest of the project does
not depend on it.

---

## 3. Architecture

```
[ AI agent: Claude / Cursor / Cerebras ]
                │  Model Context Protocol (stdio)
                ▼
[ GAINZ MCP ]  ── policy verdicts, IPLD CIDs, ZK proof hashes
                │  function-call access key (scoped, no full-access key)
                ▼
[ GuardVault (near-sdk) ]  ── per-tx cap, 24h window, allowlist, asset list,
                │             slippage, non-empty Noir proof, audit log, report anchors
                ▼
        NEAR account / recipient

        ┌───────────── verification plane (on-chain, reproducible) ─────────────┐
        │ GAINZ core:  policy → analytic bound → red-team range → report CID (verified on-chain) │
        │ web MVP (Next.js on Vercel): same engine, byte-identical hashes          │
        │ Noir circuit: proves compliance without revealing the limit             │
        └────────────────────────────────────────────────────────────────────────┘
```

### Repository layout

| Path | What it is |
|---|---|
| `contracts/guard_vault/` | `GuardVault` contract + sandbox integration test |
| `crates/near_guard_core/` | policy compiler, enforcement engine, loss bound, range, CID/report |
| `crates/near_guard_mcp/` | MCP stdio server, CLI `--demo`, optional Cerebras intent line |
| `circuits/policy_check/` | Noir circuit: private daily-limit compliance proof |
| `skills/near-agent-guard/` (GAINZ) | agent skill instructions for MCP clients |
| `policy/*.json` | the authored policies used by the demo and the fixtures |
| `fixtures/policy_vectors.json` | shared vectors asserted by **both** Rust and TypeScript |
| `web/` | Next.js MVP deployed to Vercel (range UI + enforcement console) |
| `docs/` | submission text, demo script, security model |

### One engine, two languages, identical hashes

The web app cannot call Rust, and the contract cannot call TypeScript, so the policy engine exists
twice: `crates/near_guard_core` and `web/src/lib`. They are kept honest by
`fixtures/policy_vectors.json`:

```bash
cargo test -p near_guard_core --test vectors   # Rust side
cd web && npm test                             # TypeScript side, same file
```

Both must produce the same policy commitment, the same IPLD audit CID, the same proof hash, the same
bounds **and the same report CID** — the range's PRNG is ported bit-for-bit (xorshift64\* over
`BigInt`), so this is a real equivalence check, not a smoke test.

Regenerate the reference numbers with:

```bash
cargo run -q -p near_guard_core --example vectors
```

---

## 4. The enforcement layer (`GuardVault`)

The agent holds only a **function-call access key** scoped to this contract. It cannot transfer
directly, cannot edit limits, cannot add allowlist entries, and cannot anchor reports.

| # | Rule | Code | Enforced by |
|---|---|---|---|
| 1 | amount must be positive | `non_positive_amount` | contract |
| 2 | recipient must not be the vault or the agent itself | `self_transfer` | engine |
| 3 | a non-empty Noir proof must be attached | `missing_zk_proof` | contract |
| 4 | asset must be allowed (both sides of a swap) | `asset_not_allowed` | engine |
| 5 | recipient must be allowlisted | `recipient_not_whitelisted` | contract |
| 6 | amount ≤ per-transaction cap | `per_tx_cap` | contract |
| 7 | swap slippage ≤ policy tolerance | `slippage_exceeded` | engine |
| 8 | window spend + amount ≤ daily cap | `daily_cap` | contract |
| 9 | approved actions per rolling hour ≤ limit | `rate_limit` | engine |

Rules 1, 3, 5, 6 and 8 are hard onchain assertions, together with caller authentication
(`predecessor == agent_id`) and the CID format check. Rules 2, 4, 7 and 9 need context the vault does
not see — the swap quote, the asset list, the timestamp density of a burst — so the engine that
prepares and signs the call enforces them, and the range assumes exactly that split when it computes
the bound. Nothing is enforced only in the UI: the MCP server refuses the same actions with the same
rule codes.

Approved actions are logged with an **IPLD CID** and the **Noir proof hash**; the owner can also
anchor a **report hash** with `anchor_report(report_hash, policy_commitment, declared_cap,
analytic_bound, observed_loss, counterexample_count, verdict)`. The contract refuses a second anchor
of the same hash and refuses an agent-originated anchor.

Build it:

```bash
rustup target add wasm32-unknown-unknown
cargo build -p guard_vault --target wasm32-unknown-unknown --release
# `cargo check -p guard_vault` without --target is refused by near-sdk on purpose:
cargo check -p guard_vault --target wasm32-unknown-unknown
```

Deploy to testnet (from the project brief, `cargo-near` not required for the demo):

```bash
cd contracts/guard_vault
cargo near deploy build-non-reproducible-wasm vault-guard.testnet \
  with-init-call new json-args '{"owner_id":"owner-guard.testnet","agent_id":"agent-bot.testnet","max_tx_amount":"5000000000000000000000000","daily_limit":"10000000000000000000000000","window_model":"rolling"}' \
  prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' network-config testnet sign-with-keychain send
```

---

## 5. The verification layer (GAINZ)

### 5.1 Analytic bound

```
rate_ceiling = rate_limit_per_hour × per_tx_cap × 24
spend_bound  = min(daily_cap, rate_ceiling) × (2 if window is calendar-day else 1)
slippage     = spend_bound × max_slippage_bps / 10 000
value_at_risk = spend_bound + slippage
```

Closed form, integer micro-NEAR arithmetic, with its assumptions printed in every report.

### 5.2 Red-team range

Six deterministic strategies, seeded, over a fixed 48h horizon that always contains two day
boundaries:

| Strategy | What it probes |
|---|---|
| `split_sweep` | staying under the per-tx cap until the budget is gone |
| `boundary_double` | spending the day budget at 23:59 and again at 00:00 |
| `slippage_max` | swapping at exactly the tolerated slippage, repeatedly |
| `allowlist_fanout` | many small payments to allowlisted addresses |
| `injection_probe` | prompt-injection style drains to unknown addresses |
| `mixed` | random interleaving of the above |

The range never proposes an action the policy forbids, so anything it finds is a **policy** bug. A
`bound_violation` (observed > analytic bound) would mean the bound calculator itself is wrong; the
test suite asserts it never happens.

### 5.3 Report

`build_report` returns the bound, the range result, counterexamples with a recommended fix, a
verdict (`bounded` / `under_bounded` / `bound_violation`) and a **report CID**. The canonical bytes
exclude the wall clock, so *same policy + same seed ⇒ same hash*, which is what makes the anchor
meaningful.

---

## 6. MCP server

```json
{
  "mcpServers": {
    "near-agent-guard": { // GAINZ
      "command": "cargo",
      "args": ["run", "--quiet", "-p", "near_guard_mcp", "--", "--policy", "/absolute/path/policy/policy.json"],
      "env": { "CEREBRAS_API_KEY": "optional" }
    }
  }
}
```

Tools: `near_safe_transfer`, `near_policy_status`, `near_run_range`, `near_audit_trail`,
`near_report_anchor_payload`. A refused payment returns `isError: true` with the rule code, so a
well-behaved agent stops instead of retrying with a split amount.

With `CEREBRAS_API_KEY` set, `--demo` prints a live agent intent; without it the demo runs offline
with a deterministic fallback. The project never needs a network connection to be judged.

---

## 7. Web MVP (Vercel)

`web/` is a Next.js app that runs the **same** engine server-side:

| Route | Purpose |
|---|---|
| `POST /api/range` | compile a policy, compute the bound, run the range, return the report |
| `POST /api/transfer` | evaluate one action, return the verdict + audit CID + next state |
| `GET /api/network` | read the live `GuardVault` on NEAR testnet: `status`, `view_account`, `get_config`, `get_report`, `get_recent_audit_logs` |

The page exposes the policy sliders, the declared/observed/bound comparison, the loss timeline, the
counterexample with its fix, the enforcement console and the report CID to anchor.

### 7.1 The live deployment

`GuardVault` is deployed and anchored on NEAR testnet, and the "Live on NEAR testnet" panel reads it
at request time — no fixtures, no cached snapshot:

| What | Where |
|---|---|
| Contract | [`fenceline-vault-8t1bk.testnet`](https://testnet.nearblocks.io/address/fenceline-vault-8t1bk.testnet) (5 NEAR/tx, 10 NEAR per rolling 24h, API provider allowlisted) |
| Agent account | `fenceline-agent-8t1bk.testnet` (holds no key in this deployment) |
| Deploy tx | [`F1oMn9bNWNc74TwQRTr85seZozkeCbdtU253ztN9opbQ`](https://testnet.nearblocks.io/txns/F1oMn9bNWNc74TwQRTr85seZozkeCbdtU253ztN9opbQ) |
| Anchored weak report | `bafyreiam7if3ugmzvk5hz4ig73gt3akp6jtkb7wvzbswod3jm5n2ejkhwy` × [`J3CBhfRuZ9jZUPXN6xvdbESH5QxLBkaAFv5ZaGboiwPA`](https://testnet.nearblocks.io/txns/J3CBhfRuZ9jZUPXN6xvdbESH5QxLBkaAFv5ZaGboiwPA) |
| Anchored hardened report | `bafyreickps35otu3w6dcegoi5md3zinfvdhxj4v4aooqhbjxfpf67tddcy` × [`DSaD1sNiDPJntqeVJ4iMDvYm2WBJ4nBL2YWm9xoXG7M1`](https://testnet.nearblocks.io/txns/DSaD1sNiDPJntqeVJ4iMDvYm2WBJ4nBL2YWm9xoXG7M1) |

The panel recomputes its own report with the default seed (1337) and 120 sequences and compares the
CID against `get_report` on chain: switch between the two presets and the anchor line follows, so a
swapped or re-seeded report is visibly unanchored. The owner and agent keys are not shipped with the
site — writing to the vault is a deliberate, keyed operation (`anchor_report`, `execute_agent_transfer`).

```bash
cd web
npm install
npm run dev      # http://localhost:3000
npm run build
```

Deploy: `vercel --prod` from `web/`, or import the repo and set **Root Directory = `web`**. No
environment variables are required for the MVP.

---

## 8. Verification status

| Check | Result |
|---|---|
| `cargo test -p near_guard_core -p near_guard_mcp` | 24 tests pass (engine rules, window semantics, bound math, range properties, CID validity, MCP protocol) |
| `cargo test -p near_guard_core --test vectors` | 6 parity/vector tests pass |
| `cargo test -p guard_vault --lib` | 17 contract arithmetic tests pass on the host: spend accumulation per window, prune, remaining budget, cap boundaries, overflow safety |
| `cargo build -p guard_vault --target wasm32-unknown-unknown --release` | contract compiles to a 424 KB wasm |
| `cargo test -p guard_vault_sandbox_test` | **not run here**: needs a native OpenSSL dev package and network to download the sandbox node; run it locally to exercise the deployed contract |
| `cd web && npm test` | 9 tests pass, including identical report CIDs vs Rust |
| `cd web && npm run build` | production build passes |
| `vercel deploy --prod` | deployed; `/` and `POST /api/range` verified on the live domain (200, counterexample returned) |
| `cargo run -p near_guard_mcp -- --demo` | end-to-end demo runs offline |

Benchmark (policy from `policy/policy.json`, seed 1337, 200 sequences × 48 steps = 9 600 actions):

| Policy | Declared | Analytic bound | Observed max loss | Verdict | Counterexample |
|---|---|---|---|---|---|
| calendar window | 10 NEAR | 20.2 NEAR | 20.0 NEAR | `under_bounded` | `window_boundary_double_spend` |
| rolling window | 10 NEAR | 10.1 NEAR | 10.0 NEAR | `bounded` | none |

---

## 9. Honest limitations

- This is an **analytic bound plus an empirical search**, not a formal proof. "No counterexample
  found" means the six strategies did not find one within the budget.
- The bound assumes `GuardVault` enforces exactly the policy the range evaluated, that the agent
  holds no other signing key, and that allowlisted recipients do not forward funds. **Forwarding is
  not visible onchain** and is the largest residual risk.
- Asset price risk is out of scope: the report bounds policy leakage, not market moves.
- The Noir circuit is a reference implementation. Onchain verification of a real proof (BN254
  verifier in the contract) is the next milestone; today the contract requires a non-empty proof and
  hashes it into the audit log.
- NEAR Intents has no testnet, so swap behaviour is modelled (amount / quote / minimum out /
  slippage) rather than executed against a live solver.

---

## 10. Hackathon submission

- Public repository: this repo
- Demo video (60–90s) and shot list: [`docs/DEMO_SCRIPT.md`](docs/DEMO_SCRIPT.md)
- Form answers, tracks, and the X post: [`docs/SUBMISSION.md`](docs/SUBMISSION.md)
- Threat model and attack taxonomy: [`docs/SECURITY_MODEL.md`](docs/SECURITY_MODEL.md)

Tracks entered with this single submission: **`security`** (lead), **`agents`**, **`business`**.

---

MIT licensed — see [LICENSE](LICENSE).
