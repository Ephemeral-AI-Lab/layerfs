import { openCloudflareSqlite } from "../../packages/sqlite-cloudflare/dist/index.js";
import {
  PORTABLE_COW_CASE_IDS,
  PORTABLE_COW_PAGE_SIZES,
  preparePortableCowPageSize,
  verifyPortableCowPageSize,
} from "../../packages/testkit/dist/index.js";
import { env } from "cloudflare:workers";
import { evictDurableObject, reset, runInDurableObject } from "cloudflare:test";
import { afterEach, expect, test } from "vitest";

afterEach(async () => {
  await reset();
});

test(
  "all persisted COW page sizes survive real Durable Object eviction",
  { timeout: 300_000 },
  async () => {
    const results = [];
    for (const pageBytes of PORTABLE_COW_PAGE_SIZES) {
      const stub = env.FILESYSTEM.getByName(`portable-cow-${pageBytes}`);
      const preparation = await runInDurableObject(stub, async (_instance, state) => {
        const adapter = await openCloudflareSqlite({ storage: state.storage });
        try {
          return await preparePortableCowPageSize(adapter, pageBytes);
        } finally {
          adapter.close();
        }
      });
      await evictDurableObject(stub);
      const result = await runInDurableObject(stub, async (_instance, state) => {
        const adapter = await openCloudflareSqlite({ storage: state.storage });
        try {
          return await verifyPortableCowPageSize(adapter, preparation);
        } finally {
          adapter.close();
        }
      });
      expect(result.cases).toEqual(PORTABLE_COW_CASE_IDS);
      expect(result.pageHeadCount).toBe(3);
      expect(result.pageVersionCount).toBe(3);
      expect(result.finalPartialBytes).toBe(17);
      results.push(result);
    }
    console.log(
      `m6-cow-evidence ${JSON.stringify({
        adapter: "cloudflare-durable-object",
        restart: "evictDurableObject",
        results,
      })}`,
    );
  },
);
