export const ROOT_PAGE_PATHS = [
  "/",
  "/accounts",
  "/account-manager",
  "/aggregate-api",
  "/apikeys",
  "/projects",
  "/models",
  "/model-groups",
  "/plugins",
  "/skills",
  "/logs",
  "/settings",
  "/author",
] as const;

export type RootPagePath = (typeof ROOT_PAGE_PATHS)[number];
