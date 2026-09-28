# #273 prospective WRITE/Commit scaling decision (research checkpoint)

> **Decision state:** hypotheses and bounded experiments only. No product or
> workload change, no new measurement, no qualified matched speed result. Source
> audited: `61aaad7b6722a6bceabbae95986e176815a37b28` (product last changed at
> `52878a58429a61fa40a17fc615b21f847aaacb3c`, tree
> `ce59ca331084e3b3a2c183f13bc5bc42b81200e9`). Frozen #271 control
> `48b51e874a41b3e1e6c6661e145316df8b408f07` is untouched. This is not
> the v2/v3 migration specification and authorizes **no** format change.

## 1. Evidence and decision rule

Inputs: [iteration log](CHECKPOINT5-OPTIMIZATION-LOG.md) 009–012, especially
WRITE 100/512/4,096 counters (the 4,097th WRITE is *after* the last sample),
[raw result](../../../../benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-012/RESULTS.json)
and [manifest](../../../../benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-012/SHA256SUMS),
[original campaign](CHECKPOINT5-LOG.md), [root-cause qualifications](CHECKPOINT5-ROOT-CAUSE-RESEARCH.md),
[implementation proposal](PHASE4.5-IMPLEMENTATION-SPEC.md),
[format authority](ACTIVE-FORMAT-AND-EVALUATION-v1.md) and
[registered workloads](../../../../docs/roadmap/0.1/0.1.7/issue273-checkpoint5-execution-spec.md).
The iter-012 manifest verifies from its own directory (267 paths); checking it
from the repository root produces false missing-file errors. No historical
receipt is promoted to this source. PR #274 was checked OPEN/draft; the
assigned worktree was clean and no Cargo job was found. Nine existing Docker
containers have other or unknown owners; none was stopped or adopted.

The nine latest full byte oracles, callbacks, cleanup and v4 WRITE checkpoints
PASS, but each row is **INELIGIBLE** without matched frozen control and full
cache/phase resource proof. The separate #248 public space result is 1,814,528
versus 18,751,488 allocated B at its 4,096-WRITE checkpoint (90.3% less,
10.33x smaller, under 3 MiB); its current public gate is **INCOMPLETE** for
missing explicit C1-zero provenance. Clean and one-edit *SDK* pinned-journal
controls are **NOT_RUN**. Exact three sequential Execs and complete mutations
are **NOT_RUN**. The two 25 s control timeouts are censored complete walls;
they have no completed Exec/Commit denominators. Repeated-100 latest complete
wall 1.612 s exceeds historical 1.228 s (both ineligible); its outside-phase
difference is unexplained. Do not sum `charged_backing_bytes` and nested
metadata into a physical-space claim: use independently checked `st_blocks *
512`, actual backing allocation, separate quota, Budget, RSS and phase-local
cgroup domains. Earlier patch-prefix O(P*M) and exact-reserve O(E_f^2) copies
were already addressed in iterations 001/009–011: no duplicate credit.

**Ranking:** Exec 4,097 append/dispersed/repeated is 4.671/9.308/5.188 s;
Commit 0.129/0.504/0.022 s. The dispersed Commit source fill is 0.072 s:
eliminating it entirely can save at most 0.072/10.666 = 0.68% of that
complete wall, holding everything else fixed. Prioritize WRITE publication
and admissible closure before a locality-only performance patch. An exact
physical page floor is not evidence that *all* generated index versions are
necessary. Do not infer a global quadratic WRITE from nine finite cases.

## 2. Symbols, interfaces and count boundaries

- `W` = accepted immutable public WRITEs in the measured Exec; `N` = required
  **final** Packed replacement references in selected G1 (not historic W);
  `P` = distinct logical packs in those references (physical versions may
  exceed P due to pins/revisions); `E_f` = selected extents for one saved file;
  `D` = affected inode/directory identities; `H` = selected index height;
  `A` = allocated physical page versions; `G` = retained selecting revisions/
  pins; `K` = maximum charged required references/window (currently 1,024);
  `B` = maximum charged replacement bytes/window (currently 32 KiB);
  `Q_fetch` = actual authenticated page reads, distinct from readback of newly
  created pages. `L_i` = distinct logical packs in window `i`; `r_p` = record
  count of logical pack `p`; `V` = index node visits, `J` = retired versions;
  `X` = affected leaf/branch closure plus its necessary contents; `C` = actual
  carries/splits; `U` = hot slots evicted/normalized; `T` = released owner
  entries, including pinned-cohort transfers. Page/slot fanout is fixed by
  4 KiB page grammar; height is **not** a fixed constant. All Big-O below are
  source-derived *conditional* count bounds, not proven timing bounds.
- Public FUSE WRITE enters `filesystem/active_file.rs` ->
  `backing/active/generation.rs::write_tiny_file` -> `hot_path.rs::try_hot_file`
  or `generation.rs::write_with`; the generic route plans affected `E`/`R`/`P`
  and I/D edits, compacts touched packs, then calls
  `index.rs::prepare_file` -> `splice.rs::admit/change` ->
  `IndexCandidate::publish`, `pack::prepare_from/publish`, retirement and
  notifier before replying. Other index callers: `Index::prepare` from
  generic index updates in `generation.rs` and `lifetime.rs`; payload writes
  share the index/generic route. No helper may optimize tiny writes by
  violating these callers' namespace/payload/held-handle semantics.
- Capture in `overlay/snapshot.rs` pins the active `IndexSnapshot` after
  sealing the pack tail; `index.rs::pin(true)` advances generation/revision.
  `commit/active.rs::prepare` calls `scan_dirty`, `scan_extents`, then
  `ActiveUpload` for SaveFile from `Captured::active_view()`; its one
  `ActivePackReader` resolves G1 locators through `IndexSnapshot::get`
  (uncached resolver), reads/decodes the *whole* pack, checks exact
  slot/inode/generation/revision/offset and copies required bytes.
  `commit/active_source.rs::Prefetch` sorts a bounded window by logical pack,
  then `ActiveUpload::pull` emits descriptors and replacement bytes in file
  order. Symlink/payload/Base/Zero paths do not use this packed reader; C5
  reconciliation in `commit/active_reconcile.rs`, `commit/reconcile.rs` and
  `commit/completion.rs` applies only captured changed identities while G2
  edits and process execution continue. The reader is exported from
  `active/mod.rs`, but the current caller search found only `commit/active.rs`
  and its `commit/active_source.rs` prefetch; repeat the search before any edit.

## 3. WRITE potential and custody model (to test, not assume)

For accepted WRITE `t`, let `x_t` be changed/normalized nodes plus metadata
records, `h_t` selected height, `c_t` carries, `u_t` evicted slots, `j_t`
retirements actually inspected, `a_t` new 4 KiB versions and `q_t` old-page
fetches. Let `d_t` include the affected identities and exact input bytes.
The candidate count oracle is

```text
sum_t [ input_t + output_ack_t + O(d_t * H_t + x_t + c_t + u_t*H_t
       + j_t*(log(G+1) + ownership_check_t) + a_t*(4096+log(A+1))
       + 4096*a_t readback + q_t*4096 + bounded hot/auth lookups_t) ]
```

`d_t*H_t` is a deliberately conservative upper term for distinct changed
lookup paths, not a claim that the same root must be fetched for each key.
`x_t` counts only reached closure plus boundary normalization; a full scan
of unrelated historical W per WRITE would violate the oracle. Relevant
`R`/`L` inverse references and compaction must be charged by *actual affected
reference count*, not omitted. `A` includes versions subsequently released;
resident bytes are a different quantity. Ordered-map operations include
registry/pin `O(log A)`/`O(log(G+1))` comparisons; physical file syscalls
and readback remain charged and deadline-sensitive even if lookup is fast.
Observers (v4 status formatting and four snapshots, daemon/callback/independent
oracle after the command) must be reported separately; none makes missing
C1 or cgroup data equal zero.

A possible amortization for an eligible monotone frontier: with byte-balanced
leaf/branch spare space `b_l > 0` at each level `l`, charge one token to each
insertion at each affected level, spend `b_l` tokens on a split/carry, and
sum `W + sum_l O(W/(product_{j<=l} b_j)) + initial/height-transition
costs`. This is **conditional** on the actual codec occupancy and no forced
slot eviction. `H` rises at root split; `IndexCandidate::prepare_file` grows
root only while `children.len()>1`; root-height transitions are a separate
count, not a constant-`H` proof. `hot_path.rs::hot_plan` validates selected
I/D bytes, frontier Base/Zero or EOF, ancestor pages/fences/epochs and fit;
`direct` may touch only changed leaves until a carry. Cache/slot potential
`Phi = unused eligible capacity + free charged slots` drops on insertion and
is replenished only by actual carry or eviction; once slots/bytes fill,
charge `u_t` for eviction and recapture explicitly rather than amortizing it
away. A replaced hot page retains slot+epoch (not physical page ID); eviction
and reuse increment epoch, and selected G1 directory must continue resolving
its old page. A generic replacement may change arbitrary offsets, so this
frontier proof must **not** be applied to repeated/dispersed schedules.

Generic admission currently clones at most eight cursors, assembles their
bindings plus the new seed, traces paths in `Mutation::admit`, marks every
unused hot slot obsolete and normalizes paths to its parents. It first removes
any old cursor with the same inode, but `seed.cursor` may refresh from that
`prior_cursor`; preserving it blindly could consume one of eight cursors or
one of 64 slots and miss a changed ancestor/fence. `admit` iterates all
occupied hot slots and `change` can traverse normalized branches even without
updates there. This is a **bounded per-WRITE** scan (64 slots, <=8 cursors,
1 MiB hot resident), not by itself an asymptotic W² term, but may account for
avoidable *page versions* when the displaced closure was not semantically
changed. Full binding authentication and future eligibility must be kept.
`Retirement::retire` checks the selecting frozen interval and holds an owner
in the latest selecting cohort; `release_selector` visits its bucket and may
reassign to another pin. No ordinary WRITE should iterate all `J` owners;
total inspections can include `O(J*G*log(G+1))` in adversarial staggered pin
release, which must be counted per pin event, not hidden as a WRITE constant.
`PageStore::create_from` checks unique identity, preallocation, exact physical
blocks, direct write, 4 KiB readback/authentication; `release` verifies owner
identity/blocks then unlinks/refunds, retaining failures. One pack candidate
per accepted tiny WRITE and its checked readback is mandatory here. Reuse or
preallocation cannot overwrite any selected G1/G2 incarnation or refund an
unknown outcome. Report lock-held time versus bulk work; never add a global
quiescence/barrier or after-ack continuation.

**RAM/physical bounds:** fixed hot state `<=8` cursors, 64 slots, <=64 index
nodes, 1 MiB hot bytes, <=8 pack pages; transient `prepare_file` scratch,
cloned cache/cursors, `Mutation::loaded/created/replaced`, staged page buffers
and retirement reservations must all be admitted to the default 8 MiB Budget
before allocation, with old+new growth overlap. Worst-case retained active
quota is `4096 * (live_versions + G1/G2-selected versions + unknown owners)`
plus physically checked payload/metadata, **not** `4096*N` and not constant
in W or G. No universal bound under unlimited pins follows from a fixed
cursor count: pin admission/quotas or refusal must close it. Memory pressure
or quota refusal is allowed before a new acknowledgment, never a partial
success. The per-WRITE acknowledgement, immutable versions, writes/readback
and changed-edge/canonical construction are O(W), O(A), O(required bytes)
floors; total public WRITE is not O(log W).

## 4. Commit recurrence, layout alternatives and lower bound

With `m` windows on the ordered final Packed references (`m <= N` when each
contains one or more), the *measured reader* loads
`L = sum_{i=1}^m L_i - adjacent_window_same_pack_hits` (when the one-page
reader retains the last logical pack; the counter `distinct_packs` counts
`sum L_i`). Thus `L <= sum L_i <= N`; with all ordered references to one
pack and a retained reader it may be 1, with adversarial pack permutations
it may approach N. `m >= max(ceil(N/K),ceil(required_packed_bytes/B))`;
reference/byte distribution can force more windows than this lower bound.
If all P packs recur in each window, `L ~ m*min(P,K)` until the `N` cap;
for P growing alongside W, a finite intermediate regime can resemble
`(N/K)*P`, **not** an unbounded global Theta(W²). Decoded records are
`sum_{loads} r_p`, at most `r_max * L` for the fixed pack grammar (80
one-byte records per page, fewer larger records). Locator seeks are <=L
for current reader; `Q_fetch` includes up to H authenticated index-page
reads per uncached locator plus pack reads, not the page-creation readbacks.
Existing result: dispersed 100/512/4,097 has `N=100/512/4097`,
`L=2/7/209`, `m=1/1/5`, locator index reads `4/14/627`, decoded records
`100/512/16468`. Append is `2/7/52` loads and `100/512/4097` decoded
records; repeated `N=1`, one load, `20/32/17` decoded records. G1
locators/slots remain selected even if the physical G2 tail is rewritten.

Commit count model for one saved file:

```text
O(E_f*H for 128-entry paged scans in a conservative uncached resolver model
  + E_f for extent validation/descriptors
  + N log K for window grouping + L*H for locator lookups
  + sum_loaded r_p for pack decode + required replacement bytes
  + C1/C2 canonical(file bytes, changed nodes, 1 worker)
  + D*H + C5(reached changed identities, applicable inverse refs/pins))
```

No `O(E_f*H)` term means scanning *all* historical W; improve it using
resolver page-path reuse if counters show that it is avoidable. Current
`scan_extents` geometrically grows the charged vector and validates contiguous
coverage; `ActiveUpload::new` additionally allocates a 24-byte descriptor per
extent. RAM has `O(E_f * (sizeof(Extent)+24))` retained during SaveFile, with
explicit old+new `Vec` capacity overlap during growth, plus `O(K*(sizeof
Reference+sizeof usize)+B)` scatter, one decoded pack page/reader, page
scan buffers, dirty identities `O(D)`, C1/C2 streaming workspace and pinned
G1/G2 page ownership. A transient exact `try_reserve_exact` can return extra
capacity: charge *actual* allocated capacity, not just requested. A file with
huge `E_f` can refuse under 8 MiB; **no constant-RAM Commit claim**. Each
SaveFile emits ordered descriptors then ordered replacement bytes; Base
references may read canonical old bytes inside C1, Zero produces required
zeros and Payload uses bounded stream, not the pack reader. Complete
command includes capture, metadata construction, remote C1/C2 and reconcile,
all under existing deadline; 72 ms source-only savings cannot yield a 2x wall.

**Pack-once impossibility (conditional, not a storage impossibility):** take
P packs, each containing one needed record in each of m ordered output blocks
in adversarial pack permutation. The source can hold <=8 pages and <=B
out-of-order bytes and must emit block 1 before block 2. For m blocks whose
future bytes exceed the charged scatter capacity, an online single-pass
ordered source cannot retain all future output after reading each pack once;
it must reload some packs, store an out-of-order spool of Omega(future bytes),
make bounded *additional* passes with their time/I/O charged inside Commit,
or change physical layout so ordered references co-reside. Reading all packs
up front while withholding ordered output merely moves the unbounded buffer
elsewhere. This does not preclude an order-preserving physical representation
or a valid bounded multi-pass algorithm; prove physical/version ownership and
one-ack cost before adopting one.

## 5. Competing designs (prospective; no chosen product change)

The following predictions are *operands and bounds*, not simulated measured
speedups. A valid design must beat the dominant Exec pool **net of** any
additional charged work. ``baseline`` means iter-012 current candidate, not
frozen #271. No design may branch on registered workload ID.

| Design | Mechanism and expected nine-cell count change | RAM/physical charge, locks and failure proof needed |
| --- | --- | --- |
| **A. Changed-closure hot/generic continuation** | Retain a same-inode cursor only when selected I/D/fence/epoch still authenticates; compute a union of *current required keys* with reusable selected closure, not unconditionally all stale cursor bindings. Avoid normalizing/rebuilding an old hot path when it is still the selected unchanged owner and space exists. For append 100/512/4097, baseline hot counts 98/510/4094 (at last sampled WRITE 98/510/4094), seek 45/45/50 at WRITE 100/512/4096; no promise of fewer mandatory pack writes. For dispersed 100/512/4096, seek 1443/7623/61389, admissions 27/396/3936 and normalizations 27/398/4330: expected *conditional* reductions in unnecessary U, selected page versions and retirement inspections; worst-case 64-slot thrash remains, and 5 hot WRITEs stay 5 unless genuine eligibility is proven. Repeated: zero hot; 1601/8203/65637 seeks at WRITE tiers are logarithmic-path queries, not a licence to misclassify overwrite as frontier. Commit L remains 2/7/52, 2/7/209, 1/1/1 unless write layout independently changes. | <=8 cursor/64 slot/1 MiB hot bytes/<=64 index node limits **unchanged**; charge candidate/old cache copies and changed closure. Existing writer+index lock must not acquire unrelated locks in reverse order; bulk owner release outside critical mutation path where current semantics permit. If a binding cannot be fully authenticated, fall back to generic original path before mutation, not an unverified fast acknowledgement. G1/G2 old directory, old revisions and close refund must match exactly. |
| **B. Physically safe owner/page publication** | Explore a per-page checked live owner with a reusable *released* free-slot pool or bounded preallocated arena, or reduce duplicate index rewrites by sharing unchanged edges. Current WRITE 4096 append creates pack/index 4096/16672; dispersed 4096/26392 plus 4095 hot directory; repeated 4096/4096. Upper benefit in page versions is only versions demonstrated to be redundant by affected-closure proof; a required page still incurs 4 KiB write **and** immediate authenticated 4 KiB readback. The write/readback times 1.750/0.964 s append and 2.844/1.413 s dispersed are scoped Exec operands, not an achievable saving estimate. Repeated one pack+index per ack is a strong control; no claim of eliminating it. Commit pack counts unchanged except layout effects separately proved. | Fresh epoch/identity per reuse, actual blocks checked before charge transfer and after reuse, pre-reservation plus old/new versions, final-owner release including pins/readers/captured old bytes, unknown-outcome quarantine. Never overwrite a selected incarnation; reclaim only after last current and G1/G2 owner and checked unlink/refund. Fewer files may reduce create/unlink metadata syscalls; preallocation may *increase* quota and inode/extent metadata and must be included before ack, not setup credit. Must maintain exclusive O_DIRECT/ext4 guarantees and failure-injected short-write, stat mismatch, unlink failure custody. No fsync or new background worker. |
| **C. Order-local SaveFile representation / pack layout** | Keep existing charged 1024-ref/32 KiB grouping as control. Candidate C1: order-preserving physical pack organization at WRITE time with a selected logical locator translation and bounded on-demand compaction; C2: in-Commit bounded multi-pass with explicitly charged external runs only if they fit Budget/quota and deadline. With unchanged layout, A/B leave Commit loads append 2/7/52, dispersed 2/7/209, repeated 1/1/1. C would aim dispersed loads closer to P=2/7/52 for this *particular* schedule, but no arbitrary-order global O(P) bound follows; extra WRITE/repack I/O may dominate Exec and violate one-ack pin safety. Best possible source-fill-only reduction at dispersed 4097 is 72 ms/10.666 s wall. Locator seeks/index reads may fall from 209/627 toward 52/~156 only *if* identity-correct grouping becomes possible; decoded records from 16468 toward ~4097 only in the ideal single-decode schedule. Verify page versions, bytes, peak RAM, quota, and total command rather than selecting by loads. | A new order index may require an O(E_f) ordered descriptor and/or O(N) out-of-order bytes, so an admissible C must expose its precharge, output order, pass count, page allocation, G1/G2 pins and migration. A physical-order rewrite after WRITE ack is prohibited; move any repacking inside the ack or Commit with charged resource/time. Any new pack/index format must first complete §6. Existing SaveFile framing, canonical C1/C2 identity and single worker cannot change as an optimization shortcut. |

Observed *within-Exec* immutable page versions at the last sampled WRITE;
`directory` is a subset of `index`, **not** an additional allocation. The
requested direct write bytes equal `4096*(pack+index)` and immediate readback
requests the same number of bytes. These are not device-traffic or retained
allocation measurements. Proposed A/B must explain each removed page; C
must count additional WRITE-side pack/reindex pages if it reorders layout.

| Pattern | W sampled | Pack / index / directory versions | Write B = readback B | Current Commit loads / locator index reads (full W+1=4097 last tier) |
| --- | ---: | ---: | ---: | ---: |
| Append | 100 / 512 / 4096 | 100/255/99 ; 512/1940/511 ; 4096/16672/4095 | 1,454,080 / 10,043,392 / 85,065,728 | 2/4 ; 7/14 ; 52/156 |
| Dispersed | 100 / 512 / 4096 | 100/373/99 ; 512/2827/511 ; 4096/26392/4095 | 1,937,408 / 13,676,544 / 124,878,848 | 2/4 ; 7/14 ; 209/627 |
| Repeated | 100 / 512 / 4096 | 100/100/0 ; 512/512/0 ; 4096/4096/0 | 819,200 / 4,194,304 / 33,554,432 | 1/1 ; 1/1 ; 1/1 |

A+B is the **leading research pair**, not a selected patch: it targets
observed WRITE cost, while C is conditional on a *total wall* advantage after
paying for extra WRITE I/O. First falsifier for A is proof that every
normalization at dispersed 4096 removes an actually changed or unavoidably
ineligible leaf/ancestor under 64 slots; first falsifier for B is that all
extra page versions equal changed closure plus unavoidable pinned copies.
If both fail, report that current costs are explained rather than forcing a
format rewrite. A wider LRU/window by itself is not a fourth architecture.

**Adversarial comparison required before choosing:** around K=1024 and B=32768,
try N=K-1/K/K+1 (and B-1/B/B+1 required bytes), alternating P>K packs,
reverse and interleaved pack order, one huge Payload adjacent to Packed/Zero/
Base, split of a 128-entry extent scan; record `L`, decoded records, seeks,
bytes, and refusal with *no* uncharged spool. Across WRITE try exact leaf/
branch carry and height changes, 7/8/9 live cursor inodes, 63/64/65
reachable hot slots, many-file shared tails/locators and R/L inverse
references, chosen G1 pin release orders with successor G2 writes and held
handles, quota 4096 bytes below/at page reservation and Budget just below/
at old+new capacity overlap. Mark these **diagnostics**, not replacement
registered selections. Do not use them to tune the nine result walls.

## 6. Compatibility boundary before considering B/C format changes

Current new attachments use `LFSAIDX2` and `LFSAHOT2` v2 index/directory;
pack pages retain `LFSAPAK1` v1 and original key/extent meanings. The
following is a *design checklist*, **not** a prospective v3 grammar or
approval. Before implementing an arena/free-slot or ordered pack format,
write a separate versioned specification with: exact page header/magic,
slot and epoch/address/locator serialization, maximum page and payload size,
checksum/readback and file-identity rules; old v1 pack/v2 index reader
selection by explicit version; exact handling of old pinned bytes and
pre-existing attachment (likely reject/recreate rather than silently
migrate); and proof that older binaries never reinterpret new bytes as old
ones. Define whether the reader can resolve both kinds within one selected
G1 view; do not guess from an on-disk filename. If an old reader cannot
understand v3, declare the mixed-binary/downgrade boundary explicitly and
avoid selecting it until all old pinned views drain (without pausing their
processes). No change to C1/C2, Bridge SaveFile wire semantics or canonical
root/identity is implied. Require exact physical owner/epoch transitions:
`free after final current/reader/G1/G2 pin -> checked unlink or verified
reserved slot -> increment epoch with overflow refusal -> reserve actual
physical blocks and Budget for old+candidate -> write -> readback/auth ->
atomic select -> refund only after last owner`. A syscall with unknown outcome
remains quarantined and charged; no unconditional retry or speculative
reclaim. This spec and external migration/pin tests precede *any* format
implementation or performance receipt.

## 7. Prospective diagnosis and falsification programme

1. **No product edit first.** Derive page/seek oracle from already retained
   phase-local v4 samples; parse all nine raw results once and verify manifest.
   The current count ledger is the starting sample, not a new candidate arm.
   Use existing production cause counters (`PageStore::status` gives encode,
   identity/preallocation, direct write/readback/auth, release, fit/actual
   merge, cache decode, directory/index/pack writes, visits/admissions/carries)
   and `LFS_ACTIVE_SOURCE v=2`. Missing breakdown: distinct *changed vs
   normalization-only* page versions, hot occupancy/eviction reason, pin
   cohort transfers and time under writer lock; add real production telemetry
   only if the count cannot be inferred. Never add test-only `src/` hooks.
2. **Pre-register an oracle** per accepted WRITE and final reference:
   expected one immutable pack candidate per tiny ack; index/new directory
   versions <= actual changed closure + carry/split + authenticated
   admission/eviction closure (prove each term), seeks <= counted path
   queries times `H` and bounded cursor keys, retire visits <= new replaced
   owners plus explicitly released pin-cohort items. Append at WRITE
   100/512/4096 should reproduce pack 100/512/4096 and index
   255/1940/16672, dispersed pack 100/512/4096 and index
   373/2827/26392 (including directory 99/511/4095); repeated pack/index
   100/512/4096. These are **observed starting operands**, not ceilings for
   a changed product. For Commit require append N=100/512/4097,
   L=2/7/52; dispersed N=100/512/4097, L=2/7/209;
   repeated N=1/1/1, L=1/1/1. Locator index reads per pack load
   are 2/2/3 for dispersed 100/512/4097 (4/14/627 total), tracking
   observed selected height, not a universal fixed path. Explicitly count D, H changes, C, U, selected
   pinned owner copies, `A`, `Q_fetch`, actual quota, pre/post Commit
   physical blocks and Budget peak; failure/malformed/interleaved C1 or
   v4 snapshot is **INCOMPLETE**, not zero. Decode and locator line must be
   phase-local to each SaveFile. Predict counts **before** executing a
   changed identity, then compare deltas instead of post hoc fitted Big-O.
3. Test A with a small deterministic public Workspace/mounted sequence
   that alternates two offsets in the same inode and an unrelated inode,
   both with/without selected pins; determine which normalizations were
   caused by the removed same-inode cursor, obsolete slot, carry or byte cap.
   Test B by tracing unique physical page versions and readback/owner
   status across exact 100/512/4096 WRITEs and single pin release, without
   changing formats. Reject a hypothesis if its predicted avoidable versions
   are all required changed closure. Model C from the existing schedule
   permutation first; execute no changed performance arm to test a model.
4. Only after a winning, source-bound design: architecture document and
   public external regression in the *same algorithm/format commit*, exact
   first-parent/staged/committed production LOC via
   `python3 tools/production_loc.py --json --root <snapshot>` (Core,
   reference, combined); keep all tests outside product `src/`, <=999
   physical lines/source file and <=200/delegation-only lib/mod. Then run
   affected host release workspace/FUSE tests, all-target warning-denying
   Clippy/fmt, boundary and tools tests; exact aarch64 release Linux
   executable on an **owned ext4** volume with `TMPDIR=/work`,
   `LAYERFS_ACTIVE_TEST_ROOT=/work`, `--test-threads=1`; affected
   `stage_route.py` functional cases with `performance_claim=false`,
   `cache_claim=null`. Preserve raw stdout/stderr and all failures. Use
   one Cargo owner/target per worktree and one construction worker; never
   touch another owner's Docker resources.
5. A changed identity permits **one** prospective labelled count diagnostic
   per affected case and candidate arm (`checkpoint5_273.py run --diagnostic`,
   `admission_eligible=false`, verifier SKIPPED; wall includes instrumentation
   overhead), followed when stable by **one** independently verified attempt
   per registered nine-cell selection at a frozen identity. New `iter-NNN`
   directories only, with checksums and untouched historical receipts.
   Respect 15/25-second *complete* commands, no warm-phase credit,
   after-ack work, new worker or hidden setup. The runner has no generic
   `--setup clone`/`--perf-fast`: reuse protected closed masters through its
   actual prepare/copy mechanism and sealed immutable binaries/images.
   Preserve all three source identities (first archived #273, diagnostic
   intermediate, current candidate) separately.
6. Numeric comparison stays **NOT_RUN** until a separate owned checkout of
   unchanged frozen #271 product gets only genuinely common harness/driver/
   observer changes with product seal proven unchanged. Then collect all
   12 selections row-major control->candidate under common cache and
   resource capabilities. Without private cache/phase-local cgroup evidence
   the rows remain INELIGIBLE. Obtain an owner-supported live/pinned SDK
   capability or ruling for the two controls and explicit C1-zero provenance
   for #248; do not synthesize receipts from detached Store, Workspace-only
   functional proofs or missing telemetry. Keep PR draft; #273 stays open.

**Stop/revise decision:** Only when phase-local count terms at all 3x3 tiers
and adversarial boundaries are explained by input/output, selected height,
changed closure, explicit carries/evictions/pins and canonical construction,
with exact quota/Budget custody and a continuing G2 process, may we state
"no *known avoidable* factor in covered operations." If a factor or external
capability remains unresolved, retain NOT_RUN/INCOMPLETE/INELIGIBLE and
report the achieved scoped improvement, not universal 2x, 10x,
constant-RAM or release eligibility.

## 8. Subsequent experimental decision (product telemetry `6ea57701b`, extended test `91aca43f5`)

The prospective §§1–7 above were written before either change; their sample
identity remains iter-012. This section records **new count experiments, not
new registered performance arms**, from [iteration 013](../../../../benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-013/RESULTS.json)
and its [raw checksum manifest](../../../../benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-013/SHA256SUMS).
The production `representation_only_pages` counter counts index pages emitted
while a recursive generic index mutation has **no key updates** in that
subtree. It is a subset of index page writes, includes failed staging, does
not count a directory page or changed-key path, and is **not** a removable-page
counter. Public stage runs are functional/count diagnostics with
`performance_claim=false` and `cache_claim=null`; their instrumented walls
are not numeric speed comparisons.

| Public stage route | Accepted WRITEs | Seeks / WRITE | Index pages / WRITE | Representation-only / index pages | Other evidence |
| --- | ---: | ---: | ---: | ---: | --- |
| dispersed generic 100 | 100 | 15.19 | 3.90 | 0 / 390 | Full bytes, Commit and clean close PASS |
| dispersed generic 512 | 512 | 15.04 | 5.60 | 0 / 2,866 | Full bytes, Commit and clean close PASS |
| dispersed generic 4,097 | 4,097 | 14.97 | 6.78 | 810 / 27,780 (2.9%) | Full bytes, Commit and clean close PASS |
| **extended diagnostic** dispersed generic 8,192 | 8,192 | 14.00 | 7.58 | 3,154 / 62,117 (5.1%) | **FAIL** after accepted WRITEs: canonical Commit observed, local C5 reconciliation `Capacity`; final byte/clean-close checks **NOT_RUN** |
| same inode, 48 overwrites at one offset then 48 alternating offsets (following 32 eligible appends) | 48 + 48 | 438/48 and 439/48 | 144/48 and 144/48 | 0 / 144 in each | Full private/committed bytes and clean close PASS; 48 directory + 48 pack versions in each group |

The extended tier is an explicitly *nonregistered* diagnostic, not a replacement
for 4,097 or a passing workload. `8192` uses distinct permuted positions in
an 8,194-byte file; `gcd(2654435761,8194)=1`. Its WRITE counts end before the
failing Commit. This failure is **not** attributed definitively to a specific
allocation: `commit/active_reconcile.rs::prepare` constructs O(E_f) selected
extent/deletion lists, its charged update map coexists with source/selected
state, and `publish_reconcile_map` precharges scratch/compaction on the default
8 MiB Budget. The precise failing reservation was not instrumented. Neither
repeat the same diagnostic to improve its status nor label its locally failed
Commit a complete byte/space proof. It exposes a separate C5 memory-headroom
question above the registered tier, not an observed quadratic public WRITE.
Raw stderr, the known-local failure and inspected/removed **owned** container
and volume are retained in iter-013; removal is not a clean Workspace refund.

### Source-bound WRITE result and revised choice

For these one-byte tiny public WRITE schedules (with bounded per-WRITE changed
coverage), `ExtentPlan::replace` uses `floor` and a bounded range scan of
*affected* extents, not the file's historic W. At a one-byte point it reaches
at most the overlapping extent(s) and boundary pieces. `reclaim::live_refs`
uses bounded-prefix patch queries; the selected inverse scan sees actual
references of the touched pack/payload, not unrelated packs. In
`generation.rs::write_with`, `compaction::plan(..., max_sources=1)` **cannot
enter** its full-P pressure scan (`pressure` requires `max_sources > 1`). A
logical pack has a fixed 4 KiB body; tiny records are <=128 bytes and a
one-byte pack has <=80 records. The generic index copies <=8 cursors, checks
<=64 hot slots and only reached paths plus their normalized ancestors; the
index height is capped by `MAX_LEVEL=7`. Its branch/leaf scans operate on
fixed 4 KiB pages. `PageStore` page/owner registry and the frozen-pin map use
ordered lookups (`O(log A)` and `O(log(G+1))`); ordinary retirement checks
newly replaced owners, and a final pin release separately visits its actual
cohort. At most 32 captures and 160 pinned revision keys are admitted.

Consequently the *source-level count* for these accepted point WRITEs is
`O(W * (log(A+1) + H*(affected_key_paths + U) + page_bytes*new_pages))`,
plus explicit affected inverse references, bounded pack/slot work, input,
notifier and owner syscalls. Here `H<=7`, `U<=64` hot slots per mutation,
fixed 4 KiB pages and constant-size one-byte overlap keep the non-registry
terms bounded per WRITE **unless** pressure/refusal or a separately charged
pin-release event changes the operation. This is not an assertion of
constant elapsed time, universal O(1) WRITE across arbitrary-length writes,
constant-RAM Commit, or an unconditional bound for payload/namespace paths.
Nor does it establish that every index page is necessary. The count evidence
is consistent with the bound: seeks per accepted dispersed WRITE do **not**
rise from 100 to 8,192 (15.19, 15.04, 14.97, 14.00), while index page versions
per WRITE and representation-only versions rise. Avoidable local churn is a
*bounded per-WRITE amplification*, not the claimed historical-W sweep. No
unbounded quadratic WRITE factor was found in these covered schedules.

**Architecture direction:** retain the v2 authenticated changed-closure index,
immutable checked acknowledgement, fixed hot caps and generic ordered path.
Do *not* implement a wider LRU, unproved preallocated/free-slot v3, or Commit
pack rewrite to "eliminate quadratic WRITE". Design A remains a narrow
**conditional** follow-up: validate every extra no-key subtree/old cursor
against actual affected closure, slots, fences, epochs and pins; only skip a
representation rewrite when its old selected target can provably remain
referenced, with identical future hot eligibility and old-G1 bytes. Even
eliminating **all** 810 representation-only pages at 4,097 would affect only
2.9% of index page versions, and would not remove one mandatory pack version
per ACK or the required page I/O. For latency architecture B should compare
measured direct write/readback (2.796/1.441 s in the separate 4,097 stage
profile) against create/preallocation/release and *first* prove a checked
compatible v2/v3 owner grammar; no slot reuse or saved-wall claim is selected.
The higher-priority newly exposed issue for an extended scope is C5's
charged O(E_f) reconciliation capacity at 8,192, **not** quadratic WRITE.
There is no permission to change the registered workload/Budget or to call
iter-013 a new numeric comparison. Iter-012 nine cells retain INELIGIBLE,
#248 C1-zero remains INCOMPLETE and SDK pinned controls NOT_RUN.
