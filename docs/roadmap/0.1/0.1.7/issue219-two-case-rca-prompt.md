# Handoff: two-case RCA against v0.1.6 on an independent worktree (#219)

> **Status:** Handoff prompt. **Exploration, pairing and gap-finding only.** No
> optimization is authorized by this page, and no performance claim is made by it.

**Owner direction (2026-09-21):** two cases are the optimization targets, each to be
**matched against v0.1.6** rather than argued from memory. Work in an **independent
worktree**, explore and experiment first, and report the gaps. The namespace-init
multi-worker count is recorded but is **not** treated as the explanation for our
slowness - it is the cheapest thing to refute by measurement, and the RCA continues
past it.

## 1. The two cases

| | case | family | route | timer | size | path to the Store |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | `namespace-10000` | `init_namespace` | `namespace` | `layerstack_init_ns` | 10,000 files / 300 MB + **100 MB anchor** = **400 MB** | direct, host-side |
| 2 | `payload-create-100m` | `payload_create_read` | `workspace` | `pure_call_sum_ns` | **100 MB** payload | container FUSE + daemon -> host service |

**Case naming, because it is easy to misread.** In `init_namespace` the number in the
case id is a **file count, not megabytes**: the eight registered cases are
`namespace-{100,1000,10000,100000}` in a `-compact-v3` and a `-text-v1` variant, with
100 files / 5 MB (+1 MB anchor), 1,000 / 20 MB (+5 MB), 10,000 / 300 MB (+**100 MB**)
and 100,000 / 500 MB (+**100 MB**). There is **no `namespace-100mb` case**; "100 MB"
is the **anchor file** (`NAMESPACE_ANCHOR_BYTES = 100_000_000`), which only the two
largest tiers carry. The owner's "100 MB namespace case" is therefore
`namespace-10000` - the 10,000-file tier with the 100 MB anchor - and
`namespace-100000` is its larger sibling with the same anchor.

Case 1 writes the namespace into the host-owned Store directly: the runner stages it
under `payload/`, and `namespace` / `store-footprint` are the routes that take no
`payload/input` staging and no daemon hop (`shared/runner.py`). Case 2 carries its
payload from the container through the daemon into the host service
(`LAYERFS_EXEC_TRANSPORT=daemon`, `LAYERFS_FUSE_TRANSPORT=daemon`), and its receipt
scope line reads *"Linux daemon/FUSE container command window; host
coordinator/Store process CPU/RSS/IO reported separately in records; host CPU is not
container-capped"*. Both end in one Store commit. Case sizes come from
`benchmark/fs-bench-pro/families/init_namespace/mod.rs::NAMESPACE_SCENARIOS` and the
family registry.

Both cases are also **not size-matched** (6 MB against 100 MB) and are **not
mix-matched** (100 small files plus a 1 MB anchor against one 100 MB blob). Any
comparison must say which of the two it is making.

## 1c. Acceptance bar - proposed, owner confirmation pending

The repository declares **no rate target** for either case. Its declared targets are
times, and both cases already sit far inside them: `PRODUCT_TARGET_NS = 15 s`
(`shared/runner.py:39`, reporting-only, `PASS` iff `elapsed <= 15 s`), the
`payload_create_read` ordinary development target of 1-5 s
(`docs/roadmap/0.1/0.1.3/payload-create-read.md:96`), the strict tier-100
`pure_call_sum_ns < 1 s` assessment (#47), and the waived 2.7 s cold Init target that
applies to `namespace-100000`, not to `namespace-10000`. So "gap" is currently
undefined and must be fixed by the owner before the squads measure. This is the
proposed bar; do not treat it as approved until the issue says so.

1. **Reproducibility first, for `namespace-10000`.** The candidate must reproduce
   **<= 578.245 ms (>= 691.8 MB/s)** on a run with a **declared cache contract** and a
   **passing verification**. Today that row has `cache_contract: null` and
   `verification_status: NOT_RUN`, the six candidate medians span 402.721-1100.711 ms
   (median 753.13 ms, ~531 MB/s), and two rows are `INCOMPLETE` - so the 691.8 MB/s
   figure is a single unverified, cache-undeclared row, not reproducible evidence.
   This step is therefore a *reproducibility and verification* bar, not a speedup.
2. **Then the pair.** The same case on the v0.1.6 reference arm - same seed, setup
   (`fresh-output`), topology, harness and image identity, and the same declared cache
   state - with the candidate median **<= the reference median**, both reported with
   their spread. `namespace-10000` has no baseline arm today (8 candidate / 0
   baseline), so producing it is part of meeting the bar, and `namespace-100000`
   (15 baseline rows, same 100 MB anchor) is the substitute if pairing from existing
   receipts is preferred.
3. **No best-of.** The bar is met by one declared sample per arm at the declared cache
   state, never by selecting the fastest of several runs. Every non-passing and
   `INCOMPLETE` row stays in the report.

For `payload-create-100m` the same shape applies with its own numbers: the candidate's
24 rows span 0.4389-0.7236 s (138-228 MiB/s) with no baseline arm, and the r26-era
`Previous (ms)` cell of 682.771 ms (~146 MiB/s) is the only reference-era figure, so
its bar is (2) plus a verification and cache declaration - not a number borrowed from
a different era.

## 2. What the harness already supports, and the first thing to establish

- `shared/runner.py --source-arm {baseline,candidate}` (default `candidate`) passes
  `LAYERFS_BENCH_SOURCE_ARM` into the workload. Baseline arms exist unevenly, and
  **neither focus case has one**: per `init_namespace` case the receipt tally is
  `namespace-100-compact-v3` 11 candidate / 3 baseline, `namespace-1000-compact-v3`
  6 / 0, **`namespace-10000` 8 / 0**, `namespace-100000` 32 / 15; and
  `payload_create_read` has **no baseline rows at all** (across the whole tree:
  1,614 `candidate`, 75 `baseline`, covering only `init_namespace`,
  `dedup_branch_history`, `tiny_file_churn`, `edit_length_preserving`). So **both
  focus cases must produce their reference arm.** If a case with an existing pair is
  preferred, `namespace-100000` carries the same 100 MB anchor and has 15 baseline
  rows - state the substitution explicitly if it is made.
- **Establish what `baseline` actually selects** before quoting any pair: read
  `benchmark/fs-bench-pro/workload/` and the image build, and compare the baseline and
  candidate receipts' `product_identity` and `image` (they differ - the example
  baseline row is under `host-store/issue118/20260912/infra-pair-1-baseline/`). Say
  whether it is the v0.1.6 tag's code path, an earlier candidate, or a configuration
  variant; if it cannot be established, say so and treat the pair as `INCOMPLETE`.
- Timers by route: `workspace` -> `pure_call_sum_ns`, `namespace` ->
  `layerstack_init_ns`, `sdk` -> `edit_commit_ns`. Other knobs: `--family`, `--case`,
  `--seed`, `--cpus`, `--memory-mib`, `--topology`. Build and reuse mechanics:
  `benchmark/fs-bench-pro/QUICKSTART.md`.
- **Every located receipt for both families carries `verification_status: NOT_RUN`
  and `cache_contract: null`.** No row supports a cache-sensitive claim on its own,
  and the create/init numbers are cache-sensitive.

## 3. Already measured - cite these, do not redo them

| Fact | Evidence |
| --- | --- |
| Transport 802 MiB/s (1 stream), 1556 MiB/s (2), once the aarch64 AEAD profile is applied | `evidence/issue216-aead-build-config-20260921T024000Z/` |
| Construct 617 MiB/s against save 165 MiB/s; the route is store-bound | `evidence/issue216-store-path-attribution-20260921T034000Z/` |
| Deduplicating content stores 0.3 MiB and runs the save at 1778 MiB/s (byte-bound, not CPU-bound) | same page |
| Plain-SQLite mimic: sequential BLOB inserts 209-273 MiB/s; one append to a growing blob 150 MiB/s; aggressive grow-to-2x 46 MiB/s; 16 KiB pages +30%; coarser commits +10% | same page, `baseline/` |
| Writer budget is not a throughput knob: 125.29 / 142.68 / 140.99 / 132.76 MiB/s at budgets 1/2/4/8 | `evidence/issue216-recheck-enforced-profile-20260921T031500Z/` |
| Window-to-window machine variation is large: 16.7-26.0 s for one history case, and nothing in preflight predicts a fast window | `evidence/stage-6-history-209-format-20260920T222118Z/README.md` |
| [#209](https://github.com/Ephemeral-AI-Lab/layerfs/issues/209): commit once per pack append (42x more commits) cost stride10 2.06x; the remaining commit cost is page writes proportional to pack body; page-cache hypothesis refuted | `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-*/` |
| `init_namespace` receipt medians, converted with the case sizes: `namespace-10000` (400 MB) at 402.721 / 407.598 / **578.245** / 928.022 / 1020.422 / 1100.711 ms = **363-993 MB/s** (578.245 ms = 691.8 MB/s), candidate only; `namespace-100-compact-v3` (6 MB) at 9.35-31.67 ms candidate against 19.32-23.85 ms baseline | [#219 comment](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219) |

## 4. Questions this handoff must answer

1. **Pair both cases against the section 1c bar.** Does a gap exist at all, per case,
   on matched arms with declared cache state? Report both arms' identities, the harness/image identity and
   the exact commands. If case 2's baseline arm cannot be produced, say why and mark
   the row `INCOMPLETE` instead of substituting a different era.
2. **Attribute each case by phase.** Per case: preparation / exec / sdk / commit /
   cleanup, plus container CPU versus host CPU/RSS/IO. Which phase dominates each
   case, and where do the two cases differ? No cell may be inferred.
3. **Refute or confirm the worker-count direction.** One-difference arms on namespace
   init forcing 1 versus the multi-worker default (4 = `SMALL_CONTENT_WORKERS`,
   `crates/layerfs-layerstack-store/src/objects.rs`; 8 =
   `construction_worker_limit()`, `crates/layerfs-workspace/src/changes.rs`). Report
   the **measured share** of any gap the worker count can explain. The owner's
   direction is that it is not the explanation; if the measurement agrees, the RCA
   must name what is.
4. **Separate cardinality from bandwidth.** Case 1 is 10,000 small files plus a
   **100 MB anchor**; case 2 is one 100 MB blob. The anchor is what makes case 1
   bandwidth-bearing at all, so report it separately from the small-file count. Cross the axes: the same bytes as 1 file versus
   many files, and the same file count with different bytes. Price per-file overhead
   separately from per-byte cost.
5. **Price the hop.** On case 2, compare the container -> daemon -> host-service path
   against a direct host-side equivalent at equal bytes, declaring the topology
   change and keeping the cache contract identical.
6. **Price the commit.** Both cases end in one Store commit. Measure its share per
   case (an older r26-era row suggests ~55% on payload-create) and test the
   pack-append / page mechanism from the store-path attribution rather than assuming
   it transfers.

## 5. Worktree and subagent setup

```sh
git worktree add ../layerfs-219-rca -b codex/219-rca origin/main
```

- One **coordinator** owns the worktree, serializes builds and measurements, and
  holds that worktree's measurement lock
  (`core/benchmark/fs-bench-pro-storage-content/.measurement.lock` is per worktree;
  builds must not share a Cargo target directory across worktrees).
- Suggested squads, each with its own report and raw receipts: **S1** pair and harness
  custody (Q1 plus what `baseline` selects); **S2** phase attribution (Q2); **S3**
  worker-count refutation (Q3); **S4** cardinality/bandwidth split (Q4); **S5** hop
  cost (Q5); **S6** commit attribution (Q6); **S7** read-only reviewer that
  re-derives every arithmetic claim from the raw files and labels each row
  PASS / FAIL / INCOMPLETE.
- Never interrupt another owner's run. A build overlapping a timed phase is recorded
  as declared interference on the row, not hidden.

## 6. Measurement rules that bind this work

One sample per case per arm, fresh `--output`, append-only receipts, no best-of and no
re-running until a number passes; cache state declared and enforced equally across
arms, with cold and warm rows never pooled; identities pinned (source/seal/tree,
product, compilation, dependency, image, harness, workload) so a rebuilt artifact
needs a rebuilt matched arm; complete-command budgets respected (<= 15 s, declared
exception list to 25 s, verification <= 60 s) or the row recorded `NOT_RUN` with its
wall time; **never retune or relabel historical arms or receipts, including v0.1.6
ones**; report `INELIGIBLE`, `INCOMPLETE` and unrun work as plainly as PASS; keep one
construction producer per ordinary operation (namespace init keeps its documented
exception, which is a fact to measure, not a lever to raise); no new durability, no
WAL/fsync, no third-party patching, no aggregate gate. This repository runs no CI -
report exactly which commands ran, which did not, and why.

## 7. Deliverables and stop conditions

- A **gap report**: per case, per phase, per arm, every cell sourced or marked
  `NOT_MEASURED`; the pairing established or declared impossible with the reason.
- For each hypothesis in §4: either a pre-registered treatment with the identity it
  will be compared against, or a written refutation with receipts.
- Status comments on [#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219)
  (and [#218](https://github.com/Ephemeral-AI-Lab/layerfs/issues/218) where the store
  path is implicated). **Nothing is closed on this handoff alone.**
- Stop when both cases carry a paired answer or a documented reason they cannot be
  paired. Do not implement an optimization before the gap is established; do not
  raise worker counts, enlarge timeouts or shrink the workload to make a gate pass;
  do not modify the reference crates as a fix.
