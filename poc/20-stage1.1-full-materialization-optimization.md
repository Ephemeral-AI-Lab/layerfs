# Stage 1.1M — Portable Full-Materialization Optimization

Status: **Verified implementation/correctness closed; performance
`REVISE_NO_AUTHORIZED_OWNER`; `terminal_pass=false`**
Baseline authority: [16 — Stage 1.1 Apple edge benchmark](16-stage1-part1-apple-edge-benchmark.md)
Canonical, trust, durability, and Apple authority: [10 — handoff freeze](10-handoff-freeze.md)
Historical accepted source: `f3dd4a32273a4c5cbe5e7ca2287c945ba4434c30`
Verified performance operand: `9800f8650bbb5f1ae89fe8de2724bcd7e331716a`
Current-source correctness closure: `0403ea7166b332c5ddcb7b6cf04f60a0610fd5db`
Sequence: **Stage 1.1 accepted baseline -> Stage 1.1M attribution and repair -> Stage 1.2**

## M1 execution-authority correction

Post-freeze executable inspection proved that the preserved `f3dd4a3` release
binary cannot execute the section-12 parity operation: its CLI has no
single-materialization or primer command, its fixed Apple-edge route requires
47 rows, and every C08 row opens a fresh Engine and performs no same-open
primer. It also cannot emit the hidden SQL/native/sync facts that M1 was
created to add. Requiring that immutable binary to run the new operation is
therefore impossible, not a product or performance failure.

The authorized smallest correction is:

```text
accepted historical executable
  = preserved unchanged as Stage 1.1 correctness/wall custody

historical parity harness
  = product Rust/Cargo files byte-identical to f3dd4a3
  + evaluator-only parity command and custody schema
  = no attribution instrumentation and no product change

instrumented A/B control
  = the same evaluator command
  + attribution-only product facts/timers

candidate
  = frozen instrumented control plus selected product optimizations
```

The exact 8-primer/8-measured-row conditioning, estimator, wall gates, fixtures,
oracles, pair order, and performance targets remain unchanged. Product-source
manifests independently prove that the historical harness product is exactly
`f3dd4a3`; evaluator-source manifests identify the added command. Fields the
uninstrumented harness cannot emit are proved by product-source identity plus
focused instrumented observation goldens and are never fabricated as zero.
The accepted binary, attempt-014, and all failed rows remain immutable.

## 0. Decision

Attempt 014 remains immutable accepted evidence for its exact source:

```text
47/47 rows
51/51 edit operations
34/34 durable transitions
fresh full materialization p50 73.002 ms / 328.759 MiB/s
complete campaign 13.517581334 s
```

Its byte, metadata, history, durability, resource, and measured-wall results
remain valid. Later review found that its materialization attribution was not
complete:

```text
33 source-Store SQL statements and 3 sequential Store connection opens were hidden by
  scratch construction

3 Apple workspace-setup syncs happened before VFS native counters began

regular-file and directory sync walls were unavailable

C08 was fresh-destination reconstruction, not retained-workspace refresh
```

Therefore:

```text
Stage 1.1 correctness and measured wall        PASS / preserved
Stage 1.1 complete SQL/native attribution      REVISE / repaired here
Stage 1.1M implementation/correctness           PASS / current source
Stage 1.1M Verified performance                 REVISE / no authorized owner
```

This document does not edit, relabel, replace, or delete attempt-014 rows. It
defines one portable constant-factor optimization of authenticated full
materialization and one source-bound before/after campaign.

### 0.1 Executed terminal result

The implementation and measurement loop has run. The compact durable receipt
is [the Stage 1.1M current-source closure](evidence/stage1.1m-current-source-closure-20260825/summary.md).
The controlling Verified result is:

| Gate | Observed | Disposition |
|---|---:|---|
| 24 MiB p50 | `62.191459 ms / 385.905 MiB/s` | FAIL by `8.858126 ms` |
| 24 MiB p95 | `65.981500 ms / 363.738 MiB/s` | FAIL by `7.314500 ms` |
| 96 MiB p50 | `179.337500 ms / 535.304 MiB/s` | PASS |
| 96 MiB p95 | `183.878333 ms / 522.084 MiB/s` | PASS |
| Measured zero-byte p50 | `24.071333 ms` | report |
| Fitted intercept | `23.142779 ms` | FAIL by `3.142779 ms` |
| Fitted sustained bandwidth | `614.617 MiB/s` | PASS |

M7 is retained. It removed one redundant fresh-construction install-parent
barrier and saved the direct owner `4.330125 ms` at 24 MiB and `4.384750 ms`
at 96 MiB. The independently audited Engine and native owner surfaces contain
no further safe owner meeting the 3 ms prospective floor at both targets.
The paired acceptance population was therefore not consumed: the absolute
24 MiB gates already fail. This is an honest terminal `REVISE`, not a target
waiver.

Current-source correctness is separately closed by attempt-015: `47/47`
rows, `51/51` edit/sub-edit operations, `34/34` durable transitions, exact
bytes/metadata/history/refresh and the fixture's exact empty hard-link topology,
zero FullFallback, zero forbidden
rematerialization, no BUSY/LOCKED, Q terminal zero, connections terminal zero,
FD closure, and zero owned residue.

## 1. Exact operation taxonomy

The unqualified phrase `warm materialization` is forbidden. Reports use one of
these labels:

| Label | Source state | Destination state | Required work |
|---|---|---|---|
| `first_open_fresh_destination` | Fresh Engine; source cache warm-or-unknown | Fresh empty destination | Full fetch/auth/decode/write/metadata/sync |
| `same_open_warmed_source_fresh_destination` | Same Engine remains open after one untimed full primer | Fresh empty destination | Full auth/decode/write/metadata/sync |
| `warm_authenticated_null_sink` | Same Engine and primer | No native output | Full materializer source traversal/auth/decode |
| `warm_authenticated_digest` | Same Engine and primer | No native output | Source traversal/auth/decode plus full digest |
| `native_durable_output` | Sealed deterministic bounded-stream source | Fresh empty destination | Native write/metadata/install/sync only |
| `existing_destination_exact_noop` | Retained live authority already equals target | Existing workspace | Authority checks; literal zero payload/native work |
| `retained_workspace_refresh` | Retained live A authority; target B | Existing workspace | Changed paths/ranges only |
| `clone_patch_research` | Trusted same-volume physical seed | Fresh clone | Deferred; outside this specification |

Attempt-014 C08 is:

```text
first_open_fresh_destination
source OS cache warm-or-unknown
fresh full physical output
not incremental refresh
```

The separately reported `721.526 MiB/s` null sink, `431.690 MiB/s` digest,
and `525.611 MiB/s` full materialization values have no retained source,
executable, row, timer, or cache-conditioning custody. They are diagnostic
hypotheses only. This specification reproduces or rejects them.

Fresh full materialization remains:

```text
Theta(F + represented metadata bytes)
```

No SQL, scratch, counter, or sync optimization changes that asymptotic class.
Exact no-op and A-to-B refresh remain the normal high-performance developer
routes.

## 2. Goals and non-goals

### Goals

```text
attribute every materialization nanosecond and operation fact
separate fixed setup cost from sustained per-byte cost
remove repeated Verified SQL control work without trust or concurrency drift
cache the admitted immutable StoreId inside one Engine
remove hidden Store inspection from derived scratch creation
reduce scratch files/setup while preserving authenticated cleanup
record actual native setup/write/metadata/replace/sync facts
batch fresh-tree directory durability only with exact incomplete-state proof
preserve one-pass bounded payload streaming
remain portable above layerfs-os
```

### Non-goals

```text
no canonical byte, ObjectId, FileStateV3, namespace, inode, or metadata change
no FastCDC change
no Store SQLite schema/version/profile change
no WAL, mmap, pool, retry, mailbox, worker, async runtime, or dependency
no pack/carrier/sparse/persistent projection cache
no mounted filesystem, FSKit, FUSE, File Provider, or watcher
no APFS clone disguised as canonical full materialization
no cache purge or controlled-device-cold claim
maximum user input/materialized/oracle file =96 MiB
SQLite Store authority files may exceed 100 MiB through framing/index overhead;
  report them separately and never treat them as user payload
no Stage 1.2 npm/workspace execution
```

## 3. Portable ownership boundary

The dependency shape remains:

```text
layerfs-core
  canonical algorithms and codecs only; no planned change

layerfs-engine
  Store admission, guarded authenticated reads, SQL facts, derived scratch

layerfs-vfs
  universal materialization order, incomplete/complete state, fact deltas,
  requested durability class

layerfs-os
  concrete platform handles, metadata, install, sync, and exact native facts

layerfs-sdk
  thin route and diagnostics propagation

layerfs-eval
  fixtures, custody, timers, equations, statistics, and disposition only
```

No Apple syscall, APFS route name, `libc`, or platform conditional may enter
Core, Engine, VFS, or SDK product logic. Linux, Windows, and WASM adapters use
the same VFS lifecycle and fact vocabulary; only `layerfs-os` implements native
metadata and durability.

An adapter that cannot represent required metadata or durability returns typed
`Unsupported` or `UnrepresentableMetadata`. It never silently weakens
`Complete`.

Performance thresholds in this document are Apple/APFS evidence. Complexity,
memory, trust, cleanup, and fact-accounting gates are portable.

## 4. Engine read optimization

### 4.1 Current defect

A clean Verified object or payload-batch read currently executes:

```text
BEGIN
SELECT trusted_history
data SELECT
ROLLBACK
```

Attempt-014 materialization:

| Root | Data scopes | Total statements | Control/trust statements |
|---|---:|---:|---:|
| R15 | 75 | 300 | 225 |
| R30 | 77 | 308 | 231 |
| R34 | 76 | 304 | 228 |

### 4.2 Long read transaction is not the default repair

A read transaction held while writing and syncing a complete file retains a
SQLite `SHARED` lock. Under `DELETE/FULL/busy=0`, another Engine handle's writer
may reach COMMIT, require `EXCLUSIVE`, and fail immediately with `BUSY`.

Therefore the default implementation is a trust-guarded autocommit data
statement, not one transaction spanning native output.

Conceptual single-object query:

```sql
SELECT a.trusted_history,
       o.kind,
       o.canonical_length,
       o.canonical_bytes
FROM layerfs_authority AS a
LEFT JOIN layerfs_objects AS o
  ON a.trusted_history = 0
 AND o.object_id = ?1
WHERE a.authority_id = 1
```

The authority row must remain observable even when history is dirty or the
object is absent. A guarded inner join or a zero-row result is insufficient:
it conflates dirty history with `MissingObject`. The ordered payload query and
`read_ref` use the same atomic dirty-versus-missing distinction while
preserving ordinal, duplicates, exact first missing position, role, and batch
maximum 64 in one SQLite statement snapshot.

Required behavior:

```text
clean + requested rows present
  -> authenticate/decode/callback in exact order

clean + missing requested row
  -> exact MissingObject at the first missing ordinal

trusted history present
  -> deliver zero callbacks
  -> close statement
  -> run the existing full retained-union scrub
  -> clear trusted history durably
  -> execute the requested read once

missing authority row or malformed authority/object
  -> typed failure; no callback after failure
```

The scrub-followed-by-read continuation is a trust transition, not a general
contention retry. No retry loop is permitted.

Hard concurrency rules:

```text
no process- or Engine-global clean bit
read lock lasts no longer than the current query/batch callback
no transaction survives into file sync or directory sync
no increase in BUSY/LOCKED events
concurrent Trusted commit between batches is detected by a later guard
immutable accepted-root objects may be read across statement snapshots
```

A whole-operation read snapshot is allowed only after a separate quiescence or
journal-lock proof. It is not the first implementation.

### 4.3 StoreId caching

Engine admission must read and validate the exact 32-byte StoreId once, count
that admission SQL, and store it in the Engine instance:

```text
Engine::store_id() = O(1), zero SQL after admission
```

Rules:

```text
StoreId is durable identity authority, not content-addressed object payload
no caller-supplied StoreId is trusted
each newly opened generation re-admits StoreId
path substitution cannot silently rebind a live Engine
compaction/new generation preserves and revalidates StoreId
cache cost is exactly 32 bytes per Engine; no global registry
```

### 4.4 Counter aggregation

Authenticate and role-decode every row, accumulate counters locally per batch,
and merge once for each completed or failed batch.

```text
current counter-lock class O(payload references + structural objects)
target counter-lock class  O(payload batches + structural query scopes)
```

On failure, merge only actual fetched/authenticated/decoded work and preserve
the first failing ordinal.

## 5. Exact SQL accounting

Every executed SQLite statement belongs to exactly one family:

```text
admission
publication
live_verified_integrity
primary_read
scratch_owner_setup
scratch_derived_setup
scratch_operation
scratch_recovery
reconciliation
compaction
```

Count transaction controls, PRAGMAs, schema queries, StoreId reads, trust
guards, query executions, owner writes, and recovery inspections. One
`UNION ALL` execution is one statement; report its requested
`payload_lookup_references` separately.

Hard equations:

```text
all_SQL
  = admission
  + publication
  + live_verified_integrity
  + primary_read
  + scratch_owner_setup
  + scratch_derived_setup
  + scratch_operation
  + scratch_recovery
  + reconciliation
  + compaction

hidden_SQL                         = 0
materialization StoreId SQL       = 0
scratch-triggered Store reopen    = 0
scratch-triggered schema preflight= 0
```

For a measured materialization operation, every nonapplicable family is
reported as exact zero with a named reason; it does not disappear from the
equation. Report statement executions separately from
`payload_lookup_references`. Do not call the latter index probes unless a real
SQLite scan-status observer is installed.

## 6. Derived scratch optimization

### 6.1 Authority classification

```text
canonical Store                     authoritative
accepted RefState                   authoritative
completed live workspace            derived projection with live authority
partial materialization workspace   incomplete and non-authoritative
scratch entries                     derived, disposable, never resumed
scratch owner marker                cleanup authority only
```

Current materialization creates three `DELETE/FULL/FILE` scratch databases,
executes 55 scratch statements for six rows, and opens the Store three extra
times for 33 unreported schema/StoreId statements. The no-hard-link fixture
creates a hard-link table but no hard-link row.

### 6.2 First admitted repair

```text
cache StoreId once
create one physical DiskTable
use fixed DiskNamespace prefixes for hard links, native authority, topology
retain current owner/recovery durability
do not write hard-link rows unless ref_count > 1
count setup, operations, observation, cleanup, and recovery exactly
```

Do not relax scratch durability in the first repair.

### 6.3 Crash equation

Current source establishes:

```text
process crash during materialization
  -> partial workspace has no Complete authority
  -> scratch entries are never resumed
  -> stale exact-owned scratch is authenticated and removed
  -> incomplete owned workspace is removed
  -> later materialization restarts from canonical Store
```

This makes lighter derived-row durability eligible for a conditional second
repair, but not a blind `FULL -> OFF` change. If the database containing the
owner row becomes corrupt, recovery cannot authenticate deletion and must
preserve residue.

Conditional lighter durability requires either:

```text
a separately durable exact owner marker plus disposable derived storage
or another proved cleanup authority independent of corruptible derived rows
```

Required crash tests cover owner creation, schema creation, first/middle/final
entry, cleanup, wrong StoreId, malformed schema, foreign file/symlink/directory,
locked live scratch, and terminal residue. If any fails, retain consolidated
`DELETE/FULL` scratch and report its measured cost.

## 7. Portable projection facts

VFS currently infers some native work while Apple setup happens before VFS
counters exist. Add one constant-sized cumulative portable observation:

```text
ProjectionFacts
  workspace_setup_calls / optional wall
  temp_create_calls
  workspace_marker_write_calls / workspace_marker_write_bytes / optional wall
  content_write_calls / content_write_bytes / optional wall
  metadata_value_write_calls / metadata_value_write_bytes / optional wall
  aggregate_native_write_calls / aggregate_native_write_bytes /
    inclusive-report-only optional wall
  metadata_validate/apply/verify calls / optional walls
  regular_file_sync attempts/successes/failures / optional wall
  directory_sync attempts/successes/failures / optional wall
  recovery_marker/content_temp/post_hardlink file-sync class counts
  staging/root_parent/install_parent/dirty_tree/final_root directory-sync
    class counts
  replace attempts/requested/prior/ambiguous / optional wall
  cleanup attempts/residue / optional wall
```

Rules:

```text
counts are mandatory for a supported operation
timers may be explicitly Unavailable outside measured adapters
Unavailable is never zero
facts support checked cumulative before/after deltas
fact storage is O(1); no event log or path vector
setup facts are observable immediately after open_workspace
attempts and failures remain visible on error paths
```

A snapshot available only from a successfully returned workspace is
insufficient. `open_workspace` must expose facts on failure without adding an
event log, for example:

```text
open_workspace_observed(...)
  -> Result<(Workspace, ProjectionFacts),
            OpenWorkspaceFailure { error, facts }>
```

An equally small driver-level cumulative counter with a checked before/after
delta is acceptable. Failed marker creation, staging creation, and setup sync
attempts must remain visible.

Do not create per-syscall traits. Keep one `ProjectionDriver` and one
`ProjectionWorkspace`; extend their observation and requested durability
contract minimally.

## 8. Fresh-tree directory durability

Current Apple fresh construction performs at least:

```text
recovery-marker file sync
staging-directory sync
root-parent sync
temporary content-file sync
destination-parent sync after file rename
final directory/root sync after metadata
```

All calls and exclusive walls must be observed before removal.

The generic fresh-tree route may request:

```text
ImmediateDirectoryDurability
DeferredToIncompleteTreeBoundary
```

For deferred fresh construction:

```text
construct private temp
apply and verify exact metadata
for an nlink=1 regular file, sync the final temp exactly once
rename into an IncompleteDerived tree
finish all children and directory metadata
sync each dirty directory once, bottom-up
revalidate root binding
only then install Complete authority
```

For a hard-link group, preserve the accepted ordering unless a separate test
proves restrictive flags are safe before linking:

```text
construction sync before representative rename
create all aliases
apply final restrictive metadata/flags
post-alias representative sync
then directory barriers
```

Every namespace mutation dirties its containing directory: regular rename,
symlink creation, hard-link creation, directory creation, unlink, and directory
metadata mutation. Sync every dirty directory once after its final mutation,
bottom-up. Complete remains forbidden until all barriers, root metadata, final
root sync, and root-binding revalidation succeed.

Any rename ambiguity, post-visibility error, or directory-sync failure leaves
`IncompleteDerived` or typed durability ambiguity. It never installs Complete.

Standalone/live refresh retains immediate parent durability until separately
qualified. The general `atomic_replace` contract is not weakened.

## 9. Metadata and output path

Apple metadata internal stages become:

```text
validate requested metadata once before mutation
apply complete permissions/xattrs/ACL/mtime/flags
verify exact complete metadata once before install
sync file once
rename
verify stable identity and exact metadata once after install
```

Retain exact-name-only `com.apple.provenance` filtering, supported xattr/ACL/
BSD-flag limits, restrictive flag ordering, no-follow handling, and unsupported
metadata refusal.

The current payload path remains the default:

```text
SQLite borrowed canonical BLOB
  -> one identity hash
  -> role/header decode
  -> payload slice
  -> 1 MiB BufWriter
  -> native write
  -> one regular-file sync
```

Measure canonical bytes exposed/hashed, payload bytes delivered/buffered, write
calls/bytes, and partial/zero-write behavior. Do not add `writev`, preallocation,
`copyfile`, direct I/O, clone, or another buffer without a focused owner proof.

## 10. Expected file ownership

```text
crates/layerfs-core/
  no planned changes

crates/layerfs-engine/src/
  lib.rs          guarded reads, StoreId cache, counters/timers
  refs.rs         guarded read_ref
  scratch.rs      admitted StoreId, consolidated namespaces, recovery facts
  integrity.rs    existing scrub compatibility only

crates/layerfs-engine/tests/
  store_and_publication.rs
  faults_and_reopen.rs

crates/layerfs-vfs/src/
  driver.rs       ProjectionFacts and generic durability request/outcome
  lib.rs          checked fact deltas and operation diagnostics
  materialize.rs  scratch consolidation, deferred fresh-directory barriers
  workspace.rs    facts and Complete/IncompleteDerived authority transitions
  refresh.rs      preserve immediate semantics and consume real facts
  resolver.rs     no materialization change

crates/layerfs-os/src/apple/
  workspace.rs    actual facts, fresh-tree sync policy, fault mapping
  metadata.rs     validate/apply/verify consolidation
  ffi.rs          only if a genuinely missing reviewed wrapper is proved

crates/layerfs-os/tests/
  apple_stage1.rs

crates/layerfs-sdk/src/
  lib.rs          thin fact propagation only

tools/layerfs-eval/src/
  stage1_materialize.rs  one minimal attribution/acceptance module
  main.rs                thin command routing

poc/
  this controlling specification
```

Do not add `read_session.rs`, another trait hierarchy, benchmark framework, or
new representation unless the selected implementation cannot fit clearly in
the listed shared owners.

## 11. Complexity, memory, CPU, and resource gates

For logical file bytes `F`, represented metadata `M`, paths `P`, extents `E`,
and inode population `I`:

```text
time
  Theta(F + M)
  + O(P log I)
  + namespace/metadata visits
  + fixed workspace/scratch/durability setup

source SQL
  O(structural objects + ceil(payload references / 64))

authentication
  Theta(canonical bytes fetched), exactly one identity pass per object

scratch disk
  O(P + hard-link/topology edges)

working memory
  O(tree height + 64 descriptors + metadata chunks + output buffer
    + bounded Engine SQLite page cache + bounded scratch SQLite page cache
    + O(1) ProjectionFacts)
```

Hard bounds:

```text
individual product buffer                   <=1 MiB
operation Q high-water                       <=8 MiB
operation Q terminal                         =0
payload batch references                     <=64
24 and 96 MiB product-process RSS            <=32 MiB
primary Store connection                     =1
query readers                                <=2
materialization scratch connections           <=1
isolated materialization total SQLite connections <=2
retained-reader campaign total SQLite connections <=3
FD high-water                                <=24
FD terminal                                  =baseline
Store/scratch connections terminal            =0
owned temp/journal/sidecar residue             =0
all-extents Vec                               forbidden
complete namespace in memory                  forbidden
source-sized Vec                              forbidden
worker/async/background thread                 forbidden
busy spin/retry                               forbidden
```

CPU structural gates:

```text
identity-hash bytes     = authenticated canonical bytes exactly once
digest-sink hash bytes  = F only for warm_authenticated_digest; separate field
role decode passes      = fetched rows
payload delivered       = logical output + represented metadata values
counter locks           = O(batches + structural query scopes)
no digest-sink hash inside complete full-materialization timer
```

Report per-row user/system CPU, fixed CPU at zero bytes, and CPU ns/MiB. After
subtracting fixed CPU:

```text
candidate CPU ns/MiB at 96 MiB
  <= 1.25 * candidate CPU ns/MiB at 24 MiB
```

This CPU scaling gate applies to the complete public full-materialization arm;
the attribution-only arms report their own applicable CPU facts separately.

Complete diagnostic:

```text
wall <30 s
user + system CPU <25 s
rows serial
network 0
```

## 12. Attribution diagnostic and control custody

The accepted `f3dd4a3` executable has no complete attribution timers or native
facts. Preserve it and attempt-014 as immutable historical correctness/wall
evidence, but do not pretend it can emit the new measurements.

M1 creates and freezes a distinct instrumented A/B control:

```text
historical control
  = f3dd4a3 + attempt-014
  = immutable accepted correctness and wall evidence

instrumented A/B control
  = f3dd4a3 behavior plus attribution-only facts/timers
  = one clean commit and one frozen release executable
  = byte/root/SQL/native/sync/trust/work parity with historical control

candidate
  = frozen instrumented control plus the selected product optimizations
  = identical observation instrumentation
```

Freeze the instrumented control executable before product repair. Compare the
candidate to that executable, not to a rebuilt or uninstrumented historical
binary. Use exact sizes:

```text
0 MiB   fixed-cost witness
24 MiB  primary latency witness
96 MiB  sustained-bandwidth witness; maximum allowed file
```

Before freezing it, run one M1 parity mini-campaign. Attempt-014 itself is not
the timing control because its C08 cache class differs. Use the preserved
`f3dd4a3` executable and the instrumented executable under identical 24 MiB
fixture, same-open primer, fresh destination, oracle, cleanup, and command
semantics:

```text
pair 1 historical instrumented
pair 2 instrumented historical
pair 3 instrumented historical
pair 4 historical instrumented

8 separate append-only primer rows
8 adjacent measured rows
n=4 p50/p95 estimator from section 15

instrumented p50 <= historical p50 + max(1 ms, 3%)
instrumented p95 <= historical p95 + 1 ms
bytes/roots/SQL/native/sync/trust work exact
preferred parity campaign wall <5 s; hard wall <10 s
```

Each executable invocation owns its Engine, performs its primer once, records
the primer separately, then measures its paired row. The candidate comparison
uses the frozen instrumented control; attempt-014 remains historical evidence.

For each size and arm:

```text
open one Verified Engine
complete required scrub outside the sample
run one untimed full primer to a fresh destination
verify and remove the primer destination
run three measured rows under the same open Engine
use a new empty destination for every full row
verify exact bytes/metadata and remove immediately
```

Every primer is a separate append-only warmup row, included once in complete
campaign wall and excluded from every measured operation and measured-row
wall. For the three attribution observations, sort ascending and report
`p50 = position 2`, `p95 = position 3`, plus every raw value.

Attribution arms:

```text
same materializer source traversal -> null sink
same traversal -> digest sink
native durable output only
complete public SDK -> VFS -> Engine -> Apple materialization
```

The null/digest path must prove the same structural objects, payload references,
authentication bytes, and batches as materialization. A resolver-cache route is
not an admissible substitute, and an evaluator copy of the traversal is
forbidden. `native_durable_output` must stream a sealed deterministic source of
exact length and digest with a buffer no larger than 1 MiB; no source-sized
`Vec` is permitted.

Control-attribution population:

```text
4 arms * 3 sizes * 1 warmup       =12 warmups
4 arms * 3 sizes * 3 measured rows=36 measured rows
```

Freeze an interleaved arm/size order in `schedule.json` before readiness; four
serial arm blocks are forbidden.

Every attribution row uses two independent labels:

```text
operation_label = one exact taxonomy label from section 1
source_conditioning = same_open_after_primer
controlled_device_cold = false
incremental_refresh = false
```

Preferred diagnostic wall is under 15 seconds; hard wall is under 30 seconds.
No unchanged-source rerun for favorable noise.

### 12.1 Required artifacts

```text
environment.json
fixture-manifest.json
schedule.json
preregistration.json
readiness.json
rows.jsonl
summary.json
summary.md
campaign-time.txt
failure-ledger.json
source-manifest-control.json
source-manifest-candidate.json
source-manifest-historical.json
executables.json
commands.json
```

Each source manifest freezes the exact Rust/Cargo file population, per-file
hashes, aggregate digest, commit, and dirty-tree state. `executables.json`
binds separate immutable control/candidate paths and SHA-256/BLAKE3 values; the
candidate build must not overwrite the frozen control. Rows may reference
these receipts by digest.

Every row binds the arm and control/candidate identity; dirty tree `false`;
source commit, Rust and Cargo manifest digest, executable SHA-256/BLAKE3;
fixture/root/size; schedule/preregistration digest; exact argv; environment and
APFS identity; pair, order, cache taxonomy, primer receipt, and warmup identity;
product operation wall; exclusive and nested diagnostic timers; Engine/
scratch/projection facts; CPU/RSS/Q/FD/connections; byte/auth/sync equations;
oracle; cleanup/residue; and exact status/error. Rows are append-only. The
summary retains every raw value, estimator rule, fixed/slope model, paired
wins, target ladder, resource closure, artifact hashes, and all preserved
failures.

## 13. Exclusive timer and counter contract

Every field declares one of `exclusive` or `inclusive_report_only`. Only
leaf-exclusive regions enter the operation equation. A parent region either
stays report-only or reports `exclusive = inclusive - named children`.

Engine:

```text
connection mutex wait
trust guard / scrub continuation
nonpayload query prepare and step
payload query prepare and step
identity authentication
role decode
payload callback dispatch overhead
counter merge

payload callback total = inclusive_report_only
```

The callback contains native sink work, so callback total is excluded from the
exclusive sum. Compute dispatch overhead as callback total minus exclusive
content write, flush, and any other exclusive sink work. SQLite row-step timing
must pause while identity authentication and role decode run.

Scratch:

```text
StoreId/schema inspection
file/schema/profile creation
owner establishment
derived get/put
storage observation
close and required operation-owned scratch cleanup/recovery
```

Projection:

```text
workspace root create/open
recovery marker write
recovery marker fsync
staging directory sync
root parent sync
name preflight
content temp creation
content write
content flush
metadata validate
metadata apply
metadata preinstall verify
content file sync
atomic rename/reconcile
postinstall identity/metadata verify
install parent sync
root metadata
final root sync
authority/topology completion
```

Workspace setup may contain marker/staging syncs, and authority completion may
contain scratch cleanup. Those parents are inclusive report-only unless their
children are subtracted. Explicit destination cleanup is outside the product
operation.

Timer equation:

```text
product_operation_wall
  = sum(all leaf-exclusive Engine regions)
  + sum(all exclusive scratch regions)
  + sum(all leaf-exclusive projection regions through Complete return)
  + operation_residual

abs(operation_residual) <= max(0.5 ms, 1% of product_operation_wall)

row_wall
  = product_operation_wall
  + exact_oracle_wall
  + explicit_cleanup_wall
  + row_residual

campaign_wall
  = setup/reset/open rows
  + separate primer/warmup rows
  + measured row walls
  + final closure wall
```

The throughput numerator uses only `product_operation_wall`, matching
attempt-014. Oracle and explicit cleanup remain measured in the row; setup,
open, and primer are separate rows. None can improve or degrade materialization
MiB/s, but all enter complete campaign wall.

Hard counter equations:

```text
fetched rows = authentication passes = role decode passes
canonical bytes fetched = identity-authentication bytes
content payload bytes delivered = F
metadata payload bytes delivered = M
total canonical payload delivered = F + M
native content bytes written = F
workspace marker bytes written = exact recovery-record bytes
scratch database bytes written = separately observed
metadata native bytes written = separately observed or typed Unavailable
payload batch maximum <=64
canonical Store writer transactions =0
canonical publication COMMITs =0
canonical CDC bytes =0
scratch transactions = separately observed and nonzero when used
FullFallback = rematerializations =0
actual regular-file and directory sync classes exact
hidden SQL = hidden native calls =0
Q terminal =0
destination exact before cleanup
terminal residue =0
```

Arm applicability:

| Fact family | Null sink | Digest sink | Native durable output | Complete public full |
|---|---|---|---|---|
| Engine fetch/auth/decode | Applicable | Applicable | `NotApplicable` | Applicable |
| Identity-hash bytes | Applicable | Applicable | `NotApplicable` | Applicable |
| Digest-sink hash bytes | `NotApplicable` | Exactly `F` | `NotApplicable` | `NotApplicable` |
| Source-plan scratch | Same as public source traversal when used | Same as public source traversal when used | `NotApplicable` | Applicable when used |
| Native content/metadata/sync | `NotApplicable` | `NotApplicable` | Applicable | Applicable |
| Complete authority/oracle | `NotApplicable` | `NotApplicable` | Native-only oracle | Exact public Complete |

The hard equations apply only to their declared arm. The complete public arm
must close every equation. Every row carries its taxonomy operation label plus
`source_conditioning = same_open_after_primer`; null/digest/native-only rows
must not be labeled fresh-destination full materialization. `NotApplicable`
and `Unavailable` never become zero.

## 14. Fixed-cost and bandwidth model

For each source/arm:

```text
T0  = p50(0 MiB)
T24 = p50(24 MiB)
T96 = p50(96 MiB)

slope_ns_per_MiB = (T96 - T24) / 72
sustained_bandwidth_MiB_s = 1e9 / slope_ns_per_MiB
modeled_T(size) = T0 + size * slope_ns_per_MiB
residual_24 = T24 - modeled_T(24)
residual_96 = T96 - modeled_T(96)
predicted_T100 = modeled_T(100)
```

The model is valid only when:

```text
T96 > T24
slope_ns_per_MiB >0
abs(residual_24) <=max(2 ms, 5% of T24)
abs(residual_96) <=max(2 ms, 5% of T96)
```

The 96 MiB result is empirical. The 100 MiB value remains predictive.

## 15. Before/after acceptance campaign

Preserve one exact control executable before candidate work. Build the candidate
once after source freeze. Both arms use identical sealed fixtures, schedule,
cache classification, destination semantics, oracles, and durability.

Paired acceptance applies only to the complete public
`same_open_warmed_source_fresh_destination` path. Null, digest, and native arms
are attribution controls, not product-acceptance substitutes.

For each size `0/24/96 MiB`, run four adjacent balanced pairs:

```text
pair 1 A B
pair 2 B A
pair 3 B A
pair 4 A B
```

Interleave sizes:

```text
pair 1:  0, 24, 96
pair 2: 96, 24,  0
pair 3: 24,  0, 96
pair 4:  0, 96, 24
```

Population:

```text
24 measured product rows
24 paired warmups
preferred complete wall <15 s
hard complete wall <30 s
```

For the four measured A/B observations per size, sort ascending and report:

```text
p50 = arithmetic mean of positions 2 and 3
p95 = position 4
minimum, maximum, and all raw values retained
```

Freeze this estimator and the complete pair/size order before readiness.

Candidate additionally requires:

```text
wins >=3/4 adjacent pairs at 24 MiB
wins >=3/4 adjacent pairs at 96 MiB
fixed-cost p50 <= control +1 ms
candidate p95 <= control p95 +1 ms unless it reaches a higher absolute class
no semantic, CPU, memory, connection, sync, or residue regression
```

“Reaches a higher absolute class” means the candidate passes that class's p50
and p95 gates at both 24 and 96 MiB. A p50-only promotion never waives a tail
regression.

After candidate acceptance, run one exact-source workspace closure, one release
build, zero-row readiness, and one final Stage 1.1 47-row regression campaign.

## 16. Performance targets

### 16.1 Absolute throughput classes

| Class | 24 MiB p50 | 24 MiB p95 | 96 MiB p50 | 96 MiB p95 | Disposition |
|---|---:|---:|---:|---:|---|
| 375 MiB/s | 64.000 ms | 70.400 ms | 256.000 ms | 281.600 ms | minimum useful |
| 400 MiB/s | 60.000 ms | 66.000 ms | 240.000 ms | 264.000 ms | intermediate; continue |
| 450 MiB/s | 53.333 ms | 58.667 ms | 213.333 ms | 234.667 ms | **primary terminal target** |
| 500 MiB/s | 48.000 ms | 52.800 ms | 192.000 ms | 211.200 ms | strong/stretch |
| 800 MiB/s | 30.000 ms | 33.000 ms | 120.000 ms | 132.000 ms | research only; not required |

Terminal optimization PASS requires all correctness/resource gates plus:

```text
24 MiB p50 >=450 MiB/s
24 MiB p95 >=409 MiB/s equivalent (<=58.667 ms)
96 MiB p50 >=450 MiB/s
96 MiB p95 >=409 MiB/s equivalent (<=234.667 ms)
fitted sustained bandwidth >=500 MiB/s
fitted fixed cost <=20 ms
```

Relative to attempt-014's 24 MiB p50 `73.002 ms / 328.759 MiB/s`, the
primary target means at least `19.669 ms` less wall, `26.9%` lower latency, and
`36.9%` higher throughput. The 500 MiB/s stretch means at least `25.002 ms`
less wall, `34.2%` lower latency, and `52.1%` higher throughput. The 96 MiB
fixture validates that the gain is sustained rather than a small-file timing
artifact.

Prospective product goals:

```text
24 MiB full                    450 MiB/s primary; 500 MiB/s stretch
96 MiB full                    450 MiB/s primary; 500 MiB/s stretch;
                               600 MiB/s research
```

The null-sink `650/750 MiB/s` and digest `400/500 MiB/s` values are diagnostic
planning thresholds only in M2 because their motivating values were
uncustodied. M2 reports them without determining PASS. They may become
candidate source-path gates only if the instrumented-control evidence freezes
them prospectively before candidate measurement.

`800 MiB/s` is never a hard Stage 1.1M requirement for 24 MiB fresh durable
full materialization. It is reported only if exact source, sync, authentication,
timer, CPU, memory, and independent raw audit all close.

A sub-1-ms miss remains a numerical miss and may be separately labeled
`NONMATERIAL_MICROVARIANCE`; it is not silently converted into PASS.

### 16.2 Real warm lifecycle gates

```text
exact managed no-op
  p50 <=5 ms
  zero payload/native/CDC/transaction/COMMIT work

retained A-to-B refresh
  zero rematerialization
  changed paths/ranges only
  same-size p50 <=25 ms
```

Large-file throughput targets do not apply to many-small-file Stage 1.2. That
campaign reports files/s, fixed cost, per-directory syncs, and byte throughput.

## 17. Fault and correctness matrix

Engine:

```text
clean guarded object and batch
ordered duplicates and missing middle object
dirty history delivers zero callbacks before one scrub
Trusted commit immediately before a guarded query
Trusted commit between guarded batches
writer COMMIT while reader is between batches
paused callback proves lock window is one batch only
no transaction survives into native sync
corrupt object still fails identity
partial callback failure preserves exact counters
StoreId exact across compaction/new generation
ambiguous scrub never returns data
```

Scratch:

```text
cached StoreId creation executes zero Store SQL
one table isolates all namespaces
no-hard-link and hard-link topology exact
crash at owner/schema/first/middle/final row/cleanup
locked live scratch preserved
wrong StoreId/malformed/foreign/symlink never removed
terminal scratch/journal/connections zero
conditional disposable profile spills while memory remains bounded
```

Portable projection fault driver:

```text
temp create, short/zero/error write, metadata, file sync
rename before/after visibility and lost ACK
postvisibility before directory sync
directory sync and post-sync before Complete
two files: two temp syncs, two renames, one directory barrier
nested directories sync once each bottom-up
root revalidation before Complete
immediate live refresh durability unchanged
fact mutation rejects hidden/missing syncs
failed open exposes marker/staging/setup-sync attempt facts
hard-link cuts before representative rename, after representative rename,
  between aliases, before restrictive metadata, after metadata, after final sync
```

Apple/APFS:

```text
all setup/file/directory sync classes counted
metadata/xattr/ACL/flags/provenance exact
collision/no-follow/stable-identity reconciliation
owned incomplete recovery; foreign entries preserved
terminal staging/temp residue zero
```

## 18. Fast implementation sequence

```text
M0 freeze this spec, fixtures, artifact schema, estimator, and zero-row schedule
M1 add attribution facts/timers only; run the 8-row identical-conditioning
   historical/instrumented parity mini-campaign; freeze the instrumented control
M2 run one 36-row control attribution and prospectively freeze any source gates
M3 cache StoreId and remove hidden scratch Store inspections
M4 consolidate scratch namespaces; retain FULL owner/recovery first
M5 aggregate counters and implement guarded autocommit reads
M6 add portable ProjectionFacts and exact Apple observations
M7 implement deferred fresh-tree directory barrier only if sync wall is material
M8 consolidate Apple metadata passes only if measured
M9 decide conditional scratch durability only after crash proof
M10 focused touched-crate checks/tests during iteration
M11 one final workspace fmt/check/test/clippy closure
M12 one release candidate build and zero-row readiness
M13 one paired 0/24/96 acceptance campaign
M14 independent raw artifact audit
M15 one final Stage 1.1 regression campaign
```

### 18.1 Executed milestone ledger

| Milestone | Result | Exact disposition |
|---|---|---|
| M0 | PASS | Spec, fixtures, estimator, custody and schedules frozen. |
| M1 | PASS with preserved miss | Eight-row parity closed. Attempt-004's `0.489333 ms` p95 excess is `NONMATERIAL_MICROVARIANCE`, not a numerical threshold PASS. |
| M2 | PASS after append-only repairs | Exact 12 warmups + 36 measured rows; all earlier attempts retained. |
| M3 | PASS / retained | StoreId cache is exact; no further tuning after its independently measured sub-3 ms effect. |
| M4 | PASS | Derived namespaces share one authenticated scratch database while retaining DELETE/FULL cleanup and recovery. |
| M5 | skipped | Guarded-read route stayed below the user-authorized 3 ms prospective floor; no target weakening. |
| M6 | PASS | Portable projection facts, disjoint leaves, failed-open facts and exact Apple accounting implemented. |
| M7 | PASS / retained | Fresh IncompleteDerived install-parent barrier deferred; live refresh and hard-link order unchanged. |
| M8 | skipped | Metadata owner is sub-3 ms. |
| M9 | retained DELETE/FULL | Lighter scratch durability not pursued under the floor. |
| M10 | PASS | Focused crate tests/checks and fault cuts passed. |
| M11 | PASS after preserved failures | One full closure exposed a Trusted/Verified lifetime test defect; smallest invalidated serial scopes and downstream scopes close on `0403ea7`. |
| M12 | PASS | One clean current-source release and one zero-row readiness receipt. |
| M13 | harness PASS; campaign skipped | Absolute 24 MiB gates already fail, so paired acceptance would not admit the candidate. |
| M14 | PASS | M7 raw rows, Engine owners, native owners and final attempt-015 were independently audited. |
| M15 | PASS | Current-source Stage 1.1 attempt-015 closes 47/51/34 in `13.430358958 s` accounted wall. |

### 18.2 Current-source release and regression

```text
source commit                 0403ea7166b332c5ddcb7b6cf04f60a0610fd5db
dirty tree at build/run       false
release SHA-256               347746fc4ec7e78654a1b041bbe97f2ec8945bb286e537df43de270d71a44d53
release BLAKE3                1cb5b6b208d2a24ac94ffb43e4db30317c4082a300504c62c1f268119d06038b
attempt-015 rows SHA-256       b6f815dbe2c9bed34e8c9e539568c0d1b7faf44012a84a461b9d2625950ec01a
attempt-015 summary SHA-256    ea22611b90bcf28569e668e0ce3d3beed91206bc0c6116aa102fe6fc6180187a
complete wall equation        13,430,358,958 = 9,077,248,419 + 4,353,110,539 ns
RSS / Q high / Q terminal     28,770,304 / 8,388,607 / 0 B
connections high / terminal   2 / 0
FD baseline / terminal        5 / 5
owned residue                 0
```

The current regression's phase equation is `74,236 fetched = 74,236 identity
authentications = 74,236 role decodes`; its publication equation is `34
transactions = 34 COMMITs`, with zero rollback and zero authentication,
role-decode, new-object or incumbent-object equation failures. The final
receipt and raw hashes are under
`poc/evidence/stage1.1m-current-source-closure-20260825`.

On any intermediate miss:

```text
preserve the failure
identify the largest exclusive owner
apply the smallest shared repair
run the focused proof
continue
```

Do not rerun unchanged source for favorable noise.

## 19. Completion

The completion predicates remain:

```text
closed attempt-014 evidence remains immutable
all hidden SQL/native facts are zero
instrumented control parity and custody are exact
guarded reads preserve trust and writer coexistence
scratch recovery and terminal cleanup are exact
Core/canonical bytes remain unchanged
memory/CPU/connection/buffer/Q gates pass
24/96 MiB primary throughput target passes
fixed/sustained model closes
one final Stage 1.1 regression campaign remains exact
three independent raw audits return PASS
```

Otherwise the disposition is `REVISE`, the raw failure is retained, and the
optimization continues without weakening targets or correctness.

The executed result satisfies the correctness, attribution, portability,
resource, custody and final-regression predicates, but not the 24 MiB or fixed
performance predicates. The terminal status under current authority is:

```text
REVISE_NO_AUTHORIZED_OWNER
terminal_pass=false
```

Continuing the Verified performance loop requires explicit authority expansion
for a new >=3 ms owner, bounded multi-object authentication, a canonical or
durability change, or a changed target. None is inferred. A separate explicit
`TrustedLocalDev` class may be investigated prospectively only if it remains
distinct from Verified and retains close + Verified reopen/full scrub at every
publish/export/share boundary.
