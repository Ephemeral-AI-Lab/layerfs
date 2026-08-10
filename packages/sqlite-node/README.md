# @ephemeralai/fs-sqlite-node

Node.js SQLite adapter for Ephemeral AI FS, implementing the
[`@ephemeralai/fs` sqlite-driver contract](../../packages/fs/src/sqlite/driver.ts).

## Selected driver

This package uses Node's built-in `node:sqlite` module (`DatabaseSync`). It is available
unflagged from Node 22.13 and emits an experimental feature warning; the underlying
bundled SQLite version is reported by `SELECT sqlite_version()`. The adapter is tested
against the bundled SQLite of the supported Node line, currently 3.50.x on Node 24.

- Minimum supported Node.js version: 22.13 (`engines`).
- Minimum SQLite version: 3.45 (the minimum bundled by supported Node lines); the
  adapter relies on standard SQLite features only and does not require a network
  connection or optional extensions.

## Durability profile

`durability` defaults to `"acknowledged"`: WAL journal mode, `synchronous=FULL` (a
return from a write transaction means SQLite acknowledged that profile), foreign keys
enabled, a bounded 5 000 ms busy timeout, a 16 MiB page-cache target
(`cacheTargetBytes`), zero memory mapping (`mmapLimitBytes`), and `temp_store=FILE`. A
`"relaxed-test"` profile (WAL + `synchronous=NORMAL`) is available for tests and must
not be used by the Computer production factory.

The adapter reports its effective configuration and tested limits through
`driver.capabilities`:

- `maxBlobBytes`: largest bindable/returnable BLOB — 64 MiB hard ceiling, further
  bounded by the journal profile (`floor((maxJournalBytes - page overhead) / 2)`);
- `maxBindings`: 32 766 positional parameters per statement;
- `journalMode: "wal"`, `memoryPolicy: "configured"`;
- `maxPhysicalDatabaseBytes`: enforced with a finite `max_page_count` policy (main
  database ceiling);
- `maxJournalBytes`: a finite soft checkpoint target, reported with
  `journalQuotaPolicy: "checkpoint-backpressure"` and `journalSizeLimitIsHard: false` —
  one committing transaction may temporarily take the WAL above the target, and a
  blocked checkpoint backpressures the next writer.

## Statement contract

SQL is exposed only inside `transaction(mode, callback)` values, which become invalid as
soon as the callback returns. The adapter rejects connection-level SQL, nested
transaction control, common-table expressions, `RETURNING`, writable-result queries,
temporary or attached schemas, expanding functions, and unbounded result-producing
expressions; every multi-row statement must carry `maxRows` and `maxBytes` budgets and
is decoded incrementally. Bindings are positional `?` parameters; BLOBs are returned as
detached `Uint8Array` instances.
