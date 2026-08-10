import { existsSync, statSync } from "node:fs";
import { DatabaseSync, type SQLOutputValue } from "node:sqlite";
import type {
  FilesystemSQLiteDriver,
  FilesystemSQLiteTransaction,
  QueryBudget,
  SQLiteDriverCapabilities,
  SQLiteCheckpointResult,
  SQLitePhysicalStorage,
  SqliteBindings,
  SqliteRow,
  SqliteRunResult,
  SqliteValue,
  TransactionMode,
} from "@ephemeralai/fs/sqlite-driver";

const TYPED_ARRAY_PROTOTYPE = Object.getPrototypeOf(Uint8Array.prototype) as object;
const typedArrayBuffer = Object.getOwnPropertyDescriptor(
  TYPED_ARRAY_PROTOTYPE,
  "buffer",
)!.get!;
const typedArrayByteOffset = Object.getOwnPropertyDescriptor(
  TYPED_ARRAY_PROTOTYPE,
  "byteOffset",
)!.get!;
const typedArrayByteLength = Object.getOwnPropertyDescriptor(
  TYPED_ARRAY_PROTOTYPE,
  "byteLength",
)!.get!;

export interface OpenNodeSqliteOptions {
  readonly filename: string;
  readonly readOnly?: boolean;
  readonly create?: boolean;
  readonly busyTimeoutMs?: number;
  readonly durability?: "acknowledged" | "relaxed-test";
  readonly cacheTargetBytes?: number;
  readonly mmapLimitBytes?: number;
  readonly maxPhysicalDatabaseBytes?: number;
  readonly maxJournalBytes?: number;
}

function intrinsicBytes(value: Uint8Array): Uint8Array {
  try {
    const buffer = Reflect.apply(typedArrayBuffer, value, []) as ArrayBufferLike;
    const byteOffset = Reflect.apply(typedArrayByteOffset, value, []) as number;
    const byteLength = Reflect.apply(typedArrayByteLength, value, []) as number;
    return new Uint8Array(buffer, byteOffset, byteLength);
  } catch {
    throw new TypeError("SQLite BLOB values must be Uint8Array instances");
  }
}

function ownBytes(value: Uint8Array): Uint8Array {
  const source = intrinsicBytes(value);
  const owned = new Uint8Array(source.byteLength);
  owned.set(source);
  return owned;
}

function binding(
  value: SqliteValue,
  capabilities: SQLiteDriverCapabilities,
): null | string | number | Uint8Array {
  if (typeof value === "number" && !Number.isSafeInteger(value))
    throw new RangeError("SQLite numbers must be safe integers");
  if (value instanceof Uint8Array) {
    const bytes = intrinsicBytes(value);
    if (bytes.byteLength > capabilities.maxBlobBytes)
      throw new RangeError("SQLite BLOB exceeds adapter limit");
    return ownBytes(bytes);
  }
  return value;
}

function output(value: SQLOutputValue): SqliteValue {
  if (typeof value === "bigint") {
    if (
      value < BigInt(Number.MIN_SAFE_INTEGER) ||
      value > BigInt(Number.MAX_SAFE_INTEGER)
    )
      throw new RangeError("SQLite returned an unsafe integer");
    return Number(value);
  }
  if (value instanceof Uint8Array) return ownBytes(value);
  return value;
}

function rowBytes(row: SqliteRow): number {
  let bytes = 32;
  for (const [name, value] of Object.entries(row))
    bytes +=
      name.length * 2 +
      (value instanceof Uint8Array
        ? value.byteLength
        : typeof value === "string"
          ? value.length * 2
          : 8);
  return bytes;
}

function bindingBytes(bindings: SqliteBindings): number {
  let bytes = 0;
  for (const value of bindings)
    bytes +=
      value instanceof Uint8Array
        ? intrinsicBytes(value).byteLength
        : typeof value === "string"
          ? value.length * 2
          : 8;
  return bytes;
}

function leadingSqlKeyword(sql: string): string {
  let source = sql;
  while (true) {
    source = source.trimStart();
    if (source.startsWith("--")) {
      const end = source.indexOf("\n");
      source = end < 0 ? "" : source.slice(end + 1);
      continue;
    }
    if (source.startsWith("/*")) {
      const end = source.indexOf("*/", 2);
      if (end < 0) throw new TypeError("unterminated SQL comment");
      source = source.slice(end + 2);
      continue;
    }
    return /^[A-Za-z]+/u.exec(source)?.[0]?.toUpperCase() ?? "";
  }
}

function assertReadOnlySql(sql: string): void {
  const keyword = leadingSqlKeyword(sql);
  if (
    keyword !== "SELECT" &&
    keyword !== "VALUES" &&
    keyword !== "WITH" &&
    keyword !== "EXPLAIN"
  )
    throw new Error("EROFS: read transaction accepts only read-only SQL");
}

export class NodeSQLiteDriver implements FilesystemSQLiteDriver {
  readonly kind = "sqlite" as const;
  readonly readOnly: boolean;
  readonly capabilities: SQLiteDriverCapabilities;
  readonly #database: DatabaseSync;
  readonly #filename: string;
  readonly #pageSize: number;
  readonly #maxJournalBytes: number;
  readonly #journalBackpressureBytes: number;
  #closed = false;
  #transactionActive = false;
  constructor(options: OpenNodeSqliteOptions) {
    if (!options.filename) throw new TypeError("filename is required");
    if (
      !options.readOnly &&
      options.create === false &&
      options.filename !== ":memory:" &&
      !existsSync(options.filename)
    )
      throw new Error("SQLite database does not exist and create is false");
    this.readOnly = options.readOnly ?? false;
    this.#filename = options.filename;
    const cacheTargetBytes = options.cacheTargetBytes ?? 16 * 1024 * 1024;
    const mmapLimitBytes = options.mmapLimitBytes ?? 0;
    const maxPhysicalDatabaseBytes = options.maxPhysicalDatabaseBytes ?? 10 * 1024 ** 3;
    const maxJournalBytes = options.maxJournalBytes ?? 1024 ** 3;
    const durability = options.durability ?? "acknowledged";
    for (const [name, value, allowZero] of [
      ["cacheTargetBytes", cacheTargetBytes, false],
      ["mmapLimitBytes", mmapLimitBytes, true],
      ["maxPhysicalDatabaseBytes", maxPhysicalDatabaseBytes, false],
      ["maxJournalBytes", maxJournalBytes, false],
    ] as const)
      if (!Number.isSafeInteger(value) || value < (allowZero ? 0 : 1))
        throw new RangeError(
          `${name} must be a ${allowZero ? "nonnegative" : "positive"} safe integer`,
        );
    this.#database = new DatabaseSync(options.filename, {
      readOnly: this.readOnly,
      timeout: options.busyTimeoutMs ?? 5_000,
      readBigInts: true,
      enableForeignKeyConstraints: true,
      enableDoubleQuotedStringLiterals: false,
      allowExtension: false,
    });
    if (!this.readOnly) {
      this.#database.exec(
        `PRAGMA journal_mode=WAL; PRAGMA synchronous=${durability === "acknowledged" ? "FULL" : "NORMAL"}; PRAGMA cache_size=-${Math.max(1, Math.floor(cacheTargetBytes / 1024))}; PRAGMA mmap_size=${mmapLimitBytes}; PRAGMA journal_size_limit=${maxJournalBytes};`,
      );
    }
    this.#pageSize = Number(
      this.#database.prepare("PRAGMA page_size").get()?.page_size ?? 4096,
    );
    if (!Number.isSafeInteger(this.#pageSize) || this.#pageSize <= 0)
      throw new Error("SQLite returned an invalid page size");
    const minimumJournalBytes = this.#pageSize * 8 + 2 * 9248;
    if (maxJournalBytes < minimumJournalBytes)
      throw new RangeError(
        `maxJournalBytes must hold SQLite overhead and one canonical manifest node (${minimumJournalBytes} bytes)`,
      );
    this.#maxJournalBytes = maxJournalBytes;
    let effectiveMaxPhysicalDatabaseBytes = maxPhysicalDatabaseBytes;
    if (!this.readOnly) {
      const requestedPageCount = Math.max(
        1,
        Math.floor(maxPhysicalDatabaseBytes / this.#pageSize),
      );
      this.#database.exec(
        `PRAGMA max_page_count=${requestedPageCount}; PRAGMA wal_autocheckpoint=${Math.max(
          1,
          Math.floor(maxJournalBytes / (this.#pageSize + 24) / 2),
        )}`,
      );
      const effectivePageCount = Number(
        this.#database.prepare("PRAGMA max_page_count").get()?.max_page_count,
      );
      if (!Number.isSafeInteger(effectivePageCount) || effectivePageCount <= 0)
        throw new Error("SQLite returned an invalid max_page_count");
      effectiveMaxPhysicalDatabaseBytes = effectivePageCount * this.#pageSize;
    }
    this.#journalBackpressureBytes = Math.max(
      this.#pageSize * 4,
      Math.floor(maxJournalBytes * 0.75),
    );
    const rawJournalMode = String(
      this.#database.prepare("PRAGMA journal_mode").get()?.journal_mode ?? "",
    ).toLowerCase();
    this.capabilities = Object.freeze({
      maxBlobBytes: Math.min(
        64 * 1024 * 1024,
        Math.floor((maxJournalBytes - this.#pageSize * 8) / 2),
      ),
      maxBindings: 32_766,
      durability,
      journalMode: rawJournalMode === "wal" ? "wal" : "rollback",
      memoryPolicy: "configured",
      cacheTargetBytes,
      mmapLimitBytes,
      maxPhysicalDatabaseBytes: effectiveMaxPhysicalDatabaseBytes,
      maxJournalBytes,
      physicalQuotaPolicy: "driver-enforced",
      journalQuotaPolicy: "checkpoint-backpressure",
      journalSizeLimitIsHard: false,
    });
  }
  transaction<T>(
    mode: TransactionMode,
    callback: (tx: FilesystemSQLiteTransaction) => T,
  ): T {
    if (this.#closed) throw new Error("SQLite driver is closed");
    if (this.#transactionActive)
      throw new Error("nested SQLite transactions are forbidden");
    if (this.readOnly && mode !== "read")
      throw new Error("EROFS: write transaction requested on read-only adapter");
    if (mode !== "read") this.#enforceJournalBackpressure();
    this.#transactionActive = true;
    let active = true;
    let begun = false;
    let queryOnly = false;
    try {
      if (mode === "read") {
        this.#database.exec("PRAGMA query_only=ON");
        queryOnly = true;
      }
      this.#database.exec(
        mode === "read"
          ? "BEGIN DEFERRED"
          : mode === "write"
            ? "BEGIN IMMEDIATE"
            : "BEGIN EXCLUSIVE",
      );
      begun = true;
      let journalEstimate =
        mode === "read" || this.#filename === ":memory:"
          ? 0
          : (this.#fileBytes(`${this.#filename}-wal`) ?? 0);
      const tx: FilesystemSQLiteTransaction = Object.freeze({
        scope: Symbol("sqlite-transaction"),
        run: (sql: string, bindings: SqliteBindings = []): SqliteRunResult => {
          if (!active) throw new Error("SQLite transaction value is no longer active");
          this.#validateStatement(sql, bindings, mode);
          const bindingEstimate =
            mode === "read"
              ? 0
              : this.#pageSize * 4 + bindingBytes(bindings) * 2;
          if (journalEstimate + bindingEstimate > this.#maxJournalBytes)
            throw new Error(
              "ENOSPC: WAL backpressure exceeds rollback-safe transaction admission envelope",
            );
          const result = this.#database
            .prepare(sql)
            .run(...bindings.map((value) => binding(value, this.capabilities)));
          const changes = Number(result.changes);
          const rowid = Number(result.lastInsertRowid);
          if (!Number.isSafeInteger(changes) || !Number.isSafeInteger(rowid))
            throw new RangeError("SQLite returned unsafe write counters");
          if (mode !== "read") {
            const changedPageEstimate = changes * this.#pageSize * 4;
            journalEstimate += Math.max(bindingEstimate, changedPageEstimate);
            if (journalEstimate > this.#maxJournalBytes)
              throw new Error(
                "ENOSPC: WAL backpressure exceeds rollback-safe transaction change envelope",
              );
          }
          return { changes, lastInsertRowid: rowid };
        },
        all: <Row extends SqliteRow = SqliteRow>(
          sql: string,
          bindings: SqliteBindings,
          budget: QueryBudget,
        ): readonly Row[] => {
          if (!active) throw new Error("SQLite transaction value is no longer active");
          this.#validateStatement(sql, bindings, mode);
          if (
            !Number.isSafeInteger(budget.maxRows) ||
            budget.maxRows <= 0 ||
            !Number.isSafeInteger(budget.maxBytes) ||
            budget.maxBytes <= 0
          )
            throw new RangeError("invalid query budget");
          const result: Row[] = [];
          let bytes = 0;
          for (const raw of this.#database
            .prepare(sql)
            .iterate(...bindings.map((value) => binding(value, this.capabilities)))) {
            if (result.length >= budget.maxRows)
              throw new RangeError("SQLite result row budget exceeded");
            const normalized = Object.fromEntries(
              Object.entries(raw).map(([name, value]) => [name, output(value)]),
            ) as Row;
            bytes += rowBytes(normalized);
            if (bytes > budget.maxBytes)
              throw new RangeError("SQLite result byte budget exceeded");
            result.push(Object.freeze(normalized));
          }
          return Object.freeze(result);
        },
      });
      const result = callback(tx);
      if (result && typeof result === "object" && "then" in result)
        throw new TypeError("SQLite transaction callbacks must be synchronous");
      active = false;
      this.#database.exec("COMMIT");
      begun = false;
      if (mode !== "read") this.#checkpointAfterCommit();
      return result;
    } catch (error) {
      active = false;
      if (begun)
        try {
          this.#database.exec("ROLLBACK");
        } catch {}
      throw error;
    } finally {
      if (queryOnly) {
        try {
          this.#database.exec("PRAGMA query_only=OFF");
        } catch {}
      }
      this.#transactionActive = false;
    }
  }
  close(): void {
    if (!this.#closed) {
      if (this.#transactionActive)
        throw new Error("cannot close SQLite during a transaction");
      this.#closed = true;
      this.#database.close();
    }
  }
  physicalStorage(): SQLitePhysicalStorage {
    if (this.#closed) throw new Error("SQLite driver is closed");
    if (this.#filename === ":memory:") return Object.freeze({});
    return Object.freeze({
      mainFileBytes: this.#fileBytes(this.#filename) ?? 0,
      walBytes: this.#fileBytes(`${this.#filename}-wal`) ?? 0,
    });
  }
  checkpoint(
    mode: "passive" | "restart" | "truncate" = "passive",
  ): SQLiteCheckpointResult {
    if (this.#closed) throw new Error("SQLite driver is closed");
    if (this.#transactionActive)
      throw new Error("cannot checkpoint SQLite during a transaction");
    if (this.readOnly) throw new Error("EROFS: cannot checkpoint a read-only adapter");
    return this.#checkpointInternal(mode);
  }
  #validateStatement(
    sql: string,
    bindings: SqliteBindings,
    mode: TransactionMode,
  ): void {
    if (!sql.trim() || sql.includes("\0")) throw new TypeError("invalid SQL statement");
    if (bindings.length > this.capabilities.maxBindings)
      throw new RangeError("SQLite binding limit exceeded");
    if (mode === "read") assertReadOnlySql(sql);
  }
  #fileBytes(filename: string): number | undefined {
    try {
      return statSync(filename).size;
    } catch (error) {
      if (
        error &&
        typeof error === "object" &&
        "code" in error &&
        (error.code === "ENOENT" || error.code === "ENOTDIR")
      )
        return undefined;
      throw error;
    }
  }
  #checkpointInternal(
    mode: "passive" | "restart" | "truncate",
  ): SQLiteCheckpointResult {
    if (this.#filename === ":memory:")
      return Object.freeze({
        mode,
        busy: 0,
        logFrames: 0,
        checkpointedFrames: 0,
      });
    const pragma = mode.toUpperCase();
    const row = this.#database.prepare(`PRAGMA wal_checkpoint(${pragma})`).get();
    const busy = Number(row?.busy ?? 0);
    const logFrames = Number(row?.log ?? 0);
    const checkpointedFrames = Number(row?.checkpointed ?? 0);
    if (
      !Number.isSafeInteger(busy) ||
      !Number.isSafeInteger(logFrames) ||
      !Number.isSafeInteger(checkpointedFrames) ||
      busy < 0 ||
      logFrames < 0 ||
      checkpointedFrames < 0
    )
      throw new Error("SQLite returned invalid checkpoint counters");
    return Object.freeze({
      mode,
      busy,
      logFrames,
      checkpointedFrames,
      walBytes: this.#fileBytes(`${this.#filename}-wal`) ?? 0,
    });
  }
  #enforceJournalBackpressure(): void {
    if (this.#filename === ":memory:") return;
    const walBytes = this.#fileBytes(`${this.#filename}-wal`) ?? 0;
    if (walBytes < this.#journalBackpressureBytes) return;
    const checkpoint = this.#checkpointInternal("truncate");
    const remaining = checkpoint.walBytes ?? 0;
    if (checkpoint.busy !== 0 || remaining >= this.#journalBackpressureBytes)
      throw new Error("ENOSPC: WAL checkpoint backpressure threshold remains pinned");
  }
  #checkpointAfterCommit(): void {
    if (this.#filename === ":memory:") return;
    const walBytes = this.#fileBytes(`${this.#filename}-wal`) ?? 0;
    if (walBytes < Math.floor(this.#journalBackpressureBytes / 2)) return;
    try {
      this.#checkpointInternal("passive");
    } catch {}
  }
}

export async function openNodeSqlite(
  options: OpenNodeSqliteOptions,
): Promise<NodeSQLiteDriver> {
  return new NodeSQLiteDriver(options);
}
