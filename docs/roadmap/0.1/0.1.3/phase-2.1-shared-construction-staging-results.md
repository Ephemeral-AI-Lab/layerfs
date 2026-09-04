# Phase 2.1 shared construction and staging results

Status: `APPROACH_B_REFACTOR_TERMINAL_PASS`

The shared construction, admission, staging, publication-ownership, input-shape,
and selective-verification deliverables are complete at
`95578a5e24ac15f38a07535dfdf1fcc9fee80065`. The 100,000-file / 500,000,000-byte
namespace target remains an explicit **MISS**: the final fixed-eight-core candidate
median was 2.564084876 seconds, not at most 2.2 seconds, and its 6.18 CPU-second
median was 98.56% of the matched baseline rather than at most 90%. This result is
not a namespace performance pass or a release-readiness claim.

## Source ledger

- Implementation base: `4c9b14a6b489eb6de08d4bfd0d4a723745013ab4`.
- Final implementation: `95578a5e24ac15f38a07535dfdf1fcc9fee80065`,
  tree `e8d5dd6251fcc0a01c9328ab0345b3dbb848b346`, branch
  `codex/phase21-shared-construction`.
- Dedicated integration worktree after evidence recovery:
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-p21-integration`.
- Authoritative local specification SHA-256:
  `954f2b5c453f1ea0bde052313716cc25230449fbb6a24c067ec73dfd3e7130b2`.
- Handoff prompt SHA-256:
  `52c6856ab4475ff36ea0a2bcb49f1eafe5e75c14cce846d795423a91fae34b5e`.
- Experiment checkout: base `a40b17e05486e5b747b689e7710475d739556a69`,
  committed revision `69bca3950a02f465f36aafb3d93bdf0e9eda991f`, tracked
  prototype diff SHA-256
  `bde89ea9e9061d360bc5415025acab5c9182be366da8054c9baf49c64f2fb853`.
- No-rollback amendment: commit
  `7d78ee7e68373bfcf6ec562f13e07d67b2572372`, local SHA-256
  `68789626c6be122d266e45f09a07fd1a6e34fab4a7bd9b06d00a3eb6475ab7be`.
- Final Workspace build seals: source
  `d4dbf7f0d3f7171f832438bfaf735806b7f12c46b004c5c69442295e879953ef`,
  product `06094354e417d373a4f391104fb3b5f2c2b5c414d9d83385b89d083af6ff6c02`,
  harness `28baa5b3b8bbbcd34afb555f83a3ea679c7dc51e1e9bef3cc2e348d3e3b0d2d8`.

The final Linux diagnostic binaries were rebuilt directly from the two committed
trees. Their SHA-256 identities are
`9ecf8dc25e9ca1bc8ef968eb7d7a446719f19bced44742719590954671fd9e82`
for the baseline and
`f296bf1acd6318fd207d64953863830fa8ba723c5c5378191bbe1c8fb7c68200`
for the candidate.

## Integrated boundaries

The implementation is split into six local commits so each boundary remains
reviewable:

| Commit | Integrated result |
| --- | --- |
| `e11abde3f` | Exact operation-scoped eight-entry portable-metadata result cache and sorted affected-page directory/inode construction with bounded scratch ownership. |
| `a849a1308` | Shared checked object insertion and schema v5 staging/publication primitives. |
| `30b54c44c` | Workspace-local construction ownership, short conditional publication, exact returned-snapshot continuation, and retained-publication recovery. |
| `14b00ca68` | The single explicit-selection `verify-selected.py` companion. |
| `9929c27dc` | Affected construction lint corrections with no product-route change. |
| `95578a5e2` | Explicit selected proof-only reliability dispatch in the companion. |

Schema v5 adds exactly:

```sql
workspace_stages(workspace_id, branch_id, root_id)
```

There is no status, generation, timestamp, conflict, receipt, lock, per-file,
rollback, or GC schema. A complete validated root is staged, its Branch head is
changed only by the existing conditional transaction, and the stage is retired.
A no-op follows the same stage/retire path without adding history. Head movement
retains the stage and does not undo another publisher. Publication success followed
by presentation or stage-cleanup failure is reported as published plus cleanup or
presentation failure, and recovery does not create a second Commit.

Workspace construction no longer holds a lifetime Store permit or Branch lease
while private file work or producers run. SQLite mutation remains serialized in
bounded admission/publication transactions. A continuing Workspace installs the
exact Commit/root returned by its own publication instead of rereading a newer
Branch head; only the explicit commit-and-close route skips installation.

The initializer, Workspace frontier builder, and localized change path call the
shared cache/batch updater through their real callers. Existing wrappers remain.
`construction.rs`, `commit_file.rs`, a universal executor, and a public bulk API
were not added: no current caller required another facade, and adding one would be
unused scaffolding rather than a reuse boundary.

## Bounded feasibility decision

The native matched baseline at the implementation base measured complete
initialization wall times of 3.386390708, 3.422449958, and 4.131761792 seconds;
product CPU was 12.410827625, 11.407668666, and 11.706084209 CPU-seconds.

Two and only two optimization hypotheses were run:

1. Per-worker exact metadata results reduced misses from 2,000 to 16, canonical
   frames from 439,056 to 425,168, and SQL submissions from 423,819 to 422,433.
   The 100,000-file sample was 3.331898584 seconds / 12.078214293 CPU-seconds,
   missing the explicit 2.5--2.75-second / 11-CPU-second continuation screen.
2. Passing authenticated structural IDs removed 103,000 redundant hashes:
   canonical hashes fell from 528,165 to 425,165 and exactly matched emitted
   frames. The sample was 3.287184166 seconds / 11.848280792 CPU-seconds and
   missed the same screen.

The useful work reductions were retained. They cannot remove the 500 MB source
read, 100,000 opens, 101,001 metadata observations, approximately 422,000 unique
canonical objects, or their SQLite B-tree writes. The conditional fixed-128 SQL
experiment was not run because its stated 2.5--2.75-second prerequisite was not
met. No worker, queue, cache, or SQL sweep followed. This selected Approach B.

## Namespace measurement ledger

All rows below are the 100,000-file / 500,000,000-byte namespace initializer.
Wall is the complete `initialize_layerstack` call, including final canonical
root and LayerStack publication. CPU is user plus system CPU for that same product
phase. The final terminal comparison uses the last six rows only.

| Cohort | Arm | Samples: wall seconds / CPU-seconds |
| --- | --- | --- |
| Native feasibility | baseline | `3.386390708 / 12.410827625`; `3.422449958 / 11.407668666`; `4.131761792 / 11.706084209` |
| Hypothesis 1 | candidate | `3.331898584 / 12.078214293` |
| Hypothesis 2 | candidate | `3.287184166 / 11.848280792` |
| Initial fixed-8 set, raw path later deleted | baseline | `2.526632960 / 6.32`; `4.114179960 / 7.78`; `3.593851502 / 6.99` |
| Initial fixed-8 set, raw path later deleted | candidate | `2.668513626 / 7.26`; `4.974437044 / 6.96`; `3.660614169 / 7.74` |
| Recovered set with invalid non-hex counter nonce | baseline | `2.573101210 / 6.15`; `2.546558459 / 6.45`; `2.502368667 / 6.22` |
| Recovered set with invalid non-hex counter nonce | candidate | `2.249678626 / 5.90`; `2.536243835 / 6.31`; `2.442280001 / 6.11` |
| **Final matched fixed-8 set** | **baseline** | `2.527612334 / 6.27`; `2.635165918 / 6.30`; `2.539132709 / 6.19` |
| **Final matched fixed-8 set** | **candidate** | `2.564084876 / 6.18`; `2.592729709 / 6.21`; `2.497588043 / 6.11` |

The first fixed-8 raw directories and the first selective-verification receipts
were deleted together with the original dedicated worktree by a stale nested
sealed-build cleanup. The measurements are retained above under their actual
identities rather than silently discarded. Evidence recovery then used a new
nonmatching custody path. The first recovered timing set is also retained, but a
non-hex diagnostic nonce suppressed its work-counter receipt; the corrected final
set is the terminal comparison.

### Final matched comparison

| Metric | Baseline | Candidate | Result |
| --- | ---: | ---: | --- |
| Median complete wall | 2.539132709 s | 2.564084876 s | +0.98%; target `<=2.2 s` **MISS** |
| Median product CPU | 6.27 CPU-s | 6.18 CPU-s | 98.56% of baseline; `<=90%` **MISS** |
| Maximum product CPU | 6.30 CPU-s | 6.21 CPU-s | candidate maximum passes |
| Maximum process peak RSS | 81,920,000 B | 83,529,728 B | reported separately |
| Maximum initialization incremental HWM | -- | 77,033,472 B | below 128 MiB |
| Maximum explicit named buffer ownership | 9,967,243 B | 9,975,250 B | below 10 MiB |
| SQLite connection-cache target | 32 MiB | 32 MiB | preserved |
| Swap / cgroup OOM / OOM-kill | 0 / 0 / 0 | 0 / 0 / 0 | pass |

Candidate work counters across the three final samples were:

- eight workers, direct path, zero active producers after completion;
- metadata cache hits 100,984 and misses 16 in every sample, versus baseline
  99,000 / 2,000;
- canonical frames and hashes both 425,159--425,195, versus baseline frames
  439,052--439,073 and hashes 542,052--542,073;
- SQL submitted rows 422,445--422,479, versus 423,852--423,879;
- no object-segment writes or rereads and no parent merge;
- one-MiB peak carried-slab queue and less than ten MiB simultaneous explicit
  buffer ownership;
- admission transactions peaked at 6,682 objects / 4,194,297 bytes;
- consumer idle was 231.621--235.465 ms, versus baseline 196.324--212.550 ms.

The candidate therefore has less real construction work and slightly lower
matched CPU, but the remaining source/admission work plus consumer idle does not
support the required wall or 10% CPU reduction. Its 0.98% wall difference is small
relative to the retained source/cache variation and is paired with lower CPU and a
lower maximum CPU; there is no unexplained reproducible performance regression.

Raw final evidence:

- read-only custody manifest:
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-p21-terminal-evidence/evidence.sha256`
  (SHA-256
  `264f1857f102b86039dd5eb35513d6234d8d34f05fcbfb3c25d8e2c7c1e97934`);
- baseline:
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-p21-terminal-evidence/fixed8-baseline-final-4c9b14a6`;
- candidate:
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-p21-terminal-evidence/fixed8-candidate-final-95578a5e`;
- independently revalidated input:
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-p21-terminal-evidence/fixture-namespace-100000-r2.txt`.

The input oracle reports 100,000 files, 1,000 data directories, 500,000,000
logical bytes, eight verifier workers, one-MiB maximum verifier buffer, and digest
`6fc793a9703bd0a21066f9fb12622c3451b16bd6ad7ef8b7382351351ac80a7e`.
The first recovery check correctly failed because a prior host-to-container copy
had changed only the fixture root mode/mtime; restoring `0750` and
`1700000000.0` made the unchanged generated descendants pass the independent
oracle before the final timing set.

## Functional and selective verification

Focused checks passed for:

- the cache's exact canonical keying, fixed eight-entry bound, and operation scope;
- all seven sorted directory/inode batch-updater cases, including canonical
  split/balance parity, sparse identity preservation, dense deletion, mixed
  boundaries, variable names, and reserve-before-allocation failure;
- structural same-seed identity handoff;
- direct discovery of root files plus directories;
- a 4,000-file single directory split into bounded tasks with exact canonical
  output;
- root hard-link fallback before publication;
- v4-to-v5 migration and exact three-column schema, no-op retirement, stale-head
  retention, and checked-insertion conflict authentication;
- Workspace owner isolation, exact returned-snapshot continuation after later
  publication, retained-stage freeze/discard, and published presentation-failure
  recovery without a second Commit.

The three affected crates passed `cargo check`. Focused tests passed after two
compile repairs and one test-ownership repair. Affected-crate Clippy passed with
`--no-deps -D warnings` plus the pre-existing `large-enum-variant` suppression;
unrelated pre-existing FUSE tuple and receipt-enum warnings were not changed. No
full workspace suite or #38/#39 family was run.

Exactly one companion exists:
`benchmark/fs-bench-pro/verify-selected.py`. It accepts one explicit
family/case/seed/source/assets selection, rejects bulk/range/implicit selection,
reuses the existing namespace and Workspace runners/oracles, reserves cleanup
time, writes one read-only `verification.json`, and cannot pass at or after 59
seconds.

Final selected receipts:

| Route | Status | End-to-end wall | Receipt |
| --- | --- | ---: | --- |
| `init_namespace / namespace-100 / seed 1 / candidate` | PASS | 10.768865292 s | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-p21-terminal-evidence/verification/namespace-100-95578a5e/verification.json` |
| `workspace_reliability / workspace-published-presentation-failure-proof / seed 1 / candidate` | PASS | 9.398156292 s | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-p21-terminal-evidence/verification/workspace-published-presentation-95578a5e/verification.json` |

The namespace receipt checks an independent fixture digest, a fresh reopened root,
bounded verifier resources, and cleanup. Its selected performance control was
84.912375 ms complete lifecycle with 59,998,208-byte peak RSS. The Workspace
receipt reuses the existing independent Workspace oracle; it confirms the
published-result/presentation-failure path, resource receipt, zero OOM, no timeout,
runtime stopped after client cleanup, and supervisor/mutable cleanup pass.

Both receipts explicitly omit exhaustive Phase 1 replay, per-sample full-file
verification, and history replay. Those omissions are intentional constraints,
not PASS claims.

## Terminal assessment and deferred applicability

Approach B is terminal because the complete reusable refactor, exact staging
foundation, short ownership path, input-shape controls, resource bounds, cleanup,
and bounded selected verification pass while the infeasible namespace performance
target remains truthfully marked MISS. Remaining risk is ordinary host/cache and
SQLite scheduling variation; it cannot turn the measured 2.564-second / 98.56%
CPU result into the required 2.2-second / 90% result.

The reusable interfaces are deliberately narrow: the eight-entry metadata builder
cache, sorted directory/inode updater, checked insertion, three-column stage API,
conditional publication result, and Workspace exact-snapshot recovery. Their
applicability to #38 and #39 is deferred for later user discussion. No #38/#39
implementation, execution, qualification, closure, or detailed plan occurred in
Phase 2.1.
