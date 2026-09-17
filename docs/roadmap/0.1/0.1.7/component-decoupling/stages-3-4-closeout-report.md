# Stages 3-4 closeout report (post-review)

> **Status: in progress.** Target LayerFS v0.1.7; not a released contract. This
> document is the tracking table and the evidence index for the work packets in
> `stages-3-4-closeout-prompt.md` §3. It closes nothing until every row of the
> gate table below is PASS with a named artifact or owner-waived.

**Start identity.** Commit `91c3a0741fff64e8161d5c1b6e759f347ffbf757`, tree
`4ef6f30f78d6a96f9fc0abee0f9abbf6a86a5ae9` — the pinned snapshot the independent
review read, and an ancestor of every commit below. Working tree at start: the
three untracked review artifacts
(`stages-3-4-review-20260916T233008Z.md`, `stages-3-4-closeout-prompt.md`,
`evidence/stages-3-4-review-20260916T233008Z/`) which this work commits
unmodified.

**Evidence round.** `docs/roadmap/0.1/0.1.7/evidence/stages-3-4-closeout-20260916T235641Z/`.
Every earlier generation, including the review round, is untouched. The round
carries `record.sh`, which appends each command, its exit code and its raw
combined output to an append-only log.

**Artifact note.** The review's four `loc-*.csv` tables were generated with CRLF
line endings. They are committed here with LF so that `git diff --check` is clean;
every number, every row and every column is otherwise byte-identical, and no
Markdown or log artifact of the review was edited.

**Toolchain.** `cargo 1.85.1 (d73d2caf9 2024-12-31)`, `rustc 1.85.1 (4eb2518d3
2025-03-15)`, `LAYERFS_CONSTRUCTION_WORKERS=1` on every test invocation.
`tools/preflight.sh` was not run and no aggregate wrapper was substituted.

---

## 1. Escalations

### E1 — B1 (owner decision required): the repeated-sample measurement campaign

```text
BLOCKER: B1 owner decision
Packet: W8
Exact question or condition: may the matched v0.1.6-versus-candidate C1 campaign run
  more than one sample per case per arm (the verification contract allows one sample
  per case unless an owner-approved campaign says otherwise), and if so how many?
Evidence: docs/roadmap/0.1/0.1.7/component-decoupling/stages-3-4-verification.md and
  stages-3-4-measurement-addendum.md (n = 1 rule); the review's §6.5.
What I did instead while waiting: W1-W7, W9 and W10.
What remains blocked: G13 and G15 (and therefore the Stage 3 "qualified" verdict). G14
  and G16 were completed while waiting and are PASS.
Next action on unblock: commit the versioned campaign addendum (cases, seeds,
  identities, cache state, sample count), then collect the matched arms.
```

### E2 — B1 (owner decision required): the two documented format/design deviations

```text
BLOCKER: B1 owner decision
Packet: W6.3, W6.4
Exact question or condition: (a) may pooled leaf records stay in the v1 Ordinary lane
  with the objects.object_role column distinguishing them, with the deviation recorded
  in physical-encoding-and-packing.md, or must they move to v6? (b) may v5 remain
  rejected and be recorded in the design document as a scope decision, given that the
  candidate's schema identity is deliberately not the reference's, so v5 has no Store
  to act on in this batch?
Evidence: docs/roadmap/0.1/0.1.7/component-decoupling/physical-encoding-and-packing.md
  lines 270-284; the review's §2.10.
What I did instead while waiting: every other packet.
What remains blocked: nothing else - both rows are documentation rows and are
  recorded as "current state + deviation recorded" until the owner answers.
Next action on unblock: apply the owner's choice and update physical_formats.
```

---

## 2. Gate table

| # | Gate | Source | Status | Evidence |
| --- | --- | --- | --- | --- |
| G1 | CHUNK absent/ineligible candidate selects FULL; save never fails for an advisory candidate | W1 | **PASS** | `w1/README.md`; `delta_payload` +3 cases, all three fail with `ObjectMissing` without the fix |
| G2 | Every pooled read applies the owner's captured pack ceiling | W2 | **PASS** | `w2/README.md`; four sites supply `self.ceiling`, structural + behavioural oracles both fail without it |
| G3 | Emitted edit pages are encoded and hashed once, at final emission; superseded drafts released | W3 | **PASS** | `w3/README.md`; `Draft::Page`/`Draft::Node`, encode+hash only in `commit_node`, release on supersession + `tree::discard` |
| G4 | Frontier memory is a function of height/fanout, asserted by a real oracle | W3, W4.1 | **PASS** | `w3/README.md`; peaks `[2208,2288,2448,2768,3408]` at 1/2/4/8/16 edits; control without release `[6308,…,129728]` |
| G5 | Finality argument committed; children-before-parents asserted for edits | W3, W4.6 | **PASS** | finality argument R1-R3 in `w3/README.md`; `assert_children_precede_parents` called from `edit_bounds` |
| G6 | All nine sealed reference cases still match root, partition and survivors | W3 | **PASS** | `w3/w3-verify.log`: `edit_reference` 2 tests / nine sealed cases, exit 0 |
| G7 | Every overclaiming or vacuous oracle replaced; each new case fails without its fix | W4 | **PASS** | `w4/README.md`; W4.1-W4.8 table, two measured controls, the rest source-derived and labelled |
| G8 | Integrated multi-edit chunked pipeline case exists and passes | W4.5 | **PASS** | `edit_pipeline::a_multi_edit_chunked_stream_round_trips_in_current_result_coordinates`, store-backed logical readback |
| G9 | Every significant allocation has owner, bound, multiplicity, lifetime and release event | W5, W7.3 | **PASS** | ledger in `w7/README.md` (15 owners, no unowned owner) and `w5/README.md` |
| G10 | No size-proportional hidden collector remains on a real path | W5 | **PASS** | `w5/README.md`; catalogue streamed, placement measured not copied, every level flushed, both pack caches bounded, cleanup transactions bounded |
| G11 | Phase-local heap ledger and a labelled RSS scope exist | W7 | **PASS** | `examples/memory_ledger.rs` + `w7/w7-verify.log`: 6 measured phases, sampled RSS at boundaries, lifetime RSS labelled |
| G12 | #168's simultaneous index/codec/SQL memory gate has an input and a result | W7 | **PASS** | `w7/README.md`: pooled save at the E1b shape peaks at 3.45 MiB heap with index 57 600 B + 3 MiB codec workspaces + SQLite journal/BLOBs live |
| G13 | Matched reference campaign collected under a pre-committed addendum with aligned byte accounting | W8 | OPEN (B1) | |
| G14 | Every declared verification case is RUN or NOT_RUN with a reason | W8.4 | **PASS** | `stages-3-4-verification.md` §2.1: eleven declared cases mapped to the product case that executes them, two `NOT_RUN` with reasons, plus the new real `store-bytes` row (`w8/README.md`, `w8/w8-verify.log`) |
| G15 | "Existing-or-better" for latency/storage/memory is resolved, or waived in writing | W8.6 | OPEN (B1) | |
| G16 | The clipped `e1c` receipt is re-collected or annotated wherever quoted | W8.7 | **PASS** | `w8/README.md`: the receipt is untouched, and the disclosure was added to the ledger (appended), `stages-3-4-verification.md` §4 and the final review's quoting table; re-collection is declined with the reason |
| G17 | Acceptance documents match the source; per-file actual-size table exists | W9 | **PASS** | `w9/README.md`; report §1/§7 corrected in place, §8 carries the plan's table, oracle paperwork fixed |
| G18 | Counter defects fixed, same counter applied to both snapshots, combined total restated | W9.5 | **PASS** | `w9/README.md`: 956 over-removed lines and 4 083 test-module lines measured; combined 76 400; both snapshots counted by the corrected counter |
| G19 | Limits boundary list run or explicitly unrun with reasons | W10 | **PASS** | `w10/README.md`; 13 boundaries, 12 run through public APIs with raw stdout in `w10/w10-verify.log`, the 8 MiB - 1 deferred refusal UNRUN with its source proof (408 MiB base floor) and its premise measured |
| G20 | Every commit carries a first-parent `Production LOC:` line and this review's S1/S2 findings are closed | all | **PASS** | nine commits from `97414bac4` to `c56c28dd1` each carry exactly one first-parent `Production LOC:` line (checked with `git log --format=%B`); findings closure table in §2.1 |

**Stage verdicts.** Stage 3 needs G1, G2, G9-G14, G17-G19. Stage 4 needs G3-G8,
G13-G15, G17-G19. G13 and G15 are the only rows still open, both on E1 (the owner's
campaign decision), so neither stage may be called complete yet.

### 2.1 Review findings closure (F1-F10)

`stages-3-4-review-20260916T233008Z.md` grades its findings S1 (blocks an issue's
own acceptance), S2 (real defect or real missing work) and S3 (documentation or
latent trap); G20 asks for the S1/S2 ones to be closed. Every finding, its packet
and where its proof lives:

| Finding | Severity | Packet | Status and evidence |
| --- | --- | --- | --- |
| F1 CHUNK delta candidates skip the eligibility check | S1 | W1 | Closed: `select.rs` probe; three `delta_payload` cases, all three fail without the fix (`w1/README.md`, `w1/w1-fails-without-fix.log`) |
| F1b the pooled lane reads above the publication watermark | S2 | W2 | Closed: four pooled read sites supply `self.ceiling`; structural and behavioural oracles, both fail without it (`w2/README.md`) |
| F1c cold-start index replay materialises the whole catalogue | S2 | W5.1 | Closed: `pool::for_each_group` streams it and the recurrence keeps three scalars (`w5/README.md`) |
| F2 checkpoint D's decoded frontier not implemented | S1 | W3, W10.1 | Closed: `Draft::Page`/`Draft::Node`, one encode/hash per node at final emission, release on supersession, plus the refcounted cascade W10.1 added after the ceiling case found the deeper leak (`w3/README.md`, `w10/README.md` §1) |
| F4 oracles that do not test what they claim | S2 | W4 | Closed: per-case table of what each replaced oracle now asserts, two measured controls (`w4/README.md`, `w4/w4-fails-without-fix.log`) |
| F5 acceptance documents contradict the code | S3 | W9 | Closed: report §1/§7 corrected in place with dates, §8 per-file table, oracle headers fixed (`w9/README.md`) |
| F6 a full canonical clone per object on the prepared-save path | S2 | W5.2 | Closed: `MutationOwner::offer` takes `&FinalizedObject` (`w5/README.md`) |
| F7 pack placement deep-clones the open pack per fit probe | S2 | W5.3 | Closed: `layout::append_fits` probes in place; `pack_locator` fit-probe equivalence (`w5/README.md`) |
| F8 unbounded owners the ledger does not carry | S2 | W5, W7.3 | Closed: fifteen owners with bound, multiplicity, lifetime and release event; no unowned owner remains (`w5/README.md`, `w7/README.md`) |
| F9 latent traps in the extended code | S3 | W6 | Closed: dead code deleted, traps removed or made explicit, deviations recorded (`w6/README.md`) |
| F10 documented format/design deviations to confirm with the owner | S2 | W6.3, W6.4 | Addressed as far as the agent may go: both deviations are now recorded in the design document and the code, and the confirmation itself is the owner decision E2. No code change until it is answered (`w6/README.md`, `physical-encoding-and-packing.md`) |

Eleven findings: two S1 (F1, F2) and six S2 (F1b, F1c, F4, F6, F7, F8) closed with
a measured control run, a structural oracle, or the bounds and ledger work of W5 and
W7.3; one S2 (F10) resolved into the recorded owner decision E2, with no code change
until the owner answers; and two S3 documentation findings (F5, F9) closed.

---

## 3. Per-packet log

### W2 — the pooled lane honours the publication watermark (G2) — PASS

* **Defect.** `cas/owner.rs:459,544,658,685` passed `i64::MAX` as the read
  ceiling, so the pooled lane's stated watermark invariant rested only on the
  `retained_pack_ceiling != highest_pack_id` precondition in
  `MutationOwner::acquire` (`owner.rs:162-171`).
* **Change.** The four pooled read sites now supply the owner's own
  `self.ceiling` (which already includes the packs this save created);
  `encoding/pool/read.rs:70-84` decides the ceiling before consulting its
  decoded-value cache. `VisibilityCeiling` remains the refusal.
* **Proof.** `visibility.rs` gains two oracles: a behavioural refusal driven by a
  real save whose bounded early commit leaves a value-group pack above the
  watermark (`group_values` cold and cached, `PoolIndex::sync`, `PoolIndex::find`
  all refuse; all succeed at the published ceiling), and a structural one that
  reads the pooled lane's source regions and rejects an unbounded ceiling there.
  Both fail in `w2-fails-without-fix.log`. Re-ran `visibility` (7), `metadata_pool`
  (10), `metadata_chain` (2), `metadata_window` (2), `metadata_pool_index` (5),
  `edit_pipeline` (4) and clippy, all exit 0.
* **Reachability.** Unreachable through the public API today; the change is
  fail-closed, and `w2/README.md` states that plainly.
* **Production LOC.** core `10938 -> 10938` (delta 0).

### W3 — checkpoint D's decoded frontier (G3-G6) — PASS

* **Defect.** `file/edit/tree.rs:52-58` held encoded drafts, encoded and hashed
  each draft at creation (`:105-139`), never released a superseded draft
  (`:112-132`), decoded again on every visit (`:82-101`) and re-decoded and
  re-encoded each reached node in `commit_node` (`:185`).
* **Change.** `Draft::Page(FinalizedObject)` holds a builder page as the finalized
  object it already is (final emission is a move); `Draft::Node(ExtentNode)` holds
  a node this operation built, decoded, under an operation-local key, and is
  encoded and hashed exactly once inside `commit_node` after the walk proves it
  final. `child_summaries` recovers children from descriptors, and `commit_node`
  patches each descriptor with the child's real identity before encoding the
  parent, so children precede parents. `EditObjects::release` drops a node a
  `split`/`concat` supersedes; `tree::discard` drops the replaced range the edit
  does not use. `EditCounters::nodes_created` now counts published nodes and
  `peak_deferred_bytes` the peak live charge. `ConstructedFile` gained
  `counters: EditCounters` so the real C1 path reports frontier work.
* **Proof.** `edit_bounds.rs::the_retained_frontier_does_not_grow_with_the_edit_count`
  replaces the vacuous frontier oracle (W4.1) and asserts per case:
  `payloads_created == edits`, `nodes_created ==` emitted mapping objects,
  a changed mapping, child-first emission (W4.6) and a frontier that does not
  double with the edit count. `w3-fails-without-fix.log` holds both controls.
  `edit_reference` reproduces all nine sealed cases (exit 0, 56.6 s), plus
  `edit_localized`, `edit_batch`, `edit_model` and the whole workspace
  (43 targets / 251 tests).
* **Finality.** Rules R1-R3 and the join/edit arguments are in `w3/README.md`.
* **Production LOC.** core `10938 -> 11053` (delta +115); C1 4385 -> 4500.

### W4 — oracles repaired, unexercised cases closed (G7, G8) — PASS

* **Change.** Test-only: `edit_bounds` (W4.1 replacement lives in W3, plus the new
  bounded sink), `policy_capacity` (depth 4 versus depth 50), `object_identity`
  (envelope ±1 and the `FinalizedObject::new` refusal), `streaming` (source and
  per-file assertions), `edit_pipeline` (renamed single-edit case plus a real
  multi-edit store-backed case), `metadata_pool` (honest group case, compressed
  group, damaged delta, chain budget on both sides), `metadata_pool_index`
  (invalidation oracle that can actually reach the phantom ordinals),
  `delta_chains` (role check reached with a real stored object of another role),
  and `support` (advisory-preserving consumer, store-backed logical reader,
  byte-counting source). `assert_children_precede_parents` is now called from an
  `edit_*` target (W4.6).
* **Proof.** `w4-verify.log`: 18 focused targets plus the workspace suite
  (43 targets, 258 tests, 0 failed), clippy, fmt, product boundary, both tool
  suites and `git diff --check`, all exit 0 (one clippy lint in a new test is
  retained with its re-run). `w4-fails-without-fix.log` holds two measured
  controls; the rest are labelled source-derived in `w4/README.md`.
* **Production LOC.** core `11053 -> 11053` (delta 0; test-only).

### W5 — bounded-owner fixes (G9 input, G10) — PASS

* **W5.1** `index.rs::retained_start` no longer materialises the catalogue:
  `sqlite/pool.rs::for_each_group` streams it (the reference's shape) and the
  recurrence keeps three scalars.
* **W5.2** `MutationOwner::offer` takes `&FinalizedObject`; the per-object
  canonical clone in `cas/save.rs` is gone.
* **W5.3** `layout::append_fits` replaces the open-pack deep clone and uses
  `lane.group_count_limit()`/`lane.pack_limit()`; the dead `fits` predicate is
  deleted. New oracle: `pack_locator` fit-probe equivalence.
* **W5.4** `flush_streaming` cascades through every level under the same rule;
  `MappingBuild` reports `peak_pending` and `streaming` asserts the height bound.
* **W5.5** Both pack caches are bounded by `DEPENDENCY_PACK_CACHE_BYTES` (4 MiB)
  with a wholesale release; the stale "inside this wave" comment is corrected.
* **W5.6** Cleanup keeps its single attempt and newest-first order but commits and
  reopens at the writer's transaction bounds.
* **W5.7** The dead 32 MiB index budget is gone with the real bound stated where
  it is reported; `Candidates::live_bytes` returns declared bytes;
  `SaveOutcome.chain` is the save total (new oracle, with a control run) and
  `ReadCounters::packs_read` is counted where a body is fetched.
* **W5.8** The pooled reader caches the covering catalogue row and copies single
  values (`PoolReader::group_value`) instead of one query and one group clone per
  row.
* **Evidence.** `w5/w5-verify.log` (22 targets + workspace 43/261 + clippy, fmt,
  boundary, tools, `git diff --check`, all exit 0) and `w5/w5-fails-without-fix.log`.
  The owner ledger with bound, multiplicity, lifetime and release is in
  `w5/README.md`; W7.3 extends it with the allocations reported live.
* **Production LOC.** core `11053 -> 11166` (delta +113); C1 +8, C2 +105.

### W6 — latent traps, dead code and design deviations — PASS (E2 open)

* **W6.1** `pool_base` charges both chain budgets what a read actually pays, so an
  accepted metadata depth is usable, including 50; `DepthCache::cost_of`'s walk
  bound was one record short of the deepest supported chain. Oracle:
  `metadata_pool::a_deep_metadata_depth_admits_and_reads_a_fifty_link_chain`, with
  two controls in `w6-fails-without-fix.log`.
* **W6.2** the dead `InodeLeaf` dispatch is an explicit refusal,
  `delta_depth_for_role` maps a pooled leaf to the pooled depth, the 23-byte
  constants are renamed for what they measure, and 16 dead items are deleted
  (including `FileView::walk_extents`, 59 lines).
* **W6.3 / W6.4** both deviations are recorded in
  `physical-encoding-and-packing.md` and pinned by
  `physical_formats::the_pooled_lane_assignment_and_the_v5_scope_are_the_shipped_ones`;
  the owner decision is escalation E2.
* **W6.5** the ranked simplification list is worked; the one item kept (the
  bounded demand scan in `mapping/read.rs`) is kept with its reason stated.
* **Evidence.** `w6/w6-verify.log` — 26 targets, workspace 43/263, clippy, fmt,
  boundary, both tool suites and `git diff --check`, all exit 0; controls in
  `w6/w6-fails-without-fix.log`.
* **Production LOC.** core `11166 -> 10983` (delta -183); C1 -120, C2 -63.

### W7 — memory instrumentation and the allocation ledger (G9, G11, G12) — PASS

* **W7.1** `core/crates/layerfs-storage/examples/memory_ledger.rs` installs a
  counting `#[global_allocator]` in an external target and reports, per real
  product phase, current bytes, phase-local peak, allocation count and charged
  bytes. Product source gains no hook.
* **W7.2** RSS is sampled with `ps -o rss=` **at phase boundaries only**, with the
  window and coverage stated in the receipt; the lifetime high-water is reported
  from `/usr/bin/time -l` on the example binary (16 777 216 B) and labelled as a
  lifetime number; the in-process `ru_maxrss` row is `null` with its reason.
* **W7.3** The ledger in `w7/README.md` covers the COW frontier, the immutable
  bases, the FULL/PREFIX alternatives, all five pack lanes, the SQLite MEMORY
  journal and BLOB copies, the ordered set, both pack caches, the value cache, the
  candidate index, the depth cache, both codec workspaces, the builder levels and
  the telemetry report — each with owner, bound, live multiplicity, overlap,
  lifetime and release event. No unowned owner remains.
* **W7.4** The receipt states what the connection profile sets and what it does
  not (`cache_size`/`mmap_size` unset, engine maxima the host library's,
  connection count unqualified); `content-io-memory-audit.md` now carries the
  candidate profile beside the reference rows it audits.
* **G12's result.** At the E1b shape (24 leaves × 100 rows = 2 400 values) the
  pooled save's phase-local heap peak is 3.45 MiB with the ordered set (57 600 B),
  the codec workspaces (3 MiB) and SQLite work live; the phase charges 81.5 MiB
  across 9 977 allocations, so the pooled lane spends allocation rate rather than
  residency. Nulls and coverage are tabulated.
* **Evidence.** `w7/w7-verify.log`: the example run (exit 0), the same binary under
  `/usr/bin/time -l` with its sha256, 14 focused targets, the workspace suite
  (43/263), clippy, fmt, boundary, tools and `git diff --check` — all exit 0.
* **Production LOC.** core `10983 -> 10983` (delta 0; example and docs only).

### W9 — documents, counter and the plan's actual-size table (G17, G18) — PASS

* **W9.1** The report's §1 no longer says pooling is unimplemented or the frontier
  partial; its LOC block and per-target table (including `timing` C1 = 7) match the
  code at the closeout commit, and each correction is dated and names what it
  replaced.
* **W9.2** The transition paragraph states the real residual gap (read
  amplification unmeasured) instead of presenting the handoff-prescribed dispatch
  as a rebuild defect.
* **W9.3** `edit_reference.rs`'s header and the oracle README's `git diff` claim are
  corrected, with the date and the reason.
* **W9.4** The file plan's required per-file table now lives in the report §8, with
  directory totals, disjoint package totals and the files that are not plan rows.
* **W9.5** Both counter defects fixed with a focused tool test each; the same
  corrected counter is applied to `c38961f2f` (core 6 152) and to the current tree
  (core 10 983, reference 65 417, combined **76 400**). The over-removal is 956
  lines and the test-module inflation 4 083 lines, both measured.
* **W9.6** `core/README.md`, the roadmap index, `implementation-plan.md`,
  `implementation-issues.md` and the experiment ledger (L33) carry the closeout.
* **Evidence.** `w9/w9-verify.log` — 11 commands, all exit 0.

### W8 — the campaign packet, completed except the owner-gated campaign (G14, G16) — PASS; G13/G15 OPEN (E1)

* **W8.4 (G14)** `stages-3-4-verification.md` §2.1 now maps every case the frozen
  contract declares to the product case that runs it: eleven RUN, and two `NOT_RUN`
  with reasons - the matched campaign itself (E1 open, so no addendum and no arm may
  exist) and read amplification on the representation transition, which has no case
  and is the residual gap the acceptance report names. The missing `store-bytes` row
  is now real: a 1 048 583-byte fixture saved through a real Store retains 60 objects
  (1 052 271 B canonical) in six pack bodies of 1 052 818 B, largest 259 777 B inside
  the 262 144-byte ordinary pack limit, inside one 1 204 224-byte database file and no
  other file; the row reports pack bodies apart from the database and never presents a
  database delta as write I/O.
* **W8.7 (G16)** The clipped `e1c-pooled-512` receipt is disclosed everywhere it is
  quoted - an appended ledger section, a dated paragraph in
  `stages-3-4-verification.md` §4, and a dated correction above the table in
  `stages-3-4-final-review.md`. The receipt, its tree and its `stdout.log` are
  untouched and nothing is re-labelled. Re-collection is declined with the reason
  recorded: the arm is a wiring demonstration whose cited counts are complete, and a
  second n=1 run could not change what it may be claimed for.
* **Blocked.** W8.1/W8.2/W8.3/W8.5/W8.6/W8.8 and rows G13/G15 need E1's answer; the
  order of work on a yes is stated in `w8/README.md`.
* **Evidence.** `w8/w8-verify.log` (two recorded command groups, both exit 0, raw
  `MEASURED store-bytes` line on disk) and `w8/README.md`.
* **Production LOC.** unchanged at `11058` (test and documentation only, delta 0).

### W10 — limits boundary coverage (G19) — PASS, one boundary UNRUN

* **W10.1 (production fix).** The 4 096-edit ceiling case exposed a leak the W3
  frontier case could not: pages created inside `concat_inner`'s `split` recursion
  became unreachable when the next level replaced their parent (286 leaked level-1
  pages at 4 096 edits). `tree.rs` now counts the parents that reference each draft
  (`parent_refs`) and the drafts no live draft references (`detached`), so releasing
  a superseded node cascades to children no surviving draft names, and `apply.rs`
  calls `objects.settle(mapping)` after every edit. This completes G3's release
  rule; both measured peaks (`353 952` B / 79 pages and `384 112` B / 122 pages at
  the edit ceiling) and the byte-equality readback are in `w10/README.md` §1.
* **W10.2** Twelve boundaries run: CHUNK depth at a changed cap (2 vs 3) and at 50,
  the edit ceiling at 4 095/4 096/4 097 with the refusal in `EditStream::new`
  before any mutation, the 8 MiB field and 16 MiB envelope at plus or minus one, the
  pooled window filled to exactly 131 072 distinct entries plus the wholesale reset
  and the cold-start replay over 131 200 real ordinals, the read wave (measured
  batch = 32 objects; the byte window is the derived identity, labelled as such),
  the C2 accept wave splitting by bytes, the `PoolReader` 512 KiB release
  (`peak_retained=518 300` B against a 1 460 000 B decode), a 24 MiB + 1 C1 read, a
  6 MiB + 1 file through a real Store (27 packs, twice read back), and the SQLite
  profile with its host-dependent engine values left unqualified.
* **W10.3** The 8 MiB - 1 deferred ceiling is **UNRUN**: the charge is live, each
  live draft charges at most `MAX_NODE_OBJECT_BYTES + 128 = 8 320` B, non-root pages
  hold at least 64 entries and an edit operation creates at most
  `length / 8 192 + 3 x 4 096` extents, so crossing the ceiling needs a base of
  `427 819 008` B (about 408 MiB) - outside the packet's fixture budget. The
  derivation's premise *is* run and measured (`3 148` B per draft on the largest
  in-budget shape); the refusal branch has no coverage and is reported as such.
* **W10.4** Directory, name, path and workspace-size dimensions are recorded as
  outside Stages 3-4 (Stages 5/7) with no invented value.
* **Evidence.** `w10/w10-verify.log` (15 blocks: rustfmt, four focused groups, the
  measured cases with `--nocapture`, the workspace suite 43/272, `edit_reference`
  56.9 s, clippy, product boundary, both tool suites, plus two of my own
  command-level mistakes kept on disk) and `w10/README.md`.
* **Production LOC.** core `10983 -> 11058` (delta +75); C1 `4388 -> 4463`,
  C2 `5863 -> 5863`, telemetry unchanged; reference `65417`, combined `76400 -> 76475`.

### W1 — CHUNK delta candidates obey the eligibility rule (G1) — PASS

* **Defect.** `core/crates/layerfs-storage/src/encoding/delta/select.rs:209-210`
  took `advisory.first()` for `ObjectRole::Chunk` with no eligibility probe, so an
  advisory predecessor with no committed row reached `acquire` and failed the save
  with `StorageError::ObjectMissing` where the whole-file lane selects FULL.
* **Change.** `select.rs:209-221` routes the CHUNK candidate through a new
  `probe` (`select.rs:300-315`) that applies the same `eligible` test the
  WHOLE_FILE lane uses and counts `ineligible_candidates`/`absent_candidates`;
  `acquisition` (`select.rs:317-331`) now shares that helper. The first-candidate
  rule is unchanged: CHUNK still considers exactly one candidate and never the
  admitted-FULL cache, and no retry over other candidates was added.
* **Proof.** Three new `delta_payload` cases (`tests/delta_payload.rs:280-430`),
  all through real stores and real C1 producers:
  `an_absent_chunk_candidate_selects_full` (FULL, `trials == 0`,
  `absent_candidates == 1`, readback byte-exact);
  `an_ineligible_chunk_candidate_selects_full` (present but wrong role,
  `ineligible_candidates >= 1`, FULL);
  `a_same_save_unsealed_chunk_predecessor_selects_full` (two adjacent 8 192-byte
  replacements in one real chunked C1 edit stream: the second replacement's
  right boundary is the first replacement's payload, which this save has not
  published, and `absent_candidates == 1`).
* **Fails without the fix.** Reverting only the CHUNK branch makes all three cases
  fail; the third fails with
  `ObjectMissing(ObjectId("7d362394..."))` at the save, exactly the reported
  defect. Raw stdout of the failing run is `w1/w1-fails-without-fix.log`.
* **Harness finding (test-only).** The review's source-derived trigger is only
  reachable if the C1-to-C2 handoff carries the advisory list, and the storage
  test consumer (`tests/support/mod.rs`) was silently dropping it: `Collected`
  now retains and re-emits `AdvisoryPredecessors`. This is test scaffolding, not
  product code.
* **Commands.** `w1/w1-verify.log` (9 commands, all exit 0): `delta_payload`
  (13 passed), `edit_pipeline` (4 passed), `check_product_boundary.py`,
  `core/tools` unit tests, `tools/production_loc.py` self-test, `fmt --check`,
  `clippy --all-targets -D warnings`, `production_loc.py --files`,
  `git diff --check`. Whole-command wall for `delta_payload`: 0.24 s
  (cached build), inside the 15 s budget.
* **Production LOC.** `core 10929 -> 10938 (delta +9)`; C1 4385 -> 4385,
  C2 5812 -> 5821, telemetry 732 -> 732. Reference 68476 unchanged (its total is
  not certified until W9.5; no combined total is quoted).
* **Does not prove.** That a CHUNK candidate is ever *chosen* as a PREFIX record
  by a real edit pipeline: the correctness of the eligibility decision is proven,
  the delta win rate of the edit path is not.
