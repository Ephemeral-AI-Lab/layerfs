import { openCloudflareSqlite } from "../../packages/sqlite-cloudflare/dist/index.js";
import {
  PORTABLE_MAINTENANCE_FAULT_TOPOLOGY,
  runPortableMaintenanceFaultAttempt,
  verifyPortableMaintenanceFaultRecovery,
  type PortableMaintenanceFaultKind,
  type PortableMaintenanceFaultVariant,
} from "../../packages/testkit/dist/index.js";
import { env } from "cloudflare:workers";
import { evictDurableObject, reset, runInDurableObject } from "cloudflare:test";
import { afterEach, expect, test } from "vitest";

afterEach(async () => {
  await reset();
});

const selectedVariant =
  __EFS_M6_MAINTENANCE_VARIANT__ as PortableMaintenanceFaultVariant;
const selectedKind = __EFS_M6_MAINTENANCE_KIND__ as PortableMaintenanceFaultKind;
const selectedStart = Number(__EFS_M6_MAINTENANCE_START__);
const selectedEnd = Number(__EFS_M6_MAINTENANCE_END__);
const hasChunk =
  (selectedVariant === "snapshot" ||
    selectedVariant === "collection" ||
    selectedVariant === "abandoned") &&
  (selectedKind === "statement" || selectedKind === "batch");

(hasChunk ? test : test.skip)(
  `Durable Object ${selectedVariant} ${selectedKind} faults ${selectedStart}-${selectedEnd} survive eviction`,
  { timeout: 300_000 },
  async () => {
    const limit =
      selectedKind === "statement"
        ? PORTABLE_MAINTENANCE_FAULT_TOPOLOGY[selectedVariant].durableStatements
        : PORTABLE_MAINTENANCE_FAULT_TOPOLOGY[selectedVariant].committedBatches;
    expect(
      selectedStart > 0 && selectedEnd >= selectedStart && selectedEnd <= limit,
    ).toBe(true);
    const baselineStub = env.FILESYSTEM.getByName(
      `maintenance-${selectedVariant}-baseline`,
    );
    const baselineAttempt = await runInDurableObject(
      baselineStub,
      async (_instance, state) => {
        const adapter = await openCloudflareSqlite({ storage: state.storage });
        try {
          return await runPortableMaintenanceFaultAttempt(
            adapter,
            selectedVariant,
            selectedKind,
            Number.MAX_SAFE_INTEGER,
          );
        } finally {
          adapter.close();
        }
      },
    );
    const baseline = baselineAttempt.resultCounters;
    expect(baseline).toBeDefined();
    await reset();

    for (let ordinal = selectedStart; ordinal <= selectedEnd; ordinal += 1) {
      const stub = env.FILESYSTEM.getByName(`maintenance-${selectedVariant}`);
      const attempt = await runInDurableObject(stub, async (_instance, state) => {
        const adapter = await openCloudflareSqlite({ storage: state.storage });
        try {
          return await runPortableMaintenanceFaultAttempt(
            adapter,
            selectedVariant,
            selectedKind,
            ordinal,
          );
        } finally {
          adapter.close();
        }
      });
      expect(attempt.injected).toBe(true);
      await evictDurableObject(stub);
      const recovered = await runInDurableObject(stub, async (_instance, state) => {
        const adapter = await openCloudflareSqlite({ storage: state.storage });
        try {
          return await verifyPortableMaintenanceFaultRecovery(adapter, selectedVariant);
        } finally {
          adapter.close();
        }
      });
      expect(recovered).toEqual(baseline);
      await reset();
    }
    console.log(
      `m6-maintenance-fault-evidence ${JSON.stringify({
        adapter: "cloudflare-durable-object",
        variant: selectedVariant,
        kind: selectedKind,
        range: [selectedStart, selectedEnd],
        limit,
        restart: "evictDurableObject",
      })}`,
    );
  },
);

(hasChunk ? test.skip : test)(
  "shared maintenance fault topology is exact in Workerd",
  async () => {
    for (const variant of ["snapshot", "collection", "abandoned"] as const) {
      const stub = env.FILESYSTEM.getByName(`maintenance-topology-${variant}`);
      const attempt = await runInDurableObject(stub, async (_instance, state) => {
        const adapter = await openCloudflareSqlite({ storage: state.storage });
        try {
          return await runPortableMaintenanceFaultAttempt(
            adapter,
            variant,
            "statement",
            Number.MAX_SAFE_INTEGER,
          );
        } finally {
          adapter.close();
        }
      });
      expect(attempt.injected).toBe(false);
      expect(attempt.metrics).toEqual(PORTABLE_MAINTENANCE_FAULT_TOPOLOGY[variant]);
      await reset();
    }
  },
);
