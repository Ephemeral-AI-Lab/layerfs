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

export interface DurableObjectSqlCursor<
  Row extends Record<string, unknown> = Record<string, unknown>,
> extends Iterable<Row> {
  readonly rowsRead: number;
  readonly rowsWritten: number;
  toArray(): Row[];
}
export interface DurableObjectSqlStorage {
  exec<Row extends Record<string, unknown> = Record<string, unknown>>(
    query: string,
    ...bindings: unknown[]
  ): DurableObjectSqlCursor<Row>;
  readonly databaseSize: number;
}
export interface DurableObjectSQLiteStorage {
  readonly sql: DurableObjectSqlStorage;
  transactionSync<T>(callback: () => T): T;
}
export interface OpenCloudflareSqliteOptions {
  readonly storage: DurableObjectSQLiteStorage;
  readonly maxManagedPayloadBytes?: number;
  readonly maxJournalBytes?: number;
}

function input(
  value: SqliteValue,
  capabilities: SQLiteDriverCapabilities,
): SqliteValue {
  if (typeof value === "number" && !Number.isSafeInteger(value))
    throw new RangeError("SQLite numbers must be safe integers");
  if (value instanceof Uint8Array) {
    if (value.byteLength > capabilities.maxBlobBytes)
      throw new RangeError("SQLite BLOB exceeds Durable Object limit");
    return value.slice();
  }
  return value;
}
function output(value: unknown): SqliteValue {
  if (value === null || typeof value === "string") return value;
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value))
      throw new RangeError("Durable Object SQLite returned an unsafe integer");
    return value;
  }
  if (value instanceof Uint8Array) return value.slice();
  if (ArrayBuffer.isView(value))
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength).slice();
  throw new TypeError("Durable Object SQLite returned an unsupported value");
}
function rowBytes(row: SqliteRow): number {
  let bytes = 32;
  for (const [key, value] of Object.entries(row))
    bytes +=
      key.length * 2 +
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

export class CloudflareSQLiteDriver implements FilesystemSQLiteDriver {
  readonly kind = "sqlite" as const;
  readonly readOnly = false;
  readonly capabilities: SQLiteDriverCapabilities;
  readonly #storage: DurableObjectSQLiteStorage;
  #closed = false;
  #active = false;
  constructor(options: OpenCloudflareSqliteOptions) {
    this.#storage = options.storage;
    this.capabilities = Object.freeze({
      maxBlobBytes: 2 * 1024 * 1024,
      maxBindings: 100,
      durability: "acknowledged",
      journalMode: "runtime-managed",
      memoryPolicy: "runtime-managed",
      maxPhysicalDatabaseBytes: Math.min(
        options.maxManagedPayloadBytes ?? 10 * 1024 ** 3,
        10 * 1024 ** 3,
      ),
      maxJournalBytes: options.maxJournalBytes ?? 256 * 1024 ** 2,
      physicalQuotaPolicy: "runtime-enforced",
      journalQuotaPolicy: "runtime-enforced",
      journalSizeLimitIsHard: false,
    });
  }
  transaction<T>(
    mode: TransactionMode,
    callback: (tx: FilesystemSQLiteTransaction) => T,
  ): T {
    if (this.#closed) throw new Error("Durable Object SQLite driver is closed");
    if (this.#active) throw new Error("nested SQLite transactions are forbidden");
    this.#active = true;
    let active = true;
    try {
      return this.#storage.transactionSync(() => {
        if (mode === "read") this.#storage.sql.exec("PRAGMA query_only=ON").toArray();
        const tx: FilesystemSQLiteTransaction = Object.freeze({
          scope: Symbol("durable-object-sqlite-transaction"),
          run: (sql: string, bindings: SqliteBindings = []): SqliteRunResult => {
            if (!active)
              throw new Error("SQLite transaction value is no longer active");
            this.#validate(sql, bindings, mode);
            const cursor = this.#storage.sql.exec(
              sql,
              ...bindings.map((value) => input(value, this.capabilities)),
            );
            cursor.toArray();
            return { changes: cursor.rowsWritten };
          },
          all: <Row extends SqliteRow = SqliteRow>(
            sql: string,
            bindings: SqliteBindings,
            budget: QueryBudget,
          ): readonly Row[] => {
            if (!active)
              throw new Error("SQLite transaction value is no longer active");
            this.#validate(sql, bindings, mode);
            const cursor = this.#storage.sql.exec(
              sql,
              ...bindings.map((value) => input(value, this.capabilities)),
            );
            const rows: Row[] = [];
            let bytes = 0;
            for (const raw of cursor) {
              if (rows.length >= budget.maxRows)
                throw new RangeError("SQLite result row budget exceeded");
              const row = Object.fromEntries(
                Object.entries(raw).map(([key, value]) => [key, output(value)]),
              ) as Row;
              bytes += rowBytes(row);
              if (bytes > budget.maxBytes)
                throw new RangeError("SQLite result byte budget exceeded");
              rows.push(Object.freeze(row));
            }
            return Object.freeze(rows);
          },
        });
        try {
          const result = callback(tx);
          if (result && typeof result === "object" && "then" in result)
            throw new TypeError("SQLite transaction callbacks must be synchronous");
          return result;
        } finally {
          active = false;
          if (mode === "read")
            this.#storage.sql.exec("PRAGMA query_only=OFF").toArray();
        }
      });
    } finally {
      active = false;
      this.#active = false;
    }
  }
  close(): void {
    this.#closed = true;
  }
  get databaseSize(): number {
    return this.#storage.sql.databaseSize;
  }
  #validate(sql: string, bindings: SqliteBindings, mode: TransactionMode): void {
    if (!sql.trim() || sql.length > 100 * 1024)
      throw new RangeError("Durable Object SQL statement limit exceeded");
    if (bindings.length > this.capabilities.maxBindings)
      throw new RangeError("Durable Object SQLite binding limit exceeded");
    if (mode === "read") assertReadOnlySql(sql);
  }
}

export async function openCloudflareSqlite(
  options: OpenCloudflareSqliteOptions,
): Promise<CloudflareSQLiteDriver> {
  return new CloudflareSQLiteDriver(options);
}
