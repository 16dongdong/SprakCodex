function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

export function unwrapUsageSnapshotPayload(payload: unknown): unknown {
  const source = asRecord(payload);
  return source && "snapshot" in source ? source.snapshot : payload;
}
