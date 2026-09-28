# #273 phase 4.5: bounded hot WRITE publication

> **Status:** Implementation specification, prospectively adopted at
> `941613f65`. Private index v2 is implemented at `91c9c4938`; hot publication
> and lifetime work is implemented through `a2359620a`, with the frozen
> functional candidate/proof source `4ae36ad3a`. The [append-only
> log](PHASE4.5-LOG.md), [frozen handoff](HANDOFF-PHASE45-FROZEN.md) and
> [identity record](evidence/phase4.5/hot-publication-20260928/FROZEN-CANDIDATE.json)
> state the verified scope and retained failures. No performance result or
> release admission is claimed. Checkpoint 5 remains **NOT_RUN** pending owner
> review. This status update changes no registered case, control, cache rule
> or bound below.

Original proposal source reviewed: `79eca5ddd1b8d6eb1f84a53f5e2486a83bac0d27`; the last
checkpoint-4 product change is `05fca30fb46a616a6988a60d3532f4255c9b3cf8`.
The before-#273 control remains
`48b51e874a41b3e1e6c6661e145316df8b408f07`.
The [research audit](PHASE4.5-RESEARCH-AUDIT.md) records source findings,
alternatives and primary research. The [checkpoint-4 log](CHECKPOINT4-LOG.md)
remains the authority for its historical results.

## 1. Scope, sequencing and integration

Implement the ordinary hot tiny-WRITE route before checkpoint 5. Cover EOF
append and an advancing edit frontier through inherited Base/Zero coverage,
including the original #248 separated-write pattern. Keep general indexed
splice for arbitrary overlap, large Payload extents and other mutations.
The target is amortized constant **structural index operations** for a
qualified resident frontier. Section 9 retains the other CPU terms.

The owner selected **hot path; concurrency constraints only**. Concurrent SDK
Exec admission, control sessions, process-group ownership, Stage scheduling
and Store transaction admission are outside this implementation. Hot state
belongs to the Workspace inode/view, so later concurrent Exec can use it.

The main lane's [#264](https://github.com/Ephemeral-AI-Lab/layerfs/issues/264)
and draft [PR #269](https://github.com/Ephemeral-AI-Lab/layerfs/pull/269) own
identity-relative mounted namespace and charged ancestry. Their design
recognizes 256 nodes as an allocation chunk and leaves several other limits
for separate audits. The owner assigns adjacent namespace/resource work to
that lane. Do not recreate it or introduce a new 256-node/128-dirty cap here.

**Merge order, owner direction: fully complete side lane #273 first; merge
the main lane afterward.** #264 integration is not a prerequisite for this
lane's phase 4.5 implementation or checkpoint-5 samples. Develop and prove
the hot path against this lane's existing stable inode identities. When the
main lane later integrates, pin its actual source and repeat the affected
combined ancestry/active-backing/handle/admission proofs. Its currently posted
receipts do not establish every broader resource change.

Both lanes use the name phase 4.5: qualify this work as **#273 hot backing**,
and that work as **#264 mounted namespace**. Checkpoint 5 stays **NOT_RUN**
until this lane's hot implementation and focused proof are reviewed and its
candidate is frozen. Preserve the registered cases, control and limits.
Publish necessary prospective format/count amendments before measurement;
never relabel checkpoint-4 receipts as v2 or speed results.

## 2. Required Workspace properties

The repository [AGENTS.md](../../../../AGENTS.md#workspace-private-backing-and-running-processes)
records these owner requirements:

1. Private backing is incremental throughout the attachment, **before and
   after Commit**. Edit changed records/reached paths and share untouched
   pages. No Workspace rebuild, inherited-subtree copy-up, historical-WRITE
   replay, or backing reset at an Exec/Commit boundary.
2. Running processes continue during capture, Stage and Commit. No process
   suspension/freezing, Exec-completion/drain barrier, whole-Workspace
   quiescence, restart or unmount/remount as an implementation prerequisite.
3. Capture pins complete G1 selection and advances live generation to G2.
   SaveFile/C1/C5 read G1 while G2 edits continue. Reconcile applies the
   captured changed set and preserves G2, held handles, selected readers,
   aliases and known/unknown-outcome custody.
4. Ordinary filesystem mutation serialization remains. Count work under
   its gate; bulk transfer and unrelated retirement run outside it. These
   rules do not assert zero callback blocking or a hard bound on OS/native
   I/O latency. Explicit shutdown/cancellation keeps its authorized lifecycle.

At the proposal's reviewed source, reconcile scanned changed-file extents and
prepared index work while holding the Workspace state lock. The following
requirements govern its replacement; the implemented source and remaining
affected work are recorded in the log. Do not
add a global cache flush or process barrier. Prepare frozen dirty rows, saved
facts and extent-deletion descriptions outside the state gate where their
selected identity permits it. Under the gate, recheck each live inode version:
unchanged G1 may use its prepared patch; intervening G2 retains its selected
extents. Account remaining affected-index preparation/installation honestly;
do not describe it as constant-time. A known C5 result remains owned if local
reconcile fails; never resend it or await quiet writers to resolve custody.

## 3. Proposal-reviewed architecture: why a leaf cursor is insufficient

```text
ordinary mounted WRITE
  |
  +-- mutation permit / handle / coherence checks
  +-- own_payload: bounded temporary p-* input
  +-- I lookup in serial_original
  +-- I + D lookup in active_file
  +-- E floor/range -> sorted E/R/L updates
  +-- shared pack candidate + P + I + D
  +-- generic B-tree candidate
  |      leaf replacement
  |        -> exact-max/physical-child replacement in parent
  |        -> copied ancestors to new root
  +-- select revision -> another I lookup -> all-Node attribute scan
  +-- checked notification -> FUSE reply
```

Branches store exact child maxima and physical PageRefs. Even a cached leaf
changes ancestor records after replacement. The ordinary route also touches
I, D, E, P and R. Wrapper lookups and retirement sweeps must be counted. The
current `HotInode` type is a codec, not an existing hot cache.

## 4. Selected core architecture

Keep one pooled ordered index and existing authenticated page/pack ownership.
Add stable tagged hot targets, fixed partition fences and one selected
bounded directory. Reuse existing crates, Budget, PageStore, verified I/O,
mutation permits and Service lowering. No per-file backing/index root,
separate extent/locator forest or general cache interface is required.

```text
SDK Mount + ordinary Exec             future concurrent Exec admission
            |                         (outside this phase)
            v
FUSE callbacks -> ProjectionMutationPermit
            |
            v
Workspace mutation coordinator
  handle + baseline + generation + coherence validation
            |
            +---- ineligible ----> generic v2 indexed splice
            |
            v eligible
  bounded HotCursor set (inode identities, never Exec identities)
            |
            v
  HotCandidate: shared pack + I/E/P/R [+ first D]
            |
            v
  verified page candidates + candidate HotDirectory
            |
            v
  ONE selected ActiveView publication
            |
            +---- checked notification -> accepted-byte reply
            +---- selecting-pin retirement -> exact unlink/refund

  Capture: pin ActiveView G1 -> live G2
            |
            v
  frozen v2 resolver -> existing SaveFile -> C1 -> C5
            |
            v
  captured-row incremental reconcile; preserve live G2
```

### 4.1 Selected view and resolver

```text
ActiveView
  incarnation / generation / revision / baseline identity
  root: Cold(PageRef) | Hot(slot, reuse_epoch) | Empty
  height
  selected HotDirectory PageRef (optional when no Hot target exists)
  selected pack-tail state / watermark
       |
       v
fenced branch: [lower, upper) partition
       |
       +-- Cold(id, epoch) ----------------> immutable physical node
       |
       +-- Hot(slot, reuse_epoch)
                    |
                    v
           THIS view's HotDirectory[slot]
                    |
                    v
           authenticated physical node version
```

The directory is authoritative selected state; the cache is a charged copy.
Never resolve a frozen HotRef through the live directory. Every get/floor/
scan/read-file, pack-locator/inverse lookup, directory handle, capture,
lowering, compaction and reconcile uses the composite selection. Carry height
and page birth facts; do not re-walk the root/replaced pages to discover them.

### 4.2 Prospective private index v2 grammar

Freeze and implement this grammar together. Preserve C1/C2/Bridge formats,
leaf key/value meanings, v1 pack records and large Payload backing. New
attachments select v2. Do not adopt/migrate an older incarnation or change
a running attachment's format. No durable root manifest/recovery promise.

| Item | Proposed encoding and validation |
| --- | --- |
| Index file | `a-index-v2-<page-id>-<epoch>`; verified exclusive relative create; 4,096 bytes |
| Directory file | `a-hot-v2-<page-id>-<epoch>`; same allocator/custody/authentication/block accounting |
| Header | Existing 128-byte framing; index magic `LFSAIDX2`, directory magic `LFSAHOT2`, version 2; explicit leaf/branch/directory kinds |
| Leaf | Existing `key_len:u16, value_len:u16, key, value`; typed family validation preserved |
| Child target | 17 bytes: `tag:u8, first:u64, epoch:u64`; 0 = Cold page ID/epoch; 1 = Hot slot/reuse epoch; other tags rejected |
| Branch cell | `fence_len:u16, fence, target`; nonempty fence is exclusive upper key; zero length inherits upper bound for final child only |
| Directory prefix | `slot_count:u16=64, active_count:u16` equal to occupied entries, `reserved:u32=0` |
| Directory entry | Fixed 32 bytes: reuse epoch 8, physical PageRef 16, level 1, kind 1, reserved 6; array index is slot number |
| Directory body | `8+64*32=2,056` bytes; header record count 64; vacant entries and remaining 1,912 body bytes/tail zero |

Cold targets require nonzero page ID/epoch. Hot slot is 0..63, epoch nonzero;
lookup rejects absent/mismatched epoch, kind or level. Validate selected
incarnation, header identity/digest, zero bytes, slot count and occupancy
before exposing records. Extend production boundary-guard format/name
coverage; no test-only parser path.

Directory entries target index leaves/branches only: level 0 iff leaf,
level 1..7 iff branch. Directory pages cannot themselves be child targets.
Reject cycles, duplicate reachable Hot slots and inconsistent child levels;
the eight-level descent bound is also enforced during validation.
Keep the charged per-slot epoch high-water array in the incarnation owner;
vacant directory entries do not reset it.

Fences partition the inherited key range; they are not child maxima. Each
cell belongs to its inherited `[lower, upper)` range. Rightmost child inherits
its parent's upper bound, ultimately infinity. Split adds the first key of
the right partition as the left exclusive fence. Validate ordered finite
fences and level changes. Atomically prune empty child subtrees during
mutation; every selected branch child is nonempty. Then `floor` descends
the matching partition or the preceding nonempty subtree's right edge,
retaining O(h) seek. Do not retain an arbitrary empty-partition walk and
claim the same bound. Validate floor/search against inherited ranges;
current exact-max logic cannot be reused unchanged.

### 4.3 Hot closure and ownership

- Every current Hot node is reachable, and **all its ancestors are Hot** to
  the selected root. A Cold subtree cannot conceal current Hot targets.
- At most 64 current Hot nodes and 8 cursor descriptors. Shared root/branch
  and I/P/R leaves count once. Admission computes its full affected path
  union before allocation; eight arbitrary maximum-height files need not fit.
- Minimum supported profile: when overall Host/backing budgets permit it,
  at least one inode's I/D/P/R plus <=2 E paths at maximum eight-level height
  must admit (<=48 nodes before shared-path savings). Count and byte caps
  must support this concrete profile; classifying every WRITE as a boundary
  cannot satisfy the hot-path gate.
- Cursor key: `(incarnation, inode, generation, baseline identity, selected
  inode/frontier versions)`. Other files advancing the global revision do
  not alone invalidate it. Attributes/link/truncate/reconcile update or
  invalidate affected cursors under the publication boundary.
- Retain selected I, exact D membership, E frontier and Base/Zero source
  coverage, partition paths and versions. P and R follow the **shared live
  pack tail**, never a stale per-file copy. Shared leaves get one merged
  candidate, not independent competing cursor versions.
- Return acknowledged inode/attributes from publication. Use validated
  selected facts in `serial_original`, `active_file` and `inode_updates`;
  remove the four root I walks around the specialized write. Replace the
  all-Node attribute scan with `node_index`/a validated retained slot.
- Replacing a logical node retains its HotRef epoch. Reassigning a slot
  increments the charged per-slot reuse epoch; overflow refuses. Never
  reuse the same pair while any view/candidate can name it. Older numeric
  slots can coexist with new epochs through old pinned directories/pages.

## 5. Eligible WRITE and atomic publication

Choose the route before candidate allocation. Missing cursor/partition is
explicit cold admission or generic work. A candidate error returns once;
do not retry through another algorithm or publish cached facts prematurely.

| Class | Eligibility / bounded affected set |
| --- | --- |
| Ordinary EOF append | 1..128 bytes; selected EOF; already dirty; admitted I/E/P/R paths; no split, pressure relocation or cache admission |
| Advancing inherited edit | 1..128 bytes; advancing offset; overlap only current Base/Zero frontier; exact gap/suffix/source offset; <=2 E leaves and admitted insertion/source partitions |
| Boundary event | First D insertion, inline-to-E promotion, split/carry, first family/partition admission or selective eviction; pays declared boundary work |
| General splice | Arbitrary overlap, completed changed ranges, Payload/L owners, many affected extents or incompatible selected state; generic semantics through v2 resolver |

Increasing offset alone is insufficient. #248 writes inside an 8,194-byte
base file; it is not EOF append. Preserve its Base suffix, source offsets
and gaps across E partitions. No per-WRITE search of completed extents in
an admitted ordinary case.

Regular shared-pack rollover must advance the **already admitted P/R
frontiers** with counted structural carries, without a new root search for
each logical pack page. A new pack prefix alone is not cold admission:
otherwise one admission per approximately 80 one-byte slots leaves
O(W*h/80) aggregate work. Stable append/separated sequences keep cold
admissions bounded between explicit captures/evictions or genuinely missing
partitions; P/R rollover work belongs in their amortized carry proof.

```text
validate selected view
        |
        v
reserve complete memory + physical candidate + retirement bounds
        |
        v
stage verified pack and changed hot leaves (acknowledged pages untouched)
        |
        v
stage directory copy selecting every candidate version
        |
        v
install ONE ActiveView + matching inode/cursor/dirty facts
        |
        v
retire/enqueue replaced owners; retain cleanup cause and exact charge
        |
        v
checked notification -> attempt reply with permit held and typed outcome
```

No in-place acknowledged pack/index writes. Selection covers bytes, length,
attrs, D, P/R, generation and revision. Notification failure after selection
retains the published view/coherence outcome. Quota/EIO before selection
leaves old view exact. Unknown allocation/unlink/outcome retains custody;
no refund/retry on a guess.

Attempt ordinary unpinned release in the triggering WRITE before returning
its cleanup outcome and before the FUSE reply. Pinned cohort work belongs
to the eventual release event. Do not move WRITE work after acknowledgement
or lose the invocation's typed published/cleanup result.

Current FUSE creates one temporary `p-*` even for a one-byte input. Retain
and count that constant I/O floor in this minimum scope. A genuine borrowed
tiny-byte API could remove it later while preserving admission/deadline/
copy-before-reply semantics; it is not needed for the structural proof.

## 6. Carries, generic mutations, capture and retirement

### 6.1 Byte-balanced splits

Use balanced contiguous splits near the byte midpoint. Full-left/short-right
can repeatedly split before a cold Base suffix. For body `C=3,968`, largest
leaf cell `r=788`, require the conservative normal-overflow lower occupancy
`C/2-r=1,196` bytes in each half, both fitting C. Proposed largest branch
cell is 291 bytes; its conservative lower occupancy is 1,693 bytes. Preserve free
capacity in the advancing partition.

Keep the frontier HotRef where practical; seal closed siblings Cold only
when they contain no current Hot descendants. Shared hot subtrees remain
Hot. Update parent/fences only on structural carry, and propagate as needed.
Maximum level 7 permits eight levels; height overflow refuses atomically.
This private format ceiling is distinct from #264 namespace admission.

### 6.2 Selective normalization

```text
evicted cursor's unique Hot leaf
       |
       v
parent candidate: Hot(child) -> Cold(selected physical child)
       |
       v
remove only that directory entry
       |
       v
repeat through affected unique ancestors
       |
       v
stop at shared Hot ancestor; preserve other cursors
```

Use bounded current-user masks/counts and affected parent paths. Frozen views
own old directories via pins; they are not current cursor users. Generic v2
mutation updates affected Hot nodes natively or normalizes only affected
closure. No all-directory flush on miss/capture/Commit/unrelated rename; no
historical E scan to seal a leaf. Charge the triggering operation and select
the complete result atomically.

Changing `Hot(child)` to `Cold(selected physical child)` **does not retire
the child page**: it is still current/reachable through the Cold edge, with
uninterrupted lifetime and charge. Retire only owners that lose their final
current selection, such as replaced parent/directory versions. A directory
entry diff alone is not a physical retirement proof.

Cold-to-Hot admission and Hot-to-Cold normalization preserve the physical
page's original birth/current interval. Replacing its target representation
does not start a new physical lifetime or discard older selecting pins.

### 6.3 G1/G2 with active processes

```text
same running processes / same mount / same handles
G1 writes ----> | pin G1 + select G2 | ----> G2 writes/read/heartbeat
                | publication gate |
                          |
                          v
        frozen root + frozen directory + pack selection
                          |
                          v
             SaveFile -> C1 -> C5 (off state gate)
                          |
                          v
       captured-row reconcile + saved-base installation
                          |
                          v
preserve G2 Hot mappings/bytes; release only unselected owners
```

Capture pins the resolver without sealing every cursor. Clean Commit does
not drain caches or rebuild the active index. Edited Commit touches captured
dirty rows/necessary paths; unrelated hot entries survive. A new generation/
baseline may require charged cursor readmission without rewriting untouched
backing. Preserve held handles and exact accepted G2 revisions.

### 6.4 Selecting-pin retirement cohorts

Replace active index/directory retired-vector sweeps on every WRITE with
charged cohorts keyed by selecting revision. For retired page lifetime
`[birth, retire)`:

1. Find latest selecting pin in that interval (`G<=160`).
2. None: attempt exact release in the triggering operation.
3. Otherwise enqueue under that revision, with entry reserved before selection.
4. Its final pin release processes **only that cohort**. Reassign to latest
   remaining selector or release. Drop empty charged bucket allocations.

Pin registration/publication/release remain ordered: no unregistered older
view may appear after its owners have been released. Failed unlink/identity
checks retain explicit failed custody and physical/memory charge; do not
return it to a vanished pin bucket. Preserve selected reads and cleanup/write
refusal policy. Refund observed successfully unlinked blocks only.

Pack/payload retirement must also avoid inspecting unrelated pinned owners
on ordinary WRITE: use selecting cohorts or existing triggered zero-owner
events as appropriate. Count remaining legacy metadata/payload maintenance
separately; index counters cannot hide that cost. Each retired item can move
through up to G cohorts under newest-first release. Keep the physical
PageStore BTreeMap and its honest CPU term; a slot-registry redesign is
additional custody work.

## 7. Expected files and responsibility

Paths are repository-relative. Create modules only with real implementation.
Production files <=999 physical lines; lib.rs/mod.rs <=200 and declaration/
delegation only. Tests stay external; no test-only product branches or APIs.

```text
core/crates/layerfs-workspace/
  src/
    backing/active/
      mod.rs               declarations/reexports
      page.rs              framing; v2 index/directory kinds
      keyed.rs             fixed fences/tagged targets
      records.rs           existing typed family validation
      index.rs             selected resolver; generic get/floor/scan/splice
      hot_directory.rs     NEW fixed table codec/view resolution
      hot_cursor.rs        NEW charged inode/frontier admission/eviction
      hot_path.rs          NEW eligible candidates/balanced carries
      retirement.rs        NEW selecting-pin cohorts/release events
      generation.rs        coordinator/composite publication
      pack.rs              existing shared tail; v1 body preserved
      pages.rs             existing physical custody/quota
      extents.rs           general overlap/source coverage
      reclaim.rs           inverse owners/pack/payload retirement
      compaction.rs        relocation through selected v2 view
      lifetime.rs          affected-owner reconcile/cleanup
      reader.rs            captured pack-locator resolution
    filesystem/
      original.rs          selected hot inode facts; later #264 identity reads
      active_file.rs       admission/dispatch/published attrs/indexed Node
    overlay/
      snapshot.rs          composite pins; G1 -> G2
    commit/
      active.rs            Service lowering/incremental reconcile
      active_scan.rs       extract scan/preparation if line limit requires
  tests/
    active_backing.rs      format/custody/count proofs via ordinary APIs
    stage.rs               Service/mounted G1/G2/Commit/continuity proofs
    stage_route.py         external receipt/oracle registration
core/docs/issues/273/
  PHASE4.5-IMPLEMENTATION-SPEC.md
  PHASE4.5-RESEARCH-AUDIT.md
  HANDOFF-CHECKPOINT5.md
  evidence/phase4.5/        create with actual proofs
```

Current `generation.rs` is 888 physical lines; `commit/active.rs` 939. Put
new behavior in focused modules before their ceiling. Update
[active architecture](../../architecture/proposal/fuse-workspace-snapshot-overlay/60-active-backing.md)
in the same commit as format/algorithm source changes. No new dependency.

## 8. RAM, physical backing and admission bounds

### 8.1 Prospective hot caps

| Domain | Bound and charging |
| --- | --- |
| Current hot nodes/cursors | <=64 nodes, <=8 cursors; charge masks/descriptors/epochs/retained capacities; shared nodes counted once |
| Resident node/pack cache | <=64 index node pages, <=8 pack pages; directory copies included separately in byte cap |
| Hot resident byte cap | 1 MiB includes raw pages, decoded capacities, directory, cursors/paths/metadata; enforce count and byte caps together |
| Ordinary candidate | EOF <=6 active page files; advancing edit <=7; mounted temporary input additionally <=1 p-* |
| Split-only candidate | Conservative <=98 active page files under six-path/eight-level derivation; combined events use their own bound |
| Combined eligible admission/normalization | <=227 active candidate files under the merged-plan derivation below; excludes inline promotion/arbitrary overlap/pressure compaction |
| Overall Host memory | Existing configured Budget, default 8 MiB; precharge retained/transient allocations and old+new capacity overlap |
| Physical backing | Existing quota; actual observed blocks; candidates/pins/failed owners charged until successful release |

Raw node/pack subtotal: `72*4,096=294,912` bytes (288 KiB). **Not total RAM.**
The 1 MiB resident cap includes decoded buffers/directory/cursors; not every
possible 64-node decoded set must fit. Candidate scratch and retirement
custody are separately pre-reserved from the same overall Budget. These
caps do not bound process RSS/cgroup/page cache without separate evidence.

Ordinary EOF: pack 1 + <=4 I/E/P/R leaves + directory 1 = <=6 (24 KiB under
verified one-block-per-page allocation). Advancing edit adds a second E
leaf: <=7 (28 KiB). First D is a boundary event. Counts exclude existing/
retiring owners and temporary input. Independently check `st_blocks*512`.

Split-only: <=6 changed leaf paths over <=8 existing levels; at most two
candidate nodes per affected existing node yields
`2*min(64,6*8)+2=98`, including pack/directory. Root growth must fit the
maximum successful eight-level height and a separately checked node count.
Each affected existing node is staged at most once. At height eight a root
carry cannot grow; at height <=7, adding one new root still stays below 98.
This bound excludes cold admission, normalization and inline promotion.

Combined eligible admission/normalization: affected existing Hot closure
<=64 nodes, plus <=6 cold seek paths of <=8 levels gives <=112 existing
nodes. Merge normalization, admission and bounded-key mutation **before**
physical staging; each existing node produces <=2 candidates once. Including
<=1 new root, pack and directory yields `2*(64+6*8)+3=227` newly created
active files (929,792 B = 908 KiB at 4 KiB each). The bounded I/D/P/R and
<=2 E paths must establish the two-candidate property. Inline promotion,
arbitrary overlap and pressure compaction do not inherit it. The 64-node
term permits only necessary affected closure, never unrelated normalization.

Both caps include failed staged candidates. They exclude existing/pinned/
retiring owners, mounted temporary input, physical allocation excess and
memory/custody reservations, which remain charged separately. Reserve the
resulting Hot-slot occupancy as well as candidate pages: a shared split
sibling can need another Hot slot. Exceeding height/slot/quota/byte bounds
refuses before selection; do not enlarge a bound to pass a selection.

### 8.2 Growing general structures

Physical custody is O(A) charged entries (128 B allowance/owned page) and
O(log A) map lookup. Retired owners, dirty identities, observed namespace
Nodes, moved origins and large payloads have their own charged growth.
Admission refusal is a bound, not unlimited scale in an 8 MiB Host.

Commit still builds O(E_f) final-file extent/descriptor scratch, with dirty/
saved maps growing by changed identities. Constant-RAM streaming Commit is
separate work. The frozen control already streamed its per-file descriptors
and replacements with a bounded cursor/window; checkpoint 4's materialized
vector is a Workspace scratch-scaling regression, retained explicitly in
this hot-path scope. This comparison excludes C1/Store construction and the
separate growing payload registry. Fix the audited **cumulative reconcile reservation gap**:
precharge growing `updates`/`node_updates` and every map/vector clone; a
per-file charge cannot cover cumulative entries after it drops. This is
necessary for bounded admission, not duplication of #264 ancestry work.

128 handles, 32 captures, <=160 distinct selecting revisions and eight
index levels remain separate actual bounds here. v1's 256-node and
128-successor-dirty statements are not active-route enforcement proofs.
After the main lane merges, pin its actual resource contract and check the
combined route without silently reinstating those counts.

## 9. Before/after complexity and bounded CPU claims

Definitions: W writes; K overlapped extents; E indexed extents; E_f final
extents in one dirty file; B page fanout/occupancy; h index height (<=8
levels); A all owned pages, including candidates/pins/failed custody; V
resident Nodes; G selecting revisions (<=160); J retired owners; L live tiny
slots; a cold admissions; x selective normalization work; M reconcile rows.

| Work/space | Before #273 (frozen control) | Checkpoint 4 source | Phase 4.5 target |
| --- | --- | --- | --- |
| Separated one-byte payload allocation | O(4 KiB*L) plus metadata/pins/transients | O(4 KiB*ceil(L/80)) packed payload plus index/pins/tail/dead-slot slack | Same pack asymptotic; shared current/candidate/pinned directory pages added |
| Eligible append/frontier publication | Path-local extent + keyed I/D COW: approximately O(h) for bounded overlap | Reached family paths O(h), copied ancestors, wrapper I/D searches | After admission, constant leaf/selector operations; balanced carries amortize O(1) structural operations |
| W separated/EOF writes, E=Theta(W) | Approximately O(W*log_B E) structural work plus actual edge/custody/cleanup | Approximately O(W*log_B E) structural work plus sorted update/custody/cleanup | O(W) qualified structural work plus explicit admission/carry/normalization/custody/lifecycle terms |
| Repeated same-location writes | Final spans constant without pins; O(W) path work for fixed namespace | General route; bounded K, smaller packed allocation | General route remains; no claimed asymptotic gain from append specialization |
| Arbitrary K-overlap WRITE | Touched subtree splice plus copied-page edge/ownership work; no whole-tree scan required | Approximately O((1+ceil(K/128))*h + K log K + affected tree/owner work) | General indexed semantics preserved; no O(1) arbitrary-overlap claim |
| Resident attr update | O(V) all-Node scan | O(V) all-Node scan | One O(log V) node_index lookup or validated retained slot |
| Physical registry | Existing allocator/ownership lookup | O(log A) per read/create/release/pin; status O(A) | Retained; no status scan for hot admission |
| Retirement | Owner-dependent release | O(J) pinned sweep can occur each WRITE; drain can repeat pinned prefix O(q*r) | No unrelated pinned sweep on ordinary WRITE; selecting/release events counted |
| Commit Workspace per-file upload scratch | Streamed descriptor/replacement cursor: O(h+fixed leaf/I/O window); already final-view lowering | Materialized O(E_f) extents and 24*E_f descriptor bytes; RAM scaling regression | O(E_f) retained/charged; incremental resolver/reconcile; precharged O(M) patch storage; no constant-RAM claim |
| Total RAM | Charged spans/owners/nodes and scratch grow by workload | O(A+J+V+dirty/payload owners), O(E_f) Commit scratch; reservation gap audited | Same growing domains + fixed charged hot cache; close gap; admitted allocations <= configured Budget |
| Retained versions | Actual frozen/reader custody | Selected owners held until final selector releases | Same ownership, including old directories and HotRef epochs |

For narrow separated/append writes, the old implementation shares untouched
subtrees and the source does not support a sustained O(W^2) claim.
Packing reduces a separated-write space constant, not its linear
asymptotic. Repeated overwrite can already have O(1) retained payload space
after maintenance when no old view is pinned. Historical old charged bytes
and later physical counts are not a matched frozen-source sample.

The log expressions expose dependence on tree size/height. Both reviewed
formats also cap height at eight levels, so traversal has a finite structural
ceiling within that supported domain. The optimization removes height-
dependent search/copy work on the eligible route; it is not a claim that the
existing finite-height implementation has unbounded per-WRITE traversal.

### 9.1 Structural amortization

An admitted stable frontier with bounded overlap and no forced eviction
changes constant leaves/one directory. Byte-balanced splits reserve enough
space for a positive bounded number of subsequent maximum-size insertions;
parents change only on child carry. Prove actual occupancy for leaf/branch
codecs, then sum:

```text
W ordinary writes + W/B carries + W/B^2 higher carries + ... = O(W)
```

Replacement-only I and existing P locator changes do not change fences.
P/R rollover advances admitted frontiers with amortized carries. First D,
partition admission and forced eviction pay explicit boundary work. A fixed
batch through an immutable spine remains O(W*h/B), not the proposed bound.
For mixed workloads retain `O(W + a*h + x + general-overlap work)` structural
work, plus actual compaction/release. A working set exceeding cache capacity
may pay admission/eviction each WRITE and remain logarithmic. Captures and
generation changes are not free work moved out of a measurement.

### 9.2 Full CPU and lifecycle

For p<=6/7 ordinary active page candidates, retain:

```text
O(input_bytes + p*4096 + p*log A + log V + log G)
  + actual temporary-payload/legacy maintenance
  + charged split/admission/compaction/release events
```

This is **not demonstrated full CPU O(1)**. Page size/G/hot limits are fixed;
A and V remain budget-limited growing structures. No hard elapsed-time bound
follows from a deadline that cannot preempt a native kernel call.

One retired owner can move through G cohorts: conservatively
O(J*G*log G), plus physical registry/unlink costs. Pressure compaction pays
every slot/inverse reference relocated; one pack page can have many refs.
Report counts and atomic budget refusal, not unlimited bounded-CPU progress
for arbitrary overlap or retention.

## 10. Implementation sequence and proof gates

| Step | Completion condition |
| --- | --- |
| 4.5.0 proposal | Review v2 grammar, count bounds, incremental/process rules and later main-lane integration contract; preserve v1 evidence |
| 4.5.1 format/resolver | Every selected reader/capture uses tagged targets/fences/exact directory; malformed identity/epoch/fence refuses |
| 4.5.2 hot publication | Charged cursor/cache/candidate, I/D wrapper reuse, indexed Node update, EOF/Base/Zero frontier |
| 4.5.3 boundary/lifetime | Balanced carries, selective generic/eviction work, triggered retirement, G1/G2 reconcile and cumulative scratch precharge |
| 4.5.4 public proof | Focused external functional/count/custody/continuing-process proofs against this side lane |
| 4.5.5 handoff | Update architecture/status, retain results, per-commit production LOC, freeze source; then checkpoint 5 |
| Checkpoint 5 completion | Complete/review the registered matched set and independent proofs, retain all failures/qualifications; fully complete #273 before main-lane merge |
| Later main-lane merge | Integrate #264 actual active callers/ancestry/resource policy; run only affected combined proofs; no retroactive identity relabel |

Minimum external proofs through ordinary product APIs/mounted route:

- Growing eligible EOF and inherited separated sequences: all wrapper
  lookups/node visits/index/pack/directory I/O, candidate bytes, carries,
  retirement inspections/refunds. No ordinary nonboundary ancestor copy or
  root search. Count any remaining input/legacy maintenance separately.
- Every leaf/branch carry and inline promotion; both E partitions around
  Base suffixes; gap/Zero coverage; interleaved files sharing P/R and leaves.
- Hot-to-general overlap/Payload/truncate/attrs, aliases/link/rename,
  forgotten lookup and held open-unlinked file; generic reads resolve v2.
- Cursor eviction, slot epoch reuse, old reader/directory/capture pins,
  both pin-release orders; unrelated retained owners cause zero ordinary
  retirement inspections.
- G1 SaveFile/C5 beside G2 writes. A real mounted process spans Commit,
  continues heartbeat and acknowledges G2 edits without pause/restart/remount.
  Independent old/new head and G2/held-handle byte/identity oracle. One
  existing process can span Commit; no concurrent SDK Exec feature needed.
- After saved-base installation, continue an eligible append/frontier sequence
  on the same inode and an unrelated surviving hot cursor. Count necessary
  affected readmission separately; prove no unrelated closure normalization,
  blanket cache drain or private-backing reset before/after Commit.
- Atomic Host/backing quota refusal, candidate short-write/EIO, published
  notifier failure, failed unlink custody, independent st_blocks/charge
  equality and exact clean-close refund.
- Cache/descriptor/aggregate Budget exhaustion, including old+candidate
  capacity and reconcile clones. Refusal cannot justify shrinking a case.

Use scoped locked-release Core tests, examples/Clippy/format, product-boundary
guard and self-tests. Report unchanged checkpoint-4 C1 test failures until
their lane resolves them; no CI/preflight claim. Keep one construction worker
except Init, cache/deadline policy, ARMv8 build inputs, no-sync/no-retry rules.
No performance sample is part of this specification. Use the
[checkpoint-5 handoff](HANDOFF-CHECKPOINT5.md) after this lane's 4.5 gates close.

### Checkpoint-5 reconcile patch lookup (prospective optimization)

`prune_dead` and `prune_dead_payloads` share the inverse-reference liveness
check. For each touched R/L logical page, probe only the bounded prefix range
of the ordered patch map instead of searching every unrelated update. Preserve
the exclusive `tag + 1` bound for logical `u64::MAX` and the subsequent paged
selected-index scan (including references beyond the first 128). This changes
patch lookup from O(P*M) to O(P log M + R) for P touched logicals, M patch
entries and R matching patch entries examined; selected-index lookup and
retained owners still have their own charged costs. It occurs after SaveFile;
it does not eliminate or accelerate the earlier source processing by itself.

### Checkpoint-5 packed replacement locality (prospective implementation)

For one captured G1 file SaveFile, final extents are in file-offset order,
while tiny packs are in mutation order. A single-page upload reader therefore
reloads the same pack for separated or dispersed edits. The source now reserves
at most 256 required Packed references and 32,768 scattered replacement bytes
per upload before making a remote SaveFile call. It visits only the declared
extents, sorts this fixed window by logical page, resolves each locator from
that upload's captured index, reads one pack at a time through the existing
identity-validating reader, scatters only required bytes, and emits them in
original stream order. Windows may cross Base/Zero/Payload descriptors; those
retain the original non-packed stream paths and large-record behavior. The
single-page reader (not an enlarged LRU) holds at most one decoded pack; the
additional reference/index/scatter vector capacities are charged against the
same configured Workspace Budget, including any old/new pack overlap.

A window's load upper bound is its number of distinct required logical packs,
not one load per output-order extent. Across windows the same logical pack may
load again. With a 256-reference limit, pure schedule arithmetic for the
registered 100/512/4,097 dispersed positions predicts 2/14/827 loads rather
than the old one-page-reader prediction 41/512/4,097; these are **hypotheses**
about the exact source schedule, not runtime observations or latency claims.
Existing whole-file descriptor and extent vectors remain O(E_f), are already
charged, and are not made constant-RAM by this source-locality optimization.
Source counters report complete flag, window/reference/distinct-pack counts,
actual page loads/hits, locator lookups and index reads/seeks, decoded records
and bytes, and source read/copy time for each SaveFile under the production
complexity diagnostic. Their diagnostic overhead is inside the observed phase.

### Checkpoint-5 publication-cause telemetry (prospective diagnostic)

A cumulative production counter group within PageStore accounts for page
framing/encoding, physical create/identity, allocation/accounting, direct
write, direct readback, authentication/byte comparison, and retired-page
release/unlink. The hot fit merge, actual selected merge, node encoding and
validated cache decode carry separate timers. Every phase sample subtracts its
own pre-WRITE counters; these subsets do **not** purport to sum to complete
Exec (FUSE temp input, other CPU/metadata work and timer overhead remain).
Counters run in the actual product, including their overhead in the measured
phase, on both successful and failed paths. Physical publication is still
immutable per selected incarnation; no readback or authentication is removed.
No format or worker limit is changed by observing these causes.

### Checkpoint-5 prepared hot-node reuse

Once `Mutation::emit` has encoded a Node and PageStore `create_from` has
verified its authenticated, byte-identical physical readback, retaining a hot
node no longer allocates and decodes another independent `Node` from those
same bytes. The existing selected-slot kind, record-count, epoch and revision
checks still govern publication and later reads still authenticate the page.
The retained prepared Node is charged at least as conservatively as the old
cache allocation, including overlap with the mutation's temporary owned Node
and the underlying stored/verified Page. Cold reads and admission of an
existing page continue to use full decode/identity verification. This avoids
one duplicate decode of a newly published hot node; it is not an authorization
to omit page readback or to reuse a node after another incarnation is selected.
