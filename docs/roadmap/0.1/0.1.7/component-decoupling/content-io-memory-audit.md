# Content I/O and memory audit against v0.1.6

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Companion to the [generic content I/O contract](content-io.md).
Three read-only agents audited I/O/calls, memory/lifetimes and adapter boundaries;
the owner narrowed the target to logical content and database storage. This
document includes only cluster 1/2 findings and the external-cost boundary.
Three follow-up reviews challenged final-only construction, operation-wide memory
and local/remote SQL placement. Their findings below supersede the earlier target
of preserving generic candidate spill: remove payload staging on proven paths,
with multi-edit and namespace ordering proof gaps explicitly retained.
No implementation, build or benchmark was performed.
The follow-up [handoff review](finalized-object-handoff.md) adds producer/consumer
ownership evidence, the singleton capacity/spill targets IO13/IO14, and explicit
acceptance-versus-persistence semantics. These remain source findings and proposals.
The [file-content review and cut list](file-content.md#9-what-to-cut-and-what-to-preserve)
adds coordinate/replay rules, exact COW partition obligations and source evidence
for file-specific reductions. Its three reviewers also checked the resulting draft.
The [filesystem-tree review and cut list](filesystem-tree.md#8-source-backed-cut-list)
adds sorted/direct-inode construction, exact metadata sizing, shared reads and
semantic-only reference ordering. Three reviewers checked source and draft. The
old Workspace bridge is evidence, not the target model. C1's ordering records and
C2's physical value index are explicit remaining temporary-storage owners.

## 1. Source provenance and evidence levels

| Identity | Value |
| --- | --- |
| Peeled v0.1.6 source commit | 44cf748486863ab7c21ca47e731bd88e2b9a7b4a |
| Annotated tag object, not source commit | dbdf0fed6fceba9f72997287eaa7d7ee9ae0fd79 |
| Review HEAD | a8a1ba848429d5f2fbba83c2de22dada8c29def9 |
| Product-path difference | None in crates/ between source tag and review HEAD |

Reproduce the source applicability check:

```sh
git rev-parse 'v0.1.6^{commit}'
git diff --stat 'v0.1.6^{commit}' a8a1ba848429d5f2fbba83c2de22dada8c29def9 -- crates
```

The [release verification record](../../../../../release-notes/0.1.6/verification.md)
identifies the measured revision, product seal and historical source applicability.
Local source links below refer to the unchanged reference product files, not to
the replacement implementation. A source-visible duplicate operation establishes
a removal candidate. It does not establish elapsed-time or RSS improvement.

## 2. Core optimization ledger

| ID | Reference evidence | Proposed cut and verification |
| --- | --- | --- |
| IO1 | [Read interfaces](../../../../../crates/layerfs-content/src/object/access.rs#L3), [ObjectSource/CoreReader](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L923) overlap; batch defaults can loop over point reads | One authenticated read contract, with actual bounded batches. Keep format/policy/identity reservation as explicit input. Count SQL/location/group reads; removing forwarding calls alone is not a round-trip claim |
| IO2 | [owns_batch_demand](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L1035) reads owned spill data, then emit_owned_batch_demand reads it again; [spill get](../../../../../crates/layerfs-layerstack-store/src/objects/spill.rs#L658) allocates and reads | IO10 removes both reads where finalized output eliminates staging. If a specific algorithm still needs temporary access pending redesign, [batched locations](../../../../../crates/layerfs-layerstack-store/src/objects/spill.rs#L620) avoid the ownership-probe payload read. This source alternative is not a target-wide spill requirement or hidden runtime fallback |
| IO3 | [Mixed batch](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L3692) clones an already authenticated persisted object then rehashes it at delivery | Move authenticated results into bounded ordering slots. Preserve required demand order, identity/cardinality checks and external authentication; eliminate only this internal extra copy/hash |
| IO4 | [File length](../../../../../crates/layerfs-content/src/file/content.rs#L60) re-encodes an authenticated payload to decode file state; [small encoding](../../../../../crates/layerfs-content/src/file/content.rs#L69) builds inner then outer bytes | Decode existing payload fields directly; allocate final canonical envelope once; prove byte-identical output |
| IO5 | [Metadata value group](../../../../../crates/layerfs-layerstack-store/src/objects/admission/metadata_values.rs#L112) compresses, clones and decompresses to digest the original body | Digest that exact body before compression; verify identical digest and record/group semantics |
| IO6 | [Tail materialization](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1268) precedes [merged pack assembly](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2388) | Selected target: compatible append with placement before assembly; borrow groups and materialize only selected write once. Preserve groups/locators, short-pack density, visibility and acknowledgement. Seal-once insertion stays a separate qualification, not a runtime mode. Count copied/assembled/written bytes and SQL calls |
| IO7 | [Compact inode lookup batch](../../../../../crates/layerfs-content/src/tree/inode/table.rs#L77) independently descends per key | Share necessary ancestor traversal within the bounded key batch; measure actual node reads for shared and unrelated paths |
| IO8 | [Prepared admission](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L201) reconstructs temporary ID uniqueness containers | Remove only when private constructor ownership proves uniqueness; retain external duplicate-byte/collision validation. A proof obligation, not unconditional deletion |
| IO9 | [Read results](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L3865) clone repeated-ID payload demands | Borrow/share one authenticated allocation while it remains live; preserve duplicate demand delivery and lifetime, without introducing a global cache |
| IO10 | [Finalized complete-file writer](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L758) rejects [reads](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L910); [sorted trees](../../../../../crates/layerfs-content/src/tree/batch.rs#L1) keep unfinished siblings private. Generic [reachability](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L3018) and [multi-edit overlays](../../../../../crates/layerfs-content/src/file/rope/edit.rs#L198) remain elsewhere | Remove generic payload spill/index/reachability/readback on final-only paths. Prove the multi-edit decoded-boundary rewrite before deletion, including canonical partitioning and late input failure |
| IO11 | [Mapping frontier](../../../../../crates/layerfs-content/src/file/rope/build.rs#L185) and [namespace budget](../../../../../crates/layerfs-content/src/tree/batch.rs#L10) give local bounds; [reference events](../../../../../crates/layerfs-workspace/src/changes.rs#L59) require different ordering | Apply limits across all files and namespace changes, consume file results incrementally, share packs/batches and block slow producers. Resolve compact event ordering; no unbounded input/result map or caller-cost omission |
| IO12 | [Incremental BLOB reads](../../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L896), [local session ownership](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2238) and [watermark cleanup](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2530) assume embedded access/authority | Selected remote adapters need grouped bounded reads, atomic writes, enforced writer ownership, read-after-write behavior and unknown-outcome handling. Count network calls/bytes and SDK buffers; never map each local BLOB call or delta link to an RPC |
| IO13 | [Finalized writer](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L822) rejects canonical objects above its 256-KiB slab limit; [admission](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4257) has an isolated larger-object route | Reconcile producer/admission/codec/pack capacities for accepted profiles before allocation. Preserve supported singletons and cross-file batching; no hidden cutoff change, expanded budget or workload reduction. Count the current object beside a blocked outgoing slab |
| IO14 | [RAW singleton pack preparation](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1196) writes a header/object to a temporary file, drops canonical bytes, then allocates and rereads the full pack | Replace this round trip with allocation reuse or explicitly budgeted in-memory assembly. Preserve exact format, selected encoding, collision comparison and supported sizes; qualify simultaneous canonical/pack/driver allocations and actual staging-I/O removal |

```text
IO2 today: ownership probe -> scratch payload read
                              then emission -> same payload read
target:    final-only output -> no payload staging/readback on proven paths
           any retained temporary access must justify its own bounded algorithm

IO3 today: authenticated owned result -> borrowed callback -> clone -> rehash
target:    authenticated owned result -> move into bounded order slot -> borrow

IO6 today: selected groups -> standalone pack -> merged pack
target:    size/placement plan -> assemble selected pack once

IO14 today: canonical -> temporary-file write -> full pack reread -> DB
target:     canonical -> bounded in-memory pack assembly -> DB
```

Existing efficiencies to preserve:

- [Membership/location lookup](../../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L422)
  already returns locators and uses <=128-ID pages, also respecting SQLite's
  parameter limit. Do not add a membership probe before that same lookup.
- [Small predecessor lookup](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L415)
  already batches unique bases; packed read waves group physical demands.
- [Absence epoch](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1295)
  permits skipping only still-valid rechecks. Keep checks when the proof expires.
- Streaming construction and moved finalized batches already exist. Keep bounded
  producer/admission overlap unless measured replacement justifies its removal.
- Base/target hashing and reconstruction are real work. Never delete integrity
  checks at persistent/external trust boundaries under the name of fewer calls.

Direct inode-value output should remain available from cluster 1; migration of
the current Workspace caller's temporary store/read sequence is later integration.
No Workspace-specific call deletion is counted in this ledger's core speed claim.

Completion evidence: [writer finish](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L772)
only flushes a channel, and [admission finish](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4002)
returns final work. Actual final cohort commit follows the
[owner's publication callback](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1338).
Do not time either earlier finish as complete persistence. The handoff contract
preserves final transaction composition and incremental closure, with no additional
commit or full-graph reread required solely by the new API boundary.

## 3. Memory ownership ledger

Distinguish generic candidate storage from file mutation's structural overlays.
FileMutationBatch already streams replacement payloads and sealed nodes; its
deferred map is not a whole-file payload spool. The file proposal targets encoded
draft-node overhead while preserving the existing bounded algorithm's behavior.

Every row states a purpose, capacity scope, lifetime/coexistence and release or
overlimit rule. These are reference limits, not a measured total RSS bound.
Alternative lifetimes and nested reservations must not be added blindly.

| Allocation / owner | Why it is needed and current bound | Lifetime / coexistence | Target action and qualification |
| --- | --- | --- | --- |
| Caller-owned input | Stable bytes, source or edit descriptors; caller's declared size, not a core-enforced whole-input RAM cap | Caller retains through required reads; can coexist with every core phase | Borrow, do not duplicate. Declare caller bytes separately in core-owned measurements and include them in complete-operation claims |
| CDC scanner | [gear.rs](../../../../../crates/layerfs-content/src/file/cdc/gear.rs#L54): 32-KiB input array plus up-to-32-KiB chunk accumulator per scanner | Current chunk and emitted canonical result can overlap | Necessary bounded streaming state; no whole-input collection; keep default scanner profile |
| Whole-file construction | [content.rs](../../../../../crates/layerfs-content/src/file/content.rs#L224): default 128-KiB probe, payload/framing; raw+23 canonical bytes for current role | Probe, retained conversion ranges, scanner and output may overlap | Encode final allocation once; derive accepted-T capacities and conversion coexistence; reject unsupported settings before work |
| File mapping builder | [build.rs](../../../../../crates/layerfs-content/src/file/rope/build.rs#L14): bounded pending fanout, flush at 192, bounded tree level | Per active build; node summaries/output exist alongside scanner | Keep pending/touched paths, not all payload chunks. Bound by declared fanout/height and charge emitted objects separately |
| File edit structural overlays | [DeferredFileObjects](../../../../../crates/layerfs-content/src/file/rope/edit.rs#L198): charged prune threshold 4 MiB, hard limit 8 MiB minus 1; inner [DeferredNodes](../../../../../crates/layerfs-content/src/file/rope/state.rs#L64) is separate | Encoded nodes, decoded lookups, copied bytes, reachability sets and replacement builder/output can coexist | Replace with owned unfinished boundaries after exact partition/finality proof. The map's charged threshold is not total peak memory; count decoded capacity and both join boundaries under a proved byte budget |
| Known-edit equality/replay | [Changed-range comparison](../../../../../crates/layerfs-workspace/src/changes.rs#L2009) precedes re-reading changed replacements | Bounded replacement/base windows and read-plan state; later construction may acquire replacement again | C1 owns required no-op checks with a stable replayable range source. Count both passes and physical read amplification; avoid unbounded undecided-prefix retention and repeated root/plan setup |
| Producer object / partial or blocked slab / queue | [Slab limits](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L40): 256 KiB and 512 objects; [channel](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L397): four slots | Current incoming object can coexist with the batch blocked in [flush-before-push](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L823), queued slabs and the consumer | Reserve the full allowed producer footprint before construction; charge allocation/container capacity. Dequeue releases a slot, not global byte ownership; consumer allowances take over |
| Object fields / direct references | Established role, direct child IDs and bounded physical hints from a checked constructor | Owned or valid borrowed descriptors coexist with canonical bytes until their consumer finishes | Reuse checked fields, keep leaves allocation-free for references, release direct-reference data after needed checks; never retain a whole-operation graph or seen-ID set |
| Namespace update state | [tree/batch.rs](../../../../../crates/layerfs-content/src/tree/batch.rs#L10): 4-MiB reservation, bounded pages and child batch | Decoded nodes, caller changes, reference journal and output coexist | Preserve reservation checks. Input changes and reference accounting require their own charged bounds; no claim all namespace RAM is inside 4 MiB |
| Filesystem inode effects / edge records | [ReferenceJournal](../../../../../crates/layerfs-workspace/src/changes.rs#L59): 65-byte edge records; [FrontierInodes](../../../../../crates/layerfs-workspace/src/changes.rs#L2224): bounded pending map and tiered runs | Addition/removal passes, current records, run readers, subtree-release pages and tree output coexist | Retain bounded ordering semantics, extract only logical fields, remove compact-profile record encode/ID rewrite/redecode. Exact resource/backend/row layout must be qualified without Workspace types; disk/cache costs count |
| Attribute tree construction / reads | [MetadataTreeBuilder](../../../../../crates/layerfs-content/src/tree/metadata/tree.rs#L69) retains unresolved groups; [portable cache](../../../../../crates/layerfs-content/src/filesystem/change.rs#L268) is exact and construction-local | Key/value descriptors, trial encodings, final output, read waves and small cached attribute values coexist | Replace trial serialization with exact checked sizing; share demanded paths and checked values. Preserve existing bounded reuse, partitions and attribute domains |
| Reference canonical read-your-writes data | [DeferredObjectStore](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2656): 6-MiB default content allowance; [bounded output](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L3418) uses 1 MiB | Newly emitted objects retained while construction still needs them; admission output may overlap | Remove generic retention/spill on final-only paths; keep only unfinished algorithm state. Multi-edit rewrite needs proof; no automatic disk overflow in the target |
| Reference scratch index/order/references | [objects.rs](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L49): declared 64-MiB index ceiling, separate order/reference/spill-buffer limits | Operation-owned; multiple structures/connections may coexist | Delete candidate-wide structures with the generic store on proven paths. Distinguish necessary namespace event ordering; any retained mechanism needs explicit bounds and measured I/O/residency |
| Object read demand slots | IDs, locations, order and authenticated outputs; [batch boundary](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L1001) <=128 IDs | Bounded wave plus selected group/base buffers | Enforce byte capacity as well as count; move authenticated results, reuse repeated-ID bytes while live; release after demanded callbacks complete |
| Incoming physical batch | [admission.rs](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L183): <=512 objects; multi-object canonical bytes <=512 KiB; any batch below 4 MiB, allocation-capacity check <=6 MiB | Canonical targets overlap selected encodings/association metadata | Bound count and bytes, preserve planned singleton handling; larger records cannot silently expand ledgers |
| Base and chain buffers | [read.rs](../../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L683): iterative previous/next canonical data plus encoded records | Current base, target and next reconstruction can coexist | Required for delta; borrow existing operands, retain only required chain data. Whole-file 512-KiB canonical closure is cumulative work, not simultaneous versions |
| Chain work envelopes | [delta.rs](../../../../../crates/layerfs-layerstack-store/src/objects/delta.rs#L6): whole depth8, canonical512KiB, owned encoded256KiB; [native](../../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L1408): depth4, raw closure1MiB, encoded384KiB/decoded512KiB work checks | Applied per reconstruction/search scope, not an allocation bucket | Preserve units and checks; configured depth does not enlarge bytes. Ineligible optional base selects FULL; required corruption errors |
| Codec workspace | [pack.rs](../../../../../crates/layerfs-layerstack-store/src/objects/pack.rs#L233): native encoder1MiB/decoder256KiB; Small encoder2MiB/decoder1MiB | Relevant encode/decode step; input/output and dictionary structures also live | Keep bounded static contexts. [Encoder drop before base read](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L649) intentionally prevents overlap; reuse cannot keep both workspaces alive for free |
| FULL/PREFIX alternatives | Native frame bound33,024 B; Small135,168 B, role/profile-specific | FULL and PREFIX candidates can briefly coexist with operands | Necessary for existing actual-cost selection; release losing frame promptly. Derive non-default capacities from exact codec parameters |
| Groups / pack output | [pack.rs](../../../../../crates/layerfs-layerstack-store/src/objects/pack.rs#L6): ordinary64-KiB group/256-KiB pack, bounded record count, supported singleton rules | Selected records/groups coexist with chosen assembled pack and open-tail ownership | Placement first, assemble once. Count source plus destination while copying and any SQL-owned duplicate |
| Reference singleton temporary pack | [prepare_singleton](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1196): temporary file avoids a second resident multi-MiB canonical copy before full pack allocation | Canonical allocation, temporary-file I/O/cache, then assembled pack; driver ownership can overlap | IO14 removes staging only with proven capacity-safe in-memory assembly. Keep identity/collision operands and supported sizes; deleting the file alone does not prove a lower peak |
| Admission bookkeeping | [reservations](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L345): 6-MiB data and 2-MiB scratch/association checks cover several rows above | Session/prepare/cohort; ownership persists across bounded work | Do not add these as independent extra buffers. Keep byte-capacity accounting, write-ownership guarantees and bounded failed-attempt cleanup |
| Store SQLite | [schema.rs](../../../../../crates/layerfs-layerstack-store/src/schema.rs#L510): requested32-MiB cache/connection, MEMORY journal/temp, mmap0 | Connection and current SQL work; dirty pages, journal, statements/binds/BLOBs overlap core buffers | Cache setting is not total heap/RSS cap. Bound query/batch/cohort cardinality, account connection count and qualify SQL allocations; no stronger durability claim |
| Reference scratch SQLite | [spill.rs](../../../../../crates/layerfs-layerstack-store/src/objects/spill.rs#L697): requested4-MiB cache/connection, temp FILE, mmap0 | Scratch-index lifetime, possibly several independent connections | Remove with generic payload/index staging on proven paths. Do not replace it with a temporary payload table in the real DB or claim a whole-operation no-SQL proof before ordering is resolved |
| C2 physical inode-value fingerprint index | Reference [ValueIndex](../../../../../crates/layerfs-layerstack-store/src/objects/metadata.rs#L209): up to 131,072 indexed values, 32-MiB SQLite file ceiling and requested 4-MiB cache; target bounded Store-owned BTreeSet retains same window | Physical pooling/reuse work; charge tree nodes, allocator/split/query transients and decoded groups, including Store multiplicity | [Replacement proof](physical-encoding-and-packing.md#replace-the-disposable-sql-fingerprint-index): same window/ordinal/equality semantics and no CPU/memory/storage regression. Old cache is not total RSS; tuple bytes are not new total memory. Reopen/sync stays timed where required; failed sync invalidates and fails without retry |
| Remote DB adapter | Selected provider's request/response, serialization and SDK buffering; exact limits not chosen | In-flight bounded reads/writes coexist with canonical/encoded batches | Bound bytes and request concurrency before submission; count copies/expansion and runtime buffers. Slow consumption stops production |
| Timer/report | [recording.rs](../../../../../core/crates/layerfs-telemetry/src/timer/recording.rs#L14): <=1024 nodes, depth32, label128 bytes | Operation; collection/attachment/serialization can overlap report structures | Coarse scope counts, disabled path before work, no payload capture. Report output/temporary text has its own owner and release event |

The reference is not evidence that all these owners already fit a single
global memory cap. Missing charges for map/set allocation, reference-journal
growth, simultaneous scratch connections and SQLite temporary allocations are
explicit accounting/proof tasks. Do not call them bounded merely because payload
chunks are small, or satisfy a bound by reducing supported workload coverage.

## 4. Peak memory and concurrency

```text
core-owned peak = max over time of distinct live allocations:
    construction input storage owned by core
  + canonical objects retained anywhere in the operation
  + mapping / namespace / reference metadata
  + any explicitly justified event-ordering storage and I/O buffers
  + base / chain data
  + codec workspace
  + encoded alternatives / groups / pack output
  + session bookkeeping and report data
  + SQLite allocations attributable to that operation
```

Moving one Vec changes owner; it does not create a second allocation. A borrowed
slice does not own a copy. Cumulative decoded bytes describe work, not resident
memory. Reservations often cover the same allocations as codec/batch/pack caps;
count underlying allocations once.

```text
shared core state -----------------------------+
active operation 1 allocations -----------------+
active operation 2 allocations -----------------+--> process attribution
... only explicitly admitted concurrent work ---+
shared connection/allocator/runtime overhead ---+

whole-system accounting additionally includes:
caller/source-provider retention + OS file cache + later adapter/kernel memory
```

State the operation concurrency and live connection count explicitly. One
construction producer is not a bound on all other operations. Initialization's
declared parallelism is a separate multiplication. Reuse existing resource owners
and simple capacity checks; this is not a new memory service or global pool.

The target operation streams many files through shared admission/pack allowances
and consumes their roots and logical summaries incrementally into namespace construction. Per-file
bounds do not permit an unbounded file list, result map, task fan-out or retained
trace per file. Demand-driven namespace/range reads also stay bounded. The selected
compact reference-ordering reducer is documented; its exact backing/quota and
resource proofs remain. Preparation and sorting costs cannot disappear from
complete-operation accounting. The [implementation review](implementation-plan.md#5-memory-and-disk-ownership)
distinguishes all remaining RAM/disk owners from generic payload staging.

For a supported cutoff T, qualify the maximum simultaneous storage across
construction/conversion, FULL encoding, DELTA encoding, reconstruction and
singleton pack/SQL handoff. Derived capacity must fit declared limits before work.
Do not scale work or memory budgets merely because a larger T/depth was requested.

### What bounded can honestly mean

1. Enforced application allocation/structure limits under declared input and
   concurrency constraints: prove from the actual code and scoped observations.
2. Whole-operation memory: additionally include providers, SQL and allocator
   allocations that coexist; document unknown attribution rather than guess.
3. Whole-process/system residency: add shared state and OS/cache domains; heap
   bounds and SQLite cache settings alone cannot prove it.

No absolute total C1/C2 RSS cap is established by this source review. Closing the
SQL/allocator/concurrency accounting is required before claiming such a bound.
Generic candidate spill is a removal target, not a required capacity mechanism.
The filesystem-tree proposal retains compact inode-effects ordering; that named
mechanism must have an explicit memory/disk quota, lifetime and failure policy.
C2's selected ordered-set fingerprint-index replacement is another distinct owner
needing qualification. The [detailed C2 cut list](physical-encoding-and-packing.md#9-reuse-and-cut-list)
also covers candidate batching, duplicate signatures, codec copies and the strict
single-attempt rule. Busy handlers, SDK retries and stale-proof refresh are removal
targets; failed work is not repeated to manufacture success.
The [save/persistence cut list](admission-and-persistence.md#9-cut-list-and-measurement)
adds owner-held scalar cursors, empty-finish elimination, batched catalogue writes
and indexed cleanup. Its diagnostic seen-index cut is conditional on an explicit
receipt contract change: distinct reuse and reuse occurrences must not be relabeled.
New location/base indexes add real retained bytes and write work to qualification.
The reference spill path does not itself prove a total backing bound. Reference
spill, any retained event storage and page-cache behavior require separate
qualification; an external Workspace spool policy proves no core quota. Memory
hints are not hard residency guarantees and do not permit warm-cache credit.

## 5. Validation plan

For IO1-IO14, record actual location/SQL/network calls, staging reads/writes, object/group reads,
canonical/hash passes, copied/allocated capacities, encoded/assembled bytes and
elapsed time on identical default-policy inputs. Include final DB/index growth
and pack occupancy; fewer temporary bytes do not establish improved final storage
efficiency. Verify bytes, identities,
ordering, duplicate demand handling and missing/corrupt inputs separately.

Exercise repeated IDs/shared groups, both delta roles and depth boundaries,
pack lifetime/density, incompressible singletons through the actual writer path,
many small files, slow consumers, failure while output is blocked,
fragmented edits, namespace reference ordering, cutoff transitions, late source
failure after persisted cohorts and uncertain remote outcomes. Compare actual
reference staging with target removal; already-direct paths get no fictional
spill savings. Measure per-operation peaks and lifetime overlap rather than adding
unrelated counter maxima. Retain declared provider costs inside their scope.

Reference admission types are private; the
[native fixture](../../../../../crates/layerfs-layerstack-store/src/objects/admission/native_tests.rs#L28)
shows a real database path without history entities, but does not create a public
benchmark API. Freeze a legitimate reference harness/access method before timing.
No test-only product hooks or copied alternative implementation may manufacture
a favorable baseline. The new standalone functions must be real product APIs.

Require no default-case latency/resource regression and demonstrated gains for
targeted work reductions before claiming optimized performance. No percentage
gain is fabricated here. Follow the [I/O qualification contract](content-io.md#7-measurement-and-completion)
and all repository measurement rules; historical v0.1.6 waivers do not waive
new regressions.

Apply the same evidence discipline to the filesystem-tree cut list: direct-value
compact updates, removal of unused synthetic record hashes, metadata sizing and
decode checks, shared inode/attribute reads, and removal of retry-only work.
Preserve checked topology, unseen aliases, additions-before-release, exact page
partitions and tiered ordering complexity. No whole-filesystem speedup follows
from timing only an isolated sorted update with preparation omitted.

## 6. Deferred integration observations

The wider audit also found Workspace/frontier pagination, remote payload copies,
local wire serialization, observer-only RPCs and completion/cancellation issues.
They are deliberately not target changes in this document. Future Workspace and
adapter designs may differ entirely, and their allocation/protocol ownership
will be reviewed then. No buffer/transport subsystem or identical Workspace mode
is assumed for cluster 1/2, and no deferred transport saving is credited to this
core optimization proposal.
