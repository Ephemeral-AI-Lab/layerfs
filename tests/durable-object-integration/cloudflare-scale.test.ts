import { openCloudflareSqlite } from "../../packages/sqlite-cloudflare/dist/index.js";
import { PortableScaleSession } from "../../packages/testkit/dist/index.js";
import { env } from "cloudflare:workers";
import { evictDurableObject, reset, runInDurableObject } from "cloudflare:test";
import { afterEach, expect, test } from "vitest";

afterEach(async () => {
  await reset();
});

test(
  "the shared 100,000-row scale suite passes in the faithful runtime",
  { timeout: 600_000 },
  async () => {
    const stub = env.FILESYSTEM.getByName("portable-scale-conformance");
    const session = new PortableScaleSession("cloudflare-durable-object-scale");
    let result;
    let phaseIndex = 0;
    for (;;) {
      if (phaseIndex === 1 || phaseIndex === 3)
        console.log(
          `m6-workerd-resource-window ${JSON.stringify({
            phase: phaseIndex === 1 ? "baseline" : "full",
            edge: "start",
          })}`,
        );
      const outcome = await runInDurableObject(stub, async (_instance, state) => {
        const adapter = await openCloudflareSqlite({
          storage: state.storage,
          maxPhysicalDatabaseBytes: 512 * 1024 * 1024,
          maxJournalBytes: 512 * 1024 * 1024,
        });
        try {
          return await session.run(adapter);
        } finally {
          adapter.close();
        }
      });
      if (outcome.status === "complete") {
        result = outcome.result;
        break;
      }
      if (
        outcome.completedPhase === "baseline-measured" ||
        outcome.completedPhase === "full-measured"
      )
        console.log(
          `m6-workerd-resource-window ${JSON.stringify({
            phase: outcome.completedPhase === "baseline-measured" ? "baseline" : "full",
            edge: "end",
          })}`,
        );
      console.log(
        `m6-scale-phase ${JSON.stringify({
          phase: outcome.completedPhase,
          restart: "evictDurableObject",
        })}`,
      );
      await evictDurableObject(stub);
      session.recordPhysicalRestart();
      phaseIndex += 1;
    }
    expect(result).toMatchObject({
      schema: "efs-portable-scale-result-v1",
      adapter: "cloudflare-durable-object-scale",
      rows: 100_000,
      baselineRows: 10_240,
      physicalRestarts: 5,
    });
    console.log(`m6-scale-evidence ${JSON.stringify(result)}`);
  },
);
