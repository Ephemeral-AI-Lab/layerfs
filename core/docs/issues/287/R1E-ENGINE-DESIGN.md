# R1e owned SQLite bootstrap, engine credits and provider qualification

> **Status: Dated preparation; no implemented capability or provider PASS.**
> Read-only audit, 2026-09-30. Product source pin:
> `06fe8363d5c317c49876d5189d374c1a34cc6010` (published R1b-cache).
> R1c workers own their current C1/C2 changes. This note does not edit product,
> tools, dependencies or configuration and runs no build, Cargo command, product
> test or benchmark. The explicitly read-only CLI inventory is recorded below.
> Root owns the actual startup/FFI/Bridge contract and implementation; follow the
> [current checklist](CHECKLIST.md) for execution, not this prospective ordering.

SC-03/06/07/08 and the [R0 interface freeze](R0-FROZEN-INTERFACES.md),
[resource audit](R0-RESOURCE-AUDIT.md),
[R1c provider inventory](R1C-STATE-DESIGN.md) and
[SERVER packet](../../architecture/proposal/bounded-workspace-implementation-20260930/SERVER.md)
own the required accounting. A qualified 32-MiB **EngineGuard**, healthy protected
catalog progress and the 256-MiB physical Server target are three separate facts.
Successful heap readback establishes none of the latter two.

## Resolve the startup trap first

The upstream 3.51.0 primary
[malloc.c](https://raw.githubusercontent.com/sqlite/sqlite/version-3.51.0/src/malloc.c#L129)
calls `sqlite3_initialize()` in `sqlite3_hard_heap_limit64`, before its query/set
branch, unless the engine was built with `SQLITE_OMIT_AUTOINIT`. A negative
argument queries the limit but does **not** avoid initialization. Soft-limit
queries have the same ordering. The local **unselected** 3.53.2 amalgamation
agrees at
[lines31604–31619](/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libsqlite3-sys-0.38.2/sqlite3/sqlite3.c#L31604).
Neither source is a proof of the Core-linked Apple implementation; a fresh
linked-process test below must establish that behavior.

Therefore the first guard call cannot be a negative heap-limit query, a PRAGMA,
connection open, native allocation or another automatic initializer. The owned
startup sequence is:

1. Establish exclusive pre-initialization process authority, before any competing
   SQLite call or worker that may make one.
2. Call `sqlite3_config(SQLITE_CONFIG_MEMSTATUS, 1 as c_int)` and require
   SQLITE_OK. SQLITE_MISUSE means already initialized: refuse before any limit
   query/change or Store/catalog/scratch activity. Other errors also refuse.
3. Call `sqlite3_initialize()` exactly once and require SQLITE_OK.
4. Only now query the old hard limit. This profile accepts 0 or exactly 33,554,432;
   another existing provider limit is Unsupported, not permission to raise it.
5. Set the hard limit to 33,554,432 bytes; check the returned prior limit and
   read back exactly that value. A negative return, mismatch or concurrent
   mutation is terminal. No restore/retry/shutdown path exists.
6. Prove active tracking with a bounded native allocation/status check while
   startup still owns exclusivity. A proposed 8-KiB probe is actual capability
   validation, not a test-only product hook: memory-used/malloc-count must rise
   consistently with native `sqlite3_msize`, then the owner frees it exactly
   once. Accounting/error after allocation still frees that known probe.
7. Produce an unforgeable same-process EngineGuard owner with the exact profile,
   provider version/source id and fixed startup disposition. Open participating
   connections only after this owner exists.

The initialization/default profile and probe allowance are charged. Cache the
one success or failure; do not retry a failed bootstrap or reset SQLite to make
it eligible. Repeated consumers borrow the established owner. No guard Drop
calls shutdown, changes limits or attempts to restore a prior configuration.
Before a required effect, validate that its current hard limit remains exact;
a foreign change refuses rather than being silently repaired.

The [global configuration API](https://www.sqlite.org/c3ref/config.html) is not
thread-safe, and most settings refuse after initialization. A OnceLock protects
this wrapper's contenders only; it does not exclude a foreign SQLite initializer.
The [MEMSTATUS contract](https://www.sqlite.org/c3ref/c_config_covering_index_scan.html)
allows enabling accounting when compiled disabled by default. Its documented
heap/status behavior and the
[heap-limit contract](https://www.sqlite.org/c3ref/hard_heap_limit64.html)
also require the actual allocator/cache configuration to support enforcement.
An arbitrary embedding process or alternative preconfigured allocator/page-cache
cannot become this supported owned process by claiming a token.

## Genuine startup and safe factory boundary

The production native binary currently calls `host::run` at
[bin/layerfs-server.rs](../../../crates/layerfs-server/src/bin/layerfs-server.rs#L1),
lines1–9. In
[host/run.rs](../../../crates/layerfs-server/src/host/run.rs#L50),
Store open occurs at53, telemetry construction at55, History construction at61
and the acceptor at71. Put the owned bootstrap at the binary's initial startup
boundary and pass its established owner into a safe host assembly route. It must
precede Store open, history configuration, enabled telemetry and listener/session
workers. `Service::new` is too late: it receives already opened authorities.

The SDK/operator composition also reaches Store before `assemble`:
[host/assembly.rs](../../../crates/layerfs-server/src/host/assembly.rs#L80),
create82–90 and open94–99; history opens109–117. Add a genuinely used safe
`create/open_with_engine` composition, or an explicit owning profile field,
that requires the established guard **before** those first opens. A current
ServerConfig may already contain an enabled Runtime supplied by its embedding
caller. Therefore merely adding global configuration inside these public
constructors cannot prove startup exclusivity. SDK project operations borrow the
already assembled Server
([project.rs](../../../crates/layerfs-api/sdk/src/project.rs#L17)); they do not
own process bootstrap.

For a public Rust bootstrap, exclusion of competing SQLite calls is a safety
precondition. Already-initialized state is a defined refusal case, rather than
permission to reconfigure it. The concrete sound boundary is an audited unsafe startup function
in C2, with the native binary's **single early call** included explicitly in the
source audit. Safe owned factories consume its unforgeable result; they never
attempt global configuration themselves. Server's library can retain
`forbid(unsafe_code)`. An embedding caller must establish that startup authority
at its own audited boundary, or use the explicit compatibility profile without
an EngineGuard claim. There is no error-driven compatibility fallback.
Root must freeze the exact native startup visibility and ownership boundary
before implementation; do not expose a safe late global-mutator API based only
on a process-local mutex or thread-count snapshot.

The current daemon is a native remote client:
[daemon/run.rs](../../../crates/layerfs-daemon/src/run.rs#L36),
lines36–89, and its
[dependencies](../../../crates/layerfs-daemon/Cargo.toml) contain no C2/History
SQLite owner. It connects to the Server and constructs Workspace/FUSE authority.
Do not initialize SQLite in that daemon simply because the Server needs a guard,
or imply a daemon runtime capability proves the remote Server's memory. Future
daemon SQLite ownership would need its own explicit process startup contract.

## Existing FFI and transparent source-boundary extension

The lock selects rusqlite 0.40.2 and libsqlite3-sys 0.38.2, without a selected bundled
SQLite feature. The existing
[ffi reexport](/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rusqlite-0.40.2/src/lib.rs#L55)
already supplies the needed published APIs. The local3.34.1 binding set exports:

| Native API | Exact primary binding source |
| --- | --- |
| MEMSTATUS 9; initialize; variadic config | [bindings](/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libsqlite3-sys-0.38.2/bindgen-bindings/bindgen_3.34.1.rs#L237), lines237,785,797 |
| Native malloc64/free/msize and memory used/highwater | Same binding file, lines895,910,913,916,919 |
| Hard limit and global status64 | Same file, lines1688 and2113 |
| DB cache-used/cache-spill selectors | Same file, lines426 and437; check each API result on the actual provider |

No new dependency, third-party change, custom allocator replacement or bundled
engine substitution is necessary. Safe per-connection limits/PRAGMAs already
exist. Safe global config/heap/status wrappers are absent; obtaining a raw
connection handle is itself
[unsafe](/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rusqlite-0.40.2/src/lib.rs#L952).

Put closed native calls in proposed `sqlite/engine_ffi.rs`, with safe profile/
owner/observation logic in a focused `sqlite/engine.rs`. Keep raw pointers,
variadic argument types, native allocation RAII and connection-handle/status
lifetime checks at that audited boundary. Status only borrows a live Connection
under its actual exclusive owner; it stores no handle past that borrow and never
resets counters. Expose product resource observations used by the admission/
diagnostic path, not methods added only to let tests inspect private state.

The current guard permits only `src/encoding/codec.rs`
([check_product_boundary.py](../../../tools/check_product_boundary.py#L16),
lines16–51). The owning implementation must explicitly update that exact-file
allowlist to include the new C2 FFI file and the native binary's single startup
call, reject every sibling/other executable, and retain the library root lints.
Update Core AGENTS, C2 lib/encoding comments, applicable contracts and the
SaveConnectionProfile explanation
([store.rs](../../../crates/layerfs-storage/src/cas/store.rs#L171),
lines171–179) in the same source commit. Do not put engine code inside the zstd
codec to inherit its exception. Existing line ceilings remain unchanged.

Extend the external guard self-tests at
[test_check_product_boundary.py](../../../tools/test_check_product_boundary.py#L43):
both exact audited C2 sites permitted; adjacent files, nested names, alternate
paths/includes and other binaries rejected; content/telemetry remain forbidden;
Server library remains forbidden; root lint removal still fails. This is an
audited boundary expansion, not disabling or relaxing the scanner globally.

## What the source must replace or admit before a strict profile

Current C2 config
([connection.rs](../../../crates/layerfs-storage/src/sqlite/connection.rs#L31),
lines31–71) does not enforce cache/mmap/heap and does not verify temp_store.
History config
([history/open.rs](../../../crates/layerfs-history/src/sqlite/open.rs#L181),
lines181–206) also lacks those resource controls. Configure and verify each
participating main/TEMP schema before a required effect: C2 cache 2MiB, read
cache 1MiB, scratch 512KiB, proposed C5 cache 1MiB, mmap 0, MEMORY journal/temp, OFF sync,
FK on, busy 0 and existing zero-worker profile. The C5 cache proposal is within
its still-unqualified total 4MiB credit, not additional headroom. Preserve persisted page size;
derive any page thresholds from that exact value. A cache-size suggestion alone
does not bound dirty pages, journals, SQL results or transient allocations.

The concrete prospective spill threshold for cache budget C and verified page
size P is `max(1, floor(C/P))` pages, with spill enabled and exact readback before
effects. At4096-byte pages this is512 for C2,256 for read/C5 and128 for scratch.
Freeze that declared policy before source enablement; do not inherit the observed
20000-page threshold, disable spill or raise cache after a miss. This only bounds
the intended spill threshold: actual dirty/journal/native overlap still needs
the real-provider shape proofs, and its changed I/O work is a#288 dependency.

| Existing production shape | Required correction/proof before advertising fit |
| --- | --- |
| Private published-index clones on acquisition | [cas/lifecycle.rs](../../../crates/layerfs-storage/src/cas/lifecycle.rs#L30), lines40–52, clones PoolIndex/Candidates after the persisted Save slot. Admit the actual clone/workspaces before Save effects; preserve the exact published/private visibility snapshot. These are first-party Rust bytes, not SQLite heap or a copied SQLite database. |
| Another replacement index clone after known publication | Same file, lines308–330: old shared + private + new replacement coexist while assignment evaluates. Pre-admit that publication capacity; no predictable allocation after known publication. Retain sound bounded indexes if exact overlap fits, or use a pre-funded immutable published snapshot plus bounded private effects with an exact visibility boundary. Do not add a new universal cache manager. |
| Index reported-byte limitations | [Candidates](../../../crates/layerfs-storage/src/encoding/delta/candidates.rs#L54) is fixed8192 slots plus65536 references and uses actual type sizes at238; [PoolIndex](../../../crates/layerfs-storage/src/encoding/pool/index.rs#L125) estimates tuple-size+8, while [policy.rs](../../../crates/layerfs-storage/src/policy.rs#L241) permits131072 entries. BTree node allocation is not established by that estimate; observe actual owned clones/capacity and include temporary replacement. |
| Singleton exceeds nominal wave/transaction bytes | [batch.rs](../../../crates/layerfs-storage/src/cas/batch.rs#L43), lines43–67, explicitly accepts an oversized object into an empty batch. The4MiB−1 wave constant at [policy.rs](../../../crates/layerfs-storage/src/policy.rs#L140) is not a maximum of that path. The legal C2 canonical ceiling is16MiB and singleton capacity adds4096. Preserve format compatibility; prospectively admit its actual larger shape or implement bounded physical subtransactions without prematurely making its locator/completion visible. |
| Whole pack SQL value | [lookup.rs](../../../crates/layerfs-storage/src/sqlite/lookup.rs#L170), lines170–183, selects complete `p.data` into a Rust Vec. The SQLite result and first-party copy are distinct simultaneous owners. At the singleton ceiling, this is not a2MiB engine read. Replace large reads with checked metadata/length/visibility selection and existing safe incremental Blob reads into admitted first-party windows, retaining the immutable selected location and exact pack checks. |
| Large incremental write in one transaction | [write.rs](../../../crates/layerfs-storage/src/sqlite/write.rs#L126), lines126–136 and186–217, inserts capacity-sized zeroblob and writes the complete selected body through at-most-three writes. Incremental I/O removes a full SQL parameter but does not itself cap dirty/journal ownership. Pre-admit actual engine pages; where needed, bounded private body transactions must leave locator/header completion unavailable until exact body/EOF checks pass. A write chunk without a transaction boundary cannot bound a MEMORY journal. |
| Cleanup by row count | [cleanup.rs](../../../crates/layerfs-storage/src/sqlite/cleanup.rs#L21), lines21–78, deletes one pack per transaction. That bounds rows, not arbitrary BLOB/journal memory or engine traversal. Qualify the actual large singleton and provider secure-delete/page/freelist behavior; if it exceeds the declared phase, redesign its bounded retirement/cutover authority before enabling that shape. No journal-OFF change, speculative quota raise or durability expansion. |
| Tail close | `substr` rewriting exists at [write.rs](../../../crates/layerfs-storage/src/sqlite/write.rs#L168), but its actual finish caller [lifecycle.rs](../../../crates/layerfs-storage/src/cas/lifecycle.rs#L268) invokes it only for the bounded pooled-metadata tail. Do not invent a16MiB singleton tail-rewrite witness. Include its real bounded transient/journal state. |
| Candidate persistence and SQL sort state | [candidates.rs](../../../crates/layerfs-storage/src/encoding/delta/candidates.rs#L282) loads at most8192 rows in stamp order and flushes its ring. Native temporary/index/statement memory remains engine-owned; verify real query plans and actual DB/global status instead of inferring native bytes from row counts. |

Store read_batch holds arbitration for the **whole** wave
([store.rs](../../../crates/layerfs-storage/src/cas/store.rs#L414), lines414–455).
Thus two launched same-Store reads do not prove simultaneous complete SQL BLOB
values. Existing serialization is part of the source/provider profile, not
permission to claim two independent reads progressed concurrently. A later
released-lock/channel composition must re-admit its actual overlap.

The singleton, journal and retained-cache shapes can exceed the proposed phase
credits and can exhaust a32MiB shared guard when composed with the other owners.
This is a source-derived risk/gate, **not a measured engine crossing here**.
Readback does not cure it. Do not advertise strict supported capacity until the
ordinary largest supported paths and their failure/cleanup paths pass under
the unchanged guard and all required other live owners.

## Phase credits and the protected catalog remain separate

The proposed global ledger is6MiB retained C2 connections +12MiB single active
Store SQL wave +4MiB admitted reads +4MiB scratch/global +4MiB C5 =30MiB,
leaving2MiB under the32MiB guard. Distinguish baseline retained allocation from
incremental active work, so neither gets counted twice or omitted. Before the
catalog is admitted, ordinary live engine shapes must be proven at most26MiB,
with the declared2MiB margin and4MiB catalog work left available.

A token accounting4MiB does not physically partition SQLite malloc. Scheduling
credits need actual bounded statement/result/dirty/journal/cache shapes, including
cleanup and startup, so ordinary owners cannot consume that headroom. Release
a phase credit only after its actual reader/statement/transaction owner ends;
retained caches and Unknown connections remain charged. SQL-wave arbitration
and phase credits are separate from the persisted two Save slots. Idle Saves
cannot reserve full active-wave buffers forever, and C5 cannot borrow their slots.

Native protection is another gate. Current acceptor admission may fill every
general session and uses a2MiB session thread stack
([acceptor.rs](../../../crates/layerfs-server/src/host/acceptor.rs#L132),
lines132–164). Complete the frozen protected catalog/control purpose, FD/channel,
preauthentication, terminal and executor owners before claiming native progress.
No memory-counter test can replace the real C5 operation through that route.

ServiceCapabilities has no separately allocated EngineGuard feature bit in the
R0 grammar. StrictServerMemory bit7 must remain zero until the physical/profile
requirements pass; ProtectedCatalog bit6 has its own native/liveness gate.
Internal EngineGuard support/source proof can be reported separately. Root owns
any prospective Bridge bit/record change; this note allocates none.

## Fresh-process owning external proofs

Use ordinary external tests under storage/server tests, with each global case
run in a **fresh subprocess** of the owning test executable/native binary.
An external test can select one child case via its own test-only environment
and `--exact`; no helper runner/family or product test hook is needed. Do not
reuse one process by shutdown/reset, lower/restore limits, substitute a Python
SQLite engine, or let another test initialize the library first. Record actual
Core-linked version/source id/compile options, native symbols, lock/build flags
and the exact product source. All cases below are unrun.

1. **Owned bootstrap/tracking.** Call the actual guarded entry on a fresh process,
   then open the real C2 Store, real C5 catalog and owned scratch. Verify exact
   limit, connection settings and current native memory/malloc counters. The
   production tracking probe and further bounded native allocations must move
   those counters; default-MEMSTATUS text or a scalar32MiB response is not proof.
2. **The query-before-config trap.** In an independent fresh child, first call
   native hard_heap_limit64(-1), then the production bootstrap. Expect precise
   already-initialized refusal on an automatic-init provider. This tests the
   ordering failure without inserting a fault branch into product.
3. **Foreign already initialized.** Open a real foreign SQLite connection first,
   create/read a small marker, then invoke bootstrap once. It must refuse,
   preserve that connection/marker and prior limit, and create no owned Store,
   catalog or scratch. No shutdown, guessed adoption or attempt to fix it.
4. **Native enforcement.** After valid owned bootstrap, hold at most32 bounded
   native allocations from `sqlite3_malloc64`, touch their pages, and record
   `sqlite3_msize`, MEMORY_USED and MALLOC_COUNT. One declared request exceeding
   remaining guard space must return null with the limit unchanged; stop at that
   refusal, not free-and-retry for a nicer outcome. Free known pointers once and
   prove their tracked ownership ended. No replacement/fake allocator is used.
5. **Real SQL NOMEM/custody.** Use the actual provider and largest legal supported
   SQL/C2 shapes under a prospectively declared finite allocation pressure owner.
   Establish whether refusal occurred before SQL effects, within an operation,
   at COMMIT/ROLLBACK or during cleanup. Keep current
   [write.rs](../../../crates/layerfs-storage/src/sqlite/write.rs#L58)
   known/Unknown semantics and
   [finish.rs](../../../crates/layerfs-storage/src/cas/finish.rs#L12)
   quarantine/cleanup custody. Do not treat NOMEM as a guessed rollback, resend
   the operation or raise the guard. A real failure is not a healthy-progress PASS.
6. **Largest normal shape.** Independently pinned valid objects exercise maximum
   admitted singleton/ordinary/pooled paths, existing-pack/freelist reuse,
   candidate-ring flush, supported scratch maximum, reads and failed-Save cleanup.
   Observe actual native SQL memory and first-party overlap at their real phases;
   exact bytes/identities are checked separately. No favorable smaller shape
   substitutes for the supported maximum, and no larger profile is activated here.
7. **Protected catalog progress.** Keep two real StageChanges saves active with
   verified persisted slots and real phase witnesses. Add the actually qualified
   read/scratch/SQL ownership, then execute catalog allocation/stage/CAS on its
   protected native route and prove completion/refund/custody. A submitted RPC,
   launched PID or sleep is insufficient. Raw malloc holders in the enforcement
   case do not replace actual product owners for this healthy-progress proof.

Global `sqlite3_status64` and per-connection DB status are native engine facts.
Use resetFlag0; the lifetime highwater is labeled as such and cannot stand in
for a phase number. Current CACHE_USED, SCHEMA_USED, STMT_USED and real spill
count deltas help identify shape; they are not a complete per-connection journal
allocator compartment. The
[global status](https://www.sqlite.org/c3ref/c_status_malloc_count.html) and
[DB status](https://www.sqlite.org/c3ref/c_dbstatus_options.html) APIs have distinct
units/meaning and unsupported codes must remain Unsupported. C5 connection
observation must respect its actual provider boundary; no private-handle leak or
new dependency/cross-crate visibility only for tests.

Existing nix reexports locked libc0.2.189. Native allocator corroboration can use
Darwin `malloc_zone_statistics(NULL,...)`, whose installed primary
[header](/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include/malloc/malloc.h#L444)
defines current in-use and reserved bytes and distinguishes its highwater; the
[pinned bindings](/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libc-0.2.189/src/unix/bsd/apple/mod.rs#L892)
already provide it. GNU/Linux mallinfo2 has a different availability/scope;
musl cannot be credited with that interface. These aggregate C allocator facts
include other allocations and allocator retention, and omit file-cache proof.
They corroborate native activity; they do not replace SQLite-specific tracking
or establish physical Server containment. No allocator hook/replacement is added.

The prospective owning commands are locked storage `--test engine_guard` and
Server `--test engine_bootstrap`, plus covering product-boundary/self-tests,
fmt/Clippy/owning Core checks at root's coherent handoff. Names become real
external targets only when implemented. This note executed none. Builds remain
inside the owned worktree with the root ARMv8 AEAD flags; no foreign Cargo target,
CI, aggregate preflight or benchmark campaign is used.

## Read-only local inventory and remaining capability scope

This preparation ran only this CLI inventory command, with no assignments:

```text
sqlite3 :memory: 'PRAGMA secure_delete; PRAGMA cache_size; PRAGMA cache_spill; PRAGMA page_size; SELECT sqlite_source_id();'
secure_delete=2 (FAST), cache_size=2000, cache_spill=20000, page_size=4096
source_id=2025-06-12 13:14:41 f0ca7bba1c5e232e5d279fad6338121ab55af0c8c68c84cdfb18ba5114dcaapl
```

The earlier R1c inventory reports CLI version3.51.0, hard limit0 and
DEFAULT_MEMSTATUS=0. These are separate Apple CLI observations, not linked Core
runtime evidence. Upstream3.51.0 tagged source is not assumed identical to that
Apple source id. The positive default cache values here imply page units, and
the large spill threshold cannot certify the proposed2MiB connection profile.
Secure-delete FAST is not ON; do not infer all freed overflow pages are journaled
or assert a measured16MiB delete journal. Provider mode matters: see the
[secure-delete contract](https://www.sqlite.org/pragma.html#pragma_secure_delete)
and upstream
[freePage2](https://raw.githubusercontent.com/sqlite/sqlite/version-3.51.0/src/btree.c#L6455).
The linked provider's actual behavior needs the owning cases above.

The existing provider may support the new explicit pre-init guard without any
dependency change. Current bootstrap remains unsupported for that claim. Even
after the engine proof passes, **PhysicalMemory remains unavailable/incomplete**
until a real supported provider measures/enforces the simultaneous whole Server
pool, native allocators/stacks, SQLite and physical file cache with healthy
progress. Darwin heap counters and Linux hard cgroup containment alone cannot
prove that composition. No relocated Docker Server, lifetime-peak substitution,
heap-only PASS or ignored observation gap is permitted.

Select coherent checkpoints only when their genuine delivery/proofs exist:
R1e-bootstrap for the actual entry/guard and native enforcement;
R1e-engine-shapes for corrected C2 read/write/index/cleanup ownership and exact
phase credits; R1e-protected-composition for real occupied-owner native catalog
progress. A partial/unsupported shape keeps dependent StrictServerMemory and
ProtectedCatalog enablement disabled. Changes to native startup, SQL cache/spill,
large BLOB access, private index publication and transaction cadence are precise
later#288 qualification dependencies. No campaign, new runner/family, speed
admission or release completion is claimed here.
