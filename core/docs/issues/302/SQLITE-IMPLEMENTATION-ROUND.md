# Durable SQLite implementation round

> **Status:** Dated implementation checkpoint; benchmark admission incomplete.

The active workspace now contains the six domain/persistence/project/telemetry
packages. `layerfs-persistence` replaces `layerfs-metadata`; there is no parallel
PostgreSQL implementation, transport driver or S3 dependency. PostgreSQL selection
returns BackendUnavailable before touching a path. Excluded former runtime/S3
packages are retained reference source, not active fallback composition.

Objects, Metadata and History share one SQLite Store. The common provider,
publication validation and history operations are separate from engine SQL,
transaction, row, schema and BLOB mechanics. Schema1 has exactly12 logical
tables, application_id1279677264, page_size4096. Published pack rows cannot be
updated/deleted; final sealed bytes are inserted once. The bounded C2 builder
constructs unpublished packs in memory, so this path does not perform an
incremental database append or resurrect a private-save database catalogue.
Arc<Vec<u8>> shares sealed byte storage through publication without duplicating
whole bodies. SQLite receives borrowed body bindings. The first-wins set-based
locator statement splits against actual variable/SQL-length limits, in the
same atomic body/locator/catalogue/signature/window transaction.

WAL/FULL, foreign keys, macOS fullfsync/checkpoint_fullfsync, busy0, mmap0,
cache-2048KiB, wal_autocheckpoint1000 and journal_size_limit4194304 are applied
and read back. The retained journal limit is not a peak WAL bound. The platform
is macOS and VFS selection is default; direct VFS name and physical sync/write
system calls remain UNAVAILABLE. SQL/VM/binding/transaction/commit/body counts
come from actual execution; read commits and write commits are distinguishable.
SQL/commit/transaction spans overlap. Explicit checkpoint results include Busy
and completed/pending frame counts. Final application shutdown/accounting must
retain all checkpoint and close work in the declared command.

Definite failures roll back once. An uncertain SQL/body/commit result quarantines
the shared session without retry or guessed rollback. Read-only writes refuse
before BEGIN; an external write lock returns Busy after one BEGIN attempt.
Catalog identity, incarnation, schema definition, counters and binding are
validated at explicit reopen. Acknowledged inode reservations survive reopen.
No sandbox host endpoint or transport has been implemented by this round.

Verification at the implementation identities:

- Initial owning Storage/History/Persistence/Project suite:58 PASS,0 FAIL,0 ignored.
- After transaction-outcome hardening, actual schema-statement accounting and
  removal of obsolete encoding arbitration, final Persistence/Project suite:
  40 PASS,0 FAIL,0 ignored. This is changed-source coverage, not a speed sample.
- Cases include atomic failure after body INSERT, immutable published rows,
  first-wins order, bounded multi-row statements, hash/framing refusal, Busy,
  read-only, canonical save/reuse/read, C5 conditional transitions/stages/pages/
  identity/refusal, durable reopen, and SIGKILL of a live acknowledged writer.
- The full native namespace oracle checks every path/kind/portable field/file byte
  at100 and1000, over memory and durable ports. It is functional coverage, not a
  benchmark receipt. Process-kill recovery is not a physical power-loss claim.
- Warning-denying Clippy, fmt, product-boundary scan and23 tooling tests pass at
  their recorded source identities. Initial missing lock transition, scripting
  marker/escaping/import errors, unsupported legacy test APIs and two Clippy
  findings remain retained in the turn/tool logs. The first broad offline lock
  regeneration chose newer transitive identities; it was discarded. The final
  narrow package lock transition introduces zero third-party identities.

Archived native-private-save/service tests are not counted as new coverage.
They remain paired with historical product snapshots. C1 and framing/codec/hash
algorithms are retained; no third-party code was edited, no CI/aggregate preflight
was run and there is no push/PR/merge.

Namespace retention is now count-instrumented: entry/job/path/frontier/children,
serial/inode vectors, and directory bindings/change capacities. The focused
100/1000 cases expose growth. Counts exclude opaque allocator/BTree/DirEntry/
PathName internals, database/worker/cache/OS state, and do not prove a total
importer memory bound. Larger qualification and conditional catalogue/paged
builder work remain required.

All revised Step10 competitive decisions are NOT_RUN in this checkpoint:
Init100/1000/10000/100000 and history stride10/3/1 (17/53/157). The active goal's
terminal condition is actual pass of all required selections under the user's
10% working margin, independent proof, storage/accounting/cache/budget gates.
Unit tests, files, compilation and a promising small row cannot satisfy it.
The history deadline exception and numeric Init allocation contract are pending
user clarification. Prior PG/MinIO FAIL receipts retain their original status.

The new benchmark/verifier examples compile in the active workspace. The future
matched reference wrapper is harness-only, calls the pinned unmodified Service
import with identical explicit stack/seed/name, and disables operation recording
for the speed scope. Fresh database creation and final close are inside both
arms' complete child comparison; candidate final checkpoint is included.
Historical PG/MinIO speed/cause run functions now fail before any service start.
The new SQLite runner, seven-case prospective registration and matched release
receipts remain follow-up work, not admission evidence.

Exact production source comparison for this implementation:143628->140936,
delta-2692. Reference65417 unchanged; core retained source78211->75519.
The workspace membership change separates active core78211->27987 from
inactive retained core reference0->47532; that scope change is not code deletion
or an algorithmic simplification. Actual replacement/retirement contributes the
combined-2692 delta. Stable migration source-path totals:old6025->191 (shared
placement algorithm retained), new7821->11068 (includes retained S3 source under
the same classification), rest64365->64260. The root production_loc.py method
counts exact first-parent/staged source snapshots including shipped SQL,
excluding tests/docs/harnesses/comments/blanks/inline test code. The commit
message and frozen counter identify the exact tree. Tracked text-check copies
normalize only trailing EOF whitespace; custody JSON preserves raw hashes and
raw output locations. No measurement receipt is normalized or overwritten.
