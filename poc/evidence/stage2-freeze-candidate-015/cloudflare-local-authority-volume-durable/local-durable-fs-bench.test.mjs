import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { test } from "node:test";

import { Database, initializeSchema, SQLiteWorkspaceProvider } from "@cloudflare/dofs";

import { FileSQLiteStorage } from "./local-durable-fs-bench.mjs";

const SOURCE = readFileSync(new URL("./local-durable-fs-bench.mjs", import.meta.url), "utf8");

test("durable contract cannot lose a required stage", () => {
  const stages = {
    barrier: /wal_checkpoint\(TRUNCATE\)[\s\S]*fsyncSync\(fd\)/,
    sameContainerRestart: /docker\("start", containerName\)/,
    freshNodeProcess:
      /docker\("start", containerName\)[\s\S]*runBoundHelper\([\s\S]*"--verify-db"[\s\S]*receipt\.restore/,
    namedVolumeAuthority:
      /docker\(\.\.\.volumeCreateArgv\)[\s\S]*docker\("volume", "inspect", volumeName\)[\s\S]*type=volume,src=\$\{volumeName\},dst=\/durable-state/,
    workspaceIsOnlyFuse: /workspaceMount !== undefined/,
    reconcileBeforePush:
      /await reconcileWatermarks\(db, client\.sync\)[\s\S]*await pushOnce\(db, client\.sync\)/,
    exactVerification: /verifyFuse\(containerName, expectedHash\)/,
    authoritativeCleanup:
      /"--pull-cleanup"[\s\S]*docker\("rm", "-f", containerName\)[\s\S]*docker\(\.\.\.volumeRemoveArgv\)[\s\S]*volumeAbsent/,
  };
  for (const [stage, pattern] of Object.entries(stages)) {
    assert.match(SOURCE, pattern, `${stage} is required`);
  }
  assert.doesNotMatch(SOURCE, /UPSTREAM_URL/);
});

test("file SQLite barrier survives a fresh DatabaseSync", () => {
  const dir = mkdtempSync(resolve(tmpdir(), "cloudflare-file-sqlite-test-"));
  const path = resolve(dir, "store.sqlite");
  try {
    const writer = new FileSQLiteStorage(path);
    const db = new Database(writer);
    initializeSchema(db, () => 1);
    const provider = new SQLiteWorkspaceProvider(db, { now: () => 2 });
    provider.mkdirSync("/workspace");
    provider.writeFileSync("/workspace/proof", Buffer.from("durable"));
    const checkpoint = writer.durableBarrier();
    writer.close();
    assert.equal(checkpoint.busy, 0);

    const reader = new FileSQLiteStorage(path, { readOnly: true });
    try {
      const reopened = new SQLiteWorkspaceProvider(new Database(reader));
      assert.equal(reopened.readFileSync("/workspace/proof", "utf8"), "durable");
    } finally {
      reader.close();
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
