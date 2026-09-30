# R0 source and resource accounting audit

> **Status: Current planning checklist; no release candidate exists.**
> Implementation preparation for #287, 2026-09-30. Source inspected:
> `7edddbdb8e8512627aed0ed42533ef099d802384` on
> `codex/issue287-implementation`, in
> `/Users/yifanxu/.codex/worktrees/issue287-implementation/layerfs`.
> Source observations below describe that commit. The proposed resource contract
> requires implementation and its stated exit proofs before enablement; its
> arithmetic is not measured memory, a benchmark PASS, or release admission.

This audit owns Server/C1/C2/C5 resource composition. The integrated R0 owner
retains Bridge contract allocation and rollout acceptance. Product source was
not changed, tests and benchmarks were not run, and this audit creates no
physical-provider qualification. Root and Core AGENTS, the
[scenarios](../../../../scenarios.md), packet
[README](../../architecture/proposal/bounded-workspace-implementation-20260930/README.md),
[ROLLOUT](../../architecture/proposal/bounded-workspace-implementation-20260930/ROLLOUT.md),
[LAYOUT](../../architecture/proposal/bounded-workspace-implementation-20260930/LAYOUT.md),
and [SERVER](../../architecture/proposal/bounded-workspace-implementation-20260930/SERVER.md)
were read. Applicable contracts include
[C1 file construction](../../../../docs/roadmap/0.1/0.1.7/component-decoupling/file-content.md),
[finalized allocation handoff](../../../../docs/roadmap/0.1/0.1.7/component-decoupling/finalized-object-handoff.md),
[C2 storage](../../../../docs/roadmap/0.1/0.1.7/component-decoupling/object-storage.md),
[admission/persistence](../../../../docs/roadmap/0.1/0.1.7/component-decoupling/admission-and-persistence.md),
and the source-backed [C5 description](../../architecture/16-history.md).
Historical architecture pins remain historical when they differ from this pin.

Affected scenarios are SC-01 through SC-08: population-independent resident
state, actual work on payload/metadata/cleanup, exact old selections, admitted
overlap, catalog progress, and definite/Unknown custody. This audit does not
activate a larger file/count tier or the deferred 10,240 case.

## Current admission and authority

| Source at the inspected commit | Observed behavior | R1 consequence |
| --- | --- | --- |
| [Service handler](../../../crates/layerfs-server/src/service/handler.rs#L42), lines 42–62 and 202–211 | One atomic writer counter per configured Store; every non-read-only request holds one permit for the whole request. Reads use a separate process-wide count of two. | Pure C5 requests currently consume the same service permits as long C2 work. Changing this is an explicit admission-contract change. |
| [Operation predicates](../../../crates/layerfs-bridge/src/contract/request.rs#L378), lines 378–464 | `content_mutation()` and `metadata_mutation()` already classify operations exhaustively. Fork, CommitStaged, AddLayer, DiscardStage and ReserveInodes are metadata-only. | Reuse these predicates. Do not infer a resource domain from opcode numbers or duplicate a second unchecked command list. |
| [Catalog dispatch](../../../crates/layerfs-server/src/service/save/catalog.rs#L130), lines 130–187 | Metadata-only commands call C5 directly and do not invoke `Store::begin_save`. | Removing their C2 service permit does not create a third C2 Save. Their control/engine/response capacity still needs admission. |
| [Persisted C2 reservation](../../../crates/layerfs-storage/src/sqlite/ownership.rs#L120), lines 120–152 | `BEGIN IMMEDIATE`, the persisted writer budget, live private count, lowest-free slot, publication snapshot and inserted Save row are one attempted transaction. Retained private rows count toward capacity. | Preserve default two, supported slot space, and retained-owner refusal. A process-local byte pool cannot replace this cross-process capacity authority. |
| [C2 acquire](../../../crates/layerfs-storage/src/cas/lifecycle.rs#L37), lines 37–74 | Save reservation commits before shared-index clones and codec allocation; failed preparation attempts checked abandonment. | Reserve the prospective byte/connection owners before `begin_save` can create dependent effects. Preserve the existing definite/Unknown cleanup behavior. |
| [C2 wave commit](../../../crates/layerfs-storage/src/cas/lifecycle.rs#L167), lines 167–189 | A SQL wave commits before releasing Store arbitration; no SQL transaction spans subsequent input/preparation. | Keep this bounded transaction ownership. Do not hold C5 admission across C1 construction or an entire upload. |
| [C5 provider](../../../crates/layerfs-history/src/sqlite/rows.rs#L80), lines 80–129; [provider state](../../../crates/layerfs-history/src/sqlite/open.rs#L34), lines 34–52 | One connection behind a nonblocking `try_lock`; one short transaction; writable authority checked before statements. Unknown result quarantines the retained connection. | Catalog contention already has a precise Busy/Unknown boundary. Typed service admission must preserve it, without waiting/retry/refresh. |
| [C5 allocator](../../../crates/layerfs-history/src/sqlite/allocation.rs#L31), lines 31–84 | A reservation advances one scope high-water row inside its transaction. It burns the whole range, checks authority and uses checked signed-SQL arithmetic. The code requires the new high-water to be strictly below `i64::MAX`. | Keep real scope-wide allocation and exact burns. Initial/refill range policy is a Workspace caller policy, not recycled serials or a new allocator. |
| [Native acceptor](../../../crates/layerfs-server/src/host/acceptor.rs#L29), lines 29–34, 132–164; [session bound](../../../crates/layerfs-bridge/src/contract/request.rs#L38), lines 38–50 | Native sessions equal writer setting plus two reads. Each admitted connection receives a 2 MiB thread stack. There is no catalog-reserved channel/dispatch context. | A catalog request may use a free read-side session while two Saves occupy sessions, but this is not protected capacity when all sessions are occupied. |

The current C2 schema is 10
([policy](../../../crates/layerfs-storage/src/policy.rs#L58), line 58); C5 is schema
1 ([catalog](../../../crates/layerfs-history/src/sqlite/open.rs#L54), lines 54–69).
They are separate files and authorities. This resource milestone does not change
canonical formats, Store roles, page sizes, writer settings or C5 schema.

## Existing bounded primitives and remaining populations

These are reusable source mechanisms. Reuse does not certify their new complete
composition.

| Primitive | Current bound/source | Required replacement or integration |
| --- | --- | --- |
| Sorted-tree allocation lease | [C1 sorted Budget](../../../crates/layerfs-content/src/filesystem/sorted/budget.rs#L21), lines 21–49, 69–110 reserves before growth and returns credits on actual owner drop. | Keep its narrow operation responsibility. Compose its simultaneous encoded/decoded/output capacities with the outer Save lease. |
| Workspace atomic byte charge | [backing Budget](../../../crates/layerfs-workspace/src/backing/budget.rs#L15), lines 15–40 | Preserve its separate daemon/Workspace owner. A Server lease must not borrow this budget or make it a whole-host/RSS claim. |
| Reference ordering runs | [RunStore](../../../crates/layerfs-content/src/filesystem/references/runs.rs#L36), lines 36–64, 145–179: 16 KiB merge buffers, 64 MiB ordering bytes, at most 32 tiers, pending/input/output overlap accounting and retained lookup scans. | Reuse size-tiered merges and monotone scans; bind a real owned backing on the Server route. The current [filesystem composition](../../../crates/layerfs-server/src/service/save/filesystem.rs#L83), lines 83–88, supplies `None`. No allocation-error spill fallback. |
| Physical run ownership | [OrderingBacking](../../../crates/layerfs-content/src/filesystem/references/backing.rs#L34), lines 34–76; [FileBacking account](../../../crates/layerfs-content/src/filesystem/references/backing.rs#L79), lines 79–111, 211–239 | Keep reserve-before-append and checked release. Bounded run handles/failed-owner paths, allocated blocks and cache domains must be included; logical byte counts alone do not prove residency. |
| Prepared row slots | [RowSpool](../../../crates/layerfs-content/src/filesystem/rows/spool.rs#L37), lines 37–107: fixed 32-byte slots and 48-byte header, declared-count table before writes, capacity refusal. | Reuse slot addressing and sealed replay. Replace [whole directory decode](../../../crates/layerfs-content/src/filesystem/rows/spool.rs#L296), lines 296–316: it allocates the full payload and a binding vector together. |
| Mapping cache | [PageCache](../../../crates/layerfs-content/src/file/mapping/read.rs#L94), lines 94–137, 145–146: default 64-page ceiling, caller-owned `make_room_for`. | `insert` must enforce admission itself. [Localized edits](../../../crates/layerfs-content/src/file/edit/tree.rs#L175), lines 175–181, currently insert without `make_room_for`. Preserve the pages the current pass must use via explicit owner/window lifetimes. |
| Mapping read batches | [mapping traversal](../../../crates/layerfs-content/src/file/mapping/read.rs#L470), lines 470–543: page acquisition is in 32-page waves. | Whole `level`/`next` frontier vectors still grow with the level width. Replace them with a bounded traversal frontier or paged work queue; a wave-sized decode window does not bound that population. |
| Mapping drafts | [EditObjects](../../../crates/layerfs-content/src/file/edit/tree.rs#L109), lines 109–122 has draft/parent/detached/committed maps; its draft byte ceiling is at lines 26–33. | Externalize exact draft/reference/commit populations with narrow typed state records. Preserve split/join/finality and v1 canonical partitions. A draft ceiling does not account every associated map. |
| Reference reduction | [ReferenceReducer](../../../crates/layerfs-content/src/filesystem/references/reduce.rs#L54), lines 54–102 and 168–179 | Pending rows are bounded, but declared-new serials are a resident set and touched rows become a full vector. Use ordered/paged exact state instead of moving repeated whole-population scans to SQLite. |
| Prepared parser | [decoder](../../../crates/layerfs-bridge/src/contract/prepared_stream.rs#L330), lines 330–364 | A directory count creates a full `Vec` before final aggregate name-count refusal. Check remaining declared names/bytes and resident allowance before each binding and deliver one checked binding at a time. Bridge grammar stays under the coordinated owner. |
| Prepared service admission | [receive](../../../crates/layerfs-server/src/service/save/prepared.rs#L57), lines 57–79 and 113–125 | Counts are currently limited by `ordering_bytes / 1024` and sink conversion creates another binding vector. Separate shape, resident and disk credits; raising RAM does not authorize a wider shape. |
| C2 private cleanup | [cleanup](../../../crates/layerfs-storage/src/sqlite/cleanup.rs#L21), lines 21–78: 128 locator/hint rows or one pack per transaction, final Save-row deletion last. | Reuse exact Save ownership and this paged cleanup. Include singleton BLOB/journal shape and failed retained slot; no quota refund on an unproved removal. |

The new metadata-only `construction_state/` adapter and its runtime SQL are target
files in LAYOUT, not source present at this pin. C1 receives typed get/put/remove,
ordered page and seal/release ports; it imports no SQLite, filename, mount or
command policy. Server/C2 owns the real SQLite adapter. Values are at most 8 KiB;
dirty flushes are at most 128 records and 64 KiB. A directory is a binding run,
not a single arbitrarily wide indexed value.

## Simultaneous first-party allocation freeze proposal

Call this the **Issue287 Server resource admission v1** proposal in R0. It is a
deployment resource profile, not a canonical format or guessed wire capability
number. All quantities below use MiB = 1,048,576 bytes. The implemented constructors
must reject incompatible configuration before admitting affected work.

Keep the packet's 72 MiB standard Save owner and 176 MiB process first-party pool.
The owner's capacities are acquired once before dependent allocation/effects,
then transferred with the allocation. Reservations are not eagerly resident
bytes. Returning an API result, draining a pending wave, or cloning a cache does
not return credit while the bytes remain alive elsewhere.

| Per standard Save subowner | Proposed capacity | Allocation obligation |
| --- | ---: | --- |
| Encoder / decoder arenas | 16 + 1 MiB | Actual [encoder constant](../../../crates/layerfs-storage/src/encoding/codec.rs#L55), lines 55–57. Decoder is lazy at lines 469–487 but its possible overlap is reserved. |
| Private indexes and publication clones | 8 MiB | Shared old index, every active private copy and new publication copy coexist. No clone before capacity admission. |
| Canonical wave / encoding / groups / packs | 12 MiB | Include drained objects, already accepted next object, encoded alternatives, pending groups, open lane tails and selected pack assembly. |
| C1 frontier / ordering / indexed windows | 8 MiB | Bound encoded and decoded representations, merge inputs/output buffers, caches, draft page and recursion/height context simultaneously. |
| Provider / cache / result ownership | 12 MiB | Include canonical result owners and copies retained by their consumers, pack/group/dependency caches and locator ownership. |
| Transport / records / bookkeeping | 1 MiB | Frame/body/terminal/headers are capacity owners; no result population encoding. |
| Reserved allocator/container/transient margin | 14 MiB | Must be substantiated by actual allocation paths and the external real allocator observation, not assumed to cover an unknown population. |
| Total | 72 MiB | Named subowners sum 58 MiB; 72 − 58 = 14 MiB margin. |

For R0 integration, select these additional first-party subceilings inside the
176 MiB pool: 8 MiB Store-shared index allowance, 6 MiB fixed service state,
1 MiB protected catalog work/terminal state, and a distinct 64 KiB protected
control ring. With two standard Saves and two ordinary 8 MiB read owners:

```text
2 * 72 + 2 * 8 + 8 + 6 + 1 + 64 KiB / MiB = 175.0625 MiB
176 − 175.0625 = 0.9375 MiB unassigned pool margin
```

This makes the packet's previously unspecified Store-shared/service remainder
explicit. These are capacity ceilings to implement and qualify; current source
is not claimed to fit them. Failed owners keep their retained allocations and
completion credits. A generic singleton uses the separately declared 96 MiB
owner and fresh role-specific arithmetic before construction. Two such owners
do not fit this standard profile even before shared/control/read costs; no hidden
quota expansion follows a refusal. A raised persisted writer setting is still
bounded independently by this byte pool.

Keep strict reads byte-admitted at the provider acquisition boundary. A
`ReadWindowPermit` names count, canonical-output capacity, locator/pack/dependency
capacity and the exact result/cache lifetimes. The provider verifies locator and
role size claims before decode/output growth, authenticates all returned objects,
and returns exact demand order/cardinality. The standard 8 MiB read lease requires
bounded grouped chunks whose actual overlapping owners fit it. The existing
[32 MiB returned-canonical ceiling](../../../crates/layerfs-storage/src/policy.rs#L161),
lines 161–163, is retained for independent compatibility callers under an explicit
larger lease/profile; it cannot be described as covered by an 8 MiB lease.
[Current reads](../../../crates/layerfs-storage/src/cas/read.rs#L93), lines 93–109,
check total locator lengths before packs/decode, but provide no caller-specific
window. The stored candidate collection and dependency/cache owners also count.

The metadata-index claim needs a separate compiled-layout check.
[PoolIndex](../../../crates/layerfs-storage/src/encoding/pool/index.rs#L126), lines
126–138, charges `size_of::<(i64, u32)>() + 8` per entry rather than actual B-tree
allocation. At a compiled tuple size of 16, 131,072 entries charge 3 MiB; this is
conditional source arithmetic, not an observed allocator bound. The content
index has a [704 KiB declared arena bound](../../../crates/layerfs-storage/src/encoding/delta/candidates.rs#L54)
with a compiled-size assertion at lines 115–123. Thus three live old-shared/private
copies plus a publication copy already multiply both indexes. C2
[acquisition](../../../crates/layerfs-storage/src/cas/lifecycle.rs#L43), lines
43–52, and [publication](../../../crates/layerfs-storage/src/cas/lifecycle.rs#L312),
lines 312–330, actually clone them. Keep exact equality/window/ordinal policies;
qualify B-tree/container overhead before the proposed 8 MiB index allowance can
become an enforced supported profile.

The existing pending-batch algorithm also matters:
[push](../../../crates/layerfs-storage/src/cas/batch.rs#L43), lines 43–68, drains a
wave and retains the new object at the same time. The standard wave target is
less than 4 MiB at [policy lines 124–144](../../../crates/layerfs-storage/src/policy.rs#L124),
and a larger singleton is valid in an empty batch. Do not add drained-wave bytes
twice, omit the next retained object, or call an oversized singleton a standard
4 MiB wave. Maximum canonical object is 16 MiB and singleton framing adds up to
4 KiB at [policy lines 170–173](../../../crates/layerfs-storage/src/policy.rs#L170).

## SQLite accounting and protected headroom

Current [C2 configuration](../../../crates/layerfs-storage/src/sqlite/connection.rs#L31),
lines 31–47, sets MEMORY journal, synchronous OFF, temporary MEMORY, foreign keys
and busy timeout zero. Verification at lines 57–71 checks journal/sync/FK/busy;
it does not verify temp_store. Cache/mmap fields are read observables at lines
94–117, without writes/enforcement. Current
[C5 configuration](../../../crates/layerfs-history/src/sqlite/open.rs#L181), lines
181–206, has the same persistence settings and no cache/mmap/heap guard.
No `hard_heap_limit` enforcement exists under `core/crates/` at this source.

Freeze the following new strict profile as implementation obligations:

- Dedicated Server process assembly establishes one process-wide 32 MiB SQLite
  hard heap guard before Store/catalog/scratch activity. Configure and read back
  with the pinned safe SQL interface, and verify every participating connection's
  resource profile. A missing, ignored, incompatible or externally changed guard
  makes this profile Unsupported. Do not change another embedding process's
  unrelated SQLite users implicitly.
- Use explicit verified C2 cache 2 MiB, scratch cache 512 KiB, admitted read cache
  1 MiB and mmap zero, keeping MEMORY journal/temp, OFF sync and zero busy timeout.
  Keep current persisted page-size choices; setting a cache is not a page-layout
  experiment. Include TEMP/database caches, statement shapes, allocator metadata,
  rollback journal, dirty pages and BLOB bindings in the engine ledger.
- One standard profile contains one logical Store and one writable catalog. C2
  retained connection credits are `2 * 3 = 6 MiB`; the single active Store SQL-wave
  credit is 12 MiB; read connections are `2 * 2 = 4 MiB`; scratch/global is 4 MiB;
  protected catalog is 4 MiB. Total `6 + 12 + 4 + 4 + 4 = 30 MiB`, leaving 2 MiB
  guard margin. More Stores/catalogs need a new composed profile before admission.
- Credits describe scheduling and enforced statement/row/byte/dirty-page shape.
  They do not physically reserve a per-connection heap compartment. The 4 MiB
  catalog headroom is unqualified until real-engine bounds establish it under
  occupied Save/read/scratch owners. A global heap cap alone cannot reserve it.
- NOMEM, SQL/refusal/COMMIT and cleanup outcomes keep the existing known/Unknown
  classification. A spontaneous engine allocation failure after earlier accepted
  effects is not an upfront refusal; retain its exact custody. Do not raise the
  guard, retry a transaction or select another journal/provider.

SQLite documents cache_size as a per-connection page-cache suggestion, while its
heap guard covers the library heap across connections. The hard-limit PRAGMA can
lower a limit and returns readback; it cannot raise it. These provider semantics
require subprocess-isolated guard tests because restoring a higher limit through
SQL is unavailable. Readback alone does not demonstrate catalog protection.
See [SQLite cache/guard documentation](https://sqlite.org/pragma.html#pragma_cache_size)
and [heap-limit API contract](https://sqlite.org/c3ref/hard_heap_limit64.html).

The [SaveConnectionProfile source](../../../crates/layerfs-storage/src/cas/store.rs#L169),
lines 169–179, explicitly records that pinned safe rusqlite lacks the needed
connection cache-spill observation. Do not add a new unsafe/FFI site, patch a
dependency, or report a different connection's spill count as the Save's count.
Independent external provider evidence remains separate and explicitly scoped.

## Smallest complete catalog separation path

Name the first product checkpoint **R1a: typed catalog admission and real Save-slot
progress**. It proves one necessary part of R1 and leaves strict memory/physical
enablement pending until the following submilestones pass.

1. Split the Service's admission implementation into focused files, with typed
   `C2SavePermit`, `C5CatalogPermit` and read/control ownership. Reuse the existing
   exhaustive mutation predicates after authorization/request validation. Pure
   metadata commands acquire one short permit keyed to the actual catalog
   authority/incarnation, not a separately counted permit per StoreAccess alias.
   A full catalog allowance refuses before executing a mutation.
2. Provide the pure catalog dispatch with only its C5/response authority. Preserve
   exact body EOF, grant/scope/identity/deadline checks. No pure catalog path calls
   `begin_save`, clones content indexes or allocates a codec arena.
3. For composite Stage/Commit/import, acquire C2 construction ownership only for
   its real content phase. Complete C2, retain fixed outcome state, release the
   C2 permit/arena owner, then admit the short C5 phase. Do not hold a C5 permit
   through prepared input, C1 construction, blocked output or a channel wait.
   Internal stage and CAS custody remains exact if later admission fails.
4. Native control protection requires the coordinated Bridge owner to freeze an
   authenticated channel-purpose/protected-dispatch rule and one fixed control
   execution owner. A spare generic accepted socket can be occupied by bulk work
   and does not meet this rule. The control executor is not a third C2 writer or
   another construction producer. Until implemented/proved, report native
   full-session catalog progress unavailable rather than claiming it from direct
   Service progress.

The R1a proof must occupy actual C2 Save slots, not only Service counters. Existing
[admission tests](../../../crates/layerfs-server/tests/admission.rs#L102), lines
102–169, pause the first SaveFile body read. But
[content::mutate](../../../crates/layerfs-server/src/service/save/content.rs#L23),
lines 23–54, spools the whole SaveFile body before `begin_save` at lines 67–69.
Those barriers therefore establish Service admission only.

Use two authorized valid prepared StageChanges requests with distinct Workspace
identities. [stage](../../../crates/layerfs-server/src/service/save/catalog.rs#L236),
lines 236–248, begins Save before consuming the prepared body. Pause each request
with external deterministic stream coordination at body consumption, and inspect
the real Store's two non-null `saves.active_slot` rows before proceeding. While
both requests remain paused:

- Require ReserveInodes to finish through the same public Service handler and
  writable real C5 catalog. Verify returned scope/start/count against an
  independent high-water oracle and the actual committed allocator row.
- Require a third content operation to refuse at admission before reading its
  body or inserting another Save. Verify exactly two retained C2 Save owners
  remain; catalog work created no content Save/codec owner.
- Consume one Workspace's admitted initial serial block, refill a real block and
  verify disjoint ranges/burned exposure. Exercise read-only catalog authority,
  wrong grant/scope, finite range exhaustion and externally held SQLite contention
  with precise refusal and unchanged intended roots/high-water after a definite
  refusal. No automatic replacement request follows Unknown.
- Finish the allocator response before releasing either StageChanges source.
  Then release both, require their exact content/stage outcomes and independently
  verify roots/bytes and complete known cleanup. Barriers and persisted-owner
  inspection establish overlap; sleep, submitted RPC or launched PID do not.
- Repeat the route as a native contract case once its protected channel exists,
  additionally saturating ordinary read/data sessions and proving bounded catalog
  terminal ownership. Native success using an otherwise free read slot does not
  substitute for the saturated protection proof.

All coordination belongs in external tests. Use real Store and C5 provider,
ordinary protocol/source I/O and kernel/provider state. No fake allocator/clock,
fault branch, test-only product hook or benchmark runner is required.

## Named R1 delivery boundaries

| Checkpoint | Owned complete delivery and prerequisites | Exit proof / dependent enablement |
| --- | --- | --- |
| R1a: typed catalog admission and real Save-slot progress | Service admission/phase guards and pure C5 dispatch; existing C5 identity/transaction contract. Coordinated Bridge owns any native channel-purpose change. | Real two persisted Save owners plus successful allocator response before either source resumes; third C2 refusal; exact catalog contention/authority/burn/Unknown custody. Native saturation gap remains explicit until protected route passes. |
| R1b: admitted canonical/cache/wave owners | C2 allocation leases and caller byte/count read windows; C1 cache insertion admission; simultaneous pending/drained/clone/result owners. Depends on R0 capacity/profile freeze. | Exact refusal before dependent allocation/effects, 64-page crossing on ordinary localized edits and reads, ownership transfer/last-release arithmetic, slow consumer plus next-object overlap, independent byte/cardinality/authentication oracle. No new canonical algorithm. |
| R1c: sealed paged construction metadata | Narrow C1 indexed/run contracts and C2 metadata-only SQLite adapter/runtime SQL; charged disk/run/index/release owners; bind real backing in Server composition. Depends on R1b windows. | Indexed ordered pages/128-record and 64 KiB flushes, selection/seal mismatch refusal, bounded fan-in and merge overlap, released versus failed retained owners, known small-path selection before effects, no repeated-prefix cursor work. No empty scaffolding. |
| R1d: paged C1 frontier/draft integration | Replace at least one complete current construction path's full frontier/draft/reference populations with R1c ports, retaining canonical split/join and external v1 vectors. Extend through the required filesystem path before full R1 completion. | Independent v1 roots/partitions and exact graph/effect outcomes; scale surviving extents/directory width/identity counts separately; bounded actual simultaneous allocations and work-driven monotone scans. Moving a quadratic historical scan to disk fails the gate. |
| R1e: SQLite and protected control composition | One declared process guard, verified per-connection profile, phase-scoped engine shape credits and fixed native control owner. Depends on actual R1b–R1d owner inventory and Bridge protection. | Isolated real-provider guard/return/error tests; actual two-Save/read/scratch occupancy while allocator/control completes; NOMEM/COMMIT/cleanup classification; full first-party and engine ledger fits. Unsupported engine headroom keeps strict profile disabled. |

These are responsibility/dependency checkpoints, not arbitrary iteration numbers.
The integrated owner may further split a large row only around a complete public
behavior and its own exit proof. Tests/docs-only work does not count as delivery
of a missing implementation. Each checkpoint needs the mandated commit/LOC/#287
update from the root owner.

Future covering commands, scoped to the frozen checkpoint, include locked
`layerfs-server` admission/history tests, `layerfs-history` allocation/conditional
updates, `layerfs-storage` write_admission/connection_profile/memory_bounds and
affected C1 integration tests. Use explicit test target names and reuse unchanged
proofs. Final full Core tests/examples/fmt/warning-denying Clippy, product boundary
guard and guard self-tests remain required. This audit ran none of these commands;
it does not report them as passed.

## Physical capability and qualification boundary

The inspected tool host reports `Darwin`, `arm64`. This read-only observation does
not provide Linux cgroup-v2 containment, a capable native Linux Store/cache domain,
or a phase-memory observer. The proposed dedicated Server envelope is therefore
unqualified here. It must not be implemented by quietly moving the benchmark
Server or SQLite into Docker. Logical byte/provider tests on Darwin are distinct
from strict physical Server qualification.

Keep the packet's proposed 224 MiB memory.high, 256 MiB memory.max, swap zero,
32 MiB SQLite guard and 16 MiB runtime/stack allowance as separately enforced,
read-back target inputs. `176 + 32 + 16 = 224 MiB`, leaving 32 MiB at the 256 MiB
boundary for accounted file/socket/kernel/cache ownership. This arithmetic does
not prove that successful IO avoids reclaim stalls or OOM. The process/domain
must be created before activity and retain owned Store cache charges across
instances; foreign Store users and VM/host cache layers require inclusion or a
precise unsupported scope. Direct replay/run IO and buffered SQLite need separate
provider evidence.

Linux memory.high induces throttling/reclaim, while memory.max can invoke OOM;
healthy progress requires its own evidence. On kernels offering resettable
memory.peak, reset and read through the same open descriptor for the phase;
opening another descriptor does not preserve that reset observation. Read exact
domain identity, current/stat/event observations and supported reset behavior on
the selected provider. A lifetime peak, heap-only count or sample omitting file
cache is unavailable/incomplete for that proof. The
[kernel cgroup-v2 contract](https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html#memory)
supports this distinction; it does not qualify this host.

Required observations stay separate: first-party reserved/retained capacities,
actual real-allocator requested bytes and omitted allocator overhead, SQLite
engine/cache/journal scope, domain anon/file/dirty/socket/kernel state, successful
control/catalog progress, disk allocated-block high-water, selected old owners and
known/Unknown cleanup. No speed PASS or frozen-source resource PASS is claimed.

#288 owns later benchmark qualification for changed FileSet Save cadence,
construction/paged state, index-clone policy, resource profile and native overlap.
No seven-family, Family2, Family8/9 or new performance row was run by this audit.
Its remaining physical/provider gates are #287 implementation dependencies, not
permission to relabel old receipts or mark #288 complete.
