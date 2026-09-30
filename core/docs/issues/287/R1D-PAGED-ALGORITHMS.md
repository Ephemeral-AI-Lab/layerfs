# R1d paged C1 algorithms and accumulated-work laws

> **Status: Dated design checkpoint; no implementation or qualification claim.**
> Read-only source preparation, 2026-09-30. Audited published parent:
> `765202c45e11b3c96b2b16c40b35e810a7270d34` (R1a).
> R1b-cache source and real-provider proofs belong to their current workers.
> Source observations below use the committed parent, excluding those edits.
> This note changes no product, counter or dependency, and runs no build, Cargo
> command, product test or benchmark. The current next action remains
> **R1b-cache** in the owner-directed preparation assignment. HEAD advanced to
> `06fe8363d5c317c49876d5189d374c1a34cc6010` after source reading; that change is
> not promoted to the audited source here. Follow the owning
> [checklist](CHECKLIST.md) for the current execution action. These are dependent
> proposals after the R1c DirectoryRoots port.

The [R0 freeze](R0-FROZEN-INTERFACES.md),
[R1c preparation](R1C-STATE-DESIGN.md),
[SERVER packet](../../architecture/proposal/bounded-workspace-implementation-20260930/SERVER.md)
and [scenario catalog](../../../../scenarios.md) own the obligations. Root
retains interfaces, Bridge coordination, resource admission and acceptance.
SC-03/05/06/07/08 are affected. C1's construction operation is one producer;
these changes do not enable concurrent Workspaces, v2 namespace certification or
an unqualified physical Server profile.

## Source populations and existing work worth retaining

All line references in this section name the audited parent. Cache-related
working edits may move lines; the implementation checkpoint must re-pin them.

| Authority or work | Concrete source replacement |
| --- | --- |
| Draft bodies, parent counts, detached membership and committed identities | [file/edit/tree.rs](../../../crates/layerfs-content/src/file/edit/tree.rs#L109), lines 109–122; `drafts`, `parent_refs`, `detached`, `committed` |
| Full detached candidate collection | Same file, lines 263–283, `settle`; collection repeats by depth, while each retired draft is removed once. Preserve its avoidance of the older repeated-prefix prune. |
| Release error suppression and unchecked parent count | Same file, lines 239–250 and 287–309: `+= 1`, `.ok()`/`unwrap_or_default()` |
| Final publication membership and draft-to-canonical map | Same file, lines 386–482, `finish`/`commit_node`; separate `published` set and `committed` map |
| Wide mapping frontier | [mapping/read.rs](../../../crates/layerfs-content/src/file/mapping/read.rs#L458), lines 458–544: complete `level`/`next` vectors despite 32-page provider waves |
| Whole directory row at each cursor/lookup | [rows/source.rs](../../../crates/layerfs-content/src/filesystem/rows/source.rs#L24), lines 24–26 and 61–62: `DirectoryUpdate` contains its complete changes |
| New-parent membership/counts and fresh final rows | [filesystem/update.rs](../../../crates/layerfs-content/src/filesystem/update.rs#L176), lines 176–183, 429–468 and 535–582: `initial_counts`, final `rows`, unreachable-parent maps |
| Validation additions and alias populations | [validate.rs](../../../crates/layerfs-content/src/filesystem/validate.rs#L165), lines 165–173, 310–345 and 382–423: additions, parent names, candidates, changed names, bound sites, pending/seen |
| Repeated effective subtree walks and full directory materialization | [validate/cycles.rs](../../../crates/layerfs-content/src/filesystem/validate/cycles.rs#L44), lines 44–117 and 214–273: a new walk per rebound directory, `base`/`merged` vectors |
| Build reachability population | Same file, lines 144–197: stated edges, declared directories, indegrees, seen and pending |
| Validation memo sized by operation population budget | [validate.rs](../../../crates/layerfs-content/src/filesystem/validate.rs#L566), lines 566–627: record/absence maps and prefetch collection; use bounded real windows plus exact paged facts |
| Declared-new membership and complete touched states | [references/reduce.rs](../../../crates/layerfs-content/src/filesystem/references/reduce.rs#L54), lines 54–102 and 177–207 |
| Backward-demand prefix scan | [references/runs.rs](../../../crates/layerfs-content/src/filesystem/references/runs.rs#L340), lines 340–395; line 365 restarts at offset zero |
| Complete zero seeds, release queue/stack and retained base facts | [update.rs](../../../crates/layerfs-content/src/filesystem/update.rs#L643), lines 643–717, and [references/release.rs](../../../crates/layerfs-content/src/filesystem/references/release.rs#L51), lines 51–187 |

The current final-row consumer is already streamed:
[reduce.rs](../../../crates/layerfs-content/src/filesystem/references/reduce.rs#L323),
lines 323–421. Reuse it and its bounded authenticated base batches. Likewise,
RowSpool already has ordered, binary-searchable 32-byte slots
([spool.rs](../../../crates/layerfs-content/src/filesystem/rows/spool.rs#L37),
lines 37–59 and 394–421). Reuse its scalar value/new-serial lookups, replacing
the wide directory result with a cursor. Do not add a second membership set.
The Phase B alias walk already follows changed bindings once
([validate.rs](../../../crates/layerfs-content/src/filesystem/validate.rs#L485),
lines 485–505); preserve that fix.

## Shared key decision before the first producer

The R1c note proposed a full 41-byte selector32/phase8/table1 key prefix.
DirectoryRoots fits that proposal, but a maximum NameBinding does not:
`41 + parent8 + name-length2 + name255 = 306`, exceeding the frozen 288-byte
key limit. This is a source-independent arithmetic gate, not a reason to reduce
the supported 255-byte component or hash/truncate names.

Root's 2026-09-30 design decision selects a **compact issued session token8**,
bound to the complete operation selector32 in the exact session owner/header.
The shared logical key prefix becomes `token8 + phase8 + table1 = 17` bytes.
Thus NameBinding is at most 282 bytes, and the initial DirectoryRoots encoded
record becomes `17 + serial8 + framing6 + root32 = 63` bytes. The old 87-byte
R1c arithmetic remains a historical proposal; the owning freeze and initial
producer must transparently adopt the same compact convention.

The implementation gate is exact issuance and association: nonzero checked
token, issuer/incarnation, full selector, one private session, native directory
and open-file device/inode identity, and selected phase/table. No truncated
digest, guessed token, open-time adoption or cross-session lookup is allowed.
The full selector and issuer/native identity belong in the seal transcript and
their fixed header/capsule bytes remain charged. Factoring them out of each key
does not discard their authority. Root owns the exact issuer implementation.
Table names below are typed responsibilities, **not allocated numeric tags**;
allocate closed private discriminants with their actual codec/caller.

## Concrete record classes

Widths below use that 17-byte prefix and six bytes of record framing. They are
encoded widths; real Vec capacity, decoded fields, lookup heads and SQLite row
ownership have separate simultaneous charges. A page/flush respects both
128 records and 65,536 encoded bytes.

| Typed table / local key | Exact proposed value / maximum encoded record |
| --- | --- |
| DirectoryRoots / serial8 | root32 / 63 |
| BindingChange / parent8 + name-length2 + component1..255 | child8, disposition1, flags1, reserved6 =16 / 304 |
| DeclaredSerial / serial8 | ordinal8, kind1, flags1, reserved6 =16 / 47; use the sealed prepared scalar source instead when it already owns this fact |
| BaseFact / serial8 | present1 plus existing 73-byte InodeValue (all zero when absent) =74 / 105 |
| NonFileParent / child8 | parent8, name-length2, component1..255 =11..265 / 296; single-site fact after exact alias validation |
| ParentEffect / child8 + sequence8 | parent8, name-length2, component, disposition1, flags1, reserved6 =19..273 / 312; bounded sorted group reduction, no wide reverse-name key |
| ReferenceState / serial8 | existing complete 96-byte Count/Effect Row / 127; serial duplication is checked |
| VisitFact or OnceFact / serial8 | state1, flags1, reserved6, owner/sequence8 =16 / 47; distinct typed namespaces and phase selections |
| ReleaseJob / sequence8 | serial8, kind1, flags1, reserved6, selected effective directory root32 =48 / 79 |
| ReleaseCursor / sequence8 | root32, name-length2, last component0..255, finished1, reserved6 =41..296 / 327 |
| DraftDecoded / draft32 | header64 plus at most 128 leaf40 or branch48 cells; maximum 6,208 / 6,263 |
| DraftPageMeta / draft32 | header64 / 119 |
| DraftCanonical / draft32 | checked canonical bytes at most 8,192 / 8,247 |
| DraftReference / draft32 + ordinal2 | direct referenced id32 / 89 |
| DraftPredecessor / draft32 + ordinal1 | id32 + checked provenance1 =33 / 89; at most four |
| DraftParentCount / draft32 | checked count8 / 63 |
| DetachedJob / sequence8 | draft32 + queued owner/sequence8 =40 / 71 |
| DraftCommitted / draft32 | canonical id32 / 87 |
| CanonicalEmission / canonical32 | disposition1, flags1, reserved6 =8 / 63 |

The draft header64 is: form1, role1, level1, flags1, cell-count2,
reference-count2, logical-bytes8, extents8, canonical-id32,
predecessor-count1 and reserved7. Decoded drafts have a zero canonical id until
final encoding; already-finalized pages bind their actual id. Form/role/flags
and provenance are closed checked codecs, with actual discriminants frozen by
their owner before use. Current synthetic draft identity generation
([tree.rs](../../../crates/layerfs-content/src/file/edit/tree.rs#L342),
lines 342–350) remains a private checked identity, not a claimed stored digest.

At maximum width, 128 BindingChange rows occupy 38,912 bytes; ReferenceState
rows occupy 16,256; ReleaseCursor rows occupy 41,856. Ten maximum DraftDecoded
rows occupy 62,630 bytes, while seven maximum DraftCanonical rows occupy
57,729. A byte cap therefore wins well before 128 for draft pages. A finalized
page's canonical bytes and direct references can total 8,192 + 128*32 bytes;
they cannot be packed into one 8-KiB value. Preserve them separately, along with
the advisory predecessors carried by
[FinalizedObject](../../../crates/layerfs-content/src/object/output.rs#L88),
lines 88–94 and 153–167. No payload enters these tables.

## Named dependent delivery boundaries

These are proposals for root to select, each with a real production caller and
checkpoint. A delivered slice checks its own exit gates and leaves complete
R1d composition unqualified until its populations are retired. No unused table,
interface-only commit or arbitrary iteration number constitutes delivery.

### R1d-run-seek: exact ordering lookup without prefix restart

This is independently eligible after the coherent R1c port; it directly serves
ReferenceReducer::entry and release::state. The comment asserting ascending
demands in [runs.rs](../../../crates/layerfs-content/src/filesystem/references/runs.rs#L555)
does not establish that predicate. Directory bindings are name ordered, not child
serial ordered; release and subsequent edits can demand descending/repeated
serials. Merely putting the same scan in SQLite preserves its quadratic law.

Reuse the actual fixed-stride 96-byte Row, immutable run handle and
`OrderingRun::read_at(offset, &mut buffer)`
([backing.rs](../../../crates/layerfs-content/src/filesystem/references/backing.rs#L28),
lines 28–45). Add an exact point reader using a checked binary search over
`[0, run.count)`, reading one 96-byte row at checked `index*96`. Run length must
equal count*96; first/last, strict ordering, row grammar and immutable owner are
bound when the run is finalized. A short read, identity/length mismatch or
provider failure is terminal; do not catch it and start a sequential scan.
Search tiers in the existing newest-wins order. A hit in the pending map still
wins. This changes no canonical grammar or inode count rule.

Keep an explicit monotone scan for genuinely ordered consumption, with its
last-key precondition checked. Arbitrary `entry/state` uses exact point lookup
from its first demand; it does not implicitly assume order or restart a prefix.
Two-way geometric spill/merge is already within frozen fan-in eight. Before an
ordered all-state pass, consolidate/adopt the single run as the existing code
does; do not open the up-to-32 tier readers from `visit_newest_first` together
to implement a new eight-way profile. Input/output overlap stays charged until
their actual owned handles are released.

Exit proof: an external public reducer test uses declared real file backing,
forces multiple spills, and applies name-order serial permutations, descending
queries, gaps and repeated edits. A small independent map/event interpreter
computes final Count/Effect rows and root partitions. Count actual `read_at`
calls/bytes through an ordinary external backing implementation over real files;
for Q point demands across T live runs, decoded probes are at most
`Q * sum_t(ceil(log2(n_t+1))+1)`, never Q times the complete run prefix.
Separately prove one-pass ascending consumption and cleanup. This is a bounded
cause diagnostic, with no speed verdict or benchmark receipt.

### R1d-mapping-cursor: a bounded ordinary read frontier

Replace `traverse`'s width-sized level vectors with one structural depth-first
cursor. The actual mapping grammar bounds branch level at31 and entries at128
([types.rs](../../../crates/layerfs-content/src/file/mapping/types.rs#L12),
lines 12–23). Admit at most32 path frames; each holds one branch's at-most-128
child descriptors, next-child position and absolute origin. Advance each frame
monotonically. The logical descriptor storage upper bound is
`32*128*48 = 196,608` bytes before frame/Vec overhead; charge actual type sizes
and capacities, not this encoded arithmetic as a heap proof.

Collect the next at-most32 logical leaf demands across frame boundaries, using
point branch acquisition only as needed to discover them, then use R1b's
independently owned grouped page wave. A single root leaf retains root context;
other nodes use non-root context. Keep exact level, coverage and absolute
origin arithmetic. Retain the existing grouped32 payload acquisition and
logical sink order. Only the current canonical wave, path descriptors and
bounded cache are resident. No queue contains every node of a wide level.

An ascending RangeCursor may retain its selected file's bounded path/leaf
position; that owner closes before a cursor for another selected FileState starts.
There is no lifetime per-file cursor map. Cache eviction cannot invalidate the
current wave or the descriptor path, and never proves a page will survive every
later range. No SQL/scratch allocation is introduced for ordinary independent
read callers merely to replace this frontier.

The navigation acquisition law changes: a full walk's branch acquisitions are
bounded by visited branch nodes B; grouped leaf calls by `ceil(L/32)` when leaves
are collected across frame boundaries. Record B/L and actual calls against the
old level-wave law; do not claim equal speed or credit an earlier cache. If the
count cost blocks a required mechanism gate, design a separately admitted paged
FIFO with monotone append/read cursors that preserves level waves, before using
it. An error cannot select that alternative at runtime. Its ordinary reader
ownership/caller would need its own complete integration.

Exit proof: independent canonical v1 trees at levels0/1/2 and a wide level,
full reads, boundary-spanning sparse ranges, zeros and repeated payload ids;
assert exact expected bytes and partitions. Real-provider grouped-demand counts
and externally observed allocation overlap must prove the declared cursor/wave
bound, including a cache below32 entries. Resource and source/error checks are
separate from expected bytes; performance qualification remains #288.

### R1d-file-drafts: paged final-state split/join and publication

After R1c's state port and the cache/cursor contracts, replace EditObjects' four
maps plus final publication membership with the draft record classes above.
The genuine production integration is
[save/content.rs](../../../crates/layerfs-server/src/service/save/content.rs#L99),
line 99, which calls C1 `apply_edits`; pass its admitted typed state into that
common path and `replace_chunked`'s EditObjects construction
([edit/apply.rs](../../../crates/layerfs-content/src/file/edit/apply.rs#L273),
line 273). Keep the retained independent public entry point as a delegating
prospectively admitted compatibility adapter, not an unused parallel algorithm.
Keep split/join's existing boundary algorithms
([tree.rs](../../../crates/layerfs-content/src/file/edit/tree.rs#L573),
lines 573–656 and 721 onward). Untouched stored summaries stay immutable and
shared. Do not enumerate their leaves, rebuild the entire mapping, or encode/
hash a decoded draft after every mutation just to store it on disk.
Serialize a private decoded node; canonical encoding/hash happens once when
the final selected tree reaches that node. Already-finalized pages retain their
canonical bytes, direct references and predecessor order unchanged.
Preserve the existing `EDIT_DEFERRED_LIMIT = 8 MiB - 1` admitted resident
compatibility bound; external scratch selection does not silently enlarge it,
the body/count profile or the simultaneous Save pool. Any separately supported
external shape requires its own declared arithmetic and real-provider exit proof.

Acquire one explicit draft root reference for the new selected result and
release the prior root reference after the new ownership is known. Parent
retention uses checked counts, including root ownership. A zero transition
queues one exact DetachedJob; a queued-membership fact prevents duplicate
pending entries. Drain with monotonically increasing sequence keys rather than
collecting the whole detached set. A revived queued draft is skipped by its
current checked count; its stale queue entry is consumed once. A later zero
transition obtains a new sequence. Each removed draft enumerates its bounded
outgoing references once and may retire children. No retained-tree prune or
recursive WRITE-history scan is introduced.

Partly created multi-record drafts are a separate owned creating phase; they
are invisible to selected-root lookups until body/reference/predecessor totals
and all acknowledged parent changes are sealed. One bounded SQL batch may not
contain header plus128 other records. Decompose creation into admitted bounded
batches and a fixed final ready transition; do not enlarge128 or make an
operation-sized transaction. A known failure aborts that owned phase once;
Unknown quarantines its exact session/credits. Release/decode errors remain
errors instead of `unwrap_or_default` child loss.

Finish uses an admitted at-most32 mapping postorder stack, bounded node/body
slots, DraftCommitted point facts and CanonicalEmission point facts. Persist
an in-flight emission fact before handing a finalized object to the existing
consumer. Known accepted becomes accepted; failure or lost acknowledgement
does not permit another accept/send. A metadata failure after accepted C2
output retains that accepted candidate and aborts final success. Exact C2 error
custody remains at the owning adapter, following R1c. Chunks continue directly
to the owning consumer; scratch receives no payload.

Exit proof: independent v1 byte/root/partition vectors for overlapping edits,
interior split, equal/different-height join, reused ranges and final shortening;
verify accepted object/reference/predecessor facts through the actual consumer.
Count private rows changed, decoded bodies, canonical encodes, outgoing edges
retired and accepted emissions. Work must follow touched boundaries plus
actually created/retired nodes, not total retained mapping per edit. Real C2
provider/custody/cleanup proofs cover final acceptance and the selected physical
profile. This slice replaces C1 operation state; R2 separately removes live
Workspace recursive authority/history.

### R1d-binding-cursors: sealed input and ordered directory effects

After R1c, add a real header/binding cursor to the prepared namespace source
and its Server parser/sink, coordinated by root's Bridge owner. Keep existing
25/12/10/73-byte wire fields and exact section/EOF checks. A header binds
parent, binding count, encoded bytes and selected input; names/children are
returned in strictly increasing component order under both page bounds. The
existing slice source delegates through the same cursor in its explicit caller-
owned compatibility profile. `directory_for` must return that selection/header
and a bounded binding cursor, rather than materializing the complete directory.

BindingChange is written once in input order and sealed with counts/bytes/
digest before semantic construction. Directory output merges one bounded base
`list_after` page with that monotone change cursor. Empty changes keep existing
bindings exactly; a genuinely new empty directory has none. Preserve same-name
replacements and the existing orphan/new-parent policy.

Replace `unreachable_parents` with paged new-parent facts and two monotone passes:
record only non-root declared-new parents, then mark exact retained bindings.
Seal the unbound subset; both subsequent production and value membership join
it without a resident BTreeMap. DirectoryRoots remains the R1c table. Fresh
build counts/final values become streamed serial joins; no `vec![0; new_rows]`
or final `Vec` enters `apply_inode_values`.

Exit proof: ordinary StageChanges/fresh build through the actual parser and
bounded external state, including a directory exceeding one page, maximum-length
components, count/byte/EOF refusal, permutation and alias-preserving moves.
Independent v1 expected roots/partitions and exact parser/source counts pass.
Do not label complete namespace authority bounded while the dependent graph/
reference/release maps still exist.

### R1d-reference-state: streamed complete counts and zero seeds

After binding cursors and exact run seek, use ReferenceState as mutable exact
point authority during effects. Reuse the current 96-byte row semantics, including
new Count versus existing signed Effect and supplied 73-byte values. Declared-
new membership comes from the sealed scalar source or implemented DeclaredSerial
table, never a second complete BTreeSet. Read base facts in existing grouped
batches; phase identity binds the immutable selected inode table and resolver.
BaseFact persists an exact present/absent result with that context.

Seal initial effects only after every binding addition/removal and directory
value is known, preserving the current all-effects-before-values discipline.
A single ordered ReferenceState cursor carries complete state to the zero-count
scan, rather than `touched_serials`' Vec or one new `find` per serial. The zero
scan joins base facts monotonically, keeps the root protected, and emits
ReleaseJob rows containing the effective kind/root the release actually needs.
It never collects a zero vector or prefetches every seed into a map. Fresh builds
use the same serial/count/value join, streaming directly to `apply_inode_values`.

Release mutates a distinct current count phase through exact indexed gets/puts;
sealed initial effects and prepared input remain immutable. After release, seal
the final count phase and stream final rows in serial order through the existing
inode builder. No canonical output is taken from a stale sealed initial phase.
OrderingBacking remains for genuinely different-key sorts and compatibility
spill paths; do not retain a simultaneous duplicate resident authority.

Exit proof: multi-alias files, non-file single binding, same-name replacements,
new orphan policy, mixed fresh/existing counts and directory moves. An independent
event interpreter derives exact final values/removals. Count additions, removals,
point operations, serial joins and final rows. Bounds concern actual transitions
and emitted rows, not historical spill prefixes. Complete before-effects resource
admission, accepted-output custody and real cleanup are separate gates.

### R1d-namespace-graph: one exact graph proof per selected input

Replace validation's additions/alias/cycle maps using the sealed binding source,
paged ParentEffect/NonFileParent/BaseFact/VisitFact and bounded graph work cursors.
For v1 base authority, scan the reachable base once with paged exact seen facts
and continuations; stream each base directory page against the change cursor.
Emit original and surviving parent-site facts for candidate non-files, retaining
exact names as values. Ordered child grouping compares at most the finite sites
needed to prove one final directory/symlink parent; a second surviving site
refuses. Regular-file aliases retain their ordinary count semantics.

Use the validated effective directory parent relation as the graph authority;
do not retain a duplicate complete adjacency graph. Follow its parent links
with one graph-color/reachability proof, persisted continuations and exact
seen/color state. Each selected parent link is expanded once; shared completed
paths reuse that exact result. Directory input/base cursors advance strictly;
the graph does not get walked afresh for every changed binding. For a build, prove every
required declared directory reachable from root exactly once, preserving the
current unreachable-parent exclusion and disconnected-cycle refusal. For an
update, preserve root, no-cycle and legal move/alias predicates. A cycle in an
otherwise disconnected affected component must not evade the selected proof.

The work law is O(V+E+C) logical graph/merge visits for one selected v1 operation,
plus indexed point costs and required ordering passes. It remains a whole-base
v1 qualification cost; it does not prove a small edit has population-independent
validation. After R3's certified v2 parent index, select effective parent
overrides and follow the affected ancestor closure once with shared exact
colors, giving O(C+A) logical visits. Authentication alone cannot select this
incremental capability. No cached certificate from a different head/context,
ad hoc ancestor walk per change or query-based guessed certification is allowed.

Namespace depth is not the extent-tree height31. The path API's256 components
also do not establish a graph-depth limit for arbitrary prepared serial graphs.
Spill namespace continuations as records; do not use either constant to justify
a resident depth/population vector or silently reject an existing admitted graph.
Validation's resident memo becomes one admitted current base wave plus exact
paged facts, with distinct absent/unknown custody.

Exit proof: legal same-/cross-parent permutations, aliases outside changed
directories, symlink single-parent facts, root move refusal, multi-change cycles,
disconnected build cycles, and a deep graph. Independent semantic interpreter
proves the expected graph or exact refusal before canonical construction.
Count actual directory pages, edges, node expansions, point probes and ordering
rows for multiple changes sharing ancestors/subtrees; no C*V repeated walk.
V1 compatibility and later v2 certification remain separate proofs.

### R1d-release-compose: paged release through final root and cleanup

After reference and graph state, integrate ReleaseJob/ReleaseCursor plus an
exact per-serial queued/released OnceFact into `release_zero_count`. Consume
jobs by increasing sequence, listing one existing 64-entry/8-KiB directory page
per cursor step. Update its last exact name once after acknowledged effects;
enqueue a child only on a proven zero transition. Carry its effective kind/root
from the already read page/current state. Preserve addition-before-release and
root protection, and do not buy the child's base inode again after the page
already supplied it. Queue/cursors live in the owned state, with one current
page/wave resident; arbitrary namespace depth cannot grow a Vec stack.

Each effective edge is removed once, each zero directory is queued/walked once,
and each cursor continuation strictly advances. Shared regular-file aliases
remain present until their final count reaches zero. Seal final counts, finish
the streamed inode builder, close all row/run/state readers, and perform one
checked known cleanup before filesystem-root emission, matching the existing
[update.rs](../../../crates/layerfs-content/src/filesystem/update.rs#L499)
lines 499–519 boundary. Accepted canonical output and Unknown scratch ownership
remain charged on failure; cleanup cannot silently become success or retry.

Exit proof: a broad removed subtree, a deep removed chain, a moved child retained
elsewhere, aliases crossing the released boundary, orphan/new-directory cases,
and successive C1 operations with separate selected phases. Owning real-provider
tests prove bytes, namespace facts, final root, exact row/run EOF, allocation
overlap, checked file cleanup and failure custody. Only this composed source can
claim retirement of the named complete C1 populations.

## Admission, proofs and precise remaining gaps

Every producer first selects a finite profile and obtains C1/C2 scratch, run,
cache/decoded, FD/connection, engine and cleanup credits. A bounded page refuses
selection/key/value/count/byte/capacity errors before its SQL/append effect.
Operation-wide predictable simultaneous needs are admitted before file/SQL and
before accepted canonical output. Semantic validation may use already admitted
private metadata; it still precedes canonical construction. Accepted data and
Unknown resources retain their exact owner, rather than receiving a guessed
refund. Mutable scans use a distinct writable phase or immutable selection;
`page_after` on a changing table is not a seal.

The initial R1c 16-MiB DirectoryRoots class cannot automatically cover R1d's full
state. Merely 65,536 maximum BindingChange records occupy
`65,536*304 = 19,922,944` logical bytes before database pages, other tables or
merge overlap. Conservatively sum each live table/run phase and physical
high-water before enabling a shape. If it exceeds the selected class, record
that scope unsupported/refused; do not grow a quota after a miss or imply the
old maximum-input arithmetic proves disk fit. Avoid duplicate complete graph
representations and release obsolete phases only when no selected reader owns
them. Freelist/delete is not a physical refund.

Current C2 scratch containment and process bootstrap remain prospective. The
[R1c provider inventory](R1C-STATE-DESIGN.md) records system SQLite MEMSTATUS
disabled by default and distinguishes current bootstrap insufficiency from the
published FFI's possible explicit pre-init capability. Root owns R1e's audited
first-party guard; this note changes no SQLite configuration and claims no Core
runtime enforcement. Unknown native file identity, physical cache observation,
engine enforcement, Darwin/Linux provider differences or cleanup cannot be
replaced by a heap-only/lifetime-peak result.

Expected-result references must be independently pinned: reuse the
[R0 canonical method/vectors](R0-CANONICAL-CONTRACT.md) for v1 and keep new v2
identity separate. The small independent semantic interpreter is external test
code, not product hooks. Real process/kernel/provider coordination establishes
overlap; launch/submission/sleep alone does not. Allocation/count instrumentation
records actual ordinary API operations and remains a bounded diagnostic. This
note ran none of those proofs; all listed exit proofs are **unrun**.

Changed mapping call composition, draft serialization/finalization, ordering
point access, namespace graph passes, indexed counts and release scheduling are
precise #288 qualification dependencies. No seven-family campaign, Family2 rerun,
Family8/9 row, speed admission or release completion is authorized here. Full
Core owning checks, component docs, product-boundary/LOC accounting and each
checkpoint's publication remain root's responsibility on the actual source.
