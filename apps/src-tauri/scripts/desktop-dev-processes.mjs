import { readlinkSync } from "node:fs";
import { resolve, sep } from "node:path";
import { spawnSync } from "node:child_process";

export function parsePositivePids(output) {
  return [
    ...new Set(
      String(output ?? "")
        .split(/\s+/)
        .filter((value) => /^\d+$/.test(value))
        .map((value) => Number.parseInt(value, 10))
        .filter((pid) => Number.isSafeInteger(pid) && pid > 0),
    ),
  ];
}

export function parseWindowsListenerPids(output, host, port) {
  const expectedAddress = `${host}:${port}`;
  const pids = new Set();

  for (const line of String(output ?? "").split(/\r?\n/)) {
    const columns = line.trim().split(/\s+/);
    if (
      columns[1] !== expectedAddress ||
      !columns.some((value) => /^LISTENING$/i.test(value))
    ) {
      continue;
    }

    const match = line.trim().match(/(\d+)$/);
    if (match) {
      pids.add(Number.parseInt(match[1], 10));
    }
  }

  return [...pids];
}

export function parseSsListenerPids(output, host, port) {
  const expectedAddress = `${host}:${port}`;
  return [
    ...new Set(
      String(output ?? "")
        .split(/\r?\n/)
        .filter((line) => line.trim().split(/\s+/)[3] === expectedAddress)
        .flatMap((line) =>
          [...line.matchAll(/\bpid=(\d+)\b/g)].map((match) =>
            Number.parseInt(match[1], 10),
          ),
        ),
    ),
  ];
}

function listWindowsListenerPids(port, host) {
  const result = spawnSync("netstat", ["-ano", "-p", "tcp"], {
    encoding: "utf8",
  });
  if (result.error || result.status !== 0) {
    return [];
  }
  return parseWindowsListenerPids(result.stdout, host, port);
}

function listPosixListenerPids(port, host, platform) {
  const lsofResult = spawnSync(
    "lsof",
    ["-nP", "-a", `-iTCP@${host}:${port}`, "-sTCP:LISTEN", "-t"],
    { encoding: "utf8" },
  );
  if (!lsofResult.error && lsofResult.status === 0) {
    return parsePositivePids(lsofResult.stdout);
  }

  if (platform === "linux") {
    const ssResult = spawnSync("ss", ["-ltnp", `sport = :${port}`], {
      encoding: "utf8",
    });
    if (!ssResult.error && ssResult.status === 0) {
      return parseSsListenerPids(ssResult.stdout, host, port);
    }
  }

  return [];
}

export function listDesktopDevListenerPids(
  port,
  host = "127.0.0.1",
  platform = process.platform,
) {
  return platform === "win32"
    ? listWindowsListenerPids(port, host)
    : listPosixListenerPids(port, host, platform);
}

function getWindowsProcessInfo(pid) {
  const command = [
    `$process = Get-CimInstance Win32_Process -Filter "ProcessId = ${pid}" -ErrorAction SilentlyContinue`,
    'if ($process) { $process | Select-Object ProcessId, ParentProcessId, CommandLine | ConvertTo-Json -Compress }',
  ].join("; ");
  const result = spawnSync("powershell.exe", ["-NoProfile", "-Command", command], {
    encoding: "utf8",
  });
  if (result.error || result.status !== 0 || !result.stdout.trim()) {
    return null;
  }

  try {
    return JSON.parse(result.stdout.trim());
  } catch {
    return null;
  }
}

function getPosixProcessWorkingDirectory(pid, platform) {
  if (platform === "linux") {
    try {
      return readlinkSync(`/proc/${pid}/cwd`);
    } catch {
      return null;
    }
  }

  const result = spawnSync(
    "lsof",
    ["-a", "-p", String(pid), "-d", "cwd", "-Fn"],
    { encoding: "utf8" },
  );
  if (result.error || result.status !== 0) {
    return null;
  }

  const pathLine = result.stdout
    .split(/\r?\n/)
    .find((line) => line.startsWith("n"));
  return pathLine?.slice(1).trim() || null;
}

export function parsePosixProcessInfo(output, pid, workingDirectory = null) {
  const match = String(output ?? "")
    .trim()
    .match(/^(\d+)\s+([\s\S]+)$/);
  if (!match) {
    return null;
  }

  return {
    ProcessId: pid,
    ParentProcessId: Number.parseInt(match[1], 10),
    CommandLine: match[2].trim(),
    WorkingDirectory: workingDirectory,
  };
}

function getPosixProcessInfo(pid, platform) {
  const result = spawnSync(
    "ps",
    ["-ww", "-p", String(pid), "-o", "ppid=", "-o", "command="],
    { encoding: "utf8" },
  );
  if (result.error || result.status !== 0) {
    return null;
  }

  return parsePosixProcessInfo(
    result.stdout,
    pid,
    getPosixProcessWorkingDirectory(pid, platform),
  );
}

export function getDesktopDevProcessInfo(pid, platform = process.platform) {
  return platform === "win32"
    ? getWindowsProcessInfo(pid)
    : getPosixProcessInfo(pid, platform);
}

export function isProcessOwnedByFrontend(processInfo, frontendDir, platform) {
  if (platform === "win32") {
    return true;
  }

  const normalizedFrontendDir = resolve(frontendDir);
  const workingDirectory = processInfo?.WorkingDirectory
    ? resolve(processInfo.WorkingDirectory)
    : null;
  if (
    workingDirectory &&
    (workingDirectory === normalizedFrontendDir ||
      workingDirectory.startsWith(`${normalizedFrontendDir}${sep}`))
  ) {
    return true;
  }

  return String(processInfo?.CommandLine ?? "").includes(normalizedFrontendDir);
}

export function findDesktopDevProcess(
  pid,
  {
    port,
    frontendDir,
    platform = process.platform,
    processInfoReader = (processPid) =>
      getDesktopDevProcessInfo(processPid, platform),
  },
) {
  let currentPid = pid;

  for (let index = 0; index < 4 && currentPid; index += 1) {
    const processInfo = processInfoReader(currentPid);
    if (!processInfo?.CommandLine) {
      break;
    }

    const normalizedCommandLine = processInfo.CommandLine.toLowerCase();
    const isNextServerProcess = /^next-server \(v[^)]+\)$/.test(
      normalizedCommandLine,
    );
    const isNextProcess =
      normalizedCommandLine.includes("next dev") ||
      normalizedCommandLine.includes("\\next\\dist\\bin\\next") ||
      normalizedCommandLine.includes("/next/dist/bin/next") ||
      normalizedCommandLine.includes("start-server.js") ||
      isNextServerProcess;
    const isDevProxyProcess =
      normalizedCommandLine.includes("before-build.mjs") &&
      normalizedCommandLine.includes("dev:desktop");
    const matchesDesktopPort =
      normalizedCommandLine.includes(`-p ${port}`) ||
      normalizedCommandLine.includes(`--port ${port}`) ||
      normalizedCommandLine.includes(`--port=${port}`) ||
      normalizedCommandLine.includes(`:${port}`);

    if (
      (isNextProcess || isDevProxyProcess) &&
      (matchesDesktopPort ||
        isDevProxyProcess ||
        isNextServerProcess ||
        index > 0) &&
      isProcessOwnedByFrontend(processInfo, frontendDir, platform)
    ) {
      return { pid: currentPid, processInfo };
    }

    currentPid = processInfo.ParentProcessId;
  }

  return null;
}

function terminateWindowsProcessTree(pid) {
  const result = spawnSync("taskkill", ["/PID", String(pid), "/T", "/F"], {
    encoding: "utf8",
  });
  if (result.error) {
    return false;
  }
  if (result.status === 0) {
    return true;
  }

  const combinedOutput = `${result.stdout ?? ""}\n${result.stderr ?? ""}`;
  return /not found|no running instance|does not exist/i.test(combinedOutput);
}

export function parsePosixProcessTreePids(output, rootPid) {
  const childPidsByParent = new Map();
  for (const line of String(output ?? "").split(/\r?\n/)) {
    const match = line.trim().match(/^(\d+)\s+(\d+)$/);
    if (!match) {
      continue;
    }
    const pid = Number.parseInt(match[1], 10);
    const parentPid = Number.parseInt(match[2], 10);
    const childPids = childPidsByParent.get(parentPid) ?? [];
    childPids.push(pid);
    childPidsByParent.set(parentPid, childPids);
  }

  const processTreePids = [];
  const appendChildrenFirst = (pid) => {
    for (const childPid of childPidsByParent.get(pid) ?? []) {
      appendChildrenFirst(childPid);
    }
    processTreePids.push(pid);
  };
  appendChildrenFirst(rootPid);
  return processTreePids;
}

function listPosixProcessTreePids(rootPid) {
  const result = spawnSync("ps", ["-e", "-o", "pid=", "-o", "ppid="], {
    encoding: "utf8",
  });
  return result.error || result.status !== 0
    ? [rootPid]
    : parsePosixProcessTreePids(result.stdout, rootPid);
}

export function terminatePosixProcessTree(
  pid,
  {
    signal = "SIGTERM",
    processTreeReader = listPosixProcessTreePids,
    killProcess = process.kill.bind(process),
  } = {},
) {
  let succeeded = true;
  for (const processPid of processTreeReader(pid)) {
    try {
      killProcess(processPid, signal);
    } catch (error) {
      if (error?.code !== "ESRCH") {
        succeeded = false;
      }
    }
  }
  return succeeded;
}

export function terminateDesktopDevProcessTree(pid, platform = process.platform) {
  return platform === "win32"
    ? terminateWindowsProcessTree(pid)
    : terminatePosixProcessTree(pid);
}
