# 90-second demo script

Target: **1080p, 60–90 seconds, no dead air**. Record the terminal once and the browser once, then
cut. Everything below runs offline except the browser page, which needs the Vercel URL (or `npm run
dev`).

## Before you hit record

```bash
# terminal 1 — pre-warm the build so the demo does not compile on camera
cargo run -q -p near_guard_mcp -- --demo > /tmp/demo.txt
# check it ran, then use the same binary:
cargo build -q -p near_guard_mcp --release
```

Browser: open <https://fenceline-near-agent-guard.vercel.app> (or `http://localhost:3000`), let the
first range finish, then reload with an empty state so the console is clean.

Terminal: font size ~16, dark theme, window 110x30 minimum, no other tabs.

## Shot list

| Time | Screen | Action | Voiceover |
|---|---|---|---|
| 0:00–0:08 | Browser, hero | Scroll to the top, cursor still | "Every AI agent with a wallet ships a spending policy. Nobody can tell you the worst case." |
| 0:08–0:20 | Browser, policy panel | Drag the daily cap slider from 10 to 30, then back | "This is a 10 NEAR daily cap, a 5 NEAR per-transaction cap, and a 4-per-hour limit. It looks safe." |
| 0:20–0:38 | Browser, range panel | Click **Run range** | "GAINZ attacks the policy using only actions the policy allows: split sweeps, day-boundary flushes, maximum slippage swaps." |
| 0:38–0:52 | Browser, counterexample | Hover the counterexample rows | "It found one. Ten NEAR declared, eighteen NEAR observed — because a calendar-day budget can be spent twice inside a 24-hour window." |
| 0:52–1:04 | Browser, preset switch | Click **Rolling window**, let the range re-run | "Same limits, rolling 24h window. Observed loss drops back to ten NEAR. Verdict: bounded." |
| 1:04–1:16 | Browser, console | Click **Pay 1.5 NEAR invoice**, then **Prompt injection** | "The same nine rules enforce this onchain. The invoice is approved with an IPLD audit CID. The injection is refused." |
| 1:16–1:30 | Terminal | `cargo run -q -p near_guard_mcp --release -- --demo` (or paste the pre-run output) | "Same engine in Rust, same verdicts, a report hash you can anchor on NEAR. Repo and live MVP in the description." |

## Terminal segment (paste-ready, pre-recorded output)

Use the section of `--demo` from `GAINZ RANGE — is the authored policy actually bounded?` to the
end. It contains: declared cap, analytic bound, observed loss, the counterexample with steps, the
rolling-window fix and the `near call ... anchor_report` line — exactly what a judge needs to see.

## If something breaks on camera

- Range is slow on a cold serverless function: click once before recording to warm it.
- No network: use `npm run dev` locally and keep the terminal demo as the fallback; the CLI never
  touches the network.
- Slider drags look jittery: pre-set the policy, and only click **Run range**.

## Upload

Title: `GAINZ — proving your AI agent's spending policy actually bounds the loss (NEAR Hackathon 2026)`

Description:

```
GAINZ — Guard AI NEAR ZK · security track, NEAR HACKATHON 2026.

A "10 NEAR per day" policy loses 20 NEAR in one 24h window, because its budget resets at midnight.
GAINZ finds that sequence, replays it, and gives the owner a fix plus a report hash to anchor
on NEAR.

Repo: https://github.com/memgochain/near-agent-guard
Live MVP: https://fenceline-near-agent-guard.vercel.app
Built at NEAR HACKATHON.
```
