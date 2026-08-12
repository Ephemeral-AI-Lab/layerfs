import { spawnSync } from "node:child_process";
import { cpus, totalmem } from "node:os";

function commandOutput(command, args) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    timeout: 10_000,
    windowsHide: true,
  });
  return result.status === 0 ? result.stdout.trim().replace(/\s+/gu, " ") : "";
}

function storageHardware() {
  if (process.env.EFS_BENCHMARK_STORAGE_HARDWARE)
    return process.env.EFS_BENCHMARK_STORAGE_HARDWARE;
  if (process.platform === "win32") {
    const value = commandOutput("powershell.exe", [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      'Get-CimInstance Win32_DiskDrive | Sort-Object Index | ForEach-Object { "$($_.Model) [$($_.MediaType), $($_.Size) bytes]" }',
    ]);
    if (value) return value;
  }
  if (process.platform === "linux") {
    const value = commandOutput("lsblk", ["-d", "-n", "-o", "NAME,MODEL,SIZE,ROTA"]);
    if (value) return value;
  }
  if (process.platform === "darwin") {
    const value = commandOutput("system_profiler", [
      "SPStorageDataType",
      "-detailLevel",
      "mini",
    ]);
    if (value) return value;
  }
  return "unavailable; set EFS_BENCHMARK_STORAGE_HARDWARE on this runner";
}

export function runtimeEnvironment(extra = {}) {
  return Object.freeze({
    platform: process.platform,
    architecture: process.arch,
    node: process.version,
    pnpm: "10.32.1",
    cpu: cpus()[0]?.model?.trim() || "unknown",
    logicalCpuCount: cpus().length,
    totalMemoryBytes: totalmem(),
    storage: storageHardware(),
    ...extra,
  });
}

export function effectiveResourceLimits(filesystem) {
  const capabilities = filesystem.capabilities;
  return Object.freeze({
    filesystem: capabilities.filesystem,
    storage: capabilities.storage,
    runtime: capabilities.runtime,
    branch: capabilities.branch,
  });
}
