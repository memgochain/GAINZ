"use client";

import { formatNear } from "@/lib/amount";
import type { TimelineBucket } from "@/lib/types";

interface Props {
  timeline: TimelineBucket[];
  declaredMicro: number;
  observedMicro: number;
}

export default function RangeChart({ timeline, declaredMicro, observedMicro }: Props) {
  const peak = Math.max(declaredMicro, ...timeline.map((bucket) => bucket.loss_micro), 1);
  const hours = timeline.length;

  return (
    <div className="chart">
      <div className="chart-head">
        <span>Loss per hour of simulated horizon</span>
        <span className="chart-legend">
          <i className="dot dot-loss" /> loss realised
          <i className="dot dot-blocked" /> attempts blocked
        </span>
      </div>
      <div className="chart-bars" role="img" aria-label="Loss per hour of the simulated horizon">
        {timeline.map((bucket) => {
          const lossHeight = Math.max(0, (bucket.loss_micro / peak) * 100);
          const blockedHeight = Math.max(
            0,
            (bucket.blocked_in_bucket / Math.max(1, ...timeline.map((b) => b.blocked_in_bucket))) * 42,
          );
          return (
            <div
              className="chart-col"
              key={bucket.hour}
              title={`t+${bucket.hour}h · loss ${formatNear(bucket.loss_micro)} NEAR · ${bucket.approved_in_bucket} approved / ${bucket.blocked_in_bucket} blocked`}
            >
              <div className="chart-blocked" style={{ height: `${blockedHeight}%` }} />
              <div className="chart-bar" style={{ height: `${lossHeight}%` }} />
            </div>
          );
        })}
      </div>
      <div className="chart-axis">
        <span>t+0h</span>
        <span>t+{Math.round(hours / 2)}h</span>
        <span>t+{hours}h</span>
      </div>
      <div className="chart-foot">
        <span>peak hour {formatNear(peak)} NEAR</span>
        <span>observed max in any 24h window {formatNear(observedMicro)} NEAR</span>
      </div>
    </div>
  );
}
