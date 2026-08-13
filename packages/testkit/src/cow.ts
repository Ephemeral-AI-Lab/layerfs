import { EphemeralFS } from "@ephemeralai/fs";
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";

export const PORTABLE_COW_PAGE_SIZES = Object.freeze([4096, 8192, 16384] as const);
export type PortableCowPageSize = (typeof PORTABLE_COW_PAGE_SIZES)[number];
export const PORTABLE_COW_CASE_IDS = Object.freeze([
  "cow-repeated-page-head",
  "cow-boundary-crossing",
  "cow-final-partial-page",
  "cow-pinned-snapshot",
  "cow-physical-reopen",
  "cow-conflicting-format-refusal",
] as const);

export interface PortableCowPreparation {
  readonly schema: "efs-portable-cow-preparation-v1";
  readonly pageBytes: PortableCowPageSize;
  readonly branchId: string;
  readonly fixtureDigest: string;
  readonly repeatedWrites: 1000;
}

export interface PortableCowResult extends PortableCowPreparation {
  readonly cases: typeof PORTABLE_COW_CASE_IDS;
  readonly pageHeadCount: number;
  readonly pageVersionCount: number;
  readonly finalPartialBytes: number;
}

type PageCounts = Readonly<Record<string, number>> & {
  readonly heads: number;
  readonly versions: number;
  readonly partial_bytes: number;
};

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`portable COW conformance: ${message}`);
}

function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  return (
    left.byteLength === right.byteLength &&
    left.every((byte, index) => byte === right[index])
  );
}

async function collect(stream: ReadableStream<Uint8Array>): Promise<Uint8Array> {
  const chunks: Uint8Array[] = [];
  let length = 0;
  const reader = stream.getReader();
  try {
    for (;;) {
      const result = await reader.read();
      if (result.done) break;
      chunks.push(result.value);
      length += result.value.byteLength;
    }
  } finally {
    reader.releaseLock();
  }
  const output = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    output.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return output;
}

function counts(
  adapter: FilesystemSQLiteDriver,
  branchId: string,
  pageBytes: number,
): PageCounts {
  const row = adapter.transaction(
    "read",
    (tx) =>
      tx.all<PageCounts>(
        "SELECT (SELECT count(*) FROM efs_cow_page_heads WHERE branch_id=?) heads,(SELECT count(*) FROM efs_cow_page_versions WHERE branch_id=?) versions,coalesce((SELECT length(v.bytes) FROM efs_cow_page_heads h JOIN efs_cow_page_versions v ON v.branch_id=h.branch_id AND v.inode_id=h.inode_id AND v.page_index=h.page_index AND v.generation=h.generation WHERE h.branch_id=? AND h.page_index=2),0) partial_bytes",
        [branchId, branchId, branchId],
        { maxRows: 1, maxBytes: 256 },
      )[0],
  );
  invariant(row !== undefined, "page counts are missing");
  invariant(
    row.partial_bytes === 0 || row.partial_bytes <= pageBytes,
    "partial page length is invalid",
  );
  return row;
}

async function digest(
  adapter: FilesystemSQLiteDriver,
  bytes: Uint8Array,
): Promise<string> {
  const hash = adapter.hashBytes
    ? adapter.hashBytes(bytes)
    : await adapter.hashBytesAsync?.(bytes);
  invariant(hash instanceof Uint8Array && hash.byteLength === 32, "SHA-256 is absent");
  return [...hash].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

/** Prepare all public COW mutations, intentionally separate from physical reopen. */
export async function preparePortableCowPageSize(
  adapter: FilesystemSQLiteDriver,
  pageBytes: PortableCowPageSize,
): Promise<PortableCowPreparation> {
  invariant(
    PORTABLE_COW_PAGE_SIZES.includes(pageBytes),
    `unsupported page size ${pageBytes}`,
  );
  const filesystem = await EphemeralFS.open({
    database: adapter,
    ownsDatabase: false,
    format: { cowPageBytes: pageBytes },
    filesystem: { preferredStreamChunkBytes: 1024 },
  });
  const branchId = `portable-cow-${pageBytes}`;
  const base = Uint8Array.from(
    { length: pageBytes * 2 + 17 },
    (_, index) => (index * 17 + pageBytes / 4096) & 0xff,
  );
  try {
    await filesystem.writeFile("/cow", base);
    const branch = await filesystem.branches.create(branchId);
    try {
      for (let iteration = 0; iteration < 1000; iteration += 1)
        await branch.writeRange("/cow", 7, Uint8Array.of(iteration & 0xff));
      let state = counts(adapter, branchId, pageBytes);
      invariant(
        state.heads === 1 && state.versions === 1,
        "1000 same-page writes did not retain one current page",
      );

      await branch.writeRange("/cow", pageBytes - 1, Uint8Array.of(0xa1, 0xb2));
      state = counts(adapter, branchId, pageBytes);
      invariant(
        state.heads === 2 && state.versions === 2,
        "boundary crossing did not install exactly two current pages",
      );

      await branch.writeRange("/cow", pageBytes * 2 + 3, Uint8Array.of(0xc3));
      state = counts(adapter, branchId, pageBytes);
      invariant(
        state.heads === 3 && state.versions === 3 && state.partial_bytes === 17,
        "final partial page was not stored at its exact logical length",
      );

      const selected = await branch.readStream("/cow");
      const expectedSnapshot = await branch.readFile("/cow");
      await branch.writeRange("/cow", 7, Uint8Array.of(0xd4));
      const pinnedState = counts(adapter, branchId, pageBytes);
      invariant(
        pinnedState.heads === 3 && pinnedState.versions === 4,
        "active stream did not pin exactly one predecessor page",
      );
      invariant(
        equalBytes(await collect(selected), expectedSnapshot),
        "pinned stream changed after page overwrite",
      );
      const current = await branch.readFile("/cow");
      invariant(current[7] === 0xd4, "new branch read missed the current page");
    } finally {
      await branch.close();
    }
    return Object.freeze({
      schema: "efs-portable-cow-preparation-v1",
      pageBytes,
      branchId,
      fixtureDigest: await digest(adapter, base),
      repeatedWrites: 1000,
    });
  } finally {
    await filesystem.close();
  }
}

/** Verify exact state and format refusal after the caller physically restarts storage. */
export async function verifyPortableCowPageSize(
  adapter: FilesystemSQLiteDriver,
  preparation: PortableCowPreparation,
): Promise<PortableCowResult> {
  const filesystem = await EphemeralFS.open({
    database: adapter,
    ownsDatabase: false,
    format: { cowPageBytes: preparation.pageBytes },
    filesystem: { preferredStreamChunkBytes: 1024 },
  });
  let state: PageCounts;
  try {
    const branch = await filesystem.branches.open(preparation.branchId);
    try {
      const bytes = await branch.readFile("/cow");
      invariant(bytes[7] === 0xd4, "current page changed after physical reopen");
      invariant(
        bytes[preparation.pageBytes - 1] === 0xa1 &&
          bytes[preparation.pageBytes] === 0xb2 &&
          bytes[preparation.pageBytes * 2 + 3] === 0xc3,
        "boundary or partial page changed after physical reopen",
      );
    } finally {
      await branch.close();
    }
    let collection = await filesystem.maintenance.collectGarbage({
      runId: `portable-cow-cleanup-${preparation.pageBytes}`,
      maxBatches: 1,
    });
    for (let call = 0; collection.state !== "complete" && call < 10_000; call += 1)
      collection = await filesystem.maintenance.collectGarbage({
        runId: `portable-cow-cleanup-${preparation.pageBytes}`,
        maxBatches: 1,
      });
    invariant(
      collection.state === "complete",
      "page cleanup collection did not finish",
    );
    state = counts(adapter, preparation.branchId, preparation.pageBytes);
    invariant(
      state.heads === 3 && state.versions === 3,
      "released predecessor page was not reclaimed after stream completion",
    );
  } finally {
    await filesystem.close();
  }

  const before = adapter.transaction(
    "read",
    (tx) =>
      tx.all<Readonly<Record<string, number>>>(
        "SELECT (SELECT main_revision FROM efs_meta WHERE singleton=1) head,(SELECT count(*) FROM efs_cow_page_versions) versions",
        [],
        { maxRows: 1, maxBytes: 256 },
      )[0],
  );
  const conflicting = preparation.pageBytes === 4096 ? 8192 : 4096;
  let rejected = false;
  try {
    await EphemeralFS.open({
      database: adapter,
      ownsDatabase: false,
      format: { cowPageBytes: conflicting },
      filesystem: { preferredStreamChunkBytes: 1024 },
    });
  } catch (error) {
    rejected =
      error !== null &&
      typeof error === "object" &&
      "code" in error &&
      error.code === "ESCHEMA";
  }
  invariant(rejected, "conflicting persisted page size was not rejected as ESCHEMA");
  const after = adapter.transaction(
    "read",
    (tx) =>
      tx.all<Readonly<Record<string, number>>>(
        "SELECT (SELECT main_revision FROM efs_meta WHERE singleton=1) head,(SELECT count(*) FROM efs_cow_page_versions) versions",
        [],
        { maxRows: 1, maxBytes: 256 },
      )[0],
  );
  invariant(
    JSON.stringify(after) === JSON.stringify(before),
    "conflicting format refusal changed durable state",
  );
  return Object.freeze({
    ...preparation,
    cases: PORTABLE_COW_CASE_IDS,
    pageHeadCount: state.heads,
    pageVersionCount: state.versions,
    finalPartialBytes: state.partial_bytes,
  });
}
