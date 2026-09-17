# Stage 5 remediation, second round: finish WP4-WP7 to terminal pass

> **Status:** executable assignment for one agent; target LayerFS v0.1.7; not a
> released contract. Written 2026-09-17 after WP1-WP3 of the remediation round.
> Repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`
> Stage 5 issue: [#170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170) - **reopened**
> Governing assignment: `stage-5-remediation-handoff-20260917.md` (the full 44-item
> checklist). This document covers only the items that are still open.

## 0. Your mission in one paragraph

WP1 (correctness gates), WP2 (ordering resources and complexity) and WP3
(enforcement and limits honesty) of the remediation round are **complete and
committed**, each proven by the reviewer's own diagnostic client and by new
external cases. Your job is to close **WP4 (composition, product boundary and
simplification), WP5 (harness, oracle and measurement integrity), WP6 (evidence,
documentation and accounting) and WP7 (the issue protocol)** to zero, and then
reach terminal pass on #170 as section 7 of the governing assignment defines it.
Do not stop at a green test run, do not mark anything you have not proven, and do
not close the issue - the owner closes it after re-review.

## 1. Where you start

| Field | Value |
| --- | --- |
| Start commit | `4c4e9163e` (branch `main`) |
| Started from | `c99a8d9f9e05a20ecc8e47b11b8f22fa791664e6`, the reviewed snapshot |
| Production LOC at the start | core **18,470** (was 17,905); reference unchanged at 65,417 |
| Full suite at the start | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked` — 384 passed / 0 failed / 0 ignored |
| Evidence so far | `docs/roadmap/0.1/0.1.7/evidence/stage-5-remediation-20260917T090752Z/` (README lists every log and its reproduction command) |
| Issue comments so far | WP1, WP2, WP3 milestones, each with moved and **deliberately unmoved** matrix rows |

Read first, in this order: `AGENTS.md`, `core/AGENTS.md`, the governing
`stage-5-remediation-handoff-20260917.md` (section 4 is the checklist; sections 3,
5, 6 are the rules you must follow), the review's section 4 matrices
(`stages-1-5-review-20260917T160000Z.md`), and the WP1/WP2/WP3 comments on #170 so
you do not redo settled work or re-promote a row someone already closed.

### 1.1 What is already closed - do not re-litigate

| Item | Commit | Evidence |
| --- | --- | --- |
| R1 second parent for a base Directory/Symlink | `b262ad38c` | `topology-after-wp1.log`: T2/T3 refused |
| R8 effective cycles (build disconnect, build cycle, update cycle) | `ef6bab19d`, `8964db93e` | `build.disconnected_cycle=REFUSED` |
| R14 no pages for a dropped directory | `ef6bab19d` | `a_dropped_directory_emits_no_page_of_its_own` |
| R29 undeclared root refused up front | `ef6bab19d` | `a_build_whose_root_is_not_declared_new_is_refused_up_front` |
| R6 listing byte bound refuses | `ef6bab19d` | `smoke-after-wp1.log`: `ObjectLimitExceeded` for 1..15 |
| R3 quadratic ordering lookup + charged reads | `8701eae12` | `ordering-growth-after-r3.log`: 3.83x -> 3.46x |
| R5 ordering ceiling covers inputs plus output | `cfc6c4ae3` | `merge_inputs_and_output_are_covered_by_the_declared_ceiling` |
| R28 validation work charged | `cfc6c4ae3` | `validation_reads_are_charged_to_the_operation` |
| R45 bounded read wave | `cfc6c4ae3` | `a_read_wave_is_bounded_by_the_declared_ceiling` |
| R9 inode-serial range, R10 attribute value bound | `a376acfee` | two cases in `filesystem_failure.rs` |
| R7 old same-version Store refused at open | `a376acfee` | `an_old_same_version_store_is_refused_at_open` |
| R22 directory-leaf count/ceiling arithmetic | `a376acfee` | `a_directory_leaf_never_exceeds_the_page_ceiling` |
| R13 cycle-check limit declared, charged, pinned | `4f2e4ca9a` | `the_cycle_check_work_limit_is_reachable_and_reported` |
| R15 `LAYERFS_TRACE` branches | `ef6bab19d` | `grep -rn 'LAYERFS_TRACE\|eprintln' core/crates/*/src` empty |
| R2 decision + contract + C2 target | `5b138ce64` | `admission-and-persistence.md` "Caller-authorized value roots" |

**One half-open item from WP2, yours to finish:** R30's release frontier still
reads one serial per demand. The touched-serial vector now has a declared bound
(`FilesystemResources::maximum_touched_serials`); batching the frontier into
`lookup_many` waves is the other half the handoff names, and it is **open**.

**One parity note you must not silently reverse:** the directory-leaf *splitter*
threshold (`Format::page_items(0)`) was deliberately left at the reference's own
value so partitions stay identity-matched. The canonical **row ceiling** moved to
the plain quotient and the encoder now refuses an oversized page as a partition
error. If any WP5 work depends on that threshold being canonical, re-derive it
against the reference oracle; do not assume it.

## 2. Your checklist - WP4, WP5, WP6, WP7

Effort figures in the governing handoff are the reviewer's estimates, not
commitments. Every line ends as **fixed with fresh evidence** or **explicitly
owner-waived in writing by the human owner**; nothing else closes a line.

### WP4 - composition, product boundary and simplification

- [ ] **R11 - collapse the duplicate leaf-page codec.** `object/inode_leaf.rs:189,239`
  and `sorted/format.rs:332,440` implement the same framing twice, disagree on the
  error for one malformation, and `stage-5-handoff.md:104-112` forbids the
  duplication. Make one delegate to the other; add one equality test against the
  sealed two-row leaf bytes. Estimated **-90 to -150 LOC**.
- [ ] **R21 - delete the dead surfaces.** `CheckedInput::removals` (never written),
  `declared_new`, `ReferenceReducer::is_touched`, `InodeIdentity::check_scope`,
  `MINIMUM_DIRECTORY_BRANCH_CHILDREN` (its own only occurrence), and the exported
  functions with zero callers: `content::object::id::authenticate`,
  `file::mapping::codec::chunk_canonical_len`,
  `storage::encoding::codec::group_body_parameters`, `sqlite::lookup::policy`
  (use `chunk_canonical_len` in the chunk encoder if that is its intent).
  Estimated **-60 to -90 LOC**. Note `is_touched` is what R3's cursor work touched;
  check the current source before deleting.
- [ ] **R24 - check a declared tree role against the object's inner tag**, or state
  the precondition in the contract and add one negative test. `encoding/full.rs:46-74`
  validates only `WholeFile`/`Chunk`; tree roles go straight to `encode_full`
  (`delta/select.rs:205-222`) and the declared role is persisted (`cas/owner.rs:800`).
- [ ] **R41 - ship the Store-to-C1 provider bridge, or record the decision.** No
  `impl AuthenticatedObjects` exists in product source; the same adapter is
  re-implemented five times in excluded support
  (`layerfs-storage/tests/support/{mod.rs:158,filesystem.rs:369}`,
  `examples/{measure_components.rs:324,measure_edits.rs:611}`,
  `tests/filesystem_pipeline.rs:78`). A Stage 7 adapter needs it. Either export one
  small provider over `Store::read_batch` or record explicitly that it is adapter
  work.
- [ ] **R23 - derive the mapping profile identity from the named constants.**
  `file/mapping/codec.rs:33-47` hashes the literals 64/128/2/31/32768 instead of
  `MIN_ENTRIES`/`MAX_ENTRIES`/`MAX_LEVEL`/`cdc::MAXIMUM_CHUNK_BYTES`.
- [ ] **R27 - make the profile self-describing or test its correspondence.**
  `root.rs:21` lists the profile as prose; nothing derives or tests it against
  `limits.rs` and the format constants, and the root-profile assertion is
  tautological after construction.
- [ ] **R31 - decide the `"new inode removal"` branch** (`reduce.rs:120-129`):
  reachable only from the release walk, untested, and labelled differently from its
  sibling `"new inode without binding"`. Decide, document, test.
- [ ] **R37/R39 - close the two items the previous review left open.** (a) C1
  navigation is one point provider call per visited page
  (`file/mapping/read.rs:190`, `filesystem/inode/read.rs:51`,
  `filesystem/directory/read.rs:285`) and each C2 read call re-creates a connection
  and a 1 MiB decode workspace (`cas/store.rs:205,210-212`): batch a level's demands
  and keep or size the workspace. (b) The provider trait takes no timing scope
  (`object/access.rs:18-20`), so C2 work inside a C1 read lands under the caller's
  span.
- [ ] **R46/R47 - decide and record the placement constraints.** The persistence
  seam is local-path-only (`cas/store.rs:92-143`) and the core is `!Send`
  (`SaveOperation` holds a raw zstd context; `FileBacking` is `Rc`-based;
  `TimingScope` is `!Send`). Either accept them in writing as adapter constraints
  or change them; do not leave them implicit.

### WP5 - harness, oracle and measurement integrity

- [ ] **R4 - make the harness select and compose what it names.**
  `examples/measure_filesystem.rs:174-179` maps four case names to one fixture size,
  so `--mode pipeline` prints the identical root for all four; `--mode c1` always
  changes the first ten bindings. Map each `--case` to a real change set (or print
  the banner `measure_edits.rs:411-422` prints and rename the rows), and route the
  integrated row through `SaveHandoff` plus a **reopened Store** read-back - the
  pattern already exists at `examples/measure_components.rs:275-296`.
- [ ] **R17 - re-collect the component comparison at the frozen commit.** The
  receipt `evidence/stage-5-component-comparison-20260917T073017Z` records
  `source_commit 3b4941f1e` and runs the candidate arm as
  `--example filesystem_primitives_candidate`, which changed in `b22712844` after
  collection; under `AGENTS.md` §3.3 the pair is invalidated. Re-run
  `tools/stage5_component_comparison.py` at your commit into a **fresh** evidence
  directory, keep both arms' identities, and state the cache state and sample
  policy. One sample per case per arm.
- [ ] **R12 - stop the oracle rewriting itself.**
  `crates/layerfs-content/tests/stage5_reference_fixtures.rs:618-620` is a plain
  `#[test]` that overwrites `core/crates/layerfs-content/tests/fixtures/filesystem/`.
  Gate the writer (env var or `#[ignore]`) and keep a read-only comparator that
  fails when the seal changes. Re-prove the seal reproduces: regenerate in an
  isolated `git archive` copy, never in place.
- [ ] **R18 - stop one trace node per accepted object.** `cas/store.rs:257-263`
  creates a child per `accept`, contradicting `admission-and-persistence.md:538`
  and capping useful detail near 500 objects. Move to one scope per batch or wave,
  or amend the contract with the owner's reason.
- [ ] **R35 - make the Stage 5 harnesses fail on a clipped report.**
  `examples/measure_filesystem.rs:285` and `examples/filesystem_timing_c1.rs:375`
  print only `report_nodes`; `measure_edits.rs:299-311` and
  `measure_pooled.rs:193-203` already consult `TimingReport::is_incomplete()`.
- [ ] **R36 - fix the `--mode c1` claim.** `stage-5-verification.md:84` says a C1
  row opens no database, but `measure_filesystem.rs:183-191` creates a Store before
  the mode dispatch. Correct the contract text or make the mode skip creation.
- [ ] **R38 - fix the C2 accounting and labels.** `framed_len` over-counts group
  framing by 4n-4 (`cas/owner.rs:358,377-382`); the save read path copies pending
  bytes twice (`cas/store.rs:327,363`); the inner scope named `storage.read` nests
  inside caller scopes also named `storage.read` at every in-repo call site; and
  `SaveOutcome.acknowledged` is hard-coded true (`cas/store.rs:60`), so an
  assertion on it cannot fail. Fix each or document why it stands.
- [ ] **R42 - do not count zero-test binaries as coverage.** Six of 58 targets are
  `unittests src/lib.rs` or doc-tests with no tests. State coverage in terms of
  tests that exist.

### WP6 - evidence, documentation and accounting

- [ ] **R16 - re-derive the Stage 5 report's tables.** The per-file LOC column is
  stale in ten rows (`update.rs` 333 -> 359, `input.rs` 128 -> 135, `read.rs`
  196 -> 191, `sorted/page.rs` 447 -> 449, `sorted/finish.rs` 154 -> 156,
  `references/record.rs` 117 -> 120, `references/backing.rs` 116 -> 208,
  `references/runs.rs` 247 -> 323, `references/merge.rs` 121 -> 123,
  `references/reduce.rs` 465 -> 472); the file count is **40**, not 24; and the
  above/below prose is **8 above / 17 below of 53**, not four and six. The
  **totals** are correct (11,160 -> 17,905, +6,745; reference 65,417) - only the
  detail is wrong. Use the reviewer's `loc-tables.md` as a cross-check, not as a
  substitute for re-running the counter. **These figures predate WP1-WP3; re-derive
  them at your commit, not at the reviewed one.**
- [ ] **R25 - correct the schema drift.** `content-storage-policy-and-tables.md:304-310`
  still says "19 columns" / "20 columns and `user_version = 2`"; the tree is
  **21 columns at `user_version = 4`** (`sql/schema.sql:10-11,54`,
  `sqlite/schema.rs:22-58`).
- [ ] **R48 - correct the small documentation drifts.** `sqlite/write.rs:34` says
  the direct delta base is "always absent in this slice" while `cas/owner.rs:791`
  writes it; and `stage-5-report.md` section 3D said "88-byte record" where the
  grammar is 96 bytes (**already fixed in `5b138ce64` - verify, do not redo**).
- [ ] **R19/R20 - make the backing honest.** After a failed removal, `release`
  zeroes `held` (`backing.rs:220-230`) so the accessors report clean while a run
  file remains; and the fixed run name (`backing.rs:179-191`) makes one stale file
  permanently fatal for every later backing in that directory. Fix the accessors
  and either tokenise the name or adopt/truncate an expected file.
- [ ] **R26/R33 - fill the negative and bound coverage.** Role-byte and flag
  negatives exist only for the inode leaf; add directory/inode branch roles, the
  root role and reserved byte, symlink role and flags, branch `subtree_bytes` and
  child-summary mismatches. Batch lookups have no cardinality cap of their own and
  no read-side bound test.
- [ ] **R32/R34 - cover and bound the read surface.** `read_portable`,
  `read_attribute` and `attribute_keys` have no caller anywhere: no test binds a
  real attribute tree to an inode and reads it through a path. `attribute_keys`
  accumulates every key with no count or byte bound, and `FilesystemReadWork` is
  consumed by nothing (resolve charges no directory work - measured
  `directory.pages_read = 0` after two stats).
- [ ] **R40 - document the interrupted-save recovery path.** A death between a
  mid-save COMMIT (`cas/owner.rs:837-843`) and the watermark transaction
  (`:904-908`) leaves every later save refused with `UninspectedState` and no
  product recovery path. This is contract-consistent; document the operator path
  and add a test that reads still work afterwards.
- [ ] **R43 - fix the Stage 4 case label or the fixture.** The case named
  `repartition-80-100` (`tests/edit_reference.rs:208-226`) has sealed base pages of
  89 and 90, not 80 and 100, because the base is canonically rebuilt; the exact
  90+90 result **is** asserted. Rename the case/report row or add a fixture whose
  base is the literal join.
- [ ] **R44 - get owner answers, do not assume waivers.** The Stages 3-4
  pooled-lane/v5 deviations (E2) are an **unanswered owner decision**, not a waiver;
  the 8 MiB-1 deferred refusal (W10) is unrun; A2 item 6 is met for unclipped arms
  only. Ask the owner and record the answer verbatim; do not upgrade any of them.
- [ ] Also yours: **update the memory ledger** in `content-io-memory-audit.md` for
  the WP2 owners (touched-serial bound, merge inputs and output, validation work,
  read-wave capacity) - the governing handoff names this under R30's update.

### WP7 - continuous: keep #170 honest

- [ ] **Update #170 at every milestone**, one comment per work package, containing:
  what changed (commit hashes), the exact before/after evidence paths, the checks
  run with exits, the production LOC line, and **the matrix rows you moved** quoted
  by the review's own row IDs with their new status - and, for any row that did not
  move, why. Post failed attempts too. Never rewrite an earlier comment; add a
  dated correction. Only the human owner can waive a line, and a waiver must be
  quoted verbatim.
- [ ] **Terminal pass.** Update the body's dated acceptance-status section - replace
  only the text between the `stage5-acceptance-status` markers - post a final
  comment, and **stop without closing the issue**.

## 3. How to work fast (the owner's explicit instruction)

The previous round lost time re-running the full workspace suite after nearly
every edit. Do not repeat that.

1. **Targeted tests during iteration.** `cargo +1.85.1 test --manifest-path
   core/Cargo.toml -p layerfs-content --locked --test <file>` for the target you
   touched, or `-p layerfs-storage --test <file>`. Add `-- --nocapture` when a
   failure needs its printed state.
2. **One full suite per commit** — the whole-workspace run in section 5, once, on
   the tree you are about to commit. If it passed on the parent and your change is
   local, a targeted run plus clippy is enough to iterate.
3. **Batch related items into one commit.** WP1-WP3 landed as six commits total for
   sixteen checklist items. Prefer one commit per coherent group of items, each
   with its own new cases, over one commit per item.
4. **Write the discriminating case first** when a fix is subtle. R3's cursor was
   wrong three times and the differential test is what caught all three; the
   ordering-growth probe caught the fourth. A test that compares against the
   component's own replay is worth more than a test that asserts a constant.
5. **Never re-run something whose inputs did not change.** Reuse the diagnostic
   binaries (`CARGO_TARGET_DIR=/tmp/lfs-remediation-diag-target`); they rebuild in
   about two seconds.

## 4. Rules that are not negotiable (carried forward, unchanged)

- **One attempted operation.** No retry, busy handler, error-driven fallback,
  alternate algorithm or backend, no resend on an unknown outcome. Unsupported
  required capabilities fail explicitly.
- **No durability theatre.** No `fsync`/`fdatasync`/`sync_all`/`sync_data`, no WAL,
  no checkpoint or recovery service. SQLite stays MEMORY journal,
  `synchronous = OFF`, `temp_store = MEMORY`, foreign keys ON, zero busy timeout.
- **No third-party patches, forks, vendoring or registry edits.** Builds stay
  `--locked`.
- **Product source only.** `core/crates/*/src` has no tests, mocks, fault
  injection, test-only cfg, test-only public APIs, debug environment branches, or
  helpers that exist only for tests. Maximum 999 physical lines per production
  file; `lib.rs`/`mod.rs` at most 200 and declaration/delegation only.
- **No debug environment branches.** R15 deleted five; do not add any back. A
  `std::env::var` in product source is a defect.
- **One producer.** Do not add workers or lanes. `init_namespace` keeps its
  existing multi-worker exception; nothing else does.
- **Every commit reports production LOC**:
  `Production LOC: <before> -> <after> (delta <signed>)` with scope and counting
  method, measured with `python3 tools/production_loc.py` on the exact first parent
  and the committed tree. Disclose estimate-versus-actual honestly; never remove
  validation to hit a number.
- **No warm-cache credit, no fabricated PASS.** A phase pays for its own work from
  a declared cache state; reuse setup, never measurement. Declare the cache state
  and the sample policy with every collected row.
- **No CI and no aggregate gate.** `tools/preflight.sh` is permanently retired.
  Run the explicit checks in section 5 and report exactly which ran.
- **Never overwrite retained evidence or historical fixtures.** New evidence goes
  to a fresh timestamped directory; receipts are append-only.
- **Do not touch** the closed #166/#167 residuals, the reference tree under
  `crates/`, or another owner's concurrent work. Do not delete or retire reference
  code.
- **Leave another owner's artifacts untracked.** The review report, the retained
  review evidence directory, the governing handoff document and
  `study/cloudflare-computer/` are untracked in the working tree and belong to
  someone else. Read them; do not commit or edit them.

## 5. Verification you must run, and report exactly

Run these **once per commit group**, on the tree you are about to commit:

    python3 core/tools/check_product_boundary.py
    python3 -m unittest discover -s core/tools -p 'test_*.py'
    python3 -m unittest discover -s tools -p 'test_production_loc.py'
    cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
    cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked
    cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
    python3 tools/production_loc.py --files
    git diff --check

Plus, for the remaining items specifically:

- the reviewer's diagnostic client re-run after WP4 and WP5 changes, with the logs
  appended to a **fresh** evidence directory (do not overwrite
  `stage-5-remediation-20260917T090752Z/`);
- every WP4-WP6 item that changes behaviour gets one new external case that fails
  on the parent commit and passes on yours;
- the R12 fixture seal re-proved by regeneration in an isolated `git archive` copy;
- the R17 component comparison re-collected with identity-matched arms;
- the R4 integrated row routed through `SaveHandoff` and a reopened Store.

"CI green" and "the preflight passed" are forbidden claims, because neither
exists. Report every exit, failure and gap.

## 6. What terminal pass requires

All of the following, with evidence:

1. Every WP1 item fixed and demonstrated by a new case that fails on its parent
   commit and passes now (**done for WP1-WP3; verify the claim still holds on your
   tree before repeating it**).
2. Every WP2-WP6 item either fixed with fresh evidence, or explicitly owner-waived
   in writing and quoted in #170.
3. The Stage 5 matrix, re-derived with the review's own row IDs, shows **zero FAIL
   and zero INCOMPLETE rows**; any `NOT_RUN` row is named with its owner-approved
   reason. Every row closed by WP1-WP3 is listed in the corresponding issue comment
   - carry those forward rather than re-deriving them from scratch.
4. The four previously failing acceptance bullets in #170's body can be ticked with
   evidence links, and the component comparison has an identity-matched receipt.
5. All section 5 checks run on the final tree with their exits recorded, production
   LOC reported per commit, and `git status` clean apart from other owners'
   untracked artifacts and your own evidence.
6. #170 carries a complete milestone history: every package reported, every failed
   attempt recorded, and the final acceptance-status section telling the truth about
   what is proven versus what is merely implemented.

**Terminal pass is not "the tests are green", "the report says so", or "the
reviewer did not look".** If something remains unproven at the end, say so plainly
and leave the issue open with the precise gap. Do not close #170.

## 7. Out of scope - do not do these

- Do not implement Stage 6 qualification (#171): the complete-operation comparison,
  cold-cache rows, pack-footprint and whole-process memory belong there.
- Do not implement Stage 7 runtime integration (#172), FUSE, adapters, transports
  or any plugin registry.
- Do not add revision/history naming, retention, GC or a workspace quota - C1/C2
  deliberately provide filesystem roots, not a Workspace lifecycle.
- Do not delete, retire or modify the reference tree under `crates/`.
- Do not enable non-compact profile support or add a converter.
- Do not close #170, produce a release or tag, or change
  #166/#167/#168/#169/#171/#172.
- Do not touch the retained review evidence or another owner's concurrent work.

## 8. Definition of done for your first hour

By the end of your first hour you should have: read the four documents in section
1, run the full suite once to confirm the baseline (384 passed / 0 failed), posted
a **start comment** on #170 naming your start commit and your work-package order,
and closed at least two WP4 items - the deletions (R11, R21) are the fastest and
they pay for themselves in LOC. Do not spend the first hour on R37/R39; they are
the largest items in the package.
