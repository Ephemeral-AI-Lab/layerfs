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
