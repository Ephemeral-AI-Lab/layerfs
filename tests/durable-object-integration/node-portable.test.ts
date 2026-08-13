import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import {
  PORTABLE_BRANCH_CASE_IDS,
  PORTABLE_CONFORMANCE_CASE_IDS,
  PORTABLE_DRIVER_CASE_IDS,
  PORTABLE_MAINTENANCE_CASE_IDS,
  PORTABLE_RESTART_CASE_IDS,
  PORTABLE_STORAGE_CONFORMANCE_CASE_IDS,
  type PortableFixtureContext,
  PortableStagingCrashSession,
  preparePortableRestart,
  runBranchConformance,
  runFilesystemConformance,
  runMaintenanceConformance,
  runSQLiteDriverConformance,
  runStorageConformance,
  verifyPortableRestart,
} from "../../packages/testkit/dist/index.js";
import { portableStorageInternals } from "./portable-storage-internals.js";
import { expect, test } from "vitest";

function fixtureContextEvidence(adapter: string) {
  return (context: PortableFixtureContext): void => {
    console.log(`m6-suite-context-evidence ${JSON.stringify({ adapter, ...context })}`);
  };
}

test(
  "the shared M2 storage suite passes against file-backed Node",
  { timeout: 120_000 },
  async () => {
    const results = await runStorageConformance(
      {
        name: "node-sqlite-storage",
        recordFixtureContext: fixtureContextEvidence("node-sqlite"),
        async create() {
          const directory = await mkdtemp(path.join(tmpdir(), "efs-m6-node-storage-"));
          const filename = path.join(directory, "filesystem.db");
          const adapter = await openNodeSqlite({ filename });
          return {
            adapter,
            capabilities: [],
            async reopen() {
              return openNodeSqlite({ filename, create: false });
            },
            async dispose() {
              await rm(directory, { recursive: true, force: true });
            },
          };
        },
      },
      portableStorageInternals,
    );
    expect(results).toEqual(
      PORTABLE_STORAGE_CONFORMANCE_CASE_IDS.map((id) => ({ id, status: "passed" })),
    );
  },
);

test("partial staging survives each physical Node restart and expires exactly", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-m6-node-staging-crash-"));
  const filename = path.join(directory, "filesystem.db");
  let adapter = await openNodeSqlite({ filename });
  const session = new PortableStagingCrashSession();
  try {
    for (;;) {
      const outcome = await session.run(adapter, portableStorageInternals);
      if (outcome.status === "complete") {
        expect(outcome.result).toEqual({
          schema: "efs-portable-staging-crash-v1",
          batches: 3,
          physicalRestarts: 3,
          recovered: true,
        });
        break;
      }
      adapter.close();
      adapter = await openNodeSqlite({ filename, create: false });
    }
  } finally {
    try {
      adapter.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("the shared restart suite survives physical Node SQLite destruction", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-m6-node-restart-"));
  const filename = path.join(directory, "filesystem.db");
  let adapter = await openNodeSqlite({ filename });
  try {
    const preparation = await preparePortableRestart(adapter);
    // Abruptly destroy the physical driver without orderly filesystem/branch cleanup.
    adapter.close();
    adapter = await openNodeSqlite({ filename, create: false });
    const result = await verifyPortableRestart(adapter, preparation);
    expect(result.cases).toEqual(PORTABLE_RESTART_CASE_IDS);
    expect(result.fixtureDigest).toBe(preparation.fixtureDigest);
    expect(result.activeLeaseRows).toBe(0);
    expect(result.stagingRows).toBe(0);
    expect(result.collectionState).toBe("complete");
    console.log(`m6-restart-evidence ${JSON.stringify(result)}`);
  } finally {
    try {
      adapter.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("the shared portable suite passes against the file-backed Node adapter", async () => {
  const results = await runFilesystemConformance({
    name: "node-sqlite-file-backed",
    recordFixtureContext: fixtureContextEvidence("node-sqlite"),
    async create() {
      const directory = await mkdtemp(path.join(tmpdir(), "efs-m6-node-portable-"));
      const filename = path.join(directory, "filesystem.db");
      let current = await openNodeSqlite({ filename });
      let disposed = false;
      return {
        adapter: current,
        capabilities: [
          "garbage-collection",
          "physical-reopen",
          "read-only-reopen",
          "second-connection",
        ],
        collectGarbage(filesystem, options) {
          return filesystem.maintenance.collectGarbage(options);
        },
        async reopen(options = {}) {
          current = await openNodeSqlite({
            filename,
            create: false,
            ...(options.readOnly === undefined ? {} : { readOnly: options.readOnly }),
          });
          return current;
        },
        async openSecondConnection() {
          return openNodeSqlite({ filename, create: false });
        },
        async dispose() {
          if (disposed) return;
          disposed = true;
          try {
            current.close();
          } catch {}
          await rm(directory, { recursive: true, force: true });
        },
      };
    },
  });
  expect(results).toEqual(
    PORTABLE_CONFORMANCE_CASE_IDS.map((id) => ({ id, status: "passed" })),
  );
});

test("the shared SQLite driver contract passes against file-backed Node", async () => {
  const results = await runSQLiteDriverConformance({
    name: "node-sqlite-driver",
    recordFixtureContext: fixtureContextEvidence("node-sqlite"),
    async create() {
      const directory = await mkdtemp(path.join(tmpdir(), "efs-m6-node-driver-"));
      const filename = path.join(directory, "driver.db");
      let current = await openNodeSqlite({ filename });
      let disposed = false;
      return {
        adapter: current,
        capabilities: ["physical-reopen"],
        async reopen() {
          current = await openNodeSqlite({ filename, create: false });
          return current;
        },
        async dispose() {
          if (disposed) return;
          disposed = true;
          try {
            current.close();
          } catch {}
          await rm(directory, { recursive: true, force: true });
        },
      };
    },
  });
  expect(results).toEqual(
    PORTABLE_DRIVER_CASE_IDS.map((id) => ({ id, status: "passed" })),
  );
});

test("the shared branch suite passes against file-backed Node", async () => {
  const results = await runBranchConformance({
    name: "node-sqlite-branches",
    recordFixtureContext: fixtureContextEvidence("node-sqlite"),
    async create() {
      const directory = await mkdtemp(path.join(tmpdir(), "efs-m6-node-branches-"));
      const filename = path.join(directory, "filesystem.db");
      let current = await openNodeSqlite({ filename });
      let disposed = false;
      return {
        adapter: current,
        capabilities: ["physical-reopen"],
        async reopen() {
          current = await openNodeSqlite({ filename, create: false });
          return current;
        },
        async dispose() {
          if (disposed) return;
          disposed = true;
          try {
            current.close();
          } catch {}
          await rm(directory, { recursive: true, force: true });
        },
      };
    },
  });
  expect(results).toEqual(
    PORTABLE_BRANCH_CASE_IDS.map((id) => ({ id, status: "passed" })),
  );
});

test("the shared maintenance suite passes against file-backed Node", async () => {
  const results = await runMaintenanceConformance({
    name: "node-sqlite-maintenance",
    recordFixtureContext: fixtureContextEvidence("node-sqlite"),
    async create({ label = "fixture" } = {}) {
      const directory = await mkdtemp(path.join(tmpdir(), `efs-m6-node-${label}-`));
      const filename = path.join(directory, "filesystem.db");
      let current = await openNodeSqlite({ filename });
      let disposed = false;
      return {
        adapter: current,
        capabilities: ["garbage-collection", "physical-reopen"],
        collectGarbage(filesystem, options) {
          return filesystem.maintenance.collectGarbage(options);
        },
        async reopen() {
          current = await openNodeSqlite({ filename, create: false });
          return current;
        },
        async dispose() {
          if (disposed) return;
          disposed = true;
          try {
            current.close();
          } catch {}
          await rm(directory, { recursive: true, force: true });
        },
      };
    },
  });
  expect(results).toEqual(
    PORTABLE_MAINTENANCE_CASE_IDS.map((id) => ({ id, status: "passed" })),
  );
});
