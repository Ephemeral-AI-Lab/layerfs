# Retained-history storage — T1

> **Status:** Specification and target. **Nothing here is implemented.** T1 is the only tier this
> campaign will pursue; T2, T3 and the floor are deferred and recorded separately.
> Tracking: [#187](https://github.com/Ephemeral-AI-Lab/layerfs/issues/187), a sub-issue of
> [#186](https://github.com/Ephemeral-AI-Lab/layerfs/issues/186).
> Parent specification: [`retained-history-storage.md`](../retained-history-storage.md).

````text
claim_kind = history-storage-efficiency
tier       = T1
target     = 47,048,435 B apparent   (0.954x v0.1.6's 49,315,840 B)
```

## Claim labels

| Label | Meaning |
| --- | --- |
| **measured** | read from a Store, a trace, or the corpus by the #187 campaign |
| **computed** | arithmetic over measured parts, stated in full |
| **est** | one or more inputs are borrowed from a different population |
| **proposed** | does not exist; a design to be argued with |
| **open — required** | a prerequisite for something else here |
| **deferred** | explicitly out of scope; an owner ruling must precede it |

## Source pins

- Product and harness read at commit `66bce8378`.
- The #187 campaign's harness change (`core/benchmark/.../src/ops/history.rs`, `shared/space.py`,
  `shared/test_space.py`) is **uncommitted** in the working tree at that commit. Production LOC delta
  **0**; `core/benchmark/` is its own Cargo workspace and is not product source.
- Corpus: `/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data`, manifest sha256
  `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`, tip
  `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`.
- Comparison artifact: v0.1.6's retained stride-10 Store,
  `benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite`,
  49,315,840 B, sha256 `80c2b10a7ca1513228306e42063611e2b716be053023b65073d0ab67fbee50af`.
- **No timing figure appears in this document.** The campaign's machine was shared; every access cost
  is stated in **bytes per read**.

## The target

`history-stride10`, 17 states `{1, 11, ..., 151, 157}`, **apparent bytes** (`st_size`, which equals
`page_count x page_size` exactly on every arm).

| | apparent | vs v0.1.6 |
| --- | --: | --: |
| the registered lane today | 128,864,256 | 2.6130x |
| **T1 target** | **47,048,435** | **0.9540x** |
| v0.1.6 (the gate) | 49,315,840 | 1.000x |

````
  128,864,256  ################################################################  registered lane
               |  R0  declare the base            -65,126,400   (81.87 % of the gap)
   63,737,856  ################################                                 [measured]
               |  R1  index: capacity + lifetime   -9,670,811
   54,067,045  ###############################                                  [computed]
               |  R2  lean row grammar             -7,018,610
   47,048,435  ##############################                                   TARGET
   49,315,840  ###############################                                  v0.1.6
               ^ margin 2,267,405 B = 0.954x
````

## Why the gate is decided by the schema, not the codec

Three causes account for the whole 79,548,416 B gap, **residual 0**:

| # | cause | bytes | share |
| --: | --- | --: | --: |
| 1 | the caller declared no cross-commit delta base | 65,126,400 | 81.87 % |
| 2 | remaining pack-blob gap (coverage) | 7,403,406 | 9.31 % |
| 3 | non-pack SQLite overhead | 7,018,610 | 8.82 % |

**The codec is not the cause and is not a lever.** The v0.1.7 codec parameter set is byte-identical to
v0.1.6's, and on the 44,141 whole-file objects both Stores hold, v0.1.7 is **2.27 % better** on the
no-delta bytes (FULL rate `0.977288x`). Cause 3 is a *fixed* per-object cost that does not shrink with
the content — which is why it, and not the codec, decides the gate.

## The work

| # | change | layer | buys | format change | status |
| --: | --- | --- | --: | --- | --- |
| **R0** | Declare the base — the caller supplies the ids | caller | **65,126,400 B** | no | **proposed**; harness-proven |
| **R1** | Index: **capacity** first, then **lifetime** | C2 | the rest of the coverage | no (persisted form: yes) | **proposed** |
| **R2** | Lean the SQLite row grammar | C2 | **7,018,610 B** | **yes** — owner amendment | **proposed** |
| **R3** | Fix the depth *measurement* | C2 | legality, not bytes | no | **proposed** |

Full specification, evidence, acceptance and open rulings: [`t1-implementation.md`](t1-implementation.md).
Blast radius, file structure and LOC: [`T1-BLAST-RADIUS.md`](../evidence/stage-6-history-187-20260920T000000Z/squad-c/T1-BLAST-RADIUS.md).
**Implementation handoff (paste-ready for the squad):**
[`history-t1-implementation-prompt.md`](../component-decoupling/history-t1-implementation-prompt.md).

## What is deferred

**T2, T3 and the floor are deferred.** They are recorded in
[`core/docs/architecture/deferred/02-history-tier-floor-deferral.md`](../../../../../core/docs/architecture/deferred/02-history-tier-floor-deferral.md)
and tracked by [#189](https://github.com/Ephemeral-AI-Lab/layerfs/issues/189) — a **standalone** issue,
no parent, carrying the `deferred` label. **No work is scheduled on them, and no number from them may
be quoted as a T1 result.**

The implementation work for T1 is tracked by
[#188](https://github.com/Ephemeral-AI-Lab/layerfs/issues/188), a sub-issue of
[#187](https://github.com/Ephemeral-AI-Lab/layerfs/issues/187).

| tier | apparent | what it is | why deferred |
| --- | --: | --- | --- |
| T2 | ~44,374,000 **est** | T1 with the remaining base-less records grouped into ~256 KiB frames | needs an access-cost ruling (59.8x read amplification) and a selective rule |
| T3 | ~31,100,000 | per-path version chains in large frames, zstd -19 | a different design: needs a path concept the Store does not have |
| floor | ~21,382,000 | one zstd -22 stream over the whole union | a lower bound, not a design: a read decodes 372 MB |

## Guardrails

- **No product source change without an owner ruling.** R0–R3 are proposals. The harness change is the
  only thing in the working tree.
- **The 217-row lane is untouched.** `--lane full` is 220 rows, `--lane smoke` is 20, both unchanged.
  `history.*` is outside the 217 by construction.
- **Never optimise `history-stride1`.** Iterate on stride10; confirm on stride3.
- **No timing claim.** Bytes only.
- **Every number here is diagnostic**, not admission evidence, until re-run under #186's contract.

## Evidence

All under `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/`:

| what | where |
| --- | --- |
| Step 0 A/B, the root-cause register, the decoder | `step0/README.md` |
| the synthesis, the proposal, the target tiers | `squad-c/SYNTHESIS.md`, `squad-c/PROPOSAL.md`, `squad-c/RECOMMENDATION.md`, `squad-c/TIERS-AND-COMPLEXITY.md` |
| the advisory list, exhaustively | `squad-e/squad-e/E1-advisory-list.md` |
| is a history model required? | `squad-e/squad-e/E2-history-concept-necessity.md` |
| #185 assessed | `squad-e/squad-e/E3-issue185-assessment.md` |
| the smallest core change | `squad-e/squad-e/E4-minimal-core-change.md` |
| the ledger entry | `LEDGER-ENTRY.md` |
