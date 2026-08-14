import { spawn, spawnSync } from "node:child_process";
import path from "node:path";
import { performance } from "node:perf_hooks";

const root = path.resolve(import.meta.dirname, "..");
const smoke = path.join(root, "tests/node-vfs/real-fuse-smoke.mjs");
const deadlineMs = 60_000;
const started = performance.now();
const environment = { ...process.env };
let command = process.execPath;
let args = [smoke];
let cwd = root;

if (process.platform === "win32") {
  // The real FUSE path is Linux FUSE running under WSL2. The benchmark uses
  // this same host arrangement: PowerShell/Node owns orchestration while the
  // WSL process owns Node, fuse-native, /dev/fuse, and the mount namespace.
  const wslPath = spawnSync("wsl.exe", ["wslpath", "-a", root.replaceAll("\\", "/")], {
    cwd: root,
    encoding: "utf8",
    windowsHide: true,
  });
  if (wslPath.status !== 0 || !wslPath.stdout.trim()) {
    console.error(
      `M7_FUSE_BLOCKED ${JSON.stringify({
        message: "WSL2 is required for the Windows-host real FUSE gate",
        platform: process.platform,
        stderr: wslPath.stderr?.trim() ?? "",
      })}`,
    );
    process.exit(2);
  }

  const candidate = spawnSync("git", ["rev-parse", "HEAD"], {
    cwd: root,
    encoding: "utf8",
    windowsHide: true,
  });
  if (candidate.status !== 0 || !/^[0-9a-f]{40}$/u.test(candidate.stdout.trim())) {
    console.error(
      `M7_FUSE_BLOCKED ${JSON.stringify({
        message: "could not resolve the Windows worktree candidate",
        platform: process.platform,
        stderr: candidate.stderr?.trim() ?? "",
      })}`,
    );
    process.exit(2);
  }

  command = "wsl.exe";
  args = [
    "--cd",
    wslPath.stdout.trim(),
    "--",
    "env",
    `M7_FUSE_CANDIDATE=${candidate.stdout.trim()}`,
    "node",
    "scripts/run-m7-fuse-gate.mjs",
  ];
  cwd = root;
  environment.WSLENV = environment.WSLENV
    ? `${environment.WSLENV}:M7_FUSE_CANDIDATE`
    : "M7_FUSE_CANDIDATE";
}

const child = spawn(command, args, {
  cwd,
  env: environment,
  stdio: "inherit",
  windowsHide: true,
});
const deadline = setTimeout(() => child.kill("SIGTERM"), deadlineMs);
child.once("error", (error) => {
  clearTimeout(deadline);
  throw error;
});
child.once("exit", (code, signal) => {
  clearTimeout(deadline);
  const elapsedMs = Math.round(performance.now() - started);
  if (code === 0 && elapsedMs < deadlineMs) {
    console.log(`m7-real-fuse-gate: PASS (${elapsedMs} ms)`);
    return;
  }
  console.error(
    `m7-real-fuse-gate: ${code === 2 ? "BLOCKED" : "FAIL"} (${code ?? signal ?? "unknown"}, ${elapsedMs} ms)`,
  );
  process.exitCode = code ?? 1;
});
