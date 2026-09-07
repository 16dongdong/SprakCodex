import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const appsRoot = path.resolve(import.meta.dirname, "..");

test("account client exposes one RPC call for batch sort updates", async () => {
  const source = await fs.readFile(
    path.join(appsRoot, "src", "lib", "api", "account-client.ts"),
    "utf8",
  );

  assert.match(
    source,
    /updateSorts:\s*\(updates:[\s\S]*?invoke\(\s*["']service_account_update_sorts["']/,
  );
  assert.match(source, /updates:\s*updates\.map\(\(update\) => \(\{/);
});

test("accounts hook reorders accounts through the batch sort API", async () => {
  const source = await fs.readFile(
    path.join(appsRoot, "src", "hooks", "useAccounts.ts"),
    "utf8",
  );

  const mutation = source.match(
    /const reorderAccountsMutation = useMutation\(\{[\s\S]*?const updateAccountProfileMutation/,
  )?.[0] || "";
  assert.match(mutation, /await accountClient\.updateSorts\(updates\)/);
  assert.doesNotMatch(mutation, /for \(const update of updates\)/);
});

test("desktop import picker results are not imported a second time by the web client", async () => {
  const source = await fs.readFile(
    path.join(appsRoot, "src", "lib", "api", "account-client.ts"),
    "utf8",
  );

  assert.match(
    source,
    /if \(picked\?\.canceled \|\| !Array\.isArray\(picked\?\.contents\) \|\| picked\.contents\.length === 0\) \{\s*return picked;\s*\}/,
  );
  assert.match(source, /const imported = await importAccountContents\(picked\.contents\)/);
});
