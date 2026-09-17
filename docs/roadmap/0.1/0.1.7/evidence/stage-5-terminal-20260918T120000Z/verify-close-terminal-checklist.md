# Closing verification: the terminal checklist itself (§8, item by item)

Verifier: the closing falsifier subagent, read-only on tracked files. Tree
`4d887a6b9e74ee3e3fa863d09f8b6494d5d79f0f` (`git rev-parse HEAD`, exit 0;
working tree clean at session start). This report tries to falsify each item of
`stage-5-terminal-handoff-20260917.md` §8 on the final tree. A roadmap report, a
passing suite, a commit message or an attractive interface is not evidence;
every claim below was re-run or re-counted. The four sibling closing reports
(`verify-close-correctness.md`, `verify-close-reads-attributes.md`,
`verify-close-evidence-limits.md`, `verify-close-cumulative.md`) are being
written concurrently and were not waited for, per the tasking.

Note on `5b93c3184` vs HEAD: `git diff --stat 5b93c3184..4d887a6b9` shows the
final commit adds only the eight `check-final-*.log` files (8 files, docs
only), so re-running the §6 checks on HEAD is equivalent to the claimed
`5b93c3184` tree for every product check.

## Item 1 — all eight §6 checks exit 0, counts recorded: **SATISFIED**

The logs record output; the codes live in commit `4d887a6b9`'s message ("all
exit 0... 65 result blocks... 434 tests passed, 0 failed, 0 ignored... boundary
guard scans 116 production Rust/SQL files"). I re-ran **all eight** myself:

| Check | Command (re-run) | Exit | Matches committed log? |
| --- | --- | --- | --- |
| Boundary guard | `python3 core/tools/check_product_boundary.py` | 0 | byte-identical (`diff` clean) |
| Core-tools self-tests | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 | 6 tests OK; only timing differs (0.009 vs 0.011 s) |
| LOC counter tests | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 | 17 tests OK |
| fmt | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 | both empty (0 bytes) |
| Workspace tests | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 | my run: 65 result blocks, 0 non-ok, 434 passed / 0 failed / 0 ignored — identical totals; per-test lines differ only in intra-suite order and timings |
| clippy | `cargo +1.85.1 clippy ... --all-targets -- -D warnings` | 0 | only build-time figure differs (0.06 vs 0.16 s) |
| LOC files | `python3 tools/production_loc.py --files` | 0 | exact match (`diff` clean, 310 lines) |
| git diff --check | `git diff --check` | 0 | both empty |

The committed `check-final-cargo-test.log` itself contains exactly 65
`test result:` blocks, all `ok.`, summing to 434 passed / 0 failed / 0 ignored
(awk over the log) — the log's content is consistent with the claimed exit 0.
No check was found to mismatch its claimed code.

## Item 2 — Stage 5 matrix 0 FAIL / 0 INCOMPLETE / 0 unowned: **SATISFIED, with one bookkeeping finding (F1)**

Counted every row of `stage-5-report.md` §16 "Stage 5 matrix - final"
(`:753-802`), expanding every `..` range against the round-2 review's §4.1
enumeration (`stages-1-5-review-20260917T230700Z.md:860-1000`, "denominator 71
+ 12 new = 83", verified row by row):

- No row carries FAIL, PARTIAL or INCOMPLETE. Every row is PASS or PASS
  (round 2), except VF-6 (NOT_RUN) and N-12 (NOT_APPLICABLE). Confirmed by
  reading all 36 displayed rows.
- VF-6's disposition is written and owned: addendum
  `stage-5-verification-addendum-20260917.md:137-143` records the owner's
  2026-09-17 decision ("defer it to stage 6"), names #171 as the owning stage,
  and explicitly says not-a-waiver/not-a-PASS. GitHub issue #171 exists
  (fetched, HTTP 200). The handoff §4 `:190` carries the same decision.
- S3-6/S4-5: owner-WAIVED, word-for-word "unchanged; not counted as PASS"
  (`:811`, `:813`), identical to the round-2 review `:986`, `:991` and the
  matrix-remediation record `:115`, `:120`; excluded from the 34-PASS count.
  Waiver provenance: `stages-3-4-completion-round-20260917.md:169`.
- Totals: 81 PASS + 1 NOT_RUN + 1 NOT_APPLICABLE = 83 — arithmetic correct
  **for the review's 83-row set**.

**Finding F1 (moderate, bookkeeping not substance):** the §16 Stage 5 table as
printed displays **84 row-equivalents** — the review's 83 **plus N-13**, which
the review places only in the cumulative matrix (`N-13..N-17` are its §4.2
additions; the §4.1 Stage 5 matrix has no N-13, grep count 0). The table's
N-13 row (`:797`) is formatted like any Stage 5 row with status PASS, and
nothing in §16 says it is counted only in the cumulative denominator. A reader
counting displayed rows finds 84 rows and 82 PASS-marked rows against the
stated "81 PASS ... of 83". The underlying set is consistent (N-13 is genuinely
PASS and genuinely a cumulative row, also displayed there), but the stated
denominator does not account for every row the table displays.

## Item 3 — cumulative matrix the same: **SATISFIED**

Counted `:804-823`: S01-1..6 (6) + S2-1..9 (9) + S3-1..5 (5) + S3-6 (1) +
S4-1..4 (4) + S4-5 (1) + TEL-1..4 (4) + X-1 (1) + N-13..N-17 (5) = **36 rows
exactly**, of which 2 owner-WAIVED (S3-6, S4-5) and **34 PASS** — the totals
line counts every displayed row. No FAIL/PARTIAL row exists. The sixth
integration route follows VF-6's disposition and is not counted as PASS.

## Item 4 — every limits-table row correct, classed, boundary-cased or derived with arithmetic: **SATISFIED**

§6 (`:418-440`) has exactly **19 rows**, every one with a Kind (enforced /
format / configurable / derived-unverified / not claimed / measured). Each row
was checked against the source and its cited case:

- Constants verified in source: `MAXIMUM_NAME_BYTES=255`,
  `MAXIMUM_PATH_BYTES=4_096`, `MAXIMUM_PATH_COMPONENTS=256`,
  `MAXIMUM_PAGE_BYTES=8_192`, `MAXIMUM_TREE_LEVEL=31` (=
  `file::mapping::MAX_LEVEL`, `types.rs:17`), inode leaf 50–100, branch 64–127,
  `MINIMUM_FILLED_PAGE_BYTES=3_277` (2/5 fill arithmetic: 3277×5=16385 ≥
  16384 = 8192×2; 3276 fails), attribute domain 64 / key 255, attribute value =
  `cdc::MAXIMUM_CHUNK_BYTES=32_768` (`gear.rs:17`, derived not restated,
  `limits.rs:58`), symlink 4,096, serial 1..`i64::MAX`
  (`identity.rs:20`), `READ_WAVE_OBJECTS=32` (`mapping/read.rs:24`) with byte
  window = 32 × 32,768 = 1,048,576 asserted as an identity in
  `tests/file_read.rs:293-296`, `MAXIMUM_WALK_ENTRIES=4_096`
  (`limits.rs:86`), scratch ≥1,024 enforced (`input.rs:99`) with default
  4 MiB (`limits.rs:41`; `sorted::page::MAXIMUM_SCRATCH_BYTES` at `page.rs:26`
  is a re-export of that one constant, not a twin), pending ≥1
  (`input.rs:104`) default 4,096 (`reduce.rs:25`),
  `MAXIMUM_READ_DEMANDS=4_096` (`objects.rs:20`),
  `MAXIMUM_LEVELS=32` (`references/runs.rs:41`),
  `GROUP_COUNT_LIMIT=256` (`layerfs-storage/src/policy.rs:57`).
- Boundary cases exist and passed in my re-run: name 255/256, path 4,096/4,097
  and 256/257, symlink 4,096/4,097+NUL, attribute domain/key at bounds
  (`tests/filesystem_limits.rs`), value = chunk max at boundary
  (`filesystem_failure.rs`), serial above `i64::MAX` refused
  (`filesystem_failure.rs:495`), wave demands 4,096/4,097
  (`filesystem_limits.rs`), payload wave object+byte ceiling (`file_read.rs`).
- Derived/unverified rows carry their arithmetic and are pinned by test:
  tiers 32 (binary-counter 2³²−1 arithmetic asserted in
  `filesystem_limits.rs::the_limits_this_suite_cannot_bound_are_named_with_their_reason`),
  level-31, page 8,192 via encoders, group count pinned in `storage_limits`
  (`:135`, `:149-164`, placement + parser enforcement).
- Theoretical ("not claimed") and Verified ("measured") rows are classed
  disclaimers; the measured figures trace to the round-2 review.

**Finding F6 (informational):** the walk-entry row's tight figures (4,096
accepted / 4,097 refused) are proven by the round-4 verification probe
(`verify-R2-F7.md:95-117`, public-API reproducer) rather than by a committed
product test; committed tests cover the rebind consequence and work-limit
reachability. Receipt evidence, acceptable, but a reader should know the tight
boundary lives in the probe receipt, not the suite.

## Item 5 — every `--case` selects the operation it names, receipt proves it: **FALSIFIED IN PART (receipt coverage incomplete; no behavioural defect found)**

Only three of the eight WP-A examples have a `--case` flag at all
(`filesystem_timing_c1`, `measure_filesystem`, `measure_edits`; grep of all
eight example sources). The other five (`measure_components`,
`measure_pooled`, `filesystem_primitives_candidate`, `edit_timing_c1`,
`memory_ledger`) have no `--case`, so the claim is vacuous for them.

Receipt coverage in `stage-5-terminal-20260918T020000Z/case-selection/`:

| Example | Case × mode space | Receipted |
| --- | --- | --- |
| `measure_filesystem` | 6 cases × 3 modes = 18 | **18/18** (`c1-*`, `c2-*`, `pipeline-*`) |
| `filesystem_timing_c1` | 6 cases (no `--mode`) | **2/6** (`c1-attribute-case/{empty,attributes}.log`) |
| `measure_edits` | 5 cases × 3 modes = 15 | **1/15** (`edits-c2/small.log`) |

So the literal checklist claim — "a case-selection receipt proves it" for
**every** `--case` — is not met: `filesystem_timing_c1`'s directory-update,
inode-update, hardlink-move and subtree-remove, and `measure_edits`' other 14
case×mode combinations, had **no case-selection receipt** at closure. WP-A's
own instruction ("Reproduce with a matrix run: every case x every mode") was
executed only for `measure_filesystem`. The dedicated verifier disclosed
exactly this (`verify-R2-F1-F2-F3-F14.md:224-225`, UNVERIFIED #3/#4), but §16's
N-1 row cites that verifier for a blanket PASS without carrying the caveat.

Mitigation — my own falsification runs on the final tree (all exit 0):
`filesystem_timing_c1` all six cases produce distinct counters (empty: read 0/
emitted 1/129 B; directory-update: read 6 waves 4/19,586 B emitted 1;
inode-update: read 1/368 B emitted 2/497 B; hardlink-move: read 2/423 B emitted
3/567 B; subtree-remove: read 6/16,797 B emitted 3/298 B; attributes: read 0
emitted 5/507 B), and `measure_edits` spot checks (`c1/small` 9 nodes,
`c1/chunked` 11, `pipeline/small` 11, `pipeline/batch` 19; distinct fixtures)
show selection works across modes. **No selection defect exists on the final
tree; the gap is missing receipts, not wrong behaviour.** The specific
review findings (F1/F2/F3) that drove N-1's FAIL are fixed and fully
receipted.

## Item 6 — every Stage-5 commit's LOC disclosure reproduces: **SATISFIED (by-design drift reading accepted), with findings F2 and F4**

My own recounts with `tools/production_loc.py` on `git archive` exports:

| Commit | My recount | Disclosure / reread log | Reproduces? |
| --- | --- | --- | --- |
| `bfd7abf2c` (drift) | parent 5d08d9e83 = 17,563; commit = 17,698 (+135) | reread.log: 17563→17698 +135; disclosed 17525→17730 +205 | disclosure does **not** reproduce — exactly as the correction records |
| `3ecb952c8` (round-4 remedy) | parent 99743b2cf = 18,797; commit = 18,792 (−5) | disclosed 18797→18792 (−5) | **yes, exact** |
| `1884e3eca`, `5b93c3184`, `680115bcd`, HEAD `4d887a6b9` | all = 18,792 core (reference 65,417, combined 84,209) | all disclose delta 0 at 18,792 | **yes, exact** |

`per-commit-loc-reread.log` covers the six drift commits, merge `a8a1ba848`
(no disclosure; recomputed 732→732) and the `64e3f9d6a`→`01d9f70f3` scope
switch; `per-commit-loc-reread-2.log` covers `f288d2af7..99743b2cf` and found
`de648507b`'s stale levels. §2's correction table (`:176-187`) matches
reread.log column for column.

**Do I accept the by-design reading? Yes.** The repository rule forbids nothing
less than an accurate disclosure, but the handoff's WP-B explicitly says "Do
**not** rewrite history", and `git commit --amend`/rebase of published commits
would destroy the audit trail the rule exists to protect. The correction
publishes, beside each unreproducible disclosure, the recomputed value, the
drift magnitude (≤38 lines), the root cause (prepared pre-commit, never
re-confirmed), the scope-switch convention and the merge's missing line — i.e.
everything an honest disclosure would have carried, at the point of use, with
the failure visible rather than silent. Seven of 27 Stage-5 commits are
documented as unreproducible; the rest reproduce (I confirmed five myself).

**Finding F4 (minor):** reread-2.log's audited range ends at `99743b2cf`. The
final five commits' reproductions rest on their commit messages plus my
recount above, not on a log inside the evidence directory; §2's "re-audited
2026-09-18" list (`:201-203`) likewise stops at `99743b2cf`.

**Finding F2 (moderate — the strongest falsification of this pass):** §2's
"Totals of the tree that carries this correction (R2-F15)" (`:219-225`) states
C1 11,922 / C2 6,112 / telemetry 763 / core **18,797** / combined **84,214**,
and asserts "the round-4 commits after `9327f6695` are documentation and
evidence only, so the production totals do not move again in this round". On
the final tree both parts are false: the remedy commit `3ecb952c8` (after
`9327f6695`) removed 5 production lines, and the actual totals are C1
**11,917** / core **18,792** / combined **84,209** (`tools/production_loc.py`
on HEAD, verified twice). The paragraph was written at `134b8df73` (git log
-S) and never updated after the remedy. The round-4 README's R2-F15 row
("the report's headline states the totals of the tree that carries it (…
18,797 … 84,214)") is therefore false on the final tree, and ledger row
R2-F15's own flip requirement — "§2 states the totals of the tree that carries
it" — is not literally met by the tree that now carries §2. The per-commit
disclosures themselves are honest (`3ecb952c8` discloses the −5); the stale
headline is a bookkeeping failure to re-run the totals paragraph after the
remedy, which is the exact failure mode R2-F15 was about.

## Item 7 — comparison governance decided, governing document cites the eligible receipt: **SATISFIED**

- Recomputed `shasum -a 256` of
  `evidence/stage-5-component-comparison-20260917T143008Z/run-1/receipt.json` =
  `43dbd9f440ea9b417278bd78498c33f5ad027901365f4940d00137fe2f014b24` —
  **exact match** with addendum §5.1 `:80`.
- `git merge-base --is-ancestor eb42c13477f85612fc864474af4489c2548d688d HEAD`
  → exit 0: the source commit **is an ancestor** of the final tree; the
  receipt's own `source_commit` field matches it; the profile (release),
  toolchain (+1.85.1) and one-sample-per-arm statements match the receipt JSON.
- The three ratios recompute from the receipt's elapsed ns (153,542/235,208 =
  0.653; 810,792/2,977,625 = 0.272; 1,838,958/3,829,000 = 0.480), all identity
  MATCH. The superseded §5.2 receipt's sha256
  (`23c923ac…7732c45`) also verifies; §5.2 is retained and marked superseded
  with the ineligibility reason (identity mismatch after `b22712844`).

## Item 8 — closing verification pass, zero open findings, all adjudicated: **COMPLETED BY THIS PASS (pending adjudication of the findings below)**

This report is that item's falsifier half. I found no falsification of items
1, 3, 4, 7, and material-but-bounded findings for items 2, 5, 6 (F1, F2, F3,
F4, F5, F6 below). Those findings are handed to the closing round's adjudicator
(the main agent) with this report; the round's own adjudication record (§16's
verification-outcome table and the README's "Verification records" section)
covers the round-4 verifiers' findings. The four sibling closing verifiers were
not waited for, per the tasking; item 8 is completed by this pass plus the
adjudication recorded in the round evidence.

## Item 9 — completion report states final totals, closed rows, unmeasured rows with reasons: **SATISFIED WITH CAVEAT**

`stage-5-completion-report-20260917.md` §12 (`:318-333`) states the final
totals — 81/0/0/1(NOT_RUN)/1(NOT_APPLICABLE) of 83, cumulative 34/0/0/2 of 36,
eight checks exit 0, 65 blocks / 434 tests — every one of which I independently
verified above. It points at §16 for the matrices and the unmeasured rows.
§16's unmeasured list (`:835-853`) covers: VF-6 (deferred, owner decision),
cold-cache/pack-footprint/process-level rows (warm in-process fixtures; the
operation's own counters are the Stage 5 evidence), the derived-unverified
figures (MAXIMUM_LEVELS, 256-group ceiling, level-31), the storage-side fixed
work budgets (scoped out with the verifier's concurrence), and the two waived
Stages 3-4 rows. I cross-checked the matrix for any unmeasured row missing
from the list: none (N-12's NOT_APPLICABLE reason is carried inline in its
matrix row; the sixth integration route follows VF-6). Caveats: §12's totals
inherit F1's display issue, and §12's "the terminal condition ... is met" is
contradicted in the letter by item 5's receipt gaps and F2; §12 itself makes no
LOC-total claim (the stale totals are §2's, see F2).

## Item 10 — #170 closure: **PENDING BY DESIGN; all preconditions verified**

Closure happens after this pass. Preconditions on the final tree:

- Final HEAD exists: `4d887a6b9e74ee3e3fa863d09f8b6494d5d79f0f` ✓.
- Matrices published: §16 landed in `5b93c3184`, present at HEAD ✓.
- Evidence directory committed: 43 tracked files under
  `evidence/stage-5-terminal-20260918T120000Z/` (all receipts, logs, README,
  both LOC reread logs, the `diagnostics/s5term` source); the only untracked
  files there are the five concurrent `verify-close-*.md` reports (this one
  included) and untracked build artifacts under `diagnostics/s5term/target/` ✓.
- Nothing tagged or released: `git tag -l | grep 0.1.7` → empty (exit 1);
  newest tag is `v0.1.6` (2026-09-16); all 42 tags are `archive/` or `v0.1.6`
  ✓. No release artifacts were found.
- The named owner of the deferral, issue #171, exists (fetched, HTTP 200).

## The round's own honesty claims

- **Round-4 README "What this directory proves" table:** the measurement rows
  are accurate — `ordering-scaling.log` and `simultaneous-memory.log` match the
  README's claims number-for-number, and I independently reproduced **every
  deterministic counter** of both receipts by rebuilding the public-API
  diagnostics client on the final tree (`CARGO_TARGET_DIR=/tmp/layerfs-s5term-target
  cargo +1.85.1 build --release`, exit 0; `s5term grid` and `s5term coexist`,
  exit 0: spilled 448/960/1,984/3,968; rows_read 2,198/6,684/19,960/59,007;
  runs 14/30/62/124; peak_owned 66,432/139,008/284,160/568,320; 64×96=6,144 B;
  5×16,384=81,920 B; 376,110 B; 349,820 B — all identical; elapsed within
  single-sample spread, no stable absolute claimed). `check-cargo-test.log`'s
  provider_errors/connection_profile claims correspond to passing tests in my
  re-run. **But the R2-F15/VF-7 row is falsified on the final tree** (F2): the
  headline totals it quotes (18,797/84,214) are those of `134b8df73`, not the
  tree that carries the report (18,792/84,209).
- **Discarded dev-profile attempt:** consistently disclosed (README
  "Discarded attempt, retained honestly"); the release-profile figures it
  contrasts are the retained receipt's and reproduce (above). The dev-profile
  figures themselves (46.3/94.1/182.0/376.1 ms) have no receipt **by design**
  and cannot be checked beyond the disclosure's internal consistency — see
  UNVERIFIED #2. The disclosure's framing (work counters identical, elapsed
  not comparable) matches what the counters show.
- **F14 impossibility statement: CONFIRMED empirically.** I re-ran
  `measure_components` on the final tree with the maximum legal input (8 MiB):
  `--mode c1` → "timings: 5 nodes at 3 levels", `--mode pipeline` → "59 nodes
  at 5 levels", `--mode c2` → "4 nodes at 2 levels", all exit 0, against the
  1,024-node budget; an 8 MiB + 1 input is refused with exit 1 ("refuses
  inputs above 8388608"); a pre-existing `--timings` path errors with exit 1
  (same `Result` plumbing the clip gate uses). The input ceiling
  (`DEMO_INPUT_LIMIT = 8 MiB`) is in source. No legal input can reach the clip
  path; the impossibility statement is honest, and the wiring + telemetry-test
  evidence for the gate is fairly characterised.

## Findings (all, with path:line)

- **F1 (moderate):** `stage-5-report.md:753-802` — Stage 5 matrix displays 84
  row-equivalents (review's 83 + N-13 at `:797`) against a stated denominator
  of 83 and 81 PASS; N-13 is a cumulative-matrix row (review §4.2) shown
  without a note that it sits outside the Stage 5 denominator.
- **F2 (moderate, strongest):** `stage-5-report.md:219-225` — "Totals of the
  tree that carries this correction" states core 18,797 / combined 84,214 and
  claims post-`9327f6695` commits are docs-only; the final tree is 18,792 /
  84,209 (remedy `3ecb952c8`, −5, after the paragraph was written). Also
  falsifies the round-4 README's R2-F15 row and ledger row R2-F15's flip
  requirement, in letter.
- **F3 (moderate):** item 5 receipt gaps — `case-selection/` covers 18/18
  (`measure_filesystem`) but 2/6 (`filesystem_timing_c1`) and 1/15
  (`measure_edits`); disclosed at `verify-R2-F1-F2-F3-F14.md:224-225` but not
  carried into §16's N-1 PASS row. No behavioural defect (my fresh runs).
- **F4 (minor):** `per-commit-loc-reread-2.log` ends at `99743b2cf`; the last
  five commits' LOC reproduction rests on commit messages + my recount, not an
  evidence-dir log; §2's re-audit list (`:201-203`) likewise stops there.
- **F5 (minor):** `head.txt` was overwritten `9327f6695` → `134b8df73` in
  `99743b2cf` (git diff), against the README's letter "Nothing in this
  directory was edited after it was written"; the README's "Code tree |
  `9327f6695` (`head.txt`)" citation no longer matches the file's content.
  All other directory edits (`README.md`, `verify-*.md`) were purely additive
  (0 removed lines, verified by diff).
- **F6 (informational):** the walk-entry tight boundary (4,096 ok / 4,097
  refused) is proven by `verify-R2-F7.md:95-117` (public-API probe), not a
  committed product test.

## UNVERIFIED

1. **The owner's exact words on the GitHub issues** (VF-6 deferral, the
   S3-6/S4-5 waivers): only the in-tree quotations and issue #171's public
   existence were verified; the issue bodies/timelines were not read (fetch
   returned page chrome; #170's state not checked — closure is post-pass by
   design).
2. **The discarded dev-profile run's figures** (46.3/94.1/182.0/376.1 ms): no
   receipt exists by design; internal consistency only.
3. **The mid-round `check-*.log` set** (run on `134b8df73`): read but not
   re-run; the final-tree set (the closure claim) was fully re-run instead.
4. **The four sibling closing verifiers' reports**: concurrent, not waited
   for, per the tasking.
5. **`#170`'s final comment and `#165`'s pointer**: pending by design
   (item 10 happens after this pass).
6. **Elapsed-time columns** of the grid/coexist receipts: single samples; my
   re-run differs within the stated same-binary spread; no stable absolute is
   claimed, so there is nothing to falsify beyond the counters (which match).

## Command log (exit codes)

All from the repository root unless noted. Read-only except this file, /tmp,
and `core/target` build artifacts.

| Command | Exit |
| --- | --- |
| `git rev-parse HEAD` | 0 (`4d887a6b9...`) |
| `git status --porcelain` | 0 (clean at start; owner's concurrent `core/docs` edits and siblings' reports excluded) |
| `git tag -l` (42 tags; `grep 0.1.7` empty) | 0 / 1 |
| `git diff --stat 5b93c3184..4d887a6b9` | 0 (logs only) |
| `python3 core/tools/check_product_boundary.py` | 0 (116 files) |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 (6 tests) |
| `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 (17 tests) |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 (65 blocks, 434/0/0) |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 |
| `python3 tools/production_loc.py --files` / `--json` | 0 (core 18,792 / ref 65,417 / combined 84,209; C1 11,917, C2 6,112, telemetry 763) |
| `git diff --check` | 0 |
| `git archive <c> \| tar -x -C /tmp/...` + `production_loc.py --root` for `bfd7abf2c~1`, `bfd7abf2c`, `3ecb952c8~1`, `3ecb952c8`, `1884e3eca`, `5b93c3184`, `680115bcd` | 0 each (17,563/17,698/18,797/18,792/18,792/18,792/18,792) |
| `shasum -a 256` both component-comparison receipts | 0 (both match addendum) |
| `git merge-base --is-ancestor eb42c1347 HEAD` | 0 |
| `cargo +1.85.1 build --manifest-path core/Cargo.toml --workspace --locked --examples` | 0 |
| `measure_components --mode c1/pipeline/c2` (8 MiB input) | 0 ×3 (5/59/4 nodes) |
| `measure_components` 8 MiB+1 input; pre-existing `--timings` | 1, 1 |
| `filesystem_timing_c1 --case {all 6}` | 0 ×6 (distinct counters) |
| `measure_edits --mode {c1,pipeline} --case {small,chunked,batch}` | 0 ×4 (distinct fixtures/nodes) |
| `CARGO_TARGET_DIR=/tmp/layerfs-s5term-target cargo +1.85.1 build --release` (s5term) | 0 |
| `s5term grid`; `s5term coexist` | 0; 0 (all deterministic counters identical to receipts) |
| assorted `git log/show/diff`, `grep`, `sed`, `awk`, file reads | 0 |

No tracked file was modified, staged, committed, pushed, checked out or reset
by this pass; no issue state was changed. Verdict per item: 1 SATISFIED, 2
SATISFIED (F1), 3 SATISFIED, 4 SATISFIED (F6), 5 FALSIFIED IN PART (F3), 6
SATISFIED with F2/F4, 7 SATISFIED, 8 completed by this pass pending
adjudication, 9 SATISFIED WITH CAVEAT (F1/F2/F3), 10 PENDING BY DESIGN
(preconditions verified).

---

# RE-VERIFICATION (post-remedy)

Remedy commit `6989a4da7daa418260991d1821eccc075b0233fd` (now HEAD; working
tree clean; `git rev-parse HEAD` exit 0). The range `4d887a6b9..6989a4da7`
contains one owner architecture commit (`f3957dc76`, docs) and the remedy
commit; the examples and all product `src/` are byte-unchanged
(`git diff --stat 4d887a6b9..6989a4da7 -- core/crates/*/examples/` empty; the
one product-tree change in the remedy is the `filesystem_bounds.rs` test file).
Each remedy re-verified independently:

**F6 → REMEDIATED.** `the_cycle_check_work_limit_is_reachable_and_reported`
(`core/crates/layerfs-content/tests/filesystem_bounds.rs:786-824`) now asserts
a build stating exactly the ceiling is accepted with
`entries_examined == limit`, and `limit + 1` is the first refusal
(`InvalidRecord("cycle check work limit")`); `limit` is
`validate::MAXIMUM_CYCLE_CHECK_ENTRIES`, which is `limits::MAXIMUM_WALK_ENTRIES`
(`validate.rs:55`) — the same 4,096 constant, not a twin. Ran it twice: inside
the full workspace suite and explicitly
(`cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test
filesystem_bounds the_cycle_check_work_limit --locked` → "1 passed; 0 failed",
exit 0).

**F2 → REMEDIATED.** §2's totals block now states C1 11,917 / C2 6,112 /
telemetry 763 / core 18,792 / reference 65,417 / combined 84,209, and records
the stale first version (18,797/84,214 at `134b8df73`) and its cause
(`3ecb952c8`'s −5 dead-code deletion) — the exact failure mode the row is
about. My recount of HEAD reproduces the new figures exactly
(`tools/production_loc.py --json`: core 18,792, C1 11,917, C2 6,112, telemetry
763, reference 65,417, combined 84,209). The R2-F15 README row now carries the
same corrected figures.

**F3 → REMEDIATED.** `case-selection-r4/` holds 21 receipts: all six
`filesystem_timing_c1` cases (`c1-*.log` — counters distinct per case and
identical to my own fresh runs: empty 0/1/129 B, directory-update 6 waves
4/19,586 B, inode-update 1/1/368 B, hardlink-move 2/2/423 B, subtree-remove
6/4/16,797 B, attributes 0/5/507 B) and all fifteen `measure_edits`
case×mode combinations (`edits-{c1,c2,pipeline}-{small,chunked,small-to-large,
large-to-small,batch}.log` — per-case fixtures distinct: 65,536/1/512,
262,144/1/4,096, 131,071/1/131,072, 262,144/1/0, 196,808/3/1,000; the c2 logs
carry the honest "supplied-object wiring probe ... not a per-case comparison"
disclosure). I ran **all 21 combinations myself** on the remedied tree: every
one exit 0, and every fixture + node-count line matches its receipt
(the 6 timing_c1 cases and 5 of the 15 edits combos in the first pass; the
remaining 10 re-run now, all MATCH). The r4 c2-small receipt is
line-consistent with round-3's `edits-c2/small.log` (deterministic lines
identical; only elapsed figures and tmp paths differ). §16's N-1 row now
carries the coverage note, including the five WP-A examples with no `--case`
flag. The examples' sources are unchanged, so the receipts describe the
shipped binaries.

**F1 → REMEDIATED.** §16's Stage 5 N-13 row now reads "(a cumulative-matrix
row, shown here for completeness; counted in the cumulative denominator only,
not in this table's 83)" — the 84-displayed-rows/83-denominator ambiguity is
resolved; the totals line (81 PASS + 1 NOT_RUN + 1 NOT_APPLICABLE = 83) now
accounts for every row the table displays as a Stage 5 row.

**F4 → REMEDIATED.** `per-commit-loc-reread-3.log` audits
`3ecb952c8, 1884e3eca, 5b93c3184, 680115bcd, 4d887a6b9, f3957dc76` — every
commit in `99743b2cf..6989a4da7` **except the remedy commit itself** (the log
is carried by that commit, so it cannot list it); all listed rows reproduce
exactly. My own recount of `6989a4da7` (first parent `f3957dc76` = 18,792 →
`6989a4da7` = 18,792, delta 0, via `git archive` + `production_loc.py`)
matches its disclosure exactly, so the full range is audited (log + recount).
§2's re-audit note now points at all three reread receipts.

**F5 → REMEDIATED.** The round README's code-tree row now states `head.txt` is
a tree pointer refreshed at the check-log commit naming `134b8df73` and that
the closing tree is the round's final commit; the Checks section distinguishes
`check-*.log` (mid tree) from `check-final-*.log` (closing tree), states where
the exit codes are recorded, and discloses the head.txt/git-status.txt refresh
as the one intentional edit to an existing file; the R2-F15 row carries
18,792/84,209; a "The closing pass (2026-09-18)" section records the five
verifiers and this falsifier's findings and remedies.

Two residual nits, recorded for completeness, neither a false claim: (a) the
README's "Production LOC" header row still reads "core 18,708 → 18,797 (+89)"
— accurate for the round's two product commits, with the final-tree 18,792
stated in the adjacent corrected R2-F15 row and §2, but the arrow can be
misread as round-end; (b) the Checks section says the exit codes are recorded
"in `stage-5-report.md` §16's preamble" — §16's preamble records the suite
green with counts (65 blocks / 434 / 0); the explicit all-eight-exit-0
statement lives in §12 of the completion report and in the landing commit
messages (both verified true). The codes are recorded; the pointer is one
section off.

**Checks on the remedied tree (re-run, all exit 0):** boundary guard (116
files), core-tools self-tests (6), LOC self-tests (17), fmt, workspace tests
(65 result blocks, 434 passed, 0 failed, 0 ignored — identical totals to the
pre-remedy tree, the remedy modified an existing test), clippy `-D warnings`,
`production_loc.py --files`, `git diff --check`.

## Final §8 verdicts on the remedied tree (6989a4da7)

| # | Item | Verdict |
| --- | --- | --- |
| 1 | Eight §6 checks exit 0, counts recorded | **SATISFIED** — all eight re-run by me on this tree, all exit 0, counts 65/434/0 and 116 files reproduced; exit codes recorded in the landing commit messages and §12 |
| 2 | Stage 5 matrix 0 FAIL / 0 INCOMPLETE / 0 unowned, dispositions, waived rows unpromoted | **SATISFIED** — F1 remediated (N-13 annotated); no FAIL/PARTIAL/INCOMPLETE row; VF-6 owner disposition written and owned (#171 exists); S3-6/S4-5 unchanged and excluded; 81+1+1 = 83 now consistent with the displayed table |
| 3 | Cumulative matrix the same | **SATISFIED** — 36 rows = 34 PASS + 2 owner-WAIVED, counted; table unchanged by the remedy |
| 4 | Every limits row correct, classed, boundary-cased or derived with arithmetic | **SATISFIED** — 19 rows verified against source in the first pass; F6 remediated (the walk ceiling's tight boundary is now a committed, passing test) |
| 5 | Every `--case` selects the operation it names, receipt proves it | **SATISFIED** — F3 remediated: 18 + 6 + 15 = 39 receipts cover every case×mode of the three `--case` examples; I ran all 21 r4 combinations myself (all exit 0, all match); the five no-flag examples are named in N-1's note |
| 6 | Every Stage-5 commit's LOC disclosure reproduces | **SATISFIED** — F2/F4 remediated: §2 states the true final totals (recounted), reread-1/2/3 + my recounts cover the whole range incl. `6989a4da7` (18,792→18,792 ✓); the seven historic drift rows remain documented beside their recomputed values (by-design reading accepted) |
| 7 | Comparison governance decided, eligible receipt cited | **SATISFIED** — unchanged by the remedy (addendum untouched, empty diff); sha256 and ancestry verified in the first pass |
| 8 | Closing pass, zero open findings, all adjudicated | **SATISFIED** — the five closing verifiers' reports and this falsifier's report are committed; findings F1-F6 adjudicated, remediated in `6989a4da7` and re-verified here |
| 9 | Completion report states totals, closed rows, unmeasured rows with reasons | **SATISFIED** — §12 unchanged (all its numbers verified true); §16's unmeasured list complete; the F1/F2/F3 caveats that qualified this item are remediated |
| 10 | #170 closed with a final comment; nothing tagged or released | **PENDING BY DESIGN** — preconditions re-verified on the remedied tree: HEAD exists, matrices published, evidence directory committed (incl. reread-3, case-selection-r4 and the five closing reports), no v0.1.7 tag (`git tag -l \| grep -c 0.1.7` → 0) |

**Conclusion.** On the remedied tree every §8 item is satisfied except item 10,
which is pending by design (the #170 closure comment happens after this pass).
The six findings were all bookkeeping; none survives re-verification, and no
behavioral or measurement defect was found in either pass. Every measurement
receipt, per-commit LOC disclosure and check I tested reproduces exactly.
