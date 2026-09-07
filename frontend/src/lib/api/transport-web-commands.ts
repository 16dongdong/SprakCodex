import { createAccountWebCommands } from "./transport-web-commands/account";
import { createAggregateApiWebCommands } from "./transport-web-commands/aggregate-api";
import { createApiKeyWebCommands } from "./transport-web-commands/apikey";
import { createCodexProfileWebCommands } from "./transport-web-commands/codex-profile";
import { createCodexSkillsWebCommands } from "./transport-web-commands/codex-skills";
import { createGatewayWebCommands } from "./transport-web-commands/gateway";
import { createLoginWebCommands } from "./transport-web-commands/login";
import { createMiscWebCommands } from "./transport-web-commands/misc";
import { createProxyProfilesWebCommands } from "./transport-web-commands/proxy-profiles";
import { createQuotaWebCommands } from "./transport-web-commands/quota";
import type { WebCommandDescriptor, WebRpcCaller } from "./transport-web-commands/shared";

export type { InvokeParams, WebCommandDescriptor } from "./transport-web-commands/shared";

export function createWebCommandMap(postWebRpc: WebRpcCaller): Record<string, WebCommandDescriptor> {
  return {
    service_observation_status: { rpcMethod: "directObservation/status" },
    service_observation_start: { rpcMethod: "directObservation/start" },
    service_observation_stop: { rpcMethod: "directObservation/stop" },
    ...createMiscWebCommands(),
    ...createCodexProfileWebCommands(),
    ...createCodexSkillsWebCommands(),
    ...createAccountWebCommands(postWebRpc),
    ...createProxyProfilesWebCommands(),
    ...createQuotaWebCommands(),
    ...createAggregateApiWebCommands(),
    ...createLoginWebCommands(),
    ...createApiKeyWebCommands(),
    ...createGatewayWebCommands(),
  };
}
