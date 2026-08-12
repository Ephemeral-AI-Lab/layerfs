// Small M4 branch-engine benchmark matrix.
//
// This is intentionally separate from publication.test.mjs: correctness and
// fault coverage remain authoritative there, while this harness measures the
// public branch API with fixture setup and verification outside the timed
// region.
//
// Usage:
//   node tests/performance/branch-bench.mjs
//   node tests/performance/branch-bench.mjs --cell=conflict --trials=3
//   node tests/performance/branch-bench.mjs --artifacts=C:\\tmp\\branch-bench

import { createHash } from "node:crypto";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { performance } from "node:perf_hooks";
import { BranchError, EphemeralFS } from "../../packages/fs/dist/index.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";

const SEED = 0x5eed_5eed;
const DEFAULT_TRIALS = 1;
const BRANCH_COUNTS = [1, 5, 10];
const PATHS_PER_BRANCH = [1, 10, 100];
const EDIT_COUNTS = [10, 100, 500];
const MIB = 1024 * 1024;

function argument(name, fallback = undefined) {
  const prefix = `--${name}=`;
  return (
    process.argv.find((value) => value.startsWith(prefix))?.slice(prefix.length) ??
    fallback
  );
}

function deterministicBytes(length, seed) {
  let state = seed >>> 0;
  const bytes = new Uint8Array(length);
  for (let offset = 0; offset < length; offset += 4) {
    state = (state + 0x6d2b79f5) >>> 0;
    let value = state;
    value = Math.imul(value ^ (value >>> 15), value | 1);
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
    value = (value ^ (value >>> 14)) >>> 0;
    bytes[offset] = value & 0xff;
    if (offset + 1 < length) bytes[offset + 1] = (value >>> 8) & 0xff;
    if (offset + 2 < length) bytes[offset + 2] = (value >>> 16) & 0xff;
    if (offset + 3 < length) bytes[offset + 3] = value >>> 24;
  }
  return bytes;
}

function digest(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function barrier(size) {
  let arrived = 0;
  let release;
  const ready = new Promise((resolve) => {
    release = resolve;
  });
  return async () => {
    arrived += 1;
    if (arrived === size) release();
    await ready;
  };
}

async function openFilesystem(filename = ":memory:", options = {}) {
  const database = await openNodeSqlite({ filename });
  const filesystem = await EphemeralFS.open({ database, ...options });
  return { database, filesystem };
}

async function closeFilesystem(database, filesystem) {
  await filesystem.close();
  database.close();
}

function resultSummary(results) {
  return {
    merged: results.filter((result) => result.outcome === "merged").length,
    conflicts: results.filter((result) => result.outcome === "conflict").length,
    changedPaths: results.reduce((sum, result) => sum + result.changedPaths.length, 0),
    conflictCount: results.reduce((sum, result) => sum + result.conflicts.length, 0),
    maxResultBytes: Math.max(...results.map((result) => JSON.stringify(result).length)),
  };
}

async function publishConcurrently(branches, operationPrefix) {
  const wait = barrier(branches.length);
  const started = performance.now();
  const samples = await Promise.all(
    branches.map(async (branch, index) => {
      await wait();
      const publicationStarted = performance.now();
      const result = await branch.publish({
        operationId: `${operationPrefix}-${index}`,
      });
      return {
        result,
        elapsedMs: performance.now() - publicationStarted,
      };
    }),
  );
  return {
    results: samples.map((sample) => sample.result),
    publicationMs: performance.now() - started,
    publicationSamplesMs: samples.map((sample) => sample.elapsedMs),
  };
}

async function runIndependent(branchCount, pathsPerBranch, trial) {
  const { database, filesystem } = await openFilesystem();
  const branches = [];
  const preparationStarted = performance.now();
  try {
    for (let branchIndex = 0; branchIndex < branchCount; branchIndex += 1) {
      const branch = await filesystem.branches.create(
        `bench-independent-${branchCount}-${pathsPerBranch}-${trial}-${branchIndex}`,
      );
      for (let pathIndex = 0; pathIndex < pathsPerBranch; pathIndex += 1) {
        await branch.writeFile(
          `/independent-${branchIndex}-${pathIndex}`,
          `value-${branchIndex}-${pathIndex}`,
        );
      }
      branches.push(branch);
    }
    const preparationMs = performance.now() - preparationStarted;
    const publication = await publishConcurrently(
      branches,
      `bench-independent-${branchCount}-${pathsPerBranch}-${trial}`,
    );
    const summary = resultSummary(publication.results);
    if (summary.merged !== branchCount || summary.conflicts !== 0) {
      throw new Error("independent fan-out produced an unexpected publication result");
    }
    for (let branchIndex = 0; branchIndex < branchCount; branchIndex += 1) {
      for (let pathIndex = 0; pathIndex < pathsPerBranch; pathIndex += 1) {
        const actual = await filesystem.readFile(
          `/independent-${branchIndex}-${pathIndex}`,
          { encoding: "utf8" },
        );
        if (actual !== `value-${branchIndex}-${pathIndex}`) {
          throw new Error("independent fan-out verification failed");
        }
      }
    }
    return {
      preparationMs,
      ...publication,
      ...summary,
      expectedPaths: branchCount * pathsPerBranch,
    };
  } finally {
    await Promise.all(branches.map((branch) => branch.close()));
    await closeFilesystem(database, filesystem);
  }
}

async function runSameInode(branchCount, trial) {
  const { database, filesystem } = await openFilesystem();
  const branches = [];
  const preparationStarted = performance.now();
  try {
    await filesystem.writeFile("/shared", "base");
    for (let index = 0; index < branchCount; index += 1) {
      const branch = await filesystem.branches.create(
        `bench-conflict-${branchCount}-${trial}-${index}`,
      );
      await branch.writeFile("/shared", `writer-${index}`);
      branches.push(branch);
    }
    const preparationMs = performance.now() - preparationStarted;
    const publication = await publishConcurrently(
      branches,
      `bench-conflict-${branchCount}-${trial}`,
    );
    const summary = resultSummary(publication.results);
    if (summary.merged !== 1 || summary.conflicts !== branchCount - 1) {
      throw new Error("same-inode writers produced an unexpected result");
    }
    const finalValue = await filesystem.readFile("/shared", { encoding: "utf8" });
    if (!/^writer-\d+$/.test(finalValue)) throw new Error("winner verification failed");
    return { preparationMs, ...publication, ...summary };
  } finally {
    await Promise.all(branches.map((branch) => branch.close()));
    await closeFilesystem(database, filesystem);
  }
}

async function runHardLinkConflict(trial) {
  const { database, filesystem } = await openFilesystem();
  const branches = [];
  const preparationStarted = performance.now();
  try {
    await filesystem.writeFile("/source", "base");
    await filesystem.link("/source", "/alias");
    const first = await filesystem.branches.create(`bench-alias-${trial}-first`);
    const second = await filesystem.branches.create(`bench-alias-${trial}-second`);
    await first.writeRange("/source", 0, new Uint8Array([65]));
    await second.writeRange("/alias", 0, new Uint8Array([66]));
    branches.push(first, second);
    const preparationMs = performance.now() - preparationStarted;
    const publication = await publishConcurrently(branches, `bench-alias-${trial}`);
    const summary = resultSummary(publication.results);
    if (summary.merged !== 1 || summary.conflicts !== 1) {
      throw new Error("hard-link conflict produced an unexpected result");
    }
    const source = await filesystem.stat("/source");
    const alias = await filesystem.stat("/alias");
    if (source.id !== alias.id || source.nlink !== 2) {
      throw new Error("hard-link identity verification failed");
    }
    return { preparationMs, ...publication, ...summary };
  } finally {
    await Promise.all(branches.map((branch) => branch.close()));
    await closeFilesystem(database, filesystem);
  }
}

async function runOverlay(edits, trial) {
  const { database, filesystem } = await openFilesystem();
  let branch;
  const preparationStarted = performance.now();
  try {
    const pageBytes = filesystem.capabilities.format.cowPageBytes;
    const base = deterministicBytes(
      Math.max(MIB, (edits + 1) * pageBytes),
      SEED + edits,
    );
    await filesystem.writeFile("/cow", base);
    branch = await filesystem.branches.create(`bench-cow-${edits}-${trial}`);
    for (let index = 0; index < edits; index += 1) {
      await branch.writeRange(
        "/cow",
        index * pageBytes,
        new Uint8Array([(index + 1) % 251]),
      );
    }
    const preparationMs = performance.now() - preparationStarted;
    const publicationStarted = performance.now();
    const result = await branch.publish({ operationId: `bench-cow-${edits}-${trial}` });
    const publicationMs = performance.now() - publicationStarted;
    const actual = await filesystem.readFile("/cow");
    if (digest(actual) === digest(base))
      throw new Error("COW edit did not change content");
    if (result.outcome !== "merged") throw new Error("COW publication did not merge");
    return {
      preparationMs,
      publicationMs,
      merged: 1,
      conflicts: 0,
      changedPaths: result.changedPaths.length,
      conflictCount: 0,
      maxResultBytes: JSON.stringify(result).length,
      pageBytes,
      edits,
      outputBytes: actual.byteLength,
    };
  } finally {
    if (branch) await branch.close();
    await closeFilesystem(database, filesystem);
  }
}

async function runPatches(edits, trial) {
  const { database, filesystem } = await openFilesystem();
  let branch;
  const preparationStarted = performance.now();
  try {
    const base = deterministicBytes(MIB, SEED ^ edits);
    await filesystem.writeFile("/patch", base);
    branch = await filesystem.branches.create(`bench-patch-${edits}-${trial}`);
    for (let index = 0; index < edits; index += 1) {
      await branch.replaceRange(
        "/patch",
        (index * 97) % base.byteLength,
        0,
        new Uint8Array([(index + 17) % 251]),
      );
    }
    const preparationMs = performance.now() - preparationStarted;
    const publicationStarted = performance.now();
    const result = await branch.publish({
      operationId: `bench-patch-${edits}-${trial}`,
    });
    const publicationMs = performance.now() - publicationStarted;
    const actual = await filesystem.readFile("/patch");
    if (actual.byteLength !== base.byteLength + edits) {
      throw new Error("structural patch output size mismatch");
    }
    if (result.outcome !== "merged") throw new Error("patch publication did not merge");
    return {
      preparationMs,
      publicationMs,
      merged: 1,
      conflicts: 0,
      changedPaths: result.changedPaths.length,
      conflictCount: 0,
      maxResultBytes: JSON.stringify(result).length,
      edits,
      outputBytes: actual.byteLength,
    };
  } finally {
    if (branch) await branch.close();
    await closeFilesystem(database, filesystem);
  }
}

async function runReplay(trial) {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-branch-replay-"));
  const filename = path.join(directory, "database.sqlite");
  let first;
  let database;
  let filesystem;
  const preparationStarted = performance.now();
  try {
    ({ database, filesystem } = await openFilesystem(filename));
    await filesystem.writeFile("/replay", "base");
    first = await filesystem.branches.create(`bench-replay-${trial}`);
    await first.writeFile("/replay", "published");
    const preparationMs = performance.now() - preparationStarted;
    const publicationStarted = performance.now();
    const original = await first.publish({ operationId: `bench-replay-op-${trial}` });
    const publicationMs = performance.now() - publicationStarted;
    await first.close();
    await closeFilesystem(database, filesystem);
    ({ database, filesystem } = await openFilesystem(filename));
    const replayStarted = performance.now();
    const replay = await filesystem.branches.replay(
      `bench-replay-op-${trial}`,
      `bench-replay-${trial}`,
    );
    const replayMs = performance.now() - replayStarted;
    if (JSON.stringify(replay) !== JSON.stringify(original)) {
      throw new Error("replay result differs after physical reopen");
    }
    return {
      preparationMs,
      publicationMs,
      replayMs,
      merged: original.outcome === "merged" ? 1 : 0,
      conflicts: original.outcome === "conflict" ? 1 : 0,
      changedPaths: original.changedPaths.length,
      conflictCount: original.conflicts.length,
      maxResultBytes: JSON.stringify(original).length,
    };
  } finally {
    if (filesystem) await filesystem.close();
    if (database) database.close();
    await rm(directory, { recursive: true, force: true });
  }
}

async function runLimitCheck(trial) {
  const { database, filesystem } = await openFilesystem(":memory:", {
    branch: { maxChangedPathsPerBranch: 10 },
  });
  let branch;
  const started = performance.now();
  try {
    branch = await filesystem.branches.create(`bench-limit-${trial}`);
    for (let index = 0; index < 11; index += 1) {
      try {
        await branch.writeFile(`/limit-${index}`, "x");
      } catch (error) {
        if (!(error instanceof BranchError) || error.code !== "LimitExceeded")
          throw error;
        return {
          preparationMs: performance.now() - started,
          publicationMs: 0,
          merged: 0,
          conflicts: 0,
          changedPaths: index,
          conflictCount: 0,
          maxResultBytes: 0,
          rejectedAt: index + 1,
          limit: "maxChangedPathsPerBranch",
        };
      }
    }
    throw new Error("changed-path limit did not reject the eleventh path");
  } finally {
    if (branch) await branch.close();
    await closeFilesystem(database, filesystem);
  }
}

function configuration(name, values) {
  return { name, ...values };
}

function artifactName(cell) {
  return Object.entries(cell)
    .map(([key, value]) => `${key}-${value}`)
    .join("-")
    .replaceAll(/[^a-zA-Z0-9-]/g, "_");
}

async function executeCell(cell, trial) {
  switch (cell.name) {
    case "independent":
      return runIndependent(cell.branchCount, cell.pathsPerBranch, trial);
    case "same-inode":
      return runSameInode(cell.branchCount, trial);
    case "hard-link":
      return runHardLinkConflict(trial);
    case "cow":
      return runOverlay(cell.edits, trial);
    case "patch":
      return runPatches(cell.edits, trial);
    case "replay":
      return runReplay(trial);
    case "limit":
      return runLimitCheck(trial);
    default:
      throw new Error(`unknown branch benchmark cell ${cell.name}`);
  }
}

function cellsFor(selection) {
  const cells = [];
  if (selection === "all" || selection === "independent") {
    for (const branchCount of BRANCH_COUNTS) {
      for (const pathsPerBranch of PATHS_PER_BRANCH) {
        cells.push(configuration("independent", { branchCount, pathsPerBranch }));
      }
    }
  }
  if (selection === "all" || selection === "conflict") {
    for (const branchCount of [5, 10]) {
      cells.push(configuration("same-inode", { branchCount }));
    }
    cells.push(configuration("hard-link", { branchCount: 2 }));
  }
  if (selection === "all" || selection === "overlay") {
    for (const edits of EDIT_COUNTS) cells.push(configuration("cow", { edits }));
  }
  if (selection === "all" || selection === "patch") {
    for (const edits of EDIT_COUNTS) cells.push(configuration("patch", { edits }));
  }
  if (selection === "all" || selection === "replay")
    cells.push(configuration("replay", {}));
  if (selection === "all" || selection === "limit")
    cells.push(configuration("limit", {}));
  if (cells.length === 0) throw new Error(`unknown --cell=${selection}`);
  return cells;
}

function percentile(values, percentileValue) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[
    Math.min(sorted.length - 1, Math.ceil(sorted.length * percentileValue) - 1)
  ];
}

async function main() {
  const selection = argument("cell", "all");
  const trials = Math.max(1, Number(argument("trials", DEFAULT_TRIALS)));
  if (!Number.isSafeInteger(trials))
    throw new Error("--trials must be a positive integer");
  const requestedArtifacts = argument("artifacts");
  const artifactsDirectory = requestedArtifacts
    ? path.resolve(requestedArtifacts)
    : await mkdtemp(path.join(tmpdir(), "efs-branch-bench-artifacts-"));
  await mkdir(artifactsDirectory, { recursive: true });
  const cells = cellsFor(selection);
  const started = performance.now();
  const artifacts = [];
  console.log(`branch-bench: cells=${cells.length}, trials=${trials}`);
  console.log(`branch-bench artifacts: ${artifactsDirectory}`);

  for (const cell of cells) {
    const samples = [];
    let pass = true;
    let errorMessage;
    for (let trial = 0; trial < trials; trial += 1) {
      try {
        samples.push(await executeCell(cell, trial));
      } catch (error) {
        pass = false;
        errorMessage = error instanceof Error ? error.message : String(error);
        break;
      }
    }
    const publicationSamples = samples
      .map((sample) => sample.publicationMs)
      .filter((value) => Number.isFinite(value));
    const artifact = {
      schema: "efs-benchmark-result-v1",
      benchmark: `M4-${cell.name}`,
      engine: "ephemeral-ai-fs",
      driver: "sqlite-node",
      fixture: { name: "small-branch-matrix", seed: SEED },
      configuration: cell,
      trials: samples.length,
      latencyMs: publicationSamples.length
        ? {
            p50: percentile(publicationSamples, 0.5),
            p95: percentile(publicationSamples, 0.95),
            p99: percentile(publicationSamples, 0.99),
          }
        : null,
      counters: samples.length
        ? samples.reduce(
            (summary, sample) => ({
              preparationMs: summary.preparationMs + sample.preparationMs,
              publicationMs: summary.publicationMs + (sample.publicationMs ?? 0),
              replayMs: summary.replayMs + (sample.replayMs ?? 0),
              merged: summary.merged + sample.merged,
              conflicts: summary.conflicts + sample.conflicts,
              changedPaths: summary.changedPaths + sample.changedPaths,
              conflictCount: summary.conflictCount + sample.conflictCount,
            }),
            {
              preparationMs: 0,
              publicationMs: 0,
              replayMs: 0,
              merged: 0,
              conflicts: 0,
              changedPaths: 0,
              conflictCount: 0,
            },
          )
        : null,
      samples,
      pass,
      ...(errorMessage ? { error: errorMessage } : {}),
    };
    artifacts.push(artifact);
    await writeFile(
      path.join(artifactsDirectory, `${artifactName(cell)}.json`),
      `${JSON.stringify(artifact, null, 2)}\n`,
    );
    console.log(
      `${cell.name.padEnd(12)} ${JSON.stringify(cell).padEnd(58)} ${pass ? "PASS" : "FAIL"}`,
    );
  }

  await writeFile(
    path.join(artifactsDirectory, "index.json"),
    `${JSON.stringify({ schema: "efs-branch-bench-v1", artifacts }, null, 2)}\n`,
  );
  const failed = artifacts.filter((artifact) => !artifact.pass).length;
  const elapsedMs = performance.now() - started;
  console.log(`branch-bench total: ${elapsedMs.toFixed(1)} ms`);
  console.log(
    `branch-bench result: ${artifacts.length - failed} passed, ${failed} failed`,
  );
  if (failed) process.exitCode = 1;
}

await main();
