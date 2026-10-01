# R1d-effective-graph selected implementation contract

> **Status: Current planning checklist; no release candidate exists.**
> Shared implementation freeze against published parent
> `79639cc6f58ae78eeb1fb638996b1d6948767215`.
> All new implementation/provider/check exits remain UNRUN. This file freezes
> the named delivery before product edits; it does not claim PASS or publication.

Root selects the coordinated [C1 proposal](R1D-EFFECTIVE-GRAPH-DRAFT.md),
[native proposal](R1D-EFFECTIVE-GRAPH-NATIVE-DRAFT.md) and
[integration plan](R1D-EFFECTIVE-GRAPH-INTEGRATION-DRAFT.md), with the concrete
allocations and clarifications below. These decisions replace their marked
review gates. Any subsequent contract correction must be recorded before the
corresponding dependent edit, with actual source/evidence and owning review.

## Scope, source and stop boundary

Deliver one complete post-Site-retirement effective cycle/fresh reachability
path through actual C1/C2/strict prepared Server. Replace repeated per-seed
subtree walks and their resident graph populations with one effective graph
construction and external selective SCC proof. Preserve canonical-v1 and
explicit old API/profile routes. SC-03/05/07/08 are affected.

Alias Facts seen/frontier, ValidationState memo, unreachable, references/counts/
touched/final rows/release, physical/global/native engine protection and R2–R7
remain open. No second scratch owner, workload recognizer, dependency, retry,
error-selected old walker, WAL/sync expansion or increased cache/worker/deadline.

Latest explicit human direction supersedes autonomous continuation after this
checkpoint: finish this delivery, owning checks, exact staged/committed LOC,
normal push and substantive #287 update, then PAUSE implementation. Review
existing source-pinned speed evidence/setup costs before any subsequent work;
no new campaign or reuse/small-path product implementation in this checkpoint.

## Actual allocated codes and phases

- `StateTable::GraphNodes = 4`, `GraphEdges = 5`. Existing roots1/claims2/sites3
  are unchanged. These are private construction-state tags, not Bridge opcodes
  or canonical profile changes.
- New private SQLite/owner profile version4, LFCS application id1279673171.
  Sites phase1/table3; Graph phase2/tables4+5; Roots phase3/table1. Old private
  profile1/2/3 APIs, SQL, bytes, native bindings and default16MiB remain exact.
- Graph mode Fresh1 follows absent Base; Update2 follows present Base. No
  independently supplied mode can disagree with subject.
- Graph stages: Deferred0, Seeding1, Expanding2, AdjacencySealed3, Solving4,
  Proved5, Retiring6, Retired7, Rejected8. Failed/Unknown attempts retain their
  exact capsule and deny dependent eligibility. Local abandon is metadata-only;
  it does not write an uncertain DB or retry cleanup.
- Update seed enrollment precedes first unexpanded selection. That selection
  closes enrollment and enters Expanding. Fresh root enrollment enters
  Expanding directly. Empty updates still close their exact graph phase. No
  late Seed mutation after expansion begins. Seed count is bounded by declared B.
- Only known Sites retirement admits graph mutation. Only known graph retirement,
  verified empty tables/indexes, COMMIT and native observation admit roots3.

## Selected capacity and configuration

`GraphCapacity::new(S)` validates default/min16,777,216 bytes,4096-byte alignment,
checked R=S/256, L=R*63, P=S/4096 and R<=u32MAX. Formal maximum aligned S is
1,099,511,623,680 bytes; it is representability only, not qualification. C2 checks
platform signed native offset/reservation conversions before StateSelection/
slot/file/SQL/Save effects. Source capability supplied by the caller is already
issued; do not falsely claim this C2 API precedes that caller action.

The live graph uses aggregate N+E<=R and60N+43E<=L, not R per table. Native file
reservation/readback/status/retention/refund use captured S. Physical B-tree,
metadata/index/freelist/MEMORY journal/cache/OS costs stay inside their actual
budgets and require separate proof. S/256 is admission arithmetic, not a fit
formula. Existing D/B/prepared count limits,4GiB file profile, examined-work
allowance,128-record/64KiB windows,512KiB cache, mmap0, workers and deadlines
remain unchanged. Actual total operation temporary disk is not inferred from S.

Root adds explicit `Service::with_construction_scratch(stores,recorder,S)` and
`Server::create_with_construction_scratch(config,S)` /
`open_with_construction_scratch(config,S)`. Existing constructors delegate with
16MiB; no mandatory ServerConfig/StoreAccess field breaks. Native operator
`LAYERFS_CONSTRUCTION_SCRATCH_BYTES` parses before Store/history/worker effects;
only absence selects default. Invalid settings fail without rounding or fallback.
It names the per-operation indexed construction-state owner. Input spool,
ordering runs and live Workspace metadata keep separate limits/gaps. Aggregate
active/retained native reservation sums each immutable owner budget, including
mixed old16MiB/newS owners. No count*current-config refund or adoption.

Native selected class is admitted before body/Save effects. Predictable pure
scope/window/derivation/provable-overflow checks precede query. Bounded selected
reads may establish unknown membership; exact distinct growth then precedes
reservation, pending write attempt and mutating SQL. The EXCLUSIVE zero-query
refusal proof uses keys provably new above acknowledged MAX. Full capacity is
not blanket prospective whole-input fit; larger actual graphs can refuse with
known earlier private custody and cleanup. No work clamp makes them fit.

## Encodings and native association

Freeze exact C1 Subject106/Scope188 layouts in the C1 proposal. Subject is SID8,
namespace32,mode1,basePresent1,baseOID32,rootSerial8,S8,R8,L8. Scope is node
StateScope81/edgeCode1/subject106. Subject recomputes derivation and follows
actual source/base/namespace/root. Preselection mismatch cannot poison another
owner. New `GraphConstructionScopes` creates sites1, graph2 and roots3 from one
bound selection and subject.

Header4 is298 bytes: `LFCSOWN4`, u16BE4, reserved6zero, then the same token/
selector/binding/nonce/parent/directory/file field offsets through192, followed
by subject106. Native binding is BLAKE3 under
`layerfs/construction-state/native/v4\0`, existing nonce32/identities72/selector32/
token8 followed by subject106. Hash subject before bind_owner; never derive the
binding from a scope containing itself. Version4 fixes capacity derivation1.

Node frame60: key25(token8/phase8/table4/serial8), value29(flags1,discovery4,
lowlink4,DFSparent8,afterChild8,incoming4), keylen2/vallen4 framing. Edge frame43:
key33(token8/phase8/table5/parent8/child8), multiplicity4, same framing. Integers
are checked BE. Keys and positive pointers use1..i64MAX; pointer zero is absent.
Ranks/incoming are checkedu32; multiplicity is positiveu32. No truncated identity.

Flags Seed1/Expanded2/RootReached4/OnStack8/Completed16/SelfLoop32/DfsFinished64;
128 is invalid. Update forbids RootReached; Fresh forbids solver fields/flags
and Seed. Completed lowlink is component-root discovery, not active lowlink.
Retain parent/after through checked return. Immutable projection masks flags39
and zeroes solver fields while retaining incoming.

Freeze adjacency285/proof317 seals, node/edge page headers318 and proof-page350,
ordered frame/hash/count/byte/EOF methods and exact domains in the proposals.
Native keeps acknowledged MAX; C1 independently folds complete streams.
Adjacency node pages return immutable NORMALIZED projections. SCC root selection
uses page keys then fresh live `graph_node` for CAS; never a stale projected
record as expected live state. Full proof pages are immutable after Proved.

## Closed mutation and acknowledgement grammar

Use the proposed `EffectiveGraphState` method list and private checked types.
`GraphConstructionState` composes IndexedState, BindingSiteState and graph state
with explicit Rust1.85 delegation; existing site-only APIs remain compatible.
New actual C1 build/update entry points use `GraphConstructionScopes`.

GraphBuildAck contains bounded exact old-option/new node+edge changes, exact
N/E/bytes/multiplicity before/after and distinct deltas, including updated parent.
C1 checks requested children/keys/grouped multiplicity, source scalar totals and
parent/current-count consistency. Append at most63 raw children: parent+children+
edges share <=128 affected records and64KiB. No two128-sized independent caps.

Closed GraphMutation variants:

- EnterRoot(before,after): any White vertex, native current0, exact next rank.
- Descend(parentBefore,parentAfter,childBefore,childAfter,edge): atomic two-target
  transition on actual FIRST edge, White child, exact parent/rank/current update.
- Advance(before,after,edge): FIRST edge, active child's discovery lowers lowlink;
  completed child's component is not an active lowlink.
- Finish(before,after): indexed outgoing EOF then DfsFinished.
- Return(parentBefore,parentAfter,childKey): exact child.parent and parent.after,
  finished child and native current; low exactly min(parent.low,child.low).
- LeaveRoot(finishedRoot): parent0, finished/completed DFS root, exact native
  current/root departure; metadata-only, no arbitrary node mutation.

Logical items and sum of affected distinct targets are both<=128; all old/proposed
frames, projections and selected rows validate before writes. Duplicate CAS
node targets in one batch are rejected. Successive transitions need separate
acknowledgements. Descend avoids an unbound pending White child between calls.

Pop consumes actual maximum OnStack discovery down to the selected SCC root in
bounded windows, returning <=128 completed-member records. C1 independently folds
strict discovery/order/component/count/Seed/SelfLoop. Native solver metadata keeps
cumulative popped count/anySeed/singletonSelfLoop, not a prefix COUNT-derived
remaining estimate. No full component Vec/index or per-seed query walk.

Confirm final semantic disposition: after valid last-window SQL COMMIT and native
observation, a cyclic component containing Seed returns its completed-member
GraphPopAck with `RejectedCycle`; owner is already sticky Rejected/no roots.
C1 checks final members and emits `effective tree cycle` without post-failure
query. Do not invent a Storage provider error for that known semantic verdict.
COMMIT/rollback/observation failure returns its actual typed failure/Unknown and
exact pending old/proposed rows/decision; no KnownRejected acknowledgement then.

Native final proof validates closed transitions, all terminal rows/empty stack
and immutable adjacency rehash. Known cyclic-seed rejection is enforced at SCC
closure; do not add quadratic component queries or an unmeasured full-rank index.
Fixed GraphOwner has16 columns including seed_count<=declaredB; SolverOwner11
tracks cumulative pop proof. Packed BLOB29 and redundant flags/discovery compare
before trust/CAS; both actual partial indexes count physically. Statements remain
<=16 columns/8 variables; conditional changes acknowledge exactly selected rows.

## Algorithm, failure order and owning exits

Update enrolls selected directory children once, expands each union vertex once
with one live EffectiveEntries, closes adjacency, then solves SCC once. Reject
only cyclic SCC containing Seed, preserving old non-seed descendant cycles.
Fresh starts root only, records reachable directory adjacency, expands excluded
parents as zero-outgoing leaves, then streams eligible fresh directories. Global
SCC Done never means root-reached. Earlier alias/root checks retain priority.
Complete adjacency closure before SCC can alter ordering among independent graph/
provider faults relative to old early per-seed rejection; both deny canonical
namespace output and retain exact original selected failure. Disclose this change.

One live source page/cursor,63-child expansion wave, CAS old/proposed windows,
bounded member/proof pages and Unknown capsule can coexist; actual Vec layout/
capacity and lifecycle sum must be reviewed. Memo/alias/native/OS owners remain
separate. Do not count encoded60 as all Rust allocation or claim physical RAM PASS.

Independent root-owned20 graph and9 budget fixtures are pinned before candidate
code; their hashes and method are in the proposals/evidence. Candidate output
cannot supply roots/verdicts. Owning exits include projected/live stale checks,
selective SCC/old-cycle/fresh exceptions, exact mode/budget/EOF/CAS/refusal,
257-chain shared-work counts, independent v1 roots, real widest/index/phase reuse,
configured48MiB/mixed budgets and fresh acknowledged process Unknown barriers.
All are prospective. No1TiB incidental allocation or qualification claim.

Run meaningful covering locked Content/Storage/Server tests/examples/Clippy,
whole-Core fmt and boundary/self-tests from owned root/target with ARMv8 flags.
Keep failed source/output; rerun only corrected/unrun covering bodies. Full Core
final checks remain R7. No CI/preflight/campaign/new family/Family2/#288 issue edit.
Root integrates/checks/stages only owned files, exact parent/staged/committed LOC,
normal pushes and one substantive #287 checkpoint, then pauses as requested.

## File and responsibility ownership

- C1 owner r0_oracles: Content graph codecs/scopes/ports/coordinator/checker,
  validation/update integration and external Content tests. Root-owned independent
  fixtures remain byte-pinned and must not be edited.
- C2 owner r1_catalog: Storage native profile4/graph SQL/session/adapter/identity/
  budget/status/retirement/Unknown and external Storage tests. Reuse sound v1–3
  paths; preserve their bytes and source behavior. No foreign targets/Cargo.
- Root: Server configured constructors/native config/source/body/composition and
  external Server tests; shared freeze/architecture/log/acceptance/LOC/publication.
- r0_resources: independent static/resource/speed-evidence review, docs only;
  no product ownership, measurements or publication.

All workers are not alone. Preserve others' edits; coordinate shared public type
names/getters/variant fields before dependent edits. Root alone runs Cargo. This
freeze authorizes this selected product delivery only, not the next checkpoint.

### Coordinated public solver clarification before dependent edit

C1 may expose `solve_effective_graph(state, adjacency, &mut ValidationGraphWork)`
as the narrow ordinary reusable coordinator called by the real checker. This is
not test-only visibility or an alternate algorithm. The20 independent graph
vectors can exercise this actual solver without conflating isolated graph facts
with earlier common alias/root rejection. Owning new construction callers still
prove that priority and independent canonical-v1 roots. Native provider proofs
remain separate; an external finite contract provider gives no physical claim.

### Coordinated preselection and public solver custody correction

Independent static review demonstrated that input-versus-caller scopes alone
could consume a valid Site owner before native rejected a foreign GraphSubject.
Add pure `graph_select(&self, scope)` to the closed port: compare the exact
captured issued owner/selection and full subject/budget before Site effects,
without SQL, phase/quota/capsule mutation. Foreign/error selection is outside
abandonment; graph mutation permission still requires known Sites retirement.

Public solve_effective_graph calls this selector before its matched error fence.
The same private solver algorithm is called by the common checker under its
existing outer fence; standalone public calls abandon their selected C1 error
once, with no double abandon or post-failure query. These corrections are selected
before their dependent edits. Already reported counter/Descend guards are static
source corrections, not executed test verdicts.

Native configured maximum format/index proof is separate from C1 work admission.
131072nodes+65536arcs may be valid raw native grammar but all Base arcs would cost
131072 examined entries under EffectiveEntries, above the unchanged65536 budget.
Do not label that as successful complete C1 operation. An owning configured case
can separately use32769nodes+32768 Base arcs (work65536,sum65537), preserving
both the actual charging rate and the distinction from raw resource maxima.


## Owning checkpoint resolution

The [coherent delivery](R1D-EFFECTIVE-GRAPH-DELIVERY.md) records actual owning
checks, exact scoped provider outcomes, retained failed attempts/corrections,
final source seal and remaining capability gates. Earlier draft UNRUN wording
describes preparation before those checks; it is not a final execution verdict.
No speed/global/physical/Linux/R1-R7 qualification is promoted.
