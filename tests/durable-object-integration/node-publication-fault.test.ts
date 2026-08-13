import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import {
  PORTABLE_PUBLICATION_FAULT_POSITIONS,
  runPortablePublicationFaultAttempt,
  verifyPortablePublicationFaultRecovery,
} from "../../packages/testkit/dist/index.js";
import { expect, test } from "vitest";

test("shared publication fault topology is measurable on Node", async () => {
  const topology: Record<string, number> = {};
  for (const variant of ["direct", "prepared"] as const) {
    const directory = await mkdtemp(path.join(tmpdir(), `efs-m6-${variant}-`));
    const filename = path.join(directory, "filesystem.db");
    const adapter = await openNodeSqlite({ filename });
    try {
      const result = await runPortablePublicationFaultAttempt(
        adapter,
        variant,
        Number.MAX_SAFE_INTEGER,
      );
      expect(result.injected).toBe(false);
      expect(result.maxTransactionStatements).toBe(
        PORTABLE_PUBLICATION_FAULT_POSITIONS[variant],
      );
      topology[variant] = result.maxTransactionStatements;
    } finally {
      adapter.close();
      await rm(directory, { recursive: true, force: true });
    }
  }
  console.log(`m6-publication-fault-topology ${JSON.stringify(topology)}`);
});

test(
  "every shared publication fault survives physical Node driver recreation",
  { timeout: 300_000 },
  async () => {
    for (const variant of ["direct", "prepared"] as const) {
      for (
        let occurrence = 1;
        occurrence <= PORTABLE_PUBLICATION_FAULT_POSITIONS[variant];
        occurrence += 1
      ) {
        const directory = await mkdtemp(
          path.join(tmpdir(), `efs-m6-${variant}-fault-`),
        );
        const filename = path.join(directory, "filesystem.db");
        let adapter = await openNodeSqlite({ filename });
        try {
          const attempt = await runPortablePublicationFaultAttempt(
            adapter,
            variant,
            occurrence,
          );
          expect(attempt.injected).toBe(true);
          adapter.close();
          adapter = await openNodeSqlite({ filename, create: false });
          await verifyPortablePublicationFaultRecovery(adapter, variant);
        } finally {
          try {
            adapter.close();
          } catch {}
          await rm(directory, { recursive: true, force: true });
        }
      }
    }
  },
);
