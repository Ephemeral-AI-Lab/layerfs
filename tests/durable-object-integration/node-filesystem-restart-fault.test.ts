import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import {
  PORTABLE_FILESYSTEM_FAULT_OPERATIONS,
  PORTABLE_FILESYSTEM_RESTART_FAULT_OPERATION_POSITIONS,
  PORTABLE_FILESYSTEM_RESTART_FAULT_POSITIONS,
  prepareFilesystemFaultAttempt,
  verifyFilesystemFaultAttempt,
} from "../../packages/testkit/dist/index.js";
import { expect, test } from "vitest";

test(
  "every filesystem statement fault survives physical Node driver destruction",
  { timeout: 300_000 },
  async () => {
    const directory = await mkdtemp(path.join(tmpdir(), "efs-m6-node-fault-restart-"));
    let observedTotal = 0;
    try {
      for (const operation of PORTABLE_FILESYSTEM_FAULT_OPERATIONS) {
        const filename = path.join(directory, `${operation}.db`);
        let observedPositions = 0;
        for (let occurrence = 1; occurrence <= 512; occurrence += 1) {
          let adapter = await openNodeSqlite({ filename });
          const attempt = await prepareFilesystemFaultAttempt(
            adapter,
            operation,
            occurrence,
          );
          // Do not close the filesystem. Destroy the physical connection and recreate
          // the engine before checking the complete old/new durable state.
          adapter.close();
          adapter = await openNodeSqlite({ filename, create: false });
          try {
            await verifyFilesystemFaultAttempt(adapter, operation, !attempt.injected);
          } finally {
            adapter.close();
          }
          if (!attempt.injected) {
            observedPositions = attempt.observedStatements;
            break;
          }
        }
        expect(observedPositions).toBe(
          PORTABLE_FILESYSTEM_RESTART_FAULT_OPERATION_POSITIONS[operation],
        );
        observedTotal += observedPositions;
      }
      expect(observedTotal).toBe(PORTABLE_FILESYSTEM_RESTART_FAULT_POSITIONS);
      console.log(
        `m6-filesystem-fault-evidence ${JSON.stringify({
          adapter: "node-sqlite-file-backed",
          statementPositions: observedTotal,
          operations: PORTABLE_FILESYSTEM_RESTART_FAULT_OPERATION_POSITIONS,
          restart: "physical-driver-destruction",
        })}`,
      );
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  },
);
