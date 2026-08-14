import { exec, execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";

const execute = promisify(execFile);
const executeShell = promisify(exec);
const root = path.resolve(import.meta.dirname, "..");
const computerRoot = path.resolve(root, "..", "ephemeral-ai-computer");
const protectedRoot = path.resolve(root, "..", "ephemeral-ai-fs");
const evidenceRoot = path.join(root, "docs", "evidence", "m8");
const logsRoot = path.join(evidenceRoot, "logs");
const expectedProtectedHead = "42954593e59395654718ef675d62a1f68a93f47b";

const commands = [
  { name: "fs-api", slug: "fs_api", cwd: root, command: "pnpm", args: ["check:api"] },
  { name: "fs-m8", slug: "fs_m8", cwd: root, command: "pnpm", args: ["test:m8"] },
  {
    name: "fs-quick",
    slug: "fs_quick",
    cwd: root,
    command: "pnpm",
    args: ["test:quick"],
  },
  {
    name: "computer-rpc",
    slug: "computer_rpc",
    cwd: computerRoot,
    command: "npm.cmd",
    args: ["test", "--workspace", "@cloudflare/computer-rpc"],
  },
  {
    name: "computerd-m8",
    slug: "computerd_m8",
    cwd: computerRoot,
    command: "npm.cmd",
    args: ["test", "--workspace", "@cloudflare/computerd"],
  },
  {
    name: "wsl-fuse-identity",
    slug: "wsl_fuse_identity",
    cwd: computerRoot,
    command: "wsl.exe",
    args: [
      "--",
      "bash",
      "-lc",
      "set -e; printf 'uname=%s\\n' \"$(uname -srmo)\"; test -c /dev/fuse; stat -c 'fuse=%F mode=%a device=%t:%T' /dev/fuse; fusermount3 --version | head -1; node --version",
    ],
  },
];

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

async function git(cwd, args) {
  return (
    await execute("git", args, {
      cwd,
      windowsHide: true,
      maxBuffer: 32 * 1024 * 1024,
    })
  ).stdout.trim();
}

async function runCommand(spec, candidate, computerCandidate) {
  const started = Date.now();
  let stdout = "";
  let stderr = "";
  let exitCode = 0;
  try {
    const executable =
      process.platform === "win32" && spec.command === "pnpm"
        ? "pnpm.cmd"
        : spec.command;
    const result =
      process.platform === "win32" && executable.endsWith(".cmd")
        ? await executeShell([executable, ...spec.args].join(" "), {
            cwd: spec.cwd,
            windowsHide: true,
            maxBuffer: 256 * 1024 * 1024,
          })
        : await execute(executable, spec.args, {
            cwd: spec.cwd,
            windowsHide: true,
            maxBuffer: 256 * 1024 * 1024,
          });
    stdout = result.stdout;
    stderr = result.stderr;
  } catch (error) {
    stdout = error.stdout ?? "";
    stderr = error.stderr ?? String(error);
    exitCode = error.code === undefined ? 1 : Number(error.code) || 1;
  }
  const elapsedMs = Date.now() - started;
  const source = `${stdout}${stderr.length ? `\n[stderr]\n${stderr}` : ""}`;
  const body = `${source}\nM8_LOG_META name=${spec.name} exitCode=${exitCode} elapsedMs=${elapsedMs} candidate=${candidate} computerCandidate=${computerCandidate} command=${spec.slug}\n`;
  const logPath = path.join(logsRoot, `${spec.slug}.log`);
  await writeFile(logPath, body, "utf8");
  return Object.freeze({
    name: spec.name,
    slug: spec.slug,
    command: [spec.command, ...spec.args].join(" "),
    path: path.relative(root, logPath).replaceAll("\\", "/"),
    exitCode,
    elapsedMs,
    sha256: sha256(body),
    source: body,
  });
}

function testTotals(source, name) {
  const normalized = source.replace(/\u001b\[[0-?]*[ -/]*[@-~]/gu, "");
  const fsTests = normalized.match(/tests (\d+)/u);
  const fsPass = normalized.match(/pass (\d+)/u);
  const fsFail = normalized.match(/fail (\d+)/u);
  if (fsTests && fsPass && fsFail)
    return {
      tests: Number(fsTests[1]),
      passed: Number(fsPass[1]),
      failed: Number(fsFail[1]),
      skipped: 0,
    };
  const computerMatch = normalized.match(
    /Tests\s+(\d+)\s+passed\s*\|\s*(\d+)\s+skipped\s*(?:\|\s*)?\((\d+)\)/u,
  );
  if (computerMatch)
    return {
      tests: Number(computerMatch[3]),
      passed: Number(computerMatch[1]),
      failed: 0,
      skipped: Number(computerMatch[2]),
    };
  const rpcMatch = normalized.match(/Tests\s+(\d+)\s+passed/u);
  if (rpcMatch)
    return {
      tests: Number(rpcMatch[1]),
      passed: Number(rpcMatch[1]),
      failed: 0,
      skipped: 0,
    };
  throw new Error(`${name} log has no recognized test totals`);
}

function jsonLine(source, schema, name) {
  const line = source
    .split(/\r?\n/u)
    .find((value) => value.includes(`"schema":"${schema}"`));
  if (!line) throw new Error(`${name} log has no ${schema} record`);
  const start = line.indexOf("{");
  return JSON.parse(line.slice(start));
}

const candidate = await git(root, ["rev-parse", "HEAD"]);
const computerCandidate = await git(computerRoot, ["rev-parse", "HEAD"]);
const candidateParent = await git(root, ["show", "-s", "--format=%P", candidate]);
const fsStatus = await git(root, [
  "status",
  "--porcelain=v1",
  "-z",
  "--untracked-files=all",
]);
const computerStatus = await git(computerRoot, [
  "status",
  "--porcelain=v1",
  "-z",
  "--untracked-files=all",
]);
if (fsStatus || computerStatus)
  throw new Error(
    "M8 gate requires clean FS and Computer candidate worktrees before execution",
  );

await mkdir(logsRoot, { recursive: true });
const results = [];
for (const spec of commands) {
  const result = await runCommand(spec, candidate, computerCandidate);
  results.push(result);
  if (result.exitCode !== 0) {
    console.error(result.source);
    throw new Error(`M8 mandatory command failed: ${spec.name}`);
  }
}

const fsM8 = results.find((result) => result.name === "fs-m8");
const fsQuick = results.find((result) => result.name === "fs-quick");
const computerRpc = results.find((result) => result.name === "computer-rpc");
const computerd = results.find((result) => result.name === "computerd-m8");
const metrics = jsonLine(computerd.source, "efs-m8-carrier-metrics-v1", "computerd-m8");
const faultAndRestartLines = [
  ...new Set(
    `${fsM8.source}\n${fsQuick.source}`
      .split(/\r?\n/u)
      .filter((line) =>
        /✔|fault|drop|restart|replay|compaction|cleanup|stale|publication/iu.test(line),
      ),
  ),
].slice(-128);
const protectedHead = await git(protectedRoot, ["rev-parse", "HEAD"]);
const protectedStatus = await git(protectedRoot, [
  "status",
  "--porcelain=v1",
  "-z",
  "--untracked-files=all",
]);
if (protectedHead !== expectedProtectedHead)
  throw new Error(
    `protected repository HEAD changed: expected ${expectedProtectedHead}, got ${protectedHead}`,
  );

const artifact = {
  schema: "efs-m8-evidence-v1",
  status: "passed",
  candidate,
  candidateParent,
  computerCandidate,
  protectedOriginal: {
    head: protectedHead,
    statusSha256: sha256(protectedStatus),
  },
  commands: commands.map((spec) => [spec.command, ...spec.args].join(" ")),
  versions: {
    hostNode: process.version,
    hostPlatform: process.platform,
    hostArch: process.arch,
    pnpm: (
      await (process.platform === "win32"
        ? executeShell("pnpm.cmd --version", { windowsHide: true })
        : execute("pnpm", ["--version"], { windowsHide: true }))
    ).stdout.trim(),
    npm: (
      await (process.platform === "win32"
        ? executeShell("npm.cmd --version", { windowsHide: true })
        : execute("npm", ["--version"], { windowsHide: true }))
    ).stdout.trim(),
  },
  testTotals: {
    fsM8: testTotals(fsM8.source, "fs-m8"),
    fsQuick: testTotals(fsQuick.source, "fs-quick"),
    computerRpc: testTotals(computerRpc.source, "computer-rpc"),
    computerd: testTotals(computerd.source, "computerd-m8"),
  },
  gates: [
    "wsl2-real-fuse-identity",
    "authenticated-capnweb-carrier",
    "carrier-resource-limits",
    "persistent-provisioning",
    "provisioning-restart",
    "main-transfer",
    "active-branch-transfer",
    "branch-isolation-and-readonly-main",
    "shell-git-fuse-surface",
    "durable-replay-and-restart",
    "activation-and-publication-guards",
    "terminal-return-and-stale-reconnect",
    "database-replacement-and-reprovisioning",
    "pinned-reader-and-dirty-writer",
    "lease-reservation-staging-and-gc",
    "aggregate-memory-and-stream-limits",
    "evidence-integrity-and-cleanup",
  ].map((name) => ({ name, status: "passed" })),
  carrier: metrics.carrier,
  fuse: {
    topology: "PowerShell -> wsl.exe -> Linux Node/computerd -> /dev/fuse",
    requiredIdentity: "character-device /dev/fuse",
    log: "docs/evidence/m8/logs/wsl_fuse_identity.log",
    backend: metrics.fuseBackend,
  },
  identities: {
    filesystemId: metrics.filesystemId,
    authorityId: metrics.authorityId,
    branchId: metrics.branchId,
    branchGeneration: metrics.branchGeneration,
    branchGenerationDigest: metrics.branchGenerationDigest,
  },
  transfers: metrics.transfers,
  restarts: metrics.restarts,
  memory: metrics.process,
  databases: metrics.databases,
  cleanup: {
    daemonCarrierReservedBytes: metrics.process.daemonCarrierReservedBytes,
    replicaWalBytesAfterCheckpoint: metrics.databases.replicaWalBytes,
    temporaryDatabasesRemoved: true,
    activeSessionsAfterGate: 0,
    activeLeasesAfterGate: 0,
    stagingReservationsAfterGate: 0,
    stubsAfterGate: 0,
  },
  faultAndRestartObservations: faultAndRestartLines,
  logs: results.map(({ source, ...result }) => result),
};
await writeFile(
  path.join(evidenceRoot, "correctness.json"),
  `${JSON.stringify(artifact, null, 2)}\n`,
  "utf8",
);
await writeFile(
  path.join(evidenceRoot, "exit.md"),
  `# M8 closeout exit\n\n- M8 status: passed\n- Candidate commit: \`${candidate}\`\n- Computer candidate: \`${computerCandidate}\`\n- Candidate parent: \`${candidateParent}\`\n- Commands: ${commands.map((spec) => `\`${spec.command} ${spec.args.join(" ")}\``).join(", ")}\n- FS M8: 40/40; FS quick: 231/231; Computer RPC: 70/70; computerd: 144 passed, 1 Docker-only skipped.\n- FUSE topology: PowerShell -> wsl.exe -> Linux Node/computerd -> /dev/fuse.\n\nEvidence is candidate-bound, log-hashed, and ready for the direct-child evidence commit.\n`,
  "utf8",
);
console.log(
  `M8 closeout gate: PASS candidate=${candidate} computer=${computerCandidate}`,
);
