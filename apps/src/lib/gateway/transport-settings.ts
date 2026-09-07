import type { GatewayTransportValues } from "@/types/settings";

export const DEFAULT_GATEWAY_TRANSPORT_VALUES: Readonly<GatewayTransportValues> =
  Object.freeze({
    sseKeepaliveEnabled: true,
    sseKeepaliveIntervalMs: 15_000,
    upstreamStreamTimeoutMs: 300_000,
    upstreamTotalTimeoutMs: 0,
  });

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function asBoolean(value: unknown, fallback: boolean): boolean {
  if (typeof value === "boolean") return value;
  if (typeof value === "number") return value !== 0;
  if (typeof value === "string") {
    const normalized = value.trim().toLowerCase();
    if (["1", "true", "yes", "on"].includes(normalized)) return true;
    if (["0", "false", "no", "off"].includes(normalized)) return false;
  }
  return fallback;
}

function asInteger(value: unknown, fallback: number, minimum: number): number {
  let parsed: number | null = null;
  if (typeof value === "number" && Number.isFinite(value)) {
    parsed = value;
  } else if (typeof value === "string" && value.trim()) {
    const candidate = Number(value.trim());
    parsed = Number.isFinite(candidate) ? candidate : null;
  }
  return parsed == null ? fallback : Math.max(minimum, Math.trunc(parsed));
}

export function normalizeGatewayTransportValues(
  payload: unknown,
): GatewayTransportValues {
  const source = asRecord(payload);
  return {
    sseKeepaliveEnabled: asBoolean(
      source.sseKeepaliveEnabled ?? source.sse_keepalive_enabled,
      DEFAULT_GATEWAY_TRANSPORT_VALUES.sseKeepaliveEnabled,
    ),
    sseKeepaliveIntervalMs: asInteger(
      source.sseKeepaliveIntervalMs ?? source.sse_keepalive_interval_ms,
      DEFAULT_GATEWAY_TRANSPORT_VALUES.sseKeepaliveIntervalMs,
      1,
    ),
    upstreamStreamTimeoutMs: asInteger(
      source.upstreamStreamTimeoutMs ?? source.upstream_stream_timeout_ms,
      DEFAULT_GATEWAY_TRANSPORT_VALUES.upstreamStreamTimeoutMs,
      0,
    ),
    upstreamTotalTimeoutMs: asInteger(
      source.upstreamTotalTimeoutMs ?? source.upstream_total_timeout_ms,
      DEFAULT_GATEWAY_TRANSPORT_VALUES.upstreamTotalTimeoutMs,
      0,
    ),
  };
}
