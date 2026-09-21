# Plan: match `namespace-10000`, in an isolated worktree (#219)

> **Status:** Handoff prompt and implementation plan. The acceptance bar in section 1
> is **approved by the owner (2026-09-21)**. No optimization is authorized before the
> gap is established with evidence, and no performance claim is made by this page.

**First goal:** `namespace-10000` must meet the approved bar. Everything else in this
plan exists to make that measurable and honest.

## 1. The case and the approved bar

| | |
| --- | --- |
| Case | `namespace-10000` - "Initialize 10,000 files / 300 MB" |
| Bytes | 10,000 files / 300 MB logical **plus a 100 MB anchor** = **400 MB** (`NAMESPACE_ANCHOR_BYTES = 100_000_000`) |
| Registered ids | `namespace-10000` and `namespace-10000-text-v1` |
| Route / timer / setup | `namespace` / `layerstack_init_ns` / `fresh-output` |
| Topology | `host-store`, 2 container CPUs, Store on the host, namespace written directly |
| Recorded candidate rows | 402.721 / 407.598 / **578.245** / 928.022 / 1020.422 / 1100.711 ms = **363-993 MB/s**, two `INCOMPLETE` |
| Recorded baseline rows | **none** (8 candidate / 0 baseline) |

**The approved bar, in three parts:**

1. **Reproduce** `<= 578.245 ms (>= 691.8 MB/s)` on a run with a **declared cache
   contract** and a **passing verification**. The row carrying that figure today is
   `cache_contract: null` and `verification_status: NOT_RUN`, so this step buys
   reproducibility and verification, not speed.
2. **Pair** the same case against the v0.1.6 reference arm - same seed, `fresh-output`,
   topology, harness and image identity, same declared cache state - with the candidate
   median **<= the reference median**, both reported with spread.
3. **No best-of.** One declared sample per arm at the declared cache state; every
   non-passing and `INCOMPLETE` row stays in the report.

## 2. Two harness mechanics that shape the plan - verify them before relying on them

**2a. `--source-arm baseline` does not select anything for this family's performance
runs.** `benchmark/fs-bench-pro/src/infra.rs:499` parses and validates
`LAYERFS_BENCH_SOURCE_ARM`, but for `init_namespace` the performance branch calls
`namespace_init_diagnostic(&work, &payload, scenario, seed, &fixture_digest, profile,
Some(&ContainerId(container)))` - which takes **no arm argument**
(`benchmark/fs-bench-pro/src/main.rs:2936-2944`). The arm is passed only to the
*verify* path (`namespace_verify_case(..., &source, ...)`). So **a "baseline" row
produced by passing `--source-arm baseline` on a current build would be mislabelled**:
it would run the same code as the candidate. S0 must confirm this by reading the code
and, if confirmed, record it as a trap: a reference row for this case can only come
from **a build of the reference product** (the v0.1.6 tag / the root crates), with its
own image and seals, which is consistent with the existing baseline rows carrying a
different `product_identity` and `image` from the candidate rows.

**2b. This case has no cold contract.** `benchmark/fs-bench-pro/shared/cold.py:22-25`
applies only when there is no sequence **and** `family == "init_namespace"` **and**
`case == "namespace-100000"`. `namespace-10000` is therefore outside the harness's
cold/residency contract, so the bar's "declared cache contract" cannot be satisfied by
that mechanism. Declare the cache state some other deterministic way, or report the row
honestly as ordinary-OS-cache with no cold claim - never as `PASS` for a cold claim.

## 3. Worktree and isolation

```sh
git worktree add ../layerfs-219-ns10000 -b codex/219-ns10000 origin/main
```

- The worktree owns its own Cargo target directory; builds must not share one across
  worktrees, and no build may take a target directory outside its own worktree.
- The measurement lock is **per worktree**
  (`core/benchmark/fs-bench-pro-storage-content/.measurement.lock`); two runs in one
  worktree never overlap, and no run interrupts another owner's.
- Host binaries are built with `python3 shared/runner.py --build-host`; the Linux image
  is passed with `--image` and its product seal must match the host's. Build and reuse
  mechanics: `benchmark/fs-bench-pro/QUICKSTART.md`.

## 4. Implementation plan

Each stage has a gate. A stage that cannot pass its gate is reported as `INCOMPLETE`
with its reason - never worked around.

**S0 - custody and feasibility (no product change).**
Inventory every `namespace-10000` receipt (case, arm, route, timer, median, setup,
cache, status) with the search recorded; confirm 2a and 2b by reading the code; and
establish *how* a v0.1.6 reference row can be produced (which tag/branch, which build,
which image, which seals) or state that it cannot.
*Gate:* a written decision on pairing feasibility, with the evidence and the exact
identities a reference arm would carry.

**S1 - reproduce the bar's first part (no product change).**
Run `namespace-10000` (both the compact and, if time allows, the `-text-v1` variant)
one sample per arm, fresh `--output`, with the cache state declared, plus a separate
`verify.sh` invocation (`families/init_namespace/verify.sh`) for the verification arm.
*Gate:* a verified, cache-declared candidate row at or under **578.245 ms**, or a
documented miss with the measured number and its cache state.

**S2 - attribution (no product change).**
Split the 400 MB: the **100 MB anchor** separately from the 10,000 small files; phases
(exec / sdk / commit / cleanup); host versus container CPU/RSS/IO; per-file versus
per-byte cost (cross the axes: same bytes as few files, same file count with different
bytes). Name which part dominates and by how much.
*Gate:* every cell sourced or marked `NOT_MEASURED`; a written mechanism, not a
correlation.

**S3 - the pair.**
Run the reference arm beside the candidate on the same case, seed, setup, topology and
declared cache state; report both identities and both spreads.
*Gate:* candidate median **<= reference median**, or a documented gap with its
mechanism and the share each candidate cause can explain.

**S4 - close the gap (only if S3 shows one; product change requires a ruling).**
Pre-register each treatment before it runs, one difference per arm, with the identity
it will be compared against. The namespace-init path keeps its documented multi-worker
exception; the worker count is **not** assumed to be the explanation (owner direction)
and raising it to pass a gate is forbidden. No Store-format change without an explicit
owner ruling; no new durability; no third-party patching.
*Gate:* matched arms with unchanged canonical results and refusal behaviour, and a
measured change in the instrument's own units - or a recorded refutation.

**S5 - verification and report.**
A passing verification arm for every performance row claimed, append-only receipts,
production LOC per commit, and a status comment on
[#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219) (and
[#218](https://github.com/Ephemeral-AI-Lab/layerfs/issues/218) where the store path is
implicated). **Nothing is closed on this plan alone.**
*Gate:* every claim in the report traceable to a receipt, and every gap stated.

## 5. Subagents

| Squad | Owns | Deliverable |
| --- | --- | --- |
| **S0** custody | Receipt inventory, 2a/2b confirmation, pairing feasibility | Inventory + decision, with the search commands recorded |
| **S1** reproduction | The candidate row, cache declaration, verification arm | Receipts + the verified row or the documented miss |
| **S2** attribution | Anchor vs small files, phases, host vs container | The attribution table with every cell sourced |
| **S3** pairing | The reference arm and the pair | Both arms' receipts and identities, the comparison |
| **S4** treatment | Pre-registered arms if a gap exists | Matched receipts or a refutation |
| **S5** review | Independent re-derivation from raw files | PASS / FAIL / INCOMPLETE per row, non-passing rows kept |

A coordinator owns the worktree, serializes builds and measurements, and holds the
lock. S5 is read-only and re-derives the arithmetic; it does not trust a summary.

## 6. Rules that bind this work

One sample per case per arm; fresh `--output`; append-only receipts; no best-of, no
re-running until a number passes; cache state declared and enforced equally across
arms, cold and warm never pooled; identities pinned (source/seal/tree, product,
compilation, dependency, image, harness, workload) so a rebuilt artifact needs a
rebuilt matched arm; complete-command budgets respected (<= 15 s, declared exception
list to 25 s, verification <= 60 s) or the row recorded `NOT_RUN` with its wall time;
**never retune or relabel historical arms or receipts, including the v0.1.6 ones**;
report `INELIGIBLE`, `INCOMPLETE` and unrun work as plainly as `PASS`; one construction
producer per ordinary operation, with `init_namespace` keeping its documented
exception; no WAL/fsync, no third-party patching, no aggregate gate. This repository
runs no CI - report exactly which commands ran, which did not, and why.

## 7. Stop conditions

Stop and report when: the bar is met with a verified, cache-declared pair; or the pair
cannot be produced and the reason is recorded; or a stage's gate fails twice for the
same reason. Do not implement an optimization before the gap is established; do not
manufacture a reference row with `--source-arm`; do not claim a cold row without the
cold contract; do not raise worker counts, timeouts or cache size to pass; do not
shrink the workload; do not modify the reference crates as a fix.
