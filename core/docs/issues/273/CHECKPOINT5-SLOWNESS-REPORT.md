# #273 checkpoint 5 — slowness report (frozen baseline vs candidate)

> **Status: reviewer-facing derived summary. Not release evidence, and not a
> qualified speed claim.** Every number below is one raw observation per case
> per arm (`sample_count=1`) taken from the retained append-only receipts of
> `campaign-3`; no cell was re-sampled, and no arm was repeated. Every row is
> `INELIGIBLE` for admission, and the Commit-phase and complete-command numbers
> additionally carry the declared container-cache limitation in §6. The
> registered-metric tables are reproduced by the sealed report generator; §5 is
> a labelled derived diagnostic, clearly marked as such.

This report deliberately reports slowness in **both** directions: where the
frozen control is slow relative to the candidate, and where the candidate is
slow relative to the control.

## 1. What was compared

| Item | Value |
| --- | --- |
| Control (baseline) | worktree `issue271-root-custody`, clean, `48b51e874a41b3e1e6c6661e145316df8b408f07`, product source seal `fc6cd63637a64cfe2db1f7ad…` |
| Candidate | worktree `issue273-active-head`, product unchanged since `a2359620a`, product source seal `1d6194737497ca0cecb5fb4c…` |
| Operation | one public `WorkspaceApi::mount`, one generic `/bin/sh -c` Exec of the ordinary C writer through FUSE, one explicit `WorkspaceApi::commit`, one status, unmount, sandbox delete |
| Workload | 10 MiB `data.bin` of `A` (matrix rows) or the 8,194-byte `#248` fixture (gate row); one mounted `write`/`pwrite` per byte |
| Sample policy | one sample per case per arm, control arm first, row-major order; 24 attempts (12 cases × 2 arms) |
| Identical in both arms | workload seal `fdf35c10…`, harness seal `6b4efffe90…`, Cargo lock `09b880a1…`, `.cargo/config.toml` `3a186383…`, rustc/cargo 1.85.1, `LAYERFS_CONSTRUCTION_WORKERS=1`, one shared oracle binary `837df9a2…` |
| Limiter | complete-command limit per case: 15 s, or 25 s for the three 4,097-write matrix rows and the gate |

`wall` below is the **complete command** (driver launch, Server open, container
create, mount, Exec, Commit, status, unmount, container delete). `Exec` and
`Commit` are the driver's own monotonic inner timers. Ratio is
`control ÷ candidate`, so **a ratio above 1.00 means the candidate is faster**
and **below 1.00 means the candidate is slower**.

## 2. Headline: the baseline misses three registered bounds

| # | Selection | Limit | Control | Candidate |
| ---: | --- | ---: | --- | --- |
| 3 | `issue273-append-4097-10m-v1` | 25 s | **25.007 s — killed at the limit** | 7.033 s |
| 6 | `issue273-dispersed-4097-10m-v1` | 25 s | **25.007 s — killed at the limit** | 13.283 s |
| 12 | `issue248-separated-4097-v1` (original #248 gate) | 25 s | **25.007 s — killed at the limit** | 7.789 s |

These are started-and-missed bounds, therefore `FAIL`, not `NOT_RUN`: the
control's Exec was still running when the harness killed the driver, so those
three control cells have no driver receipt, no counters, no verification and
**no proven product cleanup**. No counter value may be read as zero for them,
and the original #248 gate's historical FAIL history still stands — this
checkpoint does not clear it.

The control's own `issue273-repeated-4097-10m-v1` (same write count, same
fixture, only the offset schedule differs) *did* finish, at 14.518 s with a
13.576 s Exec. So the baseline's 4,097-write failure is specific to monotone
append and dispersed frames, not to the write count alone.

## 3. Registered metrics, both directions

| # | Selection | Exec c/k (ms) | ratio | Commit c/k (ms) | ratio | wall c/k (s) | ratio | charged private backing c/k | ratio |
| ---: | --- | --- | ---: | --- | ---: | --- | ---: | --- | ---: |
| 1 | append-100 | 356.1 / 110.8 | 3.21 | 37.9 / 29.4 | 1.29 | 2.362 / 1.005 | 2.35 | 438,272 / 36,864 B | 11.89 |
| 2 | append-512 | 2,303.6 / 626.8 | 3.68 | 53.0 / 37.7 | 1.41 | 3.552 / 1.540 | 2.31 | 2,269,184 / 151,552 B | 14.97 |
| 3 | append-4097 | — (killed) / 5,964.2 | n/a | — / 158.3 | n/a | 25.007 / 7.033 | n/a | — / 1,150,976 B | n/a |
| 4 | dispersed-100 | 384.7 / 143.3 | 2.68 | 33.2 / 48.2 | **0.69** | 1.329 / 1.105 | 1.20 | 466,944 / 53,248 B | 8.77 |
| 5 | dispersed-512 | 2,651.4 / 1,277.0 | 2.08 | 91.3 / 193.8 | **0.47** | 3.997 / 2.439 | 1.64 | 1,765,376 / 184,320 B | 9.58 |
| 6 | dispersed-4097 | — (killed) / 10,610.2 | n/a | — / 1,803.3 | n/a | 25.007 / 13.283 | n/a | — / 1,421,312 B | n/a |
| 7 | repeated-100 | 361.3 / 152.6 | 2.37 | 29.9 / 29.6 | 1.01 | 1.228 / 1.013 | 1.21 | 28,672 / 12,288 B | 2.33 |
| 8 | repeated-512 | 1,713.2 / 795.7 | 2.15 | 27.1 / 26.4 | 1.02 | 2.626 / 1.726 | 1.52 | 28,672 / 12,288 B | 2.33 |
| 9 | repeated-4097 | 13,575.5 / 5,938.6 | 2.29 | 27.6 / 29.0 | **0.95** | 14.518 / 6.885 | 2.11 | 28,672 / 12,288 B | 2.33 |
| 10 | clean-commit | 11.4 / 11.4 | 1.00 | 7.6 / 5.5 | 1.38 | 0.922 / 0.900 | 1.02 | n/a (no checkpoint) | n/a |
| 11 | one-edit-commit | 16.0 / 15.9 | 1.00 | 16.3 / 18.4 | **0.89** | 0.915 / 0.941 | **0.97** | n/a (no checkpoint) | n/a |
| 12 | #248 gate | — (killed) / 6,562.7 | n/a | — / 259.2 | n/a | 25.007 / 7.789 | n/a | — / 1,810,432 B | n/a |

Summary of direction:

- **Exec — candidate faster on every pair that has both numbers**: 2.08×–3.68×
  on the seven matrix rows with a completed control, and equal to within
  0.6 ms on the two quick-Commit rows, which make no real write workload.
- **Complete command — candidate faster on all nine rows where the control
  produced a receipt** (1.20×–2.35×), plus the three rows where the control
  never finished at all.
- **Charged private backing — candidate smaller on all seven rows that emitted
  a checkpoint** (2.33×–14.97×).
- **Commit — mixed, and the candidate is slower on four rows** (see §4).

## 4. Where the candidate is slower — reported plainly

| # | Selection | Metric | Control | Candidate | Candidate is slower by | ratio |
| ---: | --- | --- | ---: | ---: | ---: | ---: |
| 5 | dispersed-512 | Commit | 91.3 ms | 193.8 ms | +102.5 ms | 2.12× slower |
| 4 | dispersed-100 | Commit | 33.2 ms | 48.2 ms | +15.0 ms | 1.45× slower |
| 11 | one-edit-commit | Commit | 16.3 ms | 18.4 ms | +2.1 ms | 1.13× slower |
| 9 | repeated-4097 | Commit | 27.6 ms | 29.0 ms | +1.4 ms | 1.05× slower |
| 11 | one-edit-commit | Complete command | 0.915 s | 0.941 s | +26 ms | 1.03× slower |

There are no other cells where the candidate is slower. Two observations on
these five cells:

- The two Commit regressions with real magnitude (rows 4 and 5) are on the
  **dispersed** schedule, and in both rows the same command's Exec gained far
  more than the Commit lost (row 5: Exec −1,374 ms, Commit +103 ms; row 4:
  Exec −241 ms, Commit +15 ms). The complete command is still faster on both.
- The row-11 differences (2.1 ms and 26 ms) are at the scale of the
  unattributed lifecycle noise in §5 and are not distinguishable from it.

## 5. Derived diagnostic: attributed work, and the lifecycle asymmetry

**This section is a labelled derived diagnostic computed from the retained
receipts. It is not a registered metric, it took no new sample, and it must not
be quoted as a row result.**

The registered complete-command wall contains work that no driver phase
attributes. Computing `unattributed = wall − (mount + exec + commit + cleanup)`
per attempt:

| Arm | Unattributed, median | Unattributed, total over the 9 paired rows | Worst single value |
| --- | ---: | ---: | ---: |
| Control | 362.4 ms | 4,199.6 ms | 1,374.3 ms (`control/01`) |
| Candidate | 351.2 ms | 3,167.7 ms | 398.9 ms (`candidate/05`) |

The medians are within 11 ms of each other, but `control/01` carries 1,374.3 ms
of unattributed time against a 306–400 ms norm. That single observation
inflates row 1's wall ratio (2.35×). To show the direction does not depend on
it, the same receipts give this comparison of **attributed** time
(`mount + exec + commit + cleanup`):

| # | Selection | Attributed control / candidate (ms) | ratio | Control wall if its row-1 outlier is ignored |
| ---: | --- | --- | ---: | --- |
| 1 | append-100 | 987.2 / 699.3 | 1.41 | 1.29 s vs 1.005 s → 1.28× |
| 2 | append-512 | 3,219.1 / 1,215.0 | 2.65 | — |
| 4 | dispersed-100 | 1,006.5 / 718.8 | 1.40 | — |
| 5 | dispersed-512 | 3,634.2 / 2,040.2 | 1.78 | — |
| 7 | repeated-100 | 902.4 / 680.2 | 1.33 | — |
| 8 | repeated-512 | 2,242.7 / 1,373.4 | 1.63 | — |
| 9 | repeated-4097 | 14,176.8 / 6,550.3 | 2.16 | — |
| 10 | clean-commit | 538.9 / 549.0 | 0.98 | — |
| 11 | one-edit-commit | 540.0 / 561.0 | 0.96 | — |

So the candidate remains faster on the seven write-bearing pairs even after
removing all unattributed lifecycle time, and the two quick-Commit rows are
slightly slower in both readings (0.96–0.98×, i.e. 10–26 ms).

## 6. Why none of this is a qualified speedup — four disqualifiers

1. **Every row is `INELIGIBLE`.** Ten pairs are `INELIGIBLE`, three control
   rows are `FAIL` (bound), one control row is `INCOMPLETE`, both row-10 cells
   are `FAIL` and both row-11 cells are `INCOMPLETE`. `admission_eligible=false`
   on all 24 receipts.
2. **The Commit phase and the complete command carry a declared cache
   limitation.** The private backing that Commit reads is written by the *same
   command's* Exec, and the container's page cache cannot be invalidated
   between the two product calls without changing the frozen product or adding
   a container-side helper. The host-side Store/history clones *are*
   invalidated and residency-verified (`darwin-shared-mmap-invalidate-mincore-v1`,
   0 resident pages after invalidation and on re-check on all 24 attempts, with
   a sub-second launch gap), so the Exec phase is the only phase whose inputs
   are demonstrably cold. That is why the four Commit cells in §4 are reported
   as direction only.
3. **One sample per cell.** No median, range, percentile or repeatability
   statement is implied anywhere in this report, and the protocol forbids
   re-running an unchanged arm for a better number.
4. **Three control cells have no usable phase data** because they were killed
   at the limit, and their product cleanup is unproven. Their 25.007 s is a
   limit miss, not a measured Exec time.

## 7. Evidence, hashes and reproduction

| Item | Location |
| --- | --- |
| Campaign of record (24 raw receipts, append-only) | `benchmark-results/fs-bench-pro/issue273/checkpoint5/campaign-3/` |
| Derived tables and ratios (sealed report generator) | `benchmark-results/fs-bench-pro/issue273/checkpoint5/report-1/report.json`, `REPORT.md` |
| Published summary and receipt hash index | [evidence/checkpoint5/RESULTS.json](evidence/checkpoint5/RESULTS.json), [CAMPAIGN-RECEIPTS.sha256](evidence/checkpoint5/CAMPAIGN-RECEIPTS.sha256) |
| Independent re-verification (read-only) | [evidence/checkpoint5/INDEPENDENT-REVERIFICATION.json](evidence/checkpoint5/INDEPENDENT-REVERIFICATION.json) |
| Full execution record, causes, limitations | [CHECKPOINT5-LOG.md](CHECKPOINT5-LOG.md) |
| Execution specification and registry | [issue273-checkpoint5-execution-spec.md](../../../docs/roadmap/0.1/0.1.7/issue273-checkpoint5-execution-spec.md) |

```sh
python3 core/benchmark/fs-bench-pro/checkpoint5_273.py report \
  --root benchmark-results/fs-bench-pro/issue273/checkpoint5/campaign-3 \
  --route core/target/issue273/checkpoint5-route-1/supervisor-result.json \
  --output benchmark-results/fs-bench-pro/issue273/checkpoint5/report-1
```

## 8. Claim mapping (what a reviewer may and may not quote)

| claim_id | Claim | Kind | Status |
| --- | --- | --- | --- |
| C5-1 | The frozen control misses the registered 25 s complete-command bound on `issue273-append-4097-10m-v1`, `issue273-dispersed-4097-10m-v1` and the original #248 gate `issue248-separated-4097-v1`, while the candidate completes them in 7.033 s, 13.283 s and 7.789 s. | empirical (bound, not a speed ratio) | Valid no-go for the baseline; each cell one observation |
| C5-2 | Per-row raw directions in §3 and §4. | empirical, diagnostic | `INELIGIBLE`; no qualified ratio, no 2× claim |
| C5-3 | The named one-file, one-generation 4,096-separated-WRITE space budget of ≤ 3 MiB is met at 1,814,528 B (route row `active_separated4096`), against 18,751,488 B (17.88 MiB) in the historical #271 checkpoint. | empirical (space) | Space gate MET; the two checkpoints have different fixture identities and are not pooled |
| C5-4 | The baseline is generally slower; the candidate is uniformly faster on Exec and on every complete command the control finished. | **not claimable as stated** | Every supporting row is `INELIGIBLE`; the sentence may be quoted only with the §6 disqualifiers attached |

**Do not quote**: any median or range (n=1 nowhere supports one), any pooled
historical #271/`issue261`/`issue265` wall time as a denominator, any "2× faster"
headline, any Commit-phase regression or improvement as a product result, and
any counter value for `control/03`, `control/06` or `control/12` as measured.
