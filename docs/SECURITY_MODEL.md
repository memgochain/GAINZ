# Security model, attack taxonomy and limits

The project makes a specific, narrow claim: **for a given policy, the worst case is bounded and
reproducible**. This document states exactly what that covers and what it does not.

## 1. Assets and actors

| Asset | At risk from |
|---|---|
| Vault balance (NEAR / allowed assets) | a compromised or manipulated agent |
| Policy limits (caps, allowlist, window) | an agent trying to widen its own permissions |
| Audit trail | an agent or operator trying to rewrite history |
| Owner's funds outside the vault | full-access key compromise (out of scope) |

Actors: **owner** (deploys, edits policy, anchors reports), **agent** (holds a scoped function-call
key), **recipient** (allowlisted third party), **attacker** (prompt injector or compromised agent
runtime).

## 2. Defended

| Attack | Defence | Where |
|---|---|---|
| Prompt-injected transfer to an unknown address | recipient allowlist | `recipient_not_whitelisted` |
| One huge transfer | per-transaction cap | `per_tx_cap` |
| Many small transfers until the vault is empty | 24h window cap + hourly rate limit | `daily_cap`, `rate_limit` |
| Agent widening its own limits | owner-only setters, agent has no admin calls | `assert_owner` |
| Agent anchoring a false report | owner-only anchoring | `anchor_report` |
| Agent sending funds to itself / the vault | self-transfer rule | `self_transfer` |
| Slippage abuse inside an allowed swap | `max_slippage_bps` | `slippage_exceeded` |
| Paying with a non-approved asset | asset allowlist (both sides of a swap) | `asset_not_allowed` |
| Bypassing the ZK requirement | non-empty proof required, hash anchored | `missing_zk_proof` |
| Silent policy drift | policy commitment published onchain, report pins it | `set_policy_commitment` |

## 3. Not defended

| Risk | Why |
|---|---|
| Owner key compromise | out of scope; the owner can change any limit |
| Forwarding through an allowlisted recipient | onchain code cannot see what a recipient does next — **largest residual risk** |
| Bugs in NEAR itself, in the RPC, or in a solver | outside this project |
| Asset price collapse | the bound covers policy leakage, not market risk |
| Attacks the six strategies never generate | "no counterexample" means "not found", not "impossible" |
| A malicious MCP client that talks to the contract directly | it still needs the agent key, and the contract still applies every rule |

## 4. Attack taxonomy

| Strategy | Precondition | Loss mechanism | Reported as |
|---|---|---|---|
| `split_sweep` | per-tx cap < daily cap | legal repeat payments | `cap_escape` if the rate limit is looser than the cap |
| `boundary_double` | calendar-day window | budget refills at 00:00 within one 24h interval | `window_boundary_double_spend` |
| `slippage_max` | `max_slippage_bps > 0` | value burned per swap, inside the cap | `slippage_value_leak` |
| `allowlist_fanout` | broad allowlist | many small legitimate payments | `cap_escape` / no counterexample |
| `injection_probe` | none | drain attempt to an unknown address | blocked, counted per rule |
| `mixed` | none | interleaving of the above | depends on the policy |

## 5. The bound, precisely

```
rate_ceiling  = rate_limit_per_hour × per_tx_cap × 24
spend_bound   = min(daily_cap, rate_ceiling) × k        k = 2 for calendar window, 1 for rolling
slippage      = spend_bound × max_slippage_bps / 10 000
value_at_risk = spend_bound + slippage
```

Assumptions, repeated in every report:

1. `GuardVault` enforces exactly the policy the range evaluated (checked by the shared policy
   commitment).
2. The agent holds no other signing key with access to the vault.
3. Allowlisted recipients do not forward funds onward.
4. The spending window is measured as stated (`calendar` or `rolling`).
5. Asset price risk is excluded.

A `bound_violation` verdict means the search beat the analytic bound, i.e. the calculator is wrong.
The test suite asserts this never happens for any fixture policy.

## 6. Why the report hash is trustworthy

`report_cid` is a CIDv1 (dag-cbor, sha2-256) over a canonical string containing: the policy
commitment, the seed, the sequence/horizon/step budget, the declared cap, the analytic bound, the
observed loss, the approval/block counts, the counterexample kinds and the bound-violation count.
The wall clock is **excluded**, so:

- anyone with the report JSON and the policy can recompute the hash;
- the same run is reproducible on another machine;
- `anchor_report` on `GuardVault` binds that hash to a NEAR timestamp, and refuses duplicates.

The hash proves *the report was produced from that policy with that search budget*. It does not
prove the policy is safe — the verdict field carries that, and it is stated in words everywhere.

## 7. Reproducing the security claims

The sandbox suite lives in its own crate (`contracts/sandbox_test`) so that the contract itself
has no native TLS dependency and its own unit tests run anywhere.

```bash
cargo test -p near_guard_core -p near_guard_mcp          # rules, windows, bounds, range, MCP
cargo test -p near_guard_core --test vectors             # parity + report hashes
cargo test -p guard_vault --lib                          # contract window semantics
cd web && npm test                                      # same numbers from TypeScript
cargo test -p guard_vault_sandbox_test -- --nocapture    # onchain enforcement (needs network + OpenSSL)
```

The sandbox test asserts: a valid payment succeeds, an unknown recipient fails, an over-cap payment
fails, a split sweep past the daily cap fails, an agent cannot edit the allowlist, an owner can
anchor a report, a duplicate anchor fails, and an agent cannot anchor at all.
