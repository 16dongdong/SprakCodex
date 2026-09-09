import { createAccountWebCommands } from "./transport-web-commands/account";
import { createLoginWebCommands } from "./transport-web-commands/login";
import { createMiscWebCommands } from "./transport-web-commands/misc";
import { createProxyProfilesWebCommands } from "./transport-web-commands/proxy-profiles";
import type { WebCommandDescriptor, WebRpcCaller } from "./transport-web-commands/shared";

export type { InvokeParams, WebCommandDescriptor } from "./transport-web-commands/shared";

export function createWebCommandMap(postWebRpc: WebRpcCaller): Record<string, WebCommandDescriptor> {
  return {
    service_observation_status: { rpcMethod: "directObservation/status" },
    service_observation_start: { rpcMethod: "directObservation/start" },
    service_observation_stop: { rpcMethod: "directObservation/stop" },
    service_session_routing_list: { rpcMethod: "sessionRouting/list" },
    service_session_routing_reset: { rpcMethod: "sessionRouting/reset" },
    service_session_routing_switch: { rpcMethod: "sessionRouting/switch" },
    service_session_routing_status: { rpcMethod: "sessionRouting/status" },
    service_session_routing_set_enabled: { rpcMethod: "sessionRouting/setEnabled" },
    service_session_routing_set_account_enabled: {
      rpcMethod: "sessionRouting/setAccountEnabled",
    },
    ...createMiscWebCommands(),
    ...createAccountWebCommands(postWebRpc),
    ...createProxyProfilesWebCommands(),
    ...createLoginWebCommands(),
  };
}
