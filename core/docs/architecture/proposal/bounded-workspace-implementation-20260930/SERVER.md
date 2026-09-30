# Server, C1 and C2: streamed construction implementation specification

> **Status: Research; informative implementation proposal, not a product contract.**
> Baseline: `7edddbdb8e8512627aed0ed42533ef099d802384`, 2026-09-30.
> This packet specifies a proposed implementation. No product changes, builds,
> tests, benchmarks, commits or publications were performed for it. Proposed
> budgets, costs and LOC ranges are engineering decisions/estimates, not receipts.

## 1. Decision, boundaries and authority

Replace operation-sized resident plans with replayable logical cursors, exact
external metadata state and one bounded C1-to-C2 consuming pipeline. A file-set
construction stream serves one file or many files through the same assembler.
The directory/input/graph pipeline consumes individual bindings and values.
C2 continues to own canonical-object authentication, physical representation,
packing, private Save transactions and final visibility. C5 alone advances the
Branch. Workspace owns capture, saved facts, READY and local installation.

The v1 compatibility milestone keeps current file split/join, CDC, coalescing and
root/non-root partition behavior, replacing its storage of unfinished nodes.
The selected end-state is a versioned v2 construction/namespace profile with
immediate captured-parent file construction and an authenticated parent index.
It does not switch sparse edits to full-file CDC. Exact external graph proofs
replace full in-memory topology collections. Small inputs use the same checked
cursor/reduction semantics in admitted byte windows; they do not execute a
second historical graph algorithm.

Normative inputs are repository/core AGENTS, [scenarios](../../../../../scenarios.md),
[file content](../../../../../docs/roadmap/0.1/0.1.7/component-decoupling/file-content.md),
[filesystem semantics](../../../../../docs/roadmap/0.1/0.1.7/component-decoupling/filesystem-tree.md),
[canonical handoff](../../../../../docs/roadmap/0.1/0.1.7/component-decoupling/finalized-object-handoff.md),
[physical storage](../../../../../docs/roadmap/0.1/0.1.7/component-decoupling/object-storage.md)
and [complete I/O](../../../../../docs/roadmap/0.1/0.1.7/component-decoupling/content-io.md).
The [architecture study](../bounded-memory-commit-20260930.md) is informative.
The requested cleanup skill was read; its docs-only exclusion does not authorize
its code/test workflow here. Its SRP guidance is reflected in the proposed files.

Dependencies stay inward: Server composes Bridge, C1, C2 and C5; C1 sees canonical
providers, final consumers and narrow caller-owned metadata backing; C1 does not
depend on Workspace, FUSE, SDK, a command string, SQLite or the Server adapter.
C2 reuses its existing locked rusqlite and codecs. There is no new dependency,
third-party patch, WAL, fsync or crash-recovery service.

## 2. Current causes become forbidden states

| Source observation at baseline | New authority/type/invariant | Required proof |
| --- | --- | --- |
| `DirectoryUpdate.changes: Vec`, `RowSource::directory_for`, `PreparedRowSink::directory` materialize a complete directory | Directory header and byte-bounded binding cursor; sink accepts one binding | A wide directory never creates a directory-sized allocation; ordering/totals checked incrementally |
| `RowSpool::directory` reads complete payload before decoding another binding vector | Slot addresses a replayable binding run; one bounded decode window | Encoded and decoded windows charged together; no whole-row read |
| `receive` derives 65,536 declared-row/name admission from `ordering_bytes/1024` | Separate declared shape, resident-window and owned-disk budgets | No count is admitted merely by increasing RAM; larger shape profile requires its own arithmetic/proof |
| Service supplies no ordering backing; first default 4,096-record spill refuses | Construction receives its owned ordering capability at start | Spill inputs/output and cleanup known before allocation; no error-driven fallback |
| Full additions, alias, contents, initial counts, touched/zero and release prefetch populations | Ordered effects/values/results and paged exact state | Every population has cursor or admitted small-window storage, including release descendants |
| `EditObjects` retains drafts, parent references, detached and committed maps | Paged draft state plus bounded decoded boundary/cache state | Same split/join execution, same emitted roots, no whole-file rebuild |
| `load_node` calls cache insert without caller-required `make_room_for` | Cache insertion performs its own admission/eviction; no unchecked insertion API | Every entry path respects capacity, including localized edits and batch reads |
| Prepared parser reserves row count before final aggregate count refusal | Remaining row/name/byte allowance checked before each binding allocation | Lying totals cannot allocate beyond the current admitted window |
| C2 Save is allocated repeatedly for per-file RPCs | One generic bounded file-set assembler with shared Save per natural unit | Canonical bytes unchanged; actual physical packing/cadence measured separately |
| C2 shared/private indexes and publication clones coexist | Store/shared and Save/private byte owners, with explicit clone overlap | Fixed retained windows never become whole-Store maps; true allocator/cache scope included |
| All mutations, including `ReserveInodes`, share Store writer permits | Typed C2SavePermit and protected C5CatalogPermit; scope-bound serial ranges amortize creates | Catalog-only refill can progress while two Saves work; engine/control capacity is protected, and true catalog/range exhaustion remains precise |

The baseline C2 already uses private `save_id` output, bounded preparation waves,
short transactions and paged cleanup. Preserve those mechanisms; do not invent
an operation-sized SQLite transaction or a second persistence acknowledgement.
Known individual content roots are distinct from filesystem stage/Branch success.

## 3. Before and after through the ordinary product route

```text
BEFORE: WorkspaceApi::exec("ordinary cp/dd/mv/rm/... ") -> shell -> FUSE
                            | accepted live state
                            v
        Commit dirty Vec + saved map + extent Vec + descriptors Vec
                            |
              per-file SaveFile RPC / replacement replay
                            |
          C1 retained edit maps -> C2 Save + finish, repeated per file
                            |
         complete prepared metadata bytes -> whole-directory Vec
                            |
           C1 topology/maps/count arrays -> directory/inode roots
                            |
                    C2 finish -> C5 Branch CAS
                            |
        complete local reconciliation/deletion/update collections

AFTER: same public Exec, ordinary shell, same FUSE mutation semantics
                            | accepted paged live state
                            v
           immutable G1 cursor + independent G2 selected overlay
                            |
              FileSetSource: 1 or many selected files
                            |
       one bounded assembler; frames + actual changed-byte windows
              |                              |
       fresh direct source            inherited replay source
              |                              |
          current CDC/split/join + paged unfinished metadata
                            |
         shared bounded C2 waves/private Save -> known file results
                            | paged result run, no result Vec
                            v
         prepared binding/value cursors + frozen SubmissionInput seal
                            |
          Workspace READY + sealed SubmissionInput (unknown FS-root slot)
                            |
        one composite HistoryCommit: exact graph/effect reductions
            -> directory/inode/parent frontiers -> C2 candidate finish
            -> internal stage seal -> C5 CAS
                            |
         fixed local selection install -> owned bounded maintenance
```

The assembler responds to actual input shape and declared byte policy. It does
not detect cp, package managers, databases, benchmark fixtures or scenario IDs.
Independent existing single-file callers remain supported through a thin
one-member adapter over the same construction body. They need not adopt the
Workspace file-set wire operation to use C1/C2 independently.

## 4. Logical cursor and backing contracts

The signatures below are interface requirements, not compilable source added
to the repository. Put actual types/behavior in focused implementation files.

```text
DirectoryHeader { parent, binding_count, encoded_bytes, selection }
Binding { name[<=255 bytes], final_child: Option<serial> }
DirectoryCursor.next_header() -> header or EOF
PreparedRows.bindings(parent, byte_window) -> replayable BindingCursor
BindingCursor.next() -> one checked binding or EOF
PreparedRows.value_for(serial) -> fixed typed value or absence
PreparedRows.new_position(serial) -> exact declared position or absence

PreparedSink.begin_directory(header)
PreparedSink.binding(binding)
PreparedSink.end_directory(observed_count, observed_bytes)
PreparedSink.identity(fixed typed/portable row)
PreparedSink.finish(observed_totals)
```

Keep the current prepared body version where these changes are internal. Parse
one directory header, then exactly its sorted unique bindings; each binding
decrements remaining count/bytes before growing a buffer. Check count against
remaining declared names as well as minimum remaining bytes. Enforce maximum
name width and serial/root exclusions immediately. Final section/count/byte/EOF
checks remain mandatory. No declaration may turn encoded bytes into an unlimited
decoded vector. The old resident `FilesystemInput` adapter can offer the same
cursor over its caller-owned slices; it is not the Server streaming route.

Received rows have one stable selection ID, exact totals, ordering/schema and a
content seal. `directory_for` becomes a header/slot lookup; replay uses its binding
offset. Freeze input before semantic passes. A caller that mutates input between
passes fails selection/seal validation; it does not refresh or restart the proof.

Use two narrow capabilities already suggested by the existing ordering boundary:

```text
OrderingBacking: create owned append/read run, byte account, checked release
IndexedState: get/put/remove fixed-key bounded-value record;
              ordered page after key with caller count+byte allowance;
              seal phase; checked release
```

IndexedState exists solely for operation-sized construction metadata. It is not
a universal store, plugin, buffer service or payload CAS. Keys are bounded and
include a closed table namespace plus serial/draft ID; values are <=8 KiB.
Each table has its own typed codec. Large names are separate binding records,
not a directory-valued cell. Cursors carry last key, phase/selection and totals;
no self-referential borrowed SQL statement outlives a bounded page call.

The canonical provider also receives a required count+byte permit before
acquisition: `read_canonical_batch(ids, ReadWindowPermit, scope)` returns exact-
cardinality authenticated owners within it. Extend this existing boundary rather
than add an object service. C2 checks locator/pack/canonical size claims before
reserving decode/output buffers. Tree demands use the8 KiB role bound; file
payload demands use the supported policy object bound. Role hints bound
allocation, not authentication. Strict construction cannot call a provider that
allocates an unlimited response then checks bytes afterward. Unsupported
capability fails before effects; no point-query loop replaces a grouped read.
Retained independent APIs have an explicit compatibility wrapper/profile.

### Concrete external adapter and small path

Server owns a scratch session implemented in the existing C2 crate using
rusqlite: a **separate quota-owned metadata database**, indexed `WITHOUT ROWID`
keys, and no payload bytes or second canonical-object staging store. A shipped
`sql/construction_scratch.sql` defines the private format and counts as production
source. Keep the Store schema/canonical grammar unchanged for this adapter.
Use keyed/range queries whose order is supplied by the primary key, bounded
projection/limit, statements closed after each page, and small write transactions.
No whole-table collect, unindexed ORDER BY, view materialization or temp-table
sort is permitted on this route. Existing sequential ordering runs remain the
bulk reduction path, avoiding one indexed SQL write per binding.

Small-path selection occurs **before effects** from checked declared bytes,
record widths and known proof shape: input+slot storage <=64 KiB, exact state
<=64 KiB, and no required unbounded base walk. The same cursor/reducer algorithms
then use fixed resident arenas, with no mandatory scratch-DB open or index query.
The policy is about capacity, not commands. For topology whose visited population
is not known small, select external backing from the start, even if input is tiny.
There is no attempt at the old whole-graph algorithm followed by a catch-and-spill
fallback. Bounded caches in the external path use planned flush/eviction, with
the backing and output disk credits already admitted.

Each external-state flush contains <=128 metadata records and <=64 KiB of values;
one transaction cannot exceed that input plus its admitted engine footprint.
Mutation/read cursors use separate sealed phases where iteration order matters.
Cleanup closes the connection, releases known files/runs once and reports failed
ownership. A failed removal retains identity, allocated blocks and quota; a Drop
best-effort unlink is not checked completion.

## 5. Unified file-set construction and payload replay

Use `FileSetSource` for Workspace construction regardless of file count. A member
contains a monotonically ordered token/serial, immutable selected file version,
canonical construction Base/profile, exact lengths, final Base/Local/Zero records
and portable metadata or selected metadata root. Count/scalar passes over the
captured cursor determine a natural unit, without collecting all member plans.

The proposed wire capability is **FileSetConstruct v1**, not separate small-file,
large-file and command-specific protocols. A natural coalescing unit contains
<=512 members and <=4 MiB of encoded member headers/descriptors/replacement bytes.
A single member exceeding the coalescing byte target uses the same assembler and
its existing supported body envelope; it is not split into a different canonical
edit algorithm. The overall workload can have any admitted number of units.
Current 4 GiB file/8 GiB body limits remain until a separate larger profile passes
the arithmetic and resource gates. Natural grouping limits are work units, not
total files or lifetime counters.

One Store mutation permit and one C2 private Save serve a unit. One producer
constructs members sequentially; it shares canonical batches, lanes, codec
workspace and transactions. No finish/connection/codec allocation is inserted
solely at each file boundary. File content and metadata roots are emitted through
the ordinary C1 algorithms. C2 finish occurs once after every member and exact
body EOF succeeded. An early member root is provisional until that finish.

All ordinary Workspace file-set members use this route, including a large
singleton; no file-count, byte-size or error path falls back to standalone
SaveFile. Closed member kinds cover regular files, bounded symlink targets and
metadata-only changes to selected non-directory content. Kind/authorization and
content/metadata role checks cannot be bypassed through the enclosing operation.
Directory portable patches/declarations remain in the later prepared namespace
stream, whose directory content root C1 derives. Metadata-only members reuse
selected content roots. Symlinks use bounded `emit_symlink`, not file CDC.

### Fresh and inherited sources

Fresh construction requires no equality replay. Validate its descriptor section
into the bounded record source, then let C1 pull actual replacement bytes directly
from authenticated input into a private C2 Save. Check declared Zero bytes while
consuming them. Validate END_INPUT/exact total before publication. Partial output
on input failure remains an unpublished C2 attempt with normal definite/unknown
cleanup. This removes the redundant fresh full-payload Server spool; it does not
remove the Workspace's accepted private data or the Store's actual write.

Inherited construction retains **one** replay source for changed wire bytes,
because existing no-op comparison and edit construction require stable rereads.
Do not spool the unchanged inherited file. Up to64 KiB uses the admitted byte
window; larger replacement bytes use one charged sequential file. Existing
OriginRuns describe repeated/backward Base ranges using authenticated ranges;
they do not clone the entire Base. Replay-window cursors amortize adjacent fixed
record reads rather than seeking and reading the same edit header at every byte
call. Preserve equality comparison, source length and zero validation.

The current wire includes Zero replacement bytes. This packet preserves that
wire accounting and canonical result. An implicit-zero wire grammar is a
separate capability; sparse live Zero storage does not imply free network/CDC
work today. No compression level, cutoff, codec profile or construction fanout
changes are part of this packet.

### Paged result delivery and unknown finish

After C2 finish, read a quota-owned result run in order. A proposed fixed result
record is96 bytes: member token8, serial8, content root32, metadata root32,
logical length8 and checked flags8. Up to512 records cost49,152 logical bytes,
which **cannot** be put in the current32 KiB Success metadata frame.

Use authenticated ResultData frames under the new operation's exact aggregate
response allowance. Every row/frame is bound to request/group selection and
sequence; the receiver matches submitted token/serial order without a group Vec.
A small Success terminal states schema, selection, observed member count,
logical result bytes, result digest and completed Save disposition. Reject gaps,
duplicates, surplus frames and mismatched terminal totals/digest.

Workspace ingests directly into a paged provisional saved-fact table. It seals
one paged UnitCompletion fact after the matching terminal; lookups join that seal
to its rows to establish KnownSaved, without flipping every row or scanning the
whole unit. Lost terminal or C2 finish uncertainty
retains `UnknownFileSet(selection, delivery identity, received prefix, owner)`;
this attempt sends no final Branch command. No automatic re-send or guessed
cleanup is permitted. Known finished C2 output followed by
Branch conflict is not failed-save cleanup. The new operation requires explicit
capability/version negotiation; an old peer gets Unsupported before BEGIN.
Existing single-file APIs call the same assembler internally and retain their
independent response contract.

## 6. Edited mapping state with exact canonical behavior

Keep `apply_edits` dispatch, `Plan`, replacement-only CDC and the existing
`split -> split -> concat -> settle -> finish` execution order. Substitute a
typed `DraftState` implementation for operation-wide BTreeMap/BTreeSet storage:

```text
Draft { key, form, root_context, summary, bounded entries/canonical page }
ParentRefs { draft_key -> exact live draft-parent count }
Detached { draft_key -> candidate retirement }
Committed { draft_key -> finalized ObjectId }
EmissionSet { ObjectId -> accepted in this finish walk }
```

All populations are external in the selected external path. Only bounded decoded
pages, one replacement chunk/builder frontier and join boundaries stay resident.
Use the same next-draft-key monotone checked allocation; do not wrap or reuse
exposed keys. Draft/page records are <=8 KiB plus fixed metadata; payload objects
continue directly to C2 and are never inserted into IndexedState.

`load_node` reads one bounded stored/draft page. Its cache admits capacity before
insertion and drops/evicts known reconstructible entries; insertion itself owns
the invariant. Decoded clones required by split/join are distinct allocations
and consume frontier bytes. Resident cache eviction costs a reread, never an
identity change. Current64-page maximum is a ceiling rather than a promise that
every allocation is exactly64 pages; cache and page object overhead are admitted.

`settle` iterates detached keys through a bounded cursor, releases unreachable
draft records, and updates exact child references. It must not collect the entire
detached set. `finish` uses a bounded tree-height walk plus external committed
and emitted state, commits children first, patches draft child keys to their
real IDs, then encodes/hashes once. Stored subtrees retain their IDs. Only drafts
reachable from the final mapping are emitted; there is no speculative canonical
object-set publication or finish-time payload graph scan.

This storage substitution preserves execution/root behavior more directly than
switching to a new monotone mapping algorithm. Random metadata IO is a real cost.
After exact-root qualification, an interior-node finality optimization may seal
pages proven unreachable by all later ordered edits, leaving boundary pages per
level. The default remains the exact paged draft algorithm until that finality
proof exists; an 8 MiB refusal never triggers an alternate algorithm.

### Local parent version, construction policy and selected v2 end-state

The saved record stores both selected local parent version and canonical
construction Base/grammar profile. A generation-native resolver may expose the
immediate captured parent in result coordinates without silently changing C1's
construction recipe. G1/G2 coordinates select exact versions, not latest-by-serial.

Legacy v1 compatibility preserves the accepted split/join input/CDC partition
recipe and its expected roots. That is a bounded-memory migration milestone,
not the final scalable/concurrent architecture: an old construction Base may
still require cumulative old-delta replay. It cannot satisfy #248's final-delta
gate or the target #249 proof merely by moving the recipe to disk.

Select **file-edit-policy v2: immediate-selected-parent** for the end-state.
Every successor's ordinary Base is the known canonical result of its exact
captured predecessor file version. During a pending G1, G2 records parent-version
coordinates; one pending submission per Workspace prevents another Commit from
creating a longer unresolved live chain. The known-result binding resolves that
one edge after success. Captured dirty orphan/exception versions also receive
known canonical roots through file-set construction where future selected ranges
depend on them; their paged exception ledger owns the space. No end-state live
construction recipe retains R0 just to reproduce an old root, and no full-file
copy is introduced to remove lineage.

First touch of a successor file starts with one opaque exact parent span.
Lowering stops at it and emits one Base record; it never enumerates predecessor
private fragments. A candidate started before capture converts retained prefix/
suffix to at most two exact frozen-version parent spans plus its current change,
using bounded boundary construction. FileSet SourceCoverage counts the current
generation's descriptors/bytes, not the predecessor's fragmentation. Repeated
Commit(each one new small edit) therefore targets current-delta work plus mapping
paths, not1+2+...+n replay. Exact root/version/source authority and old-pin proofs
cover that cut; it is not permission to swap an earlier Base's coordinates.

Reuse split/join and replacement-only frozen CDC over R1; untouched R1 subtrees
retain their IDs. Changed partition/chunk identities can differ from a v1 recipe
that edited R0. The v2 namespace profile binds the write-policy digest. Changed
chunked results use FileState grammar v2 carrying the existing CDC digest and
an additional32-byte edit-policy digest; mapping/chunk byte grammars remain
unchanged. Unmodified/no-op legacy file roots remain readable/reusable. Whole-file
objects whose bytes already define identity retain their existing byte encoding.
No downgrade or automatic profile choice occurs after an error. A v1 caller
receives v1 semantics; an admitted v2 Stack receives the v2 contract.

This selects the identity/time trade explicitly: **v2 promises exact semantic
bytes and its own independently specified roots, not equality with the v1
expected-root ledger**. Freeze the precise FileState v2 layout/policy digest and
independent reference before enabling it; compare a v2 implementation to that
oracle. The current source supports neither version2 profile yet. Legacy v1
receipts keep their identities/statuses, and v2 qualification cannot relabel them.

Physical advisory predecessor hints remain non-canonical and cannot reinterpret
source coordinates. Unlinked/orphan selected versions absent from R1 use the
Workspace's sealed exception table. Server never substitutes a root merely
because its serial matches. Complete root, partition and closure comparisons
cover insertion, shrink, holes, repeated/backward copies and all overlap phases.

The FileSet grammar distinguishes construction Base from bounded authorized
source ranges. Ordinary cp/prepend acquires bytes through FUSE and emits Local
bytes; it gains no hidden copy opcode. A selected canonical source range may be
resolved directly by Server after checking its exact version/root authority,
under the same bounded range provider. A captured non-canonical source must first
be represented by its sealed paged recipe/known saved exception; a serial alone
is insufficient. An unsupported optimized mixed-origin form fails capability
admission before input. Do not recursively issue daemon ReadFile while holding
the upload channel, spool the whole unchanged file, or reinterpret all offsets
against the construction Base.

## 7. Exact namespace and graph algorithm

Use sequential passes over sealed rows and exact external state. Let N be names
changed, F typed values, G directories/edges that actually require topology proof,
and Z released descendants. These populations may grow on disk, never in a
complete resident vector. The immutable base is the valid canonical namespace
precondition; authentication alone does not validate an arbitrary imported graph.

1. **Shape and subject pass.** Check row/binding order, exact counts, scope,
   serial/root rules, immutable types, Base roles and fresh absence. Demand Base
   values in bounded count+byte waves. New serials are allocator-authorized inputs;
   C1 does not allocate them. Memoize only a bounded window.
2. **Binding effect pass.** Merge each changed name cursor with immutable Base
   pages. Record exact before/after edges and content-root results in sorted runs.
   Derive additions/removals from authenticated Base bindings, never caller
   counts. Unchanged subtrees retain IDs. The merge's final partition rules and
   sibling/summary checks remain the same C1 sorted engine.
3. **Reference reduction.** Sort/reduce effects by serial, join with fresh and
   typed value cursors and bounded Base reads. Aggregate all additions before
   releasing anything. Directory/non-file parent cardinality follows policy;
   regular files retain legal aliases. Use a compact private effect record
   (serial+signed tally/tag) where no typed value is needed, with typed values
   kept in their original input run. Do not repeat73-byte values in every edge
   event solely to fit a generic96-byte ordering row.
4. **Unreachable and release closure.** Store zero/dead candidates externally.
   Traverse a truly released directory in bounded listing pages; decrement its
   descendants and stop at retained aliases. Starting/touched/prefetched records
   are cursors, not a complete queue or map. A fresh disconnected inode is an
   error; the root's zero-count exception remains explicit. Semantic counts,
   rather than canonical-object reachability alone, determine membership.
5. **Effective graph proof.** Merge Base and changed adjacency lazily. External
   exact colors distinguish unseen/active/completed directories; an active edge
   is a cycle, a completed subtree proof is reusable only within this exact
   sealed final graph. The DFS stack and continuation also reside externally:
   namespace nesting is not B-tree level31. Prove unique-parent/alias rules,
   reachability for fresh builds, root restrictions and final-batch moves.
   Global color reuse removes repeated walks of overlapping changed subtrees;
   it does not replace alias/count validation or permit unsafe memo reuse after
   a graph mutation.
6. **Final typed output.** Join final counts, declared values, directory content
   roots and removals by serial, then feed `apply_inode_values` as an iterator.
   No initial count array, content-root BTreeMap, full final row Vec or second
   touched population exists. Return only the fixed filesystem root/summaries.

Early directory objects may be held in C2 private output while later proof runs;
no Save finish/root visibility occurs before complete final validation. Checked
retention precedes each C1 final-output promise. Invalid input aborts the private
attempt once. This is a semantic namespace proof; C2's incremental direct-object
dependency checks remain a different responsibility.

### Deep270 and sparse moves

The retained unaccepted cycle candidate measured fresh depth270 directory-row
lookups36,585 ->270 and inode demands37,398 ->814. Those are prior count diagnostics,
not this implementation or a timing promise. They support operation-local
completed-subtree reuse as a useful mechanism. The retained SDK deep270 whole
command2.805615834 s and C1 validation0.483269583 s were cache INELIGIBLE; validation
is combined work, not an isolated cycle timer. Do not fit a new speed claim from
those counts or rerun the historical arm to select a better number.

### Shape-based work bounds: externalizing quadratic work is forbidden

The latest retained #276 evidence distinguishes three facts. The original
[depth270 cost analysis](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5903016828)
derives the separate-prefix-walk count D(D+1)/2 =36,585; it does not isolate a
cycle-only timer. The later
[unaccepted completed-proof candidate](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5905762244)
reports instrumented fresh270 counts36,585 ->270 and fresh16 counts136 ->16,
plus inherited270 candidate1080 inode lookups and inherited16 counts184 ->64.
Those are candidate diagnostics, not current accepted after-state. The
[current priority inventory](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5905916098)
retains whole-Commit populations and resource/measurement gaps. This architecture
must eliminate their structural work causes without promoting those receipts.

Let K be changed parent edges, A distinct ancestors encountered in the effective
final graph, H canonical B-tree height, Hs scratch-index height, L compact effect/
value rows, G visited graph edges and Z actually released descendants. Count
logical visits separately from page/index paths: putting colors on disk changes
an O(G) graph walk into O(G Hs) page work, not constant-cost O(G) I/O. The following
are implementation invariants, not latency estimates:

| Current or prohibited repeated work | Selected structural exclusion and work upper bound |
|---|---|
| Start a DFS for every changed directory and revisit every overlapping prefix | One exact sealed-final-graph active/completed proof state. Each required graph edge is visited once: O(G) logical visits plus O(G Hs) scratch paths; no D(D+1)/2 walk. |
| Recreate or clone a whole directory for each `directory_for` demand | Header plus persistent binding/page cursor. Demand counts may grow, but retained bytes remain the admitted window; every traversal is charged and completed proofs are reused. |
| Call `RunStore::find` per name against a cursor that restarts for backward serials | Append compact effects, then sort/reduce and ordered-join once. No per-binding latest-state lookup through a linear run. Exact keyed graph/release state uses a bounded indexed port. |
| For each serial, restart scans of typed values, counts, roots or effects | Ordered merge joins advance each input cursor at most once per named pass: O(L) row advances per pass. Keyed random lookup is reserved for graph/release dependencies, with Hs charged. |
| Requeue touched, zero or prefetched inodes and replay release edges | Exact external state owns each transition; consume a release edge once, O(Z) logical release work plus required page/index paths. Retained aliases stop release at their exact count. |
| Rewrite every accumulated run on each spill | Fixed fan-in8 size-tiered merging: each row crosses O(log8(number of runs)) levels. Bound and admit all input buffers, output run and overlapping old runs before merge. |
| Rescan all surviving drafts to settle each edit | Exact detached/refcount transitions consume created/released draft nodes and edges. Charge frontier work to actual construction/release; no whole mapping population pass per edit. |
| Restart an ancestor chain for each changed edge | Shared exact final-graph colors and parent answers: O(K+A) logical chain steps, O((K+A)H) authenticated page paths plus scratch costs. Never O(K A) overlapping-prefix work. |
| Expand all predecessor private fragments on every successive Commit | One exact captured-parent span and current-generation descriptors. Across n small-edit Commits, bound descriptor lowering by O(sum(current delta descriptors)+nH+necessary canonical/release work); no1+2+...+n replay. |

The run lookup hazard is source-confirmed: current
[`RunStore::find`](../../../../crates/layerfs-content/src/filesystem/references/runs.rs)
restarts a retained `RunScan` from row0 when a requested serial precedes its
resume position. Ascending demand is efficient; an adversarial name-to-serial
order can repeatedly read prefixes. The new effect reducer must not preserve
that access pattern merely behind a spill port. Its compact signed effects are
sorted once before final-state joins; true online dependencies use exact indexed
state with checked byte/page credits.

Namespace v1 compatibility can still require a broad G, and initial v2
certification still pays the complete graph proof. The v2 incremental target
uses only verified published bases and the actual affected parent chains;
arbitrary depth contributes A, recursive deletion contributes Z, and canonical
partition/fill rules may require bounded sibling reads on affected paths. None
of those are removed to manufacture a locality claim. For batches with shared
ancestors, memoized proof ownership is exact to the same sealed final graph;
intermediate mutation invalidates the state rather than reusing a stale proof.

Future diagnostics record directory headers/bindings, graph color/edge visits,
parent steps/completed hits, scratch gets/puts/pages, run rows by merge level,
join cursor advances, draft created/released/finalized nodes, distinct canonical
pages/waves, inherited versus current descriptors and release edges. A violated
shape upper bound fails algorithm qualification even if a finite latency or RAM
row passes. Keep these counters attributable to the operation and compare them
with an external graph/root oracle; never delete cycle, alias, reference or
reachability validation to improve a count. No count ratio is a whole-latency
law, and no numerical after-speed is claimed by this packet.

Bounded DFS can still walk a large moved subtree. It is the v1 bounded-memory
milestone, **not** the final move-locality architecture. Select an authenticated
parent tree in **namespace profile v2** as the complete target below; do not bolt
an unverified reverse-parent cache onto v1 and call it a semantic proof.

### Namespace v2: authenticated parent relation and exact certification

```text
FilesystemRoot v2 (proposed148-byte value)
  existing profile/scope/root-serial/inode-table fields
  parent_tree_root[32]
  profile digest binds parent grammar + immediate-parent edit-policy v2

Parent tree: non-root Directory or Symlink serial -> {kind,parent,name}
  name <=255 bytes; parent is a Directory in the same scope
  root has no entry; regular-file aliases remain reference-counted
```

Use the existing C1 sorted-page engine with a new typed parent-record format:
8 KiB maximum pages, checked sorted serial keys, actual variable row widths,
canonical partition/fill rules and level31 ceiling. Select the existing44-byte
sorted-page envelope/header shape, with distinct parent magic/role/version.
A leaf row encodes serial8 +kind1 +parent8 +name-length2 +name bytes:
`page_bytes =44 +sum(19+name_bytes)`. The name length is checked<=255 before
allocation. Leaf fill is **canonical byte fill2/5**, including key and header:
non-root encoded bytes3277–8192, not inode leaf50–100 count fill. A255-byte name
makes one274-byte row; only29 such rows fit, and a valid filled non-root page
needs12–29. Empty44-byte or smaller-than-fill leaves are permitted only as the
tree root; deterministic split/join/root-collapse follows the existing variable-
width sorted engine's byte policy, including its checked sibling decisions.

Derive branches separately: serial upper key8 +child ObjectId32 =40 bytes per
child, plus the same44-byte page header carrying the subtree summaries. With
the selected byte-fill rule, a non-root branch has81–203 children; root branches
may have2–203, and a one-child root collapses. Never inherit inode branch64–127
or variable-name branch widths by type similarity. Encode/decode/size/fill and
partition use these exact formulas; independent profile-v2 roots test all small-
root, maximal-name and split/join transitions before this grammar is admitted.
Reserve logical roles14/15
for parent leaf/branch (baseline1–13 are occupied); freeze their precise magic,
version and wire layouts before implementation. C2 handles them through the
ordinary bounded tree/FULL lane initially, not inode-value pooling or a new
compression profile. Current C2 `sql/schema.sql` restricts object roles to1–13,
so this requires explicit **C2 schema/capability v11** supporting1–15 and their
typed-role checks. V10 cannot accept the parent roles; normal open never silently
upgrades it. The quiesced bounded metadata migration below is the primary path.
The FS v2 root directly references both inode and parent
tree roots. A loose sidecar root or caller-supplied 'validated' flag is rejected.

**Hash binding is necessary but insufficient.** Two authenticated trees can
still disagree. Add a narrow C1 `VerifiedNamespace` capability whose fields and
constructor are private. It names exact FS root, profile, scope, inode root,
parent root and validation origin. It can arise only from complete initial/import
validation or from a checked incremental successor of another verified namespace.
Parsing a descriptor, reading a C5 opaque root or authenticating objects does
not create this capability. A checked successor stores fixed verified authority,
not a recursive parent-proof Arc/DAG. Its construction parent lease releases
after certification; actual old pins retain independent root leases. Many
successive Commits therefore cannot grow a hidden proof lineage in resident
state or force recursive validation through all historical certificates.

Server keeps trusted validated-publication provenance keyed by exact root
and profile in byte-admitted live proof leases and an evictable bounded cache,
never a resident registry of every historical root. Reconstructible cache
eviction may cost certification again; it cannot change validity. Read-only
canonical-byte acquisition needs no semantic token, but incremental namespace
mutation/locality proof does. Reopening an opaque root without trusted origin
requires explicit bounded full validation; it cannot trust an unauthenticated
local sidecar/cached flag. A later persisted certificate would need its own
Server-authority/authentication/incarnation contract and is not substituted here.
The default after a process restart is therefore one explicit full certification
before locality-sensitive use of an unproven Base, retained for that open root;
subsequent accepted successors carry the inductive proof. No hidden validation
fallback/retry occurs after attempting an incremental mutation. C2 continues
authenticating canonical objects and does not learn namespace semantics.

The concrete ephemeral authority is `NamespaceValidityLease`: opaque lease ID,
Server issuer epoch, authenticated principal, Store, exact FS root/profile/scope,
inode/parent roots and private C1 VerifiedNamespace owner. Only successful full
certification or a checked successor constructor can insert it. A short
byte-admitted live registry owns one fixed record per active lease; active
Workspace/operation/pin references hold it, and exact release removes it. It is
not a record per Commit or all past roots. Optional validity caching has a fixed
byte capacity and cannot promote a parsed token to proof. Duplicate selected
roots may share an immutable verified owner, while principal-bound lease handles
remain distinct. No registry lock spans proof, IO or construction.

Read-only reopen returns opaque root bytes unless this exact live authority
exists. Before a locality-sensitive mutation, the request declares either a
valid epoch-bound lease or explicit `CertifyExactRoot` acquisition. The latter
does its full read-only certification before beginning any Save/mutation. A
stale/forged lease is refused before effects; it does not cause an automatic
refresh/retry. Server restart changes issuer epoch and invalidates every old
ephemeral lease. Cross-process serialized roots, C5 records and client tokens
cannot reconstruct private C1 validity. Establishing a new lease is an explicit
admission/open/certification action, not an extra Stage/CommitStaged publication
round trip. Its actual full-validation cost is included in the declared route.

**Initial build/import.** Enumerate all forward namespace bindings and typed
inodes using external sorted runs. Derive non-file parent rows from actual edges;
prove exact correspondence, unique parents, root/no-parent, types/scope,
reachability/cycles and all reference counts. Build both trees from those same
checked edge/value streams, then issue VerifiedNamespace. An externally supplied
v2 root undergoes the same correspondence proof before becoming a verified Base.
This O(namespace) certification is actual work, never claimed path-local.

**Incremental update.** The valid Base establishes unchanged correspondence.
For each changed binding, authenticate its old forward binding and matching old
parent row where it is non-file. Compute the final relation from the complete
batch's exact removals/additions, handling restatement and legal root exceptions.
Emit parent changes and directory changes from that one checked effect authority.
Enforce at most one final parent for each non-file and correct positive/zero
counts; regular files keep aliases. Check each changed directory's new parent
chain in the **effective final** parent relation, which overlays all batch moves
and fresh declarations on the verified Base. Active/completed exact colors and
the stack remain paged; an active ancestor is a cycle. Chains must reach root,
not an absent row, deleted parent or caller-supplied terminal. Cross-move cycles
cannot be cleared by inspecting only old ancestry.

Build changed parent/index/directory/inode pages through the existing COW engine;
unchanged pages and file roots retain IDs. Only after matching checked outputs
and complete effects/proof does C1 issue the successor capability. A directory
move touches changed bindings and required ancestor/index paths, not every
descendant solely to discover its parent. Many small edits target work
proportional to distinct changed paths/ancestors plus actual release descendants,
with external memo/runs when that union exceeds a window. Recursive deletion
still pays released descendants and arbitrary depth still pays ancestor steps.

**Compatibility and migration.** FS grammar/root version2 and its profile differ
from v1; old v1 readers fail explicitly instead of ignoring the extra root.
New readers retain an explicit v1 codec/read/write compatibility surface, using
the exact bounded graph path. Existing C5 Stacks fix scope/profile. C5 schema1
stores opaque32-byte profile/scope fields and its existing StackInitialization/
initialize_layerstack contract can initialize a new certified v2 Stack; it does
not mutate an existing Stack's profile. Server owns authorization/certified-root
orchestration and missing public import routing. C5 needs no namespace semantics,
new schema or second migration service for this initial workflow.

Choose explicit import of a selected v1 snapshot into a newly allocated v2 Stack
and scope. Server/C5 authorize the source root and new scope, count selected
identities, reserve disjoint ranges, and build a paged old->new serial remap.
Stream remapped directory bindings, inode rows and parent relation. Reuse file,
symlink and attribute roots where their formats are scope-independent; forward
directories/inode tables/FS root and Commit identities change. Initialize the
new Stack only after content/index certification and C2 completion, with exact
known/unknown initialization custody. Original v1 Stack/history remains intact.
In-place mixed-profile lineage is outside this first migration workflow.

### Primary C2 v10 -> v11 migration: quiesced bounded metadata copy

Keep all pack BLOBs, save IDs, physical ordinals and content identities in place.
At baseline no table has a foreign key to `objects`; that table itself references
`saves`/`object_packs`. Select an explicit Store maintenance capability and a
private runtime manifest, not automatic open-time repair. The operator-controlled
deployment stops normal admission, drains/closes all configured Store users and
establishes exclusive maintenance authority for the exact Store file identity.
The current process-local arbitration mutex cannot exclude an unaware v10 process
between short SQLite transactions. If deployment exclusivity cannot be established,
maintenance refuses; do not claim that a manifest flag alone stops old binaries.
New cooperating C2 opens acquire a shared lifetime maintenance/read capability
before schema access; maintenance acquires its exclusive nonblocking counterpart.
Use the existing nix0.31.3/fs safe Flock<File> API on the exact native Store
identity, with explicit unsupported provider behavior. Old binaries that never
took this lock must already be stopped/closed by deployment quiescence. The
lock is not an imaginary retrofit of cooperation into those processes.

```text
QUIESCED -> COPYING -> COPY_VERIFIED -> TABLES_SWITCHED
          -> RETIRED_EMPTY -> CLEANED -> VERSION11_PUBLISHED
```

Create `objects_v11_staging` with the same locator columns/primary key and widened
role constraint. Copy exact rows in indexed primary-key order, <=128 rows and
<=64 KiB encoded locator data per page, under short MEMORY-journal transactions.
Keep only a last-key/count/digest/phase record, not every copied locator. An
ordered bounded pass compares every key and field plus counts between source
and staging; no sampling or payload reread stands in for this metadata equality.
Pack IDs/locators remain unchanged, so no compressed payload rewrite is needed.

Inside one admitted small schema transaction rename old `objects` to
`objects_v10_retired` and staging to `objects`. Normal admission remains closed.
Delete retired locator rows in indexed primary-key batches/short transactions;
DROP the retired table only after independently known empty. Release/close exact
temporary owners, then publish `user_version=11` and reopen normal admission.
Verify the final declared schema/profile and old/new role capability. Do not
perform a full-table DELETE/DROP or ALTER-rebuild inside an operation-sized
rollback journal. Engine heap/dirty-page high-water and actual blocks are proof
obligations at every step, including schema swap and final empty-table drop.

The manifest identifies Store incarnation, maintenance selector, exact schema
phase, committed last key/count and owned tables/charges. Known interrupted
maintenance may continue only through an explicit same-selector maintenance
action using verified phase/row facts. Unknown transaction acknowledgement retains
closed admission and exact ownership; there is no guessed rename/drop, automatic
resend or new maintenance owner. MEMORY/synchronousOFF still supplies no crash
durability; a crash-damaged file is not repaired from an optimistic manifest.

Temporary disk grows by another locator B-tree and bounded manifest/journal,
**not another full model/Store payload**. With current auto-vacuum semantics,
deleting the old table can leave pages in SQLite's freelist and preserve physical
file/block high-water. Those pages are reusable, but logical cleanup is not an
automatic filesystem block refund. Final allocated bytes must report that
high-water; no large VACUUM/copy is hidden in completion. A fresh v11 destination
Store/export is separately selected placement, not a fallback after failed
maintenance; that route pays the required canonical closure transfer and may
temporarily duplicate payload bytes.

After the primary metadata upgrade, v1 snapshot -> new v2 Stack/scope migration
can reuse unchanged file/symlink/attribute objects in the same Store. It still
pays an exact namespace scan, serial remap, new directory/inode/parent pages and
certification. Parent-index bytes scale with non-file bindings:
`sum(19+name_bytes)` before page headers/fill/branches, plus32 bytes in the FS
descriptor and32 bytes per changed chunked FileState policy field. An all-regular-
file wide directory has a small/empty parent tree; a directory/symlink-heavy
namespace pays more. V2 feature/migration space is a declared new profile, not
permission to hide extra data under old history's5–10% deviation allowance.

The v2 role/profile/codec/C5 creation changes need explicit compatibility and
independent v2 expected-root ledgers. Do not compare v2 IDs to v1 ledgers as equal,
rewrite old checkpoints, or charge migration as free setup of a measured Commit.
V1 remains legacy-compatible where retained; the concurrent target's page-local
move and immediate-predecessor promises apply to admitted verified v2 Stacks.

Hot normalization/ancestor retirement remains Workspace-owned. Server output
must preserve all child identities and summary fences used by that proof. Neither
a completed graph memo nor canonical root equivalence authorizes reclaiming pinned
private ancestors; retirement requires exact selected reachability and ownership.

## 8. Typed C2/C5 admission and serial progress

Replace the cross-component mutation gate with **typed resource domains**. This
is an explicit new Service admission/profile contract, not a claim that bypassing
the old all-mutations gate leaves its meaning unchanged:

| Domain | Owns | Explicit rule |
| --- | --- | --- |
| C2SavePermit | Actual canonical construction/encoding/private Save | Existing Store writer setting/default2 unchanged; one producer per admitted unit |
| C5CatalogPermit | One short allocator/history transaction on the separate C5 DB | Serialized per catalog, protected engine/control/connection bytes; no C2 Save arena or third C2 writer |
| Read/control permit | Bounded inspections/liveness/response | Independent bytes/FD admission and exact authority |

C5-only ReserveInodes/Fork/DiscardStage/CommitStaged and other pure catalog work
do not consume a C2 Save permit. Composite HistoryCommit constructs/finishes its
canonical candidate under C2 admission, records exact outcome custody, releases
that permit, then takes the short C5 catalog phase for internal stage/CAS. Never
hold a catalog transaction through C1 construction or wait for a channel while
holding Workspace State/publication locks. Actual SQLite contention/refusal and
same-Branch conflicts remain precise; no busy handler, refresh or automatic retry.
No new helper construction worker/producer is created for catalog requests.

The fixed service-role layout must reserve an owned control execution context
that can run short catalog jobs between bounded input/construction/SQL quanta.
It cannot be a pool whose every context is blocked in synchronous4 GiB builds.
This is the ordinary declared control executor/reactor task, not a per-command
thread, extra Save arena or helper construction producer. Catalog enqueue/FD/
frame work is bounded, and fair wake-up does not hold a Store or Workspace gate.
Channel reservation alone is not sufficient progress evidence.

Select one active C5 transaction per catalog, a protected64 KiB pending control
byte ring,1 MiB first-party catalog work/terminal capacity, one protected channel/
FD owner and4 MiB engine headroom within the process-wide32 MiB SQLite guard.
Queues are byte-admitted and respect the request's finite deadline; a full ring
refuses before mutation. The global engine credit plan for the default one-Store/
one-catalog profile is C2 retained connections2*3 MiB=6 MiB, one bounded active
Store SQL wave12 MiB, read connections2*2 MiB=4 MiB, scratch/global4 MiB:26 MiB,
plus protected catalog4 MiB=30 MiB, leaving2 MiB guard margin. Read connections
use their own1 MiB cache within their2 MiB envelope. More catalogs/Stores or large
singletons require a separately derived profile and leases, not uncounted copies.

These engine credits must be derived from fixed statement shapes, cache/row/
byte/dirty-page limits and qualified provider behavior before enabling the strict
profile. Public SQLite does not provide a per-connection hard heap limit; scalar
Rust credits cannot themselves reserve engine bytes. The profile is Unsupported
until its enforced shape envelope plus global guard establishes the protected
headroom. A C2 SQL wave never holds this scheduling/admission through a whole
4 GiB Save. Narrower engine/physical capacity refusal remains explicit; do not
promise metadata progress from connection separation or an unverified counter.

Keep **preacquired allocator ranges** as a generic call-amortization mechanism:

```text
IdentityRangeLease {
  Workspace incarnation, project/scope, first, count, next, allocator receipt
}
initial block = 4,096 serials; low-water trigger = 1,024 remaining
```

Acquire a block under ordinary C5/Service mutation admission before making a new
Workspace's create capability usable. Before a bulk Commit lease, ensure at least
one full block of unused range headroom for the admitted G2 progress profile;
this acquisition occurs outside Workspace State/publication locks. A range is
one fixed owner record, not a list of4,096 serials. A tiny per-Workspace allocator/
accounting lock consumes it monotonically and records create/exposure burns;
it is not a whole-Workspace mutation sequencer. Exposed/potentially exposed
failed serials stay burned.
C5's baseline allocator already consumes the complete reserved range in its
transaction and never recycles unused numbers.

Low-water refill is an ordinary bounded C5-domain RPC, so it need not wait for
two long C2 Saves to finish. It is not a background helper producer, retry or a
reserved third C2 Save slot. If catalog/control/engine capacity is unavailable,
existing credits remain usable; exhaustion returns precise allocator
admission refusal or bounded waiting within the individual FUSE callback policy,
with no publication begun. Finite headroom cannot guarantee unlimited fresh
creates under actual catalog/quota failure. Creating million-file forests still
pays roughly ceil(fresh/4,096) successful allocator calls plus boundary headroom,
rather than one per create; this is prospective call arithmetic, not a measured
latency. Unused ranges burn serial space, not equivalent private payload bytes.

An unknown reserve response retains exact request/delivery custody and no guessed
replacement request. Prepared rows contain actual used fresh serials, not every
reserved serial. Different Workspaces in one scope receive disjoint ranges.
Scope/authority/signed64-bit arithmetic and exposure semantics stay mandatory.
The exact range/block policy is product-wide generic creation behavior.

## 9. Working windows, indexes and engine envelope

The implementation owns allocations by capacity and lifetime. A moved allocation
transfers its existing charge; it does not return credits until the last consumer
releases it. Producer admission occurs before C1 allocates a canonical object.
The wave plan includes the producer object, drained wave being consumed, next
pending object, selected alternatives, groups, tails, pack assembly, dependencies
and borrowed/reference descriptors. Do not count the drained Vec twice, or omit
the simultaneously live next object.

| Owner | Concrete selected mechanism | Lifetime/release |
| --- | --- | --- |
| Input/result framing | Existing16 KiB frames; fixed record/body windows, no whole-result encode | Per leased channel; terminal/error closes exact owner |
| Small input/exact state | <=64 KiB input arena and <=64 KiB state arena, predetermined eligible shape | Through last required replay/proof; no scratch open |
| Inherited replay | One64 KiB payload window plus fixed-record index windows | Until equality/construction complete; checked unlink |
| Draft/graph/indexed state | External records, <=64 KiB dirty flush, <=128-record batch; bounded page/cache windows | Per sealed operation; known cleanup or charged failure |
| Ordered effects | Existing bounded pending/run machinery adapted to cursor joins; fixed merge fan-in8 and16 KiB/run window | Inputs+output charged until removal; no reader per all runs |
| Mapping and namespace frontiers | Legacy fanout128; typed v2 parent branches up to203; max canonical level31, bounded sibling/root contexts | Derive frontier capacity from the selected page policy; reserve decoded/encoded overlap and chunk provider demand by count+bytes |
| Canonical wave | Existing <=512 objects and <4 MiB canonical bytes, supported singleton separate | Until C2 actually consumes it |
| Codec | Existing16 MiB encoder; decoder up to1 MiB lazily | One per active Save/read owner; reused across files, unchanged parameters |
| C2 dependency/group/value caches | Existing4 MiB pack,512 KiB group/value bounds and chain-work limits | No allocation path bypasses insertion admission |
| Store shared indexes | Existing bounded131,072 metadata entries and704 KiB content signature arena | Per live Store handle; charge shared snapshots/overhead |
| Save private indexes | Existing fixed windows; private snapshot/clone bytes admitted | Per Save; no whole-Store growth, finish overlap charged |
| SQLite | Explicit verified cache/mmap profile, bounded indexed queries/transactions, process-wide hard heap guard | All live connections, including scratch/read/catalog composition |

Keep the current bounded index algorithms/window chronology initially. The
unbounded cause is not a total-Store scan here; it is unchecked multiplication
of fixed indexes and clone overlap. Shared state, private per-Save state, old
shared snapshot and the new publication clone can coexist. Count actual B-tree
nodes/allocator overhead separately from the source's tuple-byte estimate.
File-set batching amortizes clone/codec costs across members without changing
pool candidate equality, smallest-ordinal selection or reset semantics. Do not
substitute an O(index-size) Vec insertion algorithm to avoid node accounting.

### Proposed admission numbers, not measured memory

Use a standard filesystem construction lease of **72 MiB first-party working
capacity per active Save**, a separate **32 MiB process-wide SQLite heap guard**,
<=8 MiB first-party per ordinary admitted read (larger generic reads need their
own lease), and explicit Store-shared allowance. These
are proposed conservative byte-admission ceilings, not allocations made eagerly,
current RSS or a proof that all baseline code fits them. The final allocation
ledger must derive/verify every suballowance before enabling the profile.

Within72 MiB, initially reserve16 MiB encoder +1 MiB decoder, up to8 MiB private
index/clone ownership,12 MiB canonical/encoding/group/pack overlap,8 MiB C1
frontier/order/index windows,12 MiB provider/cache/read overlap, and up to1 MiB
transport/control/bookkeeping. These sum58 MiB, leaving14 MiB for verified
allocator/container/transient overhead. All actual allocation paths must enforce
their suballowance; unused reservation is not resident memory. Unsupported or
larger generic C2 singleton roles acquire a separate96 MiB first-party lease
before construction, rather than silently exceeding the standard window.
Role/object maxima and all overlapping alternatives must be derived again for
that class; absence of global capacity is explicit refusal before effects.

Default two standard Saves therefore reserve144 MiB first-party capacity plus
one32 MiB engine guard, not32 MiB for the whole Server. The known32 MiB encoder
sum is only one part. At64 Saves, arenas alone could be1 GiB; byte admission
normally refuses that multiplicity without altering configured Store capacity.
The writer count remains policy and the byte pool is its independent physical
admission. Do not raise quotas/workers after a failed measured case.

SQLite cache_size is a page-cache allowance, not an engine heap cap. Verify a
2 MiB Store cache and512 KiB scratch cache, mmap0, MEMORY journal/temp,
synchronousOFF and busy_timeout0 in the new declared profile. Use indexed bounded
query shapes and existing short C2 transactions. The32 MiB hard limit is global
to SQLite in the process and includes other participating SQLite users; it must
be configured once and read back, not separately counted per connection. If the
selected library cannot enforce it, the strict profile is Unsupported. No new
FFI site/unsafe module is introduced. NOMEM/COMMIT/cleanup outcomes retain exact
definite-versus-unknown classification; a heap refusal cannot trigger a larger
limit, transaction replay or alternate journal mode.

[SQLite cache controls](https://sqlite.org/pragma.html#pragma_cache_size)
and [hard heap guard scope](https://sqlite.org/pragma.html#pragma_hard_heap_limit) establish
why cache and process heap are distinct. Engine admission is still not Rust RSS
or a page-cache guarantee. Exact configuration is a future declared resource
profile, not an alteration to historical receipts or a completed capacity proof.

## 10. Physical residency and disk quotas

The selected implementation is a **dedicated native Linux Server process inside
an owned cgroup-v2 memory/dirty-I/O domain**, separate from command/application
owners. Trusted bootstrap creates the domain before Server/Store/scratch activity,
sets and reads back `memory.high=224 MiB`, `memory.max=256 MiB`, `memory.swap.max=0`,
and binds the exact process/domain identity. Keep the Store domain across Server
instances so older owned cache charges remain in its aggregate scope; do not move
an already-warm process and claim its old cache charges followed it. The strict
provider requires local native Store/scratch backing and excludes an unaccounted
VM/host-filesystem cache layer. Shared external Store users/data cache make the
strict ownership claim unsupported until their whole scope is included.

Use a176 MiB first-party byte pool including shared indexes/leases/catalog/control
and active72 MiB/8 MiB working owners, one32 MiB global engine guard and16 MiB
fixed runtime/stack allowance. These leave32 MiB for accounted file/socket/kernel
cache at the256 MiB containment boundary. Admission reads actual domain current/
anon/file/dirty usage and refuses incompatible additional owners before effects;
resident reservations return on actual buffer release. Retained owner bytes stay
charged. This is a target, not a proof that two96 MiB singletons fit alongside
all reads, nor a measured peak. Configuration is rejected if fixed/protected
owners and selected concurrency cannot fit.

Linux cgroup memory.high is throttling/reclaim and memory.max is a containment
limit with possible OOM consequences; neither alone proves healthy operation or
an exact512 KiB cache window. [Kernel documentation](https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html)
is the authority. An OOM event is a failed/unknown custody outcome, not a success
or an acceptable way of enforcing normal backpressure. Validate successful IO
with stable resident high-water beneath the containment margin. Complete cold
performance eligibility remains its independent whole-domain measurement gate.

Owned payload/run IO uses direct aligned windows where supported; buffered
SQLite allocation/writeback stays inside the same enforced memory domain.
The common IO admission checks dirty/cache pressure at bounded quanta, pauses
upstream pull without queue growth when headroom is unavailable, and lets kernel
writeback/reclaim proceed under memory.high. It does not call fsync or claim
DONTNEED flushed dirty data. SQLite retains its bounded transaction/busy policy;
a kernel IO stall/timeout is reported, not retried. Qualification must show
successful long streams, control/catalog progress and no file-size-proportional
residency trend or OOM in this exact domain. Containment alone does not prove it.

Use aligned direct IO for owned sequential payload replay/runs on a supported
provider, reusing existing native IO capabilities and fixed aligned windows.
SQLite scratch/Store I/O has its verified engine envelope and deployment file-cache
account, since public rusqlite does not supply a bounded-residency VFS merely by
setting mmap0. Do not patch SQLite or add an unapproved VFS/unsafe site. Other
hosts may supply a qualified equivalent physical provider/envelope; logical
formats and algorithms remain host-independent. A host lacking required strict
enforcement reports Unsupported for that profile. Advice or buffered64 KiB
writes are not silently substituted as proof. No APFS clone/host-specific logical
semantics or fsync is introduced.

Disk ownership is separate:

```text
D_operation = changed-byte replay when semantically necessary
            + sealed input/slot runs + draft/graph/effect metadata
            + simultaneously live merge inputs/output
            + provisional saved-result runs + failed cleanup owners
D_Store     = published canonical data + unpublished Save output
            + normal indexes/physical encoding overhead
```

Current64 MiB ordering/256 MiB prepared/8 GiB save ceilings stay named
and unchanged in initial profiles. One million96-byte reference records already
need96,000,000 bytes before merge overlap; a64 MiB ordering budget cannot admit
them. Compact effect records reduce temporary bytes but do not make arbitrary
millions free. Future disk/shape profiles specify exact peak inputs/output and
quota before admission, rather than charging their byte totals to resident RAM.
Cleanup/refunds follow identity and allocated-block checks, not logical file size.

## 11. Canonical, C2 and Branch completion composition

```text
immutable input + admitted working/disk owner
 -> file-set private C2 waves, finish and bounded UnitCompletion seals
 -> Workspace READY: sealed file/exception facts + SubmissionInputSeal
    + protected install/failure resources + UNKNOWN-FS-root outcome slot
 -> SINGLE composite HistoryCommit request
    -> internal checked namespace passes + children-before-parent output
    -> private C2 waves/short transactions
    -> exact EOF + all semantic/object proofs + construction cleanup
    -> C2 candidate finish -> internal CandidateSeal/stage
    -> one C5 Branch CAS
 -> known reply fills the fixed outcome slot
 -> fixed local install, independent bounded maintenance
```

For a file-set unit, finish publishes only its known completed member objects.
For the later filesystem candidate, final graph/count proof precedes its finish.
These are not one atomic Store operation spanning all content and Branch history.
Canonical child availability is checked incrementally by C2, with no final Store
object-graph traversal. Existing early-committed private rows remain invisible
to ordinary readers until Save publication.

After C1 verifies the candidate and before Branch CAS, pre-admit and insert the
fixed successor NamespaceValidityLease registry record, terminal capability and
response fields. A known Branch publication must not then fail with proof-registry
Capacity. Workspace READY reserves a fixed returned-lease/outcome slot even though
the FS root is unknown when its composite request is sent. Failed or unknown
delivery retains the exact Server proof owner and disposition; no guessed renewal
or old-token promotion occurs after restart. This active lease remains charged
under the bounded registry policy through exact transfer/release custody.

Use the coordinated fields: CapturedToken=(incarnation,generation,revision,index,
parent resolver,scope); SubmissionInputSeal=(captured selection,known file-set
seals,prepared totals/digest,expected head/base/profile); internal CandidateSeal=
(filesystem root,expected head/base,profile,selection,counts/digest); saved tuple=
(serial,selected version,canonical content/
metadata roots,construction Base/profile). InstallCapsule and READY belong to the
Workspace/concurrency packet. READY precedes sending the final composite even
though its FS root is not known yet. This adds no public Stage/CommitStaged round
trip; standalone explicit Stage remains its independent API. Outcome fills fixed
reserved slots, not a complete
new result map after Branch success. Unknown Save/group/Branch results retain
the matching owner independently; no resend or guessed unlink is permitted.

## 12. Costs and scenario coverage

Let B be actual changed wire bytes, E final descriptors, D newly needed draft
metadata, L effect/value/proof records, G visited graph edges and Z released
descendants. W is a fixed window, P an immutable-tree page, and k=8 merge fan-in.
These are cost functions, not measured timings.

| Mechanism | Baseline cost/cause | Proposed cost and remaining price |
| --- | --- | --- |
| File input RAM | O(E) sender arrays and per-file retained draft population | Fixed windows/frontiers/cache; D on disk. Sender changes are Workspace scope |
| Fresh payload | Server writes then rereads B before canonical Store work | One direct Server consume of B into private C2; Workspace/transport/Store bytes still real |
| Inherited edit | Changed-byte replay plus proportional structural state | One replay of B, existing compare/build reads and sparse Base boundaries; metadata IO for D |
| Tiny-file coordination | F RPC/Save arena/finish/index-clone units | O(ceil(F/512)) natural units when byte target permits; not a wall-time claim |
| Input names | Encoded whole directory plus decoded whole directory | O(W) simultaneous decode, O(N) sealed input disk where required |
| Effects/join | Full arrays/maps, no Server spill backing | O(W+kW) resident; ordered runs O(L log_k(L/W)) transfer work in worst external sort |
| Topology | Repeated relevant subtree/base alias walks and resident maps | Exact external G/stack; completed memo visits one final subgraph proof per selection where applicable; arbitrary traversal still O(G) work |
| Pure move | Broad graph proof can dominate fast local rename | Legacy bounded graph work; selected v2 verified parent index gives changed/ancestor paths rather than descendant discovery |
| Final memory | Unqualified combined RSS/engine/cache | Enforced distinct windows+global engine/deployment envelope, requiring actual proof |
| Final Store space | Existing codecs/packing; accepted history tolerance | Metadata backing alone preserves identity; v2 adds parent-index pages and changes FS/FileState/Commit IDs. File-set grouping changes pack/cadence allocation; measure all terms, no automatic space reduction claim |
| Temporary space | File spool + unaccounted simultaneous state domains | Actual B if inherited + input/D/L/G/results + all merge overlap; quota-managed, possibly more metadata IO/space |

Sparse work must track mapping pages/chunks read and untouched roots, separately
from elapsed time. Exact algorithmic RAM can improve while disk metadata IO makes
a particular operation slower. A5% storage gain does not justify50% latency loss.
Preserve accepted5–10% allocation tolerance; record both raw bytes and time under
comparable cache/resource profiles before changing a default.

| Scenario | This packet's mechanism | Remaining cross-packet gate |
| --- | --- | --- |
| SC-01 fragmented/frequent mutation | Final selected descriptors, paged exact drafts/effects/results | Live mutation plans and retirement must also be bounded |
| SC-02 inherited sparse/dense/shrink | Replacement-only CDC, unchanged subtrees, one necessary replay, external structural state | Exact parent-version and larger offset/profile proof |
| SC-03 tiny forests/wide/deep names | Unified file sets, range reservations, binding cursors, global exact graph state | Generic FUSE owner/cookie/listing and complete proof cost |
| SC-04 fresh/append/replace/log | Fresh direct stream or inherited replay through same assembler | Ordinary command still pays full cp/prepend bytes; no #241 ioctl credited |
| SC-05 moves/aliases/remove | Exact effects/add-before-release, authenticated v2 parents and paged release closure | Verified-base/profile certification; selected orphan/pin retirement |
| SC-06 G1/G2/successors | Saved tuples preserve exact local versions; v2 immediate canonical parent | READY/install and independent v2 semantic/root oracle |
| SC-07 refusals/unknown/output | Bounded parser/result framing, per-unit custody and engine/disk refusal | Explicit recovery/discard, full pinned-read framing and FUSE lifecycle |
| SC-08 concurrent owners | One producer per admitted unit, Store2, exclusive byte windows, finite serial headroom | Event supervisor/control pool and per-Workspace publication/isolation |

## 13. Proposed SRP module map and physical-line budgets

Reuse the current packages. The tree is the target responsibility split, not a
directive to create empty modules. Each production file stays below999 physical
lines; every lib.rs/mod.rs stays <=200 and contains declarations/delegation only.

```text
layerfs-bridge/src/contract/prepared/
  types.rs       header/totals/cursor-facing checked records          <=350
  read.rs        incremental prepared parser and fixed sink           <=550
  write.rs       existing grammar encoder over bounded cursors        <=350
layerfs-bridge/src/contract/file_set/
  types.rs       member/unit/result envelope and supported caps        <=400
  read.rs        exact sequential member body parser                   <=550
  result.rs      paged96-byte result grammar and terminal seal         <=400
layerfs-bridge/src/adapters/native/protocol/file_set.rs
                 request/terminal framing only                        <=400

layerfs-content/src/filesystem/rows/
  source.rs      minimal logical header/binding/value cursors          <=400
  memory.rs      bounded caller-slice/inline arena adapter              <=350
  spool.rs       fixed slots and binding windows, no full directory    <=650
layerfs-content/src/filesystem/proof/
  subjects.rs    identity/role/allocator preconditions                  <=600
  bindings.rs    exact before/after effects and parent facts           <=650
  graph.rs       external colors/stack and final-graph validation      <=750
  state.rs       typed exact-state codecs/ports                        <=400
  certified.rs   nonforgeable VerifiedNamespace and validation origin   <=350
layerfs-content/src/filesystem/parents/
  codec.rs       v2 parent leaf/branch canonical grammar               <=600
  build.rs       initial exact forward/reverse correspondence           <=600
  update.rs      final-batch parent changes and ancestor proof          <=750
  read.rs        authenticated parent cursor/lookup                     <=500
layerfs-content/src/filesystem/profile.rs
                 v1/v2 root/profile/write-policy dispatch              <=450
layerfs-content/src/filesystem/references/
  effects.rs     compact ordered effect cursor                         <=550
  join.rs        typed values/counts/content-root join                 <=650
  release.rs     cursor-owned descendant release                       <=650
  runs.rs        existing merge/spill budget, fixed fan-in              <=650
layerfs-content/src/file/edit/
  apply.rs       dispatch + current split/join sequencing              <=650
  draft.rs       typed draft/refcount/completion port                  <=500
  state.rs       bounded cache and external state adapter              <=650
  finish.rs      children-first finalization with exact IDs            <=550
  tree.rs        pure existing split/concat/balance operations          <=950
  policy.rs      v1 exact compatibility / v2 immediate-parent recipe    <=350

layerfs-server/src/service/construction/
  admission.rs   unit policy and explicit byte/disk owner              <=450
  file_set.rs    one generic sequential C1/C2 assembler                <=650
  file_input.rs  descriptor/replay/direct source orchestration         <=650
  results.rs     owned result run and known/unknown delivery seal      <=450
  prepared.rs    received immutable rows and phased proofs             <=650
  finish.rs      C2 completion/checked cleanup orchestration           <=400
  namespace_import.rs
                 authorized snapshot cursor + new-scope serial remap   <=750
  verification.rs
                 byte-admitted live verified Base provenance           <=450
layerfs-server/src/host/memory_domain.rs
                 native owned-domain validation/pressure admission      <=550

layerfs-storage/src/construction_state/
  session.rs     private metadata-scratch ownership/lifecycle          <=450
  index.rs       bounded indexed metadata queries/pages/transactions   <=650
  runs.rs        adapter for owned sequential runs                     <=450
  profile.rs     verified scratch engine/cache policy                  <=350
layerfs-storage/sql/construction_scratch.sql
                 private metadata schema, no payload table             <=150
layerfs-storage/src/cas/
  admission.rs   allocation lease plan and ownership transfer          <=550
  batch.rs       pending/consuming/producer coexistence                <=400
  lifecycle.rs   existing private transaction/publication authority    <=650
layerfs-storage/src/sqlite/memory.rs
                 process engine profile/guard readback                <=350
layerfs-storage/src/migration/
  authority.rs   quiesced exact-Store maintenance owner                 <=400
  locators.rs    indexed copy and ordered equality proof                <=650
  transition.rs  small schema swap, bounded retirement/version finish   <=550
  status.rs      manifest phases and known/unknown continuation         <=450
```

Shared low-level canonical codecs/partition functions stay where they are.
Do not duplicate them in Server scratch code. C1's pure algorithms do not open
the scratch DB; the caller-owned adapter implements the narrow ports. Server
orchestration does not implement topology or physical encoding. Adapter SQL is
owned by C2 and has an independent private-format version. A new type is added
only for a distinct lifetime/authority, not an interface/factory per function.

## 14. Estimated production LOC impact

Root's reproduced baseline is reference65,417 + Core70,279 = combined135,696.
Relevant packages total33,029: C1/content13,483; C2/storage8,716; Server3,996;
Bridge6,834. These are baseline counts, not estimates. Root owns aggregate
Workspace/daemon/API/FUSE/C5/transport counting; none is included again here.
Bridge rows below include only prepared/FileSet grammar and canonical profile
envelopes. Owned socket IO, Exec lifetime/heartbeats, control pooling and view-read
framing belong to the concurrency packet and are excluded. New C5/SDK/operator
v2 Stack creation/migration routing is excluded; this packet estimates only
Server import/remap, C1 certification/index/policy and C2 role acceptance.

| Owned implementation area | Added production LOC estimate | Deleted/replaced production LOC estimate | Net estimate |
| --- | ---: | ---: | ---: |
| Prepared/binding cursor contracts and adapters (C1+Bridge) | 600–1,000 | 350–650 | -50 to+650 |
| Exact external graph/effect/value/release composition (C1) | 1,600–2,600 | 1,000–1,700 | -100 to+1,600 |
| Paged draft storage, cache admission and children-first completion (C1) | 700–1,200 | 350–650 | +50 to+850 |
| Generic file-set Server assembler/direct/replay/results (Server) | 800–1,300 | 450–750 | +50 to+850 |
| File-set wire and paged-result codec (Bridge) | 400–700 | 50–150 | +250 to+650 |
| Metadata scratch/index/run adapter and runtime SQL (C2) | 700–1,200 | 100–250 | +450 to+1,100 |
| C2/Server byte/engine/index-overlap and native-domain admission | 450–800 | 100–200 | +250 to+700 |
| Namespace v2 parent/certification/import, C2 v11 roles/metadata migration | 2,300–3,800 | 150–350 | +1,950 to+3,650 |
| File-edit-policy/FileState v2 with v1 compatibility | 250–450 | 50–100 | +150 to+400 |
| **Conservative arithmetic envelope for this packet** | **7,800–13,050** | **2,600–4,800** | **+3,000 to+10,450** |

Endpoint net range combines most optimistic/pessimistic independent bounds; it
is not a forecast that all extremes occur together. A working planning midpoint
is about+6,500 production LOC, pending exact interface scope and deletion of old
materializers. It assumes current codecs/sorted-tree/run/provider logic is reused,
no new general B-tree/runtime, and required safety/custody is retained. New runtime
SQL counts; tests/docs/examples/harness/manifests do not. Symbol moves and splits
are relocation, not new algorithmic cost. Before any future commit, recompute
exact first-parent/staged production LOC with the repository counter and report
reference/Core/combined; these ranges never enter a commit as measured totals.

## 15. Construction milestones and focused future proofs

Build the complete streamed composition, rather than layering patches around the
old population-sized plan. Each milestone has an architecture interface and a
focused proof before its dependent capability is enabled.

1. **Freeze lifetime/authority contracts.** Define FileSetSource, binding/value
   cursors, external state ownership, C2 byte/engine profile and saved seals with
   Workspace CapturedToken/READY/InstallCapsule. Identify every prohibited
   full-population allocation and exact outcome boundary.
2. **Implement the shared streamed body and bounded small path.** Generic one/
   many-member assembler, binding parser/cursors, received selection/seal, direct
   fresh and one inherited replay source. Cache insertion owns admission; lying
   row counts are refused before collection. These are structural invariants
   of the new route, not a separate bug-fix-first campaign.
3. **Implement paged exact construction state.** Current file algorithm over
   draft ports; graph/refcount/release joins over typed external state/runs.
   Establish exact canonical root/closure equivalence and no emitted speculative
   mappings. Keep small input on the same semantics without mandatory scratch SQL.
4. **Implement the selected v2 identity/locality architecture.** Certified parent
   relation, immediate selected-parent edit policy/FileState, explicit new-Stack/
   new-scope import and paged remap through existing C5 initialization, with
   quiesced metadata-only Store v11
   migration. Freeze independent v2 expected roots and
   retain exact v1 compatibility evidence separately. A bounded old recipe is
   never an end-state or error fallback.
5. **Compose Store/unit/result completion.** Bounded waves/private visibility,
   checked input/proof/cleanup, paged result terminal and unknown delivery. Then
   compose finite serial headroom and Workspace READY; no proportional allocation
   remains after Branch send.
6. **Qualify engine/physical and concurrent admission.** Actual simultaneous
   allocations, shared/private/clone peaks, SQLite guard/cache readback, physical
   cache and delayed consumer behavior. #249 enablement depends on #248 memory,
   identity/final-delta and custody gates, not merely this stream API existing.
7. **Enable only proven next optimizations.** Interior draft finality has its own
   equivalence/work gate. Larger file/count/
   disk profiles are separate prospective registrations, not relaxed old rows.

Focused future tests use production interfaces and external oracles only:

- Parser: one maximal binding, a very wide directory, exact empty row, unsorted/
  duplicate names, root binding, surplus/short bytes, count/byte overflow and
  lying-small-name/large-byte totals; ensure fixed-window refusal/custody.
- Canonical files: cutoff transitions, no-op bytes, sparse/dense edits, insert/
  delete/shrink/hole, repeated/backward ranges, G1/G2 and retained old selections;
  exact roots/partitions/objects versus independent sealed reference.
- Draft/cache: more than64 distinct loaded pages, shared/repeated child drafts,
  detached chains and encoded/decoded overlap; no missing admission path or
  emission of discarded mapping state.
- Graph: fresh/disconnected cycles, cross-move cycles, two parents, legal regular
  aliases, fresh orphan errors, root exception and released aliases; deep/wide
  populations and exact final-batch oracle; independently valid but mismatched
  inode/parent roots, forged provenance, missing parent rows and stale Base fail
  certification. V2 incremental proof observes all cross-move effects. Restart/
  opaque-root reads never turn authentication alone into VerifiedNamespace.
- Profiles/migration: exact v1 read/write roots retained; v2 policy/layout against
  an independent v2 ledger; new scope/range remap, complete forward/reverse
  correspondence, reused file/attribute bytes, changed FS/Commit identities and
  unknown initialization custody. No old receipt relabeling or silent upgrade.
- Store metadata migration: exact old/new locator fields, unchanged pack BLOBs/
  ordinals/save IDs, indexed copy/delete page bounds, small schema transactions,
  quiescence refusal with an old active user, every explicit known/unknown phase,
  only-empty DROP, final schema capability and physical high-water/freelist cost.
- File set:1/2/512 members and byte-bound singleton, output exceeding32 KiB,
  exact token/order/digest/terminal, withheld terminal, partial result and lost
  finish; no group resend and no guessed publication.
- Resources: slow consumer under existing one producer, two independent admitted
  units, shared/private publication clone overlap, engine NOMEM/transaction/
  cleanup outcomes, finite range exhaustion with Store2 occupied, exact release.
- Ordinary SDK/FUSE SC01–08: no special edit tool or command-selected route;
  prospective barriers/oracles/timers/cache scopes and retained failure rows.

No test/benchmark commands are launched by this packet. Future measurement uses
existing harness/report mechanics, one registered sample and focused final proof.
Reuse unchanged earlier-family evidence, especially Family2. File-set packing/
Save cadence is a changed physical mechanism and requires its necessary affected
storage checkpoint; do not declare broad seals alone as a reason for an entire
history rerun. Numerical cache INELIGIBLE stays distinct from functional success.

## 16. Baseline source map

- [Prepared wire/sink](../../../../crates/layerfs-bridge/src/contract/prepared_stream.rs),
  [wire request limits](../../../../crates/layerfs-bridge/src/contract/request.rs),
  [native framing](../../../../crates/layerfs-bridge/src/adapters/native/protocol/frame.rs),
  [native input/output](../../../../crates/layerfs-bridge/src/adapters/native/payload.rs).
- [Server receive](../../../../crates/layerfs-server/src/service/save/prepared.rs),
  [file replay](../../../../crates/layerfs-server/src/service/save/file_stream.rs),
  [file origins](../../../../crates/layerfs-server/src/service/save/file_stream/origin_runs.rs),
  [file mutation](../../../../crates/layerfs-server/src/service/save/content.rs),
  [filesystem composition](../../../../crates/layerfs-server/src/service/save/filesystem.rs),
  [stage/Branch ordering](../../../../crates/layerfs-server/src/service/save/catalog.rs),
  [Service writer admission](../../../../crates/layerfs-server/src/service/handler.rs).
- [C1 row ports](../../../../crates/layerfs-content/src/filesystem/rows/source.rs),
  [row spool](../../../../crates/layerfs-content/src/filesystem/rows/spool.rs),
  [update populations](../../../../crates/layerfs-content/src/filesystem/update.rs),
  [alias/subject validation](../../../../crates/layerfs-content/src/filesystem/validate.rs),
  [effective cycles](../../../../crates/layerfs-content/src/filesystem/validate/cycles.rs),
  [reference rows](../../../../crates/layerfs-content/src/filesystem/references/record.rs),
  [reducer](../../../../crates/layerfs-content/src/filesystem/references/reduce.rs),
  [ordering runs](../../../../crates/layerfs-content/src/filesystem/references/runs.rs),
  [release frontier](../../../../crates/layerfs-content/src/filesystem/references/release.rs).
- [C1 edits](../../../../crates/layerfs-content/src/file/edit/apply.rs),
  [draft state/split/join](../../../../crates/layerfs-content/src/file/edit/tree.rs),
  [cache admission](../../../../crates/layerfs-content/src/file/mapping/read.rs),
  [fresh builder](../../../../crates/layerfs-content/src/file/mapping/build.rs).
- [C2 pending wave](../../../../crates/layerfs-storage/src/cas/batch.rs),
  [Save ownership](../../../../crates/layerfs-storage/src/cas/owner.rs),
  [transaction/publication](../../../../crates/layerfs-storage/src/cas/lifecycle.rs),
  [private visibility](../../../../crates/layerfs-storage/src/sqlite/ownership.rs),
  [cleanup](../../../../crates/layerfs-storage/src/sqlite/cleanup.rs),
  [engine profile](../../../../crates/layerfs-storage/src/sqlite/connection.rs),
  [codec arena](../../../../crates/layerfs-storage/src/encoding/codec.rs),
  [pool index](../../../../crates/layerfs-storage/src/encoding/pool/index.rs),
  [content index](../../../../crates/layerfs-storage/src/encoding/delta/candidates.rs),
  [role/schema constraints](../../../../crates/layerfs-storage/sql/schema.sql).
- [C5 range authority](../../../../crates/layerfs-history/src/sqlite/allocation.rs),
  [C5 conditional Commit](../../../../crates/layerfs-history/src/sqlite/commit.rs),
  [C5 new Stack initialization](../../../../crates/layerfs-history/src/sqlite/layerstack.rs),
  [current checkpoint](../../../issues/286/SEVEN-FAMILY-CHECKPOINT-20260930.md).

Source-confirmed observations and prior finite receipts remain distinct from
proposed after-state. Current10240 stays UNKNOWN/OWNER-DEFERRED/SKIPPED; historical
dirty-exit cleanup and cache verdicts are not rewritten by this architecture.
