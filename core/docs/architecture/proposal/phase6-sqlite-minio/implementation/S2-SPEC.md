# S2 — admitted daemon metadata and scalable proof

Status: Current planning checklist; no release candidate exists.

Parent source: `bfbf48ea7694e8450bd47a3bf2c297fdac36badc`. Owner authorized
S2 on 2026-10-02 and refined the simplified route and qualification sequence on
2026-10-02. Local S2 runtime edits remain uncommitted/unverified; this revision adds no
implementation, test, benchmark or non-pausing PASS. Owning issue:
[#296](https://github.com/Ephemeral-AI-Lab/layerfs/issues/296).

Read the prospective [S2 tests and comparison plan](S2-TEST.md). Owner direction,
2026-10-02: satisfy existing [SP1 test.md](sp1/test.md) obligations, pass required
S2 focused/concurrent qualification, then compare all seven existing
`fs-bench-pro` families against the selected Core/Phase B #286 baseline. This
is a dependency sequence, not a new aggregate gate or benchmark framework. Preserve unchanged qualifying proof receipts with their
original identity/scope; do not routinely rerun unaffected passing work.

## Simplified Exec-to-Commit route

Three owners implement the route using ordinary functions and concrete types:

```text
workspace_api.exec(&workspace_id, command)
    -> ordinary command syscalls -> real FUSE
    -> S2 scoped live state + private replacement backing

workspace_api.commit(&workspace_id)
    -> existing Commit orchestration admits/captures one stable view
    -> one C1 + SP1/C2 preparation worker streams final state into storage
    -> thin C5 authority conditionally publishes
    -> S2 installs the known result and retires eligible captured state
```

S2 owns the mutable namespace, final spans, backing and capture lifetime. C1
owns canonical construction and existing edit boundaries; SP1/C2 owns encoding,
reuse, placement, dependencies and storage readiness. The existing Commit call
path owns ordering and the known outcome; C5 retains publication authority.
“Commit coordinator” describes that ordinary call path, not a new daemon,
service, registry, factory, framework or second metadata authority. Keep focused
modules rather than merging the responsibilities into a large implementation.

Use one owned capture context carrying Workspace/incarnation, capture identity,
generation/revision, project/Branch, expected head/base root, storage profile and
associated save. Carry these facts through preparation/publication/install; do
not independently reconstruct them at every handoff. Authorization checks remain
at their trust boundaries. Keep one pending Commit per Workspace incarnation
through uncertain custody. This admission need not prevent ordinary filesystem
activity once the separately qualified successor-view mechanism exists.

C1 emits canonical objects directly to the SP1/C2 consumer; no complete object
list, full namespace copy, intermediate export or second send pass. Use one
construction producer with bounded input/output windows and backpressure. SQL
statements/ownership end before command execution, C1 construction, codec work,
provider I/O, publication handoffs or unbounded cleanup. SQLite owns indexed
state; do not introduce a custom B-tree, disk pager or resident population registry.

The simplified route preserves ordinary command behavior: FUSE observes
supported syscalls rather than recognizing a command or filename. Exec can
leave filesystem mutations even on a nonzero command exit. This does not claim
universal POSIX support; unsupported operations still fail explicitly.

## Shared state and source lifetime

One persistent daemon-owned SQLite engine uses shared tables whose Workspace
keys and queries include `(workspace_id, incarnation)`. Opening inserts a small
context with its selected root, not another database/table set or eager inherited
namespace import. Under the current measurement topology the engine runs on the
macOS host with the SDK/coordinator; Linux owns FUSE/daemon/workload execution.
Engine ownership and execution locality are distinct.

Logical facts include context/root/revision, inode/name state, final file spans,
backing-source identity and custody, handles, captured views and operation-scoped
prepared/edit scratch. Index point/range lookups, directory-cookie keyset pages,
dirty/captured traversal and reclamation. This is not a mandate for one table per
fact: use indexed columns where they avoid duplicated authority. Temporary
per-Commit rows live in shared scoped tables, never per-Commit schemas.

Replacement bytes reside in immutable private backing sources; SQL stores
lengths, intervals, offsets and ownership, not a second full file-payload copy.
Successful writes replace affected final intervals. Overlapping historical
writes need not remain a Commit replay log. Preserve accepted bytes, checked
arithmetic, sparse/truncate semantics and existing normalized C1 boundary policy.

Prefer reclaimable state and a partial eligibility index on backing-source rows
over a duplicate general retirement queue when the source reference/pin/custody
facts suffice. A separate retirement table requires a concrete multi-phase or
heterogeneous ownership need. In either representation, atomically claim a
reclaimable source before releasing SQL ownership for deletion, and prevent new
references from racing with that claim. Eligibility requires no live/capture/read
references or pins and known disposition. Unknown completion is quarantined;
partial reclamation outcomes are retained. Reclaim in bounded batches. This does
not authorize deletion/GC of committed MinIO packs or history.

## Captured and live successor views

Non-pausing Commit is the intended integration target, with an explicit separate
qualification gate. One shared engine alone does not prove it. Capture generation
G as a stable selected metadata/extent view and retain required immutable backing.
Later filesystem mutations create successor state G+1; they cannot overwrite
facts still selected by G. The representation must provide indexed visibility
without copying the whole namespace or replaying a growing generation chain;
retained row versions versus persistent view roots remains a reviewed implementation
choice, not an already implemented schema claim.
Prefer indexed SQL row/view state in the existing shared engine. An alternative
custom live metadata tree/pager needs the concrete capability/work evidence
required by repository policy; the inherited canonical C1 tree is not a second
mutable authority. Exact capture DDL and reclamation predicates are reviewed
implementation inputs, not frozen by the conceptual table sketches.

```text
captured G -> C1/C2 preparation -> storage-ready -> publish G
    |                                                  |
    +-- retained source pins                           v
live G+1 -> later reads/writes/rename/unlink -> install published base G
                                                   preserve G+1 changes
```

Installation clears only captured dirty state, advances the appropriate base and
preserves successor names, sizes, extents and backing references. The inherited
read plan combines the selected base with final replacement intervals; bounded
read buffers and metadata pages must not materialize the complete file. A serialized
first integration witness may qualify its stated scope, but cannot pass the
non-pausing gate. Require deterministic actual overlap, captured/live byte
isolation, open-unlink lifetime, rename/truncate handling, Branch conflicts,
publication-known/install-failed custody, cancellation and bounded reclamation
before claiming successor support. Construction stays single-producer.

## Per-pack storage and publication order

The mutable S2 tables and immutable SP1 canonical catalog are separate logical
responsibilities. They need no per-Workspace databases and must not duplicate
canonical pack bodies as a second namespace authority. During Exec, private
backing and mutable SQL state change. During Commit the preparation worker uses:

```text
private SQL body reservation
    -> seal bounded file-payload pack
    -> MinIO PUT -> known ACK
    -> bounded SQL body-completion / locator / use / dependency registration

SQL canonical metadata bodies + mappings + pools
    -> required referenced payload custody already known

finish_storage -> owned storage-ready root
    -> C5 stage -> storage publication ACK -> conditional Branch-head update
    -> known publication -> scoped Workspace installation -> bounded retirement
```

MinIO stores regular-file whole/CDC payloads only. SQL stores directory/inode/
attribute/mapping metadata, pooled values, placements and retained dependency
facts. Placement follows typed logical use, including identical IDs used in both
domains. No giant all-output transaction; no SQL/ownership lock spans a PUT. A
known ACK is required before ready metadata references depend on payload.

Keep transitions `Captured -> StorageReady -> Published -> Installed` explicit.
Definite refusal preserves prior accepted state; uncertain provider or SQL
completion retains custody without guessed retry/resend/adoption/deletion. A
known publication with failed installation is not “nothing committed”. No
cross-provider atomic transaction, crash durability, WAL, sync, recovery service
or automatic retry is introduced.

## Scale targets and retained capability limits

A 3 GB inherited base with 100,000 files is a prospective scale target: bootstrap
by root reference, fetch inherited state lazily, use indexed FUSE lookup/cookie
pages, and process only dirty paths for a shallow edit. Fully listing 100,000
entries or reading 3 GB still pays for those required operations. Baseline and
candidate must use the same declared operation and complete data. No current
public SDK/FUSE SP1 receipt qualifies this scale.

Creating/rewriting 3 GB of private replacement bytes differs from opening a 3 GB
inherited base: it exceeds the current 1 GiB backing policy and may require
additional retained sources during Commit. Keep that policy and record the case
as blocked/unrun until an explicit supported resource profile is selected; do
not silently raise it or count inherited bytes as local backing. Population-cap
removal alone does not satisfy C1 unfinished-draft/scratch or physical-memory
gates. The 4 GiB file contract, path/name/attribute/format limits, finite identifier
spaces and explicit quotas remain separate decisions. Bounded callbacks/pages/
frames/windows stay; filling a reusable window must lead to progress rather than
an artificial total-population refusal.

## S2 dependency slices

S2a owns `phase6-live` Engine/creation/write/rename/edit/construction storage
admission and their external SQLite/C1/C5 tests. Replace the independent 512
population, directory-child, dirty-inode and edit-spool ceilings with indexed,
paged storage, with no application-defined total metadata byte/record ceiling by
default. Owner clarification, 2026-10-02: the goal is theoretically unbounded total
load at bounded resident memory, not a larger fixed supported population.

`LAYERFS_METADATA_QUOTA_BYTES` is optional explicit user storage policy; absent
means no application database-size cap. Query actual SQLite page size/capacity;
apply `max_page_count` only for an explicit quota. No default 16 MiB total DB cap.
A working-memory/scratch window does not bound total database, file, upload,
namespace or edit-stream size. Retain MEMORY journal, synchronous OFF, 2 MiB
pager cache, mmap disabled, 256 simultaneous handles, 128 KiB callback and existing
construction/packing windows. These windows are reused with backpressure; no
whole-population RAM state or transaction is permitted. Actual SQLite temporary
and journal memory, kernel cache, source-file storage and physical capability
still require owning proofs. Finite provider/format address spaces remain explicit
capabilities, not an infinity claim. Storage exhaustion and explicit quotas fail
truthfully; no automatic retry, WAL or durability expansion.

Exit gates: actual SQLite quota refusal leaves no inode/name/handle/revision
change; writes do metadata preparation before backing-file creation and preserve
accepted bytes under definite refusal; uncertain backing/SQL completion remains
quarantined. CREATE plus issued handle is one transaction. More than 512 live
children and dirty rows work with real C5 page allocation; more than 512 exact
final edit spans construct through C1. Directory cycle checks use constant memory
and a linear ancestor traversal, without a population cap. Required owning locked
checks and an exact LOC/published checkpoint complete the dependency slice.

S2b owns scalable independent scenario manifests/proof traversal and prospective
live scale cases. Replace the verifier's resident 512-entry manifest/frontier
with a paged expected catalog and work queue. Seal workload/expected data before
execution. Prove shallow one-file updates at increasing namespace populations
using actual construction/query/provider counts, full semantic/history/cleanup
checks and the existing 15 s child / 9.5 s separate-proof bounds. Unknown cache
observations remain INELIGIBLE; S2b locality diagnostics are not themselves the
subsequent seven-family speed/storage comparison or release-speed admission.
Preserve all failures and do not repeat an unchanged arm.

S2c — daemon-lifetime SQLite ownership (owner refinement, 2026-10-02). Own
Engine SQL context/schema/lifetime, daemon Workspace bootstrap/retirement and
all affected indexed queries. One shared daemon database and shared tables with
Workspace/incarnation-prefixed keys replace per-Workspace database/schema/cache
startup. Hold engine ownership only for bounded SQL operations, never commands,
construction/provider I/O or full cleanup. Preserve row isolation and pending/
Unknown custody. Opening references a selected immutable root without eagerly
importing its full namespace; inherited lazy-load correctness remains required.

Exit gates: one actual database initialization per daemon; repeated Workspace
open/close without schema recreation, per-Workspace cache growth or lifetime-row
leaks; scoped key/query isolation and stale incarnation refusal; bounded retirement
with retained pins/Unknown. Bootstrap work counts and timer boundaries must cover
actual required mount/authentication/base-selection work. Do not enable multi-Exec
or multi-Workspace solely from this refactor: remove existing lifecycle locks and
prove command/submission ownership and deterministic overlap in their own milestone.

Inherited loading/import follows these dependencies. Deepest-270 successor Exec
speed remains deferred; deep correctness and ordinary shallow-edit locality stay
required. Global publication remains the selected thin trusted-daemon API. No
restoration of global namespace certification or a second metadata authority.

## Existing content mechanisms are retained, not reopened

Owner clarification, 2026-10-02: preserve Phase 4.5/Phase B content/localized-edit
work and its receipts. Experimental construction already calls existing public
C1 `construct_stream` for fresh content and `apply_edits` for inherited content.
No shipped C1/C2 changes are selected by S2. Reuse unchanged component/canonical
proof scopes; do not restart CDC, split/join or content-edit implementation or
rerun unaffected passing arms. The remaining gate is integration through the
changed daemon metadata/source adapter, MinIO locator/read/packing adapter and
publication/install/retirement path. Prove affected bytes/head/source work with
focused real-provider cases; existing successful source-specific receipts retain
their status and are not automatically relabelled as new-backend evidence.

Phase B Family7 records a 4 KiB edit in a 10 MiB file with full old/new byte
verification (Exec 18.492959 ms, Commit 18.134292 ms); cache remains INELIGIBLE.
Phase 6 V4c1 has a separate external 1 MiB localized C1/SQLite semantic proof and
small actual MinIO two-head proof. These establish existing working mechanisms,
not a new large-MinIO-file physical-memory or speed claim. Link owning reports:
[Family7](../../../../issues/286/FAMILY7-CHECKPOINT-20260930.md),
[V4c1](RESULTS-V4C1.md). Correct status wording is 'retain proven content algorithms;
qualify the changed end-to-end integration', not 'large-file algorithm unfixed'.

## Source-audited remaining C1 accumulation gate

Owner asks whether apply_edits accepts any size, 2026-10-02. It accepts u64
replacement lengths via bounded EditSource reads and ordered current-result
EditSequence records, including insertion/deletion/overwrite. It rejects raw
streams reaching into earlier replacement bytes; the daemon supplies normalized
final state, not raw WRITE history. Arbitrary ordinary shell prepend/insert still
pays any actual shifted/full-copy bytes its syscalls produce.

Concrete retained limit: core/crates/layerfs-content/src/file/edit/tree.rs defines
EDIT_DEFERRED_LIMIT = 8 MiB - 1 and charge_bytes refuses edit.deferred_nodes above
it. DeferredSink publishes chunk payloads but holds mapping pages; this is a live
unfinished-metadata bound, not an 8 MiB replacement-byte cap. Existing external
edit_bounds.rs explicitly leaves reaching that bound unrun at its fixture scope.
Thus unchanged C1 evidence remains valid within its original scope, but cannot
prove unbounded accumulated load. Withdraw any implication that all remaining
large-edit work is provider requalification alone.

A named C1 accumulation-removal dependency must be designed/source-reviewed and
proved before the full unbounded-edit claim: bounded resident draft state with
scalable backing/finalization, exact canonical partition/root compatibility and
no quadratic scans. Do not remove or raise the guard to claim completion. S2
SQLite population-cap removal alone cannot satisfy this gate. Current code is
unmodified at this C1 site; no new checks, implementation or PASS is claimed.

## Storage parity dependency before large integrated proofs

The historical [SP1 scope](STORAGE-PARITY-SPEC.md) is retained planning material.
Published external strict implementation is
`c838d8d6db85158c6ed9576dd7e8fddf65489704`; evidence checkpoint is
`16407e03ee36e7bde8af66ae3fc12f3b1c197c80` on `codex/phase6-sp1-strict`.
Catalog schema 3 and native wire `P6SP1V2` are integration inputs. These
checkpoints are external published dependencies, not code imported into this S2
WIP. Older schema-2 receipts retain their profile, identities and verdicts.

Keep metadata and storage changes in coherent checkpoints. Finish current S2a
atomicity, integrate published strict interfaces through the shared captured
view, and pass every applicable requirement in existing [test.md](sp1/test.md)
before the seven-family speed/storage baseline campaign. Component PASS and raw
cache-ineligible observations do not satisfy the public Exec/FUSE/history or
physical-resource gates. Existing qualifying component receipts may be reused
only with matching unaffected identity/scope and explicit attribution. FULL-only
is historical prototype scope, not the selected Phase 6 profile.

Optional follow-up: qualified catalog resolution can combine descriptor/location/
logical-use/body-domain facts without weakening visibility and epoch checks. The
published 70-call small metadata observation implies an estimated reduction of
18 calls, not a measured saving. A stronger bounded group-authorization wave
requires an explicit stable eligibility lifetime separate from S2 capture and its
own private/public/cache refusal proofs. Neither optimization is implemented or
a prerequisite for beginning S2; keep it in the existing catalog owner rather
than introducing another service.

The seven-family campaign must use the existing family registry, public surfaces,
matched workload/cache/topology and qualified baseline identities. Capture both
speed and complete storage axes, including host SQLite and provider allocated
bytes, with separate setup/proof/cleanup/resource scopes and every nonpassing
cell. Owner selection, 2026-10-02: Core/Phase B [#286](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286)
is the primary comparator; retain the original
[seven-family checkpoint](../../../../issues/286/SEVEN-FAMILY-CHECKPOINT-20260930.md).
Exact baseline source/receipt bindings are maintained in [S2-TEST.md](S2-TEST.md).
Its cohort contains mixed source identities and
functional evidence with numeric speed INELIGIBLE; its historical times do not
become eligible by selection. Exact per-case baseline revision/receipt binding,
cache eligibility and semantic/timer/topology applicability remain prospective
preregistration requirements. A fresh numeric control needs a common qualified
baseline/candidate harness and eligible cache evidence. A strict-split public
runner adapter is not yet
implemented; do not invent a runnable CLI or claim a speedup. Resolve family
compatibility and baseline applicability in [S2-TEST.md](S2-TEST.md), then collect
each required arm once. No historical receipt relabeling, missing-family omission, worker/timeout
relaxation or storage-for-speed trade hidden behind a single headline.

Owning implementation issue: [#296](https://github.com/Ephemeral-AI-Lab/layerfs/issues/296), native sub-issue of #293, created 2026-10-02.
