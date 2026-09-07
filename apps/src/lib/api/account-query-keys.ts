export type AccountQueryServiceAddress = string | null | undefined;

export function normalizeQueryServiceAddress(
  addr: AccountQueryServiceAddress,
): string | null {
  const normalized = addr?.trim();
  return normalized || null;
}

export function buildAccountListQueryKey(addr: AccountQueryServiceAddress) {
  return ["accounts", "list", normalizeQueryServiceAddress(addr)] as const;
}

export function buildAccountLookupQueryKey(addr: AccountQueryServiceAddress) {
  return ["accounts", "lookup", normalizeQueryServiceAddress(addr)] as const;
}

export function buildAccountUsageListQueryKey(addr: AccountQueryServiceAddress) {
  return ["usage", "list", normalizeQueryServiceAddress(addr)] as const;
}

export function buildApiKeyListQueryKey(addr: AccountQueryServiceAddress) {
  return ["apikeys", "list", normalizeQueryServiceAddress(addr)] as const;
}

export function buildApiKeyLookupQueryKey(addr: AccountQueryServiceAddress) {
  return ["apikeys", "lookup", normalizeQueryServiceAddress(addr)] as const;
}

export function buildManagedModelSelectorQueryKey(
  addr: AccountQueryServiceAddress,
) {
  return [
    "managed-models-v2",
    "selector",
    normalizeQueryServiceAddress(addr),
  ] as const;
}

export function buildAggregateApiListQueryKey(addr: AccountQueryServiceAddress) {
  return [
    "aggregate-apis",
    "list",
    normalizeQueryServiceAddress(addr),
  ] as const;
}

export function buildManagedModelListQueryKey(
  addr: AccountQueryServiceAddress,
  includeHidden: boolean,
) {
  return [
    "managed-models-v2",
    "list",
    normalizeQueryServiceAddress(addr),
    includeHidden,
  ] as const;
}

export function buildModelGroupListQueryKey(addr: AccountQueryServiceAddress) {
  return ["model-groups", "list", normalizeQueryServiceAddress(addr)] as const;
}

export function buildModelGroupUsersQueryKey(addr: AccountQueryServiceAddress) {
  return ["model-groups", "users", normalizeQueryServiceAddress(addr)] as const;
}
