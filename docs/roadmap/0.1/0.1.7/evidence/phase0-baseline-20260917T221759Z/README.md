# Phase 0 baseline (2026-09-17) — measurements only

> **Status:** Append-only evidence for the Phase 0 baseline of
> [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178) (phased
> optimization execution), the opening act of
> [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) (Stage 6).
> **No product, test, harness or example source was changed** — this directory
> contains measurements plus the claim-verification that produced them.

| | |
| --- | --- |
| Source commit | `dcedd7ef13338366bd80446fc7a9008cca69d683` (`main`) |
| Working tree | clean tracked tree; `working_tree_dirty false` over `core/crates`+`crates` |
| Toolchain | `cargo +1.85.1` / `rustc 1.85.1 (4eb161250 2025-03-15)`, `--locked` |
| Profile | `release` |
| Host | Apple M3 Max, 14 CPUs, 36 GiB RAM, macOS — no cross-host claim |
| Workers | **one**, every timed arm (`LAYERFS_CONSTRUCTION_WORKERS=1`) |
| Samples | **one per workload per arm**; every repeat is a labelled diagnostic |
| Budget | every command ≤ 15 s; the largest complete command was **4.034 s** — **no exception was needed** |

## Files

| Path | What it is |
| --- | --- |
| `CONTRACT.md` | the measurement contract, **written before any receipt** |
| `RUNPLAN.md` | the frozen run plan (every command, once, in order) |
| `tree.txt` | the tree identity record: commit, status, toolchain, host |
| `collect.py` | the collection driver (launches vehicles; edits nothing) |
| `commands.tsv` | **every command with its exit code and wall time** |
| `logs/` | every command's complete stdout+stderr, including **failures** |
| `receipts/p0-1-matched-workload.md` | P0-1: the matched family + two `NOT_RUN` rows |
| `receipts/p0-2-spilling.md` | P0-2: the spilling determination |
| `receipts/p0-3-counter-baseline.md` | P0-3: the per-phase counter baseline |
| `p0-1-component/run-1/` | the matched-collection receipt produced by the precedent driver |
| `p0-1-component/diagnostic-run-2/` | a labelled determinism re-run of the same driver |
| `p0-3-counters/` | per-workload outputs and timing trees |
| `client/` | the P0-3 probe client (public APIs only; not part of the product workspace) |

## The three deliverables

| Item | Outcome |
| --- | --- |
| **P0-1** matched-workload receipt | `component.primitives` **collected** on all three cases, identity `MATCH` (0.909 / 0.329 / 0.519). `pipeline.filesystem` and `pipeline.c2` recorded **`NOT_RUN`** with reasons **re-verified on this tree** (`receipts/p0-1-matched-workload.md` §3–§4) |
| **P0-2** spilling determination | **Determined: no spilling occurs.** Reachable `SQLITE_DBSTATUS_CACHE_SPILL` is proven live by a control (1,032 spills at a forced 8-page cache) and reads **0** at 1× and 4× the transaction ceiling (`receipts/p0-2-spilling.md`) |
| **P0-3** per-phase counter baseline | **Collected** for all 27 frozen workloads, with the Phase 1 anchor counters present and determinism labelled (`receipts/p0-3-counter-baseline.md`) |

## Headline findings

1. **The ordering machinery does no work at the default ceiling.**
   `order.default` (4,096 pending) reports **0 spills, 0 runs, 0 rows read and 0
   rows written**; the same shape forced to 64 reports 3,968 spills and 59,007
   rows read. The forced shape reproduces the Stage 5 round-4 grid **exactly**.
2. **No SQLite cache spilling was observed**, at 8,191 rows or at 4× that, under
   the 2 MiB default. The write-path half of the O1 premise is unsupported. The
   read-path half still waits for **P1-2**, as the study itself requires.
3. **All matched component rows are `MATCH` and faster than the reference**, but
   the `small` case's same-tree re-run spread (0.909 → 0.883) is as large as the
   cross-tree delta, so cross-tree comparison is **diagnostic only**.
4. **Two counters are not reachable through the frozen vehicles**: `ValidationWork`
   (no vehicle prints it) and the edit family's `nodes_read` (only `edit_timing_c1`
   does; `measure_edits` does not). Both are recorded `NOT_EXPOSED`; adding a print
   would be a harness change, which this campaign must not make.

## Honest retention

- **Failed attempts are on disk.** D7–D24 failed once each on a driver-side
  invocation error (`--entries` on a vehicle that does not accept it; an `--output`
  path that the vehicles create themselves). Their nonzero exit codes and outputs
  are in `commands.tsv` and `logs/`, beside the successful re-runs. Nothing was
  overwritten.
- **A collection bug was found and fixed before the answer depended on it.** The
  first `diagnostic-large-cache` arm applied its pragma on a connection that no
  longer existed when the work ran, so the arm was not actually a raised-cache arm.
  It was replaced by the A/B in `receipts/p0-2-spilling.md` §3, where both arms
  differ **only** in the cache profile and the difference is visible in the
  read-backs.
- **No receipt in this directory was edited after collection.**

## Reproduction

```sh
cd docs/roadmap/0.1/0.1.7/evidence/phase0-baseline-20260917T221759Z
python3 collect.py build     # B1-B3
python3 collect.py p0-1      # C0    the matched family
python3 collect.py p0-2      # S0-S3 spilling
python3 collect.py p0-3      # D1-D26 counters
python3 collect.py extra     # D27   nodes_read anchor
python3 collect.py diag      # X1-X3 labelled determinism diagnostics
```

The driver refuses to reuse an existing output path, so a re-run must target a
fresh stamp directory. `client/` is built with
`CARGO_TARGET_DIR=/tmp/layerfs-phase0-target cargo +1.85.1 build --release --offline --locked`.

## What this directory does not claim

- **No optimization, no tuning, no pragma change on any gate sample.** The one
  pragma-changing arm is labelled diagnostic and is never a gate sample.
- **No release-admission claim, no complete-operation claim, no cold-cache claim,
  no storage claim, no cross-host claim.**
- **No Phase 1 item is opened or closed here.** The re-prioritization implied by
  findings 1–2 belongs to the owner.
