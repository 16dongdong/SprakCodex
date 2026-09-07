export function usdPerMillionToMicrousd(value: string): number {
  const normalized = value.trim();
  if (!/^\d+(?:\.\d+)?$/.test(normalized)) {
    throw new Error("价格必须是非负十进制数");
  }
  const [wholePart, fractionPart = ""] = normalized.split(".");
  const significantFraction = fractionPart.replace(/0+$/, "");
  if (significantFraction.length > 6) {
    throw new Error("价格最多支持 6 位有效小数");
  }
  const paddedFraction = fractionPart.slice(0, 6).padEnd(6, "0");
  const whole = Number(wholePart);
  const fraction = Number(paddedFraction || "0");
  if (
    !Number.isSafeInteger(whole) ||
    whole > Math.floor(Number.MAX_SAFE_INTEGER / 1_000_000)
  ) {
    throw new Error("价格超出安全整数范围");
  }
  const microusd = whole * 1_000_000 + fraction;
  if (!Number.isSafeInteger(microusd)) {
    throw new Error("价格超出安全整数范围");
  }
  return microusd;
}

export function microusdToUsdPerMillion(value: number | null): string {
  if (value == null) return "";
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error("无效的 micro-USD 价格");
  }
  const whole = Math.floor(value / 1_000_000);
  const fraction = String(value % 1_000_000)
    .padStart(6, "0")
    .replace(/0+$/, "");
  return fraction ? `${whole}.${fraction}` : String(whole);
}
