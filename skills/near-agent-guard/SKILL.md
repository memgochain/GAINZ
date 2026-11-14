---
name: near-agent-guard
description: Enforce spending limits, recipient allowlists and Noir ZK policy checks for NEAR transactions via GuardVault, and verify that a policy's worst-case loss is actually bounded before an agent is funded.
---

# Guard AI NEAR ZK

Use this skill whenever the user or an agent needs to move NEAR, pay for an API,
or execute a swap / cross-chain intent from an autonomous agent wallet.

## Hard rules

1. **Never** sign a raw `transfer` transaction with a full-access key from an agent.
2. **Always** route payments through the `near_safe_transfer` MCP tool, with a
   human readable `reason` — the reason is hashed into the IPLD audit CID that
   ends up onchain.
3. Treat a `BLOCKED` verdict as final. Do not retry the same action with a
   different spelling, a smaller split, or a different recipient: that is exactly
   what the policy is protecting against.
4. Before funding a new agent, run `near_run_range` and read the verdict. If the
   report says `under_bounded`, do not fund the vault until the policy is fixed.
5. Never change `per_tx_cap`, `daily_cap`, `allowlist` or `window_model` from the
   agent side. Those are owner-only operations.

## Tools

| Tool | When |
|---|---|
| `near_safe_transfer` | every payment or swap the agent wants to make |
| `near_policy_status` | before starting a paid task, to see the remaining budget |
| `near_run_range` | before funding, and after any policy change |
| `near_audit_trail` | to review what was approved and what was refused, with CIDs |
| `near_report_anchor_payload` | to anchor a fresh report hash on the GuardVault contract |

## Reading a report

- `declared_daily_cap` is what the owner wrote in `policy.json`.
- `analytic_bound` is the worst legal case computed from the policy.
- `observed_max_loss` is what the adversarial range actually achieved.
- `verdict = under_bounded` means a legal sequence loses more than the declared
  cap. Show the counterexample to the owner; do not fund the agent.
