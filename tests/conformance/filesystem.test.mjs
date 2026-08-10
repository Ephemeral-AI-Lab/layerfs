import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { EphemeralFS, FilesystemError } from "../../packages/fs/dist/index.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";

async function memoryFilesystem(options = {}) {
  const database = await openNodeSqlite({ filename: ":memory:" });
  const filesystem = await EphemeralFS.open({ database, ...options });
  return { database, filesystem, async close() { await filesystem.close(); database.close(); } };
}

async function bytes(stream) {
  const parts = []; let length = 0;
  for await (const part of stream) { parts.push(part); length += part.length; }
  const result = new Uint8Array(length); let offset = 0; for (const part of parts) { result.set(part, offset); offset += part.length; } return result;
}

test("public filesystem covers canonical paths, ranges, metadata, and UTF-8 ordering", async () => {
  const fixture = await memoryFilesystem({ clock: (() => { let now = 1000; return () => now++; })() });
  try {
    await fixture.filesystem.mkdir("/work//nested/../nested/", { recursive: true, mode: 0o750 });
    await fixture.filesystem.writeFile("/work/nested/data", "abcdef", { mode: 0o640 });
    assert.equal(await fixture.filesystem.readFile("/work/./nested/data", { encoding: "utf8" }), "abcdef");
    assert.deepEqual([...await fixture.filesystem.readRange("/work/nested/data", { offset: 2, length: 20 })], [...new TextEncoder().encode("cdef")]);
    await fixture.filesystem.writeRange("/work/nested/data", 8, Uint8Array.of(88));
    assert.deepEqual([...await fixture.filesystem.readFile("/work/nested/data")], [97, 98, 99, 100, 101, 102, 0, 0, 88]);
    await fixture.filesystem.replaceRange("/work/nested/data", 1, 3, new TextEncoder().encode("Q"));
    assert.deepEqual([...await fixture.filesystem.readFile("/work/nested/data")], [97, 81, 101, 102, 0, 0, 88]);
    await fixture.filesystem.truncate("/work/nested/data", 3); assert.equal(await fixture.filesystem.readFile("/work/nested/data", { encoding: "utf8" }), "aQe");
    const before = await fixture.filesystem.stat("/work/nested/data"); await fixture.filesystem.chmod("/work/nested/data", 0o600); const after = await fixture.filesystem.stat("/work/nested/data");
    assert.equal(after.mode, 0o600); assert.ok(after.ctimeMs >= before.ctimeMs); assert.equal(after.id, before.id);
    for (const name of ["z", "é", "a", "Ω"]) await fixture.filesystem.writeFile(`/work/nested/${name}`, new Uint8Array());
    const names = (await fixture.filesystem.readdir("/work/nested")).map((entry) => entry.name);
    assert.deepEqual(names, [...names].sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b))));
    assert.deepEqual(await fixture.filesystem.readdir("/work/nested", { limit: 0 }), []);
    await assert.rejects(fixture.filesystem.stat("/../../escape"), (error) => error instanceof FilesystemError && error.code === "EINVAL");
  } finally { await fixture.close(); }
});

test("hard links, symbolic links, rename, unlink, and recursive removal persist", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-public-")); const filename = path.join(directory, "filesystem.db");
  try {
    let database = await openNodeSqlite({ filename }); let filesystem = await EphemeralFS.open({ database });
    await filesystem.mkdir("/tree/sub", { recursive: true }); await filesystem.writeFile("/tree/sub/file", "original");
    await filesystem.link("/tree/sub/file", "/tree/alias"); const first = await filesystem.stat("/tree/sub/file"); const alias = await filesystem.stat("/tree/alias");
    assert.equal(first.id, alias.id); assert.equal(first.nlink, 2); await filesystem.unlink("/tree/sub/file"); assert.equal((await filesystem.stat("/tree/alias")).nlink, 1);
    await filesystem.symlink("../alias", "/tree/sub/link"); assert.equal(await filesystem.readlink("/tree/sub/link"), "../alias"); assert.equal(await filesystem.readFile("/tree/sub/link", { encoding: "utf8" }), "original");
    assert.equal((await filesystem.lstat("/tree/sub/link")).type, "symlink"); await filesystem.rename("/tree/alias", "/tree/renamed"); assert.equal(await filesystem.readFile("/tree/renamed", { encoding: "utf8" }), "original");
    await filesystem.close(); database.close(); database = await openNodeSqlite({ filename }); filesystem = await EphemeralFS.open({ database });
    assert.equal(await filesystem.readFile("/tree/renamed", { encoding: "utf8" }), "original"); await filesystem.rm("/tree", { recursive: true }); await assert.rejects(filesystem.stat("/tree"), (error) => error.code === "ENOENT");
    await filesystem.close(); database.close();
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("leased streams retain the selected snapshot across overwrite and release on completion", async () => {
  const fixture = await memoryFilesystem({ filesystem: { preferredStreamChunkBytes: 1024 } });
  try {
    const original = Uint8Array.from({ length: 20_000 }, (_, index) => index & 0xff); await fixture.filesystem.writeFile("/snapshot", original);
    const stream = await fixture.filesystem.readStream("/snapshot"); await fixture.filesystem.writeFile("/snapshot", "new");
    assert.deepEqual(await bytes(stream), original); assert.equal(await fixture.filesystem.readFile("/snapshot", { encoding: "utf8" }), "new");
    const leaseCount = fixture.database.transaction("read", (tx) => tx.all("SELECT count(*) count FROM efs_leases WHERE state<>2", [], { maxRows: 1, maxBytes: 100 })[0].count); assert.equal(leaseCount, 0);
  } finally { await fixture.close(); }
});

test("memory and transaction ceilings reject without a visible partial mutation", async () => {
  const fixture = await memoryFilesystem({ filesystem: { maxMaterializedBytes: 512 * 1024 }, runtime: { maxManagedResidentBytes: 1024 * 1024, maxCacheBytes: 256 * 1024, maxPendingWriteBytes: 256 * 1024, maxWriteSessionBytes: 64 * 1024, maxPrefetchBytes: 64 * 1024, maxQueryBatchBytes: 64 * 1024, maxPreparedResultBytes: 512 * 1024 }, storage: { maxFinalTransactionRows: 64, maxFinalTransactionBytes: 256 * 1024 } });
  try {
    await assert.rejects(fixture.filesystem.writeFile("/too-large", new Uint8Array(2 * 1024 * 1024)), (error) => error.code === "ENOSPC");
    await assert.rejects(fixture.filesystem.stat("/too-large"), (error) => error.code === "ENOENT");
  } finally { await fixture.close(); }
});

test("close is idempotent and rejects later operations", async () => {
  const fixture = await memoryFilesystem(); await fixture.filesystem.close(); await fixture.filesystem.close();
  await assert.rejects(fixture.filesystem.stat("/"), (error) => error.code === "EBADF"); fixture.database.close();
});
