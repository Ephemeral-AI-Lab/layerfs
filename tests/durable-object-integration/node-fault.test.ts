import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import {
  createStatementFaultController,
  PORTABLE_FAULT_OPERATION_POSITIONS,
  PORTABLE_FAULT_POSITIONS,
  runFilesystemFaultMatrix,
} from "../../packages/testkit/dist/index.js";
import { expect, test } from "vitest";

test(
  "filesystem mutations roll back after every file-backed Node SQL statement",
  { timeout: 300_000 },
  async () => {
    const faults = createStatementFaultController();
    const result = await runFilesystemFaultMatrix({
      name: "node-sqlite-fault-matrix",
      async create() {
        const directory = await mkdtemp(path.join(tmpdir(), "efs-m6-node-fault-"));
        const filename = path.join(directory, "filesystem.db");
        let current = await openNodeSqlite({ filename });
        let disposed = false;
        return {
          adapter: faults.wrap(current),
          capabilities: ["fault-injection", "garbage-collection", "physical-reopen"],
          faults,
          collectGarbage(filesystem, options) {
            return filesystem.maintenance.collectGarbage(options);
          },
          async reopen() {
            current = await openNodeSqlite({ filename, create: false });
            return faults.wrap(current);
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
    expect(result.faultPoint).toBe("after-sql-statement");
    expect(result.positions).toBe(PORTABLE_FAULT_POSITIONS);
    expect(result.payloadBytes).toBe(64 * 1024);
    expect(result.operationPositions).toEqual(PORTABLE_FAULT_OPERATION_POSITIONS);
    console.log(`m6-fault-evidence ${JSON.stringify(result)}`);
  },
);
