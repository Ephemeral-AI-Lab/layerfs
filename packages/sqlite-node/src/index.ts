import { existsSync } from "node:fs";
import { DatabaseSync, type SQLOutputValue } from "node:sqlite";
import type {
  FilesystemSQLiteDriver,
  FilesystemSQLiteTransaction,
  QueryBudget,
  SQLiteDriverCapabilities,
  SqliteBindings,
  SqliteRow,
  SqliteRunResult,
  SqliteValue,
  TransactionMode,
} from "@ephemeralai/fs/sqlite-driver";

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

function binding(
  value: SqliteValue,
  capabilities: SQLiteDriverCapabilities,
): null | string | number | Uint8Array {
  if (typeof value === "number" && !Number.isSafeInteger(value))
    throw new RangeError("SQLite numbers must be safe integers");
  if (value instanceof Uint8Array) {
    if (value.byteLength > capabilities.maxBlobBytes)
      throw new RangeError("SQLite BLOB exceeds adapter limit");
    return value.slice();
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
  if (value instanceof Uint8Array) return value.slice();
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
    const cacheTargetBytes = options.cacheTargetBytes ?? 16 * 1024 * 1024;
    const mmapLimitBytes = options.mmapLimitBytes ?? 0;
    const maxPhysicalDatabaseBytes = options.maxPhysicalDatabaseBytes ?? 10 * 1024 ** 3;
    const maxJournalBytes = options.maxJournalBytes ?? 1024 ** 3;
    this.capabilities = Object.freeze({
      maxBlobBytes: 64 * 1024 * 1024,
      maxBindings: 32_766,
      durability: options.durability ?? "acknowledged",
      journalMode: this.readOnly ? "wal" : "wal",
      memoryPolicy: "configured",
      cacheTargetBytes,
      mmapLimitBytes,
      maxPhysicalDatabaseBytes,
      maxJournalBytes,
      physicalQuotaPolicy: "driver-enforced",
    });
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
        `PRAGMA journal_mode=WAL; PRAGMA synchronous=${this.capabilities.durability === "acknowledged" ? "FULL" : "NORMAL"}; PRAGMA cache_size=-${Math.max(1, Math.floor(cacheTargetBytes / 1024))}; PRAGMA mmap_size=${mmapLimitBytes}; PRAGMA journal_size_limit=${maxJournalBytes};`,
      );
      const pageSize = Number(
        this.#database.prepare("PRAGMA page_size").get()?.page_size ?? 4096,
      );
      this.#database.exec(
        `PRAGMA max_page_count=${Math.max(1, Math.floor(maxPhysicalDatabaseBytes / pageSize))}`,
      );
    }
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
    this.#transactionActive = true;
    let active = true;
    if (mode === "read") this.#database.exec("PRAGMA query_only=ON");
    this.#database.exec(
      mode === "read"
        ? "BEGIN DEFERRED"
        : mode === "write"
          ? "BEGIN IMMEDIATE"
          : "BEGIN EXCLUSIVE",
    );
    const tx: FilesystemSQLiteTransaction = Object.freeze({
      scope: Symbol("sqlite-transaction"),
      run: (sql: string, bindings: SqliteBindings = []): SqliteRunResult => {
        if (!active) throw new Error("SQLite transaction value is no longer active");
        this.#validateStatement(sql, bindings, mode);
        const result = this.#database
          .prepare(sql)
          .run(...bindings.map((value) => binding(value, this.capabilities)));
        const changes = Number(result.changes);
        const rowid = Number(result.lastInsertRowid);
        if (!Number.isSafeInteger(changes) || !Number.isSafeInteger(rowid))
          throw new RangeError("SQLite returned unsafe write counters");
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
    try {
      const result = callback(tx);
      if (result && typeof result === "object" && "then" in result)
        throw new TypeError("SQLite transaction callbacks must be synchronous");
      active = false;
      this.#database.exec("COMMIT");
      return result;
    } catch (error) {
      active = false;
      try {
        this.#database.exec("ROLLBACK");
      } catch {}
      throw error;
    } finally {
      if (mode === "read") {
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
}

export async function openNodeSqlite(
  options: OpenNodeSqliteOptions,
): Promise<NodeSQLiteDriver> {
  return new NodeSQLiteDriver(options);
}
