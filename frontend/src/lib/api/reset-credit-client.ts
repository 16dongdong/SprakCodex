import type {
  ResetCredit,
  ResetCreditConsumeResult,
  ResetCreditsSnapshot,
} from "@/types";
import { invoke, withAddr } from "./transport";

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function optionalString(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function optionalNumber(value: unknown): number | null {
  const number = typeof value === "number" ? value : Number(value);
  return Number.isFinite(number) ? number : null;
}

function normalizeResetCredit(value: unknown): ResetCredit {
  const source = asRecord(value);
  return {
    id: optionalString(source.id),
    status: optionalString(source.status),
    resetType: optionalString(source.resetType ?? source.reset_type),
    grantedAt: optionalNumber(source.grantedAt ?? source.granted_at),
    expiresAt: optionalNumber(source.expiresAt ?? source.expires_at),
    redeemedAt: optionalNumber(source.redeemedAt ?? source.redeemed_at),
    rawStatus: optionalString(source.rawStatus ?? source.raw_status),
  };
}

function normalizeResetCreditsSnapshot(value: unknown): ResetCreditsSnapshot {
  const source = asRecord(value);
  const credits = Array.isArray(source.credits)
    ? source.credits.map(normalizeResetCredit)
    : [];
  return {
    availableCount: optionalNumber(
      source.availableCount ?? source.available_count,
    ),
    credits,
    nextExpiresAt: optionalNumber(
      source.nextExpiresAt ?? source.next_expires_at,
    ),
  };
}

function normalizeConsumeResult(value: unknown): ResetCreditConsumeResult {
  const source = asRecord(value);
  return {
    consumed: source.consumed === true,
    usageRefreshed:
      source.usageRefreshed === true || source.usage_refreshed === true,
    snapshot:
      source.snapshot == null
        ? null
        : normalizeResetCreditsSnapshot(source.snapshot),
    warning: optionalString(source.warning),
  };
}

export const resetCreditClient = {
  async get(accountId: string): Promise<ResetCreditsSnapshot> {
    const result = await invoke<unknown>(
      "service_usage_reset_credits",
      withAddr({ accountId }),
    );
    return normalizeResetCreditsSnapshot(result);
  },

  async consume(accountId: string): Promise<ResetCreditConsumeResult> {
    const result = await invoke<unknown>(
      "service_usage_reset_credit_consume",
      withAddr({ accountId }),
    );
    return normalizeConsumeResult(result);
  },
};

export {
  normalizeConsumeResult as normalizeResetCreditConsumeResult,
  normalizeResetCreditsSnapshot,
};
