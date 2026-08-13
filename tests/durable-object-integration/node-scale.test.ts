import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import { runScaleConformance } from "../../packages/testkit/dist/index.js";
import { expect, test } from "vitest";

test(
  "the shared 100,000-row scale suite passes against file-backed Node",
  { timeout: 600_000 },
  async () => {
    const result = await runScaleConformance({
      name: "node-sqlite-scale",
      async create() {
        const directory = await mkdtemp(path.join(tmpdir(), "efs-m6-node-scale-"));
        const filename = path.join(directory, "filesystem.db");
        let current = await openNodeSqlite({
          filename,
          maxJournalBytes: 256 * 1024 * 1024,
        });
        let disposed = false;
        return {
          adapter: current,
          capabilities: ["garbage-collection", "physical-reopen"],
          collectGarbage(filesystem, options) {
            return filesystem.maintenance.collectGarbage(options);
          },
          async reopen() {
            current = await openNodeSqlite({
              filename,
              create: false,
              maxJournalBytes: 256 * 1024 * 1024,
            });
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
    expect(result).toMatchObject({
      schema: "efs-portable-scale-result-v1",
      adapter: "node-sqlite-scale",
      rows: 100_000,
      baselineRows: 10_240,
    });
    console.log(`m6-scale-evidence ${JSON.stringify(result)}`);
  },
);
