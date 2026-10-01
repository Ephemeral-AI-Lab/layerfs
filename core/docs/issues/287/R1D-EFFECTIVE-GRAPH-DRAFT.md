# R1d-effective-graph: C1 contract and algorithm draft

> **Status: Proposed shared design; NOT an implementation freeze or PASS.**
> Inspected source: published `79639cc6f58ae78eeb1fb638996b1d6948767215`.
> This file is the C1 owner's docs-only delivery. Root owns allocation, integration,
> checks and publication. No new product/test/Cargo/benchmark work is authorized
> by this draft. Read with [the native draft](R1D-EFFECTIVE-GRAPH-NATIVE-DRAFT.md).
> Root must resolve the marked review gates and publish the shared freeze before
> either implementation owner starts. Owner/user direction now ends this execution
> after this selected graph checkpoint's owning checks, LOC, push and #287 update:
> then pause implementation and review existing speed receipts/source only, with
> no new benchmark campaign or subsequent implementation milestone.

## Selected complete responsibility

Replace **post-Site-retirement effective cycle/fresh reachability state only**.
The current `validate.rs::check_sites_selected` finalizes and retires Sites before
`check_remaining`; the new graph proof belongs where `validate/cycles.rs` currently
allocates its per-seed pending/seen or fresh declared/edges/seen/pending owners.
Roots are constructed only after the complete check returns. The proposed new
private profile therefore has Sites phase1, Graph phase2, Roots phase3, in the same
owned scratch file. Existing private profiles1/2/3, v1 canonical bytes and public
compatibility APIs retain their exact behavior and explicitly selected routes.

This does not page alias `seen`/`pending`, positive/absence memo, unreachable,
reference/count/touched/final rows or release state. Alias traversal still occurs
while Sites are live. Do not add another scratch owner or sum simultaneous memory
using a sequential max. A v1 inode-table ordered scan does not certify namespace
reachability and cannot replace the alias walk: that walk follows nonrestated old
edges plus active stored-site edges, including edges a final root may remove.

## Configured disk admission, fixed memory and separate limits

Owner/user steering selects a configurable metadata temporary-disk budget `S`
before dependent token/slot/native file/SQL/Save/body effects. Default and minimum
are16,777,216 bytes. S must be4096-byte aligned; reject an invalid value rather than
rounding it. Derive the graph class with checked arithmetic:

```text
R = S / 256                  aggregate distinct graph records
L = R * 63                   complete framed graph bytes
P = S / 4096                 actual SQLite max pages
require R <= u32::MAX
require N + E <= R
require 60*N + 43*E <= L
```

N is distinct directory vertices; E is distinct `(parent,child)` directory arcs.
Every mutable node field counts in60, not just a tiny birth projection. Duplicate
arcs retain checked multiplicity and still charge every source occurrence.

The largest page-aligned S representable by these fields is1TiB−4096. This is a
format bound, **not a supported-provider, progress, memory or performance claim**.
At the default, R=65,536/L=4,128,768/P=4096. At48MiB, R=196,608/L=12,386,304/P=12,288.
These are per-operation graph/metadata allowances, not a global file/Workspace
population or payload-size ceiling. Existing prepared-request65,536 counts,
file4GiB profile and ordering work allowance remain separate and unchanged.

Proposed `GraphCapacity::new(S:u64)->ContentResult<Self>` computes/checks every
derived field; no public constructor accepts arbitrary R/L. Getters are
`scratch_bytes()/records()/encoded_bytes()/max_pages()`. `check_growth(N,E,dN,dE)`
checks arithmetic, full current/proposed sums and distinct increments before
effects. Decode recomputes the derivation rather than trusting serialized caps.
`GraphSubject::new(source,namespace,base,root_serial,capacity)` validates the106-byte
static subject; `GraphScope::new(node_scope,edge_scope,subject)` requires the same
live selection/owner/phase and the proposed implemented codecs. Mode follows Base,
not an independently forgeable caller choice. Native accepts the subject before
issuance and creates the bound scope after exact native association.

RAM windows, native512KiB cache, mmap0, one construction producer, workers,
deadlines, finite operation bounds and MEMORY/OFF/no-WAL/no-sync policy do not grow
with S. Selecting more disk is explicit; no automatic growth or caught-error
fallback exists. Larger configured profiles require actual owning provider/resource
proofs. Existing16MiB receipts keep their original identities and claims.

Admission reserves a selected capacity, not proof that a whole arbitrary v1
operation fits. D/B counts do not certify base V/E/depth. Each growth batch checks
the exact distinct additions and full byte sum before its first effect, under the
same selected class; known earlier private rows remain owned on a later refusal.
No G clamp, dropped edge, alternate old walker or guessed rollback makes it PASS.
The default profile is initially **CAPABILITY-LIMITED** for larger actual graphs.
An update can have at most B seeds plus G newly discovered directory vertices and
G distinct arcs, so B+2G is a conservative envelope rather than a mandatory
worst-case preallocation. Configuring48MiB can represent that arithmetic for the
current default B/G limits; it does not prove physical SQLite fit or healthy progress.

## Proposed identities and exact encodings

Current StateTable assigns roots1/claims2/sites3. Root proposes new private
GraphNode4/GraphEdge5 and profile4; these are **proposals until the shared freeze**,
not allocated wire/canonical opcodes. No Bridge wire change is selected.

`GraphScope` binds the same live issued selection/native owner and actual opaque
BindingSourceId. Its proposed188-byte encoding is:

| Offset | Bytes | Field |
| --- | ---: | --- |
| 0 |81| Node StateScope: selector32/token8/owner binding32/phase8/node table1 |
|81|1| Edge table code, same selection/phase |
|82|8| Actual issued source ID |
|90|32| Namespace InodeScope |
|122|1| Mode: proposed Fresh1/Update2 |
|123|1| Base present0/1 |
|124|32| Selected filesystem-root OID; zero placeholder only when absent |
|156|8| Positive root serial through i64MAX |
|164|8| Selected S |
|172|8| Derived R |
|180|8| Derived L |

Fresh requires absent Base; Update requires present Base. Scope construction checks
the mode/base relation, selection association, codes, positive root and exact
budget derivation. Raw IDs cannot mint the source or select another live owner.
Foreign source/context/budget refuses before poisoning another owner's cached
quota/error. After selection, every known failure/Unknown terminalizes that attempt.

The static native subject is the106-byte suffix starting at source ID, before any
owner-binding self-reference. Proposed header4 is the existing192-byte native
ownership prefix followed by this complete subject, total298. Native binding must
hash the static subject with nonce/checked native identities/selector/token BEFORE
`bind_owner`; GraphScope then includes the resulting binding. Do not hash a
GraphScope containing that binding to derive itself. Native draft/root review owns
the exact magic/domain/version/header and profile publication.

Node key25 is token8/phase8/node-table1/positive serial8. Node value29 is:

```text
flags:u8
discovery:u32BE
lowlink:u32BE
DFSparent:u64BE
afterChild:u64BE
incoming:u32BE
```

Node frame60 is keylen2(25)/key25/vallen4(29)/value29. Edge key33 is
token8/phase8/edge-table1/positive parent8/positive child8. Edge value is positive
checked multiplicity:u32BE. Edge frame43 is keylen2(33)/key33/vallen4(4)/value4.
Full serials and descriptors remain lossless; no names or recursive history enter
these records. Scope/width/prefix/range/reserved arithmetic is checked before copy.

Flags are Seed1/Expanded2/RootReached4/OnStack8/Completed16/SelfLoop32/
DfsFinished64;128 is invalid. Update births have0/Seed; Fresh births have
RootReached and no Seed. Build discovery/lowlink/parent/after are zero. Expanded
is set only after exact effective-entry EOF. SelfLoop is the exact presence of an
arc to this same serial. Incoming sums acknowledged incoming multiplicity.

After adjacency sealing, Seed/Expanded/RootReached/SelfLoop/incoming and all edges
are immutable. Update forbids RootReached. Fresh performs no SCC and keeps solver
fields/flags zero. Update SCC grammar is:

- White: discovery=lowlink=parent=after=0; no solver flags.
- Entered:0<lowlink<=discovery<=N, OnStack, not Completed; parent0 for a DFS
  root or exact positive DFS parent. A root can be any White vertex in the sealed
  seed closure, not only a Seed. after0 is before the first outgoing child.
- DfsFinished: native indexed outgoing EOF was established; it may remain OnStack
  until the SCC root pops it. This is distinct from Completed.
- Completed: Expanded+DfsFinished, not OnStack; discovery retained and lowlink
  holds the component's root discovery ordinal. Retain parent/after for exact
  checked return to the parent; do not erase the relation before it is used.

## Complete graph construction without a frontier population

Update first streams every changed Directory child as Seed into native nodes;
stored kind wins over a supplied value exactly as today. Do not collect a seed Vec.
Then an actual partial unexpanded-key index returns one unexpanded directory.
Resolve its immutable Base content root from the same checked table/context, open
one existing `EffectiveEntries`, and consume it to exact EOF once. Charge every
base page and emitted effective entry at the current rates. Only directory arcs
need disk rows; nondirectory entries still pay their ordinary work/type checks.

Store compact outgoing arcs, increment multiplicity/incoming, and create each
newly discovered directory node once. `(parent,child)` PK ordering gives later
name-free solver continuation. Newly discovered serials may precede a previously
expanded serial; select from the unexpanded index, never restart a prefix scan.
There is no per-seed whole walk and no paged recreation of the old quadratic work.

Fresh starts only the root and propagates RootReached to discovered directories.
An unreachable/excluded parent is closed as an Expanded zero-outgoing leaf;
skip its source adjacency, preserving the old non-expansion exception without
leaving an unexpanded-index row stranded. Keep no Declared population or bit. After the root graph
closes, stream eligible `new_inodes`/typed values: non-root/non-excluded fresh
Directory requires an exact RootReached node; absence is `effective tree cycle`.
Incoming/multiplicity preserves the existing multiple-parent check. If verification
is deferred, check all eligible incoming>1 before any absent-declaration verdict,
so a missing low serial cannot hide a multiple-parent error encountered in the walk.
The earlier common exclusive-site check retains its own precedence.

Adjacency construction retains one live effective cursor until its parent finishes.
It never resumes a Slice header per child (that would refold NB repeatedly), never
uses BindingPoint as EOF, and never spills a live Box cursor. Once adjacency is
closed, DFS uses only child serial after-values and native rows. This avoids the
unresolved direct-spill base-name/changed-cursor continuation altogether.

## Update SCC predicate and closed mutations

For each selected changed Directory edge P→C, the old predicate is nonzero
C→C reachability OR C→P reachability. In the same effective adjacency, the latter
and P→C place them in one cyclic SCC. Thus reject only a cyclic component containing
a Seed: size>1, or singleton SelfLoop. A cyclic component with no Seed is retained
as an old v1 fact and does not independently reject the candidate.

Counterexample: selected C2 descends A3→B4→A3, but neither node returns to C2 or
its selected parent. Old `seen` suppresses the non-seed loop; unconditional active-
edge rejection would change PASS to FAIL. Tarjan classifies A/B without rejecting
C2. Also include selected cycles under parents outside base-root reachability:
the old update checker processes changed-parent rows even when unrelated to root.

C1 owns iterative Tarjan. Native rows hold both DFS continuation and SCC-stack
membership; only current serial/discovery/SCC-pop count/anySeed/self-loop scalars
remain resident. Completed proofs belong to this exact immutable Base/source/
scope/budget, never another operation. There is no serial-only lifetime cache.

Every mutating selected transition carries expected full Node60 and proposed full Node60;
native decodes actual BLOB/projections and compares the exact expected frame before
effects. A generic arbitrary CAS is insufficient. Closed transitions are:

| Transition | Required native validation |
| --- | --- |
| EnterRoot | Any White node; next discovery exact; low=discovery; OnStack and parent0. No pending White-child gap. |
| Descend | Atomic parent+White child update. Actual FIRST outgoing child after old.afterChild equals the selected edge; parent advances and child enters with its exact parent/discovery/low in the same transaction. |
| Advance | Actual FIRST outgoing child equals the selected edge. Actual child must be OnStack or Completed: active child lowers by its discovery, Completed child advances without interpreting its component as an active lowlink. White child requires Descend. |
| Finish DFS frame | Native indexed outgoing EOF, then DfsFinished; retain parent/after. |
| Return child | Child DfsFinished, child.parent==parent, parent.after==child, parent active/unfinished; proposed low is exactly min(old parent low, returned child low). |
| Pop SCC | Root DfsFinished and low==discovery. Pop actual largest OnStack discovery down to that root in bounded windows. Complete members, low=component root ordinal, count/anySeed/self-loop scalars. No scan of old components or arbitrary pop subset. |
| LeaveRoot | Metadata-only selected solver departure after the exact finished DFS root; no row replacement or child adoption. |
| Finish update | Every node Completed/DfsFinished, empty stack, valid component/rank grammar; no cyclic component containing Seed. Revalidate immutable adjacency before proof seal. |

The native edge check is an indexed query in the selected transaction, not guessed
adoption or a new receipt/issuer. Same-batch duplicate CAS targets are rejected;
successive transitions to one node occur through separate acknowledged batches.
An unchanged legal transition is acknowledged explicitly.

Complete adjacency closure before SCC can reorder independent graph/provider
faults relative to the old first-seed early cycle. Preserve earlier alias/root
semantic priority and every original error/Unknown; report this intentional query
order boundary, with no canonical output, rollback-on-guess or synthetic success.

## Proposed concrete C1 port

The names below are proposed, not existing product exports. Types have private
fields and checked scope/stage constructors. `GraphMutation` is a closed enum of
the transitions above, with exact expected/proposed records; it is not a KV put.

```rust
enum GraphMutation {
    EnterRoot { before: GraphNode, after: GraphNode },
    Descend { parent_before: GraphNode, parent_after: GraphNode,
        child_before: GraphNode, child_after: GraphNode, edge: GraphEdge },
    Advance { before: GraphNode, after: GraphNode, edge: GraphEdge },
    Finish { before: GraphNode, after: GraphNode },
    Return { parent_before: GraphNode, parent_after: GraphNode,
        child: GraphNodeKey },
    LeaveRoot { finished_root: GraphNode },
}

trait EffectiveGraphState {
    fn graph_capacity(&self, scope: &GraphScope) -> ContentResult<GraphCapacity>;
    fn graph_seed_batch(&mut self, scope: &GraphScope, seeds: &[GraphNodeKey])
        -> ContentResult<GraphBuildAck>;
    fn graph_root(&mut self, scope: &GraphScope) -> ContentResult<GraphBuildAck>;
    fn graph_unexpanded(&mut self, scope: &GraphScope)
        -> ContentResult<Option<GraphNode>>;
    fn graph_append(&mut self, scope: &GraphScope, parent: &GraphNode,
        children: &[GraphNodeKey]) -> ContentResult<GraphBuildAck>;
    fn graph_expanded(&mut self, scope: &GraphScope, parent: &GraphNode)
        -> ContentResult<GraphNode>;
    fn graph_seal(&mut self, scope: &GraphScope)
        -> ContentResult<GraphAdjacencySeal>;
    fn graph_node(&mut self, seal: &GraphAdjacencySeal, key: GraphNodeKey)
        -> ContentResult<Option<GraphNode>>;
    fn graph_node_page(&mut self, seal: &GraphAdjacencySeal,
        after: Option<GraphNodeKey>, limit: GraphPageLimit)
        -> ContentResult<GraphNodePage>;
    fn graph_edge_page(&mut self, seal: &GraphAdjacencySeal, parent: u64,
        after: Option<u64>, limit: GraphPageLimit) -> ContentResult<GraphEdgePage>;
    fn graph_begin_scc(&mut self, seal: &GraphAdjacencySeal) -> ContentResult<()>;
    fn graph_cas(&mut self, seal: &GraphAdjacencySeal,
        mutations: &[GraphMutation]) -> ContentResult<GraphMutationAck>;
    fn graph_pop(&mut self, seal: &GraphAdjacencySeal, root: &GraphNode,
        limit: GraphMutationLimit) -> ContentResult<GraphPopAck>;
    fn graph_finish(&mut self, seal: &GraphAdjacencySeal)
        -> ContentResult<GraphProofSeal>;
    fn graph_proof_page(&mut self, seal: &GraphProofSeal,
        after: Option<GraphNodeKey>, limit: GraphPageLimit)
        -> ContentResult<GraphProofPage>;
    fn graph_retire(&mut self, seal: &GraphProofSeal) -> ContentResult<()>;
    fn graph_abandon(&mut self, scope: &GraphScope) -> ContentResult<()>;
}
```

Fresh `graph_finish` is called only after exact declaration verification. It never
uses SCC Completed as reachability. Update finish requires completed SCC proof.
GraphBuildAck contains exact bounded changes `old:Option<GraphNode>/new:GraphNode`
and `old:Option<GraphEdge>/new:GraphEdge`, aggregate before/after N/E/bytes,
distinct deltas and the updated parent. C1 validates every expected source child,
new/old identity, multiplicity, incoming/count transition and parent change.63 raw
children require at most127 affected targets; no operation-sized result Vec.

GraphPopAck returns at most128 completed member records for the exact root/window,
plus window and cumulative counters/status. C1 independently folds strict descending
discovery, full component assignment, count/Seed/self-loop and exact stop/root;
it does not trust returned booleans alone. Native tracks cumulative popped/anySeed/
singletonSelfLoop in fixed selected metadata and rejects cyclic Seed components at
closure. No whole component Vec, extra full discovery/component index or quadratic
component query is introduced. Final proof checks every closed transition/all
Completed records and the empty stack against that acknowledged closure state.

Append windows derive from the128 affected-record bound: at most63 raw children
can require one parent plus63 child nodes plus63 edges. Node+edge proposals share
the same window; not128 of each. Existing keys/duplicate children still have checked
ordered multiplicity semantics; their whole expected/proposed group is validated
before effects. Mutation batches have at most128 logical items AND affected targets;
Descend counts two targets and duplicate targets are rejected. Body framing is
header-inclusive<=64KiB; actual capacities and old/proposed owners are separate.

Common C1 `check_with_sites` will take a new explicitly selected construction port
that includes graph state; old v3 entry points remain compatibility. The new owning
checker calls graph after known Site retirement and before roots. Exact preselection,
once-only abandon, original typed Storage failure, Save cleanup and Unknown custody
rules carry over. No failed graph call falls back to a resident walker.

## Seals, ordered cursors and retirement

Proposed GraphAdjacencySeal285 is version1/scope188/N8/E8/nodeBytes8/edgeBytes8/
nodeProjectionDigest32/edgeDigest32. Terminal bytes are60N/43E. Node adjacency
projection retains Seed/Expanded/RootReached/SelfLoop/incoming and normalizes solver
flags/discovery/lowlink/parent/after to zero. Proposed literal domains are
`layerfs/effective-graph/adjacency-nodes/v1\0`,
`layerfs/effective-graph/adjacency-edges/v1\0`, and
`layerfs/effective-graph/proof-nodes/v1\0`; each hashes domain+scope+ordered full
projected/current Node60 or Edge43 records+count8+recordBytes8. Native retains this exact
acknowledged seal and rehashes the immutable projection in the final transaction.

Proposed GraphProofSeal317 adds adjacencyDigest32 to version1/scope188/totals32/
fullNodeDigest32/edgeDigest32. adjacencyDigest is BLAKE3 of the exact285-byte
adjacency seal under `layerfs/effective-graph/adjacency-seal/v1\0`. A runtime provider snapshot
seal is not an independent expected semantic oracle; independent literal record/
seal vectors and root-owned graph vectors supply owning tests.

Node page header318 is adjacencySeal285/lastPresent1/lastNodeKey25/count2/bytes4/
EOF1. Proof node header350 uses proofSeal317 with the same suffix. Selected parent
edge header318 is adjacencySeal285/parent8/lastPresent1/lastChild8/maxPresent1/
maxChild8/count2/bytes4/EOF1. Parent and child are positive; None has zero placeholder,
not a real serial. Limits are<=128 records and64KiB including headers; actual Vec
capacity<=limit. Empty selected EOF is legal, empty non-EOF is not. Strict after-key
order, selected seal/context, count/byte arithmetic and sticky failure are checked.
Parent edge EOF comes from its acknowledged MAX, not OFFSET/rank/prefix COUNT.

C1 independently rehashes complete ordered pages and requires exact terminal EOF.
`graph_node_page(AdjacencySeal)` returns only the normalized immutable adjacency
projection, including during SCC. A solver root chooser consumes its keys, then
calls `graph_node()` for the actual current mutable record before expected/proposed
CAS. A projected page is never a live solver snapshot. `graph_proof_page()` returns
the completed full records under the proof seal and independently checks them.
Native checks immutable adjacency and full current rows, then acknowledges a proof
seal. Retirement deletes exact sealed nodes/edges in<=128-row windows, checks both
tables/partial indexes empty, COMMITs and observes native allocation before roots
become eligible. No root permission after uncertain or failed observation.

Known failure retains the earlier private rows and one explicit cleanup attempt.
Unknown retains exact context/stage, old/proposed full rows/counters, selected edge,
SCC boundary and expected/proposed seals/retirement position. No query adoption,
resend, refund or destructive cleanup. Abandon is metadata-only, sticky and selected;
Drop performs no implicit mutation or retry.

## Work, overlap and prospective independent exits

Build every distinct directory once and consume its effective adjacency once.
Update SCC enters each vertex once, examines each distinct arc once, and pushes/
pops each vertex once. Logical work is O(K+V+raw effective entries+E); indexed
storage costs O((V+E)Hs) and real canonical page/lookup paths are reported separately.
Per-step FIRST-edge validation adds indexed work, not a repeated subtree proof.
Fresh root discovery and declaration checking are separate bounded named passes.
Preserve existing base-page and changed-entry charging rates and the operation's
work limit; removed repeated walks do not authorize increasing that limit.

One current EffectiveEntries owns a64-entry/8192-byte base window, continuation,
base/changed peeks and one live binding cursor. Expansion proposals, old/proposed
CAS frames, provider/page Vec capacity, retained attempt capsules, hashers, memo,
SQLite cache/MEMORY journal, native and OS pages coexist.128 is a logical window,
not total allocated owners: old128+proposed128 can already coexist. Actual layout/
capacity/overlap observations must be recorded; no heap-only/global memory PASS.

Root-owned independent expected vectors use <=16-vertex nonzero Floyd-Warshall
and fresh root walks/incoming counts, with no candidate imports.20 graph cases:
`vectors.json` SHA256 `906e5b162890c8f8c903bd82230475a79b84bd6cf7f8b4e2d1c55cfc0f598676`;
`manifest.rs` SHA256 `ad9e98f157907b9fd26bde34afd2f2aeafb7ebae1cc9a58ff9cefb33aa988cb2`.
Nine independent budget cases: JSON SHA256
`f6c8e9fa7b7f66fd3d34a84a080884c16e7d472e258105165c0cce0746c47021`;
Rust SHA256 `75440d446d22eb842e7d0f7c6ce765904985dc3275542e407ef8ca3e61dff3d9`.
They are prospective expectations, not candidate/provider PASS, and are owned by
root under `tests/fixtures/effective_graph`; this draft changes none.

Required owning exits include literal Node60/Edge43/context/seal vectors; foreign
scope/budget/nonmutation; complete source/EOF/grammar/CAS refusals; old non-seed
descendant cycles through an independently encoded canonical base; selected and
disconnected cycles; repeated arcs; fresh exclusions/declarations; preserved earlier
alias priority; unchanged sealed v1 roots; and exact Unknown/cleanup at every native
phase. Root/provider gates separately prove configured disk/profile identity,
incremental refusal before effects and all index/retirement/native allocation costs.

Count-driven depth case: restate every edge of a257-directory chain. Expand each
directory/arc once; overlapping seeds reuse SCC proof. Expected canonical root is
the independently pinned unchanged Base when bindings/attributes are unchanged.
Report graph expansions, source entry rates, indexed CAS/edge/pop counts and depth;
no per-seed quadratic replay. Default65k budgets stay unchanged. A formal enlarged
budget or a passing count case is not #288 speed/release qualification.

## Shared review gates before implementation

Root must freeze actual tags/profile/mode/stage codes, header/binding/domain bytes,
config plumbing, all proposed port/type names and selected page/seal layouts.
Native/C1 must agree exact GraphBuildAck duplicate-target handling and independent
adjacency/proof verification boundaries; check every retained failure/Unknown path.
They must enumerate fixed owner metadata and native projected indexes/conditional
mutations within current engine limits. Parameterized capacity does not prove a
configured maximum physically fits; larger-provider rows stay unqualified until
their actual proof. Alias/memo/global containment/strict memory and R2–R7/#288 remain
open. This document stops at design review and authorizes no product implementation.

## Coordinated implementation interface clarification

Root approved the focused public C1 coordinator
`solve_effective_graph(state, adjacency, &mut ValidationGraphWork)` before its
export. The actual owning graph checker calls this same function after complete
adjacency construction/verification. It is a reusable selective-SCC capability,
not a test-only route or a second algorithm. Independent isolated graph vectors
exercise that coordinator; owning construction tests separately preserve earlier
alias/root verdict priority and sealed v1 roots. Finite external providers make no
physical/native allocation claim. C2 closed mutation/seal interfaces are unchanged.

Root's static ownership review clarified two preselection/terminal fences before
corresponding implementation changes. `graph_capacity(scope)` is a pure cached
full-native-scope/subject/budget getter available while Sites is still open; foreign
scope/getter refusals occur outside abandonment and consume no valid Site attempt.
C1 calls it before source/base/resource/Site effects. The standalone public solver
uses the same pure preselection, then metadata-abandons any selected inner error
once. The owning common checker calls that exact private solver body under its own
once-only terminal fence, preventing duplicate abandonment. No format/tag changes,
SQL/query adoption, retry, or capability fallback follows these corrections.

The final coordinated preselection shape supersedes the preceding getter wording:
use a distinct `graph_select(&self, scope) -> ContentResult<()>` pure cached exact
owner/context/budget selector. `graph_capacity` keeps its actual phase admission.
C1 source/checker/construction wrappers and standalone solver call `graph_select`
outside selected-error abandonment. The selector performs no SQL, phase, quota,
error-capsule mutation or adoption. Root approved this final API and C2 was notified
before dependent edits; encoded profiles and all capacity arithmetic are unchanged.

Before dependent constructors, C1/C2/root agreed a checked fixed
`GraphPopTotals(popped, any_seed, singleton_self_loop)` value grouped into
`GraphPopAck::new(scope, root, component, members, totals, disposition)`; existing
Ack getters and full member verification are unchanged. This groups related fixed
metadata without changing native fields, framing, digests or stages. Private C1
`GraphInput` holds only the four immutable reader/source/topology/exclusion views;
it owns no population, registry or solver state.

Static byte-admission review found the initial conservative43-byte item allowance
undercounted LeaveRoot's full Node60 argument when it had zero affected targets.
Before correction, the coordinated input-window rule now sums variant record/key
arguments explicitly under header317: EnterRoot/Finish120, Descend283,
Advance163, Return145, LeaveRoot60. Thus one LeaveRoot requires377 encoded window
bytes;360/376 refuse before a provider call. This is encoded argument admission,
not Rust enum layout/native selected-query/capsule or physical-memory accounting.
No encoded role/tag/native schema or default64KiB profile changes.


## Owning checkpoint resolution

The [coherent delivery](R1D-EFFECTIVE-GRAPH-DELIVERY.md) records actual owning
checks, exact scoped provider outcomes, retained failed attempts/corrections,
final source seal and remaining capability gates. Earlier draft UNRUN wording
describes preparation before those checks; it is not a final execution verdict.
No speed/global/physical/Linux/R1-R7 qualification is promoted.
