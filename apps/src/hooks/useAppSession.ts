"use client";

import { useQuery } from "@tanstack/react-query";
import { appClient } from "@/lib/api/app-client";
import { useAppStore } from "@/lib/store/useAppStore";
import { useRuntimeCapabilities } from "@/hooks/useRuntimeCapabilities";
import type { AppRole, AppSessionResult } from "@/types";

export const APP_SESSION_QUERY_KEY = ["account-manager", "session", "current"] as const;

interface UseAppSessionOptions {
  enabled?: boolean;
}

export function isAdminRole(role: AppRole | string | null | undefined): boolean {
  return role === "admin" || role === "system_admin";
}

export function resolveSessionRole(
  session: AppSessionResult | null | undefined,
  isLoading = false,
  forceSystemAdmin = false,
): AppRole {
  if (forceSystemAdmin) return "system_admin";
  return session?.role ?? (isLoading ? "system_admin" : "member");
}

export function useAppSession(options: UseAppSessionOptions = {}) {
  const serviceStatus = useAppStore((state) => state.serviceStatus);
  const { canAccessManagementRpc } = useRuntimeCapabilities();
  const isServiceReady = canAccessManagementRpc && serviceStatus.connected;
  const isQueryEnabled = (options.enabled ?? true) && isServiceReady;

  const sessionQuery = useQuery<AppSessionResult>({
    queryKey: [...APP_SESSION_QUERY_KEY, serviceStatus.addr],
    queryFn: () => appClient.getCurrentSession(serviceStatus.addr),
    enabled: isQueryEnabled,
    staleTime: 30_000,
    retry: 1,
  });

  return {
    ...sessionQuery,
    isServiceReady,
    isSessionQueryEnabled: isQueryEnabled,
  };
}
