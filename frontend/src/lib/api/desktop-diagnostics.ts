import { invoke } from "@/lib/api/transport";

export interface DesktopDiagnosticsSnapshot {
  debugMode: boolean;
  effectiveDebugMode: boolean;
  debugModeForced: boolean;
  fileLoggingEnabled: boolean;
  logDir: string;
  startupError: string | null;
}

export interface DesktopDiagnosticsSettingsPatch {
  debugMode?: boolean;
  fileLoggingEnabled?: boolean;
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function readDesktopDiagnosticsSnapshot(
  value: unknown,
): DesktopDiagnosticsSnapshot {
  const source = asRecord(value);
  return {
    debugMode: source.debugMode === true,
    effectiveDebugMode: source.effectiveDebugMode === true,
    debugModeForced: source.debugModeForced === true,
    fileLoggingEnabled: source.fileLoggingEnabled !== false,
    logDir: typeof source.logDir === "string" ? source.logDir : "",
    startupError:
      typeof source.startupError === "string" && source.startupError.trim()
        ? source.startupError
        : null,
  };
}

export async function getDesktopDiagnostics(): Promise<DesktopDiagnosticsSnapshot> {
  const result = await invoke<unknown>("app_diagnostics_settings_get");
  return readDesktopDiagnosticsSnapshot(result);
}

export async function setDesktopDiagnostics(
  patch: DesktopDiagnosticsSettingsPatch,
): Promise<DesktopDiagnosticsSnapshot> {
  const result = await invoke<unknown>("app_diagnostics_settings_set", {
    patch,
  });
  return readDesktopDiagnosticsSnapshot(result);
}

export function openDesktopDiagnosticsLogsDir(): Promise<unknown> {
  return invoke("app_diagnostics_open_logs_dir");
}
