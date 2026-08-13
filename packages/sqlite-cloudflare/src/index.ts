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
  /** Conservative byte ceiling for the configured Durable Object plan. */
  readonly maxPhysicalDatabaseBytes?: number;
  readonly maxJournalBytes?: number;
}

const MAX_SQL_TEXT_BYTES = 64 * 1024;
const FREE_PLAN_DATABASE_BYTES = 1_000_000_000;
const PLATFORM_DATABASE_BYTES = 10_000_000_000;

export type CloudflareSQLiteErrorCategory =
  "constraint" | "busy" | "corruption" | "resource-limit";

export class CloudflareSQLiteError extends Error {
  readonly name = "CloudflareSQLiteError" as const;
  readonly category: CloudflareSQLiteErrorCategory;
  readonly code: string;
  constructor(
    category: CloudflareSQLiteErrorCategory,
    code: string,
    message: string,
    cause: unknown,
  ) {
    super(`${code}: ${message}`, { cause });
    this.category = category;
    this.code = code;
  }
}

function normalizeCloudflareSQLiteError(error: unknown): unknown {
  if (error instanceof CloudflareSQLiteError) return error;
  const message = error instanceof Error ? error.message : String(error);
  if (
    /SQLITE_FULL|database or disk is full|storage quota|quota exceeded/i.test(message)
  )
    return new CloudflareSQLiteError("resource-limit", "ENOSPC", message, error);
  if (/SQLITE_BUSY|SQLITE_LOCKED|\bbusy\b|\blocked\b/i.test(message))
    return new CloudflareSQLiteError("busy", "EBUSY", message, error);
  if (
    /SQLITE_CORRUPT|SQLITE_NOTADB|database disk image is malformed|not a database|\bcorrupt(?:ion|ed)?\b/i.test(
      message,
    )
  )
    return new CloudflareSQLiteError("corruption", "ECORRUPT", message, error);
  if (
    /SQLITE_CONSTRAINT|constraint failed|UNIQUE constraint|FOREIGN KEY constraint|NOT NULL constraint|CHECK constraint/i.test(
      message,
    )
  )
    return new CloudflareSQLiteError("constraint", "SQLITE_CONSTRAINT", message, error);
  if (
    /SQLITE_TOOBIG|too (?:big|large)|result exceeds|statement too long/i.test(message)
  )
    return new CloudflareSQLiteError("resource-limit", "EFBIG", message, error);
  return error;
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

function input(
  value: SqliteValue,
  capabilities: SQLiteDriverCapabilities,
): SqliteValue {
  if (typeof value === "number" && !Number.isSafeInteger(value))
    throw new RangeError("SQLite numbers must be safe integers");
  if (typeof value === "string" && utf8ByteLength(value) > capabilities.maxBlobBytes)
    throw new RangeError("SQLite TEXT exceeds Durable Object limit");
  if (value instanceof Uint8Array) {
    if (value.byteLength > capabilities.maxBlobBytes)
      throw new RangeError("SQLite BLOB exceeds Durable Object limit");
    return value.slice();
  }
  return value;
}
function output(value: unknown, capabilities: SQLiteDriverCapabilities): SqliteValue {
  if (value === null) return value;
  if (typeof value === "string") {
    if (utf8ByteLength(value) > capabilities.maxBlobBytes)
      throw new RangeError("Durable Object SQLite result TEXT exceeds adapter limit");
    return value;
  }
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value))
      throw new RangeError("Durable Object SQLite returned an unsafe integer");
    return value;
  }
  let bytes: Uint8Array | undefined;
  if (value instanceof Uint8Array) bytes = value.slice();
  else if (value instanceof ArrayBuffer) bytes = new Uint8Array(value.slice(0));
  else if (ArrayBuffer.isView(value))
    bytes = new Uint8Array(value.buffer, value.byteOffset, value.byteLength).slice();
  if (bytes) {
    if (bytes.byteLength > capabilities.maxBlobBytes)
      throw new RangeError("Durable Object SQLite result BLOB exceeds adapter limit");
    return bytes;
  }
  throw new TypeError("Durable Object SQLite returned an unsupported value");
}
function rawRowBytes(row: Record<string, unknown>): number {
  let bytes = 32;
  for (const [key, value] of Object.entries(row))
    bytes +=
      key.length * 2 +
      (value instanceof ArrayBuffer
        ? value.byteLength
        : ArrayBuffer.isView(value)
          ? value.byteLength
          : typeof value === "string"
            ? utf8ByteLength(value)
            : 8);
  return bytes;
}
function bindingValueBytes(value: SqliteValue): number {
  return value instanceof Uint8Array
    ? value.byteLength
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

function assertSingleStatementSql(sql: string): void {
  const tokens = sqlHeaderTokens(sql, sql.length);
  const create = tokens[0]?.value === "CREATE";
  const triggerIndex = tokens.findIndex(
    (token, index) => create && index <= 3 && token.value === "TRIGGER",
  );
  if (triggerIndex >= 0) {
    let bodyDepth = 0;
    let bodyStarted = false;
    for (let index = triggerIndex + 1; index < tokens.length; index += 1) {
      const value = tokens[index]!.value;
      if (!bodyStarted) {
        if (value === "BEGIN") {
          bodyStarted = true;
          bodyDepth = 1;
        }
        continue;
      }
      if (value === "BEGIN" || value === "CASE") bodyDepth += 1;
      else if (value === "END") {
        bodyDepth -= 1;
        if (bodyDepth === 0) {
          if (tokens.slice(index + 1).some((token) => token.value !== ";"))
            throw new Error("SQLite transaction accepts exactly one statement");
          return;
        }
      }
    }
    throw new TypeError("unterminated SQLite trigger statement");
  }
  const semicolon = tokens.findIndex((token) => token.value === ";");
  if (
    semicolon >= 0 &&
    tokens.slice(semicolon + 1).some((token) => token.value !== ";")
  )
    throw new Error("SQLite transaction accepts exactly one statement");
  if (
    semicolon >= 0 &&
    tokens.slice(semicolon + 1).some((token) => token.value === ";")
  )
    throw new Error("SQLite transaction accepts exactly one statement");
}

function executableSql(sql: string): string {
  let offset = 0;
  let significantEnd = 0;
  while (offset < sql.length) {
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
    if (
      character === "'" ||
      character === '"' ||
      character === "`" ||
      character === "["
    ) {
      const close = character === "[" ? "]" : character;
      offset += 1;
      let closed = false;
      while (offset < sql.length) {
        if (sql[offset] === close) {
          if (sql[offset + 1] === close) {
            offset += 2;
            continue;
          }
          offset += 1;
          closed = true;
          break;
        }
        offset += 1;
      }
      if (!closed) throw new TypeError("unterminated SQL quoted value");
      significantEnd = offset;
      continue;
    }
    offset += 1;
    significantEnd = offset;
  }
  const significant = sql.slice(0, significantEnd).trimEnd();
  return (significant.endsWith(";") ? significant.slice(0, -1) : significant).trimEnd();
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

function assertResultSql(sql: string): void {
  const keyword = leadingSqlKeyword(sql);
  if (
    keyword !== "SELECT" &&
    keyword !== "VALUES" &&
    keyword !== "WITH" &&
    keyword !== "EXPLAIN"
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

export class CloudflareSQLiteDriver implements FilesystemSQLiteDriver {
  readonly kind = "sqlite" as const;
  readonly readOnly = false;
  readonly capabilities: SQLiteDriverCapabilities;
  readonly #storage: DurableObjectSQLiteStorage;
  #closed = false;
  #active = false;
  constructor(options: OpenCloudflareSqliteOptions) {
    this.#storage = options.storage;
    const requestedPhysical =
      options.maxPhysicalDatabaseBytes ?? FREE_PLAN_DATABASE_BYTES;
    if (!Number.isSafeInteger(requestedPhysical) || requestedPhysical <= 0)
      throw new RangeError("maxPhysicalDatabaseBytes must be a positive safe integer");
    const maxPhysicalDatabaseBytes = Math.min(
      requestedPhysical,
      PLATFORM_DATABASE_BYTES,
    );
    if (
      options.maxJournalBytes !== undefined &&
      options.maxJournalBytes !== maxPhysicalDatabaseBytes
    )
      throw new RangeError(
        "Durable Object SQLite exposes no separately enforceable journal quota; maxJournalBytes must equal the runtime storage ceiling",
      );
    this.capabilities = Object.freeze({
      maxBlobBytes: 2 * 1024 * 1024,
      maxBindings: 100,
      durability: "acknowledged",
      journalMode: "runtime-managed",
      memoryPolicy: "runtime-managed",
      maxPhysicalDatabaseBytes,
      maxJournalBytes: maxPhysicalDatabaseBytes,
      physicalQuotaPolicy: "runtime-enforced",
      journalQuotaPolicy: "runtime-enforced",
      journalSizeLimitIsHard: false,
      schemaIdentityMode: "durable-table",
      pageMetricsMode: "runtime-size-only",
    });
  }
  /**
   * WebCrypto SHA-256 for the streaming write pipeline. Digest output is
   * byte-identical to the pure-JS fallback (`cas/sha256.ts`), so golden
   * vectors and workerd parity are unaffected.
   */
  readonly hashBytesAsync = async (bytes: Uint8Array): Promise<Uint8Array> => {
    // Engine-owned payloads are plain ArrayBuffer-backed Uint8Arrays; the
    // cast only widens the view type for WebCrypto's BufferSource contract.
    const digest = await globalThis.crypto.subtle.digest(
      "SHA-256",
      bytes as unknown as BufferSource,
    );
    return new Uint8Array(digest);
  };
  transaction<T>(
    mode: TransactionMode,
    callback: (tx: FilesystemSQLiteTransaction) => T,
  ): T {
    if (this.#closed) throw new Error("Durable Object SQLite driver is closed");
    if (this.#active) throw new Error("nested SQLite transactions are forbidden");
    this.#active = true;
    let active = true;
    let callbackFailed = false;
    let callbackError: unknown;
    try {
      return this.#storage.transactionSync(() => {
        const exec = <Row extends Record<string, unknown> = Record<string, unknown>>(
          query: string,
          ...bindings: unknown[]
        ): DurableObjectSqlCursor<Row> => {
          try {
            return this.#storage.sql.exec<Row>(query, ...bindings);
          } catch (error) {
            throw normalizeCloudflareSQLiteError(error);
          }
        };
        const tx: FilesystemSQLiteTransaction = Object.freeze({
          scope: Symbol("durable-object-sqlite-transaction"),
          run: (sql: string, bindings: SqliteBindings = []): SqliteRunResult => {
            if (!active)
              throw new Error("SQLite transaction value is no longer active");
            this.#validate(sql, bindings, mode);
            assertNonResultSql(sql);
            const statement = executableSql(sql);
            const cursor = exec(
              statement,
              ...bindings.map((value) => input(value, this.capabilities)),
            );
            cursor.toArray();
            if (!Number.isSafeInteger(cursor.rowsWritten) || cursor.rowsWritten < 0)
              throw new RangeError(
                "Durable Object SQLite returned unsafe write counters",
              );
            const changeRows = exec<{ value: unknown }>(
              "SELECT changes() AS value",
            ).toArray();
            const changes = changeRows[0]?.value;
            if (
              changeRows.length !== 1 ||
              typeof changes !== "number" ||
              !Number.isSafeInteger(changes) ||
              changes < 0
            )
              throw new RangeError(
                "Durable Object SQLite returned an unsafe direct-change counter",
              );
            return { changes, totalChanges: cursor.rowsWritten };
          },
          all: <Row extends SqliteRow = SqliteRow>(
            sql: string,
            bindings: SqliteBindings,
            budget: QueryBudget,
          ): readonly Row[] => {
            if (!active)
              throw new Error("SQLite transaction value is no longer active");
            this.#validate(sql, bindings, mode);
            assertResultSql(sql);
            const statement = executableSql(sql);
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
            const cursor = exec(
              statement,
              ...bindings.map((value) => input(value, this.capabilities)),
            );
            const rows: Row[] = [];
            let bytes = 0;
            for (const raw of cursor) {
              if (rows.length >= budget.maxRows)
                throw new RangeError("SQLite result row budget exceeded");
              const nextBytes = rawRowBytes(raw);
              if (bytes + nextBytes > budget.maxBytes)
                throw new RangeError(
                  `SQLite result byte budget exceeded (${bytes}+${nextBytes}>${budget.maxBytes}; ${Object.keys(raw).join(",")})`,
                );
              bytes += nextBytes;
              const row = Object.fromEntries(
                Object.entries(raw).map(([key, value]) => [
                  key,
                  output(value, this.capabilities),
                ]),
              ) as Row;
              rows.push(Object.freeze(row));
            }
            return Object.freeze(rows);
          },
        });
        try {
          const result = callback(tx);
          if (
            result !== null &&
            (typeof result === "object" || typeof result === "function") &&
            "then" in result
          )
            throw new TypeError("SQLite transaction callbacks must be synchronous");
          return result;
        } catch (error) {
          callbackFailed = true;
          callbackError = error;
          throw error;
        } finally {
          active = false;
        }
      });
    } catch (error) {
      if (callbackFailed) throw callbackError;
      throw normalizeCloudflareSQLiteError(error);
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
  physicalStorage(): { readonly mainFileBytes: number } {
    if (this.#closed) throw new Error("Durable Object SQLite driver is closed");
    const mainFileBytes = this.#storage.sql.databaseSize;
    if (!Number.isSafeInteger(mainFileBytes) || mainFileBytes < 0)
      throw new RangeError("Durable Object SQLite returned an unsafe database size");
    return Object.freeze({ mainFileBytes });
  }
  #validate(sql: string, bindings: SqliteBindings, mode: TransactionMode): void {
    if (!sql.trim() || sql.includes("\0")) throw new TypeError("invalid SQL statement");
    assertSingleStatementSql(sql);
    if (
      utf8ByteLength(sql) > Math.min(this.capabilities.maxBlobBytes, MAX_SQL_TEXT_BYTES)
    )
      throw new RangeError("Durable Object SQL statement limit exceeded");
    if (bindings.length > this.capabilities.maxBindings)
      throw new RangeError("Durable Object SQLite binding limit exceeded");
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
    if (mode !== "read" && keyword === "PRAGMA")
      throw new Error("SQLite PRAGMA is outside the Durable Object contract");
  }
}

export async function openCloudflareSqlite(
  options: OpenCloudflareSqliteOptions,
): Promise<CloudflareSQLiteDriver> {
  return new CloudflareSQLiteDriver(options);
}
