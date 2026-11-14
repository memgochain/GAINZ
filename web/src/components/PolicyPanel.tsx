"use client";

import { policyCommitment, compilePolicy } from "@/lib/policy";
import { PRESETS, type PresetId } from "@/lib/presets";
import type { Policy } from "@/lib/types";

interface Props {
  policy: Policy;
  preset: PresetId;
  config: { seed: number; sequences: number };
  running: boolean;
  onChange: (policy: Policy) => void;
  onPreset: (preset: PresetId) => void;
  onConfig: (config: { seed: number; sequences: number }) => void;
  onRun: () => void;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export default function PolicyPanel({
  policy,
  preset,
  config,
  running,
  onChange,
  onPreset,
  onConfig,
  onRun,
}: Props) {
  const setPerTx = (value: number) => {
    const perTx = clamp(value, 0.5, 20);
    onChange({ ...policy, per_tx_cap: perTx, daily_cap: Math.max(policy.daily_cap, perTx) });
  };

  const setDaily = (value: number) => {
    const daily = clamp(value, 0.5, 100);
    onChange({
      ...policy,
      daily_cap: Math.max(daily, policy.per_tx_cap),
    });
  };

  let commitment = "";
  try {
    commitment = policyCommitment(compilePolicy(policy));
  } catch {
    commitment = "invalid policy";
  }

  return (
    <section className="panel" aria-label="Policy">
      <header className="panel-head">
        <h2>Policy</h2>
        <span className="tag">policy.json</span>
      </header>

      <div className="preset-grid">
        {(Object.keys(PRESETS) as PresetId[]).map((id) => (
          <button
            key={id}
            type="button"
            className={`preset ${preset === id ? "preset-active" : ""}`}
            onClick={() => onPreset(id)}
          >
            <span className="preset-title">{PRESETS[id].label}</span>
            <span className="preset-blurb">{PRESETS[id].blurb}</span>
          </button>
        ))}
      </div>

      <div className="field">
        <div className="field-head">
          <label htmlFor="per-tx">Per-transaction cap</label>
          <span className="field-value">{policy.per_tx_cap} NEAR</span>
        </div>
        <input
          id="per-tx"
          type="range"
          min={0.5}
          max={20}
          step={0.5}
          value={policy.per_tx_cap}
          onChange={(event) => setPerTx(Number(event.target.value))}
        />
      </div>

      <div className="field">
        <div className="field-head">
          <label htmlFor="daily">Daily cap (declared)</label>
          <span className="field-value">{policy.daily_cap} NEAR</span>
        </div>
        <input
          id="daily"
          type="range"
          min={1}
          max={60}
          step={1}
          value={policy.daily_cap}
          onChange={(event) => setDaily(Number(event.target.value))}
        />
      </div>

      <div className="field">
        <div className="field-head">
          <label htmlFor="rate">Rate limit</label>
          <span className="field-value">{policy.rate_limit_per_hour} / hour</span>
        </div>
        <input
          id="rate"
          type="range"
          min={1}
          max={20}
          step={1}
          value={policy.rate_limit_per_hour}
          onChange={(event) =>
            onChange({ ...policy, rate_limit_per_hour: Number(event.target.value) })
          }
        />
      </div>

      <div className="field">
        <div className="field-head">
          <label htmlFor="slippage">Max swap slippage</label>
          <span className="field-value">{policy.max_slippage_bps} bps</span>
        </div>
        <input
          id="slippage"
          type="range"
          min={0}
          max={500}
          step={10}
          value={policy.max_slippage_bps}
          onChange={(event) =>
            onChange({ ...policy, max_slippage_bps: Number(event.target.value) })
          }
        />
      </div>

      <div className="field-row">
        <span className="field-label">Spending window</span>
        <div className="segmented">
          {(["calendar", "rolling"] as const).map((model) => (
            <button
              key={model}
              type="button"
              className={policy.window_model === model ? "segment segment-active" : "segment"}
              onClick={() => onChange({ ...policy, window_model: model })}
            >
              {model}
            </button>
          ))}
        </div>
      </div>

      <div className="chips">
        <span className="field-label">Allowlist</span>
        <div className="chip-row">
          {policy.allowlist.map((recipient) => (
            <span className="chip" key={recipient}>
              {recipient}
            </span>
          ))}
        </div>
      </div>

      <div className="chips">
        <span className="field-label">Assets</span>
        <div className="chip-row">
          {policy.allowed_assets.map((asset) => (
            <span className="chip chip-quiet" key={asset}>
              {asset}
            </span>
          ))}
          <span className="chip chip-quiet">ZK required</span>
        </div>
      </div>

      <div className="run-block">
        <div className="run-inputs">
          <label>
            <span>seed</span>
            <input
              type="number"
              value={config.seed}
              onChange={(event) => onConfig({ ...config, seed: Number(event.target.value) })}
            />
          </label>
          <label>
            <span>sequences</span>
            <input
              type="number"
              min={1}
              max={600}
              value={config.sequences}
              onChange={(event) => onConfig({ ...config, sequences: Number(event.target.value) })}
            />
          </label>
        </div>
        <button type="button" className="primary" onClick={onRun} disabled={running}>
          {running ? "Running range…" : "Run range"}
        </button>
      </div>

      <div className="commitment">
        <span className="field-label">Policy commitment</span>
        <code title={commitment}>{commitment.slice(0, 32)}…</code>
      </div>
    </section>
  );
}
