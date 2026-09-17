# Stage 5 remediation handoff: close every open review item until terminal pass

> **Status:** executable assignment for one agent; target LayerFS v0.1.7; not a
> released contract. Written 2026-09-17 after the independent Stages 1-5 review.
> Repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`
> Stage 5 issue: [#170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170) - **reopened**

## 0. Your mission in one paragraph

Stage 5 (filesystem trees, attributes, reference ordering) was implemented, reported
and closed as completed. An independent acceptance review then found **four
reproduced blocking defects** and fifteen failing criterion rows on the same frozen
tree. #170 has been reopened. Your job is to work the complete checklist in section 4
to zero: fix what is broken, prove it with fresh evidence from real production
bodies, re-derive the stale accounting, re-collect the comparison that is no longer
identity-matched, and **update #170 iteratively at every milestone until the issue
reaches a terminal pass** (section 6). Everything else - #171 Stage 6, #172 Stage 7,
release, tags, reference retirement - is out of scope.

## 1. Context you must have before touching anything

| Field | Value |
| --- | --- |
| Reviewed snapshot | `c99a8d9f9e05a20ecc8e47b11b8f22fa791664e6` (branch `main`, clean tree at review time) |
| Review report | `docs/roadmap/0.1/0.1.7/component-decoupling/stages-1-5-review-20260917T160000Z.md` |
| Review evidence | `docs/roadmap/0.1/0.1.7/evidence/stages-1-5-review-20260917T160000Z/` (32 files, sha256 manifest) |
| Review verdict | Stage 5: 71 rows - **50 PASS, 15 FAIL, 5 INCOMPLETE, 1 NOT_RUN**. Cumulative Stages 1-5 C1/C2: 31 rows - 27 PASS, 1 FAIL, 1 INCOMPLETE, 2 owner-WAIVED |
| Implementation reports | `stage-5-report.md`, `stage-5-completion-report-20260917.md`, `stage-5-verification.md` + addendum, `stage-5-blocker-investigation-20260917.md` |
| Prior blocker audit | `evidence/stage-5-blocker-audit-20260917T055334Z/` (three defects, already fixed in `-ordering-fixes`/`-ordering-corrected`; do not re-litigate) |

The reviewer's own diagnostic client is retained at
`docs/roadmap/0.1/0.1.7/evidence/stages-1-5-review-20260917T160000Z/diagnostics/stage5-diagnostics/`
(four binaries, public API only, its own workspace). **Run it first** - it reproduces
findings R1, R2, R6, R7, R8 and R10 in seconds and gives you a before/after harness
you did not have to write. Its logs (`run.log`, `topology.log`,
`dangling-reference.log`, `ordering-growth.log`) are the "before" side of your
proof. Do not edit that directory: it is retained review evidence. Copy what you need
into your own evidence directory instead.

## 2. Read these, in this order

1. `AGENTS.md` (repository rules: cache neutrality, setup reuse, per-commit
   production LOC, no CI, no third-party patches) and `core/AGENTS.md` (product
   source only, 999-physical-line cap, 200-line `lib.rs`/`mod.rs` cap, one
   attempted operation, SQLite MEMORY journal profile).
2. The Stage 5 contract set in `docs/roadmap/0.1/0.1.7/component-decoupling/`:
   `stage-5-handoff.md`, `stage-5-file-plan.md`, `filesystem-tree.md`,
   `stage-5-verification.md` + `stage-5-verification-addendum-20260917.md`.
3. The C1/C2 contracts the review used: `canonical-objects.md`,
   `file-content.md`, `finalized-object-handoff.md`,
   `physical-encoding-and-packing.md`, `admission-and-persistence.md`,
   `content-storage-policy-and-tables.md`, `content-io.md`,
   `content-io-memory-audit.md`, `telemetry.md`.
4. The review: sections 0 (findings), 4 (matrices), 5 (simplification),
   11 (closing answers) and 12 (checklist). Section 12 is your work list; section 4
   is your acceptance test.
5. For anything you measure: `docs/general/benchmark_rules.md`,
   `benchmark/AGENTS.md`, `benchmark/fs-bench-pro/QUICKSTART.md`,
   `docs/general/release-policy.md`, `docs/general/documentation-policy.md`.

## 3. Rules that are not negotiable

- **One attempted operation.** No retry, no busy handler, no error-driven fallback,
  no alternate algorithm or backend, no resend on an unknown outcome. Unsupported
  required capabilities fail explicitly.
- **No durability theatre.** No `fsync`/`fdatasync`/`sync_all`/`sync_data`,
  no WAL, no checkpoint or recovery service. SQLite stays MEMORY journal,
  `synchronous = OFF`, `temp_store = MEMORY`, foreign keys ON, zero busy timeout.
- **No third-party patches, forks, vendoring or registry edits.** Builds stay
  `--locked`.
- **Product source only.** `core/crates/*/src` has no tests, mocks, fault injection,
  test-only cfg, test-only public APIs, debug environment branches, or helpers that
  exist only for tests. External tests in `tests/` exercise the same production
  bodies. A production file is at most 999 physical lines; `lib.rs`/`mod.rs` at
  most 200 and declaration/delegation only.
- **One producer.** Do not add workers or lanes. `init_namespace` keeps its existing
  multi-worker exception; nothing else does.
- **Every commit reports production LOC**: `Production LOC: <before> -> <after> (delta <signed>)`
  with scope and counting method, measured with `python3 tools/production_loc.py`
  on the exact first parent and committed tree. Disclose estimate-versus-actual
  honestly; never remove validation to hit a number.
- **No warm-cache credit, no fabricated PASS.** A phase pays for its own work from a
  declared cache state; reuse setup, never measurement.
- **No CI and no aggregate gate.** `tools/preflight.sh` is permanently retired.
  Run the explicit checks (section 5) and report exactly which ran.
- **Never overwrite retained evidence or historical fixtures.** New evidence goes to
  a fresh timestamped directory; receipts are append-only.
- **Do not touch** the closed #166/#167 residuals, the reference tree under
  `crates/`, or another owner's concurrent work. Do not delete or retire reference
  code - that is not authorized by this assignment.

## 4. The checklist - finish every line

Effort figures are the reviewer's **estimates**, not commitments. Every item must end
as either **fixed with fresh evidence** or **explicitly owner-waived in writing by the
human owner** - nothing else closes a line. Work packages are ordered; WP1 first.

### WP1 - Correctness gates (these block acceptance)

- [ ] **R1 - refuse a second parent for a Directory or Symlink.**
  `filesystem/validate.rs:132-160` counts only this batch's additions; the base
  binding contributes only the child's *kind*, so `d/e` and `d/e2` can both name one
  directory inode and it is stored with `namespace_ref_count = 2`. The invariant
  already exists and is not called here: `InodeValue::validate`
  (`object/inode_leaf.rs:101-113`). Fix: when the base record's kind is not
  `RegularFile`, require `previous.namespace_ref_count + added == 1`. Keep file
  hardlinks working. Test: the reviewer's `T2`/`T3` shapes must be refused; the
  `T4` same-batch control must stay refused. Estimated +4 to +8 LOC.
- [ ] **R2 - decide and enforce the persisted-reference contract for inode values.**
  `InodeValue` embeds `content_root` and `metadata_root`
  (`object/inode_leaf.rs:117-140`), a level-0 leaf row declares `child: None`
  (`sorted/format.rs:362-366`) so `sorted/page.rs:353` collects no edge, and
  `cas/dependencies.rs:47-70` validates only `FinalizedObject::references()`. A save
  is therefore acknowledged while the Store lacks the objects its root names
  (reproduced with a control arm). Choose one: **(a)** declare a leaf's value roots in
  its `references()` so the existing dependency check covers them, or **(b)** write
  the exclusion into `admission-and-persistence.md` and add a target that saves a
  dangling supplied root and asserts the documented outcome. Either way, update
  `stage-5-report.md` so it no longer implies membership is guaranteed.
  Estimated +6 to +12 LOC.
- [ ] **R6 - refuse rather than lie when the listing byte bound cannot fit one row.**
  `filesystem/directory/read.rs:219-224` returns
  `Ok(entries = [], continuation = None)` for `max_bytes` 1..15, which is
  indistinguishable from an exhausted directory; the reference returns
  `ObjectLimitExceeded` (`crates/layerfs-content/src/tree/directory/read.rs:297-299`).
  Fix the guard, and make the covering test discriminating - today
  `tests/filesystem_read.rs:144-148` accepts `Err(_) | Ok(_)` and is what hid this.
  Estimated +3 LOC.
- [ ] **R8 - check effective cycles where they can still form.**
  `validate.rs:243-245` returns early when there is no base table, and
  `validate.rs:251-253` skips every changed child absent from the base, so a build
  with a disconnected directory cycle and an update cycling two declared-new
  directories are both accepted. Fix: do not skip a child in `declared_new`; seed the
  walk from that child's own `update_for` changes (`effective_entries` already merges
  changes onto an empty base). Add both shapes as cases. Estimated +8 to +14 LOC.
- [ ] **R14 - do not emit pages for a directory the same batch deletes.**
  `filesystem/update.rs:166-256` merges every supplied update before membership is
  known, and `:261-274` rewrites the content root, so `:287-311` releases a tree
  whose new pages were just emitted. Skip or defer updates for parents whose derived
  final count is zero - or state the deviation explicitly and test it.
- [ ] **R29 - refuse a build whose `root_serial` is not a declared new inode up
  front** instead of failing late with `MissingObject` from the placeholder
  zero-identity table (`filesystem/input.rs:159-168`, `validate.rs:71-76`).

### WP2 - Ordering resources and complexity

- [ ] **R3 - make the record-backed path linear and count its reads.**
  `references/runs.rs:236-252` builds a fresh `RunReader` and scans from offset 0 on
  every call; it is called per touched serial (`reduce.rs:154-163`), per
  not-yet-pending serial inside `entry` (`reduce.rs:218-233`) and per released child
  (`release.rs:67,133`), after `touched_serials` has consolidated to **one** run
  (`reduce.rs:170-171`). Because `find` takes `&self`, those reads can never be
  counted - `rows_read` only moves in `merge.rs:116` and `runs.rs:409`. Measured:
  3.65x/3.74x/3.83x per doubling against a 2.14x/2.16x/2.38x control. Fix: keep a
  monotonic per-tier cursor for the ascending sweep (reset on write) or index the run,
  and charge the reads you perform. Re-run the reviewer's four-point probe with the
  same inputs and put both series in your evidence. Estimated +25 to +40 LOC net.
- [ ] **R5 - make `ordering_bytes` bound what the operation actually owns.**
  `spill` takes older tiers out of `levels` (`runs.rs:194-198`) and
  `consolidate` empties them first (`:270-275`), while `reserve` (`:91-105`)
  computes from `run_bytes()` only, so the check never covers inputs plus output;
  only `FileBacking`'s 256 MiB capacity stops it, and `capacity_bytes` has no product
  caller. Keep merge inputs inside the checked set until their handles drop, read
  `capacity_bytes` once at entry and fail explicitly if it cannot cover the declared
  ceiling, and sample `peak_live_runs`/`peak_run_bytes` while inputs and output
  coexist. Add a resource case that fails when true simultaneous ownership exceeds the
  declared ceiling.
- [ ] **R30 - bound the touched-serial collection and batch the release frontier.**
  `reduce.rs:170-201` materialises every touched serial (8 bytes each, uncapped by any
  declared ceiling) and `release.rs:66-93,133` reads one serial per demand while the
  module documents "one bounded wave per page". Either batch the frontier into
  `lookup_many` waves or declare the vector's bound in `FilesystemResources`. Update
  the memory ledger in `content-io-memory-audit.md`.
- [ ] **R28 - charge validation work.** Validation reads and lookups are absent from
  `FilesystemUpdateCounters` (`update.rs:32-51`) and the cycle walk discards its own
  counters (`validate.rs:302,354`), although `filesystem-tree.md:156-157` requires
  them charged. Add the counters and one assertion.
- [ ] **R45 - bound the C2 read demand.** `cas/store.rs:199-229` and
  `filesystem/objects.rs:76` take the caller's whole slice with no count or byte cap;
  the 512 objects / 512 KiB figures are admission *triggers*, not caps. Add an explicit
  bound (caller-declared, like the other resources) and a test that exceeds it.

### WP3 - Enforcement and limits honesty

- [ ] **R9 - enforce the inode-serial range on every row.** `MAXIMUM_INODE_SERIAL`
  (`filesystem/identity.rs:20`) is checked only by `InodeIdentity::new`, whose sole
  caller is the root (`root.rs:64`); other sites check only `!= 0`
  (`input.rs:156,165`), so a non-root serial above `i64::MAX` is written into leaf
  bytes and the reference would reject the page. Range-check serials in
  `FilesystemInput::check` and in the binding loop.
- [ ] **R10 - enforce the 1 MiB attribute value bound on the write path.**
  `MAXIMUM_ATTRIBUTE_VALUE_BYTES` appears in exactly one place, the read default
  (`attributes/read.rs:166`). `emit_value` (`attributes/value.rs:18-24`) and
  `apply_patches` never check it. Add the check in `emit_value` so both routes are
  covered, and add a 1 MiB + 1 refusal case.
- [ ] **R13 - declare, charge and test the cycle-check work limit.**
  `validate.rs:24,306-308`: any changed binding whose existing-directory child has an
  effective subtree above 4,096 entries fails with a message indistinguishable from a
  real cycle - a legal large-directory rename is refused. Add it to the limits table,
  charge the work, and add a case that pins the behaviour. Consider reusing membership
  instead of re-listing.
- [ ] **R7 - close the schema-compatibility claim.** `sqlite/schema.rs:149-178`
  validates identity, `user_version`, column names/order, STRICT, indexes and the
  watermark but never the CHECK bodies, so a Store built with the previous
  `object_role BETWEEN 1 AND 6` is **accepted at open** and fails only at the first
  tree-role INSERT. Do one of: validate the role range at open, or commit the
  reproduction as a `layerfs-storage` test and document the deferred refusal as the
  contract. Reproduce with
  `evidence/stages-1-5-review-20260917T160000Z/diagnostics/make_old_schema_store.py`.
- [ ] **R22 - one row over the page ceiling.** `sorted/format.rs:292-298`:
  `page_items(0) = (8192 - 44)/11 + 1 = 741` where the real ceiling is 740. Reachable
  only through the public page encoder, where it surfaces as a size error rather than a
  partition error. Fix the arithmetic.

### WP4 - Composition, product boundary and simplification

- [ ] **R11 - collapse the duplicate leaf-page codec.**
  `object/inode_leaf.rs:189,239` and `sorted/format.rs:332,440` implement the same
  framing twice, already disagree on the error for one malformation, and
  `stage-5-handoff.md:104-112` explicitly says not to duplicate the leaf codec. The
  73-byte *value* codec is already shared and stays shared. Make one delegate to the
  other and add one equality test against the sealed two-row leaf bytes.
  **Estimated -90 to -150 LOC.**
- [ ] **R15 - delete the debug environment branches.** `std::env::var("LAYERFS_TRACE")`
  plus `eprintln!` at `filesystem/update.rs:197,215,312,332` - on the timed path,
  once per observed binding, with no consumer anywhere. Product source must not carry
  test-only instrumentation, and the boundary guard does not catch it.
  **Estimated -12 LOC.**
- [ ] **R21 - delete the dead surfaces.** `CheckedInput::removals` (never written),
  `declared_new`, `ReferenceReducer::is_touched`, `InodeIdentity::check_scope`,
  `MINIMUM_DIRECTORY_BRANCH_CHILDREN` (its own only occurrence), and the exported
  functions with zero callers: `content::object::id::authenticate`,
  `file::mapping::codec::chunk_canonical_len`,
  `storage::encoding::codec::group_body_parameters`, `sqlite::lookup::policy`
  (use `chunk_canonical_len` in the chunk encoder if that is its intent).
  **Estimated -60 to -90 LOC.**
- [ ] **R24 - check a declared tree role against the object's inner tag**, or state the
  precondition in the contract and add one negative test.
  `encoding/full.rs:46-74` validates only `WholeFile`/`Chunk`; tree roles go
  straight to `encode_full` (`delta/select.rs:205-222`) and the declared role is
  persisted (`cas/owner.rs:800`).
- [ ] **R41 - ship the Store-to-C1 provider bridge, or record the decision.**
  No `impl AuthenticatedObjects` exists in product source; the same adapter is
  re-implemented five times in excluded support
  (`layerfs-storage/tests/support/{mod.rs:158,filesystem.rs:369}`,
  `examples/{measure_components.rs:324,measure_edits.rs:611}`,
  `tests/filesystem_pipeline.rs:78`). A Stage 7 adapter needs it. Either export one
  small provider over `Store::read_batch` or record explicitly that it is adapter
  work.
- [ ] **R23 - derive the mapping profile identity from the named constants.**
  `file/mapping/codec.rs:33-47` hashes the literals 64/128/2/31/32768 instead of
  `MIN_ENTRIES`/`MAX_ENTRIES`/`MAX_LEVEL`/`cdc::MAXIMUM_CHUNK_BYTES`, so
  changing a partition constant would not move `profile_id()` and the frozen-fixture
  test would still pass.
- [ ] **R27 - make the profile self-describing or test its correspondence.**
  `root.rs:21` lists `scope32;serial8;inode81;leaf50-100;branch64-127;page8192;depth31;directory-fill2/5`
  as prose; nothing derives or tests it against `limits.rs` and the format constants,
  and the root-profile assertion is tautological after construction.
- [ ] **R31 - decide the `"new inode removal"` branch** (`reduce.rs:120-129`): it is
  reachable only from the release walk, is untested, and its label differs from the
  sibling `"new inode without binding"`. Decide, document, test.
- [ ] **R37/R39 - close the two items the previous review left open.** (a) C1
  navigation is one point provider call per visited page
  (`file/mapping/read.rs:190`, `filesystem/inode/read.rs:51`,
  `filesystem/directory/read.rs:285`), and each C2 read call re-creates a connection
  and a 1 MiB decode workspace (`cas/store.rs:205,210-212`): batch a level's demands
  and keep or size the workspace. (b) The provider trait takes no timing scope
  (`object/access.rs:18-20`), so C2 work inside a C1 read lands under the caller's
  span.
- [ ] **R46/R47 - decide and record the placement constraints.** The persistence seam
  is local-path-only (`cas/store.rs:92-143`) and the core is `!Send`
  (`SaveOperation` holds a raw zstd context; `FileBacking` is `Rc`-based;
  `TimingScope` is `!Send`). Either accept them in writing as adapter constraints
  or change them; do not leave them implicit.

### WP5 - Harness, oracle and measurement integrity

- [ ] **R4 - make the harness select and compose what it names.**
  `examples/measure_filesystem.rs:174-179` maps four case names to one fixture size,
  so `--mode pipeline` prints the identical root for all four; `--mode c1` always
  changes the first ten bindings. Map each `--case` to a real change set (or print the
  banner `measure_edits.rs:411-422` prints and rename the rows), and route the
  integrated row through `SaveHandoff` plus a **reopened Store** read-back - the
  pattern already exists at `examples/measure_components.rs:275-296`.
- [ ] **R17 - re-collect the component comparison at the frozen commit.** The receipt
  `evidence/stage-5-component-comparison-20260917T073017Z` records
  `source_commit 3b4941f1e` and runs the candidate arm as
  `--example filesystem_primitives_candidate`, which changed in `b22712844` after
  collection; under `AGENTS.md` §3.3 the pair is invalidated. Re-run
  `tools/stage5_component_comparison.py` at your commit into a **fresh** evidence
  directory, keep both arms' identities, and state the cache state and sample policy.
  One sample per case per arm.
- [ ] **R12 - stop the oracle rewriting itself.** `crates/layerfs-content/tests/stage5_reference_fixtures.rs:618-620`
  is a plain `#[test]` that overwrites `core/crates/layerfs-content/tests/fixtures/filesystem/`.
  Gate the writer (env var or `#[ignore]`) and keep a read-only comparator that fails
  when the seal changes. Re-prove the seal reproduces: the reviewer regenerated all 11
  files byte-identically in an isolated `git archive` copy - repeat that, never in place.
- [ ] **R18 - stop one trace node per accepted object.** `cas/store.rs:257-263`
  creates a child per `accept`, contradicting
  `admission-and-persistence.md:538` and capping useful detail near 500 objects.
  Move to one scope per batch or wave, or amend the contract with the owner's reason.
- [ ] **R35 - make the Stage 5 harnesses fail on a clipped report.**
  `examples/measure_filesystem.rs:285` and `examples/filesystem_timing_c1.rs:375`
  print only `report_nodes`; `measure_edits.rs:299-311` and
  `measure_pooled.rs:193-203` already consult `TimingReport::is_incomplete()`.
- [ ] **R36 - fix the `--mode c1` claim.** `stage-5-verification.md:84` says a
  C1 row opens no database, but `measure_filesystem.rs:183-191` creates a Store before
  the mode dispatch. Either correct the contract text or make the mode skip creation.
- [ ] **R38 - fix the C2 accounting and labels.** `framed_len` over-counts group
  framing by 4n-4 (`cas/owner.rs:358,377-382`); the save read path copies pending
  bytes twice (`cas/store.rs:327,363`); the inner scope named `storage.read` nests
  inside caller scopes also named `storage.read` at every in-repo call site; and
  `SaveOutcome.acknowledged` is hard-coded true (`cas/store.rs:60`) so an assertion
  on it cannot fail. Fix each or document why it stands.
- [ ] **R42 - do not count zero-test binaries as coverage.** Six of 58 targets are
  `unittests src/lib.rs` or doc-tests with no tests
  (`evidence/stages-1-5-review-20260917T160000Z/cargo-test.log`). State coverage in
  terms of tests that exist.

### WP6 - Evidence, documentation and accounting

- [ ] **R16 - re-derive the Stage 5 report's tables.** The per-file LOC column is stale
  in ten rows (`update.rs` 333 -> 359, `input.rs` 128 -> 135, `read.rs` 196 -> 191,
  `sorted/page.rs` 447 -> 449, `sorted/finish.rs` 154 -> 156,
  `references/record.rs` 117 -> 120, `references/backing.rs` 116 -> 208,
  `references/runs.rs` 247 -> 323, `references/merge.rs` 121 -> 123,
  `references/reduce.rs` 465 -> 472); the file count is **40**, not 24; and the
  above/below prose is **8 above / 17 below of 53**, not four and six. The **totals**
  are correct (11,160 -> 17,905, +6,745; reference 65,417) - only the detail is wrong.
  Use the reviewer's `loc-tables.md` as a cross-check, not as a substitute for
  re-running the counter.
- [ ] **R25 - correct the schema drift.** `content-storage-policy-and-tables.md:304-310`
  still says "19 columns" / "20 columns and `user_version = 2`"; the tree is **21
  columns at `user_version = 4`** (`sql/schema.sql:10-11,54`, `sqlite/schema.rs:22-58`).
- [ ] **R48 - correct the small documentation drifts.** `sqlite/write.rs:34` says the
  direct delta base is "always absent in this slice" while `cas/owner.rs:791` writes
  it; and the `stage-5-report.md` section 3D still says "88-byte record" where the
  grammar is 96 bytes (corrected only in a later section).
- [ ] **R19/R20 - make the backing honest.** After a failed removal, `release`
  zeroes `held` (`backing.rs:220-230`) so the accessors report clean while a run
  file remains; and the fixed run name (`backing.rs:179-191`) makes one stale file
  permanently fatal for every later backing in that directory. Fix the accessors and
  either tokenise the name or adopt/truncate an expected file.
- [ ] **R26/R33 - fill the negative and bound coverage.** Role-byte and flag negatives
  exist only for the inode leaf; add directory/inode branch roles, the root role and
  reserved byte, symlink role and flags, branch `subtree_bytes` and child-summary
  mismatches. Batch lookups have no cardinality cap of their own and no read-side bound
  test.
- [ ] **R32/R34 - cover and bound the read surface.** `read_portable`,
  `read_attribute` and `attribute_keys` have no caller anywhere: no test binds a
  real attribute tree to an inode and reads it through a path.
  `attribute_keys` accumulates every key with no count or byte bound, and
  `FilesystemReadWork` is consumed by nothing (resolve charges no directory work -
  measured `directory.pages_read = 0` after two stats).
- [ ] **R40 - document the interrupted-save recovery path.** A death between a
  mid-save COMMIT (`cas/owner.rs:837-843`) and the watermark transaction (`:904-908`)
  leaves every later save refused with `UninspectedState` and no product recovery
  path. This is contract-consistent; document the operator path and add a test that
  reads still work afterwards.
- [ ] **R43 - fix the Stage 4 case label or the fixture.** The case named
  `repartition-80-100` (`tests/edit_reference.rs:208-226`) has sealed base pages of
  89 and 90, not 80 and 100, because the base is canonically rebuilt; the exact 90+90
  result **is** asserted. Rename the case/report row or add a fixture whose base is the
  literal join.
- [ ] **R44 - get owner answers, do not assume waivers.** The Stages 3-4 pooled-lane/v5
  deviations (E2) are an **unanswered owner decision**, not a waiver; the 8 MiB-1
  deferred refusal (W10) is unrun; A2 item 6 is met for unclipped arms only. Ask the
  owner and record the answer; do not upgrade any of them.

### WP7 - Continuous: keep #170 honest until terminal pass

- [ ] **Update #170 at every milestone**, not once at the end. The protocol is in
  section 6.

## 5. Verification you must run and report

Run these on the tree you actually changed, and report every exit, failure and gap -
"CI green" and "the preflight passed" are forbidden claims, because neither exists.

    python3 core/tools/check_product_boundary.py
    python3 -m unittest discover -s core/tools -p 'test_*.py'
    python3 -m unittest discover -s tools -p 'test_production_loc.py'
    cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check     # fmt has no --locked
    cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked
    cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
    python3 tools/production_loc.py --files
    git diff --check

Plus, for this remediation specifically:

- the reviewer's diagnostic client, before and after, with both logs retained;
- the four-point ordering-growth probe re-run with the same inputs, so the quadratic
  term is shown gone rather than asserted;
- one new external target or case per WP1 item, asserting the fixed behaviour;
- the fixture seal re-proved by regeneration in an isolated `git archive` copy;
- the component comparison re-collected with identity-matched arms.

## 6. Issue protocol: iterate until terminal pass

1. **Start.** Comment on #170 with: the commit you start from, the work-package order
   you will follow, and the check commands you will use. Reopen is already done - do
   not close the issue at any point in this assignment.
2. **Per work package.** After each of WP1...WP6, post one comment containing: what
   changed (commit hashes), the exact before/after evidence paths, the checks run with
   exits, the production LOC line, and **the matrix rows you moved** (quote the row IDs
   from the review's section 4, e.g. `WT-4`, `OR-8`, `VF-5`) with their new
   status. If a row did not move, say so and why.
3. **Per failed attempt.** Post it. A failed or reverted attempt with its log is
   evidence; a silently dropped attempt is not.
4. **Honesty rules.** Never mark a checkbox that is not proven. Never promote a
   `NOT_RUN` or an owner-waived row. Never rewrite an earlier comment; add a dated
   correction instead. If you cannot close a line, say so and propose the disposition.
5. **Owner waivers.** Only the human owner can waive a line. Ask, then quote the answer
   verbatim in the comment. A waiver is never a measurement.
6. **Terminal pass.** Update the body's dated acceptance-status section (replace only
   the section between the `stage5-acceptance-status` markers), post a final comment,
   and stop. Do not close the issue - the owner closes it after re-review.

## 7. What counts as a terminal pass

All of the following, with evidence, is the finish line:

1. Every WP1 item fixed and demonstrated by a new case that fails on the parent commit
   and passes on yours.
2. Every WP2-WP6 item either fixed with fresh evidence, or explicitly owner-waived in
   writing and quoted in #170.
3. The Stage 5 matrix, re-derived by an **independent** reviewer or by you with the
   review's own row IDs, shows **zero FAIL and zero INCOMPLETE rows**; any row left
   `NOT_RUN` is named with its owner-approved reason.
4. The four previously failing acceptance bullets in #170's body can be ticked with
   evidence links, and the component comparison has an identity-matched receipt.
5. All section 5 checks run on the final tree with their exits recorded, production LOC
   reported per commit, and `git status` clean apart from your own evidence.
6. #170 carries a complete milestone history: every package reported, every failed
   attempt recorded, and the final acceptance-status section telling the truth about
   what is proven versus what is merely implemented.

Terminal pass is **not** "the tests are green", "the report says so", or "the reviewer
did not look". If something remains unproven at the end, say so plainly and leave the
issue open with the precise gap.

## 8. Out of scope - do not do these

- Do not implement Stage 6 qualification (#171): the complete-operation comparison,
  cold-cache rows, pack-footprint and whole-process memory belong there.
- Do not implement Stage 7 runtime integration (#172), FUSE, adapters, transports or
  any plugin registry.
- Do not add revision/history naming, retention, GC or a workspace quota - C1/C2
  deliberately provide filesystem roots, not a Workspace lifecycle.
- Do not delete, retire or modify the reference tree under `crates/`.
- Do not enable non-compact profile support or add a converter.
- Do not close #170, produce a release or tag, or change #166/#167/#168/#169/#171/#172.
- Do not touch the retained review evidence or another owner's concurrent work.
