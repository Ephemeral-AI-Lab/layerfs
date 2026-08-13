import { openCloudflareSqlite } from "../../packages/sqlite-cloudflare/dist/index.js";
import {
  PORTABLE_PUBLICATION_FAULT_POSITIONS,
  runPortablePublicationFaultAttempt,
  verifyPortablePublicationFaultRecovery,
  type PortablePublicationFaultVariant,
} from "../../packages/testkit/dist/index.js";
import { env } from "cloudflare:workers";
import { evictDurableObject, reset, runInDurableObject } from "cloudflare:test";
import { afterEach, expect, test } from "vitest";

afterEach(async () => {
  await reset();
});

const selectedVariant =
  __EFS_M6_PUBLICATION_VARIANT__ as PortablePublicationFaultVariant;
const selectedStart = Number(__EFS_M6_PUBLICATION_START__);
const selectedEnd = Number(__EFS_M6_PUBLICATION_END__);
const hasChunk = selectedVariant === "direct" || selectedVariant === "prepared";

(hasChunk ? test : test.skip)(
  `publication ${selectedVariant} faults ${selectedStart}-${selectedEnd} survive runtime eviction`,
  { timeout: 300_000 },
  async () => {
    expect(Number.isSafeInteger(selectedStart) && selectedStart > 0).toBe(true);
    expect(
      Number.isSafeInteger(selectedEnd) &&
        selectedEnd >= selectedStart &&
        selectedEnd <= PORTABLE_PUBLICATION_FAULT_POSITIONS[selectedVariant],
    ).toBe(true);
    for (let occurrence = selectedStart; occurrence <= selectedEnd; occurrence += 1) {
      const stub = env.FILESYSTEM.getByName(`publication-${selectedVariant}`);
      const attempt = await runInDurableObject(stub, async (_instance, state) => {
        const adapter = await openCloudflareSqlite({ storage: state.storage });
        try {
          return await runPortablePublicationFaultAttempt(
            adapter,
            selectedVariant,
            occurrence,
          );
        } finally {
          adapter.close();
        }
      });
      expect(attempt.injected).toBe(true);
      await evictDurableObject(stub);
      await runInDurableObject(stub, async (_instance, state) => {
        const adapter = await openCloudflareSqlite({ storage: state.storage });
        try {
          await verifyPortablePublicationFaultRecovery(adapter, selectedVariant);
        } finally {
          adapter.close();
        }
      });
      await reset();
    }
    console.log(
      `m6-publication-fault-evidence ${JSON.stringify({
        adapter: "cloudflare-durable-object",
        variant: selectedVariant,
        statementRange: [selectedStart, selectedEnd],
        totalStatements: PORTABLE_PUBLICATION_FAULT_POSITIONS[selectedVariant],
        restart: "evictDurableObject",
      })}`,
    );
  },
);

(hasChunk ? test.skip : test)(
  "publication fault topology matches Node in Workerd",
  async () => {
    for (const variant of ["direct", "prepared"] as const) {
      const stub = env.FILESYSTEM.getByName(`publication-topology-${variant}`);
      const attempt = await runInDurableObject(stub, async (_instance, state) => {
        const adapter = await openCloudflareSqlite({ storage: state.storage });
        try {
          return await runPortablePublicationFaultAttempt(
            adapter,
            variant,
            Number.MAX_SAFE_INTEGER,
          );
        } finally {
          adapter.close();
        }
      });
      expect(attempt.injected).toBe(false);
      expect(attempt.maxTransactionStatements).toBe(
        PORTABLE_PUBLICATION_FAULT_POSITIONS[variant],
      );
      await reset();
    }
  },
);
