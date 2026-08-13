import { EphemeralFS } from "@ephemeralai/fs";
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";

export const PORTABLE_RESTART_SEED = 0x5e57a27;

export const PORTABLE_RESTART_CASE_IDS = Object.freeze([
  "restart-committed-state",
  "restart-active-branch",
  "restart-lost-response-replay",
  "restart-abandoned-lease",
  "restart-interrupted-collection",
] as const);

export type PortableRestartCaseId = (typeof PORTABLE_RESTART_CASE_IDS)[number];

export interface PortableRestartPreparation {
  readonly schema: "efs-portable-restart-preparation-v1";
  readonly seed: typeof PORTABLE_RESTART_SEED;
  readonly fixtureDigest: string;
  readonly publicationResult: string;
  readonly activeLeaseRows: number;
  readonly collectionState: "paused";
}

export interface PortableRestartResult {
  readonly schema: "efs-portable-restart-result-v1";
  readonly seed: typeof PORTABLE_RESTART_SEED;
  readonly fixtureDigest: string;
  readonly cases: readonly PortableRestartCaseId[];
  readonly verifiedEntities: number;
  readonly activeLeaseRows: number;
  readonly stagingRows: number;
  readonly collectionState: "complete";
}

const STORAGE = Object.freeze({
  maxGcBatchSize: 2,
  maxQueryBatchSize: 16,
  readLeaseMs: 10_000,
  stagingLeaseMs: 20,
});

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`portable restart conformance: ${message}`);
}

function fixtureBytes(): Uint8Array {
  let state = PORTABLE_RESTART_SEED;
  const bytes = new Uint8Array(128 * 1024);
  for (let index = 0; index < bytes.byteLength; index += 1) {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    bytes[index] = state & 0xff;
  }
  return bytes;
}

function hex(bytes: Uint8Array): string {
  let value = "";
  for (const byte of bytes) value += byte.toString(16).padStart(2, "0");
  return value;
}

async function digest(
  adapter: FilesystemSQLiteDriver,
  bytes: Uint8Array,
): Promise<string> {
  const result = adapter.hashBytes
    ? adapter.hashBytes(bytes)
    : await adapter.hashBytesAsync?.(bytes);
  invariant(
    result instanceof Uint8Array && result.byteLength === 32,
    "adapter SHA-256 is unavailable",
  );
  return hex(result);
}

function durableRows(adapter: FilesystemSQLiteDriver): {
  readonly leases: number;
  readonly staging: number;
} {
  const row = adapter.transaction(
    "read",
    (tx) =>
      tx.all<{ readonly leases: number; readonly staging: number }>(
        "SELECT (SELECT count(*) FROM efs_leases WHERE state IN (0,1)) leases,(SELECT count(*) FROM efs_staging_certificates) staging",
        [],
        { maxRows: 1, maxBytes: 256 },
      )[0],
  );
  invariant(row !== undefined, "durable lease counters are missing");
  return row;
}

/**
 * Establish durable state immediately before an unorderly physical/runtime restart.
 * The caller MUST destroy the Node connection or evict the Durable Object after this
 * function returns, without orderly filesystem or branch close.
 */
export async function preparePortableRestart(
  adapter: FilesystemSQLiteDriver,
): Promise<PortableRestartPreparation> {
  let now = 100;
  const filesystem = await EphemeralFS.open({
    database: adapter,
    ownsDatabase: false,
    clock: () => now++,
    storage: STORAGE,
  });
  const bytes = fixtureBytes();
  const fixtureDigest = await digest(adapter, bytes);
  await filesystem.writeFile("/restart-stable", bytes);

  const active = await filesystem.branches.create("portable-restart-active");
  await active.writeFile("/restart-active-value", "active-before-restart");

  const published = await filesystem.branches.create("portable-restart-published");
  await published.writeFile("/restart-published-value", "published-before-restart");
  const publication = await published.publish({
    operationId: "portable-restart-publication",
  });
  invariant(publication.outcome === "merged", "publication fixture did not merge");
  await published.close();

  const selected = await filesystem.readStream("/restart-stable");
  const reader = selected.getReader();
  const first = await reader.read();
  invariant(!first.done && first.value.byteLength > 0, "read lease was not acquired");

  for (let index = 0; index < 24; index += 1)
    await filesystem.writeFile(`/restart-orphan-${index}`, `orphan-${index}`);
  for (let index = 0; index < 24; index += 1)
    await filesystem.unlink(`/restart-orphan-${index}`);
  const collection = await filesystem.maintenance.collectGarbage({
    runId: "portable-restart-collection",
    maxBatches: 1,
  });
  invariant(collection.state === "paused", "collection did not expose restart state");
  const rows = durableRows(adapter);

  // These resources are intentionally abandoned. An orderly close here would turn
  // this into an adapter-reopen test rather than crash/runtime-eviction recovery.
  void filesystem;
  void active;
  void reader;
  return Object.freeze({
    schema: "efs-portable-restart-preparation-v1",
    seed: PORTABLE_RESTART_SEED,
    fixtureDigest,
    publicationResult: JSON.stringify(publication),
    activeLeaseRows: rows.leases,
    collectionState: "paused",
  });
}

/** Verify and finish the shared recovery scenario after a real physical/runtime restart. */
export async function verifyPortableRestart(
  adapter: FilesystemSQLiteDriver,
  preparation: PortableRestartPreparation,
): Promise<PortableRestartResult> {
  invariant(
    preparation.schema === "efs-portable-restart-preparation-v1" &&
      preparation.seed === PORTABLE_RESTART_SEED,
    "restart preparation identity differs",
  );
  let now = 1_000_000;
  const filesystem = await EphemeralFS.open({
    database: adapter,
    ownsDatabase: false,
    clock: () => now++,
    storage: STORAGE,
  });
  try {
    invariant(
      (await digest(adapter, await filesystem.readFile("/restart-stable"))) ===
        preparation.fixtureDigest,
      "committed bytes changed across restart",
    );

    const active = await filesystem.branches.open("portable-restart-active");
    invariant(
      (await active.readFile("/restart-active-value", { encoding: "utf8" })) ===
        "active-before-restart",
      "active branch state changed across restart",
    );
    await active.close();

    invariant(
      JSON.stringify(
        await filesystem.branches.replay(
          "portable-restart-publication",
          "portable-restart-published",
        ),
      ) === preparation.publicationResult,
      "lost-response publication replay changed across restart",
    );

    let collection = await filesystem.maintenance.collectGarbage({
      runId: "portable-restart-collection",
      maxBatches: 1,
    });
    for (let call = 0; call < 100_000 && collection.state !== "complete"; call += 1)
      collection = await filesystem.maintenance.collectGarbage({
        runId: "portable-restart-collection",
        maxBatches: 1,
      });
    invariant(
      collection.state === "complete",
      "collection did not resume after restart",
    );

    let cursor: string | undefined;
    let verifiedEntities = 0;
    for (let call = 0; call < 100_000; call += 1) {
      const verification = await filesystem.maintenance.verify({
        ...(cursor === undefined ? {} : { cursor }),
        maxEntities: 4,
      });
      verifiedEntities += verification.checkedEntities;
      cursor = verification.nextCursor ?? undefined;
      if (verification.complete) break;
    }
    invariant(
      cursor === undefined && verifiedEntities > 0,
      "verification did not finish",
    );
    const rows = durableRows(adapter);
    invariant(rows.leases === 0, "expired lease survived recovery maintenance");
    invariant(rows.staging === 0, "staging rows survived recovery maintenance");
    return Object.freeze({
      schema: "efs-portable-restart-result-v1",
      seed: PORTABLE_RESTART_SEED,
      fixtureDigest: preparation.fixtureDigest,
      cases: PORTABLE_RESTART_CASE_IDS,
      verifiedEntities,
      activeLeaseRows: rows.leases,
      stagingRows: rows.staging,
      collectionState: "complete",
    });
  } finally {
    await filesystem.close();
  }
}
