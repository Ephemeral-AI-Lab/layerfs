import { existsSync, statSync } from "node:fs";
import { DatabaseSync, type SQLOutputValue, type StatementSync } from "node:sqlite";
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
const MAX_SQL_TEXT_BYTES = 64 * 1024;
const MAX_CACHED_STATEMENTS = 256;

interface CachedStatement {
  readonly statement: StatementSync;
  readonly verdicts: {
    read?: Error | null;
    write?: Error | null;
    exclusive?: Error | null;
  };
}

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

function intrinsicByteLength(value: Uint8Array): number {
  try {
    return Reflect.apply(typedArrayByteLength, value, []) as number;
  } catch {
    throw new TypeError("SQLite BLOB values must be Uint8Array instances");
  }
}

function utf8ByteLength(value: string): number {
  let bytes = 0;
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code < 0x80) bytes += 1;
    else if (code < 0x800) bytes += 2;
    else if (code >= 0xd800 && code <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (next >= 0xdc00 && next <= 0xdfff) {
        bytes += 4;
        index += 1;
      } else bytes += 3;
    } else bytes += 3;
  }
  return bytes;
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
  if (typeof value === "string" && utf8ByteLength(value) > capabilities.maxBlobBytes)
    throw new RangeError("SQLite TEXT exceeds adapter limit");
  if (value instanceof Uint8Array) {
    const byteLength = intrinsicByteLength(value);
    if (byteLength > capabilities.maxBlobBytes)
      throw new RangeError("SQLite BLOB exceeds adapter limit");
    // node:sqlite consumes bindings synchronously. The intrinsic view strips
    // subclass overrides without allocating an untracked duplicate.
    // node:sqlite 24 treats a reconstructed zero-length typed-array view as
    // NULL. The original Uint8Array still has trustworthy intrinsic slots and
    // is synchronously consumed, so preserve it for the empty-BLOB case.
    return byteLength === 0 ? value : intrinsicBytes(value);
  }
  return value;
}

function output(
  value: SQLOutputValue,
  capabilities: SQLiteDriverCapabilities,
): SqliteValue {
  if (typeof value === "bigint") {
    if (
      value < BigInt(Number.MIN_SAFE_INTEGER) ||
      value > BigInt(Number.MAX_SAFE_INTEGER)
    )
      throw new RangeError("SQLite returned an unsafe integer");
    return Number(value);
  }
  if (value instanceof Uint8Array) {
    if (intrinsicByteLength(value) > capabilities.maxBlobBytes)
      throw new RangeError("SQLite result BLOB exceeds adapter limit");
    return ownBytes(value);
  }
  if (typeof value === "string" && utf8ByteLength(value) > capabilities.maxBlobBytes)
    throw new RangeError("SQLite result TEXT exceeds adapter limit");
  return value;
}

function sqliteOutputRowBytes(row: Record<string, SQLOutputValue>): number {
  let bytes = 32;
  for (const [name, value] of Object.entries(row))
    bytes +=
      name.length * 2 +
      (value instanceof Uint8Array
        ? intrinsicByteLength(value)
        : typeof value === "string"
          ? utf8ByteLength(value)
          : 8);
  return bytes;
}

function bindingBytes(bindings: SqliteBindings): number {
  let bytes = 0;
  for (const value of bindings)
    bytes +=
      value instanceof Uint8Array
        ? intrinsicByteLength(value)
        : typeof value === "string"
          ? utf8ByteLength(value)
          : 8;
  return bytes;
}

function bindingValueBytes(value: SqliteValue): number {
  return value instanceof Uint8Array
    ? intrinsicByteLength(value)
    : typeof value === "string"
      ? utf8ByteLength(value)
      : 8;
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

interface SqlHeaderToken {
  readonly kind: "identifier" | "symbol" | "other";
  readonly value: string;
}

function sqlHeaderTokens(sql: string, maxTokens = 16): readonly SqlHeaderToken[] {
  const tokens: SqlHeaderToken[] = [];
  let offset = 0;
  while (offset < sql.length && tokens.length < maxTokens) {
    const character = sql[offset]!;
    if (/\s/u.test(character)) {
      offset += 1;
      continue;
    }
    if (sql.startsWith("--", offset)) {
      const end = sql.indexOf("\n", offset + 2);
      offset = end < 0 ? sql.length : end + 1;
      continue;
    }
    if (sql.startsWith("/*", offset)) {
      const end = sql.indexOf("*/", offset + 2);
      if (end < 0) throw new TypeError("unterminated SQL comment");
      offset = end + 2;
      continue;
    }
    if (character === "." || character === ";") {
      tokens.push({ kind: "symbol", value: character });
      offset += 1;
      continue;
    }
    if (character === '"' || character === "`" || character === "[") {
      const close = character === "[" ? "]" : character;
      let value = "";
      offset += 1;
      let closed = false;
      while (offset < sql.length) {
        if (sql[offset] === close) {
          if (sql[offset + 1] === close) {
            value += close;
            offset += 2;
            continue;
          }
          offset += 1;
          closed = true;
          break;
        }
        value += sql[offset]!;
        offset += 1;
      }
      if (!closed) throw new TypeError("unterminated SQL identifier");
      tokens.push({ kind: "identifier", value: value.toUpperCase() });
      continue;
    }
    const identifier = /^[A-Za-z_][A-Za-z0-9_$]*/u.exec(sql.slice(offset));
    if (identifier) {
      tokens.push({ kind: "identifier", value: identifier[0].toUpperCase() });
      offset += identifier[0].length;
      continue;
    }
    if (character === "'") {
      offset += 1;
      while (offset < sql.length) {
        if (sql[offset] === "'") {
          if (sql[offset + 1] === "'") {
            offset += 2;
            continue;
          }
          offset += 1;
          break;
        }
        offset += 1;
      }
      tokens.push({ kind: "other", value: "STRING" });
      continue;
    }
    tokens.push({ kind: "other", value: character });
    offset += 1;
  }
  return tokens;
}

function assertDurableSchemaStatement(sql: string, keyword: string): void {
  if (keyword !== "CREATE" && keyword !== "DROP" && keyword !== "ALTER") return;
  const tokens = sqlHeaderTokens(sql);
  const value = (index: number): string | undefined => tokens[index]?.value;
  let index = 1;
  if (keyword === "CREATE") {
    if (value(index) === "TEMP" || value(index) === "TEMPORARY")
      throw new Error("temporary and virtual schemas are outside the storage contract");
    if (value(index) === "UNIQUE") index += 1;
    if (value(index) === "VIRTUAL")
      throw new Error("temporary and virtual schemas are outside the storage contract");
  }
  const objectKind = value(index);
  if (
    objectKind !== "TABLE" &&
    objectKind !== "INDEX" &&
    objectKind !== "TRIGGER" &&
    objectKind !== "VIEW"
  )
    throw new Error("SQLite schema statement is outside the storage contract");
  index += 1;
  if (value(index) === "IF") {
    index += 1;
    if (keyword === "CREATE" && value(index) === "NOT") index += 1;
    if (value(index) !== "EXISTS")
      throw new Error("SQLite schema statement is outside the storage contract");
    index += 1;
  }
  const objectName = tokens[index];
  if (!objectName || objectName.kind !== "identifier")
    throw new Error("SQLite schema statement is outside the storage contract");
  if (objectName.value === "TEMP" || value(index + 1) === ".")
    throw new Error("temporary and qualified schemas are outside the storage contract");
  if (keyword === "CREATE" && (objectKind === "INDEX" || objectKind === "TRIGGER")) {
    const on = tokens.findIndex(
      (token, tokenIndex) => tokenIndex > index && token.value === "ON",
    );
    if (
      on < 0 ||
      tokens[on + 1]?.kind !== "identifier" ||
      tokens[on + 1]?.value === "TEMP" ||
      tokens[on + 2]?.value === "."
    )
      throw new Error(
        "temporary and qualified schemas are outside the storage contract",
      );
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

function assertResultSql(sql: string): void {
  const keyword = leadingSqlKeyword(sql);
  if (
    keyword !== "SELECT" &&
    keyword !== "VALUES" &&
    keyword !== "WITH" &&
    keyword !== "EXPLAIN" &&
    !/^\s*PRAGMA\s+temp_store\s*;?\s*$/iu.test(sql)
  )
    throw new Error("EROFS: SQLite result statements must be read-only");
}

function assertNonResultSql(sql: string): void {
  const keyword = leadingSqlKeyword(sql);
  if (
    keyword === "SELECT" ||
    keyword === "VALUES" ||
    keyword === "WITH" ||
    keyword === "EXPLAIN" ||
    sqlHeaderTokens(sql, sql.length).some((token) => token.value === "RETURNING")
  )
    throw new Error("SQLite result statements require a bounded all() query");
}

function assertBoundedExpressionSql(sql: string): void {
  const tokens = sqlHeaderTokens(sql, sql.length);
  if (tokens.some((token) => token.value === "WITH"))
    throw new Error("SQLite common-table expressions are outside the bounded contract");
  const expandingFunctions = new Set([
    "ZEROBLOB",
    "RANDOMBLOB",
    "PRINTF",
    "FORMAT",
    "REPLACE",
    "GROUP_CONCAT",
    "STRING_AGG",
    "JSON_GROUP_ARRAY",
    "JSON_GROUP_OBJECT",
    "HEX",
    "QUOTE",
    "CONCAT",
    "CONCAT_WS",
  ]);
  if (tokens.some((token) => expandingFunctions.has(token.value)))
    throw new Error("SQLite expanding expressions are outside the bounded contract");
  const concatenates = tokens.some(
    (token, index) => token.value === "|" && tokens[index + 1]?.value === "|",
  );
  const usageIntegrityConcat =
    /^UPDATE efs_usage SET integrity_token=CAST\([a-z_]+ AS TEXT\)(?:\|\|':'\|\|CAST\([a-z_]+ AS TEXT\))*(?: WHERE singleton=1)?;?$/iu.test(
      sql.trim(),
    );
  if (concatenates && !usageIntegrityConcat)
    throw new Error("SQLite concatenation is outside the bounded result contract");
  const leading = tokens[0]?.value;
  if (
    (leading === "INSERT" || leading === "REPLACE") &&
    tokens.some((token) => token.value === "SELECT")
  )
    throw new Error(
      "SQLite write-from-query statements are outside the bounded contract",
    );
  if (
    leading === "CREATE" &&
    tokens.some(
      (token, index) => token.value === "AS" && tokens[index + 1]?.value === "SELECT",
    )
  )
    throw new Error(
      "SQLite write-from-query statements are outside the bounded contract",
    );
  if (leading === "CREATE" && tokens.some((token) => token.value === "TRIGGER")) {
    const begin = tokens.findIndex((token) => token.value === "BEGIN");
    for (let index = begin + 1; index > 0 && index < tokens.length; index += 1) {
      if (tokens[index]?.value !== "INSERT" && tokens[index]?.value !== "REPLACE")
        continue;
      const end = tokens.findIndex(
        (token, tokenIndex) => tokenIndex > index && token.value === ";",
      );
      const statementEnd = end < 0 ? tokens.length : end;
      if (
        tokens.slice(index + 1, statementEnd).some((token) => token.value === "SELECT")
      )
        throw new Error(
          "SQLite trigger write-from-query statements are outside the bounded contract",
        );
      index = statementEnd;
    }
  }
}

export class NodeSQLiteDriver implements FilesystemSQLiteDriver {
  readonly kind = "sqlite" as const;
  readonly readOnly: boolean;
  readonly capabilities: SQLiteDriverCapabilities & {
    readonly journalQuotaPolicy: "checkpoint-backpressure";
    readonly journalSizeLimitIsHard: false;
  };
  readonly #database: DatabaseSync;
  readonly #filename: string;
  readonly #pageSize: number;
  readonly #maxJournalBytes: number;
  readonly #journalBackpressureBytes: number;
  readonly #statementCache = new Map<string, CachedStatement>();
  #totalChangesStatement!: StatementSync;
  #closed = false;
  #closeAttempted = false;
  #closeError: unknown;
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
    try {
      this.#database.exec("PRAGMA temp_store=FILE");
      const tempStore = Number(
        this.#database.prepare("PRAGMA temp_store").get()?.temp_store,
      );
      if (tempStore !== 1) throw new Error("SQLite failed to enforce temp_store=FILE");
      this.#database.exec(
        `PRAGMA cache_size=-${Math.max(1, Math.floor(cacheTargetBytes / 1024))}; PRAGMA mmap_size=${mmapLimitBytes};`,
      );
      const configuredCacheSize = Number(
        this.#database.prepare("PRAGMA cache_size").get()?.cache_size,
      );
      const mmapRow = this.#database.prepare("PRAGMA mmap_size").get();
      const configuredMmapSize =
        mmapRow?.mmap_size === undefined ? 0 : Number(mmapRow.mmap_size);
      if (
        configuredCacheSize !== -Math.max(1, Math.floor(cacheTargetBytes / 1024)) ||
        configuredMmapSize !== mmapLimitBytes
      )
        throw new Error("SQLite failed to enforce the configured memory profile");
      if (!this.readOnly) {
        this.#database.exec(
          `PRAGMA journal_mode=WAL; PRAGMA synchronous=${durability === "acknowledged" ? "FULL" : "NORMAL"}; PRAGMA journal_size_limit=${maxJournalBytes};`,
        );
      }
      this.#pageSize = Number(
        this.#database.prepare("PRAGMA page_size").get()?.page_size ?? 4096,
      );
      if (!Number.isSafeInteger(this.#pageSize) || this.#pageSize <= 0)
        throw new Error("SQLite returned an invalid page size");
      this.#totalChangesStatement = this.#database.prepare(
        "SELECT total_changes() value",
      );
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
        if (effectiveMaxPhysicalDatabaseBytes > maxPhysicalDatabaseBytes)
          throw new Error(
            "ENOSPC: existing SQLite database exceeds the requested physical profile",
          );
      } else if (options.filename !== ":memory:") {
        const currentMainBytes = statSync(options.filename).size;
        if (currentMainBytes > maxPhysicalDatabaseBytes)
          throw new Error(
            "ENOSPC: existing SQLite database exceeds the requested physical profile",
          );
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
    } catch (error) {
      try {
        this.#database.close();
      } catch {}
      throw error;
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
          const statement = this.#prepareValidated(sql, bindings, mode);
          assertNonResultSql(sql);
          const bindingEstimate =
            mode === "read" ? 0 : this.#pageSize * 4 + bindingBytes(bindings) * 2;
          if (journalEstimate + bindingEstimate > this.#maxJournalBytes)
            throw new Error(
              "ENOSPC: WAL backpressure exceeds the configured soft transaction estimate",
            );
          const beforeChanges = this.#totalChanges();
          const result = statement.run(
            ...bindings.map((value) => binding(value, this.capabilities)),
          );
          const totalChanges = this.#totalChanges() - beforeChanges;
          const changes = Number(result.changes);
          const rowid = Number(result.lastInsertRowid);
          if (
            !Number.isSafeInteger(totalChanges) ||
            totalChanges < 0 ||
            !Number.isSafeInteger(changes) ||
            changes < 0 ||
            !Number.isSafeInteger(rowid)
          )
            throw new RangeError("SQLite returned unsafe write counters");
          if (mode !== "read") {
            const changedPageEstimate = totalChanges * this.#pageSize * 4;
            journalEstimate += Math.max(bindingEstimate, changedPageEstimate);
            if (journalEstimate > this.#maxJournalBytes)
              throw new Error(
                "ENOSPC: WAL backpressure exceeds the configured soft change estimate",
              );
          }
          return { changes, totalChanges, lastInsertRowid: rowid };
        },
        all: <Row extends SqliteRow = SqliteRow>(
          sql: string,
          bindings: SqliteBindings,
          budget: QueryBudget,
        ): readonly Row[] => {
          if (!active) throw new Error("SQLite transaction value is no longer active");
          const statement = this.#prepareValidated(sql, bindings, mode);
          assertResultSql(sql);
          if (
            !Number.isSafeInteger(budget.maxRows) ||
            budget.maxRows <= 0 ||
            !Number.isSafeInteger(budget.maxBytes) ||
            budget.maxBytes <= 0
          )
            throw new RangeError("invalid query budget");
          if (bindings.some((value) => bindingValueBytes(value) > budget.maxBytes))
            throw new RangeError(
              "SQLite binding value exceeds the result materialization budget",
            );
          const result: Row[] = [];
          let bytes = 0;
          let resultQueryOnly = false;
          try {
            if (mode !== "read") {
              this.#database.exec("PRAGMA query_only=ON");
              resultQueryOnly = true;
            }
            const iterator = statement.iterate(
              ...bindings.map((value) => binding(value, this.capabilities)),
            );
            for (const raw of iterator) {
              if (result.length >= budget.maxRows)
                throw new RangeError("SQLite result row budget exceeded");
              const nextBytes = sqliteOutputRowBytes(raw);
              if (bytes + nextBytes > budget.maxBytes)
                throw new RangeError("SQLite result byte budget exceeded");
              bytes += nextBytes;
              const normalized = Object.fromEntries(
                Object.entries(raw).map(([name, value]) => [
                  name,
                  output(value, this.capabilities),
                ]),
              ) as Row;
              result.push(Object.freeze(normalized));
            }
          } finally {
            if (resultQueryOnly) this.#database.exec("PRAGMA query_only=OFF");
          }
          return Object.freeze(result);
        },
      });
      const result = callback(tx);
      if (
        result !== null &&
        (typeof result === "object" || typeof result === "function") &&
        "then" in result
      )
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
    if (this.#closeAttempted) {
      if (this.#closeError !== undefined) throw this.#closeError;
      return;
    }
    if (this.#transactionActive)
      throw new Error("cannot close SQLite during a transaction");
    this.#closeAttempted = true;
    this.#closed = true;
    try {
      this.#database.close();
    } catch (error) {
      this.#closeError = error;
      throw error;
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
  #prepareValidated(
    sql: string,
    bindings: SqliteBindings,
    mode: TransactionMode,
  ): StatementSync {
    if (!sql.trim() || sql.includes("\0")) throw new TypeError("invalid SQL statement");
    if (
      utf8ByteLength(sql) > Math.min(this.capabilities.maxBlobBytes, MAX_SQL_TEXT_BYTES)
    )
      throw new RangeError("SQLite SQL text exceeds the bounded statement limit");
    if (bindings.length > this.capabilities.maxBindings)
      throw new RangeError("SQLite binding limit exceeded");
    const entry = this.#statementCache.get(sql);
    if (entry === undefined) {
      this.#validateStatementShape(sql, mode);
      const statement = this.#database.prepare(sql);
      if (this.#statementCache.size >= MAX_CACHED_STATEMENTS)
        this.#statementCache.delete(this.#statementCache.keys().next().value as string);
      this.#statementCache.set(sql, {
        statement,
        verdicts: { [mode]: null },
      });
      return statement;
    }
    const verdict = entry.verdicts[mode];
    if (verdict === undefined) {
      try {
        this.#validateStatementShape(sql, mode);
        entry.verdicts[mode] = null;
      } catch (error) {
        entry.verdicts[mode] = error as Error;
        throw error;
      }
    } else if (verdict) {
      throw verdict;
    }
    return entry.statement;
  }
  #validateStatementShape(sql: string, mode: TransactionMode): void {
    if (mode === "read") assertReadOnlySql(sql);
    assertBoundedExpressionSql(sql);
    const keyword = leadingSqlKeyword(sql);
    if (
      mode !== "read" &&
      new Set([
        "BEGIN",
        "COMMIT",
        "END",
        "ROLLBACK",
        "SAVEPOINT",
        "RELEASE",
        "ATTACH",
        "DETACH",
        "VACUUM",
      ]).has(keyword)
    )
      throw new Error(
        "SQLite statement is outside the callback-scoped transaction contract",
      );
    if (mode !== "read") assertDurableSchemaStatement(sql, keyword);
    if (
      mode !== "read" &&
      keyword === "PRAGMA" &&
      !/^\s*PRAGMA\s+temp_store\s*;?\s*$/iu.test(sql) &&
      (mode !== "exclusive" ||
        !/^\s*PRAGMA\s+(?:application_id|user_version)\s*=\s*\d+\s*;?\s*$/iu.test(sql))
    )
      throw new Error("SQLite PRAGMA is outside the storage transaction contract");
    if (mode !== "read" && sql.length * 2 > this.#maxJournalBytes)
      throw new Error("ENOSPC: SQL text exceeds the configured WAL target");
  }
  #totalChanges(): number {
    const value = Number(this.#totalChangesStatement.get()?.value);
    if (!Number.isSafeInteger(value) || value < 0)
      throw new RangeError("SQLite returned an unsafe total-change counter");
    return value;
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
    try {
      if (this.#filename === ":memory:") return;
      const walBytes = this.#fileBytes(`${this.#filename}-wal`) ?? 0;
      if (walBytes < Math.floor(this.#journalBackpressureBytes / 2)) return;
      this.#checkpointInternal("passive");
    } catch {}
  }
}

export async function openNodeSqlite(
  options: OpenNodeSqliteOptions,
): Promise<NodeSQLiteDriver> {
  return new NodeSQLiteDriver(options);
}
