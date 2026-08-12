import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { BranchError, EphemeralFS } from "../../packages/fs/dist/index.js";
import { EphemeralFS as OperationsFilesystem } from "../../packages/fs/dist/operations/filesystem.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import { createSqliteOperationsStorage } from "../../packages/fs/dist/sqlite/operations-storage.js";

async function setup(filename = ":memory:", options = {}) {
  const database = await openNodeSqlite({ filename });
  const filesystem = await EphemeralFS.open({ database, ...options });
  return { database, filesystem };
}

function faultingDriver(driver, control) {
  return Object.freeze({
    kind: driver.kind,
    readOnly: driver.readOnly,
    capabilities: driver.capabilities,
    hashBytes: driver.hashBytes?.bind(driver),
    hashBytesAsync: driver.hashBytesAsync?.bind(driver),
    transaction(mode, callback) {
      return driver.transaction(mode, (tx) => {
        if (!control.armed) return callback(tx);
        let position = 0;
        const wrapped = {
          scope: tx.scope,
          run(sql, bindings = []) {
            position += 1;
            control.maxPosition = Math.max(control.maxPosition, position);
            if (position === control.failAt) {
              if (control.disarmOnFault) control.armed = false;
              throw new Error(`publication fault ${control.failAt}`);
            }
            return tx.run(sql, bindings);
          },
          all(sql, bindings, budget) {
            position += 1;
            control.maxPosition = Math.max(control.maxPosition, position);
            if (position === control.failAt) {
              if (control.disarmOnFault) control.armed = false;
              throw new Error(`publication fault ${control.failAt}`);
            }
            return tx.all(sql, bindings, budget);
          },
        };
        try {
          return callback(wrapped);
        } finally {
          control.maxPosition = Math.max(control.maxPosition, position);
        }
      });
    },
    physicalStorage: () => driver.physicalStorage?.(),
    checkpoint: (mode) => driver.checkpoint?.(mode),
    close: () => driver.close(),
  });
}

function branchDurableSnapshot(database, branchId = "fault") {
  return database.transaction(
    "read",
    (tx) =>
      tx.all(
        `SELECT
          (SELECT main_revision FROM efs_meta WHERE singleton=1) main_revision,
          (SELECT state FROM efs_branches WHERE id=?) branch_state,
          (SELECT generation FROM efs_branches WHERE id=?) branch_generation,
          (SELECT count(*) FROM efs_branch_changes WHERE branch_id=?) changes,
          (SELECT count(*) FROM efs_branch_inode_overlays WHERE branch_id=?) inode_overlays,
          (SELECT count(*) FROM efs_branch_manifest_roots WHERE branch_id=?) manifest_roots,
          (SELECT count(*) FROM efs_operation_ids) operation_ids,
          (SELECT count(*) FROM efs_operation_results) operation_results,
          (SELECT count(*) FROM efs_leases) leases,
          (SELECT count(*) FROM efs_staging_entries) staging_entries,
          (SELECT count(*) FROM efs_cow_page_versions WHERE branch_id=?) pages,
          (SELECT count(*) FROM efs_patches WHERE branch_id=?) patches,
          (SELECT charged_metadata_bytes FROM efs_usage WHERE singleton=1) metadata_bytes`,
        [branchId, branchId, branchId, branchId, branchId, branchId, branchId],
        { maxRows: 1, maxBytes: 4096 },
      )[0],
  );
}

function createBarrier(size) {
  let arrived = 0;
  let release;
  const ready = new Promise((resolve) => {
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

test("branch reads a frozen base and publishes one durable revision", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/file", "base");
  const branch = await filesystem.branches.create("feature");
  await filesystem.writeFile("/file", "main-after-base");
  assert.equal(await branch.readFile("/file", { encoding: "utf8" }), "base");
  await branch.writeFile("/file", "branch-value");
  assert.equal(
    await filesystem.readFile("/file", { encoding: "utf8" }),
    "main-after-base",
  );
  const conflict = await branch.publish({ operationId: "publish-feature" });
  assert.equal(conflict.outcome, "conflict");
  assert.equal(await branch.readFile("/file", { encoding: "utf8" }), "branch-value");
  assert.equal(
    await filesystem.readFile("/file", { encoding: "utf8" }),
    "main-after-base",
  );
  assert.deepEqual(
    await filesystem.branches.replay("publish-feature", "feature"),
    conflict,
  );
  await branch.close();
  await filesystem.close();
  database.close();
});

test("fifty independent writers form one parent chain", async () => {
  const { database, filesystem } = await setup();
  const branches = [];
  for (let index = 0; index < 50; index += 1) {
    const branch = await filesystem.branches.create(`independent-${index}`);
    await branch.writeFile(`/file-${index}`, `value-${index}`);
    branches.push(branch);
  }
  const barrier = createBarrier(branches.length);
  const results = await Promise.all(
    branches.map(async (branch) => {
      await barrier.wait();
      return branch.publish();
    }),
  );
  assert.equal(results.filter((result) => result.outcome === "merged").length, 50);
  const revisions = results
    .map((result) => Number(result.revision))
    .sort((left, right) => left - right);
  const chain = database.transaction("read", (tx) =>
    tx.all(
      `SELECT revision,parent_revision FROM efs_revisions WHERE revision IN (${revisions
        .map(() => "?")
        .join(",")}) ORDER BY revision`,
      revisions,
      { maxRows: 50, maxBytes: 8192 },
    ),
  );
  assert.equal(chain.length, 50);
  assert.equal(chain[0].parent_revision, 0);
  for (let index = 1; index < chain.length; index += 1)
    assert.equal(chain[index].parent_revision, chain[index - 1].revision);
  await Promise.all(branches.map((branch) => branch.close()));
  for (let index = 0; index < 50; index += 1)
    assert.equal(
      await filesystem.readFile(`/file-${index}`, { encoding: "utf8" }),
      `value-${index}`,
    );
  await filesystem.close();
  database.close();
});

test("fifty same-inode writers yield one merge and 49 explicit conflicts", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/shared", "base");
  const branches = [];
  for (let index = 0; index < 50; index += 1) {
    const branch = await filesystem.branches.create(`same-${index}`);
    await branch.writeFile("/shared", `writer-${index}`);
    branches.push(branch);
  }
  const barrier = createBarrier(branches.length);
  const outcomes = await Promise.all(
    branches.map(async (branch, index) => {
      await barrier.wait();
      return branch.publish({ operationId: `same-op-${index}` });
    }),
  );
  assert.equal(outcomes.filter((result) => result.outcome === "merged").length, 1);
  assert.equal(outcomes.filter((result) => result.outcome === "conflict").length, 49);
  assert.match(
    await filesystem.readFile("/shared", { encoding: "utf8" }),
    /^writer-(?:[0-9]|[1-4][0-9])$/,
  );
  await Promise.all(branches.map((branch) => branch.close()));
  await filesystem.close();
  database.close();
});

test("concurrent publications of one branch produce at most one revision", async () => {
  const { database, filesystem } = await setup();
  const first = await filesystem.branches.create("single-publish");
  await first.writeFile("/once", "value");
  const second = await filesystem.branches.open("single-publish");
  const outcomes = await Promise.allSettled([first.publish(), second.publish()]);
  assert.equal(outcomes.filter((result) => result.status === "fulfilled").length, 1);
  assert.equal(
    outcomes.filter(
      (result) =>
        result.status === "rejected" &&
        result.reason instanceof BranchError &&
        result.reason.code === "BranchNotActive",
    ).length,
    1,
  );
  assert.equal(await filesystem.readFile("/once", { encoding: "utf8" }), "value");
  const revisions = database.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT count(*) count FROM efs_revisions WHERE writer_id='branch:single-publish'",
        [],
        { maxRows: 1, maxBytes: 256 },
      )[0].count,
  );
  assert.equal(revisions, 1);
  await first.close();
  await second.close();
  await filesystem.close();
  database.close();
});

test("lost-response replay survives physical reopen and operation IDs cannot cross branches", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-branch-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    let { database, filesystem } = await setup(filename);
    const first = await filesystem.branches.create("first");
    await first.writeFile("/durable", "yes");
    const published = await first.publish({ operationId: "durable-op" });
    assert.equal(published.outcome, "merged");
    await first.close();
    await filesystem.close();
    database.close();
    ({ database, filesystem } = await setup(filename));
    assert.deepEqual(
      await filesystem.branches.replay("durable-op", "first"),
      published,
    );
    const second = await filesystem.branches.create("second");
    await second.writeFile("/other", "no");
    await assert.rejects(
      second.publish({ operationId: "durable-op" }),
      (error) =>
        error instanceof BranchError && error.code === "OperationBranchMismatch",
    );
    await second.close();
    await filesystem.close();
    database.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("publication rollback survives every durable statement fault", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  const initial = await EphemeralFS.open({ database });
  await initial.writeFile("/main", "base");
  const initialBranch = await initial.branches.create("fault");
  await initialBranch.writeFile("/branch", "value");
  await initialBranch.close();
  await initial.close();

  const control = {
    armed: false,
    disarmOnFault: true,
    failAt: Number.MAX_SAFE_INTEGER,
    maxPosition: 0,
  };
  const filesystem = await OperationsFilesystem.open(
    {},
    createSqliteOperationsStorage(faultingDriver(database, control)),
  );
  control.armed = true;
  const branch = await filesystem.branches.open("fault");
  let failAt = 1;
  while (true) {
    const before = branchDurableSnapshot(database);
    control.failAt = failAt;
    control.maxPosition = 0;
    try {
      const result = await branch.publish({ operationId: "fault-publication-op" });
      assert.equal(result.outcome, "merged");
      assert.ok(failAt > control.maxPosition);
      break;
    } catch (error) {
      assert.match(error.message, /publication fault/);
      control.armed = false;
      assert.deepEqual(branchDurableSnapshot(database), before);
      assert.equal((await branch.info()).state, "active");
      assert.equal(await filesystem.readFile("/main", { encoding: "utf8" }), "base");
      assert.equal(await branch.readFile("/branch", { encoding: "utf8" }), "value");
      failAt += 1;
    }
  }
  assert.ok(failAt > 1);
  await branch.close();
  await filesystem.close();
  database.close();
});

test("publication preparation candidates roll back and release staging at every fault position", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  let now = 0;
  const initial = await EphemeralFS.open({ database, clock: () => now });
  await initial.writeFile("/candidate", new Uint8Array(20_000).fill(1));
  const initialBranch = await initial.branches.create("candidate-fault");
  await initialBranch.writeRange("/candidate", 0, new Uint8Array([2]));
  await initialBranch.close();
  await initial.maintenance.collectGarbage({ runId: "candidate-initial-gc" });
  await initial.close();

  const control = {
    armed: false,
    disarmOnFault: true,
    failAt: Number.MAX_SAFE_INTEGER,
    maxPosition: 0,
  };
  const filesystem = await OperationsFilesystem.open(
    { clock: () => now },
    createSqliteOperationsStorage(faultingDriver(database, control)),
  );
  const branch = await filesystem.branches.open("candidate-fault");
  let baseline = branchDurableSnapshot(database, "candidate-fault");
  let reservationCommitted = false;
  let failAt = 1;
  while (true) {
    control.armed = true;
    control.failAt = failAt;
    control.maxPosition = 0;
    try {
      const result = await branch.publish({ operationId: "candidate-fault-op" });
      assert.equal(result.outcome, "merged");
      assert.ok(failAt > control.maxPosition);
      break;
    } catch (error) {
      assert.match(error.message, /publication fault/);
      assert.equal((await branch.info()).state, "active");
      now = 2_000_000;
      control.armed = false;
      await filesystem.maintenance.collectGarbage({
        runId: `candidate-fault-gc-${failAt}`,
      });
      const actual = branchDurableSnapshot(database, "candidate-fault");
      if (!reservationCommitted && actual.operation_ids === 1) {
        // The operation identifier is intentionally durable before preparation;
        // it is the permanent branch/generation binding, not a publication result.
        baseline = actual;
        reservationCommitted = true;
      } else {
        assert.deepEqual(actual, baseline);
      }
      assert.deepEqual(
        [...(await filesystem.readRange("/candidate", { offset: 0, length: 4 }))],
        [1, 1, 1, 1],
      );
      assert.deepEqual(
        [...(await branch.readRange("/candidate", { offset: 0, length: 4 }))],
        [2, 1, 1, 1],
      );
      failAt += 1;
    }
  }
  await branch.close();
  await filesystem.close();
  database.close();
});

test("branch stream is immutable across later edit and discard", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/stream", "base");
  const branch = await filesystem.branches.create("stream-branch");
  await branch.writeFile("/stream", "snapshot");
  const stream = await branch.readStream("/stream");
  await branch.writeFile("/stream", "later");
  await branch.discard();
  let text = "";
  for await (const chunk of stream) text += new TextDecoder().decode(chunk);
  assert.equal(text, "snapshot");
  assert.equal((await branch.info()).state, "discarded");
  await branch.close();
  await filesystem.close();
  database.close();
});

test("reopened branch streams retain their snapshot across main edits", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-branch-stream-reopen-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    let { database, filesystem } = await setup(filename);
    await filesystem.writeFile("/stream", "base");
    const created = await filesystem.branches.create("stream-reopen");
    await created.writeFile("/stream", "reopened-snapshot");
    await created.close();
    await filesystem.close();
    database.close();

    ({ database, filesystem } = await setup(filename));
    const branch = await filesystem.branches.open("stream-reopen");
    const stream = await branch.readStream("/stream");
    await filesystem.writeFile("/stream", "main-after-reopen");
    await branch.discard();
    let text = "";
    for await (const chunk of stream) text += new TextDecoder().decode(chunk);
    assert.equal(text, "reopened-snapshot");
    await branch.close();
    await filesystem.close();
    database.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("prepared branch content is released on attach and abandoned on mutation rejection", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  const filesystem = await EphemeralFS.open({
    database,
    branch: { maxChangedPathsPerBranch: 1 },
  });
  const branch = await filesystem.branches.create("lease-cleanup");
  await branch.writeFile("/first", "attached");
  await assert.rejects(
    branch.writeFile("/second", "must-be-abandoned"),
    (error) => error instanceof BranchError && error.code === "LimitExceeded",
  );
  const active = database.transaction(
    "read",
    (tx) =>
      tx.all("SELECT count(*) count FROM efs_leases WHERE state<>2", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].count,
  );
  assert.equal(active, 0);
  assert.equal(await branch.readFile("/first", { encoding: "utf8" }), "attached");
  await branch.close();
  await filesystem.close();
  database.close();
});

test("terminal lifecycle is durable, discard is idempotent, and identifiers are never reused", async () => {
  const { database, filesystem } = await setup();
  const branch = await filesystem.branches.create("terminal");
  await branch.mkdir("/discarded-content");
  const firstDiscard = await branch.discard();
  assert.equal(firstDiscard.state, "discarded");
  assert.equal(firstDiscard.mergedRevision, null);
  assert.deepEqual(
    database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT (SELECT count(*) FROM efs_branch_changes WHERE branch_id=?) changes,(SELECT count(*) FROM efs_branch_inode_expectations WHERE branch_id=?) expectations,(SELECT count(*) FROM efs_branch_inode_overlays WHERE branch_id=?) overlays,(SELECT count(*) FROM efs_branch_manifest_roots WHERE branch_id=?) roots",
          ["terminal", "terminal", "terminal", "terminal"],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    ),
    { changes: 0, expectations: 0, overlays: 0, roots: 0 },
  );
  assert.deepEqual(await branch.discard(), firstDiscard);
  const reopened = await filesystem.branches.open("terminal");
  assert.equal((await reopened.info()).state, "discarded");
  await reopened.close();
  await assert.rejects(
    branch.readFile("/missing"),
    (error) => error instanceof BranchError && error.code === "BranchNotActive",
  );
  await assert.rejects(filesystem.branches.create("terminal"));
  await branch.close();
  await filesystem.close();
  database.close();
});

test("hard-link aliases retain identity and conflict as one inode", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/source", "base");
  await filesystem.link("/source", "/alias");
  const first = await filesystem.branches.create("link-first");
  const second = await filesystem.branches.create("link-second");
  await first.writeFile("/alias", "first");
  await second.writeFile("/source", "second");
  assert.equal((await first.stat("/source")).id, (await first.stat("/alias")).id);
  const merged = await first.publish({ operationId: "link-first-op" });
  const conflict = await second.publish({ operationId: "link-second-op" });
  assert.equal(merged.outcome, "merged");
  assert.equal(conflict.outcome, "conflict");
  assert.equal(conflict.conflicts[0].reason, "node-changed");
  assert.equal(
    (await filesystem.stat("/source")).id,
    (await filesystem.stat("/alias")).id,
  );
  assert.equal(await filesystem.readFile("/source", { encoding: "utf8" }), "first");
  await first.close();
  await second.close();
  await filesystem.close();
  database.close();
});

test("branch unlink updates durable hard-link counts without changing the base", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/source", "base");
  await filesystem.link("/source", "/alias");
  const branch = await filesystem.branches.create("link-count");
  await branch.unlink("/alias");
  assert.equal((await branch.stat("/source")).nlink, 1);
  assert.equal((await filesystem.stat("/source")).nlink, 2);
  await branch.discard();
  await branch.close();
  await filesystem.close();
  database.close();
});

test("recursive removal detects descendant changes and leaves the branch unchanged", async () => {
  const { database, filesystem } = await setup();
  await filesystem.mkdir("/tree");
  await filesystem.writeFile("/tree/file", "base");
  const branch = await filesystem.branches.create("tree-branch");
  await branch.rm("/tree", { recursive: true });
  await filesystem.writeFile("/tree/file", "main-change");
  const result = await branch.publish({ operationId: "tree-op" });
  assert.equal(result.outcome, "conflict");
  assert.equal(
    result.conflicts.some((item) => item.reason === "subtree-changed"),
    true,
  );
  assert.equal(
    await filesystem.readFile("/tree/file", { encoding: "utf8" }),
    "main-change",
  );
  await assert.rejects(
    branch.readFile("/tree/file"),
    (error) => error instanceof Error && error.code === "ENOENT",
  );
  assert.deepEqual(await filesystem.branches.replay("tree-op", "tree-branch"), result);
  await branch.close();
  await filesystem.close();
  database.close();
});

test("empty directory subtree tokens support recursive branch deletion", async () => {
  const { database, filesystem } = await setup();
  await filesystem.mkdir("/empty-tree");
  const branch = await filesystem.branches.create("empty-tree-branch");
  await branch.rm("/empty-tree", { recursive: true });
  const result = await branch.publish({ operationId: "empty-tree-op" });
  assert.equal(result.outcome, "merged");
  await assert.rejects(
    filesystem.stat("/empty-tree"),
    (error) => error instanceof Error && error.code === "ENOENT",
  );
  await branch.close();
  await filesystem.close();
  database.close();
});

test("ancestor replacement reports an ancestor conflict instead of ENOTDIR", async () => {
  const { database, filesystem } = await setup();
  await filesystem.mkdir("/ancestor");
  await filesystem.writeFile("/ancestor/child", "base");
  const branch = await filesystem.branches.create("ancestor-conflict");
  await branch.writeFile("/ancestor/child", "branch");
  await filesystem.rm("/ancestor", { recursive: true });
  await filesystem.writeFile("/ancestor", "replacement");
  const result = await branch.publish({ operationId: "ancestor-conflict-op" });
  assert.equal(result.outcome, "conflict");
  assert.deepEqual(
    result.conflicts.map((conflict) => [conflict.path, conflict.reason]),
    [["/ancestor/child", "ancestor-changed"]],
  );
  assert.equal(
    await filesystem.readFile("/ancestor", { encoding: "utf8" }),
    "replacement",
  );
  assert.equal(
    await branch.readFile("/ancestor/child", { encoding: "utf8" }),
    "branch",
  );
  await branch.close();
  await filesystem.close();
  database.close();
});

test("reusing an operation after a branch mutation replays the original result", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/shared", "base");
  const branch = await filesystem.branches.create("generation");
  await branch.writeFile("/shared", "branch");
  await filesystem.writeFile("/shared", "main");
  const first = await branch.publish({ operationId: "generation-op" });
  assert.equal(first.outcome, "conflict");
  await branch.writeFile("/other", "other");
  assert.deepEqual(await branch.publish({ operationId: "generation-op" }), first);
  await branch.close();
  await filesystem.close();
  database.close();
});

test("repeated COW writes replace an unleased page predecessor", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/page", new Uint8Array(20_000));
  const branch = await filesystem.branches.create("cow");
  const inode = await branch.stat("/page");
  const pageBytes = filesystem.capabilities.format.cowPageBytes;
  const page = (fill) => new Uint8Array(Math.min(pageBytes, 20_000)).fill(fill);
  const storage = createSqliteOperationsStorage(database);
  const budget = {
    maxRows: filesystem.capabilities.storage.maxFinalTransactionRows,
    maxBytes: filesystem.capabilities.storage.maxFinalTransactionBytes,
  };
  storage.transaction("write", budget, (tx) => {
    tx.overlay(filesystem.capabilities.storage, pageBytes).writePages(
      "cow",
      inode.id,
      20_000,
      [{ index: 0, bytes: page(1) }],
      1,
    );
  });
  storage.transaction("write", budget, (tx) => {
    tx.overlay(filesystem.capabilities.storage, pageBytes).writePages(
      "cow",
      inode.id,
      20_000,
      [{ index: 0, bytes: page(2) }],
      2,
    );
  });
  const count = database.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT count(*) count FROM efs_cow_page_versions WHERE branch_id=?",
        ["cow"],
        {
          maxRows: 1,
          maxBytes: 128,
        },
      )[0].count,
  );
  assert.equal(count, 1);
  await branch.discard();
  await branch.close();
  await filesystem.close();
  database.close();
});

test("branch handle close invalidates its streams without affecting another handle", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/stream-close", "bytes");
  const first = await filesystem.branches.create("stream-close");
  const second = await filesystem.branches.open("stream-close");
  const stream = await first.readStream("/stream-close");
  await first.close();
  await assert.rejects(
    (async () => {
      for await (const _chunk of stream) {
        // The first pull observes the closed handle.
      }
    })(),
    (error) => error instanceof Error && error.code === "EBADF",
  );
  assert.equal((await second.info()).state, "active");
  await second.close();
  await filesystem.close();
  database.close();
});

test("closed branch handles reject every filesystem method and close drains mutations", async () => {
  const { database, filesystem } = await setup();
  const branch = await filesystem.branches.create("closed-handle");
  let unblock;
  const gate = new Promise((resolve) => {
    unblock = resolve;
  });
  const content = new ReadableStream({
    async pull(controller) {
      await gate;
      controller.enqueue(new TextEncoder().encode("delayed"));
      controller.close();
    },
  });
  const mutation = branch.writeFile("/delayed", content, { maxBytes: 7 });
  await new Promise((resolve) => setTimeout(resolve, 10));
  let closed = false;
  const closing = branch.close().then(() => {
    closed = true;
  });
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.equal(closed, false);
  unblock();
  await mutation;
  await closing;
  assert.equal(closed, true);
  for (const call of [
    () => branch.info(),
    () => branch.publish(),
    () => branch.discard(),
    () => branch.readFile("/delayed"),
    () => branch.readRange("/delayed", { offset: 0, length: 1 }),
    () => branch.readStream("/delayed"),
    () => branch.writeFile("/other", "x"),
    () => branch.writeRange("/delayed", 0, new Uint8Array([1])),
    () => branch.replaceRange("/delayed", 0, 0, new Uint8Array([1])),
    () => branch.truncate("/delayed", 0),
    () => branch.mkdir("/directory"),
    () => branch.readdir("/"),
    () => branch.stat("/delayed"),
    () => branch.lstat("/delayed"),
    () => branch.chmod("/delayed", 0o600),
    () => branch.link("/delayed", "/alias"),
    () => branch.symlink("/delayed", "/link"),
    () => branch.readlink("/link"),
    () => branch.rename("/delayed", "/renamed"),
    () => branch.unlink("/delayed"),
    () => branch.rm("/delayed"),
  ])
    await assert.rejects(call(), (error) => error?.code === "EBADF");
  await filesystem.close();
  database.close();
});

test("a scheduled branch stream cannot create a lease after handle close", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/scheduled-stream", "bytes");
  const branch = await filesystem.branches.create("scheduled-stream");
  const opening = branch.readStream("/scheduled-stream");
  const closing = branch.close();
  await assert.rejects(opening, (error) => error?.code === "EBADF");
  await closing;
  assert.equal(
    database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT count(*) count FROM efs_leases WHERE owner_id LIKE 'branch-stream:%'",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0].count,
    ),
    0,
  );
  await filesystem.close();
  database.close();
});

test("a mutation admitted before handle close drains to completion", async () => {
  const { database, filesystem } = await setup();
  const branch = await filesystem.branches.create("admitted-close");
  const mutation = branch.mkdir("/drained");
  await branch.close();
  await mutation;
  const reopened = await filesystem.branches.open("admitted-close");
  assert.equal((await reopened.stat("/drained")).type, "directory");
  await reopened.close();
  await filesystem.close();
  database.close();
});

test("filesystem close waits for a branch close that is already draining", async () => {
  const { database, filesystem } = await setup();
  const branch = await filesystem.branches.create("close-race");
  let unblock;
  const gate = new Promise((resolve) => {
    unblock = resolve;
  });
  const content = new ReadableStream({
    async pull(controller) {
      await gate;
      controller.enqueue(new TextEncoder().encode("done"));
      controller.close();
    },
  });
  const mutation = branch.writeFile("/close-race", content, { maxBytes: 4 });
  await new Promise((resolve) => setTimeout(resolve, 10));
  const branchClosing = branch.close();
  let filesystemClosed = false;
  const filesystemClosing = filesystem.close().then(() => {
    filesystemClosed = true;
  });
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.equal(filesystemClosed, false);
  unblock();
  await mutation;
  await branchClosing;
  await filesystemClosing;
  assert.equal(filesystemClosed, true);
  database.close();
});

test("filesystem close drains a management call that was already scheduled", async () => {
  const { database, filesystem } = await setup();
  const creating = filesystem.branches.create("close-management-race");
  const closing = filesystem.close();
  await assert.rejects(creating, (error) => error?.code === "EBADF");
  await closing;
  assert.equal(
    database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT count(*) count FROM efs_branches WHERE id='close-management-race'",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0].count,
    ),
    0,
  );
  database.close();
});

test("branch-created directories rename their descendants atomically", async () => {
  const { database, filesystem } = await setup();
  const branch = await filesystem.branches.create("private-directory-rename");
  await branch.mkdir("/private");
  await branch.mkdir("/private/nested");
  await branch.writeFile("/private/nested/file", "private");
  await branch.rename("/private", "/moved");
  assert.equal(
    await branch.readFile("/moved/nested/file", { encoding: "utf8" }),
    "private",
  );
  await assert.rejects(
    branch.readFile("/private/nested/file"),
    (error) => error?.code === "ENOENT",
  );
  const result = await branch.publish({ operationId: "private-directory-rename-op" });
  assert.equal(result.outcome, "merged");
  assert.equal(
    await filesystem.readFile("/moved/nested/file", { encoding: "utf8" }),
    "private",
  );
  await branch.close();
  await filesystem.close();
  database.close();
});

test("branch-created hard links share identity, bytes, and link counts", async () => {
  const { database, filesystem } = await setup();
  const branch = await filesystem.branches.create("private-hard-link");
  await branch.writeFile("/source", "base");
  await branch.link("/source", "/alias");
  assert.equal((await branch.stat("/source")).id, (await branch.stat("/alias")).id);
  assert.equal((await branch.stat("/source")).nlink, 2);
  await branch.writeRange("/alias", 0, new TextEncoder().encode("X"));
  assert.equal(await branch.readFile("/source", { encoding: "utf8" }), "Xase");
  const result = await branch.publish({ operationId: "private-hard-link-op" });
  assert.equal(result.outcome, "merged");
  assert.equal(
    (await filesystem.stat("/source")).id,
    (await filesystem.stat("/alias")).id,
  );
  assert.equal((await filesystem.stat("/source")).nlink, 2);
  assert.equal(await filesystem.readFile("/alias", { encoding: "utf8" }), "Xase");
  await branch.close();
  await filesystem.close();
  database.close();
});

test("unlinking a branch-created hard-link alias decrements its inode links", async () => {
  const { database, filesystem } = await setup();
  const branch = await filesystem.branches.create("private-hard-link-unlink");
  await branch.writeFile("/source", "base");
  await branch.link("/source", "/alias");
  await branch.unlink("/alias");
  assert.equal((await branch.stat("/source")).nlink, 1);
  const result = await branch.publish({ operationId: "private-hard-link-unlink-op" });
  assert.equal(result.outcome, "merged");
  assert.equal((await filesystem.stat("/source")).nlink, 1);
  await assert.rejects(filesystem.stat("/alias"), (error) => error?.code === "ENOENT");
  await branch.close();
  await filesystem.close();
  database.close();
});

test("branch streams enforce global stream and resident-memory admission", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  const filesystem = await EphemeralFS.open({
    database,
    runtime: { maxConcurrentStreams: 1 },
  });
  await filesystem.writeFile("/stream-limit", "stream");
  const branch = await filesystem.branches.create("stream-limit");
  const first = await branch.readStream("/stream-limit");
  await assert.rejects(
    filesystem.readStream("/stream-limit"),
    (error) => error?.code === "EAGAIN",
  );
  let text = "";
  for await (const chunk of first) text += new TextDecoder().decode(chunk);
  assert.equal(text, "stream");
  const main = await filesystem.readStream("/stream-limit");
  await assert.rejects(
    branch.readStream("/stream-limit"),
    (error) => error?.code === "EAGAIN",
  );
  for await (const chunk of main) assert.ok(chunk.byteLength >= 0);
  const second = await branch.readStream("/stream-limit");
  for await (const chunk of second) assert.ok(chunk.byteLength >= 0);
  await branch.close();
  await filesystem.close();
  database.close();
});

test("branch management calls enforce global operation admission", async () => {
  const { database, filesystem } = await setup(":memory:", {
    runtime: { maxConcurrentOperations: 1 },
  });
  const branch = await filesystem.branches.create("management-limit");
  let releaseInput;
  let inputPulled;
  const pulled = new Promise((resolve) => {
    inputPulled = resolve;
  });
  const inputGate = new Promise((resolve) => {
    releaseInput = resolve;
  });
  const input = new ReadableStream({
    async pull(controller) {
      inputPulled();
      await inputGate;
      controller.enqueue(new Uint8Array([7]));
      controller.close();
    },
  });
  const mutation = branch.writeFile("/management-limit", input, { maxBytes: 1 });
  await pulled;
  await assert.rejects(
    filesystem.branches.get("management-limit"),
    (error) => error?.code === "EAGAIN",
  );
  releaseInput();
  await mutation;
  await branch.close();
  await filesystem.close();
  database.close();
});

test("branch streams open with 255 leased COW pages under bounded query budgets", async () => {
  const { database, filesystem } = await setup();
  const pageBytes = filesystem.capabilities.format.cowPageBytes;
  const pageCount = 255;
  await filesystem.writeFile("/many-pages", new Uint8Array(pageBytes * pageCount));
  const branch = await filesystem.branches.create("many-pages");
  await branch.writeRange(
    "/many-pages",
    0,
    new Uint8Array(pageBytes * pageCount).fill(7),
  );
  const stream = await branch.readStream("/many-pages");
  const bytes = new Uint8Array(await new Response(stream).arrayBuffer());
  assert.equal(bytes.byteLength, pageBytes * pageCount);
  assert.equal(bytes[0], 7);
  assert.equal(bytes.at(-1), 7);
  const leased = database.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT count(*) count FROM efs_lease_cow_pages WHERE branch_id=?",
        ["many-pages"],
        { maxRows: 1, maxBytes: 128 },
      )[0].count,
  );
  assert.equal(leased, pageCount);
  await branch.close();
  await filesystem.close();
  database.close();
});

test("over-budget branch streams use a generation-pinned snapshot", async () => {
  const { database, filesystem } = await setup(":memory:", {
    storage: { maxFinalTransactionRows: 64, maxQueryBatchSize: 16 },
  });
  const pageBytes = filesystem.capabilities.format.cowPageBytes;
  await filesystem.writeFile("/stream-budget", new Uint8Array(pageBytes * 16));
  const branch = await filesystem.branches.create("stream-budget");
  for (let index = 0; index < 16; index += 1)
    await branch.writeRange(
      "/stream-budget",
      index * pageBytes,
      new Uint8Array(pageBytes).fill(index & 0xff),
    );
  const stream = await branch.readStream("/stream-budget");
  assert.equal(
    database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT count(*) count FROM efs_lease_cow_pages WHERE branch_id=?",
          ["stream-budget"],
          { maxRows: 1, maxBytes: 128 },
        )[0].count,
    ),
    0,
  );
  const bytes = new Uint8Array(await new Response(stream).arrayBuffer());
  assert.equal(bytes.byteLength, pageBytes * 16);
  for (let index = 0; index < 16; index += 1)
    assert.equal(bytes[index * pageBytes], index & 0xff);
  assert.equal(
    database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT count(*) count FROM efs_leases WHERE id LIKE 'branch-stream:%'",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0].count,
    ),
    0,
  );
  await branch.close();
  await filesystem.close();
  database.close();
});

test("branch and operation identifiers accept 200 bytes and reject empty or 201 bytes", async () => {
  const { database, filesystem } = await setup();
  await assert.rejects(
    filesystem.branches.create(""),
    (error) => error instanceof BranchError && error.code === "InvalidBranchId",
  );
  await assert.rejects(
    filesystem.branches.create("b".repeat(201)),
    (error) => error instanceof BranchError && error.code === "InvalidBranchId",
  );
  const branch = await filesystem.branches.create("b".repeat(200));
  await branch.writeFile("/id", "value");
  await assert.rejects(
    branch.publish({ operationId: "" }),
    (error) => error instanceof BranchError && error.code === "InvalidOperationId",
  );
  await assert.rejects(
    branch.publish({ operationId: "o".repeat(201) }),
    (error) => error instanceof BranchError && error.code === "InvalidOperationId",
  );
  const result = await branch.publish({ operationId: "o".repeat(200) });
  assert.equal(result.outcome, "merged");
  await branch.close();
  await filesystem.close();
  database.close();
});

test("sibling publication uses the branch mutation clock for parent timestamps", async () => {
  for (const order of ["left-first", "right-first"]) {
    let clock = 1;
    const database = await openNodeSqlite({ filename: ":memory:" });
    const filesystem = await EphemeralFS.open({ database, clock: () => clock });
    await filesystem.mkdir("/dir");
    const left = await filesystem.branches.create(`clock-left-${order}`);
    const right = await filesystem.branches.create(`clock-right-${order}`);
    clock = 10;
    await left.writeFile("/dir/left", "left");
    clock = 20;
    await right.writeFile("/dir/right", "right");
    clock = 1000;
    if (order === "left-first") {
      const leftResult = await left.publish();
      const rightResult = await right.publish();
      assert.equal(leftResult.outcome, "merged");
      assert.equal(rightResult.outcome, "merged");
      assert.equal((await left.info()).mergedRevision, leftResult.revision);
      assert.equal((await right.info()).mergedRevision, rightResult.revision);
    } else {
      const rightResult = await right.publish();
      const leftResult = await left.publish();
      assert.equal(rightResult.outcome, "merged");
      assert.equal(leftResult.outcome, "merged");
      assert.equal((await right.info()).mergedRevision, rightResult.revision);
      assert.equal((await left.info()).mergedRevision, leftResult.revision);
    }
    assert.equal((await filesystem.stat("/dir")).mtimeMs, 20);
    await left.close();
    await right.close();
    await filesystem.close();
    database.close();
  }
});

test("range overlays publish their inode write set and preserve metadata", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/page", new Uint8Array(20_000).fill(1));
  const branch = await filesystem.branches.create("range-publication");
  const base = await branch.stat("/page");
  await branch.writeRange("/page", 4_097, new Uint8Array([7, 8, 9]));
  assert.deepEqual(
    [...(await branch.readRange("/page", { offset: 4_097, length: 3 }))],
    [7, 8, 9],
  );
  assert.equal((await branch.stat("/page")).id, base.id);
  assert.equal(
    await filesystem.readRange("/page", { offset: 4_097, length: 3 }).then((v) => v[0]),
    1,
  );
  const result = await branch.publish({ operationId: "range-publication-op" });
  assert.deepEqual(result.changedPaths, ["/page"]);
  assert.deepEqual(
    [...(await filesystem.readRange("/page", { offset: 4_097, length: 3 }))],
    [7, 8, 9],
  );
  await branch.close();
  await filesystem.close();
  database.close();
});

test("full writes after structural patches reset replay state without deleting patches", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/patched", "abcdef");
  const branch = await filesystem.branches.create("patch-then-full");
  await branch.replaceRange("/patched", 2, 2, new TextEncoder().encode("XYZ"));
  assert.equal(await branch.readFile("/patched", { encoding: "utf8" }), "abXYZef");
  await branch.writeFile("/patched", "replacement");
  assert.equal(await branch.readFile("/patched", { encoding: "utf8" }), "replacement");
  const replacementStream = await branch.readStream("/patched");
  assert.equal(await new Response(replacementStream).text(), "replacement");
  const result = await branch.publish({ operationId: "patch-then-full-op" });
  assert.equal(result.outcome, "merged");
  assert.equal(
    await filesystem.readFile("/patched", { encoding: "utf8" }),
    "replacement",
  );
  await branch.close();
  await filesystem.close();
  database.close();
});

test("active-branch GC reclaims structural patches made stale by materialization", async () => {
  const { database, filesystem } = await setup(":memory:", {
    storage: { maxFinalTransactionRows: 64, maxQueryBatchSize: 16 },
  });
  await filesystem.writeFile("/stale-patch", "abcdef");
  const branch = await filesystem.branches.create("stale-patch");
  await branch.replaceRange("/stale-patch", 2, 2, new TextEncoder().encode("XYZ"));
  await branch.writeFile("/stale-patch", "replacement");
  await branch.replaceRange("/stale-patch", 0, 0, new TextEncoder().encode("!"));
  await filesystem.maintenance.collectGarbage({
    runId: "stale-patch-gc",
    maxBatches: 100,
  });
  assert.equal(
    database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT count(*) count FROM efs_patches WHERE branch_id='stale-patch'",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0].count,
    ),
    2,
  );
  assert.equal(
    await branch.readFile("/stale-patch", { encoding: "utf8" }),
    "!replacement",
  );
  await branch.writeFile("/stale-patch", "final");
  await filesystem.maintenance.collectGarbage({
    runId: "stale-patch-gc-final",
    maxBatches: 100,
  });
  assert.equal(
    database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT count(*) count FROM efs_patches WHERE branch_id='stale-patch'",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0].count,
    ),
    0,
  );
  assert.equal(await branch.readFile("/stale-patch", { encoding: "utf8" }), "final");
  await branch.close();
  await filesystem.close();
  database.close();
});

test("branch streams retain the selected structural patches after later patches", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/patched-stream", "abcdef");
  const branch = await filesystem.branches.create("patched-stream");
  await branch.replaceRange("/patched-stream", 2, 1, new TextEncoder().encode("XY"));
  const stream = await branch.readStream("/patched-stream");
  await branch.replaceRange("/patched-stream", 0, 0, new TextEncoder().encode("!"));
  assert.equal(await new Response(stream).text(), "abXYdef");
  await branch.close();
  await filesystem.close();
  database.close();
});

test("structural patch growth falls back before exceeding materialization bounds", async () => {
  const { database, filesystem } = await setup();
  await filesystem.close();
  database.close();
  const reopenedDatabase = await openNodeSqlite({ filename: ":memory:" });
  const bounded = await EphemeralFS.open({
    database: reopenedDatabase,
    filesystem: { maxMaterializedBytes: 1024 },
  });
  await bounded.writeFile("/bounded-patch", new Uint8Array(1024));
  const branch = await bounded.branches.create("bounded-patch");
  await branch.replaceRange("/bounded-patch", 1024, 0, new Uint8Array([1, 2]));
  assert.equal((await branch.stat("/bounded-patch")).size, 1026);
  const stream = await branch.readStream("/bounded-patch");
  const streamed = new Uint8Array(await new Response(stream).arrayBuffer());
  assert.equal(streamed.byteLength, 1026);
  assert.equal(streamed[1023], 0);
  assert.deepEqual(streamed.slice(1024), Uint8Array.of(1, 2));
  const result = await branch.publish({ operationId: "bounded-patch-op" });
  assert.equal(result.outcome, "merged");
  assert.equal((await bounded.stat("/bounded-patch")).size, 1026);
  await branch.close();
  await bounded.close();
  reopenedDatabase.close();
});

test("zero-length structural-patch streams do not pin unrelated overlay rows", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/empty-patch-stream", "abcdef");
  const branch = await filesystem.branches.create("empty-patch-stream");
  await branch.replaceRange("/empty-patch-stream", 1, 0, new Uint8Array([1]));
  await branch.replaceRange("/empty-patch-stream", 2, 0, new Uint8Array([2]));
  const stream = await branch.readStream("/empty-patch-stream", {
    offset: 0,
    length: 0,
  });
  assert.equal(
    database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT count(*) count FROM efs_lease_patches WHERE branch_id=?",
          ["empty-patch-stream"],
          { maxRows: 1, maxBytes: 128 },
        )[0].count,
    ),
    0,
  );
  assert.equal((await new Response(stream).arrayBuffer()).byteLength, 0);
  await branch.close();
  await filesystem.close();
  database.close();
});

test("concurrent replacement fallbacks never publish stale composed bytes", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/replace-race", "abcdef");
  const branch = await filesystem.branches.create("replace-race");
  await branch.writeRange("/replace-race", 0, new TextEncoder().encode("Z"));
  const outcomes = await Promise.allSettled([
    branch.replaceRange("/replace-race", 1, 1, new TextEncoder().encode("XX")),
    branch.replaceRange("/replace-race", 1, 1, new TextEncoder().encode("YY")),
  ]);
  const failures = outcomes.filter((outcome) => outcome.status === "rejected");
  assert.ok(failures.length <= 1);
  for (const failure of failures) assert.equal(failure.reason?.code, "BranchChanged");
  const value = await branch.readFile("/replace-race", { encoding: "utf8" });
  if (failures.length === 0) assert.ok(value === "ZYYXcdef" || value === "ZXXYcdef");
  else assert.ok(value === "ZXXcdef" || value === "ZYYcdef");
  await branch.close();
  await filesystem.close();
  database.close();
});

test("branch writeFile follows a final symbolic link", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/target", "before");
  const branch = await filesystem.branches.create("symlink-write");
  await branch.symlink("/target", "/link");
  await branch.writeFile("/link", "after");
  assert.equal(await branch.readFile("/link", { encoding: "utf8" }), "after");
  assert.equal((await branch.lstat("/link")).type, "symlink");
  const result = await branch.publish({ operationId: "symlink-write-op" });
  assert.equal(result.outcome, "merged");
  assert.equal(await filesystem.readFile("/target", { encoding: "utf8" }), "after");
  await branch.close();
  await filesystem.close();
  database.close();
});

test("empty publication is durable and same-operation concurrent calls converge", async () => {
  const { database, filesystem } = await setup();
  const empty = await filesystem.branches.create("empty-publication");
  const emptyResult = await empty.publish({ operationId: "empty-publication-op" });
  assert.deepEqual(emptyResult.changedPaths, []);
  assert.equal(emptyResult.outcome, "merged");
  await empty.close();

  await filesystem.writeFile("/same-operation", "base");
  const first = await filesystem.branches.create("same-operation");
  const second = await filesystem.branches.open("same-operation");
  await first.writeFile("/same-operation", "branch");
  const results = await Promise.all([
    first.publish({ operationId: "same-operation-op" }),
    second.publish({ operationId: "same-operation-op" }),
  ]);
  assert.deepEqual(results[1], results[0]);
  assert.equal(
    database.transaction(
      "read",
      (tx) =>
        tx.all("SELECT count(*) count FROM efs_revisions", [], {
          maxRows: 1,
          maxBytes: 128,
        })[0].count,
    ),
    4,
  );
  await first.close();
  await second.close();
  await filesystem.close();
  database.close();
});

test("rename reports deterministic source and destination conflicts", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/source", "base");
  const branch = await filesystem.branches.create("rename-conflicts");
  await branch.rename("/source", "/destination");
  await filesystem.writeFile("/source", "main-source");
  await filesystem.writeFile("/destination", "main-destination");
  const result = await branch.publish({ operationId: "rename-conflicts-op" });
  assert.deepEqual(result.conflicts, [
    {
      path: "/destination",
      reason: "destination-changed",
      expectedRevision: null,
      actualRevision: "3",
    },
    {
      path: "/source",
      reason: "source-changed",
      expectedRevision: "1",
      actualRevision: "2",
    },
  ]);
  assert.equal(
    await filesystem.readFile("/source", { encoding: "utf8" }),
    "main-source",
  );
  assert.equal(await branch.readFile("/destination", { encoding: "utf8" }), "base");
  await branch.close();
  await filesystem.close();
  database.close();
});

test("range no-ops do not advance branch generation", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/no-op", "value");
  const branch = await filesystem.branches.create("range-no-op");
  const generation = (await branch.info()).generation;
  await branch.writeRange("/no-op", 0, new Uint8Array());
  await branch.replaceRange("/no-op", 2, 0, new Uint8Array());
  await assert.rejects(
    branch.replaceRange("/no-op", 99, 0, new Uint8Array()),
    (error) => error?.code === "EINVAL",
  );
  await branch.truncate("/no-op", 5);
  assert.equal((await branch.info()).generation, generation);
  await branch.close();
  await filesystem.close();
  database.close();
});

test("no-op chmod does not advance branch generation", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/chmod-no-op", "value", { mode: 0o640 });
  const branch = await filesystem.branches.create("chmod-no-op");
  const before = await branch.info();
  await branch.chmod("/chmod-no-op", 0o640);
  const after = await branch.info();
  assert.equal(after.generation, before.generation);
  assert.equal((await branch.stat("/chmod-no-op")).mode, 0o640);
  await branch.close();
  await filesystem.close();
  database.close();
});

test("branch handle exhaustion uses filesystem EAGAIN", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  const filesystem = await EphemeralFS.open({
    database,
    runtime: { maxOpenBranchHandles: 1 },
  });
  const first = await filesystem.branches.create("handle-limit");
  await assert.rejects(
    filesystem.branches.open("handle-limit"),
    (error) => error?.code === "EAGAIN",
  );
  await assert.rejects(
    filesystem.branches.create("handle-limit-second"),
    (error) => error?.code === "EAGAIN",
  );
  await assert.rejects(
    filesystem.branches.get("handle-limit-second"),
    (error) => error?.code === "BranchNotFound",
  );
  await first.close();
  await filesystem.close();
  database.close();
});

test("branch limits reject an impossible conflict envelope at open", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  await assert.rejects(
    EphemeralFS.open({
      database,
      branch: {
        maxChangedPathsPerBranch: 4,
        maxConflictsPerPublication: 1,
      },
    }),
    (error) => error instanceof RangeError,
  );
  database.close();
});

test("leased COW predecessors remain until the stream releases them", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/leased-page", new Uint8Array(20_000));
  const branch = await filesystem.branches.create("leased-predecessor");
  await branch.writeRange("/leased-page", 0, new Uint8Array([1]));
  const stream = await branch.readStream("/leased-page");
  await branch.writeRange("/leased-page", 0, new Uint8Array([2]));
  const retained = database.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT count(*) count FROM efs_cow_page_versions WHERE branch_id='leased-predecessor'",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0].count,
  );
  assert.equal(retained, 2);
  let bytes = 0;
  for await (const chunk of stream) bytes += chunk.length;
  assert.equal(bytes, 20_000);
  await branch.discard();
  await branch.close();
  await filesystem.close();
  database.close();
});

test("released COW leases are reclaimed without deleting current branch pages", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/cleanup-page", new Uint8Array(20_000));
  const branch = await filesystem.branches.create("cleanup-page");
  await branch.writeRange("/cleanup-page", 0, new Uint8Array([1]));
  const stream = await branch.readStream("/cleanup-page");
  await branch.writeRange("/cleanup-page", 0, new Uint8Array([2]));
  let bytes = 0;
  for await (const chunk of stream) bytes += chunk.length;
  assert.equal(bytes, 20_000);
  const beforeDiscard = database.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT count(*) count FROM efs_cow_page_versions WHERE branch_id='cleanup-page'",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0].count,
  );
  assert.equal(beforeDiscard, 2);
  await branch.discard();
  await filesystem.maintenance.collectGarbage({ runId: "cleanup-page-gc" });
  const after = database.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT (SELECT count(*) FROM efs_cow_page_versions WHERE branch_id='cleanup-page') pages,(SELECT count(*) FROM efs_lease_cow_pages) leases",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0],
  );
  assert.deepEqual(after, { pages: 0, leases: 0 });
  await branch.close();
  await filesystem.close();
  database.close();
});

test("large COW materialization and discard stay bounded under a tight row profile", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  let now = 0;
  const day = 24 * 60 * 60 * 1000;
  const filesystem = await EphemeralFS.open({
    database,
    clock: () => now,
    storage: { maxFinalTransactionRows: 64, maxQueryBatchSize: 16 },
    branch: { terminalBranchRetentionMs: 7 * day },
  });
  const pageBytes = filesystem.capabilities.format.cowPageBytes;
  const branchBytes = new Uint8Array(pageBytes * 50);
  await filesystem.writeFile("/bounded-cow", branchBytes);
  const branch = await filesystem.branches.create("bounded-cow");
  for (let index = 0; index < 50; index += 1)
    await branch.writeRange(
      "/bounded-cow",
      index * pageBytes,
      new Uint8Array(pageBytes).fill(index & 0xff),
    );
  await branch.writeFile("/bounded-cow", new Uint8Array(branchBytes.length).fill(7));
  await branch.writeRange("/bounded-cow", 0, new Uint8Array([8]));
  const snapshot = await branch.readStream("/bounded-cow");
  assert.equal(new Uint8Array(await new Response(snapshot).arrayBuffer())[0], 8);
  await branch.discard();
  await branch.close();
  await filesystem.maintenance.collectGarbage({
    runId: "bounded-cow-gc",
    maxBatches: 1_000,
  });
  const remaining = database.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT count(*) count FROM efs_cow_page_versions WHERE branch_id='bounded-cow'",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0].count,
  );
  assert.equal(remaining, 0);
  for (let index = 0; index < 40; index += 1) {
    const terminal = await filesystem.branches.create(`bounded-terminal-${index}`);
    await terminal.discard();
    await terminal.close();
  }
  now = 7 * day + 1;
  await filesystem.maintenance.collectGarbage({
    runId: "bounded-terminal-gc",
    maxBatches: 1_000,
  });
  const terminalCount = database.transaction(
    "read",
    (tx) =>
      tx.all("SELECT count(*) count FROM efs_branches WHERE state<>0", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].count,
  );
  assert.equal(terminalCount, 0);
  await filesystem.close();
  database.close();
});

test("terminal branch retention waits for a live branch stream lease", async () => {
  let now = 0;
  const { database, filesystem } = await setup(":memory:", {
    clock: () => now,
    storage: { readLeaseMs: 8 * 24 * 60 * 60 * 1000 },
    branch: { terminalBranchRetentionMs: 7 * 24 * 60 * 60 * 1000 },
  });
  await filesystem.writeFile("/terminal-stream", "bytes");
  const branch = await filesystem.branches.create("terminal-stream");
  const stream = await branch.readStream("/terminal-stream");
  await branch.discard();
  assert.equal(
    database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT branch_id FROM efs_leases WHERE owner_id LIKE 'branch-stream:%'",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0].branch_id,
    ),
    "terminal-stream",
  );
  now = 7 * 24 * 60 * 60 * 1000 + 1;
  await filesystem.maintenance.collectGarbage({
    runId: "terminal-stream-held",
    maxBatches: 100,
  });
  assert.equal((await filesystem.branches.get("terminal-stream")).state, "discarded");
  await stream.cancel();
  await branch.close();
  await filesystem.maintenance.collectGarbage({
    runId: "terminal-stream-released",
    maxBatches: 100,
  });
  await assert.rejects(
    filesystem.branches.get("terminal-stream"),
    (error) => error instanceof BranchError && error.code === "BranchNotFound",
  );
  await filesystem.close();
  database.close();
});

test("directory rename reports every moved descendant in UTF-8 order", async () => {
  const { database, filesystem } = await setup();
  await filesystem.mkdir("/old");
  await filesystem.mkdir("/old/nested");
  await filesystem.writeFile("/old/nested/file", "value");
  const branch = await filesystem.branches.create("directory-rename");
  await branch.rename("/old", "/new");
  const result = await branch.publish({ operationId: "directory-rename-op" });
  assert.equal(result.outcome, "merged");
  assert.deepEqual(result.changedPaths, [
    "/new",
    "/new/nested",
    "/new/nested/file",
    "/old",
    "/old/nested",
    "/old/nested/file",
  ]);
  assert.equal(
    await filesystem.readFile("/new/nested/file", { encoding: "utf8" }),
    "value",
  );
  await branch.close();
  await filesystem.close();
  database.close();
});

test("branch streams survive publication and collection with exact bytes", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/stream-publish", "base");
  const branch = await filesystem.branches.create("stream-publish");
  await branch.writeFile("/stream-publish", "published-snapshot");
  const stream = await branch.readStream("/stream-publish");
  const result = await branch.publish({ operationId: "stream-publish-op" });
  assert.equal(result.outcome, "merged");
  assert.equal((await branch.info()).mergedRevision, result.revision);
  assert.deepEqual(
    database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT (SELECT count(*) FROM efs_branch_changes WHERE branch_id=?) changes,(SELECT count(*) FROM efs_branch_inode_expectations WHERE branch_id=?) expectations,(SELECT count(*) FROM efs_branch_inode_overlays WHERE branch_id=?) overlays,(SELECT count(*) FROM efs_branch_manifest_roots WHERE branch_id=?) roots",
          ["stream-publish", "stream-publish", "stream-publish", "stream-publish"],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    ),
    { changes: 0, expectations: 0, overlays: 0, roots: 0 },
  );
  await filesystem.maintenance.collectGarbage({ runId: "stream-publish-gc" });
  let text = "";
  for await (const chunk of stream) text += new TextDecoder().decode(chunk);
  assert.equal(text, "published-snapshot");
  await branch.close();
  await filesystem.close();
  database.close();
});

test("expired publication results are pruned to lifetime operation tombstones", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  let now = 1;
  const filesystem = await EphemeralFS.open({ database, clock: () => now });
  const branch = await filesystem.branches.create("expire-result");
  await branch.writeFile("/value", "result");
  const result = await branch.publish({ operationId: "expire-result-op" });
  now = 30 * 24 * 60 * 60 * 1000 + 2;
  await filesystem.maintenance.collectGarbage({ runId: "expire-results" });
  await assert.rejects(
    filesystem.branches.replay("expire-result-op", "expire-result"),
    (error) => error instanceof BranchError && error.code === "OperationResultExpired",
  );
  await assert.rejects(
    branch.publish({ operationId: "expire-result-op" }),
    (error) => error instanceof BranchError && error.code === "OperationResultExpired",
  );
  const counts = database.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT (SELECT count(*) FROM efs_operation_results WHERE length(encoded)>0) results,(SELECT count(*) FROM efs_operation_ids) ids",
        [],
        { maxRows: 1, maxBytes: 256 },
      )[0],
  );
  assert.equal(counts.results, 0);
  assert.equal(counts.ids, 1);
  assert.equal(result.outcome, "merged");
  await branch.close();
  await filesystem.close();
  database.close();
});

test("terminal branch metadata follows configured retention while identifiers remain reserved", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  const day = 24 * 60 * 60 * 1000;
  let now = 0;
  const filesystem = await EphemeralFS.open({
    database,
    clock: () => now,
    branch: {
      terminalBranchRetentionMs: 7 * day,
      publicationResultRetentionMs: 9 * day,
    },
  });
  const discarded = await filesystem.branches.create("retained-discard");
  await discarded.discard();
  await discarded.close();
  now = 7 * day;
  await filesystem.maintenance.collectGarbage({ runId: "terminal-retention" });
  await assert.rejects(
    filesystem.branches.open("retained-discard"),
    (error) => error instanceof BranchError && error.code === "BranchNotFound",
  );
  await assert.rejects(
    filesystem.branches.create("retained-discard"),
    (error) => error instanceof Error && /UNIQUE|constraint/i.test(error.message),
  );

  now = 7 * day;
  const merged = await filesystem.branches.create("retained-merged");
  await merged.writeFile("/retained", "value");
  const result = await merged.publish({ operationId: "retained-result" });
  await merged.close();
  now = 15 * day;
  await filesystem.maintenance.collectGarbage({ runId: "terminal-result-retention" });
  assert.deepEqual(
    await filesystem.branches.replay("retained-result", "retained-merged"),
    result,
  );
  await filesystem.close();
  database.close();
});

test("revision retention checkpoints preserve the retained history window", async () => {
  const { database, filesystem } = await setup(":memory:", {
    storage: { maxRetainedRevisions: 2 },
  });
  try {
    for (let index = 0; index < 5; index += 1)
      await filesystem.writeFile("/retained", Uint8Array.of(index));
    const active = await filesystem.branches.create({
      id: "retention-active",
      baseRevision: "4",
    });
    await filesystem.maintenance.collectGarbage({
      runId: "retention-window",
      maxBatches: 1_000,
    });
    assert.deepEqual([...(await filesystem.readFile("/retained"))], [4]);
    assert.deepEqual([...(await active.readFile("/retained"))], [3]);
    await assert.rejects(
      filesystem.branches.create({ id: "retention-old", baseRevision: "1" }),
      (error) => error instanceof BranchError && error.code === "RevisionNotFound",
    );
    const retained = database.transaction("read", (tx) =>
      tx.all(
        "SELECT target_revision,state,phase FROM efs_revision_checkpoints ORDER BY target_revision",
        [],
        { maxRows: 16, maxBytes: 4096 },
      ),
    );
    assert.deepEqual(retained, [{ target_revision: 4, state: 1, phase: 7 }]);
    await active.discard();
  } finally {
    await filesystem.close();
    await database.close();
  }
});

test("publication rejects a write set before opening an over-budget final transaction", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  const filesystem = await EphemeralFS.open({
    database,
    storage: { maxFinalTransactionRows: 64 },
  });
  const branch = await filesystem.branches.create("bounded-final");
  for (let index = 0; index < 5; index += 1)
    await branch.writeFile(`/bounded-${index}`, `value-${index}`);
  await assert.rejects(
    branch.publish({ operationId: "bounded-final-op" }),
    (error) => error instanceof BranchError && error.code === "LimitExceeded",
  );
  assert.equal((await branch.info()).state, "active");
  await assert.rejects(
    filesystem.readFile("/bounded-0"),
    (error) => error?.code === "ENOENT",
  );
  await branch.close();
  await filesystem.close();
  database.close();
});

test("publication preflight includes terminal COW cleanup rows", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  const filesystem = await EphemeralFS.open({
    database,
    storage: { maxFinalTransactionRows: 64 },
  });
  const pageBytes = filesystem.capabilities.format.cowPageBytes;
  await filesystem.writeFile("/cow-cleanup", new Uint8Array(pageBytes * 16));
  const branch = await filesystem.branches.create("cow-cleanup");
  for (let index = 0; index < 16; index += 1)
    await branch.writeRange("/cow-cleanup", index * pageBytes, Uint8Array.of(index));
  await assert.rejects(
    branch.publish({ operationId: "cow-cleanup-op" }),
    (error) => error instanceof BranchError && error.code === "LimitExceeded",
  );
  assert.equal((await branch.info()).state, "active");
  assert.equal(
    await filesystem
      .readRange("/cow-cleanup", { offset: 0, length: 1 })
      .then((value) => value[0]),
    0,
  );
  await branch.close();
  await filesystem.close();
  database.close();
});
