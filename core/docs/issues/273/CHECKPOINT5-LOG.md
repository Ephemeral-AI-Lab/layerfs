# #273 checkpoint 5 log

> **Status:** Executed checkpoint record. Twelve registered performance
> selections were run once per arm in row-major order with the control arm
> first, plus the five correctness/space selections' covering product rows.
> Product source is unchanged from `a2359620a`; the last product change before
> this checkpoint is `a2359620a7966314fbb2a96c98e8da958df72c6c`. No row is
> admission eligible, and no historical timing is used as a denominator.
> PR #274 stays draft and #273 stays open.

## 1. What ran, and under which identities

| Item | Identity |
| --- | --- |
| Candidate worktree | `/Users/yifanxu/.codex/worktrees/issue273-active-head/layerfs` |
| Candidate branch | `codex/issue273-active-head` |
| Candidate harness commit that produced the campaign | `1c2b2c1d0274a8267dbf410122a58d11574f1b47`, tree `f378d8de…` |
| Candidate product | unchanged since `a2359620a`; product source seal `1d6194737497ca0cecb5fb4c…` |
| Control worktree | `/Users/yifanxu/.codex/worktrees/issue271-root-custody/layerfs`, clean, HEAD `48b51e874a41b3e1e6c6661e145316df8b408f07`, tree `64d06dab…`; product source seal `fc6cd63637a64cfe2db1f7ad…` |
| Workload seal (both arms, identical) | `fdf35c10efc5ea5db65cf18b…` |
| Run harness seal (both arms, identical) | `6b4efffe9029acfb5125964b65a5c0ba2f988d84e3c727577eb1bc193d24ce1c` |
| Lockfile / `.cargo/config.toml` (both arms, identical) | `09b880a18e1c221ba8309084…` / `3a1863834c9fb76e20b17994…` |
| Toolchain | `rustc 1.85.1 (4eb161250 …)`, `cargo 1.85.1 (d73d2caf0 2024-12-31)`, profile `release` |
| Construction workers | `LAYERFS_CONSTRUCTION_WORKERS=1` in every arm |
| 10 MiB master | `#271 fourhop-patterns-prepared-v1/master`, Store `20ea70dc…`, history `55a02d7d…`, proof `d2f535e0…`, fixture `eb6183ad…` |
| 8,194-byte gate master | `#271 combined-baseline-prepared-v1/master`, Store `ce49eb72…`, history `96275c90…`, fixture `871461a4…` |
| Retained 4,097-record base | built once by the candidate arm, Store/history and head commit sealed in `retained-5/retained.json`, reused byte-identically by both arms |
| Shared independent oracle | `core/crates/layerfs-server/examples/verify_checkpoint5.rs`, binary `837df9a26eb73cdf…`, built once and reused unchanged by both arms |
| Writer | `writers/write-separated.c` `dc21c66d…`, static aarch64 binary `c1c9a4a22edd6b0c…` (identical in both arms) |
| Base image | `alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`; one image per declared write-sample interval, per arm |

Every campaign receipt carries the arm's own source commit, tree, product,
product-source, workload and harness seals; the candidate and control receipts
of each pair carry the same workload and harness seal and different product
seals. The *report generator* was extended after the campaign to separate the
two nested space scopes; its identity is recorded as
`report_generator_sha256=1492aec8920e6f87…` inside `report-1/report.json`, and
the harness that actually produced the attempts remains pinned by
`harness_seal=6b4efffe90…` in all 24 receipts.

Timed-command boundary: `complete_command_wall_ns` covers driver launch, Server
open, container create, mount, Exec, Commit, status, unmount and container
delete. `exec_ns`, `commit_ns`, `mount_ns` and `cleanup_ns` are the driver's own
monotonic inner timers. The verifier is a separate invocation with its own
9-second limit and never enters a speed comparison.

## 2. Campaigns and retained attempts

The attempts are append-only under
`benchmark-results/fs-bench-pro/issue273/checkpoint5/` (gitignored raw evidence).

| Directory | What it is |
| --- | --- |
| `prepared-candidate-failed-01…03`, `prepared-control-failed-01` | harness setup attempts that failed before any measured command (missing daemon build, missing output capture, non-serialisable verifier command, relative oracle path). Retained. |
| `retained-1` | retained-base preparation whose proof used the frozen 100-write arm verifier against a 4,097-write schedule. Retained as a defect record. |
| `campaign-1`, `campaign-2` | campaign attempts that failed on the harness's own lock handoff and on a duplicate attempt-directory create. No measured command ran. Retained. |
| `candidate-preflight-1…3`, `control-preflight-1…2` | labelled harness pre-flights (`issue273-harness-preflight-v1`, `registered_selection=false`). Not results. |
| `retained-2…5` | retained-base preparations; `retained-5` is the one used by the campaign. |
| **`campaign-3`** | **the campaign of record: 24 attempts, one per case per arm.** |
| `report-1` | derived tables and ratios, reproducible from `campaign-3` by the sealed report generator. |

## 3. Performance results — twelve selections, one sample per arm

Raw elapsed values are one observation each (`sample_count=1`); no median,
range, percentile or repeatability statement is implied. `wall` is the complete
command in seconds; the limit is the registered complete-command limit.

| # | Selection | Limit | control wall | candidate wall | control status | candidate status |
| ---: | --- | ---: | ---: | ---: | --- | --- |
| 1 | `issue273-append-100-10m-v1` | 15 s | 2.362 | **1.005** | INELIGIBLE | INELIGIBLE |
| 2 | `issue273-append-512-10m-v1` | 15 s | 3.552 | **1.540** | INELIGIBLE | INELIGIBLE |
| 3 | `issue273-append-4097-10m-v1` | 25 s | **25.007 (limit)** | **7.033** | **FAIL** | INELIGIBLE |
| 4 | `issue273-dispersed-100-10m-v1` | 15 s | 1.329 | **1.105** | INELIGIBLE | INELIGIBLE |
| 5 | `issue273-dispersed-512-10m-v1` | 15 s | 3.997 | **2.439** | **INCOMPLETE** | INELIGIBLE |
| 6 | `issue273-dispersed-4097-10m-v1` | 25 s | **25.007 (limit)** | **13.283** | **FAIL** | INELIGIBLE |
| 7 | `issue273-repeated-100-10m-v1` | 15 s | 1.228 | **1.013** | INELIGIBLE | INELIGIBLE |
| 8 | `issue273-repeated-512-10m-v1` | 15 s | 2.626 | **1.726** | INELIGIBLE | INELIGIBLE |
| 9 | `issue273-repeated-4097-10m-v1` | 25 s | 14.518 | **6.885** | INELIGIBLE | INELIGIBLE |
| 10 | `issue273-clean-commit-v1` | 15 s | 0.922 | 0.900 | **FAIL** | **FAIL** |
| 11 | `issue273-one-edit-commit-v1` | 15 s | 0.915 | 0.941 | **INCOMPLETE** | **INCOMPLETE** |
| 12 | `issue248-separated-4097-v1` (original #248 gate) | 25 s | **25.007 (limit)** | **7.789** | **FAIL** | **INCOMPLETE** |

Per-row raw metrics (`c`/`k` = control / candidate):

| # | Exec ms c/k | Commit ms c/k | Mount ms k | Cleanup ms c/k | Charged private backing B c/k | Quota charge B c/k | FUSE write class c/k |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 356.1 / 110.8 | 37.9 / 29.4 | 17.3 | 572.8 / 542.6 | 438,272 / 36,864 | 1,751,144 / 1,849,512 | 100 / 100 |
| 2 | 2,303.6 / 626.8 | 53.0 / 37.7 | 17.3 | 846.0 / 533.7 | 2,269,184 / 151,552 | 1,879,912 / 1,849,512 | 512 / 512 |
| 3 | — / 5,964.2 | — / 158.3 | 18.0 | — / 547.1 | — / 1,150,976 | — / 1,853,910 | — / 4,097 |
| 4 | 384.7 / 143.3 | 33.2 / 48.2 | 17.7 | 565.8 / 506.7 | 466,944 / 53,248 | 1,751,144 / 1,849,512 | 100 / 100 |
| 5 | 2,651.4 / 1,277.0 | 91.3 / 193.8 | 17.0 | 870.1 / 549.6 | 1,765,376 / 184,320 | 1,879,912 / 1,849,512 | 512 / 512 |
| 6 | — / 10,610.2 | — / 1,803.3 | 17.4 | — / 508.1 | — / 1,421,312 | — / 1,848,820 | — / 4,097 |
| 7 | 361.3 / 152.6 | 29.9 / 29.6 | 17.2 | 489.2 / 475.4 | 28,672 / 12,288 | 1,720,240 / 1,837,800 | 100 / 100 |
| 8 | 1,713.2 / 795.7 | 27.1 / 26.4 | 17.5 | 482.6 / 527.7 | 28,672 / 12,288 | 1,720,240 / 1,837,800 | 512 / 512 |
| 9 | 13,575.5 / 5,938.6 | 27.6 / 29.0 | 17.3 | 552.7 / 560.5 | 28,672 / 12,288 | 1,720,240 / 1,837,800 | 4,097 / 4,097 |
| 10 | 11.4 / 11.4 | 7.6 / 5.5 | 17.6 | 497.1 / 512.9 | n/a (no checkpoint) | 1,719,816 / 1,837,560 | 0 / 0 |
| 11 | 16.0 / 15.9 | 16.3 / 18.4 | 17.4 | 486.1 / 504.3 | n/a (no checkpoint) | 1,720,256 / 1,837,800 | 1 / 1 |
| 12 | — / 6,562.7 | — / 259.2 | 17.6 | — / 603.9 | — / 1,810,432 | — / 1,853,910 | — / 4,097 |

`Charged private backing` is `LFS_WRITE_SAMPLE backing.allocated_bytes`, the
whole private-backing physical charge; the metadata-host pages
(`metadata.allocated_bytes`) are a nested sub-scope of the same charge and are
reported separately in `report.json`. Summing the two double-counts them; the
earlier per-attempt field `charged_backing_bytes` did sum them and must be read
as a superseded derived field, not as a product number.

`Quota charge` is the driver's `consumer_accounted_bytes` at status time (host
quota charge, not private-backing bytes).

### 3.1 Why individual cells are not PASS

- **#3, #6, #12 control — FAIL (timeout).** The complete command reached the
  registered 25-second limit with the Exec still running; the driver was killed,
  so there is no driver receipt, no counters and no verification for those
  three cells. Their measured complete-command wall is 25.007 s, i.e. the limit,
  which is a started-and-missed bound and therefore FAIL rather than NOT_RUN.
  The control's own `repeated-4097` (#9) did finish, at 14.518 s with a 13.576 s
  Exec, so the control's 4,097-write bound failure is specific to monotone
  append and dispersed frames, not to write count alone.
- **#10 both arms — FAIL (driver outcome contract).** `WorkspaceApi::commit`
  correctly returned `CommitOutcomeWire::UpToDate { head, root }` for a
  zero-edit Commit, and the frozen `benchmark_shell` driver represents only
  `Committed(_)`, so it reported `status=FAIL` with an empty head and the
  oracle was `NOT_RUN`. The measured facts survive: 11.4 ms Exec, 5.5 ms
  (candidate) / 7.6 ms (control) Commit, ~0.9 s complete command, cleanup PASS,
  quota charge 1,837,560 / 1,719,816 B. A qualified clean-Commit row needs a
  driver that can represent `UpToDate`; that is a workload change, so this cell
  is retained as FAIL and is not re-sampled.
- **#11 both arms — INCOMPLETE (declared counter contract).** Every substantive
  check passed: functional PASS, shared oracle PASS, the arm's own verifier
  PASS, cache PASS, cleanup PASS, FUSE write class 1, Exec ≈16 ms, Commit
  18.4 ms (candidate) / 16.3 ms (control) over the retained 4,097-record base.
  The single unsatisfied counter is `writer_progress_ok`: the harness declares
  the writer's four quartile `PROGRESS` checkpoints, and a one-write command can
  emit only the final one. So the row is instrument-incomplete, not a product
  failure.
- **#5 control — INCOMPLETE (interleaved evidence).** The fourth
  `LFS_WRITE_SAMPLE` (class 512) exists in the raw stderr but a concurrent
  `LFT1` daemon telemetry line was interleaved into the middle of that record,
  so the sample could not be parsed and only three of the four declared class
  checkpoints are in the receipt. All three parsed checkpoints are monotone and
  consistent, the oracle PASSed, and cleanup PASSed.
- **#12 candidate — INCOMPLETE (declared C1 counter).** Functional PASS, shared
  oracle PASS, the arm's own verifier PASS, `separated_runs=4097`, cache PASS,
  cleanup PASS, FUSE write class 4,097, complete command 7.789 s. The
  unsatisfied counter is `LFS_C1_EDIT_LOAD`: this row's Commit lowered the whole
  8,194-byte change through one replacement splice with `nodes_read=0`,
  `nodes_created=0`, so no C1 edit-load record was emitted. The row's C1
  counter set is therefore incomplete for this shape.

## 4. Space: the named 3 MiB target

The named target applies only to the one-file, one-generation 4,096-separated
WRITE checkpoint. Two independent measurements of that scope exist:

| Measurement | Before (charged private backing) | After Commit | Pack pages | Commit wall |
| --- | ---: | ---: | ---: | ---: |
| Product route row `active_separated4096` (this checkpoint) | **1,814,528 B (1.73 MiB)** | 24,576 B | 52 | 264,727,375 ns |
| Container gate row `issue248-separated-4097-v1` (candidate arm) | **1,810,432 B (1.73 MiB)** | — | — | 259.2 ms Commit |

`active_separated4096` asserts `physical_private_files(private) ==
backing_status().allocated_bytes` and `before <= 3 MiB`, so its 1,814,528 B is
the recursive `st_blocks * 512` sum of the whole private directory and it is the
scopings authority for the container row's 1,810,432 B.

- **3 MiB target: MET.** 1,814,528 B ≤ 3,145,728 B, with 1,331,200 B of slack.
- **Against the historical v1 checkpoint:** the #271 causal receipt recorded
  18,751,488 B (17.88 MiB) at accepted WRITE 4,096. The active path's
  1,814,528 B is a **10.33× reduction (90.3 % less)**. This is a space
  comparison of two measured checkpoints, not a timing claim, and the two
  measurements have different fixture identities.
- **Not claimed:** the target is met for this one-file, one-generation shape
  only. The other rows have no inherited threshold.

Separate physical evidence for the other named shapes, from the same run:

| Shape | Route row | Physical bytes | Pages |
| --- | --- | ---: | ---: |
| 128 one-byte files | `active_many_file` | 151,552 after Commit (163,840 before) | 2 pack pages before, 0 after |
| 4,097 repeated overwrites | `active_repeated` | 12,288 before and after | 1 pack page before, 0 after |
| 32 retained one-edit generations | `active_retained32` | 405,504 retained, 16,384 released | 32 retained pack pages |

## 5. Correctness and space selections

Each route execution is one registered product row at the frozen product
identity; every one is capped at 60 s. `performance_claim=false`.

| Registered selection | Covering route row(s) executed | Result | Coverage |
| --- | --- | --- | --- |
| `issue273-many-file-128-v1` | `active_many_file` (3 checks) | PASS, 4.545 s | exact: 128 one-byte files, shared pack before Commit, commit byte oracle, exact clean close (physical bytes above) |
| `issue273-multi-exec-v1` | `active_hot_continuity` (2 checks), `active_generation` (2 checks) | PASS, 1.046 s / 0.908 s | **partial**: a mounted process kept its PID, fd and inode across G1/SaveFile/C5/G2 with a same-process handle post-Commit refund, and two committed generations are byte-checked. The registered *three sequential 100-write Execs before one Commit* shape is not executed here; it is recorded as an omission, not renamed. |
| `issue273-g1-g2-v1` | `active_hot_publication` (3 checks), `active_generation` (2 checks) | PASS, 3.678 s / 0.908 s | exact for three consecutive edited generations with an old reader held across successors; counts `ordinary=949 hot_writes=1021 admissions=1 carries=73 seeks=74 visits=29770 index_writes=4114 directory_writes=1023 pack_writes=1024 retirement_inspections=5049 hot_reserved_bytes=160086 physical_bytes=368640` |
| `issue273-retained-32-v1` | `active_retained32` (3 checks) | PASS, 1.576 s | exact: one edit and pin in each of 32 generations, retention and physical release |
| `issue273-mutations-v1` | `active_namespace` (2), `active_quota_refusal` (2), `active_cleanup_failure` (2) | PASS, 1.010 s / 0.988 s / 0.915 s | **partial**: rename/unlink and link/symlink commits, quota refusal with retained acknowledged bytes and charge (`kind: StorageFull`), and a post-publication physical custody failure that retains receipt, bytes and charge. Truncate/hole and open-unlinked handles are covered by the frozen phase-4.5 proof (`active_hot_publication`, `active_hot_continuity`, `active_split_slot`) rather than by a row registered under this ID; recorded as a coverage gap. |

All ten route rows re-used the identity-matched Linux proof artifacts
(`core/target/issue273/checkpoint3-prepared-v1/result.json`,
`core/target/release`, `stage-a8c0e2b3ec9929a4`) whose producing sources are
unchanged since `4ae36ad3a`; the artifacts were hash-checked, not rebuilt.

## 6. Cache contract and its declared limitation

Both arms used the same procedure before every measured command: the cloned
Store and history were mapped `PROT_READ|MAP_SHARED` and invalidated with
`msync(MS_SYNC|MS_INVALIDATE)` (`darwin-shared-mmap-invalidate-mincore-v1`),
then re-checked with `mincore`. Every one of the 24 attempts records 0 resident
pages after invalidation and on re-check, and a launch gap under the 1-second
bound; `cache_status=PASS` on all 24.

The container-side state is declared, not enforced. The Exec of a measured
command writes the private-backing pages that the same command's Commit then
reads, and the container's page cache cannot be invalidated between two product
calls without changing the frozen product or adding a container-side helper.
The Commit and complete-command ratios therefore carry
`commit_cache_status=INELIGIBLE` and no cache-qualified speedup is claimed for
any row. The Exec phase reads host Store pages that *are* invalidated and
verified, and the charged-backing numbers are cache-independent, but both are
still reported as `INELIGIBLE` because a qualified ratio also needs a matched
pair of non-FAIL rows and there is no owner-approved admission profile for this
checkpoint.

## 7. Resource accounting

- Container cgroup anonymous/file-cache/swap: **not measured.** The driver does
  not export cgroup domains; the field is `null` with a reason, never zero.
- Process memory: `LFT1` resource events, `scope=process-shared`,
  `sampled_max_rss` (e.g. 32.7 MB for the driver in the #1 control run). These
  are process-shared samples, not phase peaks, and are not used as a bound.
- Host quota charge and refunds: `consumer_accounted_bytes` is recorded per row.
  Twenty-one of twenty-four rows report `cleanup_status=PASS` (`unmount_ok` and
  `sandbox_delete_ok` both true). The three control FAIL-by-timeout rows could
  not report either acknowledgement because the harness killed the driver at
  the limit, so product cleanup is **unproven** for `control/03`, `control/06`
  and `control/12`; `docker ps -a --filter name=shell243-` shows zero remaining
  containers, so no owned or foreign container was left behind, but that is an
  environment observation and not a product cleanup receipt.
- Physical Store/history clone bytes: recorded per row
  (`clone_physical_bytes`, e.g. 999,424 B).
- Watchdog: every route execution stayed under its 60-second cap; no route row
  was truncated.

## 8. Reproduction

```sh
# 1. shared oracle (once)
python3 core/benchmark/fs-bench-pro/checkpoint5_273.py self-check
python3 core/benchmark/fs-bench-pro/checkpoint5_273.py oracle \
  --repo "$PWD" --output benchmark-results/fs-bench-pro/issue273/checkpoint5/oracle-1
# 2. per-arm preparation (candidate from its worktree, control with --repo pointing at the #271 worktree)
python3 core/benchmark/fs-bench-pro/checkpoint5_273.py prepare --arm candidate \
  --repo /Users/yifanxu/.codex/worktrees/issue273-active-head/layerfs \
  --oracle benchmark-results/fs-bench-pro/issue273/checkpoint5/oracle-1/oracle.json \
  --output benchmark-results/fs-bench-pro/issue273/checkpoint5/prepared-candidate
# 3. retained 4,097-record base (once, reused by both arms)
python3 core/benchmark/fs-bench-pro/checkpoint5_273.py retained \
  --prepared benchmark-results/fs-bench-pro/issue273/checkpoint5/prepared-candidate/prepared.json \
  --output benchmark-results/fs-bench-pro/issue273/checkpoint5/retained-5
# 4. the campaign of record: 12 selections x (control, candidate), row-major
python3 core/benchmark/fs-bench-pro/checkpoint5_273.py campaign \
  --control-prepared .../prepared-control/prepared.json \
  --candidate-prepared .../prepared-candidate/prepared.json \
  --retained .../retained-5/retained.json \
  --output benchmark-results/fs-bench-pro/issue273/checkpoint5/campaign-3
# 5. derived tables
python3 core/benchmark/fs-bench-pro/checkpoint5_273.py report \
  --root .../campaign-3 --route core/target/issue273/checkpoint5-route-1/supervisor-result.json \
  --output benchmark-results/fs-bench-pro/issue273/checkpoint5/report-1
# 6. correctness/space route rows (one per case; 60 s cap)
python3 core/crates/layerfs-workspace/tests/stage_route.py \
  --fixture core/target/issue273/checkpoint3-prepared-v1/result.json \
  --binaries core/target/release \
  --test-binary core/target/aarch64-unknown-linux-musl/release/deps/stage-a8c0e2b3ec9929a4 \
  --case active_separated4096 --output core/target/issue273/checkpoint5-route-1/active_separated4096
```

## 9. Checks run at this identity

- `python3 core/benchmark/fs-bench-pro/checkpoint5_273.py self-check` (schedule
  algebra, registry cardinality, limits, manifest freeze), plus five labelled
  pre-flights before the first registered sample.
- `cargo +1.85.1 clippy --release --manifest-path core/Cargo.toml --locked
  -p layerfs-server --all-targets -- -D warnings` (covers the new oracle
  example).
- `python3 core/tools/check_product_boundary.py` and
  `python3 -m unittest discover -s core/tools -p 'test_*.py'`.
- Focused harness tests under `core/benchmark/fs-bench-pro/tests/`.
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check`.
- The frozen phase-4.5 product proof at `4ae36ad3a` (32 backing tests, 17 public
  functional cases / 42 named checks) is reused unchanged for this
  documentation/harness-only change; the product source and its compilation
  inputs are unchanged, so it is not rerun.
- The ten route rows above were executed at this identity with identity-matched
  Linux artifacts.
- **Independent re-verification of retained evidence** (read-only, no sample
  repeated): all 24 attempt `SHA256SUMS` re-hashed with no mismatch, and the
  shared oracle was re-run against the retained clone Store/history and declared
  manifests for `control/01`, `candidate/01`, `control/09`, `candidate/06` and
  `candidate/12` — all PASS. Recorded in
  `evidence/checkpoint5/INDEPENDENT-REVERIFICATION.json`.

## 10. Remaining failures, gaps and what still blocks closure

1. The control arm misses the 25-second complete-command bound on all three
   4,097-write rows it was asked to run that advance offsets (#3, #6) and on
   the original #248 gate (#12). The original #248 gate therefore **still
   FAILs its 25-second limit** on the frozen control, and its historical FAIL
   history stands. The candidate meets all three bounds.
2. The candidate did not complete a qualified numeric speed comparison: every
   row is `INELIGIBLE`, and the Commit/complete-command phases carry a declared
   container-cache limitation. No 2× claim follows.
3. `issue273-clean-commit-v1` is FAIL on both arms for a driver-outcome
   representation gap, not a product defect.
4. `issue273-one-edit-commit-v1` is INCOMPLETE on both arms for a declared
   writer-progress counter that a one-write command cannot satisfy.
5. `issue273-dispersed-512-10m-v1` control is INCOMPLETE: one phase sample was
   corrupted by interleaved concurrent daemon telemetry.
6. `issue248-separated-4097-v1` candidate is INCOMPLETE for a C1 counter this
   row's lowering shape does not emit.
7. `issue273-multi-exec-v1` and parts of `issue273-mutations-v1` have no row
   registered under those IDs; their coverage comes from the mapped rows above
   and the gaps are named rather than renamed.
8. No container cgroup memory bound, no swap/OOM observation, and no
   wait-free/constant-RAM Commit claim exist. Eight hot cursors, 64 slots and
   1 MiB current hot reservations remain the declared limits; Commit still
   pays charged `O(E_f)` upload scratch.
9. Concurrent SDK Exec, multiple daemon mounts, deadline-free command lifetime
   and command leases belong to #249 and were not implemented or measured.

## 10b. Reviewer-facing slowness report

`CHECKPOINT5-SLOWNESS-REPORT.md` is a derived, reviewer-facing summary of this
log that reports slowness in both directions: the three registered bound misses
on the frozen control, the per-row metric directions, the five cells where the
candidate is slower, a labelled derived diagnostic that separates attributed
from unattributed lifecycle time, the four disqualifiers that prevent any
qualified speed claim, and a claim-mapping table. It adds no sample and no new
metric.

## 11. Production LOC

Every commit in this checkpoint is harness, example or documentation only.

| Commit | Production LOC | Delta |
| --- | --- | --- |
| `1c2b2c1d0` harness + oracle example + specification | 67,158 → 67,158 | 0 |
| checkpoint-5 evidence/log (this commit) | 67,158 → 67,158 | 0 |

Core 67,158, reference 65,417, combined 132,575. Counting method:
`python3 tools/production_loc.py --json`. Scope: first-party product
implementation only; `core/benchmark/` is not product source and the oracle is
an example outside product `src/`.

## 12. Read-only root-cause audit after the slowness review

The owner requested research with subagents. The source-pinned
[root-cause research](CHECKPOINT5-ROOT-CAUSE-RESEARCH.md) and its
[derived arithmetic](evidence/checkpoint5-root-cause/DERIVED.json) add no new
performance sample and preserve all original receipts and row statuses.

They qualify section 6's cold-input claim: the campaign harness hashes each
Store/history file after eviction and zero-residency verification, reading the
input again before launch. Thus the recorded check does not establish cold
Exec inputs. Private backing also requests O_DIRECT in both Linux arms, so
the blanket explanation based on warm Linux file-data pages is too broad;
remaining cache domains and runtime handling still require separate evidence.
No row is promoted to qualified speed evidence.

The audit finds working eligible hot traversal counts alongside costly
file-per-page publication/readback, scattered Commit pack-cache misses and a
quadratic full-patch prefix search in reconcile. It also identifies quick-row
observer/retained-journal coverage gaps and arithmetic corrections to the
derived slowness summary. See the research for exact source/receipt anchors,
measured-region attribution, prediction limits and the next causal diagnostics.
