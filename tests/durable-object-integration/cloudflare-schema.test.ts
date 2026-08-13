import { openCloudflareSqlite } from "../../packages/sqlite-cloudflare/dist/index.js";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import {
  PORTABLE_CURRENT_SCHEMA_VERSION,
  PORTABLE_DURABLE_MIGRATION_STATEMENT_COUNTS,
  PORTABLE_RELEASED_SCHEMA_VERSIONS,
  runPortableMigrationAttempt,
  verifyPortableRecoverableMigrationState,
} from "../../packages/testkit/dist/index.js";
import { createV1Schema } from "../fixtures/schema-v1.mjs";
import { createV2Schema } from "../fixtures/schema-v2.mjs";
import { createV3Schema } from "../fixtures/schema-v3.mjs";
import { env } from "cloudflare:workers";
import { evictDurableObject, reset, runInDurableObject } from "cloudflare:test";
import { afterEach, expect, test } from "vitest";

const fixtures = Object.freeze({
  1: createV1Schema,
  2: createV2Schema,
  3: createV3Schema,
});

afterEach(async () => {
  await reset();
});

const selectedVersion = Number(__EFS_M6_MIGRATION_VERSION__) as 1 | 2 | 3;
const selectedStart = Number(__EFS_M6_MIGRATION_START__);
const selectedEnd = Number(__EFS_M6_MIGRATION_END__);
const hasChunk = PORTABLE_RELEASED_SCHEMA_VERSIONS.includes(selectedVersion);

(hasChunk ? test : test.skip)(
  `released schema v${selectedVersion} rolls back at selected statements ${selectedStart}-${selectedEnd} and survives eviction`,
  { timeout: 300_000 },
  async () => {
    expect(Number.isSafeInteger(selectedStart) && selectedStart > 0).toBe(true);
    expect(
      Number.isSafeInteger(selectedEnd) &&
        selectedEnd >= selectedStart &&
        selectedEnd <= PORTABLE_DURABLE_MIGRATION_STATEMENT_COUNTS[selectedVersion],
    ).toBe(true);
    for (let occurrence = selectedStart; occurrence <= selectedEnd; occurrence += 1) {
      const stub = env.FILESYSTEM.getByName(`migration-v${selectedVersion}`);
      const attempt = await runInDurableObject(stub, async (_instance, state) => {
        const adapter = await openCloudflareSqlite({ storage: state.storage });
        fixtures[selectedVersion](adapter);
        try {
          return await runPortableMigrationAttempt(
            adapter,
            selectedVersion,
            occurrence,
          );
        } finally {
          adapter.close();
        }
      });
      expect(attempt.injected).toBe(true);
      expect(attempt.finalVersion).toBeGreaterThanOrEqual(selectedVersion);
      expect(attempt.finalVersion).toBeLessThanOrEqual(PORTABLE_CURRENT_SCHEMA_VERSION);
      await evictDurableObject(stub);
      await runInDurableObject(stub, async (_instance, state) => {
        const adapter = await openCloudflareSqlite({ storage: state.storage });
        try {
          verifyPortableRecoverableMigrationState(
            adapter,
            selectedVersion,
            attempt.finalVersion,
          );
        } finally {
          adapter.close();
        }
      });
      await reset();
    }
    console.log(
      `m6-migration-evidence ${JSON.stringify({
        adapter: "cloudflare-durable-object",
        sourceVersion: selectedVersion,
        statementRange: [selectedStart, selectedEnd],
        totalStatements: PORTABLE_DURABLE_MIGRATION_STATEMENT_COUNTS[selectedVersion],
        restart: "evictDurableObject",
      })}`,
    );
  },
);

(hasChunk ? test.skip : test)(
  "released migrations have the exact shared statement topology",
  async () => {
    for (const version of PORTABLE_RELEASED_SCHEMA_VERSIONS) {
      const stub = env.FILESYSTEM.getByName(`migration-topology-v${version}`);
      const attempt = await runInDurableObject(stub, async (_instance, state) => {
        const adapter = await openCloudflareSqlite({ storage: state.storage });
        fixtures[version](adapter);
        try {
          return await runPortableMigrationAttempt(
            adapter,
            version,
            Number.MAX_SAFE_INTEGER,
          );
        } finally {
          adapter.close();
        }
      });
      expect(attempt.injected).toBe(false);
      expect(attempt.observedStatements).toBe(
        PORTABLE_DURABLE_MIGRATION_STATEMENT_COUNTS[version],
      );
      expect(attempt.finalVersion).toBe(PORTABLE_CURRENT_SCHEMA_VERSION);
      await reset();
    }
  },
);

(hasChunk ? test.skip : test)(
  "durable-table identity refuses wrong, mismatched, malformed, and absent states",
  async () => {
    await reset();
    const mutations = Object.freeze([
      [
        "wrong-application",
        "UPDATE efs_schema_identity SET application_id=1 WHERE singleton=1",
      ],
      [
        "mismatched-version",
        "UPDATE efs_schema_identity SET user_version=12 WHERE singleton=1",
      ],
      [
        "newer-version",
        "UPDATE efs_schema_identity SET user_version=14 WHERE singleton=1; UPDATE efs_meta SET schema_version=14 WHERE singleton=1",
      ],
      [
        "too-old-version",
        "UPDATE efs_schema_identity SET user_version=0 WHERE singleton=1; UPDATE efs_meta SET schema_version=0 WHERE singleton=1",
      ],
      [
        "malformed-identity",
        "DROP TABLE efs_schema_identity; CREATE TABLE efs_schema_identity(singleton INTEGER PRIMARY KEY,application_id INTEGER,user_version INTEGER); INSERT INTO efs_schema_identity VALUES(1,1161905747,13)",
      ],
      ["absent-identity", "DROP TABLE efs_schema_identity"],
    ] as const);
    for (const [name, mutation] of mutations) {
      const stub = env.FILESYSTEM.getByName(`identity-refusal-${name}`);
      const result = await runInDurableObject(stub, async (_instance, state) => {
        let adapter = await openCloudflareSqlite({ storage: state.storage });
        const filesystem = await EphemeralFS.open({
          database: adapter,
          ownsDatabase: false,
        });
        await filesystem.close();
        adapter.transaction("write", (tx) => {
          for (const statement of mutation.split(";")) tx.run(statement.trim());
        });
        adapter.close();
        adapter = await openCloudflareSqlite({ storage: state.storage });
        try {
          await EphemeralFS.open({ database: adapter, ownsDatabase: false });
          return "opened";
        } catch (error) {
          return String(error);
        } finally {
          adapter.close();
        }
      });
      expect(result).toMatch(/ESCHEMA/);
    }
  },
);
