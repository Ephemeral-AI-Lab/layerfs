import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import {
  PORTABLE_CURRENT_SCHEMA_VERSION,
  PORTABLE_RELEASED_SCHEMA_VERSIONS,
  runPortableInitializationIdentityAttempt,
  runPortableMigrationAttempt,
  verifyPortableEmptyInitialization,
  verifyPortableRecoverableMigrationState,
} from "../../packages/testkit/dist/index.js";
import { createV1Schema } from "../fixtures/schema-v1.mjs";
import { createV2Schema } from "../fixtures/schema-v2.mjs";
import { createV3Schema } from "../fixtures/schema-v3.mjs";
import { expect, test } from "vitest";

const fixtures = Object.freeze({
  1: createV1Schema,
  2: createV2Schema,
  3: createV3Schema,
});

test(
  "every released schema migration rolls back at every Node statement and physical reopen",
  { timeout: 300_000 },
  async () => {
    const statementCounts: Record<string, number> = {};
    for (const version of PORTABLE_RELEASED_SCHEMA_VERSIONS) {
      for (let occurrence = 1; occurrence <= 512; occurrence += 1) {
        const directory = await mkdtemp(path.join(tmpdir(), `efs-m6-v${version}-`));
        const filename = path.join(directory, "filesystem.db");
        let adapter = await openNodeSqlite({ filename });
        try {
          fixtures[version](adapter);
          const attempt = await runPortableMigrationAttempt(
            adapter,
            version,
            occurrence,
          );
          if (!attempt.injected) {
            expect(attempt.finalVersion).toBe(PORTABLE_CURRENT_SCHEMA_VERSION);
            expect(occurrence).toBe(attempt.observedStatements + 1);
            statementCounts[`v${version}`] = attempt.observedStatements;
            break;
          }
          expect(attempt.finalVersion).toBeGreaterThanOrEqual(version);
          expect(attempt.finalVersion).toBeLessThanOrEqual(
            PORTABLE_CURRENT_SCHEMA_VERSION,
          );
          adapter.close();
          adapter = await openNodeSqlite({ filename, create: false });
          verifyPortableRecoverableMigrationState(
            adapter,
            version,
            attempt.finalVersion,
          );
        } finally {
          try {
            adapter.close();
          } catch {}
          await rm(directory, { recursive: true, force: true });
        }
      }
      expect(statementCounts[`v${version}`]).toBeGreaterThan(0);
    }
    console.log(
      `m6-migration-evidence ${JSON.stringify({
        adapter: "node-sqlite",
        sourceVersions: PORTABLE_RELEASED_SCHEMA_VERSIONS,
        statementCounts,
        restart: "physical-driver-reopen",
      })}`,
    );
  },
);

test("Node initialization rolls back before and after every header identity write", async () => {
  let observedBoundaries = 0;
  let identityWrites = 0;
  for (let boundary = 1; boundary <= 64; boundary += 1) {
    const directory = await mkdtemp(path.join(tmpdir(), "efs-m6-node-identity-"));
    const filename = path.join(directory, "filesystem.db");
    let adapter = await openNodeSqlite({ filename });
    try {
      const attempt = await runPortableInitializationIdentityAttempt(adapter, boundary);
      observedBoundaries = Math.max(observedBoundaries, attempt.observedBoundaries);
      identityWrites = Math.max(identityWrites, attempt.identityWrites);
      adapter.close();
      adapter = await openNodeSqlite({ filename, create: false });
      if (!attempt.injected) {
        expect(boundary).toBe(attempt.observedBoundaries + 1);
        break;
      }
      verifyPortableEmptyInitialization(adapter);
    } finally {
      try {
        adapter.close();
      } catch {}
      await rm(directory, { recursive: true, force: true });
    }
  }
  expect(observedBoundaries).toBeGreaterThan(0);
  expect(identityWrites).toBeGreaterThan(0);
  console.log(
    `m6-initialization-identity-evidence ${JSON.stringify({
      adapter: "node-sqlite",
      identityMode: "sqlite-header",
      identityWrites,
      beforeAfterBoundaries: observedBoundaries,
      restart: "physical-driver-reopen",
    })}`,
  );
});
