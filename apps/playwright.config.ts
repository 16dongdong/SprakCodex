import { defineConfig } from "@playwright/test";

const PORT = 3200;
const LOCAL_TEST_HOST = "localhost";
const chromiumExecutablePath =
  process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH?.trim() || undefined;

const noProxyHosts = ["localhost", "127.0.0.1", "::1"];
const noProxy = [process.env.NO_PROXY, process.env.no_proxy, ...noProxyHosts]
  .filter(Boolean)
  .join(",");

process.env.NO_PROXY = noProxy;
process.env.no_proxy = noProxy;

export default defineConfig({
  testDir: "./tests",
  testMatch: "**/*.spec.ts",
  timeout: 30_000,
  fullyParallel: false,
  use: {
    baseURL: `http://${LOCAL_TEST_HOST}:${PORT}`,
    trace: "on-first-retry",
    video: chromiumExecutablePath ? "off" : "retain-on-failure",
    launchOptions: chromiumExecutablePath
      ? { executablePath: chromiumExecutablePath }
      : undefined,
  },
  webServer: {
    command: "pnpm run build:desktop && node tests/support/static-server.mjs",
    url: `http://${LOCAL_TEST_HOST}:${PORT}`,
    reuseExistingServer: false,
    timeout: 120_000,
    env: {
      NO_PROXY: noProxy,
      PORT: String(PORT),
      no_proxy: noProxy,
    },
  },
});
