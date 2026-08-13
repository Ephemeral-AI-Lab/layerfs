import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import {
  PORTABLE_MAINTENANCE_FAULT_TOPOLOGY,
  runPortableMaintenanceFaultAttempt,
  verifyPortableMaintenanceFaultRecovery,
  type PortableMaintenanceFaultKind,
  type PortableMaintenanceFaultVariant,
} from "../../packages/testkit/dist/index.js";
import { expect, test } from "vitest";

const selectedVariant = (process.env.EFS_M6_MAINTENANCE_VARIANT ?? "") as
  PortableMaintenanceFaultVariant | "";
const selectedKind = (process.env.EFS_M6_MAINTENANCE_KIND ?? "") as
  PortableMaintenanceFaultKind | "";
const selectedStart = Number(process.env.EFS_M6_MAINTENANCE_START ?? 0);
const selectedEnd = Number(process.env.EFS_M6_MAINTENANCE_END ?? 0);
const hasChunk =
  (selectedVariant === "snapshot" ||
    selectedVariant === "collection" ||
    selectedVariant === "abandoned") &&
  (selectedKind === "statement" || selectedKind === "batch");

async function freshAttempt(
  variant: PortableMaintenanceFaultVariant,
  kind: PortableMaintenanceFaultKind,
  ordinal: number,
) {
  const directory = await mkdtemp(path.join(tmpdir(), `efs-m6-${variant}-${kind}-`));
  const filename = path.join(directory, "filesystem.db");
  let adapter = await openNodeSqlite({ filename });
  return {
    directory,
    filename,
    get adapter() {
      return adapter;
    },
    set adapter(value) {
      adapter = value;
    },
    result: await runPortableMaintenanceFaultAttempt(adapter, variant, kind, ordinal),
  };
}

(hasChunk ? test : test.skip)(
  `Node ${selectedVariant} ${selectedKind} faults ${selectedStart}-${selectedEnd} physically reopen`,
  { timeout: 300_000 },
  async () => {
    const probe = await freshAttempt(
      selectedVariant,
      selectedKind,
      Number.MAX_SAFE_INTEGER,
    );
    const baseline = probe.result.resultCounters;
    expect(baseline).toBeDefined();
    probe.adapter.close();
    await rm(probe.directory, { recursive: true, force: true });
    for (let ordinal = selectedStart; ordinal <= selectedEnd; ordinal += 1) {
      const attempt = await freshAttempt(selectedVariant, selectedKind, ordinal);
      try {
        expect(attempt.result.injected).toBe(true);
        attempt.adapter.close();
        attempt.adapter = await openNodeSqlite({
          filename: attempt.filename,
          create: false,
        });
        expect(
          await verifyPortableMaintenanceFaultRecovery(
            attempt.adapter,
            selectedVariant,
          ),
        ).toEqual(baseline);
      } finally {
        try {
          attempt.adapter.close();
        } catch {}
        await rm(attempt.directory, { recursive: true, force: true });
      }
    }
  },
);

(hasChunk ? test.skip : test)(
  "shared maintenance fault topology is exact on Node",
  async () => {
    const topology: Record<string, unknown> = {};
    for (const variant of ["snapshot", "collection", "abandoned"] as const) {
      const attempt = await freshAttempt(variant, "statement", Number.MAX_SAFE_INTEGER);
      try {
        expect(attempt.result.injected).toBe(false);
        expect(attempt.result.resultCounters).toBeDefined();
        expect(attempt.result.metrics).toEqual(
          PORTABLE_MAINTENANCE_FAULT_TOPOLOGY[variant],
        );
        topology[variant] = attempt.result.metrics;
      } finally {
        attempt.adapter.close();
        await rm(attempt.directory, { recursive: true, force: true });
      }
    }
    console.log(`m6-maintenance-fault-topology ${JSON.stringify(topology)}`);
  },
);
