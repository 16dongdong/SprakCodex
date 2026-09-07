import type { AccountUsage, ResetCredit } from "@/types";

interface CachedResetCredits {
  present: boolean;
  availableCount: number | null;
}

function finiteNonNegativeNumber(value: unknown): number | null {
  const number = typeof value === "number" ? value : Number(value);
  return Number.isFinite(number) ? Math.max(0, Math.trunc(number)) : null;
}

export function readCachedResetCredits(
  usage: AccountUsage | null | undefined,
): CachedResetCredits {
  const raw = usage?.creditsJson?.trim();
  if (!raw) return { present: false, availableCount: null };
  try {
    const payload = JSON.parse(raw) as Record<string, unknown>;
    const resetCredits = payload?.rate_limit_reset_credits;
    if (!resetCredits || typeof resetCredits !== "object") {
      return { present: false, availableCount: null };
    }
    const source = resetCredits as Record<string, unknown>;
    return {
      present: true,
      availableCount: finiteNonNegativeNumber(
        source.available_count ?? source.availableCount,
      ),
    };
  } catch {
    return { present: false, availableCount: null };
  }
}

export function isResetCreditAvailable(
  credit: ResetCredit,
  nowSeconds = Math.floor(Date.now() / 1000),
): boolean {
  const status = (credit.status || credit.rawStatus || "")
    .trim()
    .toLowerCase();
  if (status !== "available") return false;
  return credit.expiresAt == null || credit.expiresAt > nowSeconds;
}
