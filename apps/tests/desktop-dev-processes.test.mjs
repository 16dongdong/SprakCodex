import assert from "node:assert/strict";
import test from "node:test";

import {
  findDesktopDevProcess,
  parsePositivePids,
  parsePosixProcessInfo,
  parsePosixProcessTreePids,
  parseSsListenerPids,
  parseWindowsListenerPids,
  terminatePosixProcessTree,
} from "../src-tauri/scripts/desktop-dev-processes.mjs";

const FRONTEND_DIR = "/workspace/Codex-Manager/apps";

function processInfo(
  pid,
  parentPid,
  commandLine,
  workingDirectory = FRONTEND_DIR,
) {
  return {
    ProcessId: pid,
    ParentProcessId: parentPid,
    CommandLine: commandLine,
    WorkingDirectory: workingDirectory,
  };
}

function processInfoReader(entries) {
  const processes = new Map(entries.map((item) => [item.ProcessId, item]));
  return (pid) => processes.get(pid) ?? null;
}

test("PID parsers keep only exact positive integers and remove duplicates", () => {
  assert.deepEqual(parsePositivePids("12\n12\n0\n-1\n34abc\n56\n"), [12, 56]);
});

test("Windows netstat parser selects the expected IPv4 LISTENING socket", () => {
  const output = [
    "TCP    127.0.0.1:3006    0.0.0.0:0    LISTENING    4100",
    "TCP    127.0.0.1:3005    0.0.0.0:0    LISTENING    4200",
    "TCP    127.0.0.1:30060   0.0.0.0:0    LISTENING    4250",
    "TCP    [::1]:3006        [::]:0       LISTENING    4300",
    "TCP    127.0.0.1:3006    127.0.0.1:50000 ESTABLISHED 4400",
  ].join("\r\n");

  assert.deepEqual(parseWindowsListenerPids(output, "127.0.0.1", 3006), [4100]);
});

test("Linux ss parser ignores other hosts and deduplicates listener PIDs", () => {
  const output = [
    'LISTEN 0 511 127.0.0.1:3006 0.0.0.0:* users:(("next-server",pid=5100,fd=24))',
    'LISTEN 0 511 127.0.0.1:3006 0.0.0.0:* users:(("next-server",pid=5100,fd=25))',
    'LISTEN 0 511 [::1]:3006 [::]:* users:(("other",pid=5200,fd=10))',
    'LISTEN 0 511 127.0.0.1:3005 0.0.0.0:* users:(("proxy",pid=5300,fd=11))',
  ].join("\n");

  assert.deepEqual(parseSsListenerPids(output, "127.0.0.1", 3006), [5100]);
});

test("POSIX process parser preserves parent, command, and working directory", () => {
  assert.deepEqual(
    parsePosixProcessInfo(
      "  6000 node ./node_modules/next/dist/bin/next dev -p 3006\n",
      6100,
      FRONTEND_DIR,
    ),
    processInfo(
      6100,
      6000,
      "node ./node_modules/next/dist/bin/next dev -p 3006",
    ),
  );
});

test("desktop dev process lookup recognizes a Next CLI ancestor", () => {
  const readProcessInfo = processInfoReader([
    processInfo(7100, 7000, "node internal-next-listener"),
    processInfo(
      7000,
      1,
      "node ./node_modules/next/dist/bin/next dev --webpack --port 3006",
    ),
  ]);

  const match = findDesktopDevProcess(7100, {
    port: 3006,
    frontendDir: FRONTEND_DIR,
    platform: "linux",
    processInfoReader: readProcessInfo,
  });
  assert.equal(match?.pid, 7000);
});

test("desktop dev process lookup recognizes an orphan Next server from this frontend", () => {
  const match = findDesktopDevProcess(8100, {
    port: 3006,
    frontendDir: FRONTEND_DIR,
    platform: "linux",
    processInfoReader: processInfoReader([
      processInfo(8100, 1, "next-server (v16.1.6)"),
    ]),
  });
  assert.equal(match?.pid, 8100);
});

test("desktop dev process lookup rejects unrelated and other-project listeners", () => {
  const unrelated = findDesktopDevProcess(9100, {
    port: 3006,
    frontendDir: FRONTEND_DIR,
    platform: "linux",
    processInfoReader: processInfoReader([
      processInfo(9100, 1, 'node -e require("net").createServer().listen(3006)'),
    ]),
  });
  assert.equal(unrelated, null);

  const otherProject = findDesktopDevProcess(9200, {
    port: 3006,
    frontendDir: FRONTEND_DIR,
    platform: "linux",
    processInfoReader: processInfoReader([
      processInfo(
        9200,
        1,
        "node ./node_modules/next/dist/bin/next dev -p 3006",
        "/workspace/other-app",
      ),
    ]),
  });
  assert.equal(otherProject, null);
});

test("POSIX process tree parser orders descendants before their parent", () => {
  const output = ["100 1", "110 100", "120 100", "111 110", "900 1"].join("\n");
  assert.deepEqual(parsePosixProcessTreePids(output, 100), [111, 110, 120, 100]);
});

test("POSIX termination tolerates exited processes but reports permission errors", () => {
  const signaled = [];
  const exitedProcessResult = terminatePosixProcessTree(100, {
    processTreeReader: () => [111, 110, 100],
    killProcess: (pid, signal) => {
      signaled.push([pid, signal]);
      if (pid === 110) {
        const error = new Error("already exited");
        error.code = "ESRCH";
        throw error;
      }
    },
  });
  assert.equal(exitedProcessResult, true);
  assert.deepEqual(signaled, [
    [111, "SIGTERM"],
    [110, "SIGTERM"],
    [100, "SIGTERM"],
  ]);

  const permissionResult = terminatePosixProcessTree(100, {
    processTreeReader: () => [100],
    killProcess: () => {
      const error = new Error("not permitted");
      error.code = "EPERM";
      throw error;
    },
  });
  assert.equal(permissionResult, false);
});
