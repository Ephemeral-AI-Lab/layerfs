# Stage 5 terminal handoff: drive #170 to a clean pass and close it

> **Closure record (2026-09-18):** the terminal condition was met on the round-4
> tree; [#170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170) is closed.
> The final matrices, the verification-subagent outcomes and the unmeasured rows
> with their reasons are
> [`stage-5-report.md` §16](stage-5-report.md#16-final-matrices-and-the-verification-pass-round-4-2026-09-18);
> the evidence is
> [`../evidence/stage-5-terminal-20260918T120000Z/`](../evidence/stage-5-terminal-20260918T120000Z/).
> This document is now the historical record of the loop, the ledger and the
> terminal checklist; it is not changed by the closure beyond this pointer.

> **Status:** Active implementation routing, 2026-09-17. This document is the
> executable assignment for the next agent. It supersedes
> [`stage-5-continuation-handoff.md`](stage-5-continuation-handoff.md) and
> [`stage-5-remediation-wp4-wp7-handoff-20260917.md`](stage-5-remediation-wp4-wp7-handoff-20260917.md)
> for routing purposes; the earlier documents remain the historical record.
>
> **Terminal condition (the only definition of done in this document):**
> every Stage 5 criterion and every cumulative Stages 0-5 criterion reports
> **PASS**, with **no FAIL, no INCOMPLETE and no unowned row**; every `NOT_RUN` row
> is either resolved or carries a written owner disposition; a **fresh
> verification-subagent pass** on the final tree reports **zero open findings**; the
> documents record the final state; and **#170 is closed**. Work iteratively until
> that holds.
>
> **How you work:** you are a single main agent that launches subagents. There is no
> second reviewer tool, no separate reviewer agent and no human in the loop. You
> write, you launch read-only verification subagents with fresh context, and you
> adjudicate what they return (section 2, section 7).

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

## 2. How you work: one main agent, many subagents

You are the **main agent**, and you are the only thing that persists across the
whole job. Everything else is a subagent you launch for one bounded purpose and
then discard. There is **no external reviewer** — not another coding agent, not
another tool, not a person — and you must not write the routing as if one exists.

Three roles, all filled by you at different moments:

| role | who | rules |
| --- | --- | --- |
| **writer** | you, or one subagent you give an exclusive file slice | exactly **one writer at a time** on the repository. A writing subagent never commits, never pushes, never changes issue state, and reports every file it touched |
| **verifier** | a subagent you launch with fresh context (section 7) | **read-only on all product, test, fixture and harness source.** It writes exactly one evidence file in the round's evidence directory and returns a bounded summary. A verifier must never be the writer of the code it verifies |
| **adjudicator** | you | you re-read every finding at its `path:line` and reproduce it yourself where you can, **before** acting on it. Subagents are wrong in both directions; an unreproduced finding is not a finding |

**What a subagent prompt must contain.** A subagent does not see your conversation,
so every launch is self-contained: the repository path and the frozen commit; the
exact ledger row it is verifying; the artifact that decides that row; the command
to run; the single output path it may write; the read-only constraint; the rule that
a report, a passing suite or an attractive interface is not evidence; and the
instruction to return a short summary with `path:line` citations plus anything it
could not verify. Launch independent verifications **together** in one message so
they run concurrently, and keep working on the next row while they run.

**Serialization rules that matter more than parallelism.**

- **One writer.** Never let two agents edit the repository at once. If you delegate
  implementation, give the subagent a disjoint file set, keep the commit and the
  push for yourself, and start no second writer until the first has stopped.
- **One measurement at a time.** Resource-sensitive work — any timed sample, any
  build-then-measure pair — never overlaps another agent's measurement or a build
  that would warm a cache a timed phase reads. Declare the cache state, run it
  alone, record the wall time.
- **Disjoint outputs.** Every subagent writes its own file; two agents never write
  the same evidence path.
- **Preserve concurrent work.** Never interrupt another owner's run, never rewrite
  someone else's receipt, never commit work you did not author without being told to.

### The loop (repeat until the terminal condition holds)

```text
ROUND N
  1  pick the highest-severity open row from the ledger in section 4
  2  implement it yourself, or hand one subagent a disjoint slice of it
  3  run the checks that cover the change (section 6), plus the whole-core set
  4  write the round's evidence into an append-only dated directory under
     docs/roadmap/0.1/0.1.7/evidence/stage-5-terminal-<stamp>/
  5  launch verification subagents (section 7): one per claimed row, read-only,
     fresh context, launched together, each writing its own evidence file
  6  adjudicate: re-read every finding at its path:line, reproduce it yourself
     where you can, remedy what is real, and go to 2
  7  post one comment on #170: what changed, the evidence path, the row deltas,
     the verifier outcomes, and every command that failed or did not run
  8  when the ledger is green and the verifiers return zero open findings on the
     final tree, go to section 8
```

Rules that make the loop honest:

- **Never mark a row PASS on your own word.** A row flips only when its required
  evidence exists and a subagent that did not write the code — or any reader of the
  receipt — can reproduce it.
- **Never count an assertion as evidence.** "The suite passes", "the code obviously
  does it", "the reviewer said so" are not receipts. A receipt is a command, its
  output, and the identity of the tree it ran on.
- **Never accept a finding without reading it.** A verifier's claim is a lead. Open
  the file, read the line, run the command, then decide.
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
| `VF-5` | the required comparison against the pinned reference | **owner-decided, correction done** | **owner decision, 2026-09-17:** the eligible collection at `eb42c1347` governs (0.653 / 0.272 / 0.480, identity MATCH, an ancestor of this tree); addendum §5.1 cites it and §5.2 marks the older collection superseded. Remaining work: confirm no other document still quotes the superseded rows |
| `VF-6` | complete-operation comparison | **owner-decided: deferred to Stage 6** | **owner decision, 2026-09-17:** *"i think we can defer it to stage 6."* Recorded in addendum §6. It is a deferral with a named owner (#171), **not** a waiver and **not** a PASS: Stage 5 makes no complete-operation claim. Do not re-open it inside Stage 5 and do not promote it |
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

### WP-C - Comparison governance (`R2-F5`, `VF-5`, `VF-6`) — decided

**Both rows are owner-decided; the remaining work is bookkeeping, not measurement.**

1. **Governing collection:** the eligible one at
   `eb42c13477f85612fc864474af4489c2548d688d` — receipt
   `evidence/stage-5-component-comparison-20260917T143008Z/run-1/receipt.json`,
   sha256 `43dbd9f440ea9b417278bd78498c33f5ad027901365f4940d00137fe2f014b24`,
   0.653 / 0.272 / 0.480 with identity MATCH on all six pinned identities.
   `stage-5-verification-addendum-20260917.md` §5.1 now cites it and §5.2 marks
   the earlier `073017Z` collection superseded.
2. **Complete-operation comparison (`VF-6`):** deferred to Stage 6 (#171) by owner
   decision, recorded in addendum §6. Not a waiver, not a PASS; Stage 5 claims no
   complete-operation performance.
3. **Sweep done, 2026-09-17:** `stage-5-verification-addendum-20260917.md` §5.1
   cites the governing receipt and §5.2 retains the superseded rows;
   `stage-5-completion-report-20260917.md` §5 and `stage-5-report.md` §13.4 are
   repointed. Three documents still mention `…T073017Z` and are left as written
   because they are dated records of the state at their time:
   `stage-5-remediation-handoff-20260917.md`,
   `stage-5-remediation-wp4-wp7-handoff-20260917.md` and the round-1 review
   `stages-1-5-review-20260917T160000Z.md`. Do not rewrite them; if a future
   reader could mistake one for current guidance, add a dated pointer, not an edit.
4. **Do not re-collect to chase a better ratio.** The 1.85x/2.3x spread between the
   two collections is a known open question (the older README attributes its first
   case's wall to a cold Cargo build); investigate it only if you intend to quote a
   ratio, and record what you find. Until then, quote the governing receipt's own
   numbers with its own source identity and no cross-collection comparison.

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

## 7. Verification subagents per round

You do not review your own fix, and you do not wait for anyone else. You **launch
verification subagents** and adjudicate what they return.

**Shape of a round.** One subagent per claimed row, or one per small group of rows
that share an artifact. Launch them together so they run concurrently, each with a
distinct output path under the round's evidence directory:

```text
docs/roadmap/0.1/0.1.7/evidence/stage-5-terminal-<stamp>/
  verify-<row-id>.md          one per subagent, its only write
  <the commands' own logs>    produced by the subagent or by you
README.md                     your round manifest: rows claimed, verifier outcome,
                              every command, every failure, every NOT_RUN
```

**The prompt to give each verification subagent** (fill every bracket; the subagent
sees nothing else):

> You are verifying one claim on the LayerFS repository at
> `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, frozen at `<commit>`.
>
> **HARD CONSTRAINTS.** Read-only: never modify, create or delete any product, test,
> fixture or harness source, and never run a command that writes inside `core/`.
> You may write exactly **one** file: `<evidence path>`. Never commit, push,
> checkout, reset or change issue state. Do not run resource-heavy commands
> concurrently with anything else; if the check is a measurement, say so in your
> summary before running it.
>
> **THE CLAIM.** Ledger row `<row id>`: `<the row text>`. It is claimed closed by
> `<commit or artifact>`.
>
> **WHAT TO DO.** Reproduce the row's required evidence yourself, through public
> entry points, with your own command. `<the exact command to start from>`. Then
> try to falsify it: the boundary on both sides, the malformed input, the error
> path, and the case the claim does not mention.
>
> **WHAT TO REPORT.** Write `<evidence path>` containing: the verdict
> `PASS`/`FAIL`/`INCOMPLETE`; every command you ran with its exit code; every
> finding with `path:line` and the line quoted; and an explicit list of anything
> you could **not** verify, marked UNVERIFIED. A roadmap report, a passing suite, a
> commit message and an attractive interface are **not** evidence.
>
> Then reply with at most 40 lines: verdict, the strongest evidence for it, the
> strongest evidence against it, and what you could not check.

**Adjudication, which is your job and not theirs.** For every finding: open the
file, read the line, run the command, decide. Accept it, refute it with your own
evidence, or record it as UNVERIFIED. Then act. A round is closed only when every
verifier returns zero open findings for its row and you have adjudicated all of them.

**When a verifier disagrees with you,** the default is that the verifier is right
until you have reproduced the opposite. Do not argue with a subagent; re-run the
check and let the output decide.

**Closing pass.** The final round runs the same mechanism across the whole ledger at
once — several subagents, one per matrix section, plus one that only tries to
falsify the terminal checklist itself. The terminal condition is met when that pass
returns zero open findings.

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
8. A **closing verification-subagent pass** on the final tree reports zero open
   findings, and you have adjudicated every one of them.
9. The completion report states the final totals, the closed rows, and every row
   that remains unmeasured with its reason.
10. `#170` is closed with a final comment naming the closing verification pass, its evidence
    directory, the final HEAD and the exact matrices. `#165` gets a one-line
    pointer. Nothing is tagged or released by this handoff.

Then, and only then, is Stage 5 done. Stage 6 (#171) owns whole-core qualification
and Stage 7 (#172) owns runtime integration; do not start either.

## 9. Anti-patterns that will fail verification

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
