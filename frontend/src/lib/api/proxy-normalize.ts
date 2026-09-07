import type {
  AccountProxyUrlTestEntry,
  AccountProxyUrlTestListResult,
  CfStyleResult,
  CfStyleRunStatus,
  CfStyleStatus,
  CfStyleThroughputResult,
  ProxyDiagnosticTestEntry,
  ProxyDiagnosticTestListResult,
  ProxyProfile,
  ProxyProfileListResult,
  ProxyProfileUrlTestListResult,
  ProxyProfileUrlTestResult,
  ProxySpeedTestEntry,
  ProxySpeedTestListResult,
  ProxyTestJobState,
  ProxyTestPresetsResult,
  ThroughputDirection,
} from "@/types";

function asObject(payload: unknown): Record<string, unknown> {
  return payload && typeof payload === "object" && !Array.isArray(payload)
    ? (payload as Record<string, unknown>)
    : {};
}

function asArray(payload: unknown): unknown[] {
  return Array.isArray(payload) ? payload : [];
}

function asString(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function asBoolean(value: unknown, fallback = false): boolean {
  if (typeof value === "boolean") return value;
  if (typeof value === "number") return value !== 0;
  if (typeof value === "string") {
    const normalized = value.trim().toLowerCase();
    if (["1", "true", "yes", "on"].includes(normalized)) return true;
    if (["0", "false", "no", "off"].includes(normalized)) return false;
  }
  return fallback;
}

function toNullableNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function asInteger(value: unknown, fallback = 0, min = 0): number {
  const parsed = toNullableNumber(value);
  return parsed == null ? fallback : Math.max(min, Math.trunc(parsed));
}

export function normalizeProxyProfile(payload: unknown): ProxyProfile {
  const source = asObject(payload);
  return {
    id: asString(source.id),
    name: asString(source.name),
    proxyUrlRedacted:
      asString(source.proxyUrlRedacted ?? source.proxy_url_redacted) ||
      "<invalid>",
    scheme: asString(source.scheme) || null,
    host: asString(source.host) || null,
    port: toNullableNumber(source.port),
    enabled: asBoolean(source.enabled, true),
    status: asString(source.status) || "unchecked",
    lastError: asString(source.lastError ?? source.last_error) || null,
    lastUrlLatencyMs: toNullableNumber(
      source.lastUrlLatencyMs ?? source.last_url_latency_ms,
    ),
    lastDownloadMbps: toNullableNumber(
      source.lastDownloadMbps ?? source.last_download_mbps,
    ),
    lastUploadMbps: toNullableNumber(
      source.lastUploadMbps ?? source.last_upload_mbps,
    ),
    lastTestedAt: toNullableNumber(
      source.lastTestedAt ?? source.last_tested_at,
    ),
    ip: asString(source.ip) || null,
    countryCode: asString(source.countryCode ?? source.country_code) || null,
    countryName: asString(source.countryName ?? source.country_name) || null,
    regionName: asString(source.regionName ?? source.region_name) || null,
    cityName: asString(source.cityName ?? source.city_name) || null,
    geoCheckedAt: toNullableNumber(
      source.geoCheckedAt ?? source.geo_checked_at ?? source.lastTestedAt ?? source.last_tested_at,
    ),
    geoError: asString(source.geoError ?? source.geo_error ?? source.lastError ?? source.last_error) || null,
    asn: toNullableNumber(source.asn),
    asOrg: asString(source.asOrg ?? source.as_org) || null,
    isp: asString(source.isp) || null,
    asDomain: asString(source.asDomain ?? source.as_domain) || null,
    flagImgUrl: asString(source.flagImgUrl ?? source.flag_img_url) || null,
    flagEmoji: asString(source.flagEmoji ?? source.flag_emoji) || null,
    timezoneId: asString(source.timezoneId ?? source.timezone_id) || null,
    timezoneOffset: toNullableNumber(
      source.timezoneOffset ?? source.timezone_offset,
    ),
    timezoneUtc: asString(source.timezoneUtc ?? source.timezone_utc) || null,
    tagsJson: asString(source.tagsJson ?? source.tags_json) || null,
    notes: asString(source.notes) || null,
    accountsCount:
      toNullableNumber(source.accountsCount ?? source.accounts_count) ?? null,
    createdAt: asInteger(source.createdAt ?? source.created_at),
    updatedAt: asInteger(source.updatedAt ?? source.updated_at),
  };
}

export function normalizeProxyProfileListResult(
  payload: unknown,
): ProxyProfileListResult {
  const source = asObject(payload);
  return {
    items: asArray(source.items ?? payload).map(normalizeProxyProfile),
  };
}

export function normalizeProxyTestPresetsResult(
  payload: unknown,
): ProxyTestPresetsResult {
  const source = asObject(payload);
  const defaults = asObject(source.defaults);
  const uploadEndpoint = asObject(
    source.uploadEndpoint ?? source.upload_endpoint,
  );
  return {
    speedProviders: asArray(
      source.speedProviders ?? source.speed_providers,
    ).map((item) => {
      const provider = asObject(item);
      return {
        id: asString(provider.id),
        label: asString(provider.label),
        providerFamily: asString(
          provider.providerFamily ?? provider.provider_family,
        ),
        files: asArray(provider.files).map((fileItem) => {
          const file = asObject(fileItem);
          return {
            fileSizeId: asString(file.fileSizeId ?? file.file_size_id),
            downloadUrl: asString(file.downloadUrl ?? file.download_url),
            readLimitBytes: toNullableNumber(
              file.readLimitBytes ?? file.read_limit_bytes,
            ),
          };
        }),
      };
    }),
    fileSizes: asArray(source.fileSizes ?? source.file_sizes).map((item) => {
      const fileSize = asObject(item);
      return {
        id: asString(fileSize.id),
        label: asString(fileSize.label),
        bytes: asInteger(fileSize.bytes),
        warning: asBoolean(fileSize.warning),
      };
    }),
    defaults: {
      speedProviderId: asString(
        defaults.speedProviderId ?? defaults.speed_provider_id,
      ),
      fileSizeId: asString(defaults.fileSizeId ?? defaults.file_size_id),
      latencyPresetId: asString(
        defaults.latencyPresetId ?? defaults.latency_preset_id,
      ),
    },
    uploadEndpoint: {
      status: asString(uploadEndpoint.status),
      configured: asBoolean(uploadEndpoint.configured),
      source: asString(uploadEndpoint.source),
      url: asString(uploadEndpoint.url) || null,
    },
  };
}

export function normalizeProxyProfileUrlTestResult(
  payload: unknown,
): ProxyProfileUrlTestResult {
  const source = asObject(payload);
  return {
    id: asInteger(source.id),
    proxyProfileId:
      asString(source.proxyProfileId ?? source.proxy_profile_id) || "",
    status: asString(source.status) || "failed",
    urlLatencyMs: toNullableNumber(
      source.urlLatencyMs ?? source.url_latency_ms,
    ),
    statusCode: toNullableNumber(source.statusCode ?? source.status_code),
    testUrl: asString(source.testUrl ?? source.test_url),
    finalUrl: asString(source.finalUrl ?? source.final_url) || null,
    redirected: asBoolean(source.redirected),
    testedAt: asInteger(source.testedAt ?? source.tested_at),
    errorCode: asString(source.errorCode ?? source.error_code) || null,
    error: asString(source.error) || null,
  };
}

export function normalizeProxyProfileUrlTestListResult(
  payload: unknown,
): ProxyProfileUrlTestListResult {
  const source = asObject(payload);
  return {
    items: asArray(source.items ?? payload).map(
      normalizeProxyProfileUrlTestResult,
    ),
  };
}

function normalizeCfStyleThroughput(
  payload: unknown,
): CfStyleThroughputResult | null {
  if (!payload) return null;
  const source = asObject(payload);
  return {
    direction: asString(source.direction) as ThroughputDirection,
    finalMbps: toNullableNumber(source.finalMbps ?? source.final_mbps) ?? 0,
    rawFinalMbps:
      toNullableNumber(source.rawFinalMbps ?? source.raw_final_mbps) ?? 0,
    adjustedFinalMbps:
      toNullableNumber(
        source.adjustedFinalMbps ?? source.adjusted_final_mbps,
      ) ?? 0,
    avgMbps: toNullableNumber(source.avgMbps ?? source.avg_mbps) ?? 0,
    medianMbps:
      toNullableNumber(source.medianMbps ?? source.median_mbps) ?? 0,
    p90Mbps: toNullableNumber(source.p90Mbps ?? source.p90_mbps) ?? 0,
    maxMbps: toNullableNumber(source.maxMbps ?? source.max_mbps) ?? 0,
    totalBytes: asInteger(source.totalBytes ?? source.total_bytes),
    totalDurationMs: asInteger(
      source.totalDurationMs ?? source.total_duration_ms,
    ),
    runs: asArray(source.runs).map((item) => {
      const run = asObject(item);
      return {
        payloadBytes: asInteger(run.payloadBytes ?? run.payload_bytes),
        transferredBytes: asInteger(
          run.transferredBytes ?? run.transferred_bytes,
        ),
        totalDurationMs: asInteger(
          run.totalDurationMs ?? run.total_duration_ms,
        ),
        ttfbMs: toNullableNumber(run.ttfbMs ?? run.ttfb_ms),
        transferDurationMs: asInteger(
          run.transferDurationMs ?? run.transfer_duration_ms,
        ),
        rawMbps: toNullableNumber(run.rawMbps ?? run.raw_mbps) ?? 0,
        adjustedMbps:
          toNullableNumber(run.adjustedMbps ?? run.adjusted_mbps) ?? 0,
        status: asString(run.status) as CfStyleRunStatus,
        error: asString(run.error) || null,
      };
    }),
  };
}

function normalizeCfStyleResult(payload: unknown): CfStyleResult | null {
  if (!payload) return null;
  const source = asObject(payload);
  const endpoint = asObject(source.endpointInfo ?? source.endpoint_info);
  const usedProxy = asObject(source.usedProxy ?? source.used_proxy);
  const latency = asObject(source.latency);
  return {
    status: asString(source.status) as CfStyleStatus,
    startedAt: asString(source.startedAt ?? source.started_at),
    finishedAt: asString(source.finishedAt ?? source.finished_at),
    durationMs: asInteger(source.durationMs ?? source.duration_ms),
    errors: asArray(source.errors).map((item) => {
      const error = asObject(item);
      return {
        phase: asString(error.phase),
        message: asString(error.message),
      };
    }),
    endpointInfo: {
      observedIp: asString(endpoint.observedIp ?? endpoint.observed_ip) || null,
      observedCountry:
        asString(endpoint.observedCountry ?? endpoint.observed_country) || null,
      observedColo:
        asString(endpoint.observedColo ?? endpoint.observed_colo) || null,
    },
    usedProxy:
      Object.keys(usedProxy).length > 0
        ? {
            proxyUrlRedacted: asString(
              usedProxy.proxyUrlRedacted ?? usedProxy.proxy_url_redacted,
            ),
            proxyScheme: asString(
              usedProxy.proxyScheme ?? usedProxy.proxy_scheme,
            ),
            dnsNote: asString(usedProxy.dnsNote ?? usedProxy.dns_note),
          }
        : null,
    latency:
      Object.keys(latency).length > 0
        ? {
            rawSamplesMs: asArray(
              latency.rawSamplesMs ?? latency.raw_samples_ms,
            ).map((value) => toNullableNumber(value) ?? 0),
            minMs: toNullableNumber(latency.minMs ?? latency.min_ms) ?? 0,
            avgMs: toNullableNumber(latency.avgMs ?? latency.avg_ms) ?? 0,
            medianMs:
              toNullableNumber(latency.medianMs ?? latency.median_ms) ?? 0,
            p90Ms: toNullableNumber(latency.p90Ms ?? latency.p90_ms) ?? 0,
            p95Ms: toNullableNumber(latency.p95Ms ?? latency.p95_ms) ?? 0,
            jitterMs:
              toNullableNumber(latency.jitterMs ?? latency.jitter_ms) ?? 0,
          }
        : null,
    download: normalizeCfStyleThroughput(source.download),
    upload: normalizeCfStyleThroughput(source.upload),
  };
}

export function normalizeProxyTestJobState(
  payload: unknown,
): ProxyTestJobState {
  const source = asObject(payload);
  const downloadSummary = asObject(
    source.downloadSummary ?? source.download_summary,
  );
  const uploadSummary = asObject(source.uploadSummary ?? source.upload_summary);
  const normalizeSummary = (summary: Record<string, unknown>) =>
    Object.keys(summary).length > 0
      ? {
          median: toNullableNumber(summary.median) ?? 0,
          average: toNullableNumber(summary.average) ?? 0,
          p90: toNullableNumber(summary.p90) ?? 0,
          best: toNullableNumber(summary.best) ?? 0,
        }
      : null;
  return {
    jobId: asString(source.jobId ?? source.job_id),
    scope:
      (asString(source.scope) as ProxyTestJobState["scope"]) || "system_proxy",
    proxyProfileId:
      asString(source.proxyProfileId ?? source.proxy_profile_id) || null,
    accountId: asString(source.accountId ?? source.account_id) || null,
    kind: (asString(source.kind) as ProxyTestJobState["kind"]) || "latency",
    status:
      (asString(source.status) as ProxyTestJobState["status"]) || "queued",
    phase: (asString(source.phase) as ProxyTestJobState["phase"]) || "queued",
    downloadedBytes: asInteger(
      source.downloadedBytes ?? source.downloaded_bytes,
    ),
    uploadedBytes: asInteger(source.uploadedBytes ?? source.uploaded_bytes),
    downloadMbps: toNullableNumber(
      source.downloadMbps ?? source.download_mbps,
    ),
    uploadMbps: toNullableNumber(source.uploadMbps ?? source.upload_mbps),
    latencyMs: toNullableNumber(source.latencyMs ?? source.latency_ms),
    startedAt: asInteger(source.startedAt ?? source.started_at),
    updatedAt: asInteger(source.updatedAt ?? source.updated_at),
    error: asString(source.error) || null,
    observedIp: asString(source.observedIp ?? source.observed_ip) || null,
    observedCountry:
      asString(source.observedCountry ?? source.observed_country) || null,
    observedColo:
      asString(source.observedColo ?? source.observed_colo) || null,
    downloadSamples: asArray(
      source.downloadSamples ?? source.download_samples,
    ).map((item) => {
      const sample = asObject(item);
      return {
        payloadBytes: asInteger(sample.payloadBytes ?? sample.payload_bytes),
        durationMs: asInteger(sample.durationMs ?? sample.duration_ms),
        mbps: toNullableNumber(sample.mbps) ?? 0,
      };
    }),
    uploadSamples: asArray(
      source.uploadSamples ?? source.upload_samples,
    ).map((item) => {
      const sample = asObject(item);
      return {
        payloadBytes: asInteger(sample.payloadBytes ?? sample.payload_bytes),
        durationMs: asInteger(sample.durationMs ?? sample.duration_ms),
        mbps: toNullableNumber(sample.mbps) ?? 0,
      };
    }),
    downloadSummary: normalizeSummary(downloadSummary),
    uploadSummary: normalizeSummary(uploadSummary),
    downloadDiagnostics: asArray(
      source.downloadDiagnostics ?? source.download_diagnostics,
    ).map((item) => {
      const diagnostic = asObject(item);
      return {
        providerId: asString(
          diagnostic.providerId ?? diagnostic.provider_id,
        ),
        fileSizeId: asString(
          diagnostic.fileSizeId ?? diagnostic.file_size_id,
        ),
        status: asString(diagnostic.status),
        error: asString(diagnostic.error) || null,
        downloadedBytes: asInteger(
          diagnostic.downloadedBytes ?? diagnostic.downloaded_bytes,
        ),
        durationMs: asInteger(
          diagnostic.durationMs ?? diagnostic.duration_ms,
        ),
        mbps: toNullableNumber(diagnostic.mbps) ?? 0,
      };
    }),
    cfStyleResult: normalizeCfStyleResult(
      source.cfStyleResult ?? source.cf_style_result,
    ),
  };
}

export function normalizeProxySpeedTestEntry(
  payload: unknown,
): ProxySpeedTestEntry {
  const source = asObject(payload);
  return {
    id: asInteger(source.id),
    scope: asString(source.scope),
    proxyProfileId:
      asString(source.proxyProfileId ?? source.proxy_profile_id) || null,
    accountId: asString(source.accountId ?? source.account_id) || null,
    status: asString(source.status) || "failed",
    provider: asString(source.provider),
    observedIp: asString(source.observedIp ?? source.observed_ip) || null,
    observedCountry:
      asString(source.observedCountry ?? source.observed_country) || null,
    observedColo:
      asString(source.observedColo ?? source.observed_colo) || null,
    maxPayloadBytes: toNullableNumber(
      source.maxPayloadBytes ?? source.max_payload_bytes,
    ),
    samplesJson: asString(source.samplesJson ?? source.samples_json) || null,
    downloadSummaryJson:
      asString(source.downloadSummaryJson ?? source.download_summary_json) ||
      null,
    uploadSummaryJson:
      asString(source.uploadSummaryJson ?? source.upload_summary_json) || null,
    startedAt: asInteger(source.startedAt ?? source.started_at),
    finishedAt: asInteger(source.finishedAt ?? source.finished_at),
    errorCode: asString(source.errorCode ?? source.error_code) || null,
    error: asString(source.error) || null,
  };
}

export function normalizeProxySpeedTestListResult(
  payload: unknown,
): ProxySpeedTestListResult {
  const source = asObject(payload);
  return {
    items: asArray(source.items ?? payload).map(normalizeProxySpeedTestEntry),
  };
}

export function normalizeProxyDiagnosticTestEntry(
  payload: unknown,
): ProxyDiagnosticTestEntry {
  const source = asObject(payload);
  return {
    id: asInteger(source.id),
    scope: asString(source.scope),
    proxyProfileId:
      asString(source.proxyProfileId ?? source.proxy_profile_id) || null,
    accountId: asString(source.accountId ?? source.account_id) || null,
    status: asString(source.status) || "failed",
    provider: asString(source.provider),
    fileSizeId: asString(source.fileSizeId ?? source.file_size_id),
    downloadedBytes: toNullableNumber(
      source.downloadedBytes ?? source.downloaded_bytes,
    ),
    durationMs: toNullableNumber(source.durationMs ?? source.duration_ms),
    mbps: toNullableNumber(source.mbps),
    testedAt: asInteger(source.testedAt ?? source.tested_at),
    error: asString(source.error) || null,
  };
}

export function normalizeProxyDiagnosticTestListResult(
  payload: unknown,
): ProxyDiagnosticTestListResult {
  const source = asObject(payload);
  return {
    items: asArray(source.items ?? payload).map(
      normalizeProxyDiagnosticTestEntry,
    ),
  };
}

export function normalizeAccountProxyUrlTestEntry(
  payload: unknown,
): AccountProxyUrlTestEntry {
  const source = asObject(payload);
  return {
    id: asInteger(source.id),
    accountId: asString(source.accountId ?? source.account_id),
    status: asString(source.status) || "failed",
    urlLatencyMs: toNullableNumber(
      source.urlLatencyMs ?? source.url_latency_ms,
    ),
    statusCode: toNullableNumber(source.statusCode ?? source.status_code),
    testUrl: asString(source.testUrl ?? source.test_url),
    finalUrl: asString(source.finalUrl ?? source.final_url) || null,
    redirected: asBoolean(source.redirected),
    testedAt: asInteger(source.testedAt ?? source.tested_at),
    errorCode: asString(source.errorCode ?? source.error_code) || null,
    error: asString(source.error) || null,
  };
}

export function normalizeAccountProxyUrlTestListResult(
  payload: unknown,
): AccountProxyUrlTestListResult {
  const source = asObject(payload);
  return {
    items: asArray(source.items ?? payload).map(
      normalizeAccountProxyUrlTestEntry,
    ),
  };
}
