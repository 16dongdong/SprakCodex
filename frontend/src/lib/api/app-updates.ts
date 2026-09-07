function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readStringField(payload: unknown, key: string, fallback = ""): string {
  const source = asRecord(payload);
  const value = source?.[key];
  return typeof value === "string" ? value : fallback;
}

function readBooleanField(payload: unknown, key: string, fallback = false): boolean {
  const source = asRecord(payload);
  const value = source?.[key];
  return typeof value === "boolean" ? value : fallback;
}

function readNumberField(payload: unknown, key: string, fallback = 0): number {
  const source = asRecord(payload);
  const value = source?.[key];
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string") {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }
  return fallback;
}

function readNullableStringField(payload: unknown, key: string): string | null {
  const value = readStringField(payload, key);
  return value ? value : null;
}

export interface UpdateCheckResult {
  repo: string;
  mode: string;
  isPortable: boolean;
  hasUpdate: boolean;
  canPrepare: boolean;
  currentVersion: string;
  latestVersion: string;
  releaseTag: string;
  releaseName: string | null;
  publishedAt: string | null;
  reason: string | null;
  checkedAtUnixSecs: number;
}

export interface UpdatePrepareResult {
  prepared: boolean;
  mode: string;
  isPortable: boolean;
  releaseTag: string;
  latestVersion: string;
  assetName: string;
  assetPath: string;
  downloaded: boolean;
}

export interface PendingUpdateResult extends UpdatePrepareResult {
  installerPath: string | null;
  stagingDir: string | null;
  preparedAtUnixSecs: number;
}

export interface UpdateActionResult {
  ok: boolean;
  message: string;
}

export interface UpdateStatusResult {
  repo: string;
  mode: string;
  isPortable: boolean;
  currentVersion: string;
  currentExePath: string;
  portableMarkerPath: string;
  pending: PendingUpdateResult | null;
  lastCheck: UpdateCheckResult | null;
  lastError: string | null;
}

export function readUpdateCheckResult(payload: unknown): UpdateCheckResult {
  return {
    repo: readStringField(payload, "repo"),
    mode: readStringField(payload, "mode"),
    isPortable: readBooleanField(payload, "isPortable"),
    hasUpdate: readBooleanField(payload, "hasUpdate"),
    canPrepare: readBooleanField(payload, "canPrepare"),
    currentVersion: readStringField(payload, "currentVersion"),
    latestVersion: readStringField(payload, "latestVersion"),
    releaseTag: readStringField(payload, "releaseTag"),
    releaseName: readNullableStringField(payload, "releaseName"),
    publishedAt: readNullableStringField(payload, "publishedAt"),
    reason: readNullableStringField(payload, "reason"),
    checkedAtUnixSecs: readNumberField(payload, "checkedAtUnixSecs"),
  };
}

export function readUpdatePrepareResult(payload: unknown): UpdatePrepareResult {
  return {
    prepared: readBooleanField(payload, "prepared"),
    mode: readStringField(payload, "mode"),
    isPortable: readBooleanField(payload, "isPortable"),
    releaseTag: readStringField(payload, "releaseTag"),
    latestVersion: readStringField(payload, "latestVersion"),
    assetName: readStringField(payload, "assetName"),
    assetPath: readStringField(payload, "assetPath"),
    downloaded: readBooleanField(payload, "downloaded"),
  };
}

export function readPendingUpdateResult(payload: unknown): PendingUpdateResult | null {
  const source = asRecord(payload);
  if (!source) {
    return null;
  }

  return {
    prepared: true,
    mode: readStringField(source, "mode"),
    isPortable: readBooleanField(source, "isPortable"),
    releaseTag: readStringField(source, "releaseTag"),
    latestVersion: readStringField(source, "latestVersion"),
    assetName: readStringField(source, "assetName"),
    assetPath: readStringField(source, "assetPath"),
    downloaded: true,
    installerPath: readNullableStringField(source, "installerPath"),
    stagingDir: readNullableStringField(source, "stagingDir"),
    preparedAtUnixSecs: readNumberField(source, "preparedAtUnixSecs"),
  };
}

export function readUpdateActionResult(payload: unknown): UpdateActionResult {
  return {
    ok: readBooleanField(payload, "ok"),
    message: readStringField(payload, "message"),
  };
}

export function readUpdateStatusResult(payload: unknown): UpdateStatusResult {
  return {
    repo: readStringField(payload, "repo"),
    mode: readStringField(payload, "mode"),
    isPortable: readBooleanField(payload, "isPortable"),
    currentVersion: readStringField(payload, "currentVersion"),
    currentExePath: readStringField(payload, "currentExePath"),
    portableMarkerPath: readStringField(payload, "portableMarkerPath"),
    pending: readPendingUpdateResult(asRecord(payload)?.pending),
    lastCheck: asRecord(payload)?.lastCheck
      ? readUpdateCheckResult(asRecord(payload)?.lastCheck)
      : null,
    lastError: readNullableStringField(payload, "lastError"),
  };
}
