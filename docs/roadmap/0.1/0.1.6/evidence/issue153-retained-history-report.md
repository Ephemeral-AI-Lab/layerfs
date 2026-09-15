# Retained deepseek-harness history profiles on the v0.1.6 candidate

Campaign record for [#153](https://github.com/Ephemeral-AI-Lab/layerfs/issues/153); posted as [#153 comment 5684860153](https://github.com/Ephemeral-AI-Lab/layerfs/issues/153#issuecomment-5684860153). Ledger entry `L31`.

## Retained deepseek-harness history on the v0.1.6 candidate — 3/3 profiles PASS

Both requested profiles ran against the frozen v0.1.6 candidate, plus `stride-10` as the
shakedown, each with one performance sample and one same-Store historical verification.
All six phases PASS, every state verified against its original oracle, and a paired
reconstructed **v0.1.5 control** was collected for `stride-3`.

**Identity.** source `8308cd8e628a97cd8b7d17d184a8f69ff5f212d21b0646d84913a6df5d444e9a` @
`7fab1027a` (tree clean), product
`970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd`, compilation
`bff3ff080d64671f9bf9ef7e73450afd4dbaaecbce86c91a5c6ae285d5b63743`, dependency
`a1cf72ac4b77536d2d3b44c09872457a246673ca2eacf0997b11b293900709eb`, harness
`daa74be0…`, workload `821b2404…`, image `layerfs-bench-infra:8308cd8e628a97cd`.
Control: reconstructed v0.1.5 at `/Users/yifanxu/layerfs-v016-control`, product
`276c5970aabf485594d90ae30920b3cdb310134a7572589c558063a9d52ce093`, source
`40bb391e1efccde7…` @ `6ee1ec94c`, image `layerfs-bench-infra:40bb391e1efccde7`.

### 1. Per-profile results (candidate, one sample each)

| profile | states | Created | Store allocated / apparent B | Commit sum (median) | exec sum | per-state install | perf wall | verify wall | verified path-states / bytes |
|---|--:|--:|--|--|--|--|--|--|--|
| `stride-10` | 17 | 17/17 | 49,344,512 / 49,315,940 | 11.371 s (564.3 ms) | 59.7 s | 18.6 s | **98.0 s** | **67.5 s** | 101,477 / 561,010,345 |
| `stride-3` | 53 | 53/53 | 64,024,576 / 64,000,100 | 24.815 s (421.0 ms) | 129.8 s | 63.7 s | **241.7 s** | **199.8 s** | 306,861 / 1,676,767,835 |
| `stride-1` (`deepseek-full`, **157** commits) | 157 | 157/157 | 83,947,520 / 82,677,860 | 64.108 s (341.7 ms) | 355.5 s | 132.0 s | **617.6 s** | **570.6 s** | 904,143 / 4,936,693,030 |

Setup/cleanup outside those walls: 1.05–1.35 s setup, 0.47–0.55 s cleanup per profile;
cleanup status PASS everywhere, containers removed. Fixture preparation (one-time,
outside the measured phases): 15–22 s for the first profile, 27–38 s and 83–89 s for the
larger caches; after that the cache is reused. Per-state `transfer_ns` (the fixture tree
install into the container) is **inside** the measured work wall and is reported
separately rather than excluded.

### 2. Paired `stride-3` comparison — candidate vs reconstructed v0.1.5 control

Identical selection (indices 1, 4, …, 157), identical harness, identical fixture cache.

| measurement | candidate `970964e9…` | control `276c5970…` | ratio |
|---|--:|--:|--:|
| canonical content committed | 589,423,458 B / 73,476 objects | 589,423,458 B / 73,476 objects | **1.000** |
| verified path-states / logical bytes | 306,861 / 1,676,767,835 | 306,861 / 1,676,767,835 | **1.000** |
| Store allocated bytes | **64,024,576** | 65,064,960 | **0.984** |
| Store apparent bytes | 64,000,100 | 64,036,964 | 0.999 |
| Commit time sum | 24.815 s | 22.914 s | **1.083** |
| Commit median | 421.0 ms | 396.6 ms | 1.062 |
| exec (workload) sum | 129.8 s | 133.9 s | 0.970 |
| per-state install sum | 63.7 s | 51.4 s | 1.239 |
| performance phase wall | 241.7 s | 235.4 s | **1.027** |
| verification phase wall | 199.8 s | 188.9 s | **1.058** |

Both arms commit byte-identical canonical content and verify the same 306,861
path-states — the same figure the 2026-09-10 public run reported — so this is a paired
comparison of the same work, not two different workloads. On it, v0.1.6 retains
**1.6 % less** Store and spends **8.3 % more** Commit time (median 421.0 ms vs 396.6 ms,
+24.4 ms/commit), which the single construction worker directive predicts for
construction-heavy history Commits; the complete phases are 2.7 % and 5.8 % longer,
inside the bounded-acceptance rule's *absolute* branch (+6.3 s and +10.9 s).

### 3. `stride-1` (full 157-commit history) against recorded rows — **not** paired

The recorded pre-v0.1.6 full157 rows are from the 2026-09-12 storage campaign
(`benchmark-results/host-store/issue118/20260912/{full157-candidate-1,issue107-storage-full157,issue107-coalesce-full157}/deepseek-full/`);
same harness generation, different date and cache state, so these are cited, not re-run.

| measurement | candidate v0.1.6 | recorded rows |
|---|--:|--|
| canonical content | 871,588,115 B / 104,705 objects | 871,588,115 B / 104,705 objects (identical) |
| Store allocated | 83,947,520 B | 83,910,656 / 83,943,424 / 83,959,808 B |
| Store apparent | 82,677,860 B | 82,743,396 / 83,525,732 / 82,583,652 B |
| Commit sum (157) | 64.108 s | 61.991 s (+3.4 %) |
| exec sum (157) | 355.5 s | 408.3 s |
| performance wall | 617.6 s | 684.1 / 684.7 / 691.0 s |

Storage is at parity with the recorded spread (**within 0.02 %** of it either way) on
byte-identical canonical content; Commit time is +3.4 % against the one recorded row that
reports it. The total wall is shorter than those rows, but acquisition is
cache-uncontrolled (`fresh-store-existing-os-cache-uncontrolled`) and the workload sum
moved, so the wall difference is **not** attributed to the product.

### 4. Matched-Git comparators (cited, source-only identity — not re-run)

The Git controls depend only on the selected source trees and the frozen Git policy, so
the recorded values still apply: **Git53 = 49,332,224 B allocated** and
**Git157 = 56,373,248 B allocated**. On that basis the v0.1.6 candidate occupies
**1.298× Git53** (64,024,576 B) and **1.489× Git157** (83,947,520 B). For context only,
the 2026-09-10 #100-era public product measured 100,700,160 B on the 53-state selection
(2.041× Git53) and 134,246,400 B on the 157-state history; those are superseded product
generations and are **not** valid v0.1.6 comparators — the paired control above is
(65,064,960 B on the same 53 states), and the offline structural artifact's 59,760,640 B
was a non-public format experiment, not a public product.

### 5. Resource domains (observed, not gates)

| run | cgroup `memory_peak` (lifetime) | cgroup `file` peak | host RSS peak | host disk write | swap / OOM |
|---|--:|--:|--:|--:|--|
| candidate `stride-10` | 155,881,472 B | 67,575,808 B | 132,628,480 B | 61,726,720 B | 0 / 0 |
| candidate `stride-3` | 159,789,056 B | 63,774,720 B | 142,950,400 B | 130,535,424 B | 0 / 0 |
| candidate `stride-1` | 164,630,528 B | 64,847,872 B | 156,696,576 B | 320,438,272 B | 0 / 0 |
| control `stride-3` | 137,150,464 B | 4,435,968 B | 118,603,776 B | 817,917,952 B | 0 / 0 |

cgroup `memory_peak` is a **container-lifetime** value and decides nothing here (ledger
`L18`). The `file`-cache and host-disk-write differences follow the architecture: the
candidate's payload spool lives in the sandbox (writes land in the container's file
cache), while the v0.1.5 control wrote the same payload on the host. Linux swap and OOM
counters are zero in every run.

### 6. Deviations, stated

- Two pre-measurement harness transients, both before any state ran and with no
  measurement affected: `docker image inspect` on the candidate tag returned
  "No such image" once (a fresh process resolved it immediately afterwards and every run
  since has used the same tag), and one run died in `compilation_seals()` with
  `rustc failed: deadline` (the 10 s seal deadline; `rustc +1.85.1 -vV` measures 0.33 s
  warm). Both were retried; the retries produced the receipts above.
- The candidate and control runs share one fixture cache prepared from the pinned source
  (`03f21acf…` manifest, tip `b0a7d2ce3…`); the original inputs were not modified
  (2.3 GB under `deepseek-history-data`, untouched).
- `--source-arm baseline` labels the control arm; the candidate arm used the default
  `candidate`.
- No `historical_access` run: its sealed v2 Store
  (`f323de0e0f9ae1030efc142402bc033ad427134dd8c69f21eb5b5d6ef7426eb7`) is still absent
  (ledger `L29`), so that family remains `NOT_RUN` independently of this result.

### 7. Evidence and reproduction

Receipts: `benchmark-results/repository-history/{stride-10,stride-3,stride-1}/` (candidate)
and `/Users/yifanxu/layerfs-v016-control/benchmark-results/repository-history/stride-3/`
(control); 634 MB of state receipts, custody manifests and per-state oracles. Ledger entry
`L31`; report document
`docs/roadmap/0.1/0.1.6/evidence/issue153-retained-history-report.md`.

```bash
# candidate (this checkout, image layerfs-bench-infra:8308cd8e628a97cd)
python3 benchmark/fs-bench-pro/shared/runner.py --family repository_history \
  --profile <stride-10|stride-3|stride-1> --source-arm candidate \
  --image layerfs-bench-infra:8308cd8e628a97cd \
  --output benchmark-results/repository-history/<profile>
python3 benchmark/fs-bench-pro/shared/runner.py --family repository_history \
  --profile <profile> --source-arm candidate \
  --image layerfs-bench-infra:8308cd8e628a97cd \
  --storage-verify-run benchmark-results/repository-history/<profile>
```

No release claim: these profiles are exploratory (`admission_eligible: false`), and the
result is a v0.1.6-candidate measurement of the retained-history storage and iteration
cost, nothing more.
