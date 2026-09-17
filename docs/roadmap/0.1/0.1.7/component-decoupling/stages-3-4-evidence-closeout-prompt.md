# Stages 3–4 evidence closeout prompt: the remaining items

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract. Written
> 2026-09-17 after an independent audit of every retained receipt in the Stages 3–4
> batch (#168 / #169, under #165). Receipts and contracts are append-only: this
> prompt adds work and corrects reports; it never re-labels or rewrites a collected
> receipt.

This prompt covers only what is still open. Findings live in
[stages-3-4-closeout-report.md](stages-3-4-closeout-report.md) §5 and
[stages-3-4-verification.md](stages-3-4-verification.md) §8; this document sequences
the work and defines closure.

## 0. Why this round exists, and what it is not

Two different things are unfinished, and conflating them is the failure mode.

* **In-batch acceptance work.** #169 acceptance item 2 requires size transitions
  "with sound physical read amplification accounting". The registry says so plainly:
  `stages-3-4-verification.md:68` — "read amplification on the representation
  transition | none | **NOT_RUN** - no case exists; the residual gap the acceptance
  report names, unmeasured in this batch". No later stage absorbs it, and the
  repository forbids the relabelling: `stages-3-4-completion-handoff.md:220`
  "Do not postpone a Stage 3–4 acceptance requirement merely by relabelling it
  Stage 6." (#171 is Stage 6 "qualify the complete C1/C2 core"; #172 is Stage 7
  runtime integration — neither is this item's home.)
* **Cross-batch qualification that is genuinely deferred.** The matched v0.1.6
  campaign (latency, storage, memory) was waived in writing by the owner (E1) and the
  qualification deferred to Stage 6 (#171). What remains for this batch is not a
  measurement obligation but the *prerequisites* that must be built while the C1/C2
  surfaces and the reference pair still exist.

It is **not** a re-run of the batch, not a re-collection of any existing arm, and not
permission to retune a limit, raise a timeout, add a worker or move work out of a
timed scope.

## 1. Start identity and standing constraints

* Work in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`; read `AGENTS.md`,
  `core/AGENTS.md`, `docs/general/benchmark_rules.md` and the closeout
  report before touching measurement code.
* Resolve and record the actual start commit and tree. A rebuilt artifact needs a
  rebuilt matched arm; a harness change invalidates a pair.
* Reference revision stays `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`, source/
  oracle input only, never linked into candidate production or kept as a runtime
  fallback. Reference retirement remains a later owner decision — while it is
  deferred the matched pair is still buildable, and that window is why §B5 exists.
* Receipts are append-only: fresh `--output` per run, `create_new` trees,
  failures and `INELIGIBLE` rows stay on disk, no best-of, no deletion, no
  re-labelling.
* No CI, no aggregate preflight, no wrapper gate. Verify the affected workspace with
  explicit commands: `cargo +1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml
  --locked`, `python3 core/tools/check_product_boundary.py`, the two tool
  suites and `git diff --check`.
* Production LOC rule applies to every commit; docs-only commits report the unchanged
  total with delta 0 (currently core 11 058 in 75 files, reference 65 417, combined
  76 475 — `final/final-verify.log`).

## A. In-batch work (do not defer)

### A1 — Read amplification on the representation transition (#169 item 2)

The case does not exist; build it, on the code it describes.

1. **Define the accounted quantity once and state it in bytes as well as counts.**
   Use the counters that already exist rather than inventing a hook:
   `StoreReadCounters` (objects, packs_read, pages, ceiling, edges, max_depth,
   canonical_bytes — see `evidence/stages-3-4-closeout-20260916T235641Z/w10/w10-verify.log:1052`)
   and C1's `EditCounters` (nodes_read, nodes_created, payloads_created,
   payload_bytes, peak_deferred_bytes). State which of these is "physical read
   amplification" and why, before measuring.
2. **Case matrix.** For every accepted cutoff (128 KiB, 256 KiB, 1 MiB) run the three
   transitions on a chunked base: small → large (insert), large → small (delete),
   empty (full delete), plus an in-place overwrite control. Assert, per case:
   * the result equals the independent model and the fresh construction root;
   * a discarded range is never read — the replaced bytes are not acquired before
     being dropped;
   * the base is acquired through the supplied provider, not re-read whole;
   * the read counters are charged to the transition, with the declared bound
     asserted rather than described.
3. **Where it lives.** Content-side cases in
   `core/crates/layerfs-content/tests/edit_transitions.rs` (or a sibling target);
   anything that needs a Store belongs in
   `core/crates/layerfs-storage/tests/edit_pipeline.rs`. External tests only —
   no test-only public API in production.
4. **Control.** Each new assertion needs a run that fails without it, recorded in a
   `*-fails-without-fix.log` the way W1–W6 did — with the no-op-patch trap
   avoided (`w6/w6-fails-without-fix.log:1-4` shows how a patch that does not
   apply is caught and marked as proving nothing).
5. **Evidence.** One fresh directory
   `docs/roadmap/0.1/0.1.7/evidence/stages-3-4-read-amplification-<UTC>/` with a
   README that quotes its own raw log, the commands, exits and wall times.
6. **Then** update the registry row `stages-3-4-verification.md:68` from
   `NOT_RUN` to `RUN` **only** with that receipt named. If the case cannot be
   built, record it `NOT_RUN` again with the measured reason and seek a written
   waiver — the same standard G13/G15 were held to.

### A2 — Report-side corrections (receipts are never touched)

Each item is a misquote or an unsourced number in a report. Correct the report that
carries it, dated, naming what it replaces — the idiom already used in
`evidence/stages-3-4-oracle-20260916T214846Z/README.md` ("Corrected
2026-09-17, after the independent review recorded the overstatement") and
`w9/README.md` §W9.1.

| # | Where | The problem | What a correction must say |
| --- | --- | --- | --- |
| 1 | `evidence/stages-3-4-smoke-20260916T210931Z/README.md:34` | quotes `edit.save` 158.824 ms for pipeline small-to-large | the retained `grow/pipeline-edit-save.json:3` says `"elapsed_ns": 193977292` (193.977 ms); the old number is unsupported because the run's stdout was not retained (reported twice: E-D1, F-6; still uncorrected at HEAD) |
| 2 | same README, lines 35 and 38–53 | quotes the `shrink2` run under the `shrink` label | `shrink/` = 6 060 625 ns, `shrink2/` = 5 970 333 ns; readback 54 762 208 ns vs the quoted 54.928 ms |
| 3 | `evidence/stages-3-4-closeout-20260916T235641Z/w7/README.md:82-87` | the `rss before → after` column matches none of the three ledgers retained in `w7/w7-verify.log` (lines 24-29, 45-50, 86-91) | either cite the run that produced them and retain it, or mark the column unsourced; the four heap columns match ledger 1 verbatim |
| 4 | `w7/README.md:94` and the closeout report's G12 | calls 3 450 007 B "3.45 MiB" | it is 3.45 MB / 3.29 MiB |
| 5 | `w7/README.md:77-78` and G12 | calls a 24-leaf, 2 400-distinct-value fixture "the E1b shape" | E1b is 128 leaves with 227 distinct values (`e1b-pooled-128/stdout.log`); the closest shape is E1a's leaf count with a different value distribution |
| 6 | `w3/README.md:114` | the with-fix frontier peaks `[2208, 2288, 2448, 2768, 3408]` have no raw receipt | `edit_bounds` prints peaks only on failure; either re-run it with `--nocapture` into a fresh round, or label the vector as unreceipted. The control vector *is* receipted (`w3/w3-fails-without-fix.log:12`) |
| 7 | `stages-3-4-matched-c1-20260917T050000Z/ledger.md:3` | "collection on 2026-09-16 (host clock)" | on the host clock (+0800) it was 2026-09-17 07:22; and the three "development run" observations have no retained receipt — say so or drop the numeric claim |
| 8 | `stages-3-4-verification.md:67` vs `:110-113` | one clause says the E1 decision "is open", §4 records the written waiver | reconciled in §8 of that file by this round; the frozen §1/§2 text stays untouched |
| 9 | `stages-3-4-closeout-report.md` (this batch's tracker) | the audit's findings were not carried in the report | recorded in the new §5 of that file by this round |

## B. Harness prerequisites (must land before any campaign)

* **B1 — Telemetry clipping.** `e1c-pooled-512/stdout.log:16` prints
  `pooled.save  3.105s [incomplete]`; the tree is clipped at 1 024 nodes and
  57.46 % of the scope is unattributed (measured from `store/pooled-save.json`:
  root 3 105 519 375 ns, retained children 1 320 950 951 ns). Either raise/stream the
  tree, or cap arms so they cannot clip, and make a clipped tree a **hard failure**
  for a measured row. A campaign must never quote a partial tree.
* **B2 — Budget metric.** `wall_seconds` is printed by no product tool
  (`grep -rn 'wall_seconds' core/crates/` is empty), so the ≤15 s per-command
  rule is asserted by an unrecorded wrapper. Print the wall time in the tool, or
  retain and document the wrapper.
* **B3 — Worker identity.** `stages-3-4-verification.md:23` freezes "every run
  exports `LAYERFS_CONSTRUCTION_WORKERS=1`". No timing-round or matched-C1
  `command.txt` contains it and no core source reads the variable
  (`grep -rn 'env::var' core/crates/*/src/` is empty). Either export and record
  it, or delete the line. An unverifiable frozen identity is worse than no line.
* **B4 — The C2 lane ignores `--case`.** `core/crates/layerfs-storage/examples/measure_edits.rs:390-450`
  truncates the fixture to `whole_file_raw_limit` and applies its own patch, so
  four of five `e2-c2-*` arms wrote a byte-identical `store.sqlite`
  (`b59c7b24d91ad7d4…`, 294 912 B) and all five print the same
  `readback: verified 131094 canonical bytes`. Make `--case` drive the stored
  workload, or rename the mode so it cannot be read as a per-case comparison.
* **B5 — Seal the matched pair.** The timing round's `tool-identities.txt` names
  sha256 values that match nothing on disk (the examples were rebuilt; `measure_pooled`
  is now `7214908d5483…`). Archive the matched-pair binaries by sha256 (the
  `benchmark/fs-bench-pro` `binary-archive/<sha256>/` pattern) and record the
  archive path in the ledger before the next rebuild, or the pair will be as
  unreproducible as the timing round is now.

## C. Owner decisions (blocking, not technical)

* **E1 — the matched campaign.** Yes → execute §D. No → a written re-scope that names
  #171 as the carrying issue and restates that Stage 3–4 latency, storage and memory
  stay **unqualified**, so the closed state is never read as a measured comparison.
  The existing waiver covers G13/G15 only.
* **E2 — the two format/design deviations** (pooled leaf records staying in the v1
  ordinary lane; v5 refused by scope), recorded in
  `physical-encoding-and-packing.md` lines 270-284.
* **E3 — #169 acceptance item 2.** #168 and #169 are already closed (GitHub
  `updatedAt` 2026-09-17T02:15Z) on a waiver that does not mention read
  amplification. It therefore needs either §A1's case or its own written waiver,
  recorded in `stages-3-4-closeout-report.md`. Leaving it unstated while the issue
  is closed is the one outcome this prompt exists to prevent.

## D. The campaign, if E1 = go

1. **Commit the versioned addendum first** (the rule is in
   `stages-3-4-verification.md:155-161`): cases and exact inputs/seeds; both arms'
   identities (commit, compilation seal, sha256 **and** the retained archive path from
   B5); cache state plus the contract that enforces it, with cold and warm rows never
   pooled; sample count **n >= 5 per arm with every sample reported**; limits; the
   acknowledgement boundary; budgets. No sample may be taken before it is committed.
2. **Align the byte-accounting boundary.** Neither current arm opens a Store, and the
   two sides compute different quantities — the reference prints a content-deduplicated
   store-map delta and its batch's `nodes_created`, the candidate prints emission
   counts for both `objects_written` and `nodes_created`
   (`core/crates/layerfs-content/examples/edit_timing_c1.rs`). One defined quantity,
   computed the same way on both sides, stated in the addendum.
3. **Build the reference-side memory instrument.** Today only the candidate has one
   (`core/crates/layerfs-storage/examples/memory_ledger.rs`, one sample, debug).
   A memory comparison without a reference arm is not a comparison.
4. **Report honestly:** every sample, no best-of, `INELIGIBLE` and clipped rows
   beside the gate row, no lifetime cgroup figure as a phase number, no heap figure
   inferred from a file size.

## E. Closure definition

| Item | Closes when |
| --- | --- |
| #169 item 2 / registry row "read amplification on the representation transition" | §A1's case passes with a raw receipt, or a written waiver names it |
| G4's with-fix peak vector | re-run with `--nocapture` into a fresh round, or the report marks it unreceipted |
| The nine A2 corrections | each report names what it replaces, dated; no receipt changed |
| Reporter identity line (`stages-3-4-verification.md:23`) | exported and recorded, or deleted by owner decision |
| G13 / G15 | already closed by the written waiver; stay **unmeasured** — do not re-open, do not upgrade |
| Stages 3–4 as a whole | §A1 and §A2 done, B1–B5 landed or explicitly waived, C answered |

## F. Prohibitions

No receipt rewrite, re-label or promotion after the fact; no deletion of a failing or
`INELIGIBLE` row; no best-of, no raised limits, no inflated timeout, no extra
worker to make a gate pass; no warm/cold pooling; no aggregate gate, workflow or
wrapper (`tools/preflight.sh` stays retired); no new dependencies, patches,
vendoring or forked crates; no `fsync`/`fdatasync`/`sync_all` claims
the contract does not provide; no re-opening #166/#167/#174; no claim that a waived
row was measured.

## G. Deliverables

1. §A1 case + control + one fresh evidence directory, with the registry row updated.
2. §A2 corrections applied in the reports that quote the numbers, dated and
   attributed, with receipts untouched.
3. §B fixes with their own receipts (or an owner waiver naming each).
4. C answered in writing in the closeout report.
5. If E1 = go: the committed addendum, the sealed pair, the aligned boundary, the
   reference arm, and the campaign receipts.

## H. Reporting

For every run: the exact command, exit code, whole-command wall time, the tool
identity (commit + sha256 + archive path), the raw log path, and every non-passing
line. State plainly which of §A–§C was not done and why. A run limit is not a
technical blocker and does not change these criteria.