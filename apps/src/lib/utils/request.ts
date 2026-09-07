export interface RequestOptions {
  signal?: AbortSignal;
  timeoutMs?: number;
  retries?: number;
  retryDelayMs?: number;
  maxRetryDelayMs?: number;
  shouldRetry?: (error: unknown) => boolean;
  shouldRetryStatus?: (status: number) => boolean;
}

function createAbortError(): Error {
  if (typeof DOMException !== "undefined") {
    return new DOMException("Request aborted", "AbortError");
  }
  const error = new Error("Request aborted");
  error.name = "AbortError";
  return error;
}

function throwIfAborted(signal: AbortSignal | undefined): void {
  if (signal?.aborted) {
    throw createAbortError();
  }
}

function waitForDelay(ms: number, signal?: AbortSignal): Promise<void> {
  if (ms <= 0) {
    return Promise.resolve();
  }
  throwIfAborted(signal);
  return new Promise((resolve, reject) => {
    const id = setTimeout(() => {
      signal?.removeEventListener("abort", abort);
      resolve();
    }, ms);
    const abort = () => {
      clearTimeout(id);
      signal?.removeEventListener("abort", abort);
      reject(createAbortError());
    };
    signal?.addEventListener("abort", abort, { once: true });
  });
}

function runWithAbortSignal<T>(
  fn: () => Promise<T>,
  signal?: AbortSignal,
): Promise<T> {
  throwIfAborted(signal);
  if (!signal) {
    return fn();
  }
  return new Promise((resolve, reject) => {
    const abort = () => {
      signal.removeEventListener("abort", abort);
      reject(createAbortError());
    };
    signal.addEventListener("abort", abort, { once: true });
    fn()
      .then(resolve, reject)
      .finally(() => {
        signal.removeEventListener("abort", abort);
      });
  });
}

/**
 * 函数 `fetchWithRetry`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - url: 参数 url
 * - init?: 参数 init?
 * - options: 参数 options
 *
 * # 返回
 * 返回函数执行结果
 */
export async function fetchWithRetry(
  url: string,
  init?: RequestInit,
  options: RequestOptions = {}
): Promise<Response> {
  const {
    timeoutMs = 10000,
    retries = 3,
    retryDelayMs = 200,
    maxRetryDelayMs = 3000,
    shouldRetryStatus = (status) => status >= 500 || status === 429,
  } = options;

  let lastError: unknown;
  for (let i = 0; i <= retries; i++) {
    throwIfAborted(options.signal);
    const controller = new AbortController();
    const id = setTimeout(() => controller.abort(), timeoutMs);
    const abortFromParent = () => controller.abort();
    if (options.signal) {
      options.signal.addEventListener("abort", abortFromParent, { once: true });
    }

    try {
      const response = await fetch(url, {
        ...init,
        signal: controller.signal,
      });
      clearTimeout(id);

      if (response.ok || !shouldRetryStatus(response.status) || i === retries) {
        return response;
      }
    } catch (err: unknown) {
      lastError = err;
      if (err instanceof Error && err.name === "AbortError" && !options.signal?.aborted) {
        // Timeout retry
      } else if (i === retries) {
        throw err;
      }
    } finally {
      clearTimeout(id);
      options.signal?.removeEventListener("abort", abortFromParent);
    }

    const delay = Math.min(retryDelayMs * Math.pow(2, i), maxRetryDelayMs);
    await waitForDelay(delay, options.signal);
  }
  throw lastError || new Error("Fetch failed after retries");
}

/**
 * 函数 `runWithControl`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - fn: 参数 fn
 * - options: 参数 options
 *
 * # 返回
 * 返回函数执行结果
 */
export async function runWithControl<T>(
  fn: () => Promise<T>,
  options: RequestOptions = {}
): Promise<T> {
  const {
    retries = 0,
    retryDelayMs = 200,
    maxRetryDelayMs = 3000,
    shouldRetry = () => true,
  } = options;

  let lastError: unknown;
  for (let i = 0; i <= retries; i++) {
    try {
      return await runWithAbortSignal(fn, options.signal);
    } catch (err: unknown) {
      lastError = err;
      if (err instanceof Error && err.name === "AbortError") {
        throw err;
      }
      if (i === retries || !shouldRetry(err)) {
        throw err;
      }
    }
    const delay = Math.min(retryDelayMs * Math.pow(2, i), maxRetryDelayMs);
    await waitForDelay(delay, options.signal);
  }
  throw lastError;
}
