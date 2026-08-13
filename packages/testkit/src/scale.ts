import { EphemeralFS } from "@ephemeralai/fs";
import type {
  FilesystemSQLiteDriver,
  SqliteValue,
} from "@ephemeralai/fs/sqlite-driver";
import type { ConformanceAdapterFactory } from "./index.js";

const SCALE_ROWS = 100_000;
const BASELINE_ROWS = 10_240;
export const PORTABLE_SCALE_SEED = 0x5ca1e;
const GENERATION_BATCH = 256;
const CHARGED_ROW_BYTES = 512;
const MARK_RESERVATION_BYTES = 704;
const STORAGE = Object.freeze({
  maxGcBatchSize: 256,
  maxQueryBatchSize: 256,
  maxMaintenanceBytes: 256 * 1024 * 1024,
  maintenanceReserveBytes: 256 * 1024 * 1024,
});
const RUNTIME = Object.freeze({
  maxManagedResidentBytes: 104 * 1024 * 1024,
  maxCacheBytes: 4 * 1024 * 1024,
  maxQueryBatchBytes: 256 * 1024,
});

const USAGE_COLUMNS = Object.freeze([
  "object_count",
  "object_bytes",
  "manifest_root_count",
  "manifest_root_bytes",
  "manifest_node_count",
  "manifest_node_bytes",
  "page_count",
  "page_bytes",
  "patch_count",
  "patch_bytes",
  "staging_bytes",
  "ingest_reservation_bytes",
  "result_bytes",
  "maintenance_bytes",
  "permanent_identifiers",
  "charged_metadata_bytes",
] as const);
const USAGE_INTEGRITY_SQL = [...USAGE_COLUMNS, "mutation_sequence"]
  .map((column) => `CAST(${column} AS TEXT)`)
  .join("||':'||");

type MetaRow = Readonly<Record<string, SqliteValue>> & {
  readonly root_inode: string;
  readonly next_allocation_sequence: number;
};
type CountRow = Readonly<Record<string, SqliteValue>> & {
  readonly objects: number;
  readonly roots: number;
  readonly nodes: number;
  readonly entries: number;
};
type MarkRow = Readonly<Record<string, SqliteValue>> & { readonly value: number };

interface ScaleRecord {
  readonly index: number;
  readonly name: string;
  readonly nameBytes: Uint8Array;
  readonly inodeId: string;
  readonly objectBytes: Uint8Array;
  readonly objectHash: Uint8Array;
  readonly nodeBytes: Uint8Array;
  readonly nodeHash: Uint8Array;
  readonly rootBytes: Uint8Array;
  readonly rootHash: Uint8Array;
}

export interface PortableScaleResult {
  readonly schema: "efs-portable-scale-result-v1";
  readonly adapter: string;
  readonly seed: typeof PORTABLE_SCALE_SEED;
  readonly fixtureDigest: string;
  readonly rows: 100000;
  readonly baselineRows: 10240;
  readonly objectRows: number;
  readonly namespaceRows: number;
  readonly manifestRootRows: number;
  readonly manifestNodeRows: number;
  readonly baselineManagedPeakBytes: number;
  readonly fullManagedPeakBytes: number;
  readonly peakStorageMarks: number;
  readonly peakGcMarks: number;
  readonly verifiedRows: number;
  readonly maxMaintenanceCallMs: number;
  readonly mainFileBytes: number;
  readonly physicalRestarts?: number;
}

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`portable scale conformance: ${message}`);
}

function writeU64(view: DataView, offset: number, value: number): void {
  invariant(Number.isSafeInteger(value) && value >= 0, "invalid uint64 fixture value");
  view.setUint32(offset, value >>> 0, true);
  view.setUint32(offset + 4, Math.floor(value / 0x1_0000_0000), true);
}

function encodeLeaf(objectHash: Uint8Array): Uint8Array {
  const bytes = new Uint8Array(68);
  bytes.set([0x45, 0x41, 0x46, 0x4e]);
  const view = new DataView(bytes.buffer);
  view.setUint16(4, 1, true);
  bytes[6] = 0;
  bytes[7] = 1;
  view.setUint32(8, 1, true);
  writeU64(view, 16, 4);
  writeU64(view, 24, 1);
  bytes.set(objectHash, 32);
  view.setUint32(64, 4, true);
  return bytes;
}

function encodeRoot(nodeHash: Uint8Array): Uint8Array {
  const bytes = new Uint8Array(68);
  bytes.set([0x45, 0x41, 0x46, 0x52]);
  const view = new DataView(bytes.buffer);
  view.setUint16(4, 1, true);
  bytes[6] = 1;
  bytes[7] = 1;
  view.setUint32(8, 1, true);
  view.setUint32(12, 2, true);
  view.setUint32(16, 4, true);
  writeU64(view, 20, 4);
  writeU64(view, 28, 1);
  bytes.set(nodeHash, 36);
  return bytes;
}

async function hash(
  adapter: FilesystemSQLiteDriver,
  bytes: Uint8Array,
): Promise<Uint8Array> {
  if (adapter.hashBytesAsync) return adapter.hashBytesAsync(bytes);
  if (adapter.hashBytes) return adapter.hashBytes(bytes);
  const owned = Uint8Array.from(bytes);
  return new Uint8Array(await crypto.subtle.digest("SHA-256", owned.buffer));
}

function hex(bytes: Uint8Array): string {
  let value = "";
  for (const byte of bytes) value += byte.toString(16).padStart(2, "0");
  return value;
}

async function extendFixtureDigest(
  adapter: FilesystemSQLiteDriver,
  previous: Uint8Array,
  batch: readonly ScaleRecord[],
): Promise<Uint8Array> {
  const length = batch.reduce(
    (total, record) => total + 8 + record.nameBytes.byteLength + 32 * 3,
    previous.byteLength,
  );
  const bytes = new Uint8Array(length);
  bytes.set(previous);
  const view = new DataView(bytes.buffer);
  let offset = previous.byteLength;
  for (const record of batch) {
    view.setUint32(offset, record.index, true);
    view.setUint32(offset + 4, record.nameBytes.byteLength, true);
    offset += 8;
    bytes.set(record.nameBytes, offset);
    offset += record.nameBytes.byteLength;
    bytes.set(record.objectHash, offset);
    offset += record.objectHash.byteLength;
    bytes.set(record.nodeHash, offset);
    offset += record.nodeHash.byteLength;
    bytes.set(record.rootHash, offset);
    offset += record.rootHash.byteLength;
  }
  return hash(adapter, bytes);
}

function objectBytes(index: number): Uint8Array {
  const bytes = new Uint8Array(4);
  new DataView(bytes.buffer).setUint32(0, index, true);
  return bytes;
}

async function records(
  adapter: FilesystemSQLiteDriver,
  start: number,
  end: number,
): Promise<readonly ScaleRecord[]> {
  const encoder = new TextEncoder();
  const pending = Array.from({ length: end - start }, (_, offset) => {
    const index = start + offset;
    const name = `scale-${index.toString().padStart(6, "0")}`;
    return {
      index,
      name,
      nameBytes: encoder.encode(name),
      inodeId: `scale-inode-${index.toString().padStart(6, "0")}`,
      objectBytes: objectBytes(index),
    };
  });
  const objectHashes = await Promise.all(
    pending.map((record) => hash(adapter, record.objectBytes)),
  );
  const nodeBytes = objectHashes.map(encodeLeaf);
  const nodeHashes = await Promise.all(nodeBytes.map((bytes) => hash(adapter, bytes)));
  const rootBytes = nodeHashes.map(encodeRoot);
  const rootHashes = await Promise.all(rootBytes.map((bytes) => hash(adapter, bytes)));
  return Object.freeze(
    pending.map((record, offset) =>
      Object.freeze({
        ...record,
        objectHash: objectHashes[offset]!,
        nodeBytes: nodeBytes[offset]!,
        nodeHash: nodeHashes[offset]!,
        rootBytes: rootBytes[offset]!,
        rootHash: rootHashes[offset]!,
      }),
    ),
  );
}

function insertRecords(
  adapter: FilesystemSQLiteDriver,
  rootInode: string,
  firstSequence: number,
  records: readonly ScaleRecord[],
): void {
  adapter.transaction("write", (tx) => {
    for (let start = 0; start < records.length; start += 32) {
      const batch = records.slice(start, start + 32);
      tx.run(
        `INSERT INTO efs_cas_objects(hash,size,bytes,allocation_sequence) VALUES ${batch
          .map(() => "(?,4,?,?)")
          .join(",")}`,
        batch.flatMap((record, offset) => [
          record.objectHash,
          record.objectBytes,
          firstSequence + (start + offset) * 3,
        ]),
      );
    }
    for (let start = 0; start < records.length; start += 32) {
      const batch = records.slice(start, start + 32);
      tx.run(
        `INSERT INTO efs_manifest_nodes(hash,kind,logical_bytes,entry_count,encoded,allocation_sequence) VALUES ${batch
          .map(() => "(?,0,4,1,?,?)")
          .join(",")}`,
        batch.flatMap((record, offset) => [
          record.nodeHash,
          record.nodeBytes,
          firstSequence + (start + offset) * 3 + 1,
        ]),
      );
    }
    for (let start = 0; start < records.length; start += 25) {
      const batch = records.slice(start, start + 25);
      tx.run(
        `INSERT INTO efs_manifest_roots(hash,root_node_hash,file_size,entry_count,chunk_min,chunk_avg,chunk_max,encoded,allocation_sequence) VALUES ${batch
          .map(() => "(?,?,4,1,1,2,4,?,?)")
          .join(",")}`,
        batch.flatMap((record, offset) => [
          record.rootHash,
          record.nodeHash,
          record.rootBytes,
          firstSequence + (start + offset) * 3 + 2,
        ]),
      );
    }
    for (let start = 0; start < records.length; start += 100) {
      const batch = records.slice(start, start + 100);
      tx.run(
        `INSERT INTO efs_manifest_validations(manifest_hash,tree_depth) VALUES ${batch
          .map(() => "(?,1)")
          .join(",")}`,
        batch.map((record) => record.rootHash),
      );
    }
    for (let start = 0; start < records.length; start += 50) {
      const batch = records.slice(start, start + 50);
      tx.run(
        `INSERT INTO efs_inodes(id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token) VALUES ${batch
          .map(() => "(?,0,420,1,1,1,1,4,?,NULL,0)")
          .join(",")}`,
        batch.flatMap((record) => [record.inodeId, record.rootHash]),
      );
    }
    for (let start = 0; start < records.length; start += 25) {
      const batch = records.slice(start, start + 25);
      tx.run(
        `INSERT INTO efs_entries(parent_inode,name_sort,name,inode_id,token) VALUES ${batch
          .map(() => "(?,?,?,?,0)")
          .join(",")}`,
        batch.flatMap((record) => [
          rootInode,
          record.nameBytes,
          record.name,
          record.inodeId,
        ]),
      );
    }
  });
}

function accountRecords(
  adapter: FilesystemSQLiteDriver,
  batch: readonly ScaleRecord[],
): void {
  const variableMetadataBytes = batch.reduce(
    (total, record) => total + record.nameBytes.byteLength * 2,
    0,
  );
  adapter.transaction("write", (tx) => {
    tx.run(
      "UPDATE efs_usage SET object_count=object_count+?,object_bytes=object_bytes+?,manifest_root_count=manifest_root_count+?,manifest_root_bytes=manifest_root_bytes+?,manifest_node_count=manifest_node_count+?,manifest_node_bytes=manifest_node_bytes+?,maintenance_bytes=maintenance_bytes+?,charged_metadata_bytes=charged_metadata_bytes+?,mutation_sequence=mutation_sequence+1 WHERE singleton=1",
      [
        batch.length,
        batch.length * 4,
        batch.length,
        batch.length * 68,
        batch.length,
        batch.length * 68,
        batch.length * 3 * MARK_RESERVATION_BYTES,
        batch.length * 6 * CHARGED_ROW_BYTES + variableMetadataBytes,
      ],
    );
    tx.run(
      `UPDATE efs_usage SET integrity_token=${USAGE_INTEGRITY_SQL} WHERE singleton=1`,
    );
  });
}

function scalar(adapter: FilesystemSQLiteDriver, sql: string): number {
  const row = adapter.transaction(
    "read",
    (tx) => tx.all<MarkRow>(sql, [], { maxRows: 1, maxBytes: 128 })[0],
  );
  invariant(
    row !== undefined && Number.isSafeInteger(row.value),
    "invalid scalar result",
  );
  return row.value;
}

function exactCounts(adapter: FilesystemSQLiteDriver): CountRow {
  const row = adapter.transaction(
    "read",
    (tx) =>
      tx.all<CountRow>(
        "SELECT (SELECT count(*) FROM efs_cas_objects) AS objects,(SELECT count(*) FROM efs_manifest_roots) AS roots,(SELECT count(*) FROM efs_manifest_nodes) AS nodes,(SELECT count(*) FROM efs_entries) AS entries",
        [],
        { maxRows: 1, maxBytes: 256 },
      )[0],
  );
  invariant(row !== undefined, "scale cardinalities are missing");
  return row;
}

export type PortableScalePhaseOutcome =
  | Readonly<{
      status: "restart";
      completedPhase:
        | "baseline-built"
        | "baseline-measured"
        | "full-built"
        | "full-measured"
        | "collection-paused";
    }>
  | Readonly<{ status: "complete"; result: PortableScaleResult }>;

/** Host-coordinated scale gate whose four restart boundaries require real eviction. */
export class PortableScaleSession {
  readonly #adapterName: string;
  #phase: 0 | 1 | 2 | 3 | 4 | 5 = 0;
  #restartPending = false;
  #physicalRestarts = 0;
  #rootInode: string | undefined;
  #sequence = 0;
  #initial: CountRow | undefined;
  #exact: CountRow | undefined;
  #fixtureDigest: Uint8Array = new Uint8Array(32);
  #baselineManagedPeakBytes = 0;
  #fullManagedPeakBytes = 0;
  #peakStorageMarks = 0;
  #peakGcMarks = 0;
  #verifiedRows = 0;
  #maxMaintenanceCallMs = 0;

  constructor(adapterName: string) {
    invariant(adapterName.length > 0, "adapter name is empty");
    this.#adapterName = adapterName;
  }

  recordPhysicalRestart(): void {
    invariant(this.#restartPending, "no scale restart is pending");
    this.#physicalRestarts += 1;
    this.#restartPending = false;
  }

  async #timed<T>(operation: () => Promise<T>): Promise<T> {
    const started = performance.now();
    const result = await operation();
    this.#maxMaintenanceCallMs = Math.max(
      this.#maxMaintenanceCallMs,
      performance.now() - started,
    );
    return result;
  }

  async #snapshot(
    filesystem: EphemeralFS,
    adapter: FilesystemSQLiteDriver,
  ): Promise<{ peak: number; marks: number; objectCount: number }> {
    let result = await this.#timed(() =>
      filesystem.maintenance.snapshotStorage({ maxBatches: 8 }),
    );
    let peak = result.peakManagedResidentBytes;
    let marks = scalar(adapter, "SELECT count(*) AS value FROM efs_storage_marks");
    for (let call = 1; result.state !== "complete" && call < 20_000; call += 1) {
      result = await this.#timed(() =>
        filesystem.maintenance.snapshotStorage({ maxBatches: 8 }),
      );
      peak = Math.max(peak, result.peakManagedResidentBytes);
      marks = Math.max(
        marks,
        scalar(adapter, "SELECT count(*) AS value FROM efs_storage_marks"),
      );
    }
    invariant(result.state === "complete", "storage snapshot did not complete");
    return { peak, marks, objectCount: result.objectCount };
  }

  #pause(
    completedPhase:
      | "baseline-built"
      | "baseline-measured"
      | "full-built"
      | "full-measured"
      | "collection-paused",
  ): PortableScalePhaseOutcome {
    this.#phase = (this.#phase + 1) as 1 | 2 | 3 | 4 | 5;
    this.#restartPending = true;
    return Object.freeze({ status: "restart", completedPhase });
  }

  async run(adapter: FilesystemSQLiteDriver): Promise<PortableScalePhaseOutcome> {
    invariant(!this.#restartPending, "physical scale restart was not recorded");
    let filesystem: EphemeralFS | undefined;
    try {
      filesystem = await EphemeralFS.open({
        database: adapter,
        ownsDatabase: false,
        storage: STORAGE,
        runtime: RUNTIME,
      });
      if (this.#phase === 0) {
        const meta = adapter.transaction(
          "read",
          (tx) =>
            tx.all<MetaRow>(
              "SELECT root_inode,next_allocation_sequence FROM efs_meta WHERE singleton=1",
              [],
              { maxRows: 1, maxBytes: 256 },
            )[0],
        );
        invariant(meta !== undefined, "scale metadata row is missing");
        this.#rootInode = meta.root_inode;
        this.#sequence = meta.next_allocation_sequence;
        this.#initial = exactCounts(adapter);
        for (let start = 0; start < BASELINE_ROWS; start += GENERATION_BATCH) {
          const end = Math.min(BASELINE_ROWS, start + GENERATION_BATCH);
          const generated = await records(adapter, start, end);
          this.#fixtureDigest = await extendFixtureDigest(
            adapter,
            this.#fixtureDigest,
            generated,
          );
          insertRecords(adapter, this.#rootInode, this.#sequence, generated);
          this.#sequence += generated.length * 3;
          accountRecords(adapter, generated);
          adapter.transaction("write", (tx) => {
            tx.run("UPDATE efs_meta SET next_allocation_sequence=? WHERE singleton=1", [
              this.#sequence,
            ]);
          });
        }
        return this.#pause("baseline-built");
      }

      invariant(
        this.#rootInode !== undefined && this.#initial !== undefined,
        "scale session initialization is missing",
      );
      if (this.#phase === 1) {
        const baseline = await this.#snapshot(filesystem, adapter);
        this.#baselineManagedPeakBytes = baseline.peak;
        invariant(
          baseline.objectCount === this.#initial.objects + BASELINE_ROWS,
          "baseline snapshot object cardinality differs",
        );
        invariant(
          this.#baselineManagedPeakBytes > 0 &&
            this.#baselineManagedPeakBytes < 16 * 1024 * 1024,
          "baseline managed peak is outside the scale envelope",
        );
        return this.#pause("baseline-measured");
      }

      if (this.#phase === 2) {
        const persistedSequence = scalar(
          adapter,
          "SELECT next_allocation_sequence AS value FROM efs_meta WHERE singleton=1",
        );
        invariant(
          persistedSequence === this.#sequence,
          "allocation sequence changed across baseline restart",
        );
        for (let start = BASELINE_ROWS; start < SCALE_ROWS; start += GENERATION_BATCH) {
          const end = Math.min(SCALE_ROWS, start + GENERATION_BATCH);
          const generated = await records(adapter, start, end);
          this.#fixtureDigest = await extendFixtureDigest(
            adapter,
            this.#fixtureDigest,
            generated,
          );
          insertRecords(adapter, this.#rootInode, this.#sequence, generated);
          this.#sequence += generated.length * 3;
          accountRecords(adapter, generated);
          adapter.transaction("write", (tx) => {
            tx.run("UPDATE efs_meta SET next_allocation_sequence=? WHERE singleton=1", [
              this.#sequence,
            ]);
          });
        }
        return this.#pause("full-built");
      }

      if (this.#phase === 3) {
        invariant(
          (await filesystem.readFile("/scale-000000"))[0] === 0 &&
            new DataView((await filesystem.readFile("/scale-099999")).buffer).getUint32(
              0,
              true,
            ) === 99_999,
          "scale fixture bytes changed after physical reopen",
        );
        const full = await this.#snapshot(filesystem, adapter);
        this.#fullManagedPeakBytes = full.peak;
        this.#peakStorageMarks = full.marks;
        invariant(
          this.#fullManagedPeakBytes > 0 &&
            this.#fullManagedPeakBytes < 16 * 1024 * 1024,
          "full-scale managed peak is outside the scale envelope",
        );
        invariant(
          this.#fullManagedPeakBytes <= this.#baselineManagedPeakBytes + 512 * 1024,
          "managed high-water grew with total row count",
        );
        invariant(
          this.#peakStorageMarks >= SCALE_ROWS * 3,
          "storage marks omit reachable rows",
        );
        this.#exact = exactCounts(adapter);
        invariant(
          this.#exact.objects === this.#initial.objects + SCALE_ROWS,
          "object count differs",
        );
        invariant(
          this.#exact.nodes === this.#initial.nodes + SCALE_ROWS,
          "manifest-node count differs",
        );
        invariant(
          this.#exact.entries === this.#initial.entries + SCALE_ROWS,
          "namespace count differs",
        );
        invariant(
          full.objectCount === this.#exact.objects,
          "snapshot object count differs",
        );
        return this.#pause("full-measured");
      }

      if (this.#phase === 4) {
        let cursor: string | undefined;
        for (let call = 0; call < 20_000; call += 1) {
          const verification = await this.#timed(() =>
            filesystem!.maintenance.verify({
              maxEntities: 256,
              ...(cursor === undefined ? {} : { cursor }),
            }),
          );
          invariant(
            verification.checkedEntities <= 256,
            "verification batch is unbounded",
          );
          invariant(
            verification.peakManagedResidentBytes < 16 * 1024 * 1024,
            "verification exceeded the managed scale envelope",
          );
          this.#verifiedRows += verification.checkedEntities;
          cursor = verification.nextCursor ?? undefined;
          if (verification.complete) break;
        }
        invariant(
          cursor === undefined && this.#verifiedRows >= SCALE_ROWS * 4,
          "verification incomplete",
        );
        const collection = await this.#timed(() =>
          filesystem!.maintenance.collectGarbage({
            runId: "portable-scale-gc",
            maxBatches: 8,
          }),
        );
        invariant(collection.state === "paused", "scale collection did not pause");
        this.#peakGcMarks = scalar(
          adapter,
          "SELECT count(*) AS value FROM efs_gc_marks WHERE run_id='portable-scale-gc'",
        );
        return this.#pause("collection-paused");
      }

      invariant(this.#exact !== undefined, "full scale cardinalities are missing");
      let collection = await this.#timed(() =>
        filesystem!.maintenance.collectGarbage({
          runId: "portable-scale-gc",
          maxBatches: 8,
        }),
      );
      for (let call = 1; collection.state !== "complete" && call < 20_000; call += 1) {
        collection = await this.#timed(() =>
          filesystem!.maintenance.collectGarbage({
            runId: "portable-scale-gc",
            maxBatches: 8,
          }),
        );
        this.#peakGcMarks = Math.max(
          this.#peakGcMarks,
          scalar(
            adapter,
            "SELECT count(*) AS value FROM efs_gc_marks WHERE run_id='portable-scale-gc'",
          ),
        );
      }
      invariant(collection.state === "complete", "scale collection did not resume");
      invariant(this.#peakGcMarks >= SCALE_ROWS * 3, "GC marks omit reachable rows");
      invariant(
        (await filesystem.readFile("/scale-099999")).byteLength === 4,
        "collection swept reachable scale content",
      );
      invariant(
        this.#maxMaintenanceCallMs < 5_000,
        "one maintenance call exceeded five seconds",
      );
      invariant(
        this.#physicalRestarts === 5,
        "scale gate did not perform five physical restarts",
      );
      const mainFileBytes = adapter.physicalStorage?.().mainFileBytes;
      invariant(
        mainFileBytes !== undefined && mainFileBytes > 0,
        "physical size is missing",
      );
      invariant(
        mainFileBytes <= adapter.capabilities.maxPhysicalDatabaseBytes,
        "physical database exceeds adapter capability",
      );
      return Object.freeze({
        status: "complete",
        result: Object.freeze({
          schema: "efs-portable-scale-result-v1",
          adapter: this.#adapterName,
          seed: PORTABLE_SCALE_SEED,
          fixtureDigest: hex(this.#fixtureDigest),
          rows: SCALE_ROWS,
          baselineRows: BASELINE_ROWS,
          objectRows: this.#exact.objects,
          namespaceRows: this.#exact.entries,
          manifestRootRows: this.#exact.roots,
          manifestNodeRows: this.#exact.nodes,
          baselineManagedPeakBytes: this.#baselineManagedPeakBytes,
          fullManagedPeakBytes: this.#fullManagedPeakBytes,
          peakStorageMarks: this.#peakStorageMarks,
          peakGcMarks: this.#peakGcMarks,
          verifiedRows: this.#verifiedRows,
          maxMaintenanceCallMs: this.#maxMaintenanceCallMs,
          mainFileBytes,
          physicalRestarts: this.#physicalRestarts,
        }),
      });
    } finally {
      try {
        await filesystem?.close();
      } catch {}
    }
  }
}

/** Shared 100,000-row cursor, restart, memory, and collection scale gate. */
export async function runScaleConformance(
  factory: ConformanceAdapterFactory,
): Promise<PortableScaleResult> {
  const fixture = await factory.create({
    label: "portable-scale",
    seed: PORTABLE_SCALE_SEED,
  });
  let adapter = fixture.adapter;
  let filesystem: EphemeralFS | undefined;
  let baselineManagedPeakBytes = 0;
  let fullManagedPeakBytes: number;
  let peakStorageMarks: number;
  let peakGcMarks: number;
  let verifiedRows = 0;
  let maxMaintenanceCallMs = 0;
  let fixtureDigest: Uint8Array = new Uint8Array(32);
  const timed = async <T>(operation: () => Promise<T>): Promise<T> => {
    const started = performance.now();
    const result = await operation();
    maxMaintenanceCallMs = Math.max(maxMaintenanceCallMs, performance.now() - started);
    return result;
  };
  const open = async (): Promise<void> => {
    filesystem = await EphemeralFS.open({
      database: adapter,
      ownsDatabase: false,
      storage: STORAGE,
      runtime: RUNTIME,
    });
  };
  const reopen = async (): Promise<void> => {
    await filesystem?.close();
    filesystem = undefined;
    adapter.close();
    adapter = await fixture.reopen({ physical: true });
    await open();
  };
  const snapshot = async (): Promise<{
    peak: number;
    marks: number;
    objectCount: number;
  }> => {
    invariant(filesystem !== undefined, "filesystem is not open");
    let result = await timed(() =>
      filesystem!.maintenance.snapshotStorage({ maxBatches: 8 }),
    );
    let peak = result.peakManagedResidentBytes;
    let marks = scalar(adapter, "SELECT count(*) AS value FROM efs_storage_marks");
    for (let call = 1; result.state !== "complete" && call < 20_000; call += 1) {
      result = await timed(() =>
        filesystem!.maintenance.snapshotStorage({ maxBatches: 8 }),
      );
      peak = Math.max(peak, result.peakManagedResidentBytes);
      marks = Math.max(
        marks,
        scalar(adapter, "SELECT count(*) AS value FROM efs_storage_marks"),
      );
    }
    invariant(result.state === "complete", "storage snapshot did not complete");
    return { peak, marks, objectCount: result.objectCount };
  };
  try {
    await open();
    const meta = adapter.transaction(
      "read",
      (tx) =>
        tx.all<MetaRow>(
          "SELECT root_inode,next_allocation_sequence FROM efs_meta WHERE singleton=1",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    );
    invariant(meta !== undefined, "scale metadata row is missing");
    let sequence = meta.next_allocation_sequence;
    const initial = exactCounts(adapter);
    for (let start = 0; start < SCALE_ROWS; start += GENERATION_BATCH) {
      const end = Math.min(SCALE_ROWS, start + GENERATION_BATCH);
      const generated = await records(adapter, start, end);
      fixtureDigest = await extendFixtureDigest(adapter, fixtureDigest, generated);
      insertRecords(adapter, meta.root_inode, sequence, generated);
      sequence += generated.length * 3;
      accountRecords(adapter, generated);
      adapter.transaction("write", (tx) => {
        tx.run("UPDATE efs_meta SET next_allocation_sequence=? WHERE singleton=1", [
          sequence,
        ]);
      });
      if (end === BASELINE_ROWS) {
        await reopen();
        const baseline = await snapshot();
        baselineManagedPeakBytes = baseline.peak;
        invariant(
          baseline.objectCount === initial.objects + BASELINE_ROWS,
          "baseline snapshot object cardinality differs",
        );
        invariant(
          baselineManagedPeakBytes > 0 && baselineManagedPeakBytes < 16 * 1024 * 1024,
          "baseline managed peak is outside the scale envelope",
        );
        await reopen();
      }
    }

    await reopen();
    invariant(
      (await filesystem!.readFile("/scale-000000"))[0] === 0 &&
        new DataView((await filesystem!.readFile("/scale-099999")).buffer).getUint32(
          0,
          true,
        ) === 99_999,
      "scale fixture bytes changed after physical reopen",
    );
    const full = await snapshot();
    fullManagedPeakBytes = full.peak;
    peakStorageMarks = full.marks;
    invariant(
      fullManagedPeakBytes > 0 && fullManagedPeakBytes < 16 * 1024 * 1024,
      "full-scale managed peak is outside the scale envelope",
    );
    invariant(
      fullManagedPeakBytes <= baselineManagedPeakBytes + 512 * 1024,
      "managed high-water grew with total row count",
    );
    invariant(peakStorageMarks >= SCALE_ROWS * 3, "storage marks omit reachable rows");
    const exact = exactCounts(adapter);
    invariant(exact.objects === initial.objects + SCALE_ROWS, "object count differs");
    invariant(
      exact.nodes === initial.nodes + SCALE_ROWS,
      "manifest-node count differs",
    );
    invariant(
      exact.entries === initial.entries + SCALE_ROWS,
      "namespace count differs",
    );
    invariant(full.objectCount === exact.objects, "snapshot object count differs");

    let cursor: string | undefined;
    for (let call = 0; call < 20_000; call += 1) {
      const verification = await timed(() =>
        filesystem!.maintenance.verify({
          maxEntities: 256,
          ...(cursor === undefined ? {} : { cursor }),
        }),
      );
      invariant(verification.checkedEntities <= 256, "verification batch is unbounded");
      invariant(
        verification.peakManagedResidentBytes < 16 * 1024 * 1024,
        "verification exceeded the managed scale envelope",
      );
      verifiedRows += verification.checkedEntities;
      cursor = verification.nextCursor ?? undefined;
      if (verification.complete) break;
    }
    invariant(
      cursor === undefined && verifiedRows >= SCALE_ROWS * 4,
      "verification incomplete",
    );

    let collection = await timed(() =>
      filesystem!.maintenance.collectGarbage({
        runId: "portable-scale-gc",
        maxBatches: 8,
      }),
    );
    invariant(collection.state === "paused", "scale collection did not pause");
    peakGcMarks = scalar(
      adapter,
      "SELECT count(*) AS value FROM efs_gc_marks WHERE run_id='portable-scale-gc'",
    );
    await reopen();
    for (let call = 1; collection.state !== "complete" && call < 20_000; call += 1) {
      collection = await timed(() =>
        filesystem!.maintenance.collectGarbage({
          runId: "portable-scale-gc",
          maxBatches: 8,
        }),
      );
      peakGcMarks = Math.max(
        peakGcMarks,
        scalar(
          adapter,
          "SELECT count(*) AS value FROM efs_gc_marks WHERE run_id='portable-scale-gc'",
        ),
      );
    }
    invariant(collection.state === "complete", "scale collection did not resume");
    invariant(peakGcMarks >= SCALE_ROWS * 3, "GC marks omit reachable rows");
    invariant(
      (await filesystem!.readFile("/scale-099999")).byteLength === 4,
      "collection swept reachable scale content",
    );
    invariant(
      maxMaintenanceCallMs < 5_000,
      "one maintenance call exceeded five seconds",
    );
    const mainFileBytes = adapter.physicalStorage?.().mainFileBytes;
    invariant(
      mainFileBytes !== undefined && mainFileBytes > 0,
      "physical size is missing",
    );
    invariant(
      mainFileBytes <= adapter.capabilities.maxPhysicalDatabaseBytes,
      "physical database exceeds adapter capability",
    );
    return Object.freeze({
      schema: "efs-portable-scale-result-v1",
      adapter: factory.name,
      seed: PORTABLE_SCALE_SEED,
      fixtureDigest: hex(fixtureDigest),
      rows: SCALE_ROWS,
      baselineRows: BASELINE_ROWS,
      objectRows: exact.objects,
      namespaceRows: exact.entries,
      manifestRootRows: exact.roots,
      manifestNodeRows: exact.nodes,
      baselineManagedPeakBytes,
      fullManagedPeakBytes,
      peakStorageMarks,
      peakGcMarks,
      verifiedRows,
      maxMaintenanceCallMs,
      mainFileBytes,
    });
  } finally {
    try {
      await filesystem?.close();
    } catch {}
    try {
      adapter.close();
    } catch {}
    await fixture.dispose();
  }
}
