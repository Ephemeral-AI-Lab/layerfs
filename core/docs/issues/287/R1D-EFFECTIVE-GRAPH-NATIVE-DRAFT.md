# R1d-effective-graph: native authority and resource draft

> **Status: Proposed native design; not a shared implementation freeze.**
> Source inspected: published `79639cc6f58ae78eeb1fb638996b1d6948767215`.
> User steering permits explicitly configurable metadata temporary scratch.
> This document selects no product table tags, changes no default and runs no
> Cargo, provider body or benchmark. Root retains format/tag allocation and
> acceptance. C1 owns graph semantics and source construction. This owner owns
> only this native draft until the shared freeze.

## Delivery boundary and source evidence

The next proposed delivery pages **post-Site cycle/reachability state only**.
The actual `validate.rs::check_sites_selected` verifies/finalizes/retires Sites
before `check_remaining` calls the cycle/build checker. `update/sites.rs` creates
DirectoryRoots after that checker returns. This supplies a real sequential
Sites1 → Graph2 → Roots3 lane in a new private profile, while earlier profile
versions retain their existing phases and bytes.

Alias `pending`/`seen` in `validate/site_aliases.rs` and the positive/absence
memo in `validate/facts.rs` remain explicit open authorities. Alias traversal
occurs while Sites are live in Facts. The measured maximum-width Sites body
uses3,932 of4,096 pages, or16,105,472 page bytes; the remaining164 pages are
671,744 bytes. Site60 at65,536 rows uses3,932,160 framed bytes, leaving196,608
bytes below the4,128,768 logical ceiling. Neither remainder admits an additional
complete graph or base-fact population. Do not apply `max` to populations that
coexist or quietly allocate another16MiB owner for one construction.

The default ordering allowance is64MiB (`references/runs.rs`), giving65,536
examined work/memo entries under `validate.rs::walk_limit`, but4,194,304 declared
touched serials at the independent sixteen-byte rate. Server D/B limits do not
certify the base graph population. The v1 `FilesystemRoot` carries only profile,
scope, root serial and inode-table root; inode branches carry max-key/OID child
descriptors, not certified subtree population counts.

The source bound for updates is at most B initial directory seeds plus at most G
directory children discovered during G examined work: N≤B+G, E≤G. This is not
a proof that N+E fits the selected record capacity. A default worst-bound
precheck B+2G≤65,536 would reject every
default G=65,536 update, even a tiny actual graph. No population certificate,
automatic whole-base preflight or implicit G clamp is selected.

## Admission and supported envelope

The new version4 graph capability selects one immutable physical scratch profile
S before source/native issuance. Default and minimum remain16MiB. S must be an
exact multiple of4,096; reject unaligned values instead of silently rounding.
The proposed derivation version1 is selected by private profile/version4:

```text
S = explicitly selected metadata temporary scratch bytes
R = checked(S / 256)
L = checked(R * 63)
P = checked(S / 4,096)
N + E <= R
graph framed bytes = 60*N + 43*E <= L
native allocation = S
page size/max pages = 4,096 / P
connection cache = 512KiB
logical mutation/page window <=128 records AND64KiB including its header
```

Default S16,777,216 reproduces R65,536/L4,128,768/P4,096 exactly. Full mutable
Node60/Edge43 payload satisfies60N+43E≤60R; fixed metadata/indexes/freelists and
actual allocator/native/window owners remain separate, not free slack.
Configured capacity is not a separate65,536 hard ceiling that defeats S, and
not a per-table R cap permitting2R records. Prepared count65,536, file4GiB,
default examined work65,536, cache512KiB,128/64KiB RAM windows, workers and
timeouts remain their separate unchanged constraints.

For page alignment and u32 discovery/lowlink/component/incoming representability,
the proposed formal range is16MiB≤S≤1,099,511,623,680 (1TiB−4,096), giving
R≤4,294,967,280. This is a representation ceiling, **not a qualified profile**.
Validate checked arithmetic, selected page count, signed native offset/length
and platform-size conversions before any source/token/slot/native/SQL/Save
effects. Actual linked SQLite must read back P exactly; an engine clamp/refusal
is unsupported, never accepted as the requested profile. Native physical
allocation must observe exactly S; a provider that rounds a page-aligned S to
another physical amount remains an explicit unsupported/refused scope.

S/R/L must be copied into the actual operation and bound in subject/scope/header/
native binding/owner/seals. Every decode recomputes version1 R/L and rejects a
forged tuple. No post-admission config change alters an active or retained
owner. No automatic growth, retry or error-driven selection is permitted.

Root's configured Service/Server constructors retain old constructor signatures
as16MiB-default delegates; do not add a mandatory StoreAccess/ServerConfig field.
The proposed native operator setting is `LAYERFS_CONSTRUCTION_SCRATCH_BYTES`,
parsed before Store/history/workers. It budgets one native indexed
construction-state owner only. Input RowSpool retains its separate declared
stream-plus1MiB bound; ordering runs and live Workspace metadata retain their own
admission. Aggregate whole-operation temporary-disk admission remains open:
16MiB or48MiB selected here is not a total for all temporary metadata.

Mixed old16MiB/newS owners use checked sums of each exact active/retained
reservation. Replace no accounting with `count*current_config` or
`count*DEFAULT_BYTES`. Native verification/reservation/allocation observation,
status, quarantine, retained cleanup and final refund use that owner's captured
budget and never the current operator setting. Earlier profiles1–3 retain16MiB
and their exact API/header/binding behavior even in a mixed authority.

N and E count distinct selected node and `(parent,child)` edge records.
Multiplicity is logical source-edge work, not a way to evade the old work
ceiling. Every examined name/base page/changed binding keeps its actual charge,
including repeated endpoints whose edge record already exists.

The earlier row ceiling was not inferred from bytes. `R1C-FROZEN-STATE.md`
explicitly describes the16MiB/max65,536-row class; the claims delivery records
both4,128,768 bytes and65,536 rows. `Plan::phased` enforces max(D,B) because
those populations are sequential. It supplies no permission to admit131,072
graph nodes plus65,536 edges under its16MiB profile. User-configured version4
may explicitly select a different derived R, with its owning proof and recorded
supported scope; older versions remain exact16MiB. `R1D-PAGED-ALGORITHMS.md` requires summing live
populations and explicitly refusing shapes beyond the selected class.

Admit this finite native class and exact context before token/slot/file/SQL/Save
effects. That is **selected-capacity admission, not full-input fit admission**.
For each construction batch, pure live scope/source/budget/stage/current-failure,
framing/window and provable-overflow checks precede queries. Computing exact
new distinct membership may require bounded selected PK reads: validate every
selected stored row/projection and all proposals, then refuse aggregate growth
before reservation, pending write attempt or writes. Recheck exact selected
membership/counters under the transaction. Do not claim both zero SQL queries
and validation of unknown stored membership. A pure EXCLUSIVE/Busy proof uses
keys provably new above the acknowledged MAX; unknown membership exercises the
bounded-read path and retains its first real provider failure.
Previously accepted private rows remain known on a later capacity refusal;
canonical namespace construction has not begun. The result is explicit known
cleanup/CAPABILITY-LIMITED for that larger scope, not PASS or fallback.

Do not reduce G, B, workers, deadlines or source depth to fit a case. Retain old
public resident APIs as explicitly selected compatibility paths. A failed new
native operation never invokes one. Full prospective population admission
would need a separately supplied, source-verified bound or a real product-work
preflight; that capability is presently unavailable.

For this slice, the frame sum is legitimate only after acknowledged Sites
retirement and before roots:

```text
operation logical phase peak = max(60*B, 60*N+43*E, 63*D)
```

The physical owner is not refunded between phases. Required fixed owner/solver
metadata, old B-trees/freelists, current windows, attempt capsules, actual Vec
capacities, hashers, statements, SQLite cache/MEMORY journal and OS cache are
additional simultaneous owners. Resident facts/alias state also remain live
where the source keeps them; this expression does not qualify whole-process
memory or finish their R1 gates.

## Closed records and context

The proposed full Node60 frame is key25/value29/framing6. Its value is
`flags1/discovery4/lowlink4/DFSparent8/afterChild8/incoming4`, all unsigned
big-endian scalars. Discovery and lowlink are bounded by the selected node
population. Completed lowlink holds the component root discovery ordinal; the Completed flag
distinguishes it from an active lowlink. Parent/after zero means absent and
positive values remain ≤i64MAX. Incoming is checkedu32 and corresponds to
acknowledged incoming multiplicity.

The proposed Edge43 frame is key33/value4/framing6. Its typed local key is
parent8/child8 and its value is positive checkedu32 multiplicity. Keys retain the
existing full token/phase/table prefix; no serial/name truncation. Only the
new implemented codecs may allocate their table tags at the root's freeze.

C1's proposed flag grammar is Update Seed1, Expanded2, Fresh RootReached4,
solver OnStack8/Completed16/DfsFinished64 and immutable SelfLoop32. Bit128 is invalid.
Update birth flags are0/1; Fresh birth is4. Solver fields are zero during birth;
incoming is accumulated during construction. Expanded/SelfLoop become immutable
when the topology closes. Fresh contains only directories reached from root;
it verifies each eligible fresh-directory declaration by exact node lookup after
closure and performs no fresh SCC interpretation.

GraphScope must bind the same live issued selection/native owner, node/edge
codecs, actual BindingSourceId, namespace scope, base filesystem-root option,
root serial, mode and selected S/R/L. Values cannot be selected by raw IDs or a
guessed token. The proposed graph subject106 is:

```text
SID8 / namespaceScope32 / mode1 / basePresent1 / baseFSRoot32 / rootSerial8
/ scratchBytes8 / recordCapacity8 / logicalByteCapacity8
```

Proposed GraphScope188 is node StateScope81 / edge table code1 / subject106.
All scalars use fixed big-endian encoding. Absence uses a presence byte and
zero base placeholder, not an ambiguous object-ID sentinel. Mode/namespace/root
and selected tuple validation precede effects. C1 proposes Fresh1/Update2;
mode follows absent/present Base, not an independently forgeable argument.
GraphCapacity::new(S) recomputes R/L/P; getters are scratch_bytes/records/
encoded_bytes/max_pages and check_growth validates exact N/E/distinct increments.
The exact mode/table byte allocation remains a root shared-freeze dependency; this
draft introduces no product tags or opcodes.

The proposed private version4 is a distinct schema/header profile. Keep the
LFCS application identity and existing MEMORY/OFF/no-WAL/no-sync, workers0,
busy0, mmap0,512KiB cache, length/SQL/column/variable/expression/VDBE/trigger limits.
The tentative header298 is `LFCSOWN4`/u16version4/reserved6zero followed by the
unchanged token/selector/binding/nonce/parent/directory/file prefix through192,
then subject106. Native binding is existing nonce32/identities72/selector32/token8
plus subject106 under `layerfs/construction-state/native/v4\0`. Hash the supplied
subject arguments **before** bind_owner; hashing GraphScope's resulting binding
would be circular. Private version4 plus this domain fixes derivation version1;
S/R/L are explicitly encoded and recomputed. A different derivation requires a
different version/layout, not an inferred tuple. Exact bytes must be published
at the root's shared freeze. Earlier
v1/v2/v3 headers/bindings/SQL/API behavior stay unchanged.

## Proposed native schema and indexed operations

Prefer fixed value BLOBs plus the two necessary checked projections:

```text
graph_nodes: key BLOB25 PRIMARY KEY, value BLOB29,
             flags INTEGER, discovery INTEGER
graph_edges: key BLOB33 PRIMARY KEY, multiplicity BLOB4
graph_unexpanded: partial index over node key WHERE (flags&2)=0
graph_discovery_stack: unique partial index over discovery WHERE (flags&8)=8
```

Use STRICT WITHOUT ROWID. Borrow/check all BLOB types/widths before copying.
Decode the full Node60 and compare flags/discovery projections to the decoded
value before any trust, lookup, solve step or CAS. Match exact selected key
prefix/class/source and reject invalid reserved bits/mode combinations. Edge
decode checks exact prefix, positive parent/child ranges and multiplicity.

Both endpoint nodes must exist and be checked before an edge insertion or
multiplicity increment. Derive their exact typed node keys from the endpoints;
do not add an unnecessary endpoint index or guessed FK adoption path. Outgoing
edges use one exact parent-prefix PK range, strict `child>afterChild` ordering
and bounded LIMIT. No name history, prefix COUNT/OFFSET/rank, full collection or
unindexed ORDER BY is needed.

Construction picks the next unexpanded key from the actual partial index and
streams that directory once through the existing bounded effective-entry merge.
It closes the complete source cursor before marking that node Expanded. The
unexpanded index must be exactly empty at topology closure. Solving begins only
after acknowledged closure/native observation, so live unexpanded rows and the
solver stack do not silently overlap; their allocated pages nevertheless remain
owned and count in the file's physical high-water.

The solver retains one current serial and bounded mutation window. DFS parent
and outgoing after-child continuation live in nodes. The onstack discovery index
supplies descending SCC members using an exact discovery boundary and LIMIT≤128.
It does not scan prior components. Discovery is a checked monotone scalar;
component root discovery is bounded by N and is never a high serial or a
separate arbitrarily assigned component counter.
Completed nodes leave the stack index. DfsFinished requires exact indexed
outgoing EOF; after-child advance must match the actual FIRST edge after the
old cursor, not any arbitrary higher key. Child return requires the stored
child.parent==parent and parent.afterChild==child. Keep parent through return.
Never compare a completed lowlink/component
ordinal as an active lowlink; phase/status distinguishes the reused field.

The proposed concrete schema4 fixed metadata is:

| Table | Exact proposed columns |
| --- | --- |
| `session_owner` (12) | id, header298, root_scope81/null, root_sealed, root_records, root_record_bytes, root_digest32/null, declared_D, declared_B, selected_S, selected_R, selected_L |
| `graph_owner` (16) | id, graph_scope188, stage, nodes, edges, record_bytes, source_multiplicity8, remaining_nodes, remaining_edges, adjacency_seal285/null, proof_seal317/null, max_node25/null, max_edge33/null, after_node25/null, after_edge33/null, seed_count |
| `solver_owner` (11) | id, graph_scope188, current_serial, dfs_root_serial, next_discovery, scc_root_serial, scc_boundary_discovery, cumulative_pop_count, last_stack_discovery, any_seed, singleton_self_loop |
| `graph_nodes` (4) | key25, value29, checked flags INTEGER, checked discovery INTEGER |
| `graph_edges` (2) | key33, positive multiplicity4 |

Site owner/table layout remains its actual fixed phase under this profile.
Every singleton id is1. Every table is STRICT WITHOUT ROWID; checked BLOB widths,
u32-global counter bounds and positive/zero serial roles apply. `graph_owner`
CHECKs nodes+edges≤u32MAX and record_bytes=60*nodes+43*edges. Selected R/L are
validated against `session_owner` and native captured profile, then checked
before effects and again under each transaction; do not introduce hidden
independent per-table caps. Source multiplicity is exact checkedu64 BLOB8;
its logical work cap remains C1's unchanged allowance, not selected S.
Distinct Seed count is checked against the captured declared B even when R is
larger. Seed idempotence cannot invent another declared input population.
Cumulative SCC-pop count/anySeed/singleton-self-loop are fixed scalars, not an
unbounded component lookup table or an extra component index.

Proposed stages are Building, AdjSealed, Solving, Solved, Retiring, Retired.
Fresh skips Solving after exact root-reached verification. Solving requires
Update/approved topology. Nullable seal/MAX/cursor fields must match the actual
in-memory acknowledged capsule and empty/nonempty counts. All SQL queries and
updates stay≤16 columns/8 variables. Exact predicates use selected scope/stage,
old counters/old values; a count update binds the four new N/E/bytes/multiplicity
values plus scope/stage/oldN/oldE, not a17th column or nine parameters. Solver
metadata mutations update only their required bounded subset under exact old
state, closing statements between windows. No per-node/session registry.

## Closed mutations, acknowledgement and Unknown

Use the exact proposed names/closed enum in the
[C1 draft](R1D-EFFECTIVE-GRAPH-DRAFT.md#proposed-concrete-c1-port): capacity,
seed/root/unexpanded/append/expanded/seal, selected node/edge pages, begin_scc,
CAS/pop/finish, proof pages/retire/abandon. Append proposes at most63 raw children
because one parent plus63 child nodes plus63 edge records can fill127 affected
records. All nodes/edges share one128-affected-record window, not128 of each.
GraphBuildAck carries bounded exact oldOption/new node and edge changes, added
N/E, old/new totals and updated parent; C1 verifies every member before accepting
its delta. The same-batch source children may repeat and need checked ordered
multiplicity grouping; duplicate solver CAS targets are rejected before effects.
Fresh excluded parents are RootReached zero-outgoing expanded leaves.

The native port is a closed graph producer/solver, not arbitrary key/value put:

| Domain | Allowed effects and required custody |
| --- | --- |
| Birth/edge merge | Exact new node identities, mode-valid births, endpoint existence, checked edge multiplicity/incoming and distinct N/E increments. Retain every requested key, expected absence/old frame, proposed full node/edge frame, old/proposed counts and source-work total. |
| Expansion completion | Exact selected unexpanded node, complete current source cursor/EOF evidence, immutable self-loop/expanded projection and checked incoming/edges. Retain old/proposed node and cursor/completion context. |
| Topology close | One transaction checks all nodes Expanded, indexed key order, exact endpoints/incoming/multiplicity/totals/EOF and immutable node/edge projection seals. Retain expected/proposed seals/MAX/counts before COMMIT. |
| Solver step | EnterRoot(any white), atomic Descend(parent old/new + child old/new + selected edge), Advance, Finish, Return and metadata-only LeaveRoot. Descend is two affected targets in one transaction, with no acknowledged white-child gap. Every step validates exact frames and current/root/discovery state. Advance matches native FIRST edge after old cursor; Finish requires indexed EOF; Return checks child.parent and parent.after. No receipt/issuer or arbitrary field CAS. |
| SCC pop | Actual descending OnStack window, full completed member views, cumulative count/anySeed/singleton-self-loop and exact component-root discovery. Native terminalizes a cyclic Seed component at known closure and returns a bounded final member Ack with RejectedCycle disposition; C1 independently folds those members/flags/count/component before producing its semantic error, without postfailure queries. No scan of prior completed components or full component index. |
| Solution close | Rehash the immutable topology projection and current full Node60/Edge43 streams in the same transaction; compare the approved topology, terminal node statuses, empty stack and exact count/EOF. Retain proposed solution seal/MAX/solver terminal state. |
| Retirement | Exact sealed keys deleted in≤128-row batches with old/proposed remaining counts and after keys; verify node/edge tables and both projections empty. COMMIT and native allocation observation precede root permission. |

Every CAS batch validates **all** expected/proposed frames and all selected
stored values/projections before the first UPDATE. Conditional exact old
value/projection matching must affect one row per mutation. Same-batch duplicate
solver targets are rejected before effects; successive transitions have separate
acknowledgements. An unchanged legal mutation is explicitly acknowledged.

A final SCC-pop rejection is a known semantic result like claim Duplicate:
return the last bounded completed-member window and cumulative scalars with
RejectedCycle while own phase is terminal/roots denied. Do not throw away that
window, then query a failed owner to reconstruct it. Only known SQL/native
acknowledgement can return this disposition; COMMIT/observation uncertainty
retains the exact pending closure and returns its typed failure/Unknown instead.
This final-Ack disposition is proposed for root shared-freeze confirmation.

The window limit is128 logical mutations, not128 total owners. Old128 plus
proposed128, caller input, selected native edge/current page and
retained attempt capsule can coexist. Their actual capacities/element sizes,
fallible reservations and release order require separate source/allocation proof.
Do not claim7,680 encoded Node bytes as the whole allocation peak.

Begin/write/COMMIT occur once. Definite failure has one checked rollback;
COMMIT or failed rollback Unknown keeps the exact old/proposed mutation, stage,
scope/source, selected edge/cursor, counts, seals/MAX and SCC boundary. No resend,
query adoption, recomputation from an uncertain table, refund or destructive
cleanup. Metadata-only abandon is selected, sticky and idempotent, preserves
the original typed provider failure and poisons root eligibility; a foreign scope
cannot terminalize another owner. Drop retains the admitted native capsule.

Known graph retirement reuses the same file, not a new owner. Even a known final
SQL deletion does not unlock roots before physical observation succeeds. Graph
and Sites reader/phase gates, all selected seals, statements and owner cleanup
remain part of the one exact session; no lifetime phase scan/registry is added.

## Proposed exact seals and scans

Align with C1's proposed GraphAdjacencySeal285: version1/scope188/N8/E8/
nodeBytes8/edgeBytes8/nodeProjectionDigest32/edgeDigest32. Node adjacency
projection retains Seed/Expanded/RootReached/SelfLoop/incoming, masks solver
flags and normalizes discovery/lowlink/parent/after to zero. It is folded in
strict node-key order under `layerfs/effective-graph/adjacency-nodes/v1\0`.
Edges fold in strict edge-key order under
`layerfs/effective-graph/adjacency-edges/v1\0`. Both transcripts include exact
scope, full projected/current frame, terminal count8 and recordBytes8.

GraphProofSeal317 adds adjacencyDigest32 to version1/scope188/totals32/
fullNodeDigest32/edgeDigest32. The adjacency digest hashes the entire285-byte
acknowledged adjacency seal under `layerfs/effective-graph/adjacency-seal/v1\0`;
full current nodes use `layerfs/effective-graph/proof-nodes/v1\0`. Final native
close rehashes immutable adjacency and full current rows in the same transaction.
No candidate snapshot supplies independent expected semantics.

Node page header318 is adjacencySeal285/lastPresent1/key25/count2/bytes4/EOF1.
Proof node header350 substitutes proofSeal317. Selected-parent edge header318
is adjacencySeal285/parent8/lastPresent1/lastChild8/maxPresent1/maxChild8/
count2/bytes4/EOF1. Positive parent/child ranges and exact selected cursor are
validated before SQL/capacity/page allocation. Empty exact EOF is legal; empty
non-EOF is not. Node complete count and acknowledged MAX, edge parent MAX and
C1's independent complete-stream digest establish termination; no extra129th
row, repeated prefix COUNT or stale cursor guess is used.

Adjacency NodePage exposes normalized immutable projection only; callers take
its keys and call graph_node for fresh full mutable frames before CAS. An
adjacency seal cannot authorize arbitrary current-row pages. Proof pages expose
full final frames under the proof seal. No invalid selected cursor gets a
final/empty page or capacity refusal before its context/order validation.

Graph owner persists the full285/317-byte seals rather than one ambiguous digest
that omits the separate C1 digests/totals. Both typed seals and native
MAX/retirement cursors remain in the exact retained capsule on Unknown.

## Prospective real-provider maximum and refusal proof

All new cases are declared before running. They supplement the existing Site
receipts, not rerun them to obtain a nicer number. Native node-heavy, edge-heavy
and deep cases must follow a legitimate graph grammar and logical work budget;
fabricating all fields at simultaneous impossible maxima is not a proof.

- Node-heavy65,536 distinct empty directory seeds, E0: actual unexpanded-index
  occupancy, closure, discovery/component ordinals and completed full values.
  Parent/after remain zero where that graph requires them; do not pretend this
  fills the widest DFS pointer state.
- Deep chain N32,768/E32,767, with serials near i64MAX: actual parent and after
  pointers require full8-byte values, discovery/lowlink grow, stack reaches real
  depth, both partial-index phases and their page reuse are observed. Include
  the exact65536th record with one legal isolated seed if it is a supported
  update grammar. No names/depth Vec is retained by the solver.
- Edge-heavy fixed DAG mix, for example N512/E65,024, with independently
  generated positive high serials, ordered outgoing rows and real SCC/source
  work. Confirm per-mode semantics separately from storage-format validity.
- Checked repeated-edge/multiplicity case with one incoming total up to the
  legitimate65,536 source-edge allowance. Do not set every node's incoming to
  65,536 while claiming a65,536-edge work ceiling.
- Predecessor widest Sites → known exact retirement → each graph case → known
  graph retirement → maximum DirectoryRoots in the same selected-S file. At
  default S this remains exactly16MiB. Observe
  actual page_count/freelist/native blocks/identities/high-water, both indexes
  and stages. Site3,932 pages is prior high-water, not a graph-fit proof.
- Explicit configured profiles:16MiB default plus selected48MiB, with exact
  S/R/L/P subject/header/binding/readback, actual blocks and old16/newS mixed
  active/retained credit sum. Exercise a graph above default R that fits the
  larger admitted R without changing prepared/work/RAM limits. Declare its
  actual dimensions and proof separately; reuse default passing bodies.
- Pure invalid minimum/alignment/overflow/forged-derived/native-conversion
  configurations refuse before issuance/slot/file/SQL/Save. The formal maximum
  is arithmetic-only; do not allocate1TiB as an incidental correctness check or
  claim it supported from decode success. Readback/actual reservation refusal
  retains truthful source/error/owner effects, with no rounding or retry.
- Actual locked admission boundary: full selected aggregate count, then a row/
  edge with key above acknowledged MAX (provably new) refuses before attempt/
  reservation/query/write under an independently demonstrated EXCLUSIVE/Busy
  barrier. Unknown selected membership gets bounded reads before exact distinct
  refusal; it is not described as a zero-query result. Existing-key increments
  do not falsely consume distinct capacity and cannot evade work/multiplicity
  bounds. Include one refused node+edge composite where neither row lands.
- Real fresh-owner SHARED/COMMIT Unknown at birth/merge, expansion, topology
  close, solver transition/SCC-pop, solution close and retirement. Require exact
  attempt capsules, original Busy/Unknown, no root permission/refund/retry,
  retained same file/class and helper reaping.
- Exact query plans for PK outgoing continuation, unexpanded and discovery
  stack; corrupt redundant projections, wrong scope/source/mode/stage, forged
  seals, invalid/missing/surplus EOF, known partial retirement and error/cleanup
  precedence. Independent C1 graph/SCC/reachability oracle and unchanged v1
  canonical-root proofs remain separate from resource/physical proof.

Before declaring support, the exact widest valid cases must fit their explicitly
selected native/page class and demonstrate healthy component progress. The
S/256 derivation is prospective admission arithmetic, not a physical-fit theorem.
A failure keeps its actual source/output and scope unsupported; no autogrowth, index omission,
invalid-value shortcut or fallback repairs the verdict.

Engine/global heap, MEMORY-journal/cache/OS observation, Linux/Darwin differences,
alias/frontier memo overlap, full predictable population admission, larger graph
profiles, certified v2 namespace parent authority and StrictServerMemory remain
open. Adjacency construction now precedes SCC validation, so a later source/
provider/budget failure can precede a cycle the older per-seed walker found
sooner; record that phase ordering transparently, without synthetic success.
#288 qualification is delegated/unrun; no speed or release admission follows.

Latest user direction is to finish this selected graph checkpoint, owning checks,
exact LOC, publication and #287 update, then pause implementation for review of
existing eight-family speed evidence. This draft authorizes no next implementation,
new campaign or benchmark rerun.


## Owning checkpoint resolution

The [coherent delivery](R1D-EFFECTIVE-GRAPH-DELIVERY.md) records actual owning
checks, exact scoped provider outcomes, retained failed attempts/corrections,
final source seal and remaining capability gates. Earlier draft UNRUN wording
describes preparation before those checks; it is not a final execution verdict.
No speed/global/physical/Linux/R1-R7 qualification is promoted.
