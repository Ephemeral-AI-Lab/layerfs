# Stage 5 terminal handoff: drive #170 to a clean pass and close it

> **Status:** Active implementation routing, 2026-09-17. This document is the
> executable assignment for the next agent. It supersedes
> [`stage-5-continuation-handoff.md`](stage-5-continuation-handoff.md) and
> [`stage-5-remediation-wp4-wp7-handoff-20260917.md`](stage-5-remediation-wp4-wp7-handoff-20260917.md)
> for routing purposes; the earlier documents remain the historical record.
>
> **Terminal condition (the only definition of done in this document):**
> every Stage 5 criterion and every cumulative Stages 0-5 criterion reports
> **PASS**, with **no FAIL, no INCOMPLETE and no unowned row**; every `NOT_RUN` row
> is either resolved or carries a written owner disposition; a **fresh independent
> review** on the final tree reports **zero open findings**; the documents record
> the final state; and **#170 is closed**. Work iteratively until that holds.

---

## 1. Where you are starting

| Field | Value |
| --- | --- |
| Repository | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` |
| Branch / HEAD at handoff | `main` / `f288d2af7ecdc7e00f7df153073398d333461aa3` (pushed) |
| Issue | [#170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170) — **open**, not accepted |
| Cumulative issue | [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165) — open; carries the round-2 cumulative verdict in a comment |
| Governing review | [`stages-1-5-review-20260917T230700Z.md`](stages-1-5-review-20260917T230700Z.md) |
| Review evidence | [`../evidence/stages-1-5-review-20260917T230700Z/`](../evidence/stages-1-5-review-20260917T230700Z/) (30 files, manifest in its `README.md`) |
| Issue record | [#170 comment 5717192709](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170#issuecomment-5717192709) |
| Implementation contract | [`stage-5-handoff.md`](stage-5-handoff.md), [`stage-5-file-plan.md`](stage-5-file-plan.md), [`filesystem-tree.md`](filesystem-tree.md) |
| Measurement contract | [`stage-5-verification.md`](stage-5-verification.md), [`stage-5-verification-addendum-20260917.md`](stage-5-verification-addendum-20260917.md) |

Read first, in this order: [`AGENTS.md`](../../../../../AGENTS.md),
[`core/AGENTS.md`](../../../../core/AGENTS.md),
[`docs/general/benchmark_rules.md`](../../../general/benchmark_rules.md),
[`benchmark/AGENTS.md`](../../../../../benchmark/AGENTS.md),
[`benchmark/fs-bench-pro/QUICKSTART.md`](../../../../../benchmark/fs-bench-pro/QUICKSTART.md),
[`release-policy.md`](../../../general/release-policy.md),
[`documentation-policy.md`](../../../general/documentation-policy.md), then the
round-2 review's §1 findings and §10 actions.

The round-2 numbers you must move: **Stage 5 — 64 PASS / 9 FAIL / 8
PARTIAL-INCOMPLETE / 1 NOT_RUN / 1 NOT_APPLICABLE of 83**; **cumulative — 30 PASS /
3 FAIL / 1 PARTIAL / 2 owner-WAIVED of 36**. The two owner-WAIVED rows
(`S3-6`, `S4-5`) are the Stages 3-4 performance waivers: they stay waived, they are
not yours to re-open, and they must not be promoted into evidence.

## 2. The loop (repeat until the terminal condition holds)

```text
ROUND N
  1  pick the highest-severity open row from the ledger in section 4
  2  implement it, in the product or the harness or the document it actually lives in
  3  run the checks that cover YOUR change (section 6), plus the whole-core set
  4  write the round's evidence into an append-only dated directory under
     docs/roadmap/0.1/0.1.7/evidence/stage-5-terminal-<stamp>/
  5  post one comment on #170: what changed, the evidence path, the row deltas,
     and every command that failed or did not run
  6  request a fresh independent review of the delta (section 7)
  7  for every finding it returns: remedy it and go to 2
  8  when the ledger is green and a review returns zero findings, go to section 8
```

Rules that make the loop honest:

- **Never mark a row PASS yourself.** A row flips only when its required evidence
  exists and an independent reviewer, or a reader of the receipt, can reproduce it.
- **Never silently drop a row.** A row you cannot close is reported as FAIL or
  INCOMPLETE with its measured state, like every other row.
- **Never weaken an artifact to make a row pass** — no relaxed gate, no inflated
  timeout, no smaller workload, no deleted assertion, no re-sealed oracle, no
  rewritten receipt, no rewritten history.
- **One round, one comment.** The issue thread is the running state; make each
  comment readable without the previous ones.

## 3. Hard constraints

- Product source is `core/crates/*/src` only, product-only, **no inline tests**,
  **<= 999 physical lines per file**, **<= 200 for `lib.rs`/`mod.rs`**.
- **No** retry, busy handler, error-driven fallback, alternate backend, WAL, added
  crash-durability, or third-party patch/fork/vendoring. Unsupported required
  capabilities fail explicitly.
- SQLite stays MEMORY journal, `synchronous = OFF`, zero busy timeout. No
  `fsync`/`fdatasync`/`sync_all`/`sync_data` in any product or report path.
- **No** `tools/preflight.sh`, no CI, no aggregate gate, no replacement for either.
- One sample per case per arm unless a future owner decision says otherwise; fresh
  `--output` per run; append-only receipts; never overwrite a prior receipt,
  report or failed attempt; never reuse another owner's measurement.
- Keep unrelated work intact. Do not touch the root `crates/` reference tree
  except where this document says so.

## 4. The ledger: rows that must flip

Each row names the artifact that decides it and the evidence that flips it. Row ids
`R2-Fn` are the round-2 findings; matrix ids are the round-2 matrices.

| id | row | current | required evidence to flip it |
| --- | --- | --- | --- |
| `R2-F1` | `filesystem_timing_c1 --case attributes` times nothing | FAIL | the case applies a real attribute change inside the timed region, or is renamed, and the row's printed counters differ from `--case empty`; a receipt shows both runs |
| `R2-F2` | `measure_filesystem --mode c2` ignores `--case` | FAIL | a receipt shows six **distinct** `c2` rows, one per case, with the case banner matching what ran |
| `R2-F3` | `measure_edits --mode c2` times C1 encoding and a read | FAIL | `encode_whole_file` and the readback are outside the timed region, and the printed exclusion line is true; a receipt shows the command |
| `R2-F4` | six Stage-5 per-commit LOC disclosures do not reproduce | FAIL | a dated correction in `stage-5-report.md` §2 listing the six recomputed rows beside the disclosed ones, the combined -> core-only scope switch at `01d9f70f3`, and merge `a8a1ba848`'s missing line; a re-run of the audit reproduces both columns |
| `R2-F5` | the governing addendum cites an invalidated comparison receipt | FAIL | `stage-5-verification-addendum-20260917.md` §5 names a receipt whose `source_commit` is an ancestor of the tree the report describes, or the component comparison is explicitly withdrawn as evidence with an owner decision |
| `R2-F6` / `AT-4` | attribute value bound is 32,768 bytes, documented as 1 MiB | FAIL | either multi-chunk values exist and 1 MiB works, or the constant, `stage-5-report.md` §6, the matrix row and `tests/filesystem_failure.rs:562` all say 32,768; a boundary case proves 32,768 accepted / 32,769 refused |
| `R2-F7` / `VF-4` | the 4,096-entry whole-tree operation ceiling is undeclared | FAIL | either the ceiling is declared with both consequences (<= 4,095 bindings per build; an oversize directory cannot be rebound) or the charge becomes once-per-operation and the new figure is declared; the reproducer is attached |
| `R2-F8`, `R2-F9` / `N-6` | ordering work is superlinear and `LookupScan::settled` is dead | PARTIAL | `settled` removed or implemented; one `RunReader` per tier; a scaling receipt showing the amplification is no worse; the module's documentation matches the code |
| `N-11` / `R2-F21` | a read wave has a count ceiling but no byte ceiling | FAIL | a byte ceiling is enforced in both C1's boundary and C2's wave, with a refusal test, **or** the absence is written into the seam table as an explicit adapter obligation |
| `R2-F10` / `N-7` | every Store failure reaches C1 as `MissingObject` | PARTIAL | integrity failures are distinguishable from absence at the provider boundary, with a test |
| `R2-F11` / `N-8` | the declared transaction bound is a commit trigger | PARTIAL | the bound is enforced before accumulation, or the resource table states "commit trigger after the crossing write" |
| `R2-F12` / `N-9` | `is_incomplete()` is true for a disabled report | PARTIAL | disabled and clipped are distinguishable through the public API, with a test |
| `R2-F13` / `N-10` | a panicked child serialises as `Ok` | PARTIAL | a child that never finished reports a failed or unknown outcome, with a test |
| `R2-F14` | `measure_components` exits 0 on a clipped report | FAIL | it exits non-zero like the Stage-5 pair, with a receipt of a clipped run |
| `R2-F15` / `VF-7` | the report headline is one production commit stale | PARTIAL | `stage-5-report.md` §2 states the totals of the tree that carries it |
| `R2-F17` | mapping pages are encoded in root context, decoded in real context | PARTIAL | the encoder validates the context it is writing, with a two-sided test |
| `R2-F18` | C1 entry points never call `ConstructionPolicy::validated()` | PARTIAL | each entry point validates the policy, with a negative test, or `README.md` stops claiming it |
| `R2-F19` | the construction allocation comment is false | PARTIAL | the single-allocation cut is implemented, or the comment and the memory ledger are corrected |
| `R2-F20` | the base root is read and decoded more than once per edit | PARTIAL | the view supplies the decoded state, with a read-count assertion |
| `R2-F22` | `into_parts` drops advisory predecessors | PARTIAL | predecessors survive the conversion, with a test |
| `R2-F23` | `PRAGMA {name}` interpolates a caller string | FAIL | the pragma name is not caller-controlled |
| `R2-F24` | an acquired connection is not profile-verified | PARTIAL | the profile is re-verified on acquisition, with a test |
| `R2-F25` | the "1 MiB attribute value" test writes 4,096 bytes | PARTIAL | the case exercises the real boundary |
| `R2-F26` | two constants encode one scratch ceiling | PARTIAL | one constant, one name |
| `N-13` | `layerfs-storage` is not `forbid(unsafe_code)` | FAIL | either the zstd FFI surface is isolated behind a documented, minimal, audited module boundary with `forbid` elsewhere, or the deviation is recorded as an accepted design note with the FFI inventory |
| `N-16` | path/group/record/transaction/depth limits have no boundary test | FAIL | one boundary case per limit, or an explicit "not boundable in a test" note with the reason and the derived arithmetic |
| `N-17` | caller string interpolated into SQL | FAIL | same artifact as `R2-F23` |
| `TR-5` | simultaneous memory and backing costs | INCOMPLETE | a measured or explicitly scoped row: what is held at once, with scope, unit and the counter that produced it |
| `VF-5` | the required comparison against the pinned reference | PARTIAL | an identity-matched receipt at a commit an ancestor of the final tree, with the campaign contract frozen before collection |
| `VF-6` | complete-operation comparison | NOT_RUN | **owner disposition required**: either a frozen comparator that produces it, or a written waiver recorded in the completion report and on #170. It must not stay an orphan row |
| `VF-3` | meaningful assertions | PARTIAL | the non-discriminating cases are made discriminating |
| `VF-4` | limits evidence | FAIL | the limits table is complete, correct and boundary-tested |

## 5. Work packages

Ordered so that cheap, self-contained blockers land first and the measurement work
lands on a stable tree.

### WP-A - Harness case selection (`R2-F1`, `R2-F2`, `R2-F3`, `R2-F14`)

Files: `core/crates/layerfs-content/examples/filesystem_timing_c1.rs`,
`core/crates/layerfs-storage/examples/measure_filesystem.rs`,
`measure_edits.rs`, `measure_components.rs`, `measure_pooled.rs`,
`filesystem_primitives_candidate.rs`, `edit_timing_c1.rs`, `memory_ledger.rs`.

For **every** example, answer three questions in writing in the round's evidence:

1. Does `--case` actually select the operation the banner claims?
2. Is anything timed that the row's exclusion line says is excluded?
3. Does a clipped report fail the run?

Then fix what is wrong. Reproduce with a matrix run: every case x every mode, into
fresh `--output` directories, with the roots and counters compared.

### WP-B - Per-commit LOC honesty (`R2-F4`, `R2-F15`)

Do **not** rewrite history. Add a dated correction to `stage-5-report.md` §2.
Then make it impossible to recur: before each new commit, run the audited counter on
the first parent and on the staged tree, put the result in the message, commit, and
**re-run the counter on the committed tree** and compare. If they differ, amend the
message (not the tree) immediately.

### WP-C - Comparison governance (`R2-F5`, `VF-5`, `VF-6`)

1. Read `stage-5-verification-addendum-20260917.md` §2-§6 and
   `evidence/stage-5-component-comparison-20260917T143008Z/README.md`.
2. Decide and record: which collection governs, and at which commit.
3. If a new collection is taken, freeze the contract **before** collecting, then
   collect once per case per arm with the identity gate and the budget gate.
4. Investigate the 1.85x/2.3x spread between the two existing collections before
   quoting any ratio: the older README attributes the first case's wall time to a
   cold Cargo build, which would explain the reference-arm drift. Until that is
   explained, **no ratio is a stable absolute**.
5. Put `VF-6` to the owner with three explicit dispositions: run it, waive it in
   writing, or withdraw the requirement from Stage 5 and name the stage that owns
   it. Do not self-waive.

### WP-D - Attribute-value limit (`R2-F6`, `AT-4`, `R2-F25`)

Decide between **chunking** (keep the 1 MiB promise: split a value into
`ceil(n / 32,768)` chunk objects under the existing mapping grammar, keeping the
representation extent-only and the read bounded) and **correcting** (set the
constant to 32,768 and fix `stage-5-report.md` §6, the matrix row, and the test).
Chunking is the larger change and the one that preserves the advertised contract;
correcting is honest and cheap. Either way, add a boundary case through
`FilesystemObjects` and prove the read path returns exactly what was written.

### WP-E - Whole-tree operation ceiling (`R2-F7`, `VF-4`)

Decide between **declaring** and **re-charging**. Declaring costs a table row and
two sentences. Re-charging means hoisting the walk counter to the operation, so an
operation with many directory rebindings cannot spend N x 4,096 entries, which
also raises the practical build ceiling above 4,095 bindings. If you re-charge,
re-run `filesystem_bounds`, `filesystem_topology` and `filesystem_failure`, state
the new ceiling, and attach the reproducer.

### WP-F - Ordering work (`R2-F8`, `R2-F9`, `N-6`)

Delete or implement `LookupScan::settled`; keep one `RunReader` (and its buffer)
per tier instead of rebuilding it per lookup. Then re-measure the scaling with an
external client and record: rows spilled, rows read, rows written, runs, peak owned
bytes, elapsed, for at least four change sizes at a fixed pending ceiling. State
the amplification and the per-doubling ratio. If the ratio is still superlinear,
say so and give the reason (tiered merge is O(n log n)); do not claim O(changes).

### WP-G - Read-wave byte ceiling (`N-11`, `R2-F21`)

Prefer enforcing: a byte ceiling beside the object ceiling in
`FilesystemObjects::read_batch` and in C2's wave, with a refusal test. If
enforcing would change caller-visible behaviour in a way that needs a contract
change, write the absence into `content-io.md` and the seam table as an explicit
adapter obligation, with the arithmetic for what a 4,096-object wave can carry.

### WP-H - Telemetry semantics (`R2-F12`, `R2-F13`, `R2-F14`)

Make disabled and clipped distinguishable; make a never-finished child report a
failed or unknown outcome; make every harness fail on a clipped report. One test
each, and update the crate's own pinned test if it asserts the old behaviour
deliberately — say in the commit message that you are changing a pinned behaviour
and why.

### WP-I - C1 contract corrections (`R2-F17`-`R2-F22`, `R2-F26`, README drift)

Encoder context; entry-point policy validation; the construction allocation comment
and the memory ledger; one read of the base root per edit; `into_parts` and
predecessors; `READ_WAVE_BYTES` enforcement or relabelling; the duplicated scratch
constant; dead `slice_of`; and the stale `README.md` lines the round-2 audit
listed. Each is small; do them as one focused commit series with one test per
behaviour change.

### WP-J - Limits evidence (`N-16`, `VF-4`)

One boundary case per limit that can be bounded cheaply: path bytes (4,096/4,097),
path components (256/257), name bytes (255/256), the storage group/record/
transaction constants where reachable, `MAXIMUM_LEVELS`, and a tree deeper than
the two levels every current test builds. For limits that cannot be reached with a
practical fixture, record the derived arithmetic and mark the row "derived,
unverified at scale" with the reason. Delete every claim that a limit is
"enforced" when no check enforces it.

### WP-K - Measurement rows (`TR-5`, `VF-5`, `VF-6`)

Whichever of these the owner disposition keeps in Stage 5: freeze the contract
first, collect once, keep the receipt, and state scope/unit/basis for every number.
A source-derived budget is not a measured RSS. A lifetime high-water mark is not a
phase peak. A configured ceiling is not observed use.

### WP-L - Documents and issue (`VF-7`, every round)

Per round: evidence directory, `#170` comment, and the document that owns the
changed claim. At the end: the completion report, the roadmap index, the
`component-decoupling` index, and the closure comment.

## 6. Checks to run for every round

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
python3 -m unittest discover -s tools -p 'test_production_loc.py'
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
python3 tools/production_loc.py --files
git diff --check
```

Report every exit code, every failed or not-run command, and the discovered
target/test counts. `fmt` has no `--locked` flag. Never claim CI is green.

Measurement rounds additionally require: a declared cache state enforced equally in
both arms, a fresh `--output` path, one sample per case per arm, the measurement
lock respected, and the complete command inside the ordinary budget.

## 7. Independent review per round

Do not review your own fix. For each round, hand the delta to a **fresh** reviewer
with this instruction:

> Review the change at `<commit>` against the round-2 ledger row(s) it claims to
> close. Reproduce the row's required evidence yourself through public entry points.
> Report PASS/FAIL/INCOMPLETE per row with `path:line` and the command you ran. Do
> not trust the implementation report. Write your review beside the others and
> retain your evidence under an append-only dated directory. Do not fix anything,
> do not commit, do not change issue state.

A round is closed only when the reviewer returns zero open findings for that row.

## 8. Terminal checklist and closure

Close #170 only when **all** of the following hold on one identified tree:

1. All eight checks in section 6 exit 0, with the test counts recorded.
2. The Stage 5 matrix reports **0 FAIL, 0 INCOMPLETE, 0 unowned**; every `NOT_RUN`
   has a written owner disposition; the two `owner-WAIVED` Stages 3-4 rows are
   unchanged and are not counted as PASS.
3. The cumulative matrix reports the same.
4. Every limits-table row is correct, has a class, and either has a boundary case or
   is labelled derived/unverified with its arithmetic.
5. Every `--case` in every example selects the operation it names, and a
   case-selection receipt proves it.
6. Every commit in the Stage 5 range discloses a production LOC that the audited
   counter reproduces on the committed tree.
7. The comparison governance is decided, and the governing document cites the
   receipt that is actually eligible.
8. A **closing independent review** on the final tree reports zero open findings.
9. The completion report states the final totals, the closed rows, and every row
   that remains unmeasured with its reason.
10. `#170` is closed with a final comment naming the closing review, its evidence
    directory, the final HEAD and the exact matrices. `#165` gets a one-line
    pointer. Nothing is tagged or released by this handoff.

Then, and only then, is Stage 5 done. Stage 6 (#171) owns whole-core qualification
and Stage 7 (#172) owns runtime integration; do not start either.

## 9. Anti-patterns that will fail the review

- Closing a row because the code "obviously" does it, without a receipt.
- Marking a row PASS from a smoke run, a passing suite, or an attractive interface.
- Moving work outside a timer, enlarging a timeout, shrinking a workload, or
  changing worker counts to turn a number green.
- Re-sealing the reference oracle, rewriting a receipt, or quoting a diagnostic as
  qualification evidence.
- Rewriting commits or force-pushing to make a disclosure match.
- Promoting a waived or `NOT_RUN` row into evidence.
- Self-waiving a criterion. A waiver needs the owner, in writing, on the issue.
- Declaring "all passed" while any row is FAIL, INCOMPLETE or unowned.

## 10. If you cannot finish

Stop and post to #170 with: the row, the exact failing artifact, the command and its
output, the constraint that blocks it, and the three dispositions you need from the
owner (extend scope, waive in writing, or change the contract). An honest
INCOMPLETE with a reason is an acceptable round outcome. A green-looking ledger with
an unresolved row is not.
