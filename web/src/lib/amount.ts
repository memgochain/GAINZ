/** Money is integer micro-NEAR everywhere, exactly as in near_guard_core. */
export const MICRO_PER_NEAR = 1_000_000;
export const MS_PER_DAY = 86_400_000;
export const MS_PER_HOUR = 3_600_000;

export function microFromNear(near: number): number {
  if (!Number.isFinite(near) || near <= 0) return 0;
  return Math.round(near * MICRO_PER_NEAR);
}

export function nearFromMicro(micro: number): number {
  return micro / MICRO_PER_NEAR;
}

/** `5`, `1.5`, `0.000001` — trailing zeros trimmed, matching format_near. */
export function formatNear(micro: number): string {
  const whole = Math.floor(micro / MICRO_PER_NEAR);
  const frac = micro % MICRO_PER_NEAR;
  if (frac === 0) return String(whole);
  return `${whole}.${String(frac).padStart(6, "0").replace(/0+$/, "")}`;
}

export function dayIndex(tsMs: number): number {
  return Math.floor(tsMs / MS_PER_DAY);
}
