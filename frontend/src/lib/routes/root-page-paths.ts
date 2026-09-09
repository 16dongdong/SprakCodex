export const ROOT_PAGE_PATHS = [
  "/",
  "/accounts",
  "/sessions",
  "/logs",
  "/settings",
] as const;

export type RootPagePath = (typeof ROOT_PAGE_PATHS)[number];
