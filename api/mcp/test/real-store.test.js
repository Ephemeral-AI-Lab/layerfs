import assert from "node:assert/strict";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { call, initialize, payload, startServer } from "./helpers.mjs";

test("stdio server initializes a real Disposable Store and performs SDK file calls", { timeout: 110_000 }, async (t) => {
  const server = startServer(t, { env: { LAYERFS_NATIVE: "", CLI_FAKE_LOG: "" } });
  const source = join(server.work, "source");
  mkdirSync(source);
  writeFileSync(join(source, "seed.txt"), "seed\n");
  await initialize(server);
  const initialized = await call(server, "layerfs_init", {
    name: "mcp-e2e",
    path: source,
    timeoutMs: 90_000,
    options: { home: join(server.work, "home"), storePath: join(server.work, "store.sqlite") },
  });
  assert.equal(initialized.isError, undefined, JSON.stringify(initialized));
  assert.equal(payload(initialized).created.kind, "Initialized");
  const forked = payload(await call(server, "layerfs_branch_fork", { name: "work", from: "main" }));
  assert.equal(forked.kind, "Forked");
  const written = payload(await call(server, "layerfs_fs_write", {
    branch: "work", path: "/hello.txt", content: "hello from MCP\n", message: "MCP real Store",
  }));
  assert.equal(written.kind, "Committed");
  assert.equal(written.branch, "work");
  assert.equal(written.commit.via, "fs");
  const read = payload(await call(server, "layerfs_fs_read", { ref: "work", path: "/hello.txt" }));
  assert.equal(read.text, "hello from MCP\n");
  assert.equal(read.size, 15);
  assert.equal(read.startLine, 1);
  assert.equal(read.nextLine, null);
  assert.ok(read.snapshot.startsWith("work@"));
  const results = payload(await call(server, "layerfs_results"));
  assert.ok(results.items.some((record) => record.method === "project.init" && record.ok));
  assert.ok(results.items.some((record) => record.method === "fs.write" && record.commit));
  assert.deepEqual(payload(await call(server, "layerfs_close")), { closed: true });
  await server.stop();
  assert.equal(server.child.exitCode, 0);
  rmSync(server.work, { recursive: true, force: true });
});
