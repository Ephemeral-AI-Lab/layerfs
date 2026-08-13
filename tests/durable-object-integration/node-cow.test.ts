import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import {
  PORTABLE_COW_CASE_IDS,
  PORTABLE_COW_PAGE_SIZES,
  preparePortableCowPageSize,
  verifyPortableCowPageSize,
} from "../../packages/testkit/dist/index.js";
import { expect, test } from "vitest";

test(
  "all persisted COW page sizes survive a physical Node reopen",
  { timeout: 300_000 },
  async () => {
    const directory = await mkdtemp(path.join(tmpdir(), "efs-m6-node-cow-"));
    const results = [];
    try {
      for (const pageBytes of PORTABLE_COW_PAGE_SIZES) {
        const filename = path.join(directory, `${pageBytes}.db`);
        let adapter = await openNodeSqlite({ filename });
        const preparation = await preparePortableCowPageSize(adapter, pageBytes);
        adapter.close();
        adapter = await openNodeSqlite({ filename, create: false });
        try {
          const result = await verifyPortableCowPageSize(adapter, preparation);
          expect(result.cases).toEqual(PORTABLE_COW_CASE_IDS);
          expect(result.pageHeadCount).toBe(3);
          expect(result.pageVersionCount).toBe(3);
          expect(result.finalPartialBytes).toBe(17);
          results.push(result);
        } finally {
          adapter.close();
        }
      }
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
    console.log(`m6-cow-evidence ${JSON.stringify({ adapter: "node", results })}`);
  },
);
