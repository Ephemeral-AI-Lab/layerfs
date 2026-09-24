# #232 Workspace Exec/FUSE edit baseline (scenario version 2)

> **Status:** all 56 registered cases were attempted and receipted under one
> frozen release identity. 45 completed and passed the independent bounded
> oracle; 11 structural shifts failed inside the product's 5-second Exec
> progress window and are retained as `FAIL`. No row is `GOAL_MET`: every row
> is `INELIGIBLE` under the frozen cache contract, and every raw number is
> above its historical target. Issue #232 is **not** complete and must not be
> closed. Nothing here is a release-admission PASS.

This is the scenario-version-2 baseline. The version-1 registry, its Phase 4
sample and the
[Phase 4/5 evidence](exec-fuse-edit-phase4-5-evidence.md) remain historical and
were not rewritten.

## 1. Identity

| item | value |
|---|---|
| source commit | `1b4dfdf7efc702e67fe335ea3cad6a0d50fd550d` |
| source tree | `7bfde2ee9f41ce84d94ab188b1c4e8b45c6af3dd` |
| source dirty | `False` |
| product seal | `9a8d6644fdf2ced0fb5219fa219d32a15d86f4f2e99b10b2eb51bc8e24356f9c` |
| harness seal | `7d037a57af936b313cf0f9f4e1b4a8abd0733883d55636d2c0ff20bcaeb236a9` |
| Cargo lock | `d6fb4800e048346f87b6c825bdc2f2d446f50e33a2ead40799147408b8dc7e9b` |
| registry | `registry/workspace-exec-edit-v2.json` `05a133b543db33729d60cc321cc88682046c16bfb1761daec6a57c5df3b11823` (scenario version 2, 56 cases) |
| historical registry | `registry/workspace-exec-edit-v1.json` `e4b4d2fc67cb1630dc8f15283db3085663466071bb373c829a6026ffddb66aab` (scenario version 1) |
| image | `sha256:07898cbabbeab35eb57d8fdcd43125b17386fae91968d9d3f3863c75d62e7815` (release, base `alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`) |
| edit tool | `70d41e2868196bad3a70e266aaf8e20796965154aaa5da70964879d193545ff9` |
| daemon | `e10b3ab6303b6949560bebf9a33518d7f40fe41cdf9784698a2b5d79cea4a9c9` |

One identity, one image and one sample per case: every receipt in the campaign
carries exactly these values, and the campaign directory holds 56 case folders
plus the campaign log. Case evidence (case record, driver receipt, raw LFT1,
`perf.jsonl`, receipt) lives under
`benchmark-results/fs-bench-pro/sdk-exec-fuse/<family>/<scenario-id>/`.

## 2. What the timer covers

`edit_commit_ns` is one `layerfs-telemetry` caller operation
(`sdk.edit_commit.fuse`) that starts immediately before `WorkspaceApi::exec`
and stops on the typed `WorkspaceApi::commit` result, with `edit` and `commit`
children. Every product operation — `Server::open`, `ProjectApi::fork`,
`SandboxApi::create`, `WorkspaceApi::{mount,exec,commit,status,unmount}` and
`SandboxApi::delete` — goes through the public `layerfs-sdk` release driver;
Python prepares fixtures, supervises the 15-second complete-command budget,
checks cache eligibility, retains receipts and runs the separate verifier. The
byte copy, Branch fork, Sandbox create, Mount, Status, Unmount, Sandbox delete
and verification are outside the operation timer and are reported separately.

Commands (one fresh output path per case):

```text
python3 core/benchmark/fs-bench-pro/runner.py run --case <scenario-id> \
  --out benchmark-results/fs-bench-pro/exec-fuse-edit-v2-campaign/<scenario-id> \
  --verification skipped
python3 core/benchmark/fs-bench-pro/runner.py verify-edit \
  --run benchmark-results/fs-bench-pro/exec-fuse-edit-v2-campaign/<scenario-id>
python3 core/benchmark/fs-bench-pro/runner.py report-edit \
  --runs benchmark-results/fs-bench-pro/exec-fuse-edit-v2-campaign \
  --out benchmark-results/fs-bench-pro/exec-fuse-edit-v2-campaign
```

## 3. The 56 registered rows

| scenario | bytes | algorithm | edit+commit ms | target ms | terminal | verifier | coverage | command s |
|---|---:|---|---:|---:|---|---|---|---:|
| `overwrite-head-4k-on-1mib-ops-1` | 1048576 | single-positional-write | 44.12 | 6.31 | INELIGIBLE | PASS | full-file | 0.84 |
| `overwrite-head-4k-on-10mib-ops-1` | 10485760 | single-positional-write | 40.91 | 5.90 | INELIGIBLE | PASS | full-file | 0.84 |
| `overwrite-head-4k-on-100mib-ops-1` | 104857600 | single-positional-write | 49.13 | 5.81 | INELIGIBLE | PASS | full-file | 0.88 |
| `overwrite-head-4k-on-500mib-ops-1` | 524288000 | single-positional-write | 67.63 | 7.34 | INELIGIBLE | PASS | bounded-windows-2 | 0.86 |
| `overwrite-middle-4k-on-1mib-ops-1` | 1048576 | single-positional-write | 34.37 | 5.63 | INELIGIBLE | PASS | full-file | 0.85 |
| `overwrite-middle-4k-on-10mib-ops-1` | 10485760 | single-positional-write | 46.90 | 5.70 | INELIGIBLE | PASS | full-file | 0.96 |
| `overwrite-middle-4k-on-100mib-ops-1` | 104857600 | single-positional-write | 45.30 | 6.43 | INELIGIBLE | PASS | full-file | 0.85 |
| `overwrite-middle-4k-on-500mib-ops-1` | 524288000 | single-positional-write | 75.86 | 10.01 | INELIGIBLE | PASS | bounded-windows-2 | 0.96 |
| `overwrite-tail-4k-on-1mib-ops-1` | 1048576 | single-positional-write | 40.40 | 4.72 | INELIGIBLE | PASS | full-file | 0.84 |
| `overwrite-tail-4k-on-10mib-ops-1` | 10485760 | single-positional-write | 42.23 | 6.06 | INELIGIBLE | PASS | full-file | 0.95 |
| `overwrite-tail-4k-on-100mib-ops-1` | 104857600 | single-positional-write | 41.94 | 5.82 | INELIGIBLE | PASS | full-file | 0.84 |
| `overwrite-tail-4k-on-500mib-ops-1` | 524288000 | single-positional-write | 69.59 | 7.37 | INELIGIBLE | PASS | bounded-windows-1 | 0.85 |
| `insert-middle-4k-on-1mib-ops-1` | 1048576 | shift-grow | 155.01 | 5.30 | INELIGIBLE | PASS | full-file | 1.10 |
| `insert-middle-4k-on-10mib-ops-1` | 10485760 | shift-grow | 2905.70 | 6.40 | INELIGIBLE | PASS | full-file | 3.81 |
| `insert-middle-4k-on-100mib-ops-1` | 104857600 | shift-grow | 5002.79 | 6.25 | FAIL | NOT_RUN | - | 10.86 |
| `insert-middle-4k-on-500mib-result-capped-v2-ops-1` | 524283904 | shift-grow | 5003.24 | 7.76 | FAIL | NOT_RUN | - | 10.87 |
| `delete-middle-4k-on-1mib-ops-1` | 1048576 | shift-shrink | 127.16 | 5.06 | INELIGIBLE | PASS | full-file | 0.97 |
| `delete-middle-4k-on-10mib-ops-1` | 10485760 | shift-shrink | 2900.32 | 5.75 | INELIGIBLE | PASS | full-file | 3.82 |
| `delete-middle-4k-on-100mib-ops-1` | 104857600 | shift-shrink | 5002.66 | 6.41 | FAIL | NOT_RUN | - | 10.86 |
| `delete-middle-4k-on-500mib-ops-1` | 524288000 | shift-shrink | 5002.96 | 12.16 | FAIL | NOT_RUN | - | 10.86 |
| `append-tail-4k-on-1mib-ops-1` | 1048576 | single-positional-write | 35.25 | 5.76 | INELIGIBLE | PASS | full-file | 0.85 |
| `append-tail-4k-on-10mib-ops-1` | 10485760 | single-positional-write | 34.67 | 5.71 | INELIGIBLE | PASS | full-file | 0.85 |
| `append-tail-4k-on-100mib-ops-1` | 104857600 | single-positional-write | 44.17 | 5.60 | INELIGIBLE | PASS | bounded-windows-1 | 0.86 |
| `append-tail-4k-on-500mib-result-capped-v2-ops-1` | 524283904 | single-positional-write | 72.30 | 6.40 | INELIGIBLE | PASS | bounded-windows-1 | 0.97 |
| `prepend-head-4k-on-1mib-ops-1` | 1048576 | shift-grow | 273.87 | 5.12 | INELIGIBLE | PASS | full-file | 1.12 |
| `prepend-head-4k-on-10mib-ops-1` | 10485760 | shift-grow | 5003.60 | 5.65 | FAIL | NOT_RUN | - | 10.85 |
| `prepend-head-4k-on-100mib-ops-1` | 104857600 | shift-grow | 5002.09 | 6.54 | FAIL | NOT_RUN | - | 10.90 |
| `prepend-head-4k-on-500mib-result-capped-v2-ops-1` | 524283904 | shift-grow | 5004.74 | 6.73 | FAIL | NOT_RUN | - | 10.85 |
| `replace-grow-middle-2k-to-4k-on-1mib-ops-1` | 1048576 | shift-grow | 121.44 | 5.75 | INELIGIBLE | PASS | full-file | 0.95 |
| `replace-grow-middle-2k-to-4k-on-10mib-ops-1` | 10485760 | shift-grow | 2959.96 | 6.81 | INELIGIBLE | PASS | full-file | 3.90 |
| `replace-grow-middle-2k-to-4k-on-100mib-ops-1` | 104857600 | shift-grow | 5003.46 | 8.44 | FAIL | NOT_RUN | - | 10.85 |
| `replace-grow-middle-2k-to-4k-on-500mib-result-capped-v2-ops-1` | 524285952 | shift-grow | 5003.93 | 8.63 | FAIL | NOT_RUN | - | 10.87 |
| `replace-shrink-middle-4k-to-2k-on-1mib-ops-1` | 1048576 | shift-shrink | 117.20 | 5.30 | INELIGIBLE | PASS | full-file | 0.94 |
| `replace-shrink-middle-4k-to-2k-on-10mib-ops-1` | 10485760 | shift-shrink | 2910.64 | 5.89 | INELIGIBLE | PASS | full-file | 3.81 |
| `replace-shrink-middle-4k-to-2k-on-100mib-ops-1` | 104857600 | shift-shrink | 5003.35 | 7.13 | FAIL | NOT_RUN | - | 10.87 |
| `replace-shrink-middle-4k-to-2k-on-500mib-ops-1` | 524288000 | shift-shrink | 5002.87 | 7.35 | FAIL | NOT_RUN | - | 10.87 |
| `truncate-tail-4k-on-1mib-ops-1` | 1048576 | bounded-truncate | 33.52 | 5.03 | INELIGIBLE | PASS | full-file | 0.87 |
| `truncate-tail-4k-on-10mib-ops-1` | 10485760 | bounded-truncate | 30.63 | 9.14 | INELIGIBLE | PASS | full-file | 0.85 |
| `truncate-tail-4k-on-100mib-ops-1` | 104857600 | bounded-truncate | 38.41 | 5.44 | INELIGIBLE | PASS | full-file | 0.85 |
| `truncate-tail-4k-on-500mib-ops-1` | 524288000 | bounded-truncate | 62.30 | 7.19 | INELIGIBLE | PASS | bounded-windows-1 | 0.82 |
| `zero-extend-tail-4k-on-1mib-ops-1` | 1048576 | bounded-extend | 33.19 | 4.94 | INELIGIBLE | PASS | full-file | 0.82 |
| `zero-extend-tail-4k-on-10mib-ops-1` | 10485760 | bounded-extend | 31.55 | 5.52 | INELIGIBLE | PASS | full-file | 0.84 |
| `zero-extend-tail-4k-on-100mib-ops-1` | 104857600 | bounded-extend | 40.86 | 5.76 | INELIGIBLE | PASS | bounded-windows-1 | 0.84 |
| `zero-extend-tail-4k-on-500mib-result-capped-v2-ops-1` | 524283904 | bounded-extend | 69.59 | 6.50 | INELIGIBLE | PASS | bounded-windows-1 | 0.96 |
| `overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1` | 1048576 | single-positional-write | 36.53 | 6.20 | INELIGIBLE | PASS | full-file | 0.85 |
| `overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1` | 10485760 | single-positional-write | 36.79 | 7.24 | INELIGIBLE | PASS | full-file | 0.83 |
| `overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1` | 104857600 | single-positional-write | 46.29 | 7.46 | INELIGIBLE | PASS | full-file | 0.86 |
| `overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1` | 524288000 | single-positional-write | 80.00 | 8.13 | INELIGIBLE | PASS | bounded-windows-2 | 0.96 |
| `overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1` | 1048576 | single-positional-write | 41.23 | 6.84 | INELIGIBLE | PASS | full-file | 0.88 |
| `overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1` | 10485760 | single-positional-write | 40.79 | 6.90 | INELIGIBLE | PASS | full-file | 0.83 |
| `overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1` | 104857600 | single-positional-write | 45.80 | 8.01 | INELIGIBLE | PASS | full-file | 0.94 |
| `overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1` | 524288000 | single-positional-write | 80.22 | 9.27 | INELIGIBLE | PASS | bounded-windows-2 | 0.97 |
| `overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1` | 1048576 | single-positional-write | 38.59 | 6.02 | INELIGIBLE | PASS | full-file | 0.93 |
| `overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1` | 10485760 | single-positional-write | 45.62 | 6.63 | INELIGIBLE | PASS | full-file | 0.95 |
| `overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1` | 104857600 | single-positional-write | 56.14 | 13.54 | INELIGIBLE | PASS | full-file | 0.88 |
| `overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1` | 524288000 | single-positional-write | 72.36 | 7.98 | INELIGIBLE | PASS | bounded-windows-2 | 0.88 |

`attempted 56/56`, `completed 45`,
`verifier PASS 45`, `verifier FAIL 0`,
`verifier TIMEOUT 0`, `driver FAIL 11`,
`driver timeout 0`, `INELIGIBLE 45`,
`GOAL_MET 0`, `TARGET_MISS 0`,
`raw at or below target 0`,
`raw above target 56`,
`sandbox delete confirmed 56/56`,
`complete command inside budget 45/56`.
No attempt was retried, no case was dropped, and no timeout or failure was
relabelled.

## 4. Independent verification

The verifier is a separate identity-matched command with its own 15-second hard
cap (<10 s design goal). It reopens the case's own Store and history read-only
through public C1/C2/C5 readers, re-derives the expected bytes from the frozen
recipe, and checks the published Branch head, the retained pristine genesis
root, the final size, the declared oracle windows and — for the
`edit_canonical_chunk_count` family — the pinned canonical file root and chunk
count.

| verification wall | rows |
|---|---|
| 1 MiB final (full-file digest) | 39-69 ms |
| 10 MiB final (full-file digest) | 97-110 ms |
| 100 MiB final (full-file digest) | 637-692 ms |
| 500 MiB final (bounded windows, 196,608 declared bytes) | 38-47 ms |

Coverage: 34 rows verified with a full-file
digest, 5 with two declared bounded
windows, 6 with one declared bounded
window. Every completed row passed: size, content digest, canonical root
differs from the pristine root, extent count, published head commit, and the
unchanged pristine genesis root/extent count. The 11 failed rows have no
verification receipt because there is no completed Commit to verify; they are
retained as `NOT_RUN` verification, never as PASS.

## 5. Findings

### 5.1 A Workspace Exec that runs longer than 5 seconds without output fails

Every structural shift that needed more than five seconds of FUSE work failed
at 5.002-5.005 s:

| scenario | moved MiB | edit s | edit+commit s | detail |
|---|---:|---:|---:|---|
| `insert-middle-4k-on-100mib-ops-1` | 50 | 5.002777459 | 5.002787792 | exec failed: Failure(Failure { code: Unknown, unknown: true, cleanup: None, history: None }) |
| `insert-middle-4k-on-500mib-result-capped-v2-ops-1` | 250 | 5.003202625 | 5.003241917 | exec failed: Failure(Failure { code: Unknown, unknown: true, cleanup: None, history: None }) |
| `delete-middle-4k-on-100mib-ops-1` | 50 | 5.002628083 | 5.002660209 | exec failed: Failure(Failure { code: Unknown, unknown: true, cleanup: None, history: None }) |
| `delete-middle-4k-on-500mib-ops-1` | 250 | 5.002944958 | 5.002964875 | exec failed: Failure(Failure { code: Unknown, unknown: true, cleanup: None, history: None }) |
| `prepend-head-4k-on-10mib-ops-1` | 10 | 5.003548709 | 5.003598292 | exec failed: Failure(Failure { code: Unknown, unknown: true, cleanup: None, history: None }) |
| `prepend-head-4k-on-100mib-ops-1` | 100 | 5.002074083 | 5.0020895 | exec failed: Failure(Failure { code: Unknown, unknown: true, cleanup: None, history: None }) |
| `prepend-head-4k-on-500mib-result-capped-v2-ops-1` | 500 | 5.004684542 | 5.00474375 | exec failed: Failure(Failure { code: Unknown, unknown: true, cleanup: None, history: None }) |
| `replace-grow-middle-2k-to-4k-on-100mib-ops-1` | 50 | 5.003439167 | 5.00345575 | exec failed: Failure(Failure { code: Unknown, unknown: true, cleanup: None, history: None }) |
| `replace-grow-middle-2k-to-4k-on-500mib-result-capped-v2-ops-1` | 250 | 5.003917583 | 5.003931917 | exec failed: Failure(Failure { code: Unknown, unknown: true, cleanup: None, history: None }) |
| `replace-shrink-middle-4k-to-2k-on-100mib-ops-1` | 50 | 5.0033105 | 5.003351042 | exec failed: Failure(Failure { code: Unknown, unknown: true, cleanup: None, history: None }) |
| `replace-shrink-middle-4k-to-2k-on-500mib-ops-1` | 250 | 5.002840584 | 5.0028655 | exec failed: Failure(Failure { code: Unknown, unknown: true, cleanup: None, history: None }) |

The failure is not a benchmark timeout: `driver_timeout` is false for all
eleven, the driver ran its own cleanup, and the complete command finished in
10.85-10.90 s. The measured wall is the product's own bound. The source reading
that matches the measurement: the native bridge caps every *silent* wait at
`IO_PROGRESS_MS = 5000` (`core/crates/layerfs-bridge/src/contract/request.rs`),
and both the socket and pipe waiters re-arm that cap from the last observed
activity (`adapters/native/connection.rs` `Socket::read`,
`adapters/native/pipe.rs` `Pipe::ready`). A one-shot Exec whose child produces
no output until it exits never refreshes that window, so the host surfaces
`Failure { code: Unknown, unknown: true }` while the daemon is still working.
The daemon-side exec child is then killed by its own error path, and the row's
`unmount` reports failure while `SandboxApi::delete` still confirms removal.

This is a **product-path limit on the Exec route**, not a property of the
benchmark: no registered case may be shrunk to fit it, and the 500 MiB
structural cases stay registered with this exact outcome.

### 5.2 The in-place shift cost is superlinear in the number of blocks

The 20 structural cases move the affected suffix through the mounted FUSE file
in 128 KiB blocks. Where the shift completed, the cost of one block grows with
the number of blocks already moved:

| scenario | moved MiB | blocks | driver | edit ms | ms/block |
|---|---:|---:|---|---:|---:|
| `delete-middle-4k-on-1mib-ops-1` | 0.5 | 4 | COMPLETE | 81.18 | 20.3 |
| `replace-shrink-middle-4k-to-2k-on-1mib-ops-1` | 0.5 | 4 | COMPLETE | 75.52 | 18.9 |
| `replace-grow-middle-2k-to-4k-on-1mib-ops-1` | 0.5 | 4 | COMPLETE | 75.01 | 18.8 |
| `insert-middle-4k-on-1mib-ops-1` | 0.5 | 4 | COMPLETE | 105.85 | 26.5 |
| `prepend-head-4k-on-1mib-ops-1` | 1.0 | 8 | COMPLETE | 205.88 | 25.7 |
| `delete-middle-4k-on-10mib-ops-1` | 5.0 | 40 | COMPLETE | 2731.50 | 68.3 |
| `replace-shrink-middle-4k-to-2k-on-10mib-ops-1` | 5.0 | 40 | COMPLETE | 2711.58 | 67.8 |
| `replace-grow-middle-2k-to-4k-on-10mib-ops-1` | 5.0 | 40 | COMPLETE | 2810.93 | 70.3 |
| `insert-middle-4k-on-10mib-ops-1` | 5.0 | 40 | COMPLETE | 2733.45 | 68.3 |

A descriptive two-term fit over the 9 completed shift
rows (4-40 blocks) gives
`edit_ns ~= 15.5 ms * blocks + 1.33 ms * blocks^2`.
The fit is a description of the measured domain, not a prediction of the
500 MiB rows: those never completed, because the 5-second Exec window closed
first. The scaling alone puts a 2,000-block (250 MiB) shift far outside the
15-second complete-command budget, and the quadratic term is the reason the
route cannot be treated as "slow but linear".

### 5.3 The cache contract keeps every row INELIGIBLE

The macOS Store domain was invalidated and checked for every row and reported
0 resident pages of the whole domain. The Linux FUSE/backing domain is
`INELIGIBLE` by the frozen contract: the per-case sandbox creates its backing
and spool inside the container, Commit may read the bytes Edit just wrote from
resident backing pages, and no product-side or host-side invalidation exists.
No eligibility rule was changed and no diagnostic number is called eligible.
The raw `edit_commit_ns` remains a declared-warm diagnostic.

### 5.4 Every raw number is above its historical target

`raw_at_or_below_target` is 0 of 56. The fastest rows are the non-structural
edits at 30.63-80.22 ms against historical G2 targets of 4.72-13.54 ms. This is consistent with the Phase 4 observation and
is reported as a miss; the targets were not moved.

## 6. Custody and retained attempts

All 56 rows confirmed `SandboxApi::delete`; `docker ps -a` and `docker volume
ls` report no retained container or volume after the campaign. The complete
command stayed inside its 15-second budget in every row (max 10.90 s); the 11
failed rows spent their 5 seconds inside the product's Exec window and the rest
in confirmed teardown.

Retained, not pooled:

| attempt | identity | outcome |
|---|---|---|
| `exec-fuse-edit-phase4-04` | scenario version 1, commit `2ed00cc87`, image `sha256:cb51ce52…` | one 1 MiB overwrite sample, `INELIGIBLE`; not reinterpreted |
| `exec-fuse-edit-v2-phaseB` (8 rows) | scenario version 2, commit `1be1c59e9`, harness seal `78e08b5e…` | harness-speed defect: the frozen registry was re-derived three times per runner process (~39 s of pure harness wall per case). Superseded by the campaign above; its receipts stay on disk and are not pooled with the campaign |
| `exec-fuse-edit-v2-campaign` (56 rows) | commit `1b4dfdf7e`, harness seal `7d037a57…` | the baseline above |

The reporting path was extended after collection to read the separate retained
`verification.json` beside each receipt and to add the verifier wall, coverage
and raw-vs-target columns. That change touches neither the timer, the driver,
the verifier, the cache contract nor any receipt; the receipts keep the harness
seal they were collected with.

## 7. What remains blocked

* 11 of 56 rows have no completed Edit+Commit: the 10 MiB prepend, the five
  100 MiB structural cases and the five 500 MiB structural cases. They are
  `FAIL` at the product's 5-second Exec window (§5.1).
* 0 of 56 rows are `GOAL_MET`. The 45 completed rows are `INELIGIBLE` under the
  frozen cache contract (§5.3); the raw numbers are above target (§5.4).
* The 45 completed rows carry an eligible-route, independently verified,
  cleanup-confirmed sample; they are diagnostics, not release admission.
* No optimization was attempted in this session: the measured causes are
  recorded here for a separately approved product change.

## 8. Evidence package

| file | content |
|---|---|
| `evidence/exec-fuse-edit-v2-baseline-20260924-01/report-edit.tsv` | the 56-row runner report |
| `evidence/exec-fuse-edit-v2-baseline-20260924-01/report-edit.json` | the same rows with per-case fields and campaign counts |
| `evidence/exec-fuse-edit-v2-baseline-20260924-01/receipts.jsonl` | one compact line per registered case, in registry order |
| `evidence/exec-fuse-edit-v2-baseline-20260924-01/shift-scaling.tsv` | structural shift blocks, wall and per-block cost |
| `evidence/exec-fuse-edit-v2-baseline-20260924-01/summary.json` | identity, counts, coverage and findings |
| `evidence/exec-fuse-edit-v2-baseline-20260924-01/issue-comment.md` | the #232 status comment |
