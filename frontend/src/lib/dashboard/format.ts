"use client";


export function formatPercent(value: number | null | undefined): string {
  return value == null ? "--" : `${Math.max(0, Math.round(value))}%`;
}

// Token 统一按十进制 K/M/B 缩写；缺失值不伪装成零，舍入跨档时提升单位。
export function formatCompactTokenAmount(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "—";
  const normalized = Math.max(0, value);
  const units = ["", "K", "M", "B"];
  let unit = Math.min(3, Math.floor(Math.log10(Math.max(1, normalized)) / 3));
  let scaled = Number((normalized / 1000 ** unit).toFixed(2));
  if (scaled >= 1000 && unit < 3) {
    unit += 1;
    scaled = Number((normalized / 1000 ** unit).toFixed(2));
  }
  return `${scaled}${units[unit]}`;
}

export function estimateChartYAxisWidth(
  values: Array<number | null | undefined>,
  formatter: (value: number) => string,
  minimumWidth = 44,
): number {
  const widestLabelLength = values.reduce<number>((maxLength, value) => {
    const normalizedValue = typeof value === "number" && Number.isFinite(value) ? value : 0;
    const normalized = Math.max(0, normalizedValue);
    return Math.max(maxLength, formatter(normalized).length);
  }, 0);

  return Math.max(minimumWidth, Math.ceil(widestLabelLength * 8 + 16));
}
