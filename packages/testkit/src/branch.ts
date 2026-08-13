import { EphemeralFS, type EphemeralBranch } from "@ephemeralai/fs";
import type { ConformanceAdapterFactory } from "./index.js";
import { recordPortableFixtureContext } from "./fixture-context.js";

export const PORTABLE_BRANCH_CASE_IDS = Object.freeze([
  "branch-frozen-base",
  "branch-50-independent",
  "branch-50-conflicting",
  "branch-sibling-order",
  "branch-aba-alias-conflicts",
  "branch-deterministic-results",
  "branch-pagination",
  "branch-recursive-conflict",
  "branch-terminal-handles",
  "branch-stream-snapshot",
  "branch-replay-reopen",
  "branch-result-expiry-reservation",
] as const);
export type PortableBranchCaseId = (typeof PORTABLE_BRANCH_CASE_IDS)[number];
export interface PortableBranchCaseResult {
  readonly id: PortableBranchCaseId;
  readonly status: "passed";
}

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`portable branch conformance: ${message}`);
}

function barrier(size: number): { wait(): Promise<void> } {
  let arrived = 0;
  let release!: () => void;
  const ready = new Promise<void>((resolve) => {
    release = resolve;
  });
  return Object.freeze({
    async wait() {
      arrived += 1;
      if (arrived === size) release();
      await ready;
    },
  });
}

async function text(stream: ReadableStream<Uint8Array>): Promise<string> {
  const decoder = new TextDecoder();
  let value = "";
  const reader = stream.getReader();
  try {
    for (;;) {
      const { done, value: chunk } = await reader.read();
      if (done) return value + decoder.decode();
      value += decoder.decode(chunk, { stream: true });
    }
  } finally {
    reader.releaseLock();
  }
}

async function expectBranchCode(
  operation: Promise<unknown>,
  code: string,
): Promise<void> {
  try {
    await operation;
  } catch (error) {
    invariant(
      error !== null &&
        typeof error === "object" &&
        "code" in error &&
        error.code === code,
      `expected ${code}, received ${String(error)}`,
    );
    return;
  }
  throw new Error(`portable branch conformance: expected ${code} rejection`);
}

async function expectRejection(
  operation: Promise<unknown>,
  message: string,
): Promise<void> {
  try {
    await operation;
  } catch {
    return;
  }
  throw new Error(`portable branch conformance: ${message}`);
}

/** Shared 50-writer, conflict, snapshot, replay, and restart branch suite. */
export async function runBranchConformance(
  factory: ConformanceAdapterFactory,
): Promise<readonly PortableBranchCaseResult[]> {
  const label = "portable-branches";
  const seed = 0xb2a6c4;
  const fixture = await factory.create({ label, seed });
  await recordPortableFixtureContext(factory, fixture.adapter, label, seed);
  const results: PortableBranchCaseResult[] = [];
  let adapter = fixture.adapter;
  let filesystem: EphemeralFS | undefined;
  const passed = (id: PortableBranchCaseId): void => {
    results.push(Object.freeze({ id, status: "passed" }));
  };
  try {
    let now = 1;
    const filesystemLimits = Object.freeze({ maxReaddirEntries: 16 });
    filesystem = await EphemeralFS.open({
      database: adapter,
      ownsDatabase: false,
      clock: () => now++,
      filesystem: filesystemLimits,
      storage: { maxGcBatchSize: 8, maxQueryBatchSize: 64 },
    });

    await filesystem.writeFile("/frozen", "base");
    const frozen = await filesystem.branches.create("portable-frozen");
    await filesystem.writeFile("/frozen", "main-after-base");
    invariant(
      (await frozen.readFile("/frozen", { encoding: "utf8" })) === "base",
      "branch did not retain its immutable base",
    );
    await frozen.writeFile("/frozen", "branch-value");
    const conflict = await frozen.publish({ operationId: "portable-frozen-op" });
    invariant(conflict.outcome === "conflict", "same-inode mutation did not conflict");
    invariant(
      (await filesystem.readFile("/frozen", { encoding: "utf8" })) ===
        "main-after-base",
      "conflict changed main",
    );
    invariant(
      JSON.stringify(
        await filesystem.branches.replay("portable-frozen-op", "portable-frozen"),
      ) === JSON.stringify(conflict),
      "conflict replay differs",
    );
    await frozen.close();
    passed("branch-frozen-base");

    const independent: EphemeralBranch[] = [];
    for (let index = 0; index < 50; index += 1) {
      const branch = await filesystem.branches.create(`portable-independent-${index}`);
      await branch.writeFile(`/portable-file-${index}`, `value-${index}`);
      independent.push(branch);
    }
    const independentBarrier = barrier(independent.length);
    const independentResults = await Promise.all(
      independent.map(async (branch) => {
        await independentBarrier.wait();
        return branch.publish();
      }),
    );
    invariant(
      independentResults.every((result) => result.outcome === "merged"),
      "one of 50 independent publications did not merge",
    );
    const revisions = independentResults
      .map((result) => Number(result.revision))
      .sort((left, right) => left - right);
    invariant(
      revisions.every(
        (revision, index) => index === 0 || revision === revisions[index - 1]! + 1,
      ),
      "independent publications did not form one consecutive revision chain",
    );
    await Promise.all(independent.map((branch) => branch.close()));
    for (let index = 0; index < 50; index += 1)
      invariant(
        (await filesystem.readFile(`/portable-file-${index}`, {
          encoding: "utf8",
        })) === `value-${index}`,
        `independent publication ${index} is missing`,
      );
    passed("branch-50-independent");

    await filesystem.writeFile("/portable-shared", "base");
    const competing: EphemeralBranch[] = [];
    for (let index = 0; index < 50; index += 1) {
      const branch = await filesystem.branches.create(`portable-same-${index}`);
      await branch.writeFile("/portable-shared", `writer-${index}`);
      competing.push(branch);
    }
    const competingBarrier = barrier(competing.length);
    const competingResults = await Promise.all(
      competing.map(async (branch, index) => {
        await competingBarrier.wait();
        return branch.publish({ operationId: `portable-same-op-${index}` });
      }),
    );
    invariant(
      competingResults.filter((result) => result.outcome === "merged").length === 1,
      "50 same-inode writers did not produce exactly one merge",
    );
    invariant(
      competingResults.filter((result) => result.outcome === "conflict").length === 49,
      "50 same-inode writers did not produce exactly 49 conflicts",
    );
    invariant(
      /^writer-(?:[0-9]|[1-4][0-9])$/u.test(
        await filesystem.readFile("/portable-shared", { encoding: "utf8" }),
      ),
      "main does not contain the one winning writer",
    );
    await Promise.all(competing.map((branch) => branch.close()));
    passed("branch-50-conflicting");

    const siblingLeft = await filesystem.branches.create("portable-sibling-left");
    const siblingRight = await filesystem.branches.create("portable-sibling-right");
    await siblingLeft.writeFile("/portable-sibling-left", "left");
    await siblingRight.writeFile("/portable-sibling-right", "right");
    const rightFirst = await siblingRight.publish();
    const leftSecond = await siblingLeft.publish();
    invariant(
      rightFirst.outcome === "merged" && leftSecond.outcome === "merged",
      "independent siblings did not merge in reverse creation order",
    );
    invariant(
      leftSecond.parentRevision === rightFirst.revision,
      "sibling publications did not form the total parent chain",
    );
    await siblingLeft.close();
    await siblingRight.close();
    passed("branch-sibling-order");

    await filesystem.writeFile("/portable-aba", "same");
    const aba = await filesystem.branches.create("portable-aba-branch");
    await aba.writeFile("/portable-aba", "branch");
    await filesystem.unlink("/portable-aba");
    await filesystem.writeFile("/portable-aba", "same");
    invariant(
      (await aba.publish()).outcome === "conflict",
      "delete/recreate ABA did not conflict",
    );
    await aba.close();
    await filesystem.writeFile("/portable-alias-source", "base");
    await filesystem.link("/portable-alias-source", "/portable-alias");
    const alias = await filesystem.branches.create("portable-alias-branch");
    await alias.writeFile("/portable-alias", "branch-alias");
    await filesystem.writeFile("/portable-alias-source", "main-alias");
    invariant(
      (await alias.publish()).outcome === "conflict",
      "hard-link alias mutation did not conflict",
    );
    await alias.close();
    passed("branch-aba-alias-conflicts");

    const sorted = await filesystem.branches.create("portable-sorted-results");
    await sorted.writeFile("/portable-sort-z", "z");
    await sorted.writeFile("/portable-sort-a", "a");
    const sortedResult = await sorted.publish({ operationId: "portable-sorted-op" });
    invariant(sortedResult.outcome === "merged", "sorted result did not merge");
    invariant(
      JSON.stringify(sortedResult.changedPaths) ===
        JSON.stringify(["/portable-sort-a", "/portable-sort-z"]),
      "changed paths are not exact and sorted",
    );
    await sorted.close();
    const empty = await filesystem.branches.create("portable-empty-publication");
    const emptyResult = await empty.publish();
    invariant(
      emptyResult.outcome === "merged" && emptyResult.changedPaths.length === 0,
      "empty publication was not an auditable revision",
    );
    await empty.close();
    const mismatch = await filesystem.branches.create("portable-replay-mismatch");
    await expectBranchCode(
      filesystem.branches.replay("portable-sorted-op", mismatch.id),
      "OperationBranchMismatch",
    );
    await mismatch.discard();
    await mismatch.close();
    passed("branch-deterministic-results");

    await filesystem.mkdir("/portable-branch-pages");
    const paged = await filesystem.branches.create("portable-branch-pages");
    for (let index = 0; index < 17; index += 1)
      await paged.writeFile(
        `/portable-branch-pages/entry-${index.toString().padStart(2, "0")}`,
        new Uint8Array(),
      );
    await expectBranchCode(paged.readdir("/portable-branch-pages"), "EFBIG");
    const pageNames: string[] = [];
    let pageCursor: string | undefined;
    for (;;) {
      const page = await paged.readdir("/portable-branch-pages", {
        limit: 5,
        ...(pageCursor === undefined ? {} : { startAfter: pageCursor }),
      });
      pageNames.push(...page.map((entry) => entry.name));
      if (page.length < 5) break;
      pageCursor = page.at(-1)!.name;
    }
    invariant(
      pageNames.length === 17 && new Set(pageNames).size === 17,
      "branch pagination skipped or repeated entries",
    );
    await paged.discard();
    await paged.close();
    passed("branch-pagination");

    await filesystem.mkdir("/portable-recursive/tree", { recursive: true });
    await filesystem.writeFile("/portable-recursive/tree/value", "base");
    const recursive = await filesystem.branches.create("portable-recursive-conflict");
    await recursive.rm("/portable-recursive", { recursive: true });
    await filesystem.writeFile("/portable-recursive/tree/value", "main-changed");
    const recursiveResult = await recursive.publish({
      operationId: "portable-recursive-conflict-op",
    });
    invariant(
      recursiveResult.outcome === "conflict",
      "recursive descendant mutation did not conflict",
    );
    invariant(
      (await filesystem.readFile("/portable-recursive/tree/value", {
        encoding: "utf8",
      })) === "main-changed",
      "recursive conflict changed main",
    );
    invariant(
      (await recursive.info()).state === "active",
      "conflicted recursive branch lost its active overlay",
    );
    await recursive.discard();
    await recursive.close();
    passed("branch-recursive-conflict");

    const discarded = await filesystem.branches.create("portable-terminal-id");
    const terminal = await discarded.discard();
    invariant(terminal.state === "discarded", "discard did not become terminal");
    await discarded.close();
    await expectRejection(
      filesystem.branches.create("portable-terminal-id"),
      "terminal branch identifier was reused",
    );
    const handles = await filesystem.branches.create("portable-independent-handles");
    const secondHandle = await filesystem.branches.open(handles.id);
    await handles.close();
    await handles.close();
    await secondHandle.writeFile("/portable-second-handle", "usable");
    invariant(
      (await secondHandle.publish()).outcome === "merged",
      "closing one branch handle closed its peer",
    );
    await secondHandle.close();
    passed("branch-terminal-handles");

    await filesystem.writeFile("/portable-stream", "base");
    const streamed = await filesystem.branches.create("portable-stream-branch");
    await streamed.writeFile("/portable-stream", "snapshot");
    const selected = await streamed.readStream("/portable-stream");
    await streamed.writeFile("/portable-stream", "later");
    await streamed.discard();
    invariant((await text(selected)) === "snapshot", "branch stream snapshot changed");
    await streamed.close();
    passed("branch-stream-snapshot");

    const replayed = await filesystem.branches.create("portable-replay");
    await replayed.writeFile("/portable-replayed", "durable");
    const published = await replayed.publish({ operationId: "portable-replay-op" });
    invariant(published.outcome === "merged", "replay fixture did not merge");
    await replayed.close();
    await filesystem.close();
    filesystem = undefined;
    adapter.close();
    adapter = await fixture.reopen({ physical: true });
    filesystem = await EphemeralFS.open({
      database: adapter,
      ownsDatabase: false,
      clock: () => now++,
      filesystem: filesystemLimits,
      storage: { maxGcBatchSize: 8, maxQueryBatchSize: 64 },
    });
    invariant(
      JSON.stringify(
        await filesystem.branches.replay("portable-replay-op", "portable-replay"),
      ) === JSON.stringify(published),
      "publication replay changed across physical reopen",
    );
    invariant(
      (await filesystem.readFile("/portable-replayed", { encoding: "utf8" })) ===
        "durable",
      "published bytes changed across physical reopen",
    );
    passed("branch-replay-reopen");

    const expiring = await filesystem.branches.create("portable-expiring-result");
    await expiring.writeFile("/portable-expiring-value", "durable-result");
    const expiringResult = await expiring.publish({
      operationId: "portable-expiring-operation",
    });
    invariant(expiringResult.outcome === "merged", "expiring result did not merge");
    await expiring.close();
    now = 31 * 24 * 60 * 60 * 1000;
    let expiryCollection = await filesystem.maintenance.collectGarbage({
      runId: "portable-expire-publication",
      maxBatches: 1,
    });
    for (
      let call = 0;
      call < 10_000 && expiryCollection.state !== "complete";
      call += 1
    )
      expiryCollection = await filesystem.maintenance.collectGarbage({
        runId: "portable-expire-publication",
        maxBatches: 1,
      });
    invariant(
      expiryCollection.state === "complete",
      "expiry collection did not finish",
    );
    await expectBranchCode(
      filesystem.branches.replay(
        "portable-expiring-operation",
        "portable-expiring-result",
      ),
      "OperationResultExpired",
    );
    await expectRejection(
      filesystem.branches.create("portable-expiring-result"),
      "expired branch identifier was reused",
    );
    passed("branch-result-expiry-reservation");
    return Object.freeze(results);
  } finally {
    try {
      await filesystem?.close();
    } catch {}
    try {
      adapter.close();
    } catch {}
    await fixture.dispose();
  }
}
