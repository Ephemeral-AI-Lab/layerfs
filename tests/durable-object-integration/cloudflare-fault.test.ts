import { openCloudflareSqlite } from "../../packages/sqlite-cloudflare/dist/index.js";
import {
  PORTABLE_FILESYSTEM_RESTART_FAULT_OPERATION_POSITIONS,
  prepareFilesystemFaultAttempt,
  verifyFilesystemFaultAttempt,
  type PortableFilesystemFaultOperation,
} from "../../packages/testkit/dist/index.js";
import { env } from "cloudflare:workers";
import { evictDurableObject, reset, runInDurableObject } from "cloudflare:test";
import { afterEach, expect, test } from "vitest";

afterEach(async () => {
  await reset();
});

const selectedOperation =
  __EFS_M6_FILESYSTEM_FAULT_OPERATION__ as PortableFilesystemFaultOperation;
const expectedPositions =
  PORTABLE_FILESYSTEM_RESTART_FAULT_OPERATION_POSITIONS[selectedOperation];
const hasSelection = expectedPositions !== undefined;

(hasSelection ? test : test.skip)(
  `${selectedOperation} faults survive runtime eviction after every SQL statement`,
  { timeout: 300_000 },
  async () => {
    const stub = env.FILESYSTEM.getByName(`filesystem-fault-${selectedOperation}`);
    let observedPositions = 0;
    for (let occurrence = 1; occurrence <= 512; occurrence += 1) {
      const attempt = await runInDurableObject(stub, async (_instance, state) => {
        const adapter = await openCloudflareSqlite({ storage: state.storage });
        try {
          return await prepareFilesystemFaultAttempt(
            adapter,
            selectedOperation,
            occurrence,
          );
        } finally {
          // The filesystem is intentionally not closed. Closing only this callback's
          // adapter models a lost isolate before the physical runtime eviction below.
          adapter.close();
        }
      });
      await evictDurableObject(stub);
      await runInDurableObject(stub, async (_instance, state) => {
        const adapter = await openCloudflareSqlite({ storage: state.storage });
        try {
          await verifyFilesystemFaultAttempt(
            adapter,
            selectedOperation,
            !attempt.injected,
          );
        } finally {
          adapter.close();
        }
      });
      if (!attempt.injected) {
        observedPositions = attempt.observedStatements;
        break;
      }
    }
    expect(observedPositions).toBe(expectedPositions);
    console.log(
      `m6-filesystem-fault-evidence ${JSON.stringify({
        adapter: "cloudflare-durable-object",
        operation: selectedOperation,
        statementPositions: expectedPositions,
        restart: "evictDurableObject",
      })}`,
    );
  },
);

(hasSelection ? test.skip : test)("filesystem fault selection is explicit", () => {
  expect(__EFS_M6_FILESYSTEM_FAULT_OPERATION__).toBe("");
});
