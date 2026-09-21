# Handoff: where does the Workspace/payload-create path actually differ from v0.1.6

> **Status:** Handoff prompt. Filed from [#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219)
> after the #216 writer work merged (`1f34ac939`). It commissions a gap analysis and
> a root-cause analysis, not an implementation. No performance claim is made by
> this page.

**Owner direction (2026-09-21):** the belief is that v0.1.6 `fs-bench-pro`
payload-create results are "super fast — 100 MB files at 700 MB/s+" *including* the
Workspace FUSE path in Docker and the database commit, i.e. more steps than the
current comparison. Before any optimization is commissioned on that premise, agents
must **find the actual reference rows, verify or refute the 700 MB/s figure, and
attribute whatever gap exists phase by phase**.

## 1. What has already been read, and what it says

### The 700 MB/s figure is a millisecond column, not a rate

`docs/roadmap/0.1/0.1.4/issue91-campaign/terminal-r26/campaign/r26-report-repair/report/report.md`
line 241, table header at line 236:

```text
| Test | Tier / files / bytes | Timer | Time (ms) | Exec / SDK / Commit (ms) | Host / container peak (MiB) | Previous (ms) | Execution / verification | Target |
| payload-create-100m | 100 / 0 / 0 | pure_call_sum_ns | 662.003 | 282.778 / — / 361.895 | 54.61 / 12.02 | 682.771 | PASS / PASS | PASS |
| payload-create-500m | 500 / 0 / 0 | pure_call_sum_ns | 2701.168 | 1215.713 / — / 1468.741 | 55.53 / 12.53 | 3068.250 | PASS / PASS | PASS |
```

- `682.771` is the **Previous (ms)** column: 0.683 s for a 100 MB payload, i.e.
  **~146 MiB/s**, not 700 MB/s+. `3068.250` in the same column for 500 MB is exactly
  the v0.1.3 changelog row *"Payload creation | Create 500 MiB payload | 3.068 s"*
  (`docs/releases/v0.1.3/CHANGELOG.md:119`), which confirms the column's meaning.
- The same table's current column is 662.003 ms for 100 MB (~151 MiB/s) and
  2701.168 ms for 500 MB (~185 MiB/s).
- **Fetch before believing:** these are v0.1.4-era rows. If a v0.1.6-era table
  exists, find it and quote it with the file, line and column header. A figure that
  appears in a *time* column must never be re-reported as a rate.

### Current receipts (candidate arm) on the v0.1.6-era harness

`~/Ephemeral-AI-Lab/layerfs/benchmark-results/issue152/g5*/payload-create-*/perf.jsonl`
(`identities.source_arm = candidate`, `route = workspace`), timer
`pure_call_sum_ns`, header + sample + summary records:

| case | medians across the six located receipts | implied rate |
| --- | ---: | ---: |
| `payload-create-1m-compact-v2` | 0.0269 – 0.0420 s | 24–37 MiB/s |
| `payload-create-10m-compact-v2` | 0.0643 – 0.0903 s | 111–155 MiB/s |
| `payload-create-100m` | 0.4389 – 0.7236 s | 138–228 MiB/s |
| `payload-create-500m` | 2.1442 – 3.3690 s | 148–233 MiB/s |

All 24 located `payload-create*` receipts are `candidate`; **no reference/baseline
arm receipts for this family were found** in
`~/Ephemeral-AI-Lab/layerfs/benchmark-results/`. The `v016` results root
(`benchmark-results/v016/`) does not contain this family. So the v0.1.6 side of the
comparison may not exist as a receipt at all — establishing that, with the search
recorded, is the first deliverable.

### The one number in that row that is real signal

`Commit` is **361.895 ms of the 662.003 ms** payload-create-100m, and 1468.741 ms of
2701.168 ms for 500 MB — i.e. **55% of the create path is the database commit**, on
a route that also runs FUSE in a container. That agrees with this session's
store-path attribution (`docs/roadmap/0.1/0.1.7/evidence/issue216-store-path-attribution-20260921T034000Z/`):
construct 617 MiB/s against save 165 MiB/s, and a dedup control where 0.3 MiB stored
runs the save at 1778 MiB/s.

## 2. Already measured in this session — do not redo, cite instead

| Finding | Evidence |
| --- | --- |
| The writer budget is not a throughput knob: 125.29 / 142.68 / 140.99 / 132.76 MiB/s at budgets 1/2/4/8 (64 x 4 MiB) | `evidence/issue216-recheck-enforced-profile-20260921T031500Z/` |
| Transport 802 MiB/s (1 stream) and 1556 MiB/s (2 streams) once the aarch64 AEAD profile is applied; the route is store-bound, not wire-bound | `evidence/issue216-aead-build-config-20260921T024000Z/` |
| The store half is byte-bound, not CPU-bound: deduplicating content stores 0.3 MiB and runs the save at 1778 MiB/s | `evidence/issue216-store-path-attribution-20260921T034000Z/` |
| Plain-SQLite mimic of the same profile: sequential BLOB inserts 209–273 MiB/s; one append to a growing blob 150 MiB/s; aggressive grow-to-2x 46 MiB/s; coarser commit cadence +10%; 16 KiB pages +30% | same page, `baseline/` |
| The batch-vs-single question (64 x 8 MiB against 1 x 512 MiB) is **not resolvable** at three samples: +9.0%, +9.1%, −6.7% | `evidence/issue216-64x8-vs-1x512-20260921T021000Z/CORRECTION.md` |
| Window-to-window machine variation on the history lane is 16.7–26.0 s for the same case; nothing in preflight predicts a fast window | `evidence/stage-6-history-209-format-20260920T222118Z/README.md` |
| [#209](https://github.com/Ephemeral-AI-Lab/layerfs/issues/209): the multi-writer model commits once per pack append (42x more commits) and cost stride10 2.06x; one treatment recovered 6.65 s; the remaining commit cost is page writes proportional to pack body; the page-cache hypothesis was refuted | `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-*/` |

## 3. The questions this handoff must answer

1. **Does a v0.1.6 (or any reference-arm) payload-create receipt exist?** Find it or
   prove its absence with the search recorded (`find`/`grep` commands and their
   output). If it exists: quote file, line, timer, arm and cache declaration.
2. **Where does the current product lose or win phase by phase** against that
   reference on the same harness, same fixture, same route? Build a gap matrix with
   one row per phase (`preparation`, `exec`, `sdk`, `commit`, `cleanup`) and one
   column per arm, and mark every cell that has no evidence as `NOT_MEASURED`
   rather than inferring it.
3. **If no reference receipt exists, what is the closest defensible comparison?**
   Candidate: rebuild the v0.1.6 reference at its recorded tag, run the *same*
   family and seeds on the *same* harness, and pair the arms. State the seal
   identities of both arms and the reuse used, or declare the comparison
   `INCOMPLETE` and say why.
4. **What actually costs the commit?** The reference row says commit is 55% of the
   create path. Test, one difference per arm: pack-append rewrite (page writes ==
   ceil(pack body / 4096)), commit count, page size, and whether the payload is in
   the Store at all or in a Workspace spool.
5. **Is the FUSE-in-container step a cost or a bottleneck?** Compare the same case
   on the `workspace` route against a direct route with everything else equal, and
   report the difference as a cost with its own cache declaration.

## 4. Subagent structure

Follow the repository's evidence style: one coordinator serializes builds and
measurements; each squad writes its own report with raw receipts beside it; a
read-only reviewer re-derives the arithmetic from the raw files.

| Squad | Owns | Must produce | Stop condition |
| --- | --- | --- | --- |
| **S1 custody** | Question 1 and the search record | A receipt inventory: every located payload-create/perf.jsonl with case, arm, route, timer, median, path; plus the exact commands that establish absence for the rest | Inventory complete for the paths named in §1 and for the whole `benchmark-results` tree |
| **S2 gap matrix** | Question 2 (and 3 if no reference receipt) | The phase matrix with `NOT_MEASURED` cells marked, plus a paired-arm plan if the arms must be rebuilt | Matrix complete, every cell sourced or marked |
| **S3 commit attribution** | Question 4 | One-difference arms on the commit path with the mimic baselines as controls; a written mechanism, not a correlation | Either a mechanism with receipts or a documented refutation |
| **S4 route cost** | Question 5 | `workspace` vs direct route at equal payload, cache declared, difference priced | Difference reported with its uncertainty |
| **S5 review** | Independent re-derivation | Arithmetic re-derived from raw files; every claim labelled PASS/FAIL/INCOMPLETE; non-passing rows kept | Reviewer signs the matrix or records what could not be checked |

## 5. Method rules that bind this work

- One sample per case per arm; fresh `--output`; append-only receipts; no
  best-of, no re-runs to find a passing number, no cache warming. Diagnostics are
  allowed only when labelled as diagnostics and reported beside the sample.
- Pin identities: source commit/seal/tree, product, compilation and dependency
  seals, image, harness and workload hashes. A rebuilt artifact needs a rebuilt
  matched arm. Respect the per-worktree measurement lock; never interrupt another
  owner's run.
- Cache state is declared and enforced equally across arms. Pooled clone/fresh or
  cold/warm rows are invalid; `INELIGIBLE` rows stay in the report.
- Budgets: a performance selection's complete command is <= 15 s, with a declared
  exception list to 25 s; verification <= 60 s. A selection that cannot fit is
  reused from a qualifying receipt with the evidence cited, or recorded `NOT_RUN`.
- **Never retune or relabel historical arms or receipts**, including the v0.1.6
  ones this handoff is about. Historical rows keep their own cache declarations and
  identities.
- Keep the single construction producer per ordinary operation; namespace init
  keeps its exception. No new durability, no WAL/fsync, no third-party patching.
- This repository runs no CI and no aggregate gate. Report exactly which commands
  ran, which did not, and why.

## 6. Definition of done

- The reference-row question is **answered either way**, with the search recorded.
- The gap matrix exists, every cell sourced or `NOT_MEASURED`.
- Any proposed treatment is pre-registered with the identity it will be compared
  against, or the hypothesis is recorded as refuted.
- The receiving issue ([#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219)
  for the Workspace path, [#218](https://github.com/Ephemeral-AI-Lab/layerfs/issues/218)
  for the store path) carries a status comment with the numbers and the gaps, and
  nothing is closed on this handoff alone.
