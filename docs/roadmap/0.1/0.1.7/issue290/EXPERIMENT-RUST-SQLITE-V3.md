# Rust SQLite standalone experiment v3

Status: Research; informative and not a product contract.
Owner request2026-10-01: run SQLite experiments through Rust and report numbers.
Parent dd6ce3bb653264296a3e4d7dabbb9d75f36c26a7. Tracking#291.

## Frozen scope and membership

Seventeen Rust selections; no new MinIO, V1/V2 resampling or product mutation.
Each ID is R-<originalID-without-version>-v3, mapping to the original schema,
row population, transaction count, queries and expected result:

- R-S-directory-10000-v3, R-S-tiny-1024-b1-v3,
  R-S-tiny-1024-b128-v3, R-S-big-index-4096-v3: V1 four metadata shapes.
- R-D-128-runtime-v3, R-D-128-full-v3,
  R-D-1024-runtime-v3, R-D-1024-full-v3: eight publications each, original V2 schema.
- R-C-metadata-1-v3, R-C-metadata-2-v3, R-C-metadata-4-v3:
  eight128-locator publications/client, one/two/four real Rust threads/connections,
  held application writer gate and observed failed nonblocking admission.
- R-C-branch-conflict-2-v3: same actual selected generation0, one success/one conflict,
  loser inserted rows rolled back. R-C-sqlite-busy-2-v3: real held write/INSERT,
  second native BEGIN yields BUSY without retry, accepted commit observed afterward.
- R-S-flat-10000-v3, R-S-flat-100000-v3, R-S-deep-10-v3, R-S-deep-270-v3:
  reuse qualified V2 closed masters via independent byte copy, same128 points or
  eight full paths, one128-row keyset page, one rename/visibility SELECT/delete.

## Implementation and measurement

External release example in layerfs-storage uses its existing pinned rusqlite
0.40.2/features and platform libsqlite3; no new dependency or product source.
SELECT sqlite_version()/sqlite_source_id(), compile options and otool dylib mapping
record actual engine, binary, source/config/lockfile hashes and command/wall.
Use repository ARMv8 flags, owned Cargo target and locked release build outside
measurement. Runtime profile MEMORY/OFF; synchronized profile DELETE/FULL with
macOSfullfsyncON. Both512KiB configured cache, mmap0, busy_timeout0, temp_storeFILE,
foreign_keysON,4KiB pages. Read back settings. No WAL experiment is requested here;
existing platform provider needs a fixed WAL-reset release before future WAL use.

Prepared statements are used and fixture keys/32-byte reference IDs are prepared
outside SQL phase timers. V1 IDs are independently generated SHA256 fixture bytes
once and passed to Rust, not derived from candidate rows. This differs from Python
V1's in-timer fixture hashing and may differ in statement preparation/provider.
Historical Python rows are context, never a matched pure-language speedup claim.
No SQL projection, workload population or transaction count is reduced.

Inner Rust Instant timing covers explicit SQL operations, prepared binding/row
consumption, transaction acknowledgements and declared thread/gate coordination.
Connection/profile/schema/master copy/key fixture preparation are setup and also
covered by complete performance child wall. Prepare schema-specific statements
before bulk loops; publication may cache statements through connection. Separate
mutation/COMMIT durations and gate waiting/ACK events are recorded. Query counts
follow executed statements, with EXPLAIN plans and exact result oracles.

Python only orchestrates source seals/deadlines, constructs independent fixtures,
and runs separate exact row proof/cleanup through the existing verifier. No Python
SQL runs in a timed phase. No extra per-operation process/shell/RPC/helper stack.
One producer per operation, prospectively1/2/4 clients for concurrency only.

## Bounds and evidence

One attempt per case at frozen coherent source; performance15s, independent proof
and cleanup10s. Fresh append-only output; retain red cells/timeouts without retry.
All numerical rows remain INELIGIBLE/performance_claim=false: OS/native cache
residency is unknown, setup/copy may warm pages, reads follow writes. No cold,
physical peak-memory, power-loss, full LayerFS Commit/Exec or release admission.

Independent expected rows/hashes, exact page/statement counts, generation parents,
conflict rollback, held-write BUSY and final cleanup follow V1/V2 proofs. Native
provider differences are recorded; no engine patch/fork/substitution to gain speed.
Use same17 Rust binary selections, separate JSON/CSV report plus safe receipts and
per-commit exact productionLOC. MinIO server stays stopped and prior evidence intact.
