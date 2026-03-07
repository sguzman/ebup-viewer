export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function clamp01(value: number): number {
  return Math.min(1, Math.max(0, value));
}

export function normalizeNumber(
  value: number,
  min: number,
  max: number,
  step: number,
  decimals: number
): number {
  const clamped = clamp(value, min, max);
  if (step <= 0) {
    return Number(clamped.toFixed(decimals));
  }
  const snapped = min + Math.round((clamped - min) / step) * step;
  return Number(clamp(snapped, min, max).toFixed(decimals));
}

export function almostEqual(a: number, b: number, decimals: number): boolean {
  const threshold = Math.max(1e-8, Math.pow(10, -decimals) / 2);
  return Math.abs(a - b) <= threshold;
}
