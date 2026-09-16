# #154 rollout ledger — the six #122 families on `main` (sandbox-local snapshot route)

Append-only. One entry per phase or family closure, with the exact numbers, the
identities they were taken on, the arithmetic, the reproduction command and every
non-passing line. Superseded receipts stay on disk; nothing here is rewritten
after the fact.

The machine-readable per-case matrices live beside this file in
`docs/roadmap/0.1/0.1.6/evidence/issue154/`. Raw receipts are retained under
`benchmark-results/v016/<tag>/…` (gitignored local evidence) and are named by the
matrix's `receipt` fields.

## L1 — Phase 0: the port (no measurement)

**What was ported.** The frozen specification restored from `7b73c4b33`
(`cases.json`, `check_plan.py`, `benchmark-families.md`, `fixtures.md`,
`workloads.md`, `execution-and-verification.md`, `review-decisions.md`,
`benchmark-exclusions-issue122.json`) and the archived benchmark harness: three
new family directories (`file_size_transition`, `multi_workspace_development`,
`branch_development`), the v016 workload sources
(`v016_common/stages/m1/boundary/compact/local`) and the host orchestrator and
oracle (`src/v016_mixed.rs`, `src/v016_oracle.rs`), the fixture-preparation and
matrix tooling, the two extensions and the six `dedup_branch_history` history
profiles.

**Route adaptations** (harness at the boundary only; declared operations,
counters and oracles unchanged):

| what | why | change |
| --- | --- | --- |
| `workspace_verify::verify_split_classes` | `main`'s `persist_snapshot` takes an evidence name | passes `canonical-verification` |
| `verify-selected.py` selection deadline | `main` bounds authentication to 45 s; a v0.1.6 case is charged from entry to its declared complete-command deadline | authentication bound is the case's declared deadline when it declares one, 45 s otherwise |
| declared deadlines | the overlay line carried 90 s regular / 300 s and 600 s extended allowances | frozen `cases.json` values: 25 s regular exception ceiling (22 s worker stop + 3 s cleanup), 120 s / 300 s / 60 s watchdogs |
| `workspace_registry` membership | the archived declaration (154) did not match its own `cases()` output | measured membership 161 = 8+20+12+4+4+16+10+10+20+14+**26**+7+5+4+1, with `local_snapshot` kept |
| `campaign-handoff-prompt.md` links | eight relative links resolved outside the tree, failing the frozen `check_plan.py` link gate | repaired to the real targets |

**Not ported, by design:** the overlay product line
(`crates/layerfs-workspace/src/overlay.rs`,
`crates/layerfs-workspace-core/src/file_edit.rs`) and the overlay CI-policy
commits. `main`'s own route work is preserved: the `local_snapshot` family, the
#151 store-footprint spool accounting and the #152 remote verification faults.

**Gates.** `python3 docs/roadmap/0.1/0.1.6/check_plan.py` →
`{"status":"PASS","regular_cases":33,"extended_cases":3,"mixed_load_cases":12,"fixtures":10}`;
`rustc --edition 2021 --test workload/main.rs` → 18 passed;
`target/release/fs-benchmark-pro workspace-self-check` →
`{"status":"pass","timed_case_count":161,"sample_slot_count":483}`;
`tools/preflight.sh` → all steps passed (rustfmt 1.96, tools tests, workspace fast
suite, `clippy --workspace --locked -- -D warnings`, benchmark harness tests).

**Not yet implemented anywhere** (carried into F4/F6, per #154): the two compact
`branch_development` controls and the six `historical_access` additions with their
sealed-producer access route. `infra-list` reports 28 of the 36 declared case IDs
registered.

**Identities after the port.**

| identity | value |
| --- | --- |
| source commit | `b4349ae9194e36dff321d965b1f7239842ff159c` (`LAYERFS_SOURCE_DIRTY=false`) |
| source seal | `8ef48ec2b762f7201c08b6beda91c60526c33a199ef1c2e000bd0bd01a5732f3` |
| source tree | `aecfb3687ddf61d9eb7f74ea520c3820f6000d66` |
| product seal | `970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd` |
| compilation seal | `4ad3b7b2cd7d94e9cfee627fc66644a8376c4a4b47592047dd438ee4f9c8baee` |
| dependency seal | `d9ae8ff2144895cbb559e2270763d141b7a61f0fc559184f8cc77bd50beb0d4f` |
| image | `layerfs-bench-infra:8ef48ec2b762f720` = `sha256:10ba09aa0f50a760b8314625c4e4abe12da9bd992b3763aab7fc8fef48a359c5` |
| harness identity | `89c38e1f76388c7a1df2e830bc607ee97db01c9b1a03e419f2d385b8b84c8bc5` |
| workload-source sha256 | `78e42fe5f86199f6e3874f2ca1f6fdeb4c5c79472d3b3352ae32e623037a4836` |
| host binary sha256 | `02fa95f9a6ff1aac604f8e2ed8e4ce92efe0bcf61247851623c2977b4f2836b0` |

## L2 — F1 `file_size_transition` (7 regular cases, seed 1) — terminal

Command:

```bash
python3 benchmark/fs-bench-pro/shared/v016_rollout.py \
  --image layerfs-bench-infra:8ef48ec2b762f720 --tag f1-seed1 \
  --family file_size_transition --prepare
```

One performance and one separate verification invocation per case, `--setup
clone` (declared `closed-quiescent-byte-copy`, master unchanged), one sample,
fresh append-only outputs. The complete-command wall is measured by the driver
around each invocation; every invocation fits the 15 s gate by a factor of six or
better, so no declared exception is claimed.

| case | perf status | perf wall | perf gate | verify status | verify wall | verify gate | Created | operation timer |
| --- | --- | ---: | --- | --- | ---: | --- | ---: | ---: |
| `v016-boundary-small-control-v1` | PASS | 2.23 s | PASS | PASS | 1.97 s | PASS | 2 | 20.26 ms |
| `v016-boundary-below-v1` | PASS | 1.92 s | PASS | PASS | 1.93 s | PASS | 2 | 24.11 ms |
| `v016-boundary-exact-v1` | PASS | 1.86 s | PASS | PASS | 1.91 s | PASS | 2 | 21.49 ms |
| `v016-boundary-above-v1` | PASS | 1.91 s | PASS | PASS | 2.41 s | PASS | 2 | 21.44 ms |
| `v016-boundary-large-control-v1` | PASS | 2.35 s | PASS | PASS | 2.06 s | PASS | 2 | 19.25 ms |
| `v016-boundary-roundtrip-v1` | PASS | 2.08 s | PASS | PASS | 2.22 s | PASS | 4 | 29.61 ms |
| `v016-boundary-alias-roundtrip-v1` | PASS | 2.42 s | PASS | PASS | 1.91 s | PASS | 5 | 53.80 ms |

* No `UpToDate`, `Busy`, `HeadMoved`, presentation failure or error was observed
  in any row: `commit_outcomes = {Created: n, UpToDate: 0, Busy: 0, HeadMoved: 0,
  presentation_failures: 0}` with `n` the row's Created count.
* Verification coverage: every row ran the canonical full-state verifier
  (`canonical-verification`, whole-snapshot payload extents) plus the independent
  native oracle over the declared paths (`oracle_scope=independent-source`).
  `v016-boundary-alias-roundtrip-v1` additionally proved the declared inode-class
  split (`v016-alias-inode-classes`) and replayed its four POSIX steps. Full-payload
  and affected-set verification are separately named in the matrix; the standing
  omission in every receipt is `no exhaustive Phase 1 replay`.
* Resources (container command window): 4.7 MB current, 5.7 MB container lifetime
  peak, 0 swap, 0 OOM kills; the host Store observations record 217 KB allocated
  before the run and 348–356 KB after the first Commit. Reported as sampled, with
  the container quota being the only capped scope.

**Non-passing rows: none.** All seven cases are terminal at seed 1.

**Defects found and fixed in F1: none.** The port ran green on the first
invocation; the only F1 work was instrumenting the run and recording it.

**Probe retained.** The first end-to-end probe
(`benchmark-results/v016/r1/perf-file_size_transition-exact-seed1`,
`…/verify-file_size_transition-exact-seed1`, status PASS 2.45 s / PASS 2.09 s on the
same identity) is kept as a labelled diagnostic of the port; the F1 gate sample for
that case is the one in the table above, taken with the family's declared
instrumentation.

## L3 — F2 `mixed_load_bearing` (4 regular + 2 declared extensions, seed 1)

Command:

```bash
python3 benchmark/fs-bench-pro/shared/v016_rollout.py \
  --image layerfs-bench-infra:8ef48ec2b762f720 --tag f2-seed1 \
  --family mixed_load_bearing --prepare                 # 4 regular cases
python3 benchmark/fs-bench-pro/shared/v016_rollout.py \
  --image layerfs-bench-infra:8ef48ec2b762f720 --tag f2-seed1 \
  --family mixed_load_bearing --extended --prepare      # 2 declared extensions
```

### Regular cases

| case | perf | perf wall | gate | verify | verify wall | gate | Created | max Commit | median Commit | verify stop point |
| --- | --- | ---: | --- | --- | ---: | --- | ---: | ---: | ---: | --- |
| `…100mb-5000-k10-v1` | PASS | 3.17 s | PASS | PASS | 4.33 s | PASS | 10 | 27.20 ms | 11.41 ms | — |
| `…100mb-5000-k100-v1` | PASS | 6.58 s | PASS | PASS | 12.06 s | PASS | 100 | 26.67 ms | 11.82 ms | — |
| `…500mb-30000-k10-v1` | PASS | 5.23 s | PASS | **TIMEOUT** | 22.94 s | **FAIL** | 10 | 59.65 ms | 16.64 ms | schedule complete, verifier still running at the declared stop |
| `…500mb-30000-k100-v1` | PASS | 14.53 s | PASS | **TIMEOUT** | 23.01 s | **FAIL** | 100 | 58.41 ms | 16.43 ms | schedule complete, verifier still running at the declared stop |

* Every perf row is a real `Created` schedule: `Created = 10/100`, `UpToDate =
  Busy = HeadMoved = presentation_failures = 0`, one worker
  (`concurrency_claim = not-a-concurrent-topology`, `observed_overlap_ns = 0`).
* The four perf walls are 3.17 s / 6.58 s / 5.23 s / 14.53 s: **all inside the
  15 s gate**, the K100 L500 row with 0.47 s of headroom. No exception is claimed
  for performance.
* Verification of the two L500 cases did not finish inside the 25 s declared
  ceiling: both were stopped at the product-command deadline after the complete
  10/100-Commit schedule and its stage records had been emitted, with cleanup
  still reporting PASS. They are retained as `TIMEOUT` with their measured stop
  wall (22.94 s / 23.01 s); they are **not** counted as passes and nothing was
  widened to make them fit.

### Declared extensions

| case | mode | status | wall | watchdog | note |
| --- | --- | --- | ---: | ---: | --- |
| `v016-mixed-exhaustive-100mb-5000-k100-v1` | verify-only | **PASS** | 38.33 s | 120 s | all 101 retained states verified |
| `v016-mixed-exhaustive-500mb-30000-k100-v1` | verify-only | **TIMEOUT** | 298.28 s | 300 s | did not finish its 101-state sweep inside its own declared watchdog |

Performance for both extensions is `N/A` (verify-only declared cases), never 0 and
never `PASS`.

### Root-cause triage of the two L500 verification misses (one cycle, decisive)

1. *Where does the time go?* The verification receipts show the full schedule
   completed (`v016-stage` and `v016-commit` records for all 10/100 Commits,
   `v016-sdk-range-edit` for all SDK edits) before the outer deadline stopped the
   command; the tail is the verifier, not the schedule.
2. *Is it the schedule or the fixture?* L100 K10 verifies in 4.33 s and L100 K100
   in 12.06 s (Δ ≈ 7.7 s for 90 extra states ≈ 86 ms/state). L500 has 6× the
   paths and 5× the bytes, so its per-state verification cost is ≈ 6× L100's:
   ≈ 26 s of per-state work for the L500 K10 schedule alone, before the extra
   K100 states. The miss therefore scales with the declared per-state
   verification work, not with a constant.
3. *Independent confirmation at a declared allowance:* the L500 exhaustive case,
   whose own declared watchdog is 300 s, verified its 101 states for 298.28 s
   without finishing, while the same case at L100 finished in 38.33 s. A ~8×
   factor between L100 and L500 is consistent with the 6× path/5× byte ratio.
4. *Owner: neither a product defect nor a harness defect was demonstrated.* The
   verifier is doing the declared work (complete namespace inventory, recomputed
   content roots for every sub-128 KiB regular file, declared large-file ranges,
   every retained commit identity and parent edge) and the product's own
   verification allowance is deliberately wider (600 s cap in
   `src/v016_mixed.rs`), so the stop comes from the campaign's declared regular
   ceiling. No oracle, limit or workload was weakened.

**Escalated to the owner (rule 1 of the escalation list):** the two L500 regular
verification rows cannot fit the 25 s ceiling without weakening the declared
verification. They are retained as `TIMEOUT`/gate `FAIL`; the owner needs to rule
between a declared larger exception and a `NOT_RUN` disposition.

**Defects found and fixed in F2: none.** One harness robustness gap was fixed:
an invocation that never reached the product (Docker daemon flap: `docker image
inspect` returned "No such image" for a tag that existed) produced no sample
record; the driver now classifies that as infrastructure-invalid, retains the
failed attempt and re-runs the invalid pair once — it never re-samples a case that
produced a sample. The four invalid attempts are retained under
`benchmark-results/v016/f2-seed1/…/run-<timestamp>/`.

## L4 — F3 `multi_workspace_development` (4 regular + 1 declared extension, seed 1)

```bash
python3 benchmark/fs-bench-pro/shared/v016_rollout.py \
  --image layerfs-bench-infra:8ef48ec2b762f720 --tag f3-seed1 \
  --family multi_workspace_development --prepare
python3 benchmark/fs-bench-pro/shared/v016_rollout.py \
  --image layerfs-bench-infra:8ef48ec2b762f720 --tag f3-seed1 \
  --family multi_workspace_development --extended --prepare
```

### Regular cases

| case | perf | perf wall | verdict | verify | verify wall | verdict | Created | workers | overlap |
| --- | --- | ---: | --- | --- | ---: | --- | ---: | ---: | ---: |
| `…100mb-5000-k10-v1` | PASS | 3.33 s | PASS | PASS | 5.89 s | PASS | 20 | 2 | 187.0 ms |
| `…100mb-5000-k100-v1` | PASS | 6.94 s | PASS | PASS | 21.12 s | **EXCEPTION (declared)** | 200 | 2 | 195.5 ms |
| `…500mb-30000-k10-v1` | PASS | 5.78 s | PASS | TIMEOUT | 22.91 s | **FAIL** | 20 | 2 | 125.8 ms |
| `…500mb-30000-k100-v1` | PASS | 15.85 s | **EXCEPTION (declared)** | TIMEOUT | 22.95 s | **FAIL** | 200 | 2 | 132.2 ms |

* Both live workspaces are real: `/workspace/a` and `/workspace/b`, one worker
  record each, `observed_overlap_ns` 125–196 ms with
  `concurrency_claim = observed-overlap-required`. Every Commit is `Created`
  (`UpToDate = Busy = HeadMoved = presentation_failures = 0`), and the discard/reopen
  witness is emitted by the branch's own schedule
  (`DISCARD_COMMIT = 5`, i.e. after local Commit 5 of the primary branch).
* Declared exceptions (allowed to 25 s, listed by case):
  `…100mb-5000-k100-v1` verification 21.12 s and `…500mb-30000-k100-v1`
  performance 15.85 s.
* The two L500 verifications are the same miss as F2 (per-state verification of a
  30 000-path fixture), stopped at 22.91 s / 22.95 s with the schedule complete.

### Declared extension

| case | mode | status | wall | watchdog |
| --- | --- | --- | ---: | ---: |
| `v016-workspace-four-100mb-5000-k100-v1` | performance **and** verification, separately | **PASS / PASS** | 9.54 s / 33.05 s | 60 s each |

400 Commits across four workspaces (`/workspace/a`–`/workspace/d`), all `Created`,
`observed_overlap_ns = 82.0 ms`, each mode inside its own 60 s watchdog.

**Defects found and fixed in F3: none.** The two L500 verification misses are the
F2 escalation, not a new defect.

**Verdict accounting fix (harness, not an oracle):** the driver's wall-only gate
classified a killed invocation with a stop wall under the ceiling as an
"exception". A killed or failed invocation is a failure whatever its stop wall, so
the rendered matrix now derives the verdict from status *and* wall
(`TIMEOUT` → `FAIL`). No measured number changed.

## L5 — F4 `branch_development` (4 of 6 registered; the two compact controls are missing)

```bash
python3 benchmark/fs-bench-pro/shared/v016_rollout.py \
  --image layerfs-bench-infra:8ef48ec2b762f720 --tag f4-seed1 \
  --family branch_development --prepare
```

| case | perf | perf wall | verdict | verify | verify wall | verdict | Created | workers |
| --- | --- | ---: | --- | --- | ---: | --- | ---: | ---: |
| `v016-branch-mixed-100mb-5000-k10-v1` | PASS | 3.38 s | PASS | PASS | 7.17 s | PASS | 30 | 3 |
| `v016-branch-mixed-100mb-5000-k100-v1` | PASS | 7.06 s | PASS | TIMEOUT | 22.62 s | **FAIL** | 210 | 3 |
| `v016-branch-mixed-500mb-30000-k10-v1` | PASS | 6.49 s | PASS | TIMEOUT | 22.98 s | **FAIL** | 30 | 3 |
| `v016-branch-mixed-500mb-30000-k100-v1` | PASS | 17.52 s | **EXCEPTION (declared)** | TIMEOUT | 22.91 s | **FAIL** | 210 | 3 |

* Trunk10 then children forked from trunk commit 5: `retained_roots` 31/211,
  `longest_ancestry` 15/105, every Commit `Created`, three workers with observed
  overlap.
* `v016-branch-convergent-content-v1` and `v016-branch-fork-descendant-v1` are
  **`NOT_READY`**: neither was ever implemented on the archived line, and the
  compact-control registration, the compact schedule and its oracle do not exist
  in this tree. `infra-list branch_development` reports four of the six declared
  case IDs. This is the F4 blocker and it also gates two of F6's six consumers.
* The three verification misses are the F2 escalation with better attribution
  (see L3 addendum below).

### L5 addendum — attribution of the verification misses (measured, not inferred)

The killed receipts carry the product's own schedule timer, which separates the
schedule replay from the verifier:

| row | verify schedule timer (product) | verify invocation wall | tail = verifier + custody + cleanup |
| --- | ---: | ---: | ---: |
| mixed L500 K10 | 1.07 s | 22.91 s stopped | ≈ 21.8 s |
| branch L100 K100 (210 Commits) | 8.74 s | 22.62 s stopped | ≈ 13.9 s |

The schedule replay is not the problem: the mixed L500 K10 verify replays its
schedule in 1.07 s and still cannot finish. The tail is the declared verification
scope — complete namespace inventory, independently recomputed content roots for
every sub-128 KiB regular file, declared large-file ranges, every retained commit
identity and parent edge, plus custody. That cost is per path (≈0.5–0.7 ms/path
measured across L100 ≈ 5 000 paths and L500 ≈ 30 000 paths), so a 30 000-path
complete inventory alone needs ≈ 15–20 s. This is a measurement of the frozen
verification scope, not of the product's Commit path.

## L6 — F5 `dedup_branch_history` (4 of 6; the namespace-inode orchestrator is missing)

```bash
python3 benchmark/fs-bench-pro/shared/v016_rollout.py \
  --image layerfs-bench-infra:8ef48ec2b762f720 --tag f5-seed1 \
  --family dedup_branch_history \
  --case v016-history-large-hotset-k10-v1 --case v016-history-large-hotset-k100-v1 \
  --case v016-history-namespace-inode-k10-v1 --case v016-history-namespace-inode-k100-v1 \
  --case v016-history-boundary-cycle-k10-v1 --case v016-history-boundary-cycle-k100-v1 --prepare
```

| case | perf | perf wall | verdict | verify | verify wall | verdict |
| --- | --- | ---: | --- | --- | ---: | --- |
| `v016-history-large-hotset-k10-v1` | PASS | 1.81 s | PASS | PASS | 3.73 s | PASS |
| `v016-history-large-hotset-k100-v1` | PASS | 2.23 s | PASS | PASS | 3.62 s | PASS |
| `v016-history-boundary-cycle-k10-v1` | PASS | 1.89 s | PASS | PASS | 3.77 s | PASS |
| `v016-history-boundary-cycle-k100-v1` | PASS | 2.68 s | PASS | PASS | 3.81 s | PASS |
| `v016-history-namespace-inode-k10-v1` | **FAIL** | 1.77 s | FAIL | **FAIL** | 1.69 s | FAIL |
| `v016-history-namespace-inode-k100-v1` | **FAIL** | 1.67 s | FAIL | **FAIL** | 1.95 s | FAIL |

The four measured rows are comfortable: the S fixture (16 files, 2 658 304 B) keeps
both modes under 4 s, and the K10 prefix relationship holds.

**Root cause of the two `namespace-inode` failures (harness, missing
implementation, not a product defect):** the container workload prints
`fs-benchmark-workload: unsupported dedup native workload` and exits 1, because
`namespace-inode` is deliberately excluded from the SDK route
(`dedup_workloads::is_sdk`) and the family's own `v016_edits` returns
`"v0.1.6 namespace-inode compact schedule is not implemented yet: the five declared
HN stages have no host orchestrator"`. The frozen HN schedule
(`benchmark-families.md` §dedup_branch_history) needs five stages per cycle —
unlink/recreate two tiny files, two single-file SDK 256 B overwrites, rename the
populated `tiny` directory, alias + chmod + mtime, then a 4 KiB atomic save over
the aliased destination with alias-inode observation and removal — with four POSIX
helper executions and two SDK calls per cycle. Neither the orchestrator nor its
per-state oracle exists in this tree. Retained as `FAIL` with the exact workload
error; not re-labelled, not sampled again. This also gates two of F6's six
consumers (`v016-access-inode-before/after-v1`).

**Defects found and fixed in F4/F5: none** (the port behaved as documented; both
misses are missing implementation and one measurement-scope finding).

## L7 — Phase Q: seeds 2 and 3 for the regular matrix

```bash
python3 benchmark/fs-bench-pro/shared/v016_rollout.py --image layerfs-bench-infra:8ef48ec2b762f720 \
  --tag q2 --seed 2 --prepare
python3 benchmark/fs-bench-pro/shared/v016_rollout.py --image layerfs-bench-infra:8ef48ec2b762f720 \
  --tag q3 --seed 3 --prepare
```

One performance and one separate verification invocation per case and seed, prepared
inputs, `--setup clone`, fresh append-only outputs. Machine-readable result:
`docs/roadmap/0.1/0.1.6/evidence/issue154/phase-q-seeds.json`.

| case | perf s1 / s2 / s3 (s) | verify s1 / s2 / s3 (s) |
| --- | --- | --- |
| `v016-boundary-{small-control,below,exact,above,large-control,roundtrip,alias-roundtrip}-v1` | PASS 1.86–2.42 / 1.85–2.16 / 1.86–2.05 | PASS 1.68–2.41 / 1.79–2.24 / 1.68–2.19 |
| `v016-history-large-hotset-k10/k100-v1` | PASS 1.81/2.23, 2.09/2.57, 1.95/2.82 | PASS 3.62–3.85 in all three seeds |
| `v016-history-boundary-cycle-k10/k100-v1` | PASS 1.89/2.68, 1.89/2.35, 1.96/2.54 | PASS 3.62–3.97 in all three seeds |
| `v016-history-namespace-inode-k10/k100-v1` | FAIL 1.67–2.00 (missing HN orchestrator) | FAIL 1.65–1.95 (same) |
| `v016-mixed-development-100mb-5000-k10-v1` | PASS 3.17 / 3.00 / 5.44 | PASS 4.33 / 4.08 / 4.21 |
| `v016-mixed-development-100mb-5000-k100-v1` | PASS 6.58 / 7.62 / 10.62 | PASS 12.06 / 12.70 / 13.56 |
| `v016-mixed-development-500mb-30000-k10-v1` | PASS 5.23 / 5.44 / 6.48 | **FAIL** 22.94 / 22.81 / 22.89 |
| `v016-mixed-development-500mb-30000-k100-v1` | EXCEPTION 14.53 / 17.78 / 17.67 | **FAIL** 23.01 / 23.02 / 22.86 |
| `v016-workspace-mixed-100mb-5000-k10-v1` | PASS 3.33 / 3.30 / 5.50 | PASS 5.89 / 5.94 / 6.06 |
| `v016-workspace-mixed-100mb-5000-k100-v1` | PASS 6.94 / 7.18 / 9.77 | EXCEPTION 21.12 / 20.32 / 21.52 |
| `v016-workspace-mixed-500mb-30000-k10-v1` | PASS 5.78 / 5.76 / 5.60 | **FAIL** 22.91 / 22.85 / 22.89 |
| `v016-workspace-mixed-500mb-30000-k100-v1` | EXCEPTION 15.85 / 15.56 / 16.07 | **FAIL** 22.95 / 22.93 / 22.98 |
| `v016-branch-mixed-100mb-5000-k10-v1` | PASS 3.38 / 3.39 / 3.80 | PASS 7.17 / 7.29 / 7.60 |
| `v016-branch-mixed-100mb-5000-k100-v1` | PASS 7.06 / 7.30 / 7.48 | **FAIL** 22.62 / 22.71 / 22.76 |
| `v016-branch-mixed-500mb-30000-k10-v1` | PASS 6.49 / 7.00 / 7.26 | **FAIL** 22.98 / 22.95 / 22.99 |
| `v016-branch-mixed-500mb-30000-k100-v1` | EXCEPTION 17.52 / 17.33 / 17.38 | **FAIL** 22.91 / 22.88 / 22.97 |
| the eight unregistered regular cases | — | — |

Totals over the 33 regular cases × 3 seeds × 2 modes = 198 slots: **PASS 106,
declared exception 11, FAIL 33, not run 48** (the eight unregistered cases account
for all 48). The verdicts are stable across seeds: every PASS/EXCEPTION/FAIL class
is the same at seeds 1, 2 and 3 for every registered case, and the walls agree
within 15 % except where noted below.

### Two conditions that had to be corrected before the seed 2/3 numbers were valid

1. **First-use preparation inside a gate invocation (seed 2, first attempt,
   retained under `benchmark-results/v016/q2-regular/`).** Without `--prepare`, the
   master construction ran inside the same invocation that was gated, inflating
   the complete command wall by 13–16 s for every L500 case and turning two rows
   into `FAIL_BUDGET` (32.42 s and 28.92 s). The harness separates explicit
   first-use preparation (its own selected step, 120 s/300 s watchdogs) from the
   15 s gate, so that sweep is retained as an invalid-preparation sweep and the
   whole seed-2 matrix was re-collected with prepared inputs in tag `q2`. Product
   timers prove the attribution: at seed 2 the schedule itself moved only
   10.43 s → 13.55 s while `preparation_wall_ns` moved 2.36 s → 16.45 s.
2. **Host contention (seed 3, first attempt).** Eight rows were measured while the
   host carried unrelated desktop load (load average 27 on 14 CPUs, plus three
   leftover sample containers from wrapper-killed invocations). Their walls were
   1.7–9.3× the seed-1/2 values and three crossed the budget class (e.g.
   `v016-branch-mixed-100mb-5000-k10-v1` 31.65 s against 3.38/3.39 s). The
   criterion was fixed before the re-take (≥1.8× both other seeds, or a budget
   class change). The leftovers were removed, the driver now records the host
   load with every invocation and removes only its own orphaned containers, and
   the eight rows were re-taken once in tag `q3`; they returned to 5.60–17.67 s
   (perf) and 22.76–22.99 s (verify). Contaminated attempts are retained beside
   the live receipts as `run-<timestamp>`.

## L8 — Phase X: the three declared extensions (seed 1)

| case | declared mode | status | wall | watchdog |
| --- | --- | --- | ---: | ---: |
| `v016-mixed-exhaustive-100mb-5000-k100-v1` | verify-only, perf `N/A` | **PASS** | 38.33 s | 120 s |
| `v016-mixed-exhaustive-500mb-30000-k100-v1` | verify-only, perf `N/A` | **TIMEOUT** | 298.28 s | 300 s |
| `v016-workspace-four-100mb-5000-k100-v1` | performance + verification separately | **PASS / PASS** | 9.54 s / 33.05 s | 60 s each |

Performance is never reported as 0 or `PASS` for a verify-only case. Declared
omission: `cases.json` lists seeds 1/2/3 for the extensions, but Phase X is specified
for seed 1 and the L500 exhaustive alone costs its full 300 s watchdog per attempt,
so the extensions were collected at seed 1 only. That omission is stated, not
papered over.

## L9 — F6 `historical_access` (0 of 6; the access route does not exist)

`infra-list historical_access` returns no rows: `families/historical_access/fixture.json`
is still the inherited 11-case artifact, byte-identical to the archived line, and
none of `v016-access-{boundary-before,boundary-after,inode-before,inode-after,fork-point,divergent-head}-v1`
is registered or runnable. The archived line never implemented the route that
mounts one selected retained state of a sealed producer, so this is implementation
work, not port work. All six rows are retained as **`NOT_READY`** at all three
seeds; performance for them stays `N/A` — never 0 and never `PASS`.

Two of the six also depend on producers that do not exist yet:
`v016-access-inode-before/after-v1` need the F5 namespace-inode history
(`history-namespace-inode-k100`, Commit 94/95) and
`v016-access-fork-point/divergent-head-v1` need the F4 compact controls.

## L10 — verification redundancy removed; the seven stopped rows now pass (supersedes L3/L5)

Owner rulings this phase: **the performance allowance for a v0.1.6 invocation is
60 s**, the **verification gate is unchanged** (15 s target, 25 s declared
exception ceiling, 3 s cleanup reserve), and **one run per case per mode** — no
repeated samples and no seed 2/3 requirement. Making a verification fit is a job
for removing redundant work, never for the ceiling.

### What was redundant (profile first, then fix)

A macOS `sample` of the `workspace-run` child process — not the host wrapper, which
only `wait4`s — showed where the time went, and two defects were fixed at the root:

1. **A full namespace walk rebuilt per verified range** (`497138431`). 95 % of the
   L500 verification's CPU sat in
   `verify_root_against_shadow → verify_declared_range → namespace_view`: the
   complete O(persisted paths) traversal was rebuilt for *every* declared
   large-file range, while the caller already held that path's record. The
   comparison itself is unchanged — same bytes read from the Store, same
   byte-for-byte comparison (`verify_declared_range_at` takes the file-state root
   the caller resolved).
2. **Super-linear re-validation of shared recipe subtrees** (`a6cec6736`).
   `Content::slice` wraps one shared `Arc<Content>` source, so a plain recursive
   `validate()` re-validated the same subtree once per path to it; the M1 oracle
   replays a schedule that splices the same declared content every cycle. The walk
   now visits each distinct node once; every check (bounds, digest-custody rule,
   sha256 form, length overflow) still runs on every distinct node.
3. **Repeated authenticated reads of one file-state** (`ed9cacebd`). The declared
   length of a persisted regular file is a property of its published file-state
   root, so the first read for a root is the check and later paths reuse it.

### Before → after on the rows that were stopped (seed 1, same identity per pair)

| case | verification before | verification after |
| --- | ---: | ---: |
| `v016-mixed-development-500mb-30000-k10-v1` | 22.94 s **stopped** | 7.72 s **PASS** |
| `v016-mixed-development-500mb-30000-k100-v1` | 23.01 s **stopped** | 20.36 s **PASS** (declared exception) |
| `v016-workspace-mixed-500mb-30000-k10-v1` | 22.91 s **stopped** | 10.00 s **PASS** |
| `v016-workspace-mixed-500mb-30000-k100-v1` | 22.95 s **stopped** | 20.62 s **PASS** (declared exception) |
| `v016-branch-mixed-100mb-5000-k100-v1` | 22.62 s **stopped** | 8.30 s **PASS** |
| `v016-branch-mixed-500mb-30000-k10-v1` | 22.98 s **stopped** | 13.12 s **PASS** |
| `v016-branch-mixed-500mb-30000-k100-v1` | 22.91 s **stopped** | 22.83 s **PASS** (declared exception, boundary) |

No oracle, coverage, limit or workload was weakened: every declared range is still
read and compared, the complete namespace inventory is still proved per branch,
every retained commit identity and parent edge is still checked, and the counters
are still compared against the frozen table. The escalation of L3/L5 is therefore
**resolved by the product-side fix**, not by a ruling.

### Final seed-1 matrix (one run per case per mode)

Identity: source commit `ed9cacebd095f40f061e89ee2e45f1e360c85a99`,
`SOURCE_DIRTY=false`, source seal
`221f445fabac7e39807f96ac7bd507ffbb7311983a4c3f8ce2419e59cc3bd5b5`, product seal
`970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd` (unchanged: this
work touched only the benchmark harness and oracle), image
`layerfs-bench-infra:221f445fabac7e39`, host binary
`cda339a4263c968d3e77eee96edc30a9400d84fd112651f81432814da0079041`.

Committed evidence: `evidence/issue154/final-seed1-matrix.json` (25 rows).

| verdict class | slots of 50 (25 registered cases × 2 modes) |
| --- | ---: |
| PASS inside the 15 s target | **40** |
| declared ≤25 s exception (listed by case) | **6** |
| FAIL | **4** — the two `namespace-inode` cases only |

Declared exceptions: `mixed …500mb-30000-k100-v1` perf 18.21 s / verify 20.36 s;
`workspace …500mb-30000-k100-v1` perf 16.54 s / verify 20.62 s; `branch
…500mb-30000-k100-v1` perf 16.52 s / verify 22.83 s.

The four FAIL slots are `v016-history-namespace-inode-k{10,100}-v1` (perf and
verify each): the workload exits with `unsupported dedup native workload` because
the frozen five-stage HN schedule has no host orchestrator. Their measured walls
(1.6–1.9 s) show they are not budget misses; they are missing implementation.

The eight unregistered regular cases (two compact branch controls, six
`historical_access` cases) remain `NOT_READY` — never implemented on the archived
line either.

### Scope note

Per the owner's instruction this phase collected **one run per case per mode at
seed 1**. The earlier seeds 2 and 3 sweeps remain on disk
(`benchmark-results/v016/q2`, `q3`, `final-seed2`, `final-seed3`) as retained
history, but they are no longer the qualification criterion and no further
repeated sampling was taken.

## L11 — the ten remaining regular cases implemented; one verification row at the ceiling

**What was implemented** (all three pieces are new work, none of them existed on the
archived line either): F5's HN history orchestrator, F4's two compact branch
controls, and F6's `historical_access` route with the producer sealing it needs.
Two harness defects were found and fixed at the root while doing it, and one
frozen-spec conflict was resolved with evidence. Exact commands at the end.

### L11.1 F5 — the HN (`namespace-inode`) compact history schedule

`workload/v016_hn.rs` declares the five stages, the role arithmetic of fixture S
(the movable `tiny`/`tiny-moved` directory, `t0`/`t1` refresh, `t2`/`t3` SDK edit
targets, `t4` alias/replacement) and the exact state after every one of the
K commits; `v016-hn-stage` executes the four POSIX stages inside the mount and the
host orchestrator drives the two-call SDK stage and the Commit that follows every
stage. Declared envelope: S+2 names and S+8192 logical bytes (the alias is charged
its referent length and the atomic save's temporary is the second extra name),
directories unchanged — asserted for all 100 states of K100 and all three seeds.

A product-free rehearsal (`v016_hn::rehearsal`) runs the real stage code against a
natively created fixture and proves every intermediate state with
`verify_native`, which is what caught the one thing that is *not* derivable on
paper: renaming an entry changes the parent directory's automatic mtime, so the
fixture root's declared mode and mtime are set again in stage 3 exactly as every
other directory this schedule touches. That test is retained.

| case | perf | perf wall | verify | verify wall |
| --- | --- | ---: | --- | ---: |
| `v016-history-namespace-inode-k10-v1` | PASS | 1.97 s | PASS | 4.10 s |
| `v016-history-namespace-inode-k100-v1` | PASS | 3.28 s | PASS | 9.80 s |

Verification runs the case's own declared selection
`[0, 1, 2, 49, 50, 94, 95, 99, 100]` — commits 94 and 95 are the states the two
F6 inode consumers read — with 100 Created commits, 101 retained roots, topology
and every parent edge checked, and the standing omission of the fast history route
(`unselected historical snapshot content; exhaustive historical object/storage
census`) stated beside it.

### L11.2 F4 — the two compact branch controls

`v016_stages.rs` declares the compact topology (trunk10; A10/B10 forked from trunk
commit 5; the descendant control's C10 forked from A's local commit 5), the
ancestral-ordinal arithmetic (`offset 4096*((j-1) mod 2)`, payload
`floor((j-1)/2) mod 2` over regions initialised to Z), the branch salts and the
per-control counters; `src/v016_compact.rs` drives one live workspace at a time and
proves the graph and every branch head against the declaration.

| case | perf | perf wall | verify | verify wall |
| --- | --- | ---: | --- | ---: |
| `v016-branch-convergent-content-v1` | PASS | 2.28 s | PASS | 2.45 s |
| `v016-branch-fork-descendant-v1` | PASS | 2.09 s | PASS | 2.09 s |

Measured on the final identity: 30 and 40 Created commits, longest ancestry 15 and
20, and the control's own content contract — the convergent children publish the
*same* first-medium-file content root (`82b78c4d…`), the descendant control's B and
C publish branch-salted roots (`d45ef36b…`, `57bee450…`) that also differ from A's.

**One frozen-contract clarification.** `retained_graph_roots` counts
initial/commit state *references*, not distinct root ObjectIds
(`benchmark-families.md` §dedup_branch_history), and the convergent control makes
identical content changes on both children. The product's content-addressed
identity therefore collapses trunk commits 6..10 onto A's local commits 1..5 (and
B's onto both): 30 Created commits resolve to 15 distinct commit identities and 16
distinct roots, while the *reference* count stays 31 as declared. The oracle
records both numbers and asserts the declared reference count; it does not assert
that the distinct count equals it.

### L11.3 F6 — `historical_access`: the route, the sealing, and the six cases

Each access case mounts exactly one selected retained state of a sealed producer
with one live workspace and creates no commits. `infra-seal-producer` publishes the
producer schedule once, during fixture preparation, with its own container; the
access invocation only resolves the declared branch and ordinal, forks it, runs the
declared full reads twice inside the mount (reader pass and verifier pass) and
compares every byte with the producer's own declaration recomputed on the host.
Performance is `N/A` for all six — the route refuses to run in performance mode.

| case | producer / state | declared bytes | verify | verify wall |
| --- | --- | ---: | --- | ---: |
| `v016-access-boundary-before-v1` | boundary-cycle k100, commit 48, below role | 131 071 | PASS | 1.69 s |
| `v016-access-boundary-after-v1` | boundary-cycle k100, commit 49, below role | 131 072 | PASS | 2.10 s |
| `v016-access-inode-before-v1` | namespace-inode k100, commit 94, target + alias | 8 192 | PASS | 2.08 s |
| `v016-access-inode-after-v1` | namespace-inode k100, commit 95, target, alias ENOENT | 4 096 | PASS | 1.91 s |
| `v016-access-fork-point-v1` | compact convergent, trunk commit 5, first medium | 65 536 | PASS | 1.83 s |
| `v016-access-divergent-head-v1` | compact descendant, B local commit 10, first medium | 65 536 | PASS | 1.82 s |

The inode pair proves stat/nlink equivalence inside the mount (`nlink=2`, one
device and inode for both names) and the replacement state proves the alias
`ENOENT`; both are re-proved by the verifier pass.

### L11.4 The boundary-cycle exchange direction (frozen-spec conflict, resolved)

The frozen access table and `cases.json` require the below-role file at commit 49
to be exactly 131 072 B. The implemented exchange moved the byte the other way
(131 071→131 070 and 131 073→131 074), so no commit ordinal of that producer could
ever publish 131 072 B, and the "boundary-cycle" profile never touched the boundary
it is named for. The frozen one-line schedule ("exchange one byte between
131071/131073 files, shrinking first, then reverse next commit") admits both
directions; the two consumer rows admit only one. The exchange now moves the higher
file's last byte **down** into the lower one, so the pair oscillates between
(131 071, 131 073) and (131 072, 131 072) — every other commit puts both names
exactly on the 128 KiB boundary and leaves the `exact` control unchanged, as
declared. The two `boundary-cycle` rows are re-collected on the new identity
(2.09 s / 3.78 s and 2.66 s / 3.84 s, all PASS).

Consequence for the oracle: a splice that carries a file **across** the exact
boundary is not an extension of the previous extent list — the published
representation changes class, so the product re-encodes the whole file (measured,
not assumed: the first form of the fix failed with
`actual payload transcript differs from independent oracle` at the very step the
class changes, and the mismatch report now names the path and both
decompositions). `expected_transcripts` therefore transcribes exactly those paths
from the declared content, as a fresh import is, and leaves every other path on the
splice model. No check was removed: the recoded path is still compared byte for
byte through its own canonical decomposition.

### L11.5 Two harness defects fixed at the root

1. **The drive-level preparation wrote its master where the runner never looks.**
   `prepare_v016_fixture.py` keyed its prepared input by
   `{family, case, seed, source, recipe}` while `runner.py::_host_acquire` keys it by
   `{contract, fixture, schema_sha256, seed}`. Every `--prepare` therefore populated
   a cache the gated invocation does not consult, and the first gated invocation of
   each case re-constructed (and, for an access case, would have *unsealed*) the
   master inside its own complete-command window — the 13–16 s first-use cost L7
   measured. Preparation now writes at the key the runner acquires, and
   `_host_acquire` seals an access case's producer itself if it ever has to build
   the master, so no path can produce an unsealed access input.
2. **The prepared master's identity omitted one file.**
   `prepare_v016_fixture.py` computed `host_tree_identity` *before* writing
   `host-owner.json`, so its own cache entry failed the runner's identity check
   (`host prepared Store content mismatch`). Fixed ordering; the owner marker is
   part of the identity, `host-cache.json` is excluded by the helper.

### L11.6 Registry and bookkeeping

`workspace_registry` now declares sixteen families and **169** timed IDs
(`[8,20,12,4,4,16,10,10,20,14,26,7,5,6,6,1]`): `branch_development` 4 → 6 and the
new `historical_access` family with its six additions. `sample_slot_count` 507.
Harness layout and host-family counts updated with it; `historical_access` gained
the `setup.sh`/`perf.sh`/`verify.sh` entrypoints every family has.

### L11.7 Final seed-1 matrix (one run per case per mode)

Identity: source seal `86f14b2d68ece2ae368f8aec29070520505a61c7aa69b445414b64561640e954`,
product seal `970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd`
(**unchanged**: this work touched only the benchmark harness, the oracle and the
producer declarations), compilation seal
`679b17f0e17636ae419144edb7377be03dd1709ac362d72ea0dff61d3b89306b`, dependency seal
`374f4dfa08f98ced734e540f62dd4a8b0edb5a88c090c051ae704fbf7f67faf1`, image
`layerfs-bench-infra:86f14b2d68ece2ae` = `sha256:af6a400356e36b2f58e5eb9df07a291d8567cb975adbb183ed5d76b0c5e7268d`,
host binary `fcaa14d8decf96a6247d95a037e83eea7322aeb66a51d418acbee2e21fdd6aa7`,
harness identity `42ace192351e57ee429e09dbb6480dfffb43e65045c154f72131a99ac7626ada`.

`LAYERFS_SOURCE_DIRTY = true`: the tree carries another writer's uncommitted
`docs/roadmap/0.1.7` study material, which is outside the source seal's scope
(`crates/`, `tools/`, `benchmark/fs-bench-pro/`). The receipts name build-time HEAD
`8357b1e336d1e16eae349a3313c5f3dbdc777b82`; the seal-covered content is byte-identical
to the committed `823f556ca` (the rebuild after that commit produced the *same*
source seal and image tag), so the matrix is valid for the committed source, and
the recorded commit hash is the one that was HEAD when the binary was sealed.

Committed evidence: `evidence/issue154/final-complete-matrix.json` (33 rows) and
`evidence/issue154/final-complete-extended-matrix.json` (3 rows).

| verdict class | slots of 39 (33 regular × 2 modes, minus six perf `N/A`) |
| --- | ---: |
| PASS inside the 15 s target | **29** perf/verify rows |
| declared ≤25 s exception (listed by case) | **6** rows |
| performance `N/A` (the six access cases, declared) | **6** rows |
| **TIMEOUT** | **1** row |

Declared exceptions: `mixed …500mb-30000-k100-v1` perf 19.62 s / verify 22.15 s;
`workspace …500mb-30000-k100-v1` perf 15.92 s / verify 20.61 s; `branch
…500mb-30000-k100-v1` perf 17.12 s / verify 22.99 s.

**The one non-passing row** is `v016-branch-mixed-500mb-30000-k100-v1` verification:
the host command was stopped by the declared complete-command window at a 22.99 s
wall, after the 210-commit replay, both published heads and two of the three
complete branch-state proofs had already been emitted. It was re-taken **once** on
the same identity after the machine quietened (22.92 s → 22.99 s, host load
8.4/8.8 on 14 CPUs both times) and both attempts are retained; L10 recorded the same
row at 22.83 s PASS on the earlier identity, so it sits directly on the 25 s
ceiling. No oracle, coverage or limit was changed to move it: it is reported as a
`TIMEOUT` with its measured wall and escalated for an owner ruling (a declared
larger verification exception, or `NOT_RUN`).

### L11.8 Extensions re-collected on the final identity

| case | mode | status | wall | watchdog |
| --- | --- | --- | ---: | ---: |
| `v016-mixed-exhaustive-100mb-5000-k100-v1` | verify-only | **PASS** | 15.42 s | 120 s |
| `v016-mixed-exhaustive-500mb-30000-k100-v1` | verify-only | **PASS** | 62.68 s | 300 s |
| `v016-workspace-four-100mb-5000-k100-v1` | perf + verify | **PASS / PASS** | 8.04 s / 9.24 s | 60 s |

The L500 exhaustive replay that TIMEOUTed at 298.28 s in L8 now completes in
62.68 s on its own 300 s watchdog, and the L100 exhaustive dropped from 38.33 s to
15.42 s: the verifier redundancy removed in L10 is what the extension walls were
paying for. Performance for the two verify-only cases stays `N/A`, never 0 and
never `PASS`.

### L11.9 Defects found and fixed in this phase

| # | defect | root cause | fix |
| --- | --- | --- | --- |
| 1 | drive-level preparation cached a master the runner never acquires; first-use construction stayed inside the gate | two different cache keys for one artifact | `prepare_v016_fixture.py` keys by the runner's compatibility record |
| 2 | the prepared master failed the runner's identity check | owner marker written after the identity was computed | write it first |
| 3 | the boundary-cycle pair never reached the exact 128 KiB boundary its consumers read | exchange direction inverted | move the higher file's byte down; re-collect the two rows |
| 4 | the transcript oracle could not model a file crossing the representation class | splice model assumes the surrounding decomposition survives | transcribe the recoded path from the declared content |

### L11.10 Reproduction

```bash
python3 benchmark/fs-bench-pro/shared/runner.py --build-host
IMG=$(python3 benchmark/fs-bench-pro/shared/runner.py --build-image)
python3 benchmark/fs-bench-pro/shared/v016_rollout.py \
  --image "$IMG" --tag final2-seed1 --prepare --seed 1
python3 benchmark/fs-bench-pro/shared/v016_rollout.py \
  --image "$IMG" --tag final2-ext --prepare --extended --seed 1
python3 benchmark/fs-bench-pro/shared/v016_report.py --tag final2-seed1 \
  --out docs/roadmap/0.1/0.1.6/evidence/issue154/final-complete-matrix.json
python3 benchmark/fs-bench-pro/shared/v016_report.py --tag final2-ext \
  --out docs/roadmap/0.1/0.1.6/evidence/issue154/final-complete-extended-matrix.json
```

Product-free gates before the measurement: `python3 docs/roadmap/0.1/0.1.6/check_plan.py`
(`PASS`, 33 regular + 3 extended), `rustc --edition 2021 --test workload/main.rs`
(19 passed, including the HN rehearsal and the compact-control checks),
`target/release/fs-benchmark-pro workspace-self-check`
(`timed_case_count 169`, `sample_slot_count 507`) and
`tools/preflight.sh` (all steps passed).

## L12 — owner ruling: one declared verification exception (supersedes L11.7's residual row)

**Ruling.** The owner declared a larger verification ceiling for the single row
that could not fit the pre-existing window after the L10 redundancy work:
`v016-branch-mixed-500mb-30000-k100-v1` carries a **30-second** complete-command
ceiling instead of 25 seconds. It is a *declared exception*, not a silent watchdog
change: it is keyed by the exact registered ID in
`runner.py::V016_VERIFY_EXCEPTIONS`, the driver reports the row as `EXCEPTION`
with its measured wall (`v016_rollout.py::VERIFY_EXCEPTIONS`), and the ledger and
the group report carry the number and its arithmetic.

**What the ruling does *not* change.** The 15-second family target is still reported
separately (`performance_target_status` / `historical_product_target_status`) and a
row above it is still a TARGET_MISS. The three-second cleanup reserve still sits
inside the ceiling, so the worker stop is 27 seconds. Every *other* regular
verification keeps the unchanged 25-second ceiling — a new harness test asserts
that, so the exception cannot widen by accident. No oracle, coverage, path
selection, fixture or limit was touched: the verifier still proves three complete
branch heads over 29 984 paths each, every retained commit identity and parent
edge, and the declared counters.

**Measured arithmetic behind the number** (same identity as L11.7, before the
ruling): complete command 22.99–23.07 s stopped at the 22.0 s work window
(25 s − 3 s reserve). Of that, 2.56 s is setup, ~11.4 s is the 210-commit replay
(`pure_call_sum_ns` 22.77 s summed over the two concurrent children) and ≈7.7 s is
the proof phase. The row therefore needs ~23–24 s of complete command; 30 s leaves
the 3 s reserve plus a bounded margin for host noise, and is deliberately narrower
than the 60 s performance allowance and than the extended cases' 60/120/300 s
watchdogs.

**Re-collected row** (harness revision with the declared exception, same image and
source content as L11.7):

| case | perf | perf wall | gate | verify | verify wall | gate |
| --- | --- | ---: | --- | --- | ---: | --- |
| `v016-branch-mixed-500mb-30000-k100-v1` | PASS | 18.18 s | EXCEPTION (≤25 s) | **PASS** | **24.17 s** | declared exception (≤30 s) |

The verification now completes: the third complete branch-state proof, the counter
identity and the verification record are all published instead of being cut at the
work window. The two stopped attempts (22.92 s and 22.99 s) stay on disk as
retained history.

**Root cause still open.** The ≈7.7 s proof phase verifies the same published
inode records, metadata roots and file-state roots once per branch head; the two
children inherit most of the trunk's states. Sharing those per-object results
across branch proofs — the L10 pattern, one scope wider — would remove that
repetition and could bring the row back inside the 25-second ceiling. That work is
*not* required by this ruling and is not a substitute for it: the exception stands
as declared, and the redundant verification remains a named follow-up.

**Re-collection scope.** The ruling changes `harness_identity` (it covers
`runner.py`/`v016_rollout.py`), so the whole regular matrix and the three
extensions were re-collected once on the new revision rather than pooling rows
from two harness identities: `final3-seed1` (33 rows) and `final3-ext` (3 rows),
committed as
`evidence/issue154/final-complete-matrix.json` and
`evidence/issue154/final-complete-extended-matrix.json`.
