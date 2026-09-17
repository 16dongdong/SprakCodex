// 响应式回归只使用确定性本地数据；不连接真实服务，也不读取用户账号或密钥。
const layoutSettings = {
  updateAutoCheck: true,
  closeToTrayOnClose: false,
  closeToTraySupported: false,
  lowTransparency: false,
  lightweightModeOnCloseToTray: false,
  codexCliGuideDismissed: true,
  webAccessPasswordConfigured: false,
  locale: "zh-CN",
  localeOptions: ["zh-CN", "en"],
  serviceAddr: "localhost:48760",
  serviceListenMode: "loopback",
  serviceListenModeOptions: ["loopback", "all_interfaces"],
  routeStrategy: "ordered",
  routeStrategyOptions: ["ordered", "balanced"],
  freeAccountMaxModel: "auto",
  freeAccountMaxModelOptions: ["auto", "gpt-5"],
  modelForwardRules: "",
  accountMaxInflight: 1,
  gatewayOriginator: "codex-cli",
  gatewayOriginatorDefault: "codex-cli",
  gatewayUserAgentVersion: "1.0.0",
  gatewayUserAgentVersionDefault: "1.0.0",
  gatewayResidencyRequirement: "",
  gatewayResidencyRequirementOptions: ["", "us"],
  pluginMarketMode: "builtin",
  pluginMarketSourceUrl: "",
  upstreamProxyUrl: "",
  upstreamStreamTimeoutMs: 600000,
  sseKeepaliveIntervalMs: 15000,
  backgroundTasks: {
    usagePollingEnabled: true,
    usagePollIntervalSecs: 600,
    gatewayKeepaliveEnabled: true,
    gatewayKeepaliveIntervalSecs: 180,
    tokenRefreshPollingEnabled: true,
    tokenRefreshPollIntervalSecs: 60,
    usageRefreshWorkers: 4,
    httpWorkerFactor: 4,
    httpWorkerMin: 8,
    httpStreamWorkerFactor: 1,
    httpStreamWorkerMin: 2,
  },
  envOverrides: {},
  envOverrideCatalog: [],
  envOverrideReservedKeys: [],
  envOverrideUnsupportedKeys: [],
  theme: "tech",
  appearancePreset: "classic",
};

export const layoutAccountName = "layout-very-long-account-name@example.test";
export const layoutSupplierName = "用于检查长文本换行的上游供应商名称";

// 在浏览器测试和交互探针中安装相同协议样本；page 是独立测试页面，意外 RPC 返回错误而非访问真实上游。
export async function installLayoutFixture(page) {
  await page.route("**/api/events/**", route => route.fulfill({contentType:"text/event-stream",body:": 布局测试\n\n"}));
  await page.route("**/api/runtime**", route => route.fulfill({ json: {
    mode: "web-gateway", rpcBaseUrl: "/api/rpc", canUseBrowserFileImport: true, canUseBrowserDownloadExport: true
  } }));
  await page.route("**/api/rpc**", async route => {
    const requestBody = route.request().postDataJSON();
    const responses = {
      "appSettings/get": layoutSettings,
      "initialize": {version: "0.6.0", userAgent: "codex_cli_rs/test", codexHome: "C:/Test", platformFamily: "windows", platformOs: "windows"},
      "accountManager/session/current": {mode:"none",role:"system_admin",currentUser:null,permissions:["system:admin"]},
      "account/list": {items:[{id:"layout-account",name:layoutAccountName,label:layoutAccountName,planType:"pro",status:"active",sort:0}],total:1,page:1,pageSize:20},
      "account/usage/list": [{accountId:"layout-account",availabilityStatus:"available",usedPercent:20,windowMinutes:300,resetsAt:1900000000,secondaryUsedPercent:50,secondaryWindowMinutes:10080,secondaryResetsAt:1900604800}],
      "aggregateApi/list": {items:[{id:"layout-upstream",supplierName:layoutSupplierName,providerType:"compatible",url:"https://example.test/very/long/upstream/endpoint",status:"active",modelSlugs:["gpt-test-very-long-model-name"],createdAt:1900000000}]},
      "apikey/list": {items:[]},
      "apikey/usageStats": [],
      "apikey/managedModelListV2": {items:[],stats:{total:0,enabled:0,builtin:0,custom:0,priceMissing:0,missingRoute:0}},
      "requestlog/list": {items:[],total:0,page:1,pageSize:20},
      "requestlog/list_with_summary": {items:[],total:0,page:1,pageSize:20,summary:{totalCount:0,filteredCount:0,successCount:0,errorCount:0,totalTokens:0}},
      "requestlog/summary": {totalCount:0,filteredCount:0,successCount:0,errorCount:0,totalTokens:0},
      "codexProfile/get": {},
      "codexProfile/listCandidates": {items:[]},
      "gateway/concurrencyRecommendation/get": {usageRefreshWorkers:4,httpWorkerFactor:4,httpWorkerMin:8,httpStreamWorkerFactor:1,httpStreamWorkerMin:2,accountMaxInflight:1}
    };
    if (requestBody.method in responses) {
      return route.fulfill({json:{jsonrpc:"2.0",id:requestBody.id,result:responses[requestBody.method]}});
    }
    return route.fulfill({json:{jsonrpc:"2.0",id:requestBody.id,error:{code:-32601,message:`布局样本未定义接口：${requestBody.method}`}}});
  });
}
