import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const appsRoot = path.resolve(import.meta.dirname, "..");

async function readSource(relativePath) {
  return fs.readFile(path.join(appsRoot, relativePath), "utf8");
}

test("登录 API 使用 Device Code 判别类型并提供取消命令", async () => {
  const [types, client, normalize, auth] = await Promise.all([
    readSource("src/types/auth.ts"),
    readSource("src/lib/api/account-client.ts"),
    readSource("src/lib/api/normalize.ts"),
    readSource("src/lib/api/account-auth.ts"),
  ]);

  assert.match(
    types,
    /export type LoginType = "chatgpt" \| "chatgptDeviceCode"/,
  );
  assert.match(types, /type: "chatgptDeviceCode"/);
  assert.match(types, /type: "chatgpt"/);
  assert.match(types, /"cancelled"/);
  assert.match(types, /"expired"/);
  assert.match(client, /loginType: LoginType/);
  assert.match(
    client,
    /invoke<unknown>\("service_login_cancel", withAddr\(\{ loginId \}\)\)/,
  );
  assert.match(normalize, /if \(type === "chatgptDeviceCode"\)/);
  assert.match(auth, /rawStatus === "cancelled"/);
  assert.match(auth, /rawStatus === "expired"/);
});

test("新增账号弹窗完整处理 Device Code 生命周期", async () => {
  const source = await readSource(
    "src/components/modals/add-account-modal.tsx",
  );

  assert.match(source, /<SelectItem value="chatgpt">/);
  assert.match(source, /<SelectItem value="chatgptDeviceCode">/);
  assert.match(
    source,
    /value === "chatgptDeviceCode"[\s\S]*?t\("设备码登录"\)[\s\S]*?t\("浏览器登录"\)/,
  );
  assert.match(source, /loginType: requestedLoginType/);
  assert.match(source, /openBrowser: requestedLoginType === "chatgpt"/);
  assert.match(source, /BROWSER_LOGIN_TIMEOUT_MS = 15 \* 60 \* 1000/);
  assert.match(source, /DEVICE_CODE_LOGIN_TIMEOUT_MS = 15 \* 60 \* 1000/);
  assert.match(source, /LOGIN_COMPLETION_GRACE_MS = 5 \* 60 \* 1000/);
  assert.match(source, /status === "completing"/);
  assert.match(source, /completionGraceApplied/);
  assert.match(source, /accountClient\.cancelLogin\(loginId\)/);
  assert.match(source, /activeLoginIdRef/);
  assert.match(
    source,
    /const stopActiveLogin = useCallback\(\(\) => \{[\s\S]*?setIsLoading\(false\);[\s\S]*?setIsPollingLogin\(false\);/,
  );
  assert.match(
    source,
    /useEffect\(\s*\(\) => \(\) => \{[\s\S]*?activeLoginIdRef\.current = "";[\s\S]*?cancelLoginSession\(loginId\);/,
  );
  assert.match(source, /operationToken !== loginPollTokenRef\.current/);
  assert.match(source, /copyUserCode/);
  assert.match(source, /openLoginUrl/);
  assert.match(source, /appClient\.openInBrowser\(loginUrl\)/);
  assert.match(source, /loginType === "chatgpt" \? \([\s\S]*手动解析回调/);
});

test("浏览器登录恢复自动回调已成功的手动解析竞争", async () => {
  const source = await readSource(
    "src/components/modals/add-account-modal.tsx",
  );

  assert.match(
    source,
    /await accountClient\.getLoginStatus\(callbackState\)/,
  );
  assert.match(
    source,
    /recoveredStatus\.status === "success"[\s\S]*?completeLoginSuccess\(t\("登录成功"\), operationToken\)/,
  );
  assert.match(
    source,
    /recoveredStatus\.status === "completing"[\s\S]*?waitForLogin\(/,
  );
  assert.match(source, /手动解析回调（仅在自动回调未完成时使用）/);
  assert.doesNotMatch(source, /手动解析回调（当本地 48760 端口占用时）/);
});
