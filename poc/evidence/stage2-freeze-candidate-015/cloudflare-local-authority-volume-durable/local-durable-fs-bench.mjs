#!/usr/bin/env node

import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { closeSync, existsSync, fsyncSync, openSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath, pathToFileURL } from "node:url";
import { promisify } from "node:util";

const execFileP = promisify(execFile);
const SCRIPT = fileURLToPath(import.meta.url);
const REPO_ROOT = resolve(dirname(SCRIPT), "..");
const PRODUCT_ROOT =
  process.env.CLOUDFLARE_COMPUTER_ROOT ??
  (readable("/opt/cloudflare-computer/packages/dofs/dist/index.js")
    ? "/opt/cloudflare-computer"
    : REPO_ROOT);

export const IMAGE = "sha256:8c5100fabfd873de4ee7aabf908027e946b3fdac5328e15f9dabbf9731200bb0";
export const SOURCE = "de87919a4fd37242e960e13b7b3ba802d1eef0a0";
export const TREE = "4fb409d7e1356e1098439293d77d2fdc2dbf2190";
export const PAYLOAD_BYTES = 64 * 1024 * 1024;
const TARGET = "/workspace/payload.bin";
const DB_IN_CONTAINER = "/durable-state/authoritative.sqlite";
const PORT = 45678;

function readable(path) {
  return existsSync(path);
}

class Cursor {
  constructor(rows) {
    this.rows = rows;
  }

  toArray() {
    return this.rows;
  }
}

function sqliteValue(value) {
  if (value === undefined || value === null) return null;
  if (typeof value === "boolean") return value ? 1 : 0;
  if (
    value instanceof Uint8Array ||
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "bigint"
  ) {
    return value;
  }
  throw new TypeError(`unsupported SQLite binding: ${typeof value}`);
}

// The local equivalent of a Durable Object's authoritative SQL store.
// Unlike SQLiteTestStorage, this deliberately survives its Node process.
export class FileSQLiteStorage {
  constructor(filename, { readOnly = false } = {}) {
    this.filename = filename;
    this.raw = new DatabaseSync(filename, readOnly ? { readOnly: true } : {});
    this.cache = new Map();
    if (!readOnly) {
      this.raw.exec("PRAGMA journal_mode=WAL");
      this.raw.exec("PRAGMA synchronous=FULL");
      this.raw.exec("PRAGMA wal_autocheckpoint=0");
    }
    this.sql = {
      exec: (query, ...bindings) => {
        let statement = this.cache.get(query);
        if (statement === undefined) {
          statement = this.raw.prepare(query);
          this.cache.set(query, statement);
        }
        return new Cursor(statement.all(...bindings.map(sqliteValue)) ?? []);
      },
    };
  }

  transactionSync(closure) {
    this.raw.exec("BEGIN IMMEDIATE");
    try {
      const result = closure();
      this.raw.exec("COMMIT");
      return result;
    } catch (error) {
      this.raw.exec("ROLLBACK");
      throw error;
    }
  }

  durableBarrier() {
    const [checkpoint] = this.raw.prepare("PRAGMA wal_checkpoint(TRUNCATE)").all();
    if (checkpoint?.busy !== 0 || checkpoint?.log > 0 || checkpoint?.checkpointed > 0) {
      throw new Error(`incomplete WAL checkpoint: ${JSON.stringify(checkpoint)}`);
    }
    for (const path of [this.filename, dirname(this.filename)]) {
      const fd = openSync(path, "r");
      try {
        fsyncSync(fd);
      } finally {
        closeSync(fd);
      }
    }
    return checkpoint;
  }

  close() {
    this.cache.clear();
    this.raw.close();
  }
}

async function modules() {
  const [{ createWorkspaceClient }, driver, dofs] = await Promise.all([
    import(pathToFileURL(resolve(PRODUCT_ROOT, "packages/rpc/dist/client.js"))),
    import(pathToFileURL(resolve(PRODUCT_ROOT, "packages/rpc/dist/sync-driver.js"))),
    import(pathToFileURL(resolve(PRODUCT_ROOT, "packages/dofs/dist/index.js"))),
  ]);
  return { createWorkspaceClient, ...driver, ...dofs };
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function verifyProvider(provider, expectedHash) {
  const entries = provider.readdirSync("/workspace");
  const stat = provider.statSync(TARGET);
  const bytes = provider.readFileSync(TARGET);
  const actualHash = sha256(bytes);
  if (
    JSON.stringify(entries) !== JSON.stringify(["payload.bin"]) ||
    stat.size !== PAYLOAD_BYTES ||
    actualHash !== expectedHash
  ) {
    throw new Error(
      `authoritative inventory mismatch: ${JSON.stringify({ entries, size: stat.size, actualHash })}`,
    );
  }
  return { entries, size: stat.size, sha256: actualHash };
}

async function openWorkspaceClient(createWorkspaceClient) {
  return createWorkspaceClient({ url: `ws://127.0.0.1:${PORT}/api` });
}

async function pullAndBarrier(dbFile, expectedHash, cleanup = false) {
  const { Database, SQLiteWorkspaceProvider, initializeSchema, createWorkspaceClient, pullOnce } =
    await modules();
  const storage = new FileSQLiteStorage(dbFile);
  const db = new Database(storage);
  initializeSchema(db, Date.now);
  const client = await openWorkspaceClient(createWorkspaceClient);
  try {
    const pulled = await pullOnce(db, client.sync);
    const provider = new SQLiteWorkspaceProvider(db, { now: Date.now });
    const inventory = cleanup
      ? (() => {
          const entries = provider.readdirSync("/workspace");
          if (entries.length !== 0) throw new Error(`cleanup inventory is not empty: ${entries}`);
          return { entries };
        })()
      : verifyProvider(provider, expectedHash);
    const checkpoint = storage.durableBarrier();
    return { pulled: pulled.applied, inventory, checkpoint, pid: process.pid };
  } finally {
    await client.close();
    storage.close();
  }
}

async function restore(dbFile) {
  const { Database, initializeSchema, createWorkspaceClient, reconcileWatermarks, pushOnce } =
    await modules();
  const storage = new FileSQLiteStorage(dbFile);
  const db = new Database(storage);
  initializeSchema(db, Date.now);
  const client = await openWorkspaceClient(createWorkspaceClient);
  try {
    const reconciled = await reconcileWatermarks(db, client.sync);
    const pushed = await pushOnce(db, client.sync);
    const checkpoint = storage.durableBarrier();
    return { reconciled, pushed, checkpoint, pid: process.pid };
  } finally {
    await client.close();
    storage.close();
  }
}

async function verifyDb(dbFile, expectedHash) {
  const { Database, SQLiteWorkspaceProvider } = await modules();
  const storage = new FileSQLiteStorage(dbFile, { readOnly: true });
  try {
    const provider = new SQLiteWorkspaceProvider(new Database(storage), { now: Date.now });
    return { ...verifyProvider(provider, expectedHash), pid: process.pid };
  } finally {
    storage.close();
  }
}

async function docker(...args) {
  return execFileP("docker", args, { maxBuffer: 32 * 1024 * 1024 });
}

async function dockerExec(container, ...args) {
  return docker("exec", container, ...args);
}

async function inspect(target) {
  const { stdout } = await docker("inspect", target);
  return JSON.parse(stdout)[0];
}

async function waitReady(container) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    try {
      await dockerExec(
        container,
        "node",
        "-e",
        `fetch('http://127.0.0.1:${PORT}/health').then(r=>{if(!r.ok)process.exit(2)})`,
      );
      return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
  throw new Error("computerd did not become ready");
}

async function runBoundHelper(container, ...args) {
  const { stdout } = await dockerExec(
    container,
    "node",
    "--experimental-sqlite",
    "--no-warnings",
    "/harness/local-durable-fs-bench.mjs",
    ...args,
  );
  return JSON.parse(stdout);
}

function assertImage(image) {
  if (image.Id !== IMAGE || image.Architecture !== "arm64" || image.Os !== "linux") {
    throw new Error(`unadmitted image: ${image.Id} ${image.Os}/${image.Architecture}`);
  }
  const labels = image.Config?.Labels ?? {};
  if (
    labels["dev.layerfs.upstream-commit"] !== SOURCE ||
    labels["dev.layerfs.upstream-tree"] !== TREE
  ) {
    throw new Error(`image provenance mismatch: ${JSON.stringify(labels)}`);
  }
  return labels;
}

function assertEnvelope(container, volumeName) {
  const host = container.HostConfig;
  const workspaceMount = container.Mounts?.find((mount) => mount.Destination === "/workspace");
  const durableMount = container.Mounts?.find((mount) => mount.Destination === "/durable-state");
  if (
    container.Image !== IMAGE ||
    container.Platform !== "linux" ||
    host.NanoCpus !== 1_000_000_000 ||
    host.Memory !== 512 * 1024 * 1024 ||
    host.MemorySwap !== 512 * 1024 * 1024 ||
    host.NetworkMode !== "none" ||
    !host.CapAdd?.includes("CAP_SYS_ADMIN") ||
    !host.Devices?.some((device) => device.PathOnHost === "/dev/fuse") ||
    workspaceMount !== undefined ||
    durableMount?.Type !== "volume" ||
    durableMount?.Name !== volumeName
  ) {
    throw new Error("container does not match the admitted local envelope");
  }
}

async function processIdentity(container) {
  const details = await inspect(container);
  const { stdout } = await dockerExec(
    container,
    "bash",
    "-lc",
    "set -e; printf 'host_pid=%s\\n' \"$PPID\"; awk '{print \"pid1_start=\" $22}' /proc/1/stat; readlink /proc/1/ns/pid",
  );
  return { hostPid: details.State.Pid, startedAt: details.State.StartedAt, inside: stdout.trim() };
}

async function verifyFuse(container, expectedHash) {
  const [{ stdout: mountinfo }, { stdout: hashOut }, { stdout: inventory }] = await Promise.all([
    dockerExec(container, "cat", "/proc/self/mountinfo"),
    dockerExec(container, "sha256sum", TARGET),
    dockerExec(
      container,
      "find",
      "/workspace",
      "-mindepth",
      "1",
      "-maxdepth",
      "1",
      "-printf",
      "%y %P %s\\n",
    ),
  ]);
  if (
    !mountinfo
      .split("\n")
      .some((line) => line.includes(" /workspace ") && line.includes(" - fuse "))
  ) {
    throw new Error("/workspace is not a real FUSE mount");
  }
  const actualHash = hashOut.trim().split(/\s+/)[0];
  if (actualHash !== expectedHash || inventory !== `f payload.bin ${PAYLOAD_BYTES}\n`) {
    throw new Error(`fresh-FUSE mismatch: ${actualHash} ${JSON.stringify(inventory)}`);
  }
  return { sha256: actualHash, inventory, mount: "fuse" };
}

async function main() {
  const mode = process.argv[2];
  if (mode === "--pull-barrier") {
    console.log(JSON.stringify(await pullAndBarrier(process.argv[3], process.argv[4])));
    return;
  }
  if (mode === "--pull-cleanup") {
    console.log(JSON.stringify(await pullAndBarrier(process.argv[3], "", true)));
    return;
  }
  if (mode === "--restore") {
    console.log(JSON.stringify(await restore(process.argv[3])));
    return;
  }
  if (mode === "--verify-db") {
    console.log(JSON.stringify(await verifyDb(process.argv[3], process.argv[4])));
    return;
  }
  if (mode !== undefined) throw new Error(`unknown option: ${mode}`);

  const containerName = `cloudflare-local-durable-${process.pid}`;
  const volumeName = `${containerName}-store`;
  let containerCreated = false;
  let volumeCreated = false;
  const receipt = {
    schema: "cloudflare-local-authoritative-sqlite-durable-v1",
    status: "FAIL",
    persistenceClass: "LOCAL_AUTHORITATIVE_SQLITE_DURABLE_PENDING",
    image: IMAGE,
    source: SOURCE,
    tree: TREE,
    envelope: {
      platform: "linux/arm64",
      cpus: 1,
      memoryBytes: 512 * 1024 * 1024,
      memorySwapBytes: 512 * 1024 * 1024,
      network: "none",
      workspace: "native FUSE",
    },
    cleanup: { containerAbsent: false, volumeAbsent: false },
  };

  try {
    const image = await inspect(IMAGE);
    receipt.imageLabels = assertImage(image);
    try {
      await docker("volume", "inspect", volumeName);
      throw new Error(`refusing to reuse existing volume ${volumeName}`);
    } catch (error) {
      if (error instanceof Error && error.message.startsWith("refusing")) throw error;
    }
    const volumeCreateArgv = [
      "volume",
      "create",
      "--driver",
      "local",
      "--label",
      `dev.layerfs.owner=${containerName}`,
      "--label",
      "dev.layerfs.persistence-class=LOCAL_AUTHORITATIVE_SQLITE_DURABLE",
      volumeName,
    ];
    const { stdout: volumeOut } = await docker(...volumeCreateArgv);
    if (volumeOut.trim() !== volumeName) throw new Error(`unexpected volume: ${volumeOut}`);
    volumeCreated = true;
    const { stdout: volumeInspectOut } = await docker("volume", "inspect", volumeName);
    const volumeInspect = JSON.parse(volumeInspectOut)[0];
    if (
      volumeInspect.Name !== volumeName ||
      volumeInspect.Driver !== "local" ||
      volumeInspect.Scope !== "local" ||
      volumeInspect.Labels?.["dev.layerfs.owner"] !== containerName ||
      volumeInspect.Labels?.["dev.layerfs.persistence-class"] !==
        "LOCAL_AUTHORITATIVE_SQLITE_DURABLE"
    ) {
      throw new Error(`unadmitted volume: ${JSON.stringify(volumeInspect)}`);
    }
    receipt.volume = {
      createArgv: ["docker", ...volumeCreateArgv],
      createStdout: volumeOut.trim(),
      inspect: volumeInspect,
    };
    const { stdout: cidOut } = await docker(
      "run",
      "-d",
      "--name",
      containerName,
      "--platform",
      "linux/arm64",
      "--init",
      "--stop-timeout",
      "1",
      "--cpus",
      "1",
      "--memory",
      "512m",
      "--memory-swap",
      "512m",
      "--pids-limit",
      "512",
      "--device",
      "/dev/fuse:rwm",
      "--cap-add",
      "SYS_ADMIN",
      "--network",
      "none",
      "--tmpfs",
      "/tmp:rw,nosuid,nodev,size=1g,mode=1777",
      "--label",
      `dev.layerfs.owner=${containerName}`,
      "--mount",
      `type=volume,src=${volumeName},dst=/durable-state`,
      "--mount",
      `type=bind,src=${SCRIPT},dst=/harness/local-durable-fs-bench.mjs,readonly`,
      "-e",
      "FUSE_MOUNT=fuse",
      "-e",
      "MOUNT_POINT=/workspace",
      "-e",
      `PORT=${PORT}`,
      IMAGE,
    );
    containerCreated = true;
    const cid = cidOut.trim();
    receipt.containerId = cid;
    const initialInspect = await inspect(containerName);
    assertEnvelope(initialInspect, volumeName);
    await waitReady(containerName);
    const beforeIdentity = await processIdentity(containerName);

    const started = process.hrtime.bigint();
    const { stdout: commandOut } = await dockerExec(
      containerName,
      "bash",
      "-lc",
      `set -euo pipefail; head -c ${PAYLOAD_BYTES} /dev/urandom > ${TARGET}; sha256sum ${TARGET}; stat -c '%s' ${TARGET}; sync -f ${TARGET}; sync -f /workspace`,
    );
    const liveDone = process.hrtime.bigint();
    const [expectedHash, size] = commandOut
      .trim()
      .split(/\s+/)
      .filter((part) => !part.includes("/"));
    if (!/^[0-9a-f]{64}$/.test(expectedHash) || Number(size) !== PAYLOAD_BYTES) {
      throw new Error(`invalid timed command output: ${commandOut}`);
    }
    const pulled = await runBoundHelper(
      containerName,
      "--pull-barrier",
      DB_IN_CONTAINER,
      expectedHash,
    );
    const durableDone = process.hrtime.bigint();
    receipt.timingNs = {
      T_live: Number(liveDone - started),
      T_sync: Number(durableDone - liveDone),
      T_to_durable: Number(durableDone - started),
    };
    receipt.payload = { bytes: PAYLOAD_BYTES, sha256: expectedHash };
    receipt.pull = pulled;

    await docker("kill", "--signal", "KILL", containerName);
    const stopped = await inspect(containerName);
    if (stopped.State.ExitCode !== 137 || stopped.Id !== cid) {
      throw new Error(`unexpected stopped state: ${JSON.stringify(stopped.State)}`);
    }
    receipt.stopped = { exitCode: stopped.State.ExitCode, containerId: stopped.Id };

    const { stdout: startOutput } = await docker("start", containerName);
    const restarted = await inspect(containerName);
    if (restarted.Id !== cid) throw new Error("docker start changed the container identity");
    receipt.restart = { stdout: startOutput.trim(), containerId: restarted.Id };
    await waitReady(containerName);
    const afterIdentity = await processIdentity(containerName);
    if (
      beforeIdentity.hostPid === afterIdentity.hostPid ||
      beforeIdentity.startedAt === afterIdentity.startedAt
    ) {
      throw new Error("computerd process identity did not change across restart");
    }
    receipt.processIdentity = { before: beforeIdentity, after: afterIdentity };

    receipt.freshDbProcess = await runBoundHelper(
      containerName,
      "--verify-db",
      DB_IN_CONTAINER,
      expectedHash,
    );

    receipt.restore = await runBoundHelper(containerName, "--restore", DB_IN_CONTAINER);
    receipt.freshFuse = await verifyFuse(containerName, expectedHash);

    await dockerExec(
      containerName,
      "bash",
      "-lc",
      `set -euo pipefail; rm ${TARGET}; sync -f /workspace`,
    );
    receipt.authoritativeCleanup = await runBoundHelper(
      containerName,
      "--pull-cleanup",
      DB_IN_CONTAINER,
    );
    await docker("rm", "-f", containerName);
    containerCreated = false;
    const volumeRemoveArgv = ["volume", "rm", volumeName];
    const { stdout: volumeRemoveOut } = await docker(...volumeRemoveArgv);
    if (volumeRemoveOut.trim() !== volumeName) {
      throw new Error(`unexpected volume removal: ${volumeRemoveOut}`);
    }
    volumeCreated = false;
    receipt.cleanup = {
      volumeRemoveArgv: ["docker", ...volumeRemoveArgv],
      volumeRemoveStdout: volumeRemoveOut.trim(),
      containerAbsent: !(
        await docker("ps", "-a", "-q", "--filter", `name=^/${containerName}$`)
      ).stdout.trim(),
      volumeAbsent: !(
        await docker("volume", "ls", "-q", "--filter", `name=^${volumeName}$`)
      ).stdout.trim(),
    };
    if (!receipt.cleanup.containerAbsent || !receipt.cleanup.volumeAbsent) {
      throw new Error(`cleanup incomplete: ${JSON.stringify(receipt.cleanup)}`);
    }
    receipt.persistenceClass = "LOCAL_AUTHORITATIVE_SQLITE_DURABLE";
    receipt.status = "PASS";
  } catch (error) {
    receipt.error = error instanceof Error ? error.stack : String(error);
  } finally {
    if (containerCreated) {
      try {
        await docker("rm", "-f", containerName);
      } catch {
        // Retained as cleanup failure below.
      }
    }
    if (volumeCreated) {
      try {
        await docker("volume", "rm", volumeName);
      } catch {
        // Retained as cleanup failure below.
      }
    }
    try {
      receipt.cleanup.containerAbsent = !(
        await docker("ps", "-a", "-q", "--filter", `name=^/${containerName}$`)
      ).stdout.trim();
    } catch {
      receipt.cleanup.containerAbsent = false;
    }
    try {
      receipt.cleanup.volumeAbsent = !(
        await docker("volume", "ls", "-q", "--filter", `name=^${volumeName}$`)
      ).stdout.trim();
    } catch {
      receipt.cleanup.volumeAbsent = false;
    }
  }

  if (!receipt.cleanup.containerAbsent || !receipt.cleanup.volumeAbsent) {
    receipt.status = "FAIL";
    receipt.persistenceClass = "LOCAL_AUTHORITATIVE_SQLITE_DURABLE_PENDING";
    receipt.error ??= `terminal cleanup incomplete: ${JSON.stringify(receipt.cleanup)}`;
  }

  console.log(JSON.stringify(receipt, null, 2));
  if (receipt.status !== "PASS") process.exitCode = 1;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
