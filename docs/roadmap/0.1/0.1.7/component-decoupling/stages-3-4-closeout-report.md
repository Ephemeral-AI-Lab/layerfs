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
What remains blocked: G13, G14, G15 (and therefore the Stage 3 "qualified" verdict).
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
| G2 | Every pooled read applies the owner's captured pack ceiling | W2 | OPEN | |
| G3 | Emitted edit pages are encoded and hashed once, at final emission; superseded drafts released | W3 | OPEN | |
| G4 | Frontier memory is a function of height/fanout, asserted by a real oracle | W3, W4.1 | OPEN | |
| G5 | Finality argument committed; children-before-parents asserted for edits | W3, W4.6 | OPEN | |
| G6 | All nine sealed reference cases still match root, partition and survivors | W3 | OPEN | |
| G7 | Every overclaiming or vacuous oracle replaced; each new case fails without its fix | W4 | OPEN | |
| G8 | Integrated multi-edit chunked pipeline case exists and passes | W4.5 | OPEN | |
| G9 | Every significant allocation has owner, bound, multiplicity, lifetime and release event | W5, W7.3 | OPEN | |
| G10 | No size-proportional hidden collector remains on a real path | W5 | OPEN | |
| G11 | Phase-local heap ledger and a labelled RSS scope exist | W7 | OPEN | |
| G12 | #168's simultaneous index/codec/SQL memory gate has an input and a result | W7 | OPEN | |
| G13 | Matched reference campaign collected under a pre-committed addendum with aligned byte accounting | W8 | OPEN (B1) | |
| G14 | Every declared verification case is RUN or NOT_RUN with a reason | W8.4 | OPEN | |
| G15 | "Existing-or-better" for latency/storage/memory is resolved, or waived in writing | W8.6 | OPEN (B1) | |
| G16 | The clipped `e1c` receipt is re-collected or annotated wherever quoted | W8.7 | OPEN | |
| G17 | Acceptance documents match the source; per-file actual-size table exists | W9 | OPEN | |
| G18 | Counter defects fixed, same counter applied to both snapshots, combined total restated | W9.5 | OPEN | |
| G19 | Limits boundary list run or explicitly unrun with reasons | W10 | OPEN | |
| G20 | Every commit carries a first-parent `Production LOC:` line and this review's S1/S2 findings are closed | all | IN PROGRESS | see §3 |

**Stage verdicts.** Stage 3 needs G1, G2, G9-G14, G17-G19. Stage 4 needs G3-G8,
G13-G15, G17-G19. Neither is complete yet.

---

## 3. Per-packet log

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
