import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import { access, mkdir, mkdtemp, open as openAsync, rm } from "node:fs/promises";
import os, { tmpdir } from "node:os";
import path from "node:path";
import { performance } from "node:perf_hooks";
import readline from "node:readline";

const MIB = 1024 * 1024;
const SEED = 0x5eed5eed;
const PAYLOAD_BYTES = 16 * MIB;
const COW_EDITS = 5_000;
const NAMESPACE_OPERATIONS = 2_000;
const ACTORS_PER_KIND = 16;
const OPERATIONS_PER_ACTOR = 64;
const EXPECTED_COMPLETED_OPERATIONS = 9_056;
const EXPECTED_FINAL_PAYLOAD_DIGEST =
  "3238fa53923434d162289488f802739eecc4a45303799b7ca4c4b38fddba5d1a";
const RESTARTS = 3;
const deadlineMs = 60_000;
const started = performance.now();
const root = path.resolve(import.meta.dirname, "../..");
let phase = "environment";
let completedOperationCount = 0;
let namespaceOperationCount = 0;
let peakControllerRssBytes = process.memoryUsage().rss;
const slowestOperations = [];

function blocked(message, diagnostics = {}) {
  console.error(`M7_FUSE_BLOCKED ${JSON.stringify({ message, ...diagnostics })}`);
  process.exit(2);
}

function invariant(condition, message) {
  if (!condition) throw new Error(`real FUSE smoke: ${message}`);
}

function sampleControllerMemory() {
  peakControllerRssBytes = Math.max(peakControllerRssBytes, process.memoryUsage().rss);
}

function recordMetric(name, elapsedMs) {
  slowestOperations.push({
    name,
    elapsedMs: Math.round(elapsedMs * 1_000) / 1_000,
  });
  slowestOperations.sort((left, right) => right.elapsedMs - left.elapsedMs);
  if (slowestOperations.length > 10) slowestOperations.length = 10;
  sampleControllerMemory();
}

async function measured(name, callback, options = {}) {
  const operationStarted = performance.now();
  try {
    const value = await callback();
    completedOperationCount += 1;
    if (options.namespace) namespaceOperationCount += 1;
    return value;
  } finally {
    recordMetric(name, performance.now() - operationStarted);
  }
}

function deterministicBytes(length, seed) {
  let state = seed >>> 0;
  const bytes = Buffer.allocUnsafe(length);
  for (let index = 0; index < length; index += 1) {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    bytes[index] = state & 0xff;
  }
  return bytes;
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function fileDigest(filename) {
  const digest = createHash("sha256");
  const descriptor = fs.openSync(filename, "r");
  const buffer = Buffer.allocUnsafe(256 * 1024);
  try {
    for (let position = 0; ;) {
      const read = fs.readSync(descriptor, buffer, 0, buffer.length, position);
      if (read === 0) break;
      digest.update(buffer.subarray(0, read));
      position += read;
    }
  } finally {
    fs.closeSync(descriptor);
  }
  return digest.digest("hex");
}

function run(command, args, cwd = mountpoint, timeout = 10_000) {
  const result = spawnSync(command, args, { cwd, encoding: "utf8", timeout });
  if (result.error) throw result.error;
  if (result.status !== 0)
    throw new Error(
      `${command} ${args.join(" ")} failed (${result.status}): ${result.stderr}`,
    );
  return result.stdout;
}

if (process.platform !== "linux")
  blocked("real mounted FUSE requires Linux", { platform: process.platform });
try {
  await access("/dev/fuse", fs.constants.R_OK | fs.constants.W_OK);
} catch (error) {
  blocked("missing or inaccessible /dev/fuse", { code: error.code });
}
const fuseDevice = fs.statSync("/dev/fuse");
if (!fuseDevice.isCharacterDevice())
  blocked("/dev/fuse is not a character device", { mode: fuseDevice.mode });
try {
  await import("fuse-native");
} catch (error) {
  blocked("fuse-native test dependency is unavailable", { message: error.message });
}
const fusermount = spawnSync("sh", ["-c", "command -v fusermount"], {
  encoding: "utf8",
});
if (fusermount.status !== 0)
  blocked("fuse-native requires the fusermount executable", {
    stderr: fusermount.stderr.trim(),
  });

const directory = await mkdtemp(path.join(tmpdir(), "efs-real-fuse-"));
const database = path.join(directory, "filesystem.db");
const mountpoint = path.join(directory, "mnt");
await mkdir(mountpoint);
const server = path.join(root, "tests/node-vfs/real-fuse-server.mjs");
const storage = run("stat", ["-f", "-c", "%T", directory], root).trim();
const candidate = run("git", ["rev-parse", "HEAD"], root).trim();
const pnpm = run("pnpm", ["--version"], root).trim();

function serverProcess() {
  const child = spawn(process.execPath, [server, database, mountpoint], {
    cwd: root,
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  });
  const output = [];
  let stderr = "";
  let controlSequence = 0;
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk) => (stderr += chunk));
  const lines = readline.createInterface({ input: child.stdout });
  lines.on("line", (line) => {
    try {
      output.push(JSON.parse(line));
    } catch {
      stderr += `${line}\n`;
    }
  });
  const exit = new Promise((resolve) =>
    child.once("exit", (code, signal) => resolve({ code, signal })),
  );
  const waitFor = (kind, timeoutMs = 15_000) =>
    new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error(`FUSE server timed out waiting for ${kind}: ${stderr}`)),
        timeoutMs,
      );
      const poll = setInterval(() => {
        const foundIndex = output.findIndex((entry) => entry.kind === kind);
        if (foundIndex < 0) return;
        const [found] = output.splice(foundIndex, 1);
        clearTimeout(timer);
        clearInterval(poll);
        if (found.error) reject(new Error(found.error));
        else resolve(found);
      }, 5);
      void exit.then(({ code, signal }) => {
        clearTimeout(timer);
        clearInterval(poll);
        reject(
          new Error(
            `FUSE server exited before ${kind} (${code ?? signal ?? "unknown"}): ${stderr}`,
          ),
        );
      });
    });
  const request = async (command, timeoutMs = 30_000) => {
    const id = `control-${++controlSequence}`;
    child.stdin.write(`${JSON.stringify({ id, command })}\n`);
    return waitFor(id, timeoutMs);
  };
  return { child, exit, waitFor, request, stderr: () => stderr };
}

function mountIdentity() {
  const mountinfo = fs.readFileSync("/proc/self/mountinfo", "utf8");
  const line = mountinfo
    .split("\n")
    .find((candidateLine) => candidateLine.split(" ")[4] === mountpoint);
  if (!line || !/ - fuse(?:\.[^ ]+)? \/dev\/fuse /u.test(line))
    throw new Error(
      `mountpoint is not backed by the real kernel FUSE device: ${line ?? "missing"}`,
    );
  return line;
}

function mounted() {
  return fs
    .readFileSync("/proc/self/mountinfo", "utf8")
    .split("\n")
    .some((line) => line.split(" ")[4] === mountpoint);
}

async function waitForUnmount() {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    if (!mounted()) return;
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  throw new Error("real FUSE mount remained present after unmount");
}

async function stop(selected) {
  selected.child.stdin.end("stop\n");
  const [result, exited] = await Promise.all([
    selected.waitFor("stopped", 30_000),
    selected.exit,
  ]);
  if (exited.code !== 0) throw new Error(selected.stderr());
  await waitForUnmount();
  return result;
}

async function crash(selected, descriptor) {
  const beforeClose = await selected.request("snapshot");
  fs.closeSync(descriptor);
  let snapshot;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    snapshot = await selected.request("snapshot");
    if (snapshot.openSessionCount === 0 && snapshot.activeDurableState.leases === 0)
      break;
  }
  invariant(snapshot?.openSessionCount === 0, "fsync descriptor did not close");
  invariant(
    snapshot.activeDurableState.leases === 0,
    "fsync descriptor lease did not release",
  );
  invariant(
    snapshot.metrics.flushCount === beforeClose.metrics.flushCount &&
      snapshot.metrics.flushedWriteBytes === beforeClose.metrics.flushedWriteBytes,
    "descriptor close performed durability work after fsync",
  );
  fsyncCloseNoopVerified = true;
  invariant(selected.child.kill("SIGKILL"), "failed to terminate the fsync process");
  const exited = await selected.exit;
  invariant(
    exited.signal === "SIGKILL" || exited.code !== 0,
    "fsync process did not terminate abruptly",
  );
  if (mounted())
    spawnSync(fusermount.stdout.trim(), ["-uz", mountpoint], { timeout: 5_000 });
  await waitForUnmount();
  return snapshot;
}

async function namespaceDescriptors(currentPath = mountpoint, relative = "/") {
  const descriptors = [`${relative}|directory`];
  const names = fs.readdirSync(currentPath).sort();
  for (const name of names) {
    const filename = path.join(currentPath, name);
    const childPath = relative === "/" ? `/${name}` : `${relative}/${name}`;
    const stat = fs.lstatSync(filename);
    if (stat.isDirectory())
      descriptors.push(...(await namespaceDescriptors(filename, childPath)));
    else if (stat.isSymbolicLink())
      descriptors.push(`${childPath}|symlink|${fs.readlinkSync(filename)}`);
    else
      descriptors.push(
        `${childPath}|file|${stat.size}|${stat.nlink}|${await fileDigest(filename)}`,
      );
  }
  return descriptors;
}

function expectedNamespaceDescriptors(expectedPayload, toolDescriptors) {
  const source = Buffer.from("source");
  const result = [
    "/|directory",
    "/concurrent|directory",
    "/namespace|directory",
    `/namespace/source|file|${source.length}|251|${sha256(source)}`,
    "/smoke|directory",
    `/smoke/payload|file|${expectedPayload.length}|1|${sha256(expectedPayload)}`,
  ];
  for (let index = 0; index < 250; index += 1) {
    const suffix = index.toString().padStart(4, "0");
    const directoryPath = `/namespace/d-${suffix}`;
    result.push(`${directoryPath}|directory`);
    result.push(`${directoryPath}/hard|file|${source.length}|251|${sha256(source)}`);
    result.push(`${directoryPath}/symbolic|symlink|../source`);
  }
  for (let writer = 0; writer < ACTORS_PER_KIND; writer += 1) {
    const bytes = Buffer.alloc(OPERATIONS_PER_ACTOR);
    for (let operation = 0; operation < OPERATIONS_PER_ACTOR; operation += 1)
      bytes[operation] = (writer + operation) % 251;
    result.push(`/concurrent/w-${writer}|file|${bytes.length}|1|${sha256(bytes)}`);
  }
  return [...result, ...toolDescriptors].sort();
}

function equalStrings(left, right) {
  return (
    left.length === right.length && left.every((value, index) => value === right[index])
  );
}

async function runConcurrentActors() {
  await Promise.all([
    ...Array.from({ length: ACTORS_PER_KIND }, (_, reader) =>
      (async () => {
        const opened = await openAsync(
          path.join(mountpoint, "namespace", "source"),
          "r",
        );
        try {
          for (let operation = 0; operation < OPERATIONS_PER_ACTOR; operation += 1) {
            const bytes = Buffer.alloc(6);
            const result = await measured("concurrent-reader", () =>
              opened.read(bytes, 0, bytes.length, 0),
            );
            invariant(
              result.bytesRead === 6 && bytes.toString("utf8") === "source",
              `reader ${reader}:${operation} returned incorrect bytes`,
            );
          }
        } finally {
          await opened.close();
        }
      })(),
    ),
    ...Array.from({ length: ACTORS_PER_KIND }, (_, writer) =>
      (async () => {
        const opened = await openAsync(
          path.join(mountpoint, "concurrent", `w-${writer}`),
          "r+",
        );
        try {
          for (let operation = 0; operation < OPERATIONS_PER_ACTOR; operation += 1) {
            const bytes = Buffer.from([(writer + operation) % 251]);
            const result = await measured("concurrent-writer", () =>
              opened.write(bytes, 0, 1, operation),
            );
            invariant(
              result.bytesWritten === 1,
              `writer ${writer}:${operation} was short`,
            );
          }
          await opened.sync();
        } finally {
          await opened.close();
        }
      })(),
    ),
  ]);
}

const payload = deterministicBytes(PAYLOAD_BYTES, SEED);
const expected = Buffer.from(payload);
const fixtureDigest = sha256(payload);
const mountIdentities = [];
const processPids = [];
const processResults = [];
let selected;
let gitCommit;
let finalVerification;
let finalPayloadDigest;
let namespaceDigest;
let fsyncCrashVerified;
let fsyncCloseNoopVerified;
let closeDurabilityVerified;
let collectionInterrupted;
let collectionResumed;
let toolNamespaceDescriptors;
try {
  phase = "initial-write-and-fsync-crash";
  selected = serverProcess();
  const firstReady = await selected.waitFor("ready");
  processPids.push(firstReady.pid);
  mountIdentities.push(mountIdentity());
  await measured("mkdir-smoke", () => fs.mkdirSync(path.join(mountpoint, "smoke")));
  const dataPath = path.join(mountpoint, "smoke", "payload");
  const descriptor = fs.openSync(dataPath, "wx+");
  await measured("write-16m-payload", () => {
    for (let position = 0; position < payload.length;) {
      const length = Math.min(73_117, payload.length - position);
      invariant(
        fs.writeSync(descriptor, payload, position, length, position) === length,
        "initial mounted write was short",
      );
      position += length;
    }
    fs.fsyncSync(descriptor);
  });
  let restartStarted = performance.now();
  processResults.push(await crash(selected, descriptor));
  selected = undefined;

  phase = "cow-edits-and-namespace";
  selected = serverProcess();
  const secondReady = await selected.waitFor("ready");
  processPids.push(secondReady.pid);
  mountIdentities.push(mountIdentity());
  completedOperationCount += 1;
  recordMetric("restart-after-fsync-crash", performance.now() - restartStarted);
  await measured("digest-after-fsync-crash", async () => {
    const digest = await fileDigest(dataPath);
    invariant(digest === fixtureDigest, "fsync did not survive abrupt provider death");
    fsyncCrashVerified = true;
  });
  await selected.request("retain-smoke-revision", 10_000);
  await selected.request("reset-payload-write-callbacks", 10_000);
  const edit = fs.openSync(dataPath, "r+");
  try {
    for (let index = 0; index < COW_EDITS; index += 1) {
      const group = index % 3;
      const ordinal = Math.floor(index / 3);
      const offset =
        group === 0
          ? ordinal
          : group === 1
            ? 4096 + ((ordinal * 97) % (31 * 4096))
            : 32 * 4096 + ((ordinal * 7919) % (PAYLOAD_BYTES - 32 * 4096));
      const value = (index * 17) & 0xff;
      expected[offset] = value;
      await measured("cow-one-byte-edit", () => {
        invariant(
          fs.writeSync(edit, Buffer.from([value]), 0, 1, offset) === 1,
          `COW edit ${index} was short`,
        );
      });
    }
    fs.fsyncSync(edit);
  } finally {
    fs.closeSync(edit);
  }
  const editCallbacks = await selected.request("stop-payload-write-callbacks", 10_000);
  invariant(
    editCallbacks.mountedPayloadOneByteWriteCallbacks === COW_EDITS,
    `real FUSE host observed ${editCallbacks.mountedPayloadOneByteWriteCallbacks} one-byte edit callbacks`,
  );
  invariant(
    editCallbacks.editBatchProof.callbackCount === COW_EDITS &&
      editCallbacks.editBatchProof.flushCountDelta === 1 &&
      editCallbacks.editBatchProof.failedFlushCountDelta === 0 &&
      editCallbacks.editBatchProof.cowEditCountDelta === 1 &&
      editCallbacks.editBatchProof.cowEditSourceBytesDelta > 0 &&
      editCallbacks.editBatchProof.cowEditSourceBytesDelta <= PAYLOAD_BYTES + 524_288 &&
      editCallbacks.editBatchProof.coreBatchCountDelta > 0,
    `real FUSE edit batch proof differs ${JSON.stringify(editCallbacks.editBatchProof)}`,
  );

  fs.mkdirSync(path.join(mountpoint, "namespace"));
  fs.writeFileSync(path.join(mountpoint, "namespace", "source"), "source");
  for (let index = 0; index < NAMESPACE_OPERATIONS / 8; index += 1) {
    const suffix = index.toString().padStart(4, "0");
    const directoryPath = path.join(mountpoint, "namespace", `d-${suffix}`);
    await measured("namespace-mkdir", () => fs.mkdirSync(directoryPath), {
      namespace: true,
    });
    await measured(
      "namespace-create",
      () => fs.writeFileSync(path.join(directoryPath, "created"), `created-${suffix}`),
      { namespace: true },
    );
    await measured(
      "namespace-stat-created",
      () => fs.statSync(path.join(directoryPath, "created")),
      {
        namespace: true,
      },
    );
    await measured(
      "namespace-rename",
      () =>
        fs.renameSync(
          path.join(directoryPath, "created"),
          path.join(directoryPath, "renamed"),
        ),
      { namespace: true },
    );
    await measured(
      "namespace-hard-link",
      () =>
        fs.linkSync(
          path.join(mountpoint, "namespace", "source"),
          path.join(directoryPath, "hard"),
        ),
      { namespace: true },
    );
    await measured(
      "namespace-stat-hard-link",
      () => fs.statSync(path.join(directoryPath, "hard")),
      {
        namespace: true,
      },
    );
    await measured(
      "namespace-unlink",
      () => fs.unlinkSync(path.join(directoryPath, "renamed")),
      {
        namespace: true,
      },
    );
    await measured(
      "namespace-symbolic-link",
      () => fs.symlinkSync("../source", path.join(directoryPath, "symbolic")),
      { namespace: true },
    );
  }
  invariant(
    namespaceOperationCount === NAMESPACE_OPERATIONS,
    "namespace operation count differs",
  );

  fs.mkdirSync(path.join(mountpoint, "shell"));
  fs.writeFileSync(
    path.join(mountpoint, "shell", "message.txt"),
    "fuse-shell-marker\n",
  );
  fs.renameSync(
    path.join(mountpoint, "shell", "message.txt"),
    path.join(mountpoint, "shell", "renamed.txt"),
  );
  fs.linkSync(
    path.join(mountpoint, "shell", "renamed.txt"),
    path.join(mountpoint, "shell", "hardlink.txt"),
  );
  fs.symlinkSync("renamed.txt", path.join(mountpoint, "shell", "symlink.txt"));
  const findOutput = run("find", [".", "-maxdepth", "3", "-type", "f", "-print"]);
  invariant(
    findOutput.includes("smoke/payload") && findOutput.includes("shell/renamed.txt"),
    "find did not observe mounted files",
  );
  invariant(
    run("grep", ["-R", "fuse-shell-marker", "shell"]).includes("fuse-shell-marker"),
    "grep did not read through the mounted provider",
  );
  fs.mkdirSync(path.join(mountpoint, "repo"));
  run("git", ["init", "-q"], path.join(mountpoint, "repo"));
  run(
    "git",
    ["config", "user.email", "fuse@example.invalid"],
    path.join(mountpoint, "repo"),
  );
  run("git", ["config", "user.name", "FUSE Smoke"], path.join(mountpoint, "repo"));
  fs.writeFileSync(
    path.join(mountpoint, "repo", "tracked.txt"),
    "tracked through fuse\n",
  );
  run("git", ["add", "tracked.txt"], path.join(mountpoint, "repo"));
  run("git", ["commit", "-q", "-m", "real fuse smoke"], path.join(mountpoint, "repo"));
  const closeProof = Buffer.from("close-without-explicit-fsync\n");
  fs.writeFileSync(path.join(mountpoint, "close-proof"), closeProof);
  restartStarted = performance.now();
  processResults.push(await stop(selected));
  selected = undefined;

  phase = "concurrent-actors-and-interrupted-collection";
  selected = serverProcess();
  const thirdReady = await selected.waitFor("ready");
  processPids.push(thirdReady.pid);
  mountIdentities.push(mountIdentity());
  completedOperationCount += 1;
  recordMetric("restart-after-namespace", performance.now() - restartStarted);
  invariant(
    fs.readFileSync(path.join(mountpoint, "close-proof")).equals(closeProof),
    "close did not survive provider restart",
  );
  closeDurabilityVerified = true;
  gitCommit = run(
    "git",
    ["rev-parse", "--verify", "HEAD"],
    path.join(mountpoint, "repo"),
  ).trim();
  invariant(/^[0-9a-f]{40}$/u.test(gitCommit), "Git commit was not durable");
  toolNamespaceDescriptors = [
    ...(await namespaceDescriptors(path.join(mountpoint, "shell"), "/shell")),
    ...(await namespaceDescriptors(path.join(mountpoint, "repo"), "/repo")),
  ];
  fs.rmSync(path.join(mountpoint, "close-proof"));
  fs.mkdirSync(path.join(mountpoint, "concurrent"));
  for (let writer = 0; writer < ACTORS_PER_KIND; writer += 1)
    fs.writeFileSync(
      path.join(mountpoint, "concurrent", `w-${writer}`),
      Buffer.alloc(OPERATIONS_PER_ACTOR),
    );
  await runConcurrentActors();
  await measured("write-orphan", () =>
    fs.writeFileSync(path.join(mountpoint, "orphan"), "collect-me"),
  );
  await measured("unlink-orphan", () => fs.unlinkSync(path.join(mountpoint, "orphan")));
  const interrupted = await selected.request("collect-start", 30_000);
  collectionInterrupted = interrupted.collection.state === "paused";
  invariant(collectionInterrupted, "collection did not pause after one bounded batch");
  restartStarted = performance.now();
  processResults.push(await stop(selected));
  selected = undefined;

  phase = "resumed-collection-and-final-verification";
  selected = serverProcess();
  const fourthReady = await selected.waitFor("ready");
  processPids.push(fourthReady.pid);
  mountIdentities.push(mountIdentity());
  completedOperationCount += 1;
  recordMetric("restart-during-collection", performance.now() - restartStarted);
  const resumed = await selected.request("resume-collection", 45_000);
  collectionResumed = resumed.collection.state === "complete";
  invariant(collectionResumed, "collection did not resume to completion");
  finalPayloadDigest = await fileDigest(dataPath);
  invariant(finalPayloadDigest === sha256(expected), "final payload digest differs");
  invariant(
    finalPayloadDigest === EXPECTED_FINAL_PAYLOAD_DIGEST,
    "final payload digest does not encode the exact deterministic edit profile",
  );
  const actualNamespace = (await namespaceDescriptors()).sort();
  const expectedNamespace = expectedNamespaceDescriptors(
    expected,
    toolNamespaceDescriptors,
  );
  if (!equalStrings(actualNamespace, expectedNamespace)) {
    const firstDifference = actualNamespace.findIndex(
      (value, index) => value !== expectedNamespace[index],
    );
    const differingIndex = firstDifference < 0 ? 0 : firstDifference;
    throw new Error(
      `final namespace differs ${JSON.stringify({
        actualCount: actualNamespace.length,
        expectedCount: expectedNamespace.length,
        differingIndex,
        actual: actualNamespace.slice(differingIndex, differingIndex + 3),
        expected: expectedNamespace.slice(differingIndex, differingIndex + 3),
      })}`,
    );
  }
  namespaceDigest = sha256(Buffer.from(actualNamespace.join("\n")));
  finalVerification = await selected.request("final-verify", 45_000);
  invariant(
    finalVerification.collection.state === "complete",
    "completed collection was not retained through final verification",
  );
  invariant(finalVerification.usageVerified === true, "durable usage was not verified");
  invariant(
    finalVerification.activeDurableState.leases === 0 &&
      finalVerification.activeDurableState.staging === 0 &&
      finalVerification.activeDurableState.reservations === 0,
    "lease, staging, or result reservation leaked",
  );
  const finalStopped = await stop(selected);
  processResults.push(finalStopped);
  selected = undefined;
  invariant(!mounted(), "final FUSE unmount did not complete");

  const elapsedMs = Math.round(performance.now() - started);
  invariant(elapsedMs < deadlineMs, `profile exceeded ${deadlineMs} ms (${elapsedMs})`);
  invariant(
    completedOperationCount === EXPECTED_COMPLETED_OPERATIONS,
    `completed operation count differs (${completedOperationCount})`,
  );
  invariant(processPids.length === RESTARTS + 1, "provider process count differs");
  invariant(
    new Set(processPids).size === processPids.length,
    "provider PIDs were reused",
  );
  const peakManagedResidentBytes = Math.max(
    ...processResults.map((result) => result.metrics.peakManagedResidentBytes),
    finalVerification.metrics.peakManagedResidentBytes,
  );
  const aggregateLimitBytes =
    firstReady.providerCapabilities.runtime.maxManagedResidentBytes;
  invariant(
    peakManagedResidentBytes <= aggregateLimitBytes,
    "managed resident memory crossed the aggregate limit",
  );
  const peakRssBytes = Math.max(
    ...processResults.map((result) => result.peakRssBytes),
    finalVerification.peakRssBytes,
  );
  const transactionCount = processResults.reduce(
    (sum, result) => sum + result.transactionCount,
    0,
  );
  const providerMetrics = Object.freeze({
    coreBatchCount: processResults.reduce(
      (sum, result) => sum + result.metrics.coreBatchCount,
      0,
    ),
    flushCount: processResults.reduce(
      (sum, result) => sum + result.metrics.flushCount,
      0,
    ),
    forcedFlushCount: processResults.reduce(
      (sum, result) => sum + result.metrics.forcedFlushCount,
      0,
    ),
    failedFlushCount: processResults.reduce(
      (sum, result) => sum + result.metrics.failedFlushCount,
      0,
    ),
    admittedWriteBytes: processResults.reduce(
      (sum, result) => sum + result.metrics.admittedWriteBytes,
      0,
    ),
    flushedWriteBytes: processResults.reduce(
      (sum, result) => sum + result.metrics.flushedWriteBytes,
      0,
    ),
  });
  const mountedPayloadOneByteWriteCallbacks = processResults.reduce(
    (sum, result) => sum + result.mountedPayloadOneByteWriteCallbacks,
    0,
  );
  invariant(
    mountedPayloadOneByteWriteCallbacks === COW_EDITS,
    `real FUSE host observed ${mountedPayloadOneByteWriteCallbacks} one-byte payload edit callbacks`,
  );
  console.log(
    JSON.stringify({
      schema: "efs-m7-real-fuse-smoke-v2",
      candidate,
      platform: process.platform,
      architecture: process.arch,
      node: process.version,
      pnpm,
      kernel: os.release(),
      cpu: os.cpus()[0]?.model ?? "unknown",
      totalMemoryBytes: os.totalmem(),
      storage,
      sqlite: firstReady.sqlite,
      schemaVersion: firstReady.schemaVersion,
      fuseVersion: firstReady.fuseVersion,
      device: fs.realpathSync("/dev/fuse"),
      deviceIsCharacter: fuseDevice.isCharacterDevice(),
      deviceRdev: fuseDevice.rdev,
      fusermount: fusermount.stdout.trim(),
      uid: process.getuid?.() ?? -1,
      mountIdentity: mountIdentities,
      mountCycleIds: [1, 2, 3, 4],
      processPids,
      processRestarts: RESTARTS,
      restartUnmounts: RESTARTS,
      finalUnmounted: true,
      fsyncCrashVerified,
      fsyncCloseNoopVerified,
      closeDurabilityVerified,
      fixtureBytes: PAYLOAD_BYTES,
      seed: SEED,
      fixtureDigest,
      finalPayloadDigest,
      expectedFinalPayloadDigest: EXPECTED_FINAL_PAYLOAD_DIGEST,
      namespaceDigest,
      completedOperationCount,
      namespaceOperationCount,
      oneByteEditCount: COW_EDITS,
      mountedPayloadOneByteWriteCallbacks,
      editBatchProof: editCallbacks.editBatchProof,
      providerCowEditCount: processResults.reduce(
        (sum, result) => sum + result.metrics.cowEditCount,
        0,
      ),
      transactionCount,
      providerMetrics,
      readerActors: ACTORS_PER_KIND,
      writerActors: ACTORS_PER_KIND,
      operationsPerActor: OPERATIONS_PER_ACTOR,
      collectionInterrupted,
      collectionResumed,
      finalCollectionComplete: finalVerification.finalCollection.state === "complete",
      finalCollectionCommittedBatches:
        finalVerification.finalCollection.committedBatches,
      verificationComplete: true,
      usageVerified: true,
      activeDurableState: finalVerification.activeDurableState,
      usage: finalVerification.usage,
      storageSnapshot: finalVerification.storage,
      physicalStorage: finalVerification.physical,
      gitCommit,
      sqliteCapabilities: firstReady.sqliteCapabilities,
      filesystemCapabilities: firstReady.filesystemCapabilities,
      providerCapabilities: firstReady.providerCapabilities,
      fastCdc: {
        minimumBytes: 32_768,
        averageBytes: 131_072,
        maximumBytes: 524_288,
      },
      manifestFormat: firstReady.filesystemCapabilities.format.manifestFormat,
      operatingSystemCacheDropAttempted: false,
      operatingSystemCacheDropSucceeded: false,
      peakManagedResidentBytes,
      aggregateLimitBytes,
      peakRssBytes,
      peakControllerRssBytes,
      slowestOperations,
      smokeDeadlineMs: deadlineMs,
      elapsedMs,
    }),
  );
} catch (error) {
  throw new Error(
    `real FUSE smoke failure ${JSON.stringify({
      seed: SEED,
      phase,
      completedOperationCount,
      namespaceOperationCount,
      slowestOperations,
      error: String(error),
    })}`,
    { cause: error },
  );
} finally {
  if (selected) {
    try {
      selected.child.stdin.end("stop\n");
      await Promise.race([
        selected.waitFor("stopped", 2_000),
        new Promise((resolve) => setTimeout(resolve, 2_100)),
      ]);
    } catch {}
    if (selected.child.exitCode === null) selected.child.kill("SIGKILL");
  }
  if (mounted())
    spawnSync(fusermount.stdout.trim(), ["-uz", mountpoint], { timeout: 5_000 });
  await rm(directory, { recursive: true, force: true });
}
