# S1 — reproduce the bar's first part on a clean, sealed build (#219)

> **Status:** S1 deliverable. **No product change.** The bar's first part is
> **NOT MET** on this machine today; the measured number and its cache state are
> recorded below rather than worked around. Two verified rows were produced, one
> sample per arm, fresh `--output` each.

Worktree: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000`, branch
`codex/219-ns10000`, `origin/main` `b0260df3a2ffc371773cd062feafd4b5e435bf1e`,
`LAYERFS_SOURCE_DIRTY=false`.

## 1. The result

| | bar's first part | pseudorandom row | `-text-v1` row |
| --- | ---: | ---: | ---: |
| `layerstack_init_ns` median | **≤ 578.245 ms** | **944.881 ms** | **708.991 ms** |
| rate over the case's 400 MB (`300 MB` + `100 MB` anchor) | ≥ 691.8 MB/s | 423.3 MB/s | 564.2 MB/s |
| rate as the product reports it (`logical_bytes` 300 MB) | — | 317.5 MB/s | 423.1 MB/s |
| verdict | — | **MISS** | **MISS** |

Both rows are `PASS` for execution and `PASS` for cleanup; both miss the bar. The
`-text-v1` variant is 235.890 ms faster (25.0 %) than the pseudorandom case at
identical file counts and sizes, which is content-dependent behaviour, not noise —
see §5.

## 2. Identities (pinned; both rows identical)

| field | value |
| --- | --- |
| source commit | `b0260df3a2ffc371773cd062feafd4b5e435bf1e` |
| source tree | `8ab2dc2667d0207d2ddf36ec41ff27723f8351b2` |
| source dirty | `false` |
| source seal | `51dcd0721316619d001353e827157c7ddd1db8c0c964eedf6e4ce3cfc92ee96a` |
| product seal | `b3e3cb745340675fdc9cda843b2506e5bcafa912cde32d17f32e098ee886402e` |
| compilation seal | `9be9bc959b514ac8a0c5f8ec26156099a3f1e57420ede10c0f3a5cd2ed3c47b7` |
| dependency seal | `99a233bbac3f063432fe5cc46dec186fcca6b96c07bb57660046325fa184974c` |
| binary sha256 | `cb4179c207296202bbd1668a61d1498bb5e8036f7a8edcd0e38a0262164b03f8` |
| image | `sha256:d152ced8d21cd4ea4b6ca0e73f90aea274ba515bb44dac641a84b1c465d54296` (`layerfs-bench-infra:51dcd0721316619d`) |
| harness identity | `8a6d76dd2f2e639fb356ac74551a4aae2634df1eaf2b3603ebd458cafd90dff5` |
| workload source sha256 | `e29d728e3b295c06a4318bbe3eec43b1a7954d489ffd0bb2f22ceccd2a3ba145` |
| route / timer / topology | `namespace` / `layerstack_init_ns` / `host-store`, 2 container CPUs, 2048 MiB, no swap, 256 PIDs |
| seed | `1` |

Build: `python3 benchmark/fs-bench-pro/shared/runner.py --build-host` (release,
incremental, `CARGO_BUILD_JOBS=8`, 44.49 s wall, 68 packages recompiled — recorded
in the receipt's `host_executor.native_build_wall_ns = 44560347333` and
`recompiled_packages`). Image built from the same tree.

## 3. Cache state — declared, and it is *not* a cold row

**`cache_contract` is `null` on both rows, and that is the harness's answer, not an
omission by this run.** `shared/cold.py:22-25` gates the only cold/residency contract
in the tree on `case == "namespace-100000"`; `namespace-10000` is outside it
(confirmed in S0 §2b). The declaration this case *does* carry is the record-level
`fixture_cache_profile`:

```text
fixture_cache_profile = "reused-first-sample-uncontrolled"
```

— one of the four values the product permits, all of which end in `-uncontrolled`
(`benchmark/fs-bench-pro/src/main.rs:1611-1615`). Reading it plainly: **the fixture
was reused from the prepared host input and the OS page cache was not controlled.**

The receipts' own counters confirm the consequence. Each row scanned 300 MB and
read almost none of it from storage:

| row | `scanned_bytes` | `initialization_disk_read_bytes` | read per byte scanned |
| --- | ---: | ---: | ---: |
| pseudorandom | 300,000,000 | 729,088 (0.73 MB) | 0.0024 |
| `-text-v1` | 300,000,000 | 8,192 (0.01 MB) | 0.00003 |

These rows are therefore **ordinary-OS-cache, cache-credited rows**. Per
`AGENTS.md` §1 they are **not** admissible as `PASS` for a cold claim, and they are
not reported as one. They are comparable to each other and to the historical rows
that show the same signature (S0 §1b: 402.721 / 407.598 / 578.245 / 928.022 ms all
read 0.0 MB), and they are **not** comparable to the historical cold rows
(1020.422 ms / 337.4 MB, 1100.711 ms / 322.2 MB). Cold and warm rows are not pooled
anywhere in this report.

### 3a. Why the bar's 578.245 ms is not reachable by reproducing it

S0 established that the 578.245 ms row is (a) `SOURCE_DIRTY=true`, so not
reproducible from a clean checkout, and (b) cache-credited, reading 0.0 MB while
scanning 300 MB. This run is a clean sealed build with the *same* cache signature
and lands at 944.881 ms — **1.63× the bar** — and it is 3.3× faster than the bar on
the CPU axis while still losing on wall time:

| | pseudorandom (this run) | the 578.245 ms row |
| --- | ---: | ---: |
| `layerstack_init_ns` | 944.881 ms | 578.245 ms |
| user CPU + system CPU | 1036.7 + 752.0 = **1788.7 ms** | 670.7 + 1020.1 = **1690.8 ms** |
| disk read | 0.73 MB | 0.0 MB |
| candidate objects | 25,158 | 54,463 |
| source dirty | **false** | **true** |

The CPU totals are within 5.8 % of each other, so the 366 ms wall-time difference is
not CPU work. The two rows also disagree on `candidate_objects` (25,158 vs 54,463 —
a 2.16× difference in stored objects for the same 300 MB fixture), which means the
578.245 ms row was produced by a build whose content path differed from this one.
That is consistent with its dirty tree and with S0 §3 (its commit `56fc884e` differs
from `HEAD` in **74** production files under `crates/`). **The bar's figure is a
different product measured on a dirty tree with the cache uncontrolled; it is not a
target this build can be expected to hit, and no attempt was made to reach it by
re-running, warming, or retuning.**

## 4. Verification — passing, identity-matched

Both rows were verified **separately**, in verification mode, with the exact
identities from their performance receipt:

```sh
bash benchmark/fs-bench-pro/families/init_namespace/verify.sh \
  --case namespace-10000 --seed 1 --setup fresh --image "$LAYERFS_BENCH_IMAGE" \
  --source 51dcd0721316619d001353e827157c7ddd1db8c0c964eedf6e4ce3cfc92ee96a \
  --input  <the performance row's input_identity> \
  --verification --output <fresh path>
```

| | pseudorandom | `-text-v1` |
| --- | --- | --- |
| `status` | **pass** | **pass** |
| `cleanup_status` | pass | pass |
| `fresh_reopen_status` | pass | pass |
| `root_status` | `pass-reopen-equality` | `pass-reopen-equality` |
| scanned | 10,000 files / 300,000,000 B | 10,000 files / 300,000,000 B |
| sampled files / verified bytes | 10 / 161,912 | 10 / 161,912 |
| `verification_ns` | 77,619,792 (77.6 ms) | 67,837,542 (67.8 ms) |
| `command_wall_ns` | 1,444,908,834 (1.445 s) | 1,070,404,875 (1.070 s) |
| hard limit | 59.0 s | 59.0 s |
| source / product / image identity | match the performance row | match the performance row |
| `omissions` | `["no exhaustive Phase 1 replay"]` | same |

Both verifications declare `full_namespace_verified: false` and
`full_file_bytes_verified: false` — the profile is
`import-counts-reopen-and-sampled-fuse-v1`, which verifies all 10,000 import counts
and 300 MB scanned, a fresh reopen with root equality, and 10 sampled files. It is
**not** an exhaustive byte verification of all 10,000 files, and it is not reported
as one.

## 5. The two rows are not the same workload

`-text-v1` is not a re-run of the same case: `--no-pseudorandom` selects a distinct
registered case (`namespace-10000-text-v1`, `input_identity 5275c693…`) with
identical file counts and sizes but structured-text content instead of pseudorandom
bytes. Its `candidate_objects` is 22,242 against 25,158, and its system CPU is
381.6 ms against 752.0 ms. The two rows are **not pooled** and neither is a
best-of; both are reported.

## 6. Budgets

| | complete command (product timer + container lifecycle + cleanup) | budget | verdict |
| --- | ---: | ---: | --- |
| pseudorandom performance | 6.368 s wall (`/usr/bin/time`), `command_wall_ns` 1.457 s | ≤ 15 s | **fits** |
| `-text-v1` performance | 5.795 s wall, `command_wall_ns` 1.024 s | ≤ 15 s | **fits** |
| pseudorandom verification | 2.428 s wall, `command_wall_ns` 1.445 s | ≤ 60 s | **fits** |
| `-text-v1` verification | 2.188 s wall, `command_wall_ns` 1.070 s | ≤ 60 s | **fits** |

No exception was needed and no timeout was enlarged. Fixture preparation ran once
(`fixture_generate_ns` 2.73 s, `fixture_cache_profile: generated-warm-uncontrolled`
in the preparation record) and was then reused by identity
(`preparation.cache_hit: true`, `cache_key 10e09796…`).

## 7. Exact commands run

```sh
# build + image
python3 benchmark/fs-bench-pro/shared/runner.py --build-host
LAYERFS_BENCH_IMAGE="$(python3 benchmark/fs-bench-pro/shared/runner.py --build-image)"

# performance, one sample per arm, fresh --output each
bash benchmark/fs-bench-pro/families/init_namespace/perf.sh \
  --case namespace-10000 --seed 1 --setup fresh --image "$LAYERFS_BENCH_IMAGE" \
  --perf-fast --collection-mode --product-timeout 120 --timeout 130 --setup-timeout 900 \
  --output benchmark-results/issue219/s1-ns10000-candidate-20260921T031259Z

bash benchmark/fs-bench-pro/families/init_namespace/perf.sh \
  --case namespace-10000 --no-pseudorandom --seed 1 --setup fresh \
  --image "$LAYERFS_BENCH_IMAGE" --perf-fast --collection-mode \
  --product-timeout 120 --timeout 130 --setup-timeout 900 \
  --output benchmark-results/issue219/s1-ns10000-text-v1-candidate-20260921T031259Z

# verification, separately, exact identities (see §4)
```

Raw receipts (append-only, on disk, not overwritten):

- `benchmark-results/issue219/s1-ns10000-candidate-20260921T031259Z/perf.jsonl`
- `benchmark-results/issue219/s1-ns10000-text-v1-candidate-20260921T031259Z/perf.jsonl`
- `benchmark-results/issue219/s1-ns10000-verify-20260921T031259Z/verification.json`
- `benchmark-results/issue219/s1-ns10000-text-v1-verify-20260921T031259Z/verification.json`

## 8. Gate

> *Gate:* a verified, cache-declared candidate row at or under **578.245 ms**, or a
> documented miss with the measured number and its cache state.

**Documented miss.** Two verified, cache-declared rows: **944.881 ms** (pseudorandom)
and **708.991 ms** (`-text-v1`), both declared
`reused-first-sample-uncontrolled` with `cache_contract: null`, both reading <1 MB
from storage while scanning 300 MB, both carrying a passing identity-matched
verification, on a clean sealed build. Neither is at or under 578.245 ms, and §3a
records why that figure is not a reachable target for this build. No re-run was
made to seek a passing number.

## 9. Addendum — the tree this row was measured on carries the writer-budget refactor

Found while specifying a v0.1.7 parity row
([`core/docs/benchmark/fs-bench-pro-storage-content/namespace-10000-parity-spec.md`](../../../../../../core/docs/benchmark/fs-bench-pro-storage-content/namespace-10000-parity-spec.md)).
It does not change the S1 numbers — it dates them.

`b0260df3a` (this row's source commit) **contains**
`7075f338db36b209b59031e3e55ae11cf87eed57`, *"feat(core): replace the fixed two-save
model with a configured per-Store writer budget (#216)"* (2026-09-21 08:28:48 +0800),
which is **not** an ancestor of the tree the last full 217-row lane ran on
(`66bce8378`). `git diff --stat 66bce8378..HEAD -- core/crates/layerfs-storage/src/`
is **31 files, +2,451 / −623**.

That refactor changes what a save's commit count *means*: `cas/lifecycle.rs:166-175`
records "every step commits before that lock is released" and increments
`self.counters.commits` per step, where the retired model acknowledged once. It is why
`pipeline-filesystem-build` now reports `pipeline.commits 43` against a golden pin of
`1` and FAILs, having PASSed in `run-20260920T-fix1` … `run-20260920T-fix5`.

**Relevance to S1, stated plainly:** the 944.881 ms / 708.991 ms rows were produced by
the reference product (`crates/`) at a commit that also carries this `core/` change.
The two are separate workspaces — `cargo --manifest-path core/Cargo.toml` does not build
`benchmark/fs-bench-pro` — so **the S1 timings are unaffected**. What is affected is any
future claim that the reference product's behaviour is unchanged since the closure run:
its `core/` sibling has moved, and the 217 lane has not been re-run to say by how much.

### Instrument note: `instruments_selfcheck`'s heap-window test is a broken test, not a broken instrument

`tests/instruments_selfcheck.rs::the_heap_window_attributes_a_known_allocation_pattern`
fails on the current tree:

```text
thread 'the_heap_window_attributes_a_known_allocation_pattern' panicked at tests/instruments_selfcheck.rs:103:5:
a live 4194304-byte buffer produced a peak of 0 bytes
```

**Pre-existing and unrelated to #219.** It fails identically with this session's changes
stashed. It is also not a harness self-check failure: `runner.py self-check` reports
`PASS`, because that verb runs the Python self-checks, the registry self-check, the
golden comparison and lock parity — it does not run this Rust integration test.

**Cause.** `#[global_allocator]` is declared in the **library**
(`src/support/instruments.rs:89`). An integration test under `tests/` is a **separate
crate** that links the library; a `#[global_allocator]` declared inside a dependency does
not become the test binary's allocator. So the counters `heap_begin`/`heap_end` read are
never incremented in that binary and the peak is exactly zero. The test's own
re-exec-into-a-clean-process design is sound; the allocator simply is not installed in
the process it re-executes.

**Why it does not affect measurement.** The instrument is used where it is installed: in
the harness **binary**, whose crate root includes `src/support/instruments.rs` as its own
module. Receipts confirm it measures — the pipeline row in this session reports
`heap.peak_incremental_bytes: 18,988,411` and the C2 rows report their own non-zero
values. So `g4.heap-window` gates on a working instrument; only this self-test is
inert.

**Not fixed here, deliberately.** Making the test real means giving the test crate its
own counting `GlobalAlloc` (a `#[global_allocator]` in `tests/instruments_selfcheck.rs`),
which duplicates the allocator that must stay in the library for the binary's sake. That
is a harness change outside #219's scope, and doing it silently while implementing a
parity row would be exactly the kind of unrecorded edit this repository's rules forbid.
It is recorded here instead, for a separate ruling.
