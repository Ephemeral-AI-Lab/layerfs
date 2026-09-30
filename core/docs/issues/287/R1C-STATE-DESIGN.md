# R1c typed construction state and metadata-only scratch

> **Status: Dated planning checkpoint; not release evidence or a product contract.**
> Read-only preparation, 2026-09-30. Audited parent source:
> `765202c45e11b3c96b2b16c40b35e810a7270d34` (published R1a).
> R1b-cache workers own their source independently. This note changed no product,
> counter, dependency, engine configuration or target, and ran no build/test or
> benchmark. Its SQLite inventory queries are explicitly scoped below.

The [R0 interface freeze](R0-FROZEN-INTERFACES.md) supplies narrow caller-owned
IndexedState and OrderingBacking: closed typed table namespaces, key at most
288 bytes, value at most 8 KiB, and pages/flushes bounded by **both** 128 records
and 64 KiB of encoded bytes. C1 owns canonical algorithms; C2 supplies real
metadata-only SQLite and file ownership; Server binds their admitted lifetime.
There is no payload/CAS staging store, universal manager, new crate/dependency,
automatic retry or format migration in this checkpoint. SC-03, SC-05, SC-06,
SC-07 and SC-08 supply its population, selection and custody obligations.
The current next action remains **R1b-cache**. R1c is prospective dependent
design; its product integration follows that source checkpoint.

## Complete first production caller

Replace the directory-content result map in C1 filesystem update, rather than
adding an unused state interface. It is exact `serial -> directory ObjectId`
metadata, produced once and read in ordered batches plus point lookups:

| Responsibility | Source at the audited parent |
| --- | --- |
| Resident authority being removed | [update.rs](../../../crates/layerfs-content/src/filesystem/update.rs#L198), line 198: `contents: BTreeMap<u64, ObjectId>` |
| Production of roots | Same file, line 335: each completed directory root is inserted. |
| Ordered consumption after effects | Lines 348–387: roots are iterated in serial batches, old/supplied inode facts joined, and final values submitted to ReferenceReducer. |
| Exact membership of changed directories | Lines 391–399: supplied values for rebuilt directories are skipped. |
| Fresh-build root selection | Lines 429–469, especially 458: the rebuilt root replaces a supplied directory root. |
| Current Server composition | [save/filesystem.rs](../../../crates/layerfs-server/src/service/save/filesystem.rs#L71), lines 71–92, calls C1 with no ordering backing and cleans its received rows. |
| First dependent C2 Save effect | [save/catalog.rs](../../../crates/layerfs-server/src/service/save/catalog.rs#L241), lines 241–253, reserves Save before prepared input/C1 work. |

This is a complete first state path for both fresh build and update within C1.
Its ordinary Server prepared-update caller must actually supply the C2 scratch
adapter and include checked cleanup before known C2 finish/stage publication.
Do not add a public API used only by an example/test and call the port integrated.

Other resident authorities remain explicitly outside this first slice:
[validation additions](../../../crates/layerfs-content/src/filesystem/validate.rs#L165),
lines 165–173; alias/name/candidate collections at lines 310–345 and 382–423;
[cycle graph sets](../../../crates/layerfs-content/src/filesystem/validate/cycles.rs#L70),
lines 70–114 and 144–191; [declared-new/touched state](../../../crates/layerfs-content/src/filesystem/references/reduce.rs#L54),
lines 54–102 and 177–207; [release populations](../../../crates/layerfs-content/src/filesystem/references/release.rs#L61),
lines 61–78; and [file-edit draft/reference/committed state](../../../crates/layerfs-content/src/file/edit/tree.rs#L109),
lines 109–122. R1d replaces these coherently. Removing the directory-root map
alone does not make complete namespace construction bounded.

## Narrow interface and private record grammar

Put C1's real supplied state port under the packet's `filesystem/graph/state.rs`
or a focused `filesystem/state/` responsibility if the first integrated caller
is shared before graph implementation. Do not put behavior in `mod.rs` or create
empty future namespaces. The initial closed `StateTable` has one implemented
variant, **DirectoryRoots**. Its private discriminant may be 1 in the new scratch
format; this allocates no Bridge opcode/canonical role. Later typed namespaces
are added with their actual caller/codec and private format update.

Use these concrete semantics:

```text
StateSelection = operation-selector32(nonzero) + phase8(nonzero) + table1
StateKey       = StateSelection + typed key, total <=288 bytes
DirectoryRoot  = key serial8(big-endian, checked positive) -> value root32
EncodedRecord  = key-length:u16 + full StateKey + value-length:u32 + value
DirectoryRoot encoded record width = 2 + (32+8+1+8) + 4 + 32 = 87 bytes
```

The port supports exact `get`, checked `put/remove` in a writable phase, bounded
ordered `page_after`, phase seal and checked release. All methods receive or own
the exact operation/phase/table selection, not a borrowed SQL statement. Cursor
state is fixed: selection/seal, last emitted key, emitted count/bytes and exact
EOF status. It never owns the full key population.

Directory-root production is already in strictly ordered parent input. Buffer
at most 128 records/64 KiB, insert once, then seal the phase after the complete
directory pass. The seal binds selection, exact count/encoded bytes and SHA-256
of canonical ordered record bytes. For this ordered write-once table, update
digest/totals incrementally only after acknowledged batches. Duplicate/decreasing
keys fail; a second `put` is not an idempotent retry. Generic mutable phases that
later need replace/remove must seal from one bounded ordered pass; do not claim
an incremental digest still describes arbitrary overwritten rows.

After seal, deny put/remove before any SQL. Point lookups and ordered pages use
that immutable selection. The common directory-root consumption algorithm is
identical for prospectively admitted resident and supplied external state.
Choose representation before construction from declared input/slot/exact-state
bytes and proof shape; known eligible state is at most 64 KiB. A large/unbounded
shape receives external backing from the first record. There is no failed B-tree
allocation followed by catch-and-spill, or an active old-map fallback.

Preserve existing independent C1 signatures. Add the genuinely used
`update_filesystem_with_state_timed(objects, input, backing, state, phases)`
entry point (and fresh-build equivalent where its actual caller needs it), then
delegate both entries into one common root-index algorithm. Server calls the
supplied-state entry. Retained independent APIs construct a prospectively
admitted resident IndexedState from their existing declared resources before
canonical effects; they do not get an arbitrary new 64 KiB rejection for a
formerly admitted larger compatibility shape. That larger resident compatibility
profile is explicit and outside the strict small-state profile. The standard
Server path selects <=64 KiB resident state or supplied external state before
effects. No caller resumes the old complete-map body after a failure, and no
separate canonical algorithm is introduced. The resident adapter has the same
typed records, seal and bounded page semantics, with actual container capacity
charged to its declared compatibility owner.

## C2 scratch adapter and bounded SQL

Add focused implementation under `layerfs-storage/src/construction_state/`:
session/profile, index/query, file ownership and the C1 adapter. The shipped
`layerfs-storage/sql/construction_scratch.sql` defines the private schema and
counts as product implementation. C1 imports no rusqlite/path/process types.
Server owns one session per admitted construction operation; scratch owns no
canonical payload, Store pack, C5 record or database mutation authority.

The private database uses a fixed singleton identity/phase metadata table and
`STRICT, WITHOUT ROWID` record table. Its primary key is `(phase, table, key)`.
The session singleton binds the exact operation selector and format version;
the logical C1 key includes that selector even if the SQL representation factors
it into the owned singleton to avoid repeating it in every row. For DirectoryRoots,
enforce serial-key width 8 and root-value width 32 in both checked codecs and SQL.
No arbitrary caller table name or SQL text enters the adapter.

Queries are closed literal statements, with values bound as parameters:

```text
get:    SELECT value FROM state_records WHERE phase=? AND table_id=? AND key=?
page:   SELECT key,value FROM state_records
        WHERE phase=? AND table_id=? AND key>?
        ORDER BY key LIMIT ?        -- primary-key range; count <=128
put:    bounded INSERT batch into the exact writable phase
remove: one checked key in the exact writable phase
```

Check session selection/phase and limits before preparing a query. While stepping
one page, inspect borrowed row BLOB lengths before allocating/copying them. Count
full logical key, lengths, table/selection framing and values against the 64 KiB
output allowance; `Vec<Row>` capacity, decoded metadata and current SQLite row
ownership have their separate resident charge. The first row that cannot fit an
otherwise empty page causes precise capacity refusal, never an empty continuation
loop. A next row that would cross the byte cap remains behind the emitted last
key for the next primary-key range query. Cursors advance strictly and statements
close after the page. The sealed exact row count establishes EOF without fetching
an unaccounted 129th row or materializing a total vector.

A maximal 288-byte key plus 8,192-byte value plus six framing bytes is 8,486 bytes.
Thus a full-size record class fits at most seven rows in 64 KiB, not 128. For the
initial 87-byte directory-root class, 128 rows occupy 11,136 encoded bytes and the
count limit wins. Batch writes validate all key/value widths, uniqueness/order,
count, encoded bytes and prospective quota/engine credits **before BEGIN and the
first INSERT**. One transaction contains only that bounded batch plus fixed
phase metadata. No unindexed sort, ATTACH, wide value, temp-table ordering or
whole-table collect appears on this route.

Set/read back the declared scratch cache 512 KiB, mmap zero, MEMORY journal/temp,
OFF sync, FK on and busy zero. Use safe `rusqlite::limits` for applicable length,
SQL-length/parameter/worker limits and read back actual accepted values. A zero
worker allowance preserves one producer. The length limit must allow the actual
bounded table row, including key/framing/engine overhead; it is not the 8 KiB value
maximum itself. Refusal/unsupported settings cannot change a provider or profile.

## Admission, native file identity and custody

The current C2 crate has physical pack headroom reservation
([reservation.rs](../../../crates/layerfs-storage/src/sqlite/reservation.rs#L38),
lines 38–121), not a Server-global scratch disk quota. It opens with O_NOFOLLOW,
counts allocated blocks and uses Darwin F_PREALLOCATE/Linux KEEP_SIZE. Reuse
those platform primitives through a focused file reservation boundary; do not
reuse Store pack limits or pretend they already admit scratch files.

The caller supplies one explicit scratch permit with resident/index/query/engine,
connection/FD, temporary allocated-byte and cleanup ownership. Reserve it before
creating the scratch file or issuing SQL. For the initial DirectoryRoots profile,
select an explicit 16 MiB maximum/reserved backing class before collection; the
current maximum 65,536 directory declaration has 5,701,632 logical record bytes
at width 87. This is prospective admission arithmetic, **not a proof that SQLite
physical pages/index/journal/high-water fit 16 MiB**. The real provider must qualify
the shape before the strict physical class is advertised. No larger profile
follows an observed miss. The ordering-run budget remains separately named and
merge input/output overlap remains charged.

Create a fresh file in a caller-owned exclusive private directory, capture open-FD
device/inode and exact directory identity, bind the operation selector, and retain
that identity through close/release. Opening embedded SQLite by path has a real
path race unless the private directory/leaf authority is enforced; the Store's
process-local arbitration mutex is not that enforcement. Refuse symlinks or an
identity mismatch. Do not reopen an old leftover database, delete a path merely
because its name matches, or add open-time recovery. Unsupported path/physical
authority returns an explicit capability gap.

Before SQL growth, acquire its prospective engine/dirty-page allowance and
physical capacity. `max_page_count` plus logical row accounting alone does not
prove allocated blocks or rollback-journal memory. Reserve/observe real blocks
with the chosen provider and retain high-water/freelist credit until actual file
removal; deleting a row refunds logical occupancy only. All unused reservation
and real allocation effects are reported, not hidden in setup.

Server creates/adopts these resources before `store.begin_save`, then passes the
session's narrow C1 adapter into filesystem construction. The first implementation
must preserve the existing C1 error seam's exact C2 failure: like SaveHandoff, the
adapter can retain one original typed storage failure while returning the narrow
ContentError. Server checks that retained failure before choosing semantic success
or C2 finish. A flattened `ProviderFailure` must not lose `UnknownOutcome`.

Use the existing one-attempt write boundary
([write.rs](../../../crates/layerfs-storage/src/sqlite/write.rs#L58), lines 58–81):
BEGIN contention is precise refusal; failed COMMIT/ROLLBACK is Unknown. Mark a
release attempt before fallible close/unlink; close statements/connection, verify
owned identity, unlink once, verify removal, then refund the admitted owner.
Close failure retains the returned rusqlite Connection; unlink/identity failure
retains exact blocks and quota. Unknown SQL quarantines the owned session and
denies further mutation/destructive cleanup until an explicit exact decision.
Return/retain a bounded owning failure capsule rather than dropping the only file
identity/account. An abandoned known owner may receive one best-effort Drop
cleanup; a previously attempted release is never retried by Drop. Neither Drop
nor NotFound without proven own identity constitutes the ordinary success proof.

The present C1 FileBacking is a useful ordering contract, but not automatically
the required physical implementation. At
[backing.rs](../../../crates/layerfs-content/src/filesystem/references/backing.rs#L314),
lines 314–321, write_all failure returns the whole logical reservation even though
a partial write can have occurred; lines 285–303 also permit explicit cleanup
followed by destructor cleanup. Its path ledger is a resident map. Reuse ordering
algorithms/ports; give the C2 physical run adapter exact partial-write allocation,
identity and single cleanup custody before claiming the stronger contract.

## Pinned SQLite capability inventory and R1e boundary

The lockfile selects rusqlite **0.40.2** and libsqlite3-sys **0.38.2**. Core storage
uses existing cache/hooks/trace/limits/blob features; there is no selected bundled
SQLite feature. Read-only inventory on this Darwin/arm64 host returned:

```text
sqlite3 --version              3.51.0 (Apple build)
pkg-config --modversion sqlite3 3.51.0
pkg-config --variable=libdir sqlite3  /usr/lib
sqlite3 :memory: 'SELECT sqlite_version(); PRAGMA hard_heap_limit; PRAGMA compile_options;'
  sqlite_version =3.51.0; hard_heap_limit =0;
  DEFAULT_MEMSTATUS=0; DEFAULT_MMAP_SIZE=0; DEFAULT_WORKER_THREADS=0;
  THREADSAFE=2; SYSTEM_MALLOC; MAX_LENGTH=2147483645;
  MAX_VARIABLE_NUMBER=500000
```

These queries changed no guard/database/product state and are not the actual
product connection's enforcement proof. The registry's bundled header says
3.53.2, but that unselected amalgamation is **not** the linked engine version.
Record `rusqlite::version()`/source identity and real pragma readbacks from the
actual owning product process at implementation handoff.

Safe pinned APIs exist for pragma query/update/update-and-check and connection
limits. Safe global MEMSTATUS/config/heap/status wrappers do not exist in
rusqlite::config: it exposes per-connection DbConfig. Existing published native
`rusqlite::ffi` already exports libsqlite3_sys, whose 3.34.1 binding set includes
`sqlite3_config`, `sqlite3_initialize`, `sqlite3_hard_heap_limit64`,
`sqlite3_memory_used`, `sqlite3_memory_highwater` and `SQLITE_CONFIG_MEMSTATUS=9`.
The macOS SDK libsqlite3.0.tbd exports those symbols. No third-party alteration
is required merely to invoke them through a first-party audited boundary.

Exact read-only local primary sources for this historical inventory are:

- [rusqlite native FFI export, lines 55–74](/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rusqlite-0.40.2/src/lib.rs#L55),
  [safe pragma APIs, lines 148–159 and 227–267](/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rusqlite-0.40.2/src/pragma.rs#L148),
  [safe connection limits, lines 16–75](/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rusqlite-0.40.2/src/limits.rs#L16),
  and [per-connection DbConfig](/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rusqlite-0.40.2/src/config.rs#L15).
- [published 3.34.1 native bindings](/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libsqlite3-sys-0.38.2/bindgen-bindings/bindgen_3.34.1.rs#L785):
  config/initialize at lines 785/797, memory observers 916/919 and hard limit 1688;
  MEMSTATUS constant at line 237.
- [SDK configuration contract](/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include/sqlite3.h#L1661),
  lines 1661–1688 and 1883–1890; [SDK exported symbols](/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/lib/libsqlite3.0.tbd#L41),
  lines 41, 57–58 and 64. These are machine-local inventory links, not committed
  copies, generated headers from a new build, or proof of the running Core binary.
- The unselected [3.53.2 package header](/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libsqlite3-sys-0.38.2/sqlite3/sqlite3.h#L1694),
  lines 1694–1727, 1930–1940 and 7410–7426, documents the initialization/tracking
  conditions; its bundled version identity does not replace the linked inventory.

The observed DEFAULT_MEMSTATUS=0 makes **current bootstrap** insufficient for the
strict guard. It does not show that a new explicit owned pre-initialization
capability is impossible. Root owns that R1e contract: in a fresh process, before
any SQLite use or worker, one audited boundary calls
`sqlite3_config(SQLITE_CONFIG_MEMSTATUS, 1 as c_int)`, requires SQLITE_OK, and then
initializes and establishes/checks the declared guard. SQLITE_MISUSE (21) means
the library was already initialized and must refuse. No shutdown/reset of foreign
connections, config retry or guard deactivation is permitted. Dedicated bootstrap
must own the absence of competing SQLite initializers; a local OnceLock alone
does not establish that against unrelated embedding users.

That requires a transparent first-party unsafe-boundary extension: current
[boundary guard](../../../tools/check_product_boundary.py#L16), lines 16–50,
permits storage unsafe only in encoding/codec.rs. Keep C1 unsafe-free, place an
approved narrow native engine boundary in its own responsibility file, enumerate
the FFI calls/argument/thread/initialization invariants, update the guard and its
external self-tests, and retain the no-patch rule. Do not paste engine bootstrap
into the codec module merely to evade the current guard.

Readback is insufficient when memory tracking/custom page-cache/static-cache
conditions bypass enforcement. Fresh owned-process proof must establish tracking
and actual allocation refusal under the guard, plus correct failure/custody and
protected catalog progress. Global heap/memory-highwater are process scopes;
phase attribution needs the declared reset/owner method. The
[SQLite heap-limit contract](https://sqlite.org/c3ref/hard_heap_limit64.html) and
[pre-initialization configuration contract](https://sqlite.org/c3ref/config.html)
support these distinctions. No FFI, initialization/config mutation or enforcement
test was executed by this note. Linux containment/physical progress remains unrun.

## Production LOC counter correction

The audited counting method is `tools/production_loc.py`, SHA-256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.
Do not substitute the older, differently classified `core/tools/production_loc.py`.
The root counter at
[lines 284–299](../../../../tools/production_loc.py#L284) blanks only full-line
`--` comments despite its block-comment docstring. At
[lines 319–331](../../../../tools/production_loc.py#L319), source paths accept
only `.rs`, so runtime SQL under `src/` is omitted. Package `sql/*.sql` is already
classified, including prospective `sql/construction_scratch.sql`.

If the new SQL uses only full-line `--` comments and package `sql/`, those two
defects do not affect that exact file. Still, do not design the new shipped input
around a misleading counter. The minimal correction is:

- Replace only `blank_sql` with a scanner preserving newlines, blanking `--` line
  and `/*...*/` block comments outside SQL literals/identifiers. Preserve doubled
  single quotes, doubled double/backtick quotes and bracketed identifiers so
  comment-looking payload text still counts. Code beside a comment counts once;
  a comment-only line counts zero. Use SQLite's block-comment grammar, not Rust's
  nested-comment rules.
- Include `.sql` in recognized production `src/` inputs for Core/reference and
  existing nested API core/sdk paths. Restrict the Rust test-module exclusion
  traversal to Rust files, so SQL literals do not invent Rust module edges.
- Extend `tools/test_production_loc.py` with mixed code/block/line comments,
  multiline block comments, quoted comment text and escaped quotes, comment plus
  code on one physical line, `src/schema.sql` for both scopes/nested API, and
  unchanged tests/examples/target exclusions. Use one revised method for both
  exact first-parent and final staged snapshots, and report the new hash.

No counter code/test was edited or run here. Historical LOC messages remain
historical. If reclassification changes a comparison subtotal, label it as scope
correction and show both revised snapshots; it is not product simplification.

## R1c checkpoint proof and remaining gates

The owning checkpoint must deliver typed DirectoryRoots state, its real C2
scratch adapter, Server wiring before Save effects, canonical equivalence and
checked completion/cleanup together. External tests cover ordered output over
more than 128 records, byte-bound pages using a smaller caller allowance with
the actual 87-byte record class, exact point
membership, duplicate/decreasing keys, selection/seal mismatch, mutation after
seal, first-record-too-wide refusal and all actual quota boundary observations.
Use independent filesystem roots/partitions and real Store/C5 prepared-update
composition; known successful source/root behavior and cache proofs remain reused.

Real-provider tests must demonstrate no dependent Save/SQL effect after upfront
resource refusal, exact accepted bounded batches, successful cleanup/refund,
retained owner on actual close/unlink/contention failure, and no guessed cleanup
or resume after Unknown. Query-plan evidence must show primary-key bounded range
access; SQLite heap/native/cache and physical high-water observations retain
their declared provider/owner scopes. No fake allocator/clock, product fault hook,
new benchmark runner or performance sample is required.

Complete R1c's narrow path without claiming broad R1d graph/draft/frontier bounds,
new canonical v2 identities, schema11 migration, full strict engine/control or
physical containment. Those remain separately named gates. Every actual product
checkpoint updates its architecture, applies exact revised LOC to both snapshots,
runs the frozen meaningful owning checks and is published by root with #287
evidence. #288 benchmark qualification remains delegated/unrun.
