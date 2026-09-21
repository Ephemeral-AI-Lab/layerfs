# Pair 2 history remediation specification

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Issue: [#210](https://github.com/Ephemeral-AI-Lab/layerfs/issues/210).
Implementation PR: [#212](https://github.com/Ephemeral-AI-Lab/layerfs/pull/212).
Design parent: [#180](https://github.com/Ephemeral-AI-Lab/layerfs/issues/180).
Owner direction, 2026-09-21: create the remediation specification, update #210,
and start a separate Codex task to implement it.

## 1. Authority, baseline and outcome

The governing contract is the [implementation specification](https://github.com/Ephemeral-AI-Lab/layerfs/blob/c85cf6b69b3809d860caaad764a09e86a54ece9a/core/docs/architecture/proposal/commit-history/implementation.md)
and its [pre-publication audit](https://github.com/Ephemeral-AI-Lab/layerfs/blob/c85cf6b69b3809d860caaad764a09e86a54ece9a/core/docs/architecture/proposal/commit-history/review-20260921.md),
both pinned at `c85cf6b69b3809d860caaad764a09e86a54ece9a`.
Those proposal files are on unmerged PR #211, not on the reviewed implementation
branch. Read the pinned Git objects; do not substitute a broken relative link or
the implementation author's description for the contract.

The independent implementation review pinned
`92e56635ae4559d175fe3cd455f36f9fe6b5b498` on
`codex/pair2-history-implementation`, with a clean worktree before and after the
review. Its audited implementation baseline was
`a02168adbb1b02571941654919cefca12dbc1f42`. Verdict: **not ready**.

Restore the specified behavior, preserve the working C1/C2 and metadata
transition invariants, and produce fresh evidence at the remediated commit.
This plan does not waive an acceptance case or convert an unrun case to PASS.
The [independent-review evidence](../../../../../docs/roadmap/0.1/0.1.7/evidence/issue210-independent-review-20260921/README.md)
contains counterexamples, raw results and their interpretation.

The [architecture description](../../16-history.md) follows the resulting source.
The contract wins when that description or a historical handoff contradicts it.

## 2. Boundaries that remain fixed

- C5 remains independent of C2 SQL, packs, saves and encoders. Seven C5 tables;
  C2 schema 7 and its public content/save contracts remain unchanged.
- Service admission W=2, two private C2 saves per Store, Q=0, one construction
  producer per ordinary operation; preserve the existing namespace-init exception.
- No C5 guard or transaction across C1 construction, upload or C2 finish. No
  nested C2 arbitration/C5 locks. Metadata-only operations start no C2 save.
- Stage only after that exact operation's known successful finish. Another
  writer publishing identical bytes cannot provide acknowledgement for it.
- Preserve exact-stage retention, no-change multiplicity, same-stack FKs,
  immutable equality including Layer provenance, historical-Commit fork bases,
  and unconditional consumption of exposed reservations.
- MEMORY journal, synchronous OFF, no WAL/fsync, no crash-atomic cross-store
  claim, no writable restart without continuity, no automatic retry or repair.
- Preserve all valid profile-1/opcode-1–5 bytes, numeric codes, bounds and
  delivery behavior. History keeps grouped opcodes 6/7 and explicit Store-wide
  grants. Do not imply per-Branch ACLs.
- Keep 128 records/16 KiB history pages, 160-byte maximum cursors, 128 manifest
  entries/32 KiB metadata, and the 4,096-row membership work ceiling.
- No benchmarks, performance campaign, retuned workers/timeouts/cache policy,
  product fault hooks, injected sleeps, alternate test algorithms, new third-party
  dependencies, patches, vendoring or registry edits. Builds stay locked/offline.
- Follow [repository rules](../../../../../AGENTS.md) and
  [core rules](../../../../AGENTS.md): external tests, 999 physical lines per
  production file, 200 declaration/delegation lines per lib.rs/mod.rs, exact
  per-commit production LOC, no CI/preflight/aggregate gate.

## 3. Work packages and acceptance

Treat these as bounded work packages, not permission to refactor unrelated code.
Read every caller before changing a shared contract. Reproduce a finding first;
if new evidence refutes it, record the exact source/input and corrected verdict.
Never silently drop the finding or rewrite the historical receipt.

### P0 — Freeze the repaired history boundary

Before dependent edits, record the exact profile-2 failure/context grammar,
root-descriptor fields and cursor version/key source in a short contract note
beside this specification. The implementation can choose the smallest coherent
representation, subject to the following requirements:

- Retain typed Branch-moved, stack-moved and stage-changed expected/actual
  identities and known exact stage context. Do not infer context later by reading
  mutable rows or by parsing error messages.
- A known retained stage, a previously acknowledged stage whose final disposition
  is unknown, and an absent stage are distinct facts. Composite Commit failures
  must expose the distinction without guessing whether a stage survived.
- Keep legacy failure frames byte-identical. Select history decoding from the
  already validated request/profile; reject incompatible shapes. No downgrade,
  resend, replay, or silent acceptance of obsolete history cursor encodings.
- Root descriptors include the validated root, profile, scope and root serial.
- Cursor integrity must withstand recomputation of an unkeyed public checksum.
  Use the existing BLAKE3 capability with an explicit authority-supplied secret
  if authentication is needed; catalog IDs, incarnation and public body bytes
  are not secrets. No C5 clock/PID/random fallback or process-local cursor registry.
  The key lifecycle must support the selected read-only-reopen envelope; lost
  capability is an explicit refusal. Secrets must not appear in receipts or replies.
- Preserve the existing C5 DDL if possible. Any required DDL change needs an
  explicit version/open rule; never edit schema 1 and silently accept old files.

### P1 — Error context and unknown outcomes (R01, R07)

Files: service `operation/history.rs`; history `error.rs`, transition bodies and
`sqlite/rows.rs`; bridge failure contract/codecs and native client/server call sites.

1. Carry the P0 typed context from the deciding transaction through direct service
   and native transport. BranchMoved, StackMoved and StageChanged must remain
   distinguishable even if their outer wire class is shared.
2. Separate definite refusal, definite success and unknown persistence. Do not
   unconditionally rollback an error already classified UnknownOutcome. Inspect
   rusqlite's actual transaction drop behavior and prevent an implicit guessed
   rollback on that path using its supported APIs, without modifying the crate.
3. Quarantine an uncertain catalog/provider so later calls cannot reuse it as a
   healthy writer or expose pending state as committed. Preserve bounded resource
   ownership. Do not claim rollback succeeded if cleanup itself is uncertain.
4. Keep known C2 success intact if C5 subsequently fails; no content deletion or
   guessed stage discard. Preserve C2's existing exact-operation quarantine.

Acceptance: direct and native stale-head and wrong-token replies contain exact
typed context and leave the original stage untouched; a composite Commit refusal
reports its acknowledged/retained stage accurately; profile-1 failures remain
byte-identical; source and available external failure tests establish the unknown
disposition including RAII cleanup. Unexposable real I/O/connection-loss schedules
remain NOT_RUN with the precise missing observation, not simulated by product hooks.

### P2 — Page codec bounds and semantic classification (R03, R12, R13, R14)

Files: bridge `contract/{request,history,outcome}.rs`,
`adapters/native/protocol/{metadata,response}.rs`, `adapters/native/client.rs`.

- Compute count pre-checks from legal minimum encodings, not typical/maximal
  records. Baseline reply minima: Stack 117, Branch 71 without a head / 104 with
  a head, Commit 116, genesis Layer 85, Stage 309. Recompute affected values after P0.
- Enforce the 16 KiB history-result budget in both encoding and decoding, including
  tags, counts, length prefixes and continuation bytes, before response allocation.
  Preserve the separate 32 KiB request envelope and all legacy limits.
- Classify InitLayerStack, StageChanges and composite Commit as content mutations;
  classify CommitStaged, Fork, AddLayer, DiscardStage and ReserveInodes as metadata
  mutations. Keep actual metadata commands outside the save path.
- Restore ResultData rejection for every operation other than ReadFile before
  writing any bytes to the caller. Match every terminal reply shape to its request.

Acceptance: minimal and maximal records of all five types round-trip; a one-record
headless Branch page and a genesis-only Layer page succeed; exact-budget and
over-budget replies differ correctly; malformed/truncated/trailing/count inputs
fail before allocation. An authenticated peer sending mutation ResultData produces
a delivery failure and an untouched output buffer, for both legacy and history.

The baseline catalog's maximal widths are correct: 179/166/149/168/342 bytes for
Stack/Branch/Commit/Layer/Stage. Do not reduce them to the decoder minima. Recompute
both minima and maxima after changing a wire record, and assert encoded lengths.

### P3 — Authenticated, stable, bounded continuations (R02, R04, R05)

Files: history `sqlite/{query,commit,layerstack,branch,staging}.rs` and cursor
configuration/adapter wiring required by P0.

- Represent every legal 1–63-byte name within the existing 160-byte cursor cap.
  Prefer a bounded immutable record identity with a checked ordering-key lookup
  over enlarging all fields or shrinking the allowed name grammar.
- Bind catalog, incarnation, range, subject, immutable anchor and last delivered
  position. A caller recomputing the old public digest must not forge a valid
  sibling-chain position. Validate the position's relationship to its anchor.
- Resume from the authenticated anchor when a continuation is supplied and
  `start=None`. An explicit start must agree with that anchor. Do not replace it
  with the live head or repeatedly walk an ever-growing live head to re-prove a
  previously established immutable anchor.
- Every range resumes strictly after its last delivered record. No repetition,
  omission or invalidation merely because the live Branch/stack advances. Preserve
  Capacity/unproven, never NotFound, at the membership work ceiling.

Acceptance: names of 1, 33, 34 and 63 bytes paginate; query the same Branch/stack,
advance that actual subject, then resume with both implicit and explicit anchors;
walk Commit and Layer chains across all pages without duplicates/skips; reject
changed range/subject/catalog/incarnation/anchor/position, including a recomputed
public checksum and a sibling Commit position. Exercise read-only reopen with the
declared cursor capability and missing/wrong-capability refusal. Empty histories
must still validate malformed page/cursor inputs before reporting an empty page.

### P4 — Manifest directories and validated root descriptors (R06, R08)

Files: service `operation/{history,history_bootstrap,dispatch}.rs`, bridge snapshot
DTO/codecs and their consumers. Reuse public C1 filesystem/role validators.

- Create one DirectoryUpdate per declared directory, including every empty child.
  Group child bindings by parent independently of declaration adjacency; sort once
  and preserve duplicate-name, parent-kind, acyclicity and canonical-name checks.
- Keep one consumed C5 reservation for every manifest serial and the existing two
  save ordering: prerequisite objects finish before filesystem construction reads
  them; filesystem save finishes before catalog initialization.
- For read_branch, capture a coherent C5 snapshot and release its lock, then open
  that immutable filesystem root through C1. Validate its role/profile/scope and
  root serial, and return the P0 descriptor. Do not reread the moving Branch to
  manufacture a different context. No C2 save is needed for this read.
- Supply descriptors required by the original initialization contract from the
  known C1 construction result, without inventing serial 1. Handle shared DTOs
  consistently without converting a known metadata publication into a claimed abort.

Acceptance: root only; root plus empty directory; nested empty directories;
interleaved valid parent declarations; file/symlink mixtures; invalid parent kind,
duplicate name and dangling/wrong-role roots. Validate missing/wrong-role/profile/
scope roots on read_branch and return a non-1 root serial after earlier scope
reservations. Preserve bounded attribute/file/symlink role checks on staged values.

### P5 — Catalog open and stale publication (R09, R10)

Files: history `sqlite/{open,layerstack}.rs`, schema validation and external tests.

- Validate the actual schema against the frozen columns, STRICT/table properties,
  indexes, constraints and foreign keys, not only seven table names. Reject an
  incomplete/foreign/unsupported schema explicitly; do not repair it or alter C2.
  Check stored binding_key/catalog identity consistency and declared metadata ranges.
- Preserve exact-source UpToDate precedence. For a new publication require the
  selected Commit base, Branch base and current/expected stack head to agree before
  NoChanges or insertion. A refreshed expected token does not rebase content.
- Preserve immutable-record/provenance validation. Do not overwrite a conflicting
  derived identity or replace a normal stale-base refusal with a SQL Integrity error.

Acceptance: independently mutate/remove an index, FK/CHECK/table definition or
binding field in a disposable external catalog fixture and show open refusal;
valid read-only reopen still works and never gains write authority. After a Layer
publication, both a new-root Commit and a base-equal-root Commit from the old-base
Branch receive a typed stale-base/head refusal despite a refreshed stack token.
The exact earlier publication remains idempotent after subsequent stack advancement.

### P6 — Terminal reservations (R11)

Files: history `sqlite/allocation.rs`, `records.rs`, service/bootstrap and wire
range validation where they consume reservations.

Under the selected checked-refusal endpoint policy, validate the exclusive end
before committing the allocation. Every successful reservation must satisfy
`reservation.end() == start + count` without failure; reject an unrepresentable
terminal range atomically before exposure. Keep serials inside `1..=i64::MAX`,
bounded counts, unconditional consumption after success, and no recycling.

Acceptance uses the actual catalog allocator with externally prepared high-water
fixtures near `i64::MAX`, not only a manually constructed Reservation. Check the
last accepted range, first refused range, unchanged high-water after refusal,
concurrent callers, and consumption after later failure/discard.

### P7 — Evidence, documentation and qualification

- Update architecture paper 16 alongside affected source. State the actual source
  pin, repaired formats/bounds, remaining continuity/portability limits and any
  candidate history API compatibility change. Do not edit historical receipts.
- Add a new handoff/ledger entry that maps R01–R14 to fixes/tests/source commits
  and every H01–H14 row to an honest result. A green named test is not proof of a
  scenario it never exercises. R15 is a baseline fixed-width `expect`, not a new
  reachable-panic finding; record its disposition without a repository-wide sweep.
- Re-run the actual host daemon route with freshly built matched binaries and a
  fresh output directory. Attempt the Linux route with a source-matched image if
  the available toolchain/runtime can supply one; record identities. Linux targets
  were installed during review, so do not repeat a stale missing-target explanation.
- H04, H06's independent-stack overlap, H08 boundary/identical-root failure
  schedules, and H14 component substitution need their actual observations.
  Reuse the existing external harness where suitable. If a schedule/capability
  cannot be established without prohibited hooks or scope changes, leave that
  exact cell NOT_RUN/UNVERIFIED, explain the blocker and continue independent fixes.
  Do not call full M4/H01–H14 qualified with those cells missing.
- Obtain an independent review of the repaired head before claiming M5 complete.
  Implementation self-review does not count as that independent review.

## 4. Finding traceability

| Review ID | Baseline defect or qualification | Required package |
| --- | --- | --- |
| R01 | Typed conflict/known-stage context erased by service failure mapping | P0, P1 |
| R02 | Rehashed cursor returns sibling Branch ancestry | P0, P3 |
| R03 | Legal short Branch/genesis pages fail the 120-byte count pre-check | P2 |
| R04 | 34–63-byte names cannot fit 33-byte cursor fields | P3 |
| R05 | Live subject growth invalidates a start=None continuation | P3 |
| R06 | Empty child directories and interleaved parent groups fail bootstrap | P4 |
| R07 | Unknown C5 errors enter rollback paths and leave reusable provider state | P1 |
| R08 | GetBranch skips C1 descriptor validation and omits root serial | P0, P4 |
| R09 | Reopen accepts missing constraints and inconsistent stored binding | P5 |
| R10 | Refreshed stack token yields Integrity/NoChanges for a stale Branch base | P5 |
| R11 | Catalog acknowledges a reservation whose end() refuses | P6 |
| R12 | Codec accepts a 22,918-byte history page | P2 |
| R13 | Content-writing history commands classified metadata-only | P2 |
| R14 | Legacy mutation ResultData written before terminal failure | P2 |
| R15 | Pre-existing fixed-width expect in service read helper | P7: baseline disposition |

## 5. Execution and verification

Work in an isolated checkout based on the reviewed implementation, retaining the
original review worktree untouched. Import this documentation commit before
implementation. Use a separate `codex/` remediation branch; do not force-push,
merge PR #212 or close #210/#179/#180/#192/#193/#209. Prepare a reviewable follow-up
PR against the implementation branch and link it to #210, unless the owner later
directs a different integration route.

Recommended dependency order: P0, then P1/P2/P3, P4/P5/P6, then P7. Shared wire
types must have one coherent owner; no speculative provider framework or generic
SQL extension. Port the counterexamples into the normal external test locations
as assertions of the repaired behavior. The archived probes assert the old defects
and must not be copied unchanged as acceptance tests.

Run focused external regressions while implementing. At the final remediated
head, run these explicit checks from `core/` and retain exact commands/results:

```sh
cargo +1.85.1 test --offline --locked --workspace
cargo +1.85.1 build --offline --locked --workspace --examples
cargo +1.85.1 fmt --manifest-path Cargo.toml --all -- --check
cargo +1.85.1 clippy --offline --locked --workspace --all-targets -- -D warnings
cargo +1.85.1 build --offline --locked -p layerfs-history --no-default-features
python3 tools/check_product_boundary.py
python3 -m unittest discover -s tools -p 'test_*.py'
cargo +1.85.1 build --offline --locked --workspace --bins
python3 crates/layerfs-daemon/tests/history_route.py --output <fresh-directory>
```

No aggregate wrapper or preflight. Every build uses the current worktree's target
directory. Record the head before/after checks and preserve all failing attempts.
For each commit, compare its first parent and exact staged tree with the same
`tools/production_loc.py --root <tree> --detail` counter; confirm the committed
tree and record core/reference/combined totals, signed delta and method. The
reviewed totals are core 30,248 + reference 65,417 = 95,665 production LOC; these
are a starting comparison, not substitutes for recounting each new commit.

## 6. Exit conditions

The code-remediation report requires a justified disposition for every R row,
passing regressions for each repaired defect, full current-head checks, source-
accurate architecture documentation, per-commit LOC and an append-only handoff.
Report code remediation separately from qualification. Remaining unrun H cases
keep full acceptance incomplete; no self-authorized waiver or fabricated schedule.

Final handoff states the exact commit, fixes, command results, remaining H-case
gaps, compatibility/format consequences and follow-up PR. It makes no performance,
release, writable-restart, cross-store crash-atomicity or non-Unix persistence claim.
