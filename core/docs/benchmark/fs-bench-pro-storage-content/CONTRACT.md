# Stage 6 measurement contract

> **Status:** Frozen contract for Stage 6 ([#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)),
> written **before** any harness code or sample collection, as
> `docs/general/benchmark_rules.md` §1 requires. A change after collection needs a new
> stamp and a new directory; this file is never re-dated.
>
> Case specifications: [`c1-families.md`](c1-families.md),
> [`c2-families.md`](c2-families.md). Measurement axes:
> [`memory_cpu_space_support.md`](memory_cpu_space_support.md). Preparation:
> [`test_setup_and_cache_discipline.md`](test_setup_and_cache_discipline.md). Gates and
> oracles: [`gates_and_oracles.md`](gates_and_oracles.md). Estimate:
> [`implementation_estimate.md`](implementation_estimate.md).
>
> This file is an **index and the decision record**. It does not restate the six
> specifications; where they disagree with this file, this file wins and the
> disagreement is a defect to fix.

## 1. The question and the exact claim

```text
claim_kind = structural-complexity
```

**The question:** is the replacement C1/C2 core algorithmically sound — correct at every
declared boundary, scaling as its architecture claims, and bounded in memory, CPU and
disk?

**The claim this contract is allowed to support:** the C1/C2 implementation, at the
frozen profile, satisfies its declared bounds and its declared complexity classes on
the registered workloads.

**The claim it is NOT allowed to support:** that v0.1.7 is faster than v0.1.6. No
C1/C2 family may be paired with a v0.1.6 family — different operation surfaces
(`benchmark_rules.md` §8). The only legitimate matched pair in the tree is
`component.primitives` (3 cases), and its receipt is explicitly diagnostic.

Consequences, frozen: every gate is **absolute and single-arm**; `elapsed_ns` is
**diagnostic** and can never produce `FAIL`; the scaling gates read **counters, heap
and disk**.

## 2. Scope

**In:** `layerfs-content` (C1) and `layerfs-storage` (C2), measured through their
public APIs.

**Out:** Workspace, Branch, Commit, LayerStack, FUSE, POSIX, daemon, SDK, container,
cgroup, Monitor, and any runtime integration. Runtime acceptance is Stage 7
([#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172)). No runtime, FUSE or
cloud work may be added to obtain a core benchmark.

## 3. Registered membership and frozen cardinality

`c2.delta.boundaries` is a registered sub-lane of family C2-3, not a 21st family:
**20 families** (11 C1 + 9 C2), 21 registry rows counting the sub-lane separately. Case IDs
are globally unique and are `c1.*` / `c2.*` / `pipeline.*`.

| Layer | Family | Cases |
| --- | --- | ---: |
| C1 | `c1.construct.whole-file` | 4 |
| C1 | `c1.construct.chunked` | 4 |
| C1 | `c1.cdc.chunk-count` | 12 |
| C1 | `c1.edit.length-preserving` | 12 |
| C1 | `c1.edit.length-changing` | 32 |
| C1 | `c1.transition.boundary` | 7 |
| C1 | `c1.many-tiny` | 20 |
| C1 | `c1.tree.construct-traverse` | 12 |
| C1 | `c1.tree.namespace-mutation` | 4 |
| C1 | `c1.change-locality` | 12 |
| C1 | `c1.fs.build-scale` | 8 |
| | **C1 subtotal** | **127** |
| C2 | `c2.lifecycle` | 5 |
| C2 | `c2.reuse.cross-file` | 10 |
| C2 | `c2.delta.cdc-locality` | 20 |
| C2 | `c2.delta.boundaries` *(C2-3 sub-lane)* | 21 |
| C2 | `c2.reuse.workspace` | 14 |
| C2 | `c2.footprint` | 6 |
| C2 | `c2.delta.small-file` | 4 |
| C2 | `c2.read.waves` | 4 |
| C2 | `c2.pool.cold-warm` | 2 |
| | `pipeline.*` | 4 |
| | **C2 + pipeline subtotal** | **90** |
| | **Total** | **217** |

Declared cardinality array, checked by `registry::self_check()`:
`[4,4,12,12,32,7,20,12,4,12,8,5,10,20,21,14,6,4,4,2,4]`.

**`component.primitives` is registered but is not one of the 217.** It is 3 further
diagnostic cases (the only library-matched reference pair in the tree, from
`component_primitives.rs`). They run, they are receipted, and they are **excluded
from admission and from every count in this section** — under `claim_kind =
structural-complexity` (D1) their receipt is diagnostic and cannot gate. The registry
therefore holds **220 rows = 217 admission + 3 diagnostic**, and a report that folds
the 3 into 217 is wrong.

**Selection lanes.** `--smoke` selects one tier per family = **20 cases** (the
development loop, target <= 60 s). Full = **217** (admission). A lane that cannot fit
the budget is recorded `NOT_RUN` with its measured wall time, never shrunk.

**Cardinality parsing rule.** A bracketed profile list in a case ID (`[-compact-v2|-mixed-v4]`)
means **one case** rendered with a tier-selected profile — never one case per profile.
Read the other way the C1 ID list parses to 175 against a table that sums to 127.

## 4. Measurement contract (inherited)

The full text is in `memory_cpu_space_support.md` and
`test_setup_and_cache_discipline.md`. Frozen here:

| Item | Value |
| --- | --- |
| Samples | **one per case per arm**; no n3, no best-of, no re-runs to improve a number |
| Outputs | fresh path per run; receipts append-only; failures retained |
| Cache state | declared per row, enforced equally, **never pooled across states** |
| Workers | **1**; `LAYERFS_CONSTRUCTION_WORKERS=1` exported and asserted in the receipt |
| Complete command | <= **15 s**; declared exceptions <= **25 s**; verification <= **60 s** |
| Lock | measurement lock held per `perf`/`verify` invocation |
| Toolchain | `cargo +1.85.1`, `--locked`, release |
| Clocks | one domain: `CLOCK_MONOTONIC_RAW` (id 4), Rust and Python |
| Trace schema | `layerfs-trace-v1` |
| Receipt schema | `layerfs-core-receipt-v1` |

## 5. Owner decisions (frozen)

### D1 — Claim kind: `structural-complexity`

**Decided.** Rationale: the comparative option is not achievable — P0-1 recorded
`pipeline.filesystem` and `pipeline.c2` as `NOT_RUN` because the reference's workspace
update is private and `WorkspaceAdmission` has no public method; only
`component.primitives` (3 cases) is a matched pair, and its same-tree spread
(0.909 -> 0.883) is as large as its cross-tree delta.

**Reopen if:** the owner commissions a reference-tree entry point. That is a change to
`crates/`, not to `core/`, and it needs a new scenario identity — historical receipts
are not re-labelled.

### D2 — Per-sample acquisition does **not** invalidate a row

**Decided:** acquisition wall is reported as its own field (`acquisition_wall_ns`)
**outside** every operation timer and **outside** the row's admission decision; the
complete-command status is reported separately. Rationale: acquisition is not product
work, and `benchmark_rules.md:251-255` distinguishes operation timers from lifecycle.

**Consequence, mandatory rather than optional:** the `mincore`-first de-warm is
required (the touch-every-page path measured **18.57 s** for a 100k-file fixture), and
`acquisition_wall_ns` is published so the campaign's real cost is visible.

### D3 — The harness is its own workspace, with a lock-parity test

**Decided:** `core/benchmark/fs-bench-pro-storage-content/` is its own Cargo workspace,
carrying `Cargo.toml` (**with an empty `[workspace]` table**, or `exclude = ["benchmark"]`
in `core/Cargo.toml`), its own `Cargo.lock` and its own `target/`.

**Mandatory mitigation:** `shared/test_lock_parity.py` asserts that every package
present in both locks matches version and checksum. Without it the harness could link a
different product than the product seal names. **This test must exist before the first
receipt.**

**Alternative, recorded and declined for now:** making the child an example target of
`layerfs-storage` removes the second lock entirely (`core/Cargo.lock` diff empty) at the
cost of a benchmark-only compile error failing a core build.

### D4 — Over-budget tiers are **declared**, not cut

**Decided:** the 500 MiB payload tiers and the 100k-file / 500 MB footprint controls are
registered and carried on the <= 25 s exception list with their measured wall times.
Rationale: a cut removes the tier where the O(1) claim is most convincing and removes
the 100k controls that are the point of `c2.footprint`. Shrinking a workload to fit a
budget is forbidden.

### D5 — `c2.delta.small-file` is defined, not deferred

**Decided:** 4 cases, one per size tier, exercising the delta policy below the cutoff.
Rationale: `benchmark_rules.md` §7 forbids accepting favourable members while moving
unfavourable siblings to a later release; dropping the family would do exactly that,
and defining it costs four rows and one shape driver. C2 therefore reads **90**, not 86.

## 6. Gates and oracles

Seven gate classes (G1 correctness, G2 mechanism, G3 scaling, G4 resource, G5 cleanup,
G6 custody, G7 timing purity) and seven oracle classes (O1..O7) are frozen in
`gates_and_oracles.md`. Two consequences of D1 are restated here because they decide
admissibility:

- **`elapsed_ns` never gate-decides.** With the established **+17.6 %** same-binary
  spread the O(n) time band overlaps O(n log n) and cannot separate O(n) from O(1).
- **The O(1)-memory claim gates on the counting allocator**, not RSS: at a 10 ms
  interval RSS cannot cover any phase under ~200 ms, which is most of the size ladder.
  RSS is a G4 bound and an anomaly detector.

Two invariants added by review and frozen here:

- **Device attestation:** a row claiming a de-warmed or cold read must show
  `disk_read_bytes >= 0.9 x requested`, or it is `INELIGIBLE`. `mincore` alone cannot
  distinguish a cache-served read from a device read — the #151/L18 error.
- **Allocation attribution:** a row gating `store_allocated_bytes` must be
  `exclusive`; a COW clone's `st_blocks` double-counts blocks shared with the master,
  so the reflink rung is **forbidden** for `c2.footprint`.

## 7. Three acceptance bullets with no instrument yet

`#171` requires these; none is designed. They are Stage 6 work, not waivers:

| Bullet | Requirement |
| --- | --- |
| Failure / unknown-outcome / cleanup | exercised **without product fault injection** — external perturbation only (a declared process kill, or a hand-edited watermark so `Store::open` refuses with `Integrity`) |
| Concurrency / visibility | two `begin_save` on one Store; the second must fail `OwnershipUnavailable` (busy timeout is zero) |
| Zero retry / fallback / fsync / WAL | a sealed call-graph/manifest status plus observable runtime tripwires. **A fabricated zero is forbidden**; a counter that cannot fail an assertion is not evidence |

Until each is designed and implemented, its row is `NOT_RUN` with this contract as the
reason.

## 8. Evidence layout

```text
benchmark-results/fs-bench-pro-storage-content/     gitignored, development runs
  prepared/<compatibility-digest>/                  immutable master + manifest
  <run_id>/<case_id>/
    timing.json        byte-verbatim product receipt (never edited)
    trace.jsonl        the harness trace
    receipt.json       derived: identity, gates, statuses, budget
    report.txt         derived: the four-axis human view
docs/roadmap/0.1/0.1.7/evidence/<stamp>/            admission evidence, append-only
```

Every retained file is hashed into a run manifest. Receipts are never overwritten;
failures, `INELIGIBLE` rows and discarded attempts stay on disk with their exit codes.

## 9. What is explicitly not claimed

- No product speedup, and no comparison against v0.1.6 beyond `component.primitives`.
- No absolute latency, throughput or memory target; this contract creates the case set and the gates, not the numbers.
- No total-RSS cap — only application-owned bounds that are established.
- No cold-cache claim except under a parameterized cold contract with verified residency.
- No `init_namespace` 2.7 s target — that is Stage 7.
- No aggregate gate or CI claim; the retired `tools/preflight.sh` is not restored.

## 10. Production LOC

**Zero.** `AGENTS.md` excludes benchmark harnesses and development tools from the
production count, and `core/tools/check_product_boundary.py:90-95` scans only
`core/crates/*/src` and `core/crates/*/sql`. Every commit in Stage 6 reports
`Production LOC: <unchanged> -> <unchanged> (delta 0)`.

## 11. Errata and the verification pin

This contract is never re-dated (§1's header). But the **product** moves, and a
specification written against one commit silently becomes wrong about another. So
the pin lives here, explicitly, and every correction is recorded rather than
applied in place.

```text
specification frozen at   686c6f140   (2026-09-18, this directory + the family specs)
product re-verified at    f1bcf3789   (2026-09-19)
```

An independent read-only review re-checked the specifications against the product at
the verification commit: **every named public API still exists and is public**, every
**numeric boundary is still right** (cutoff 131,072; chunks 8,192/16,384/32,768;
4,096 edits; the walk, inode-leaf, branch, scratch, batch, transaction,
`METADATA_INDEX_VALUES`, `READ_OBJECT_LIMIT` and `LOOKUP_PAGE_IDS` limits), the C2
profile still holds, and the **35-test** sealed-oracle parity set is exact. Phase 1's
`perf(core)` rounds landed *after* the freeze and changed two things the specs
asserted:

| # | The specification said | Verified at the pin | Corrected in |
| --- | --- | --- | --- |
| E1 | the C2 statement cost centre is "one statement per row" | **wrong since P2-2** — one multi-row `INSERT` per bound chunk, chunk derived from the engine's own limits and capped at 128; `SaveOutcome.statements` and `presence_queries` are the counters that move | `c2-families.md` §1 |
| E2 | `SortedWork.pages_read` undercounts batched merges, and an inner engine in `Engine::apply_root` loses its work — both "must be fixed before counters can gate" | **first fixed by P1-11** (`sorted/page.rs:277`); **second does not reproduce** — the crate's only `Engine::<F>::new` is `sorted/finish.rs:33`, returned at `:109`, aggregated by both callers | `c1-families.md` §6, `c2-families.md` §7, `gates_and_oracles.md` §7 |
| E3 | the cache ladder is three constants; the read counters are four fields; the `opens` gate is absolute | **short by one constant** (`DECODED_GROUP_CACHE_BYTES` = 512 KiB, P2-4), **short by five fields**, and **route-dependent** — `opens` is only 0 on the opening wave through `StoreProvider::read_wave` | `c2-families.md` §§1,4; `gates_and_oracles.md` §5 |

**The instruction this leaves for Stage 6**: re-verify a cited constant or mechanism
at the commit you are actually measuring before you design a case around it. The pin
above is a checkpoint, not a promise — #178 and later work will move the product
again, and a specification that cites a mechanism is only as current as its last
verification. When you find drift, add it to this table; do not edit the prose
silently.

