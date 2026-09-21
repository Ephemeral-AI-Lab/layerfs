# Handoff: root-cause the 3.8x gap on the `namespace-10000` work

> **Status:** Handoff prompt. Filed from [#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219)
> after `pipeline-namespace-10000` first passed
> (`cc3f91989`, 2026-09-21). It commissions an **attribution and root-cause analysis**,
> not an implementation. **No performance claim is made by this page**, and the 3.8x
> figure below is **one sample per arm on two different harnesses** — it is a
> hypothesis to attribute, not a result.

## 1. The two rows, and the gap

Both perform the same declared work: build a **10,000-file / 100-directory** namespace
and persist **~300 MB** of file content including a 100,000,000-byte anchor.

| | v0.1.6 `init_namespace` / `namespace-10000` | v0.1.7 `pipeline.*` / `pipeline-namespace-10000` |
| --- | --- | --- |
| timer | `layerstack_init_ns` | `operation_ns` (root `pipeline`) |
| measured | **944.881 ms** | **3,585.5 ms** |
| ratio | — | **3.80x** |
| canonical bytes | 302,182,831 | 301,171,810 |
| objects | 25,158 `candidate_objects` | 25,245 `pipeline.inserted` |
| files / directories | 10,000 / 100 | 10,000 / 100 |
| operations stating bindings | **1** `initialize_layerstack` | **3** (`prepared.batches(4096)`: 4096 / 4096 / 1908) |
| transactions | **73** `initialize_admission_transactions` | **17,378** `pipeline.commits` |
| cache declaration | `reused-first-sample-uncontrolled`, `cache_contract: null` | `prepared-dewarmed`, 0 resident pages |
| receipt | `benchmark-results/issue219/s1-ns10000-candidate-20260921T031259Z/perf.jsonl` | `core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/ns17-pinned2-20260921T031259Z/pipeline-namespace-10000/receipt.json` |
| identities | source `b0260df3a` (dirty=false), product seal `b3e3cb7453…`, image `sha256:d152ced8d21c…` | same tree; product is the `core/` workspace, so seals differ by construction |

**The gap is not bytes and not objects.** Canonical bytes agree within 0.33 % and object
counts within 0.35 %. Something else costs 2.64 seconds.

## 2. The prime suspect, stated as a hypothesis

`pipeline.commits` is **17,378** against v0.1.6's **73** admission transactions — **238x**
more. The mechanism that moved it is already attributed for a different row:
`7075f338db36b209b59031e3e55ae11cf87eed57` ("replace the fixed two-save model with a
configured per-Store writer budget") made **every step commit before releasing the
arbitration lock** (`core/crates/layerfs-storage/src/cas/lifecycle.rs:166-175`), where the
retired model acknowledged once. That same commit moved
`pipeline-filesystem-build`'s `pipeline.commits` from **1 to 43** and forced a pin
re-baseline.

**Hypothesis:** the gap is dominated by commit cadence, not by bytes, traversal or
construction. **It is a hypothesis. §3 lists confounds that must be removed before it
can be tested, and one of them may explain the gap on its own.**

## 3. Confounds that must be neutralized FIRST

These are ordered by how likely each is to explain the gap by itself. **Do not test the
§2 hypothesis until §3.1 and §3.2 are closed** — either could account for the whole
3.8x, and both are cheaper to settle than an attribution campaign.

### 3.1 Worker count — likely the largest single confound

`AGENTS.md` §3.8 makes `init_namespace` **the only case permitted multi-worker
construction**: its path (`LayerStackStore::initialize_layerstack` →
`direct_initialize_root_directories_inner` / `prepare_parallel_root_directories`) uses
`available_parallelism().min(8)` threads and keeps its 2.7 s cold Init target.

The v0.1.7 row ran with **`LAYERFS_CONSTRUCTION_WORKERS=1`**, exported by
`runner.py:326` for every row. So v0.1.6 ran multi-threaded and v0.1.7 ran
single-threaded, and **raising the v0.1.7 count to match is forbidden** by §3.8 ("no run
raises it, and no second lane or helper worker is added to pass a gate") unless the
owner rules otherwise for a *measurement* arm.

The two rows also record different parallelism: v0.1.6's own CPU/wall on the same case
was ~1.9-2.1x; v0.1.7's row should be checked the same way. **First deliverable: report
each row's CPU/wall, and state how much of the 3.8x a thread-count difference can
explain.**

### 3.2 Construction boundary — the two rows do different amounts of work

v0.1.6's `layerstack_init_ns` **includes construction**: it scans the fixture's 300 MB
and chunks it inside the timer. v0.1.7's row **excludes** it: the C2 rule
(`core/benchmark/fs-bench-pro-storage-content/src/ops/c2.rs:1-22`, "C2 never runs C1
file construction inside a measured phase") means the 300 MB was constructed **before**
the timer as supplied objects.

**So v0.1.7 does strictly less work inside its timer and is still 3.8x slower.** That
makes the gap worse than it looks, not better — but it also means the two timers do not
cover the same phases and any like-for-like claim must say which phase it compares.
**Second deliverable: a phase matrix with one row per phase (construction, open,
build, handoff, save, publication, commit, cleanup) and one column per arm, every cell
sourced or `NOT_MEASURED`.**

### 3.3 Cache declarations differ and must not be pooled

v0.1.6's row is `reused-first-sample-uncontrolled` with `cache_contract: null` (the
harness's cold contract covers only `namespace-100000`). v0.1.7's row is
`prepared-dewarmed` with a residency gate reporting **0 resident pages**. These are two
different declared states and `AGENTS.md` §1 forbids pooling them or implying one
contract covers both.

### 3.4 Statement-count ceiling forces 3 operations

`MAXIMUM_WALK_ENTRIES = 4_096`
(`core/crates/layerfs-content/src/filesystem/limits.rs:86`) is charged per operation, so
10,101 bindings cannot be stated once. v0.1.6 states all 10,000 in one
`initialize_layerstack` call and splits internally. **Whether the 3-way split itself
costs anything is unknown and must be measured, not assumed** — a batching overhead of
unknown size sits inside the 3.8x.

### 3.5 One sample per arm

Both figures are single samples. v0.1.6's own recorded history on this case spans
402-1,101 ms across builds, and the machine shows 16.7-26.0 s window-to-window variation
on another lane. **No spread is known for either arm.** Any attribution must state what
it cannot distinguish from noise.

## 3.6 The decomposition this session could already compute — start here

Pulled from the two receipts' own CPU counters, so it needs no new run. **This is the
most informative number in this page and it reframes §2.**

| | v0.1.6 | v0.1.7 | ratio |
| --- | ---: | ---: | ---: |
| wall (`layerstack_init_ns` / `operation_ns`) | 944.9 ms | 3,585.5 ms | **3.80x** |
| CPU (user + system) | 1,788.8 ms | 3,354.4 ms | **1.88x** |
| **CPU per wall** (parallel efficiency) | **1.89x** | **0.94x** | **2.01x** |
| peak RSS | 72.2 MB | 524.8 MB | 7.3x |
| heap peak (windowed) | — | 21.8 MB | — |

**The 3.8x factorises as 1.88x (more CPU work) x 2.01x (less parallelism) = 3.78x**,
which accounts for the observed 3.80x. Two consequences:

1. **Roughly half the gap is not a speed problem at all — it is a thread-count
   problem.** v0.1.7 ran at **0.94x CPU/wall**, i.e. single-threaded, while v0.1.6
   achieved **1.89x**. `AGENTS.md` §3.8 is why: `init_namespace` is the documented
   multi-worker exception and v0.1.7's row ran under `LAYERFS_CONSTRUCTION_WORKERS=1`.
   §3.1 is therefore **not** a side confound; it is the largest single term.
2. **v0.1.7 also does 1.88x more CPU work than v0.1.6 — while its timer excludes
   construction and v0.1.6's includes it** (§3.2). That is the part that is genuinely
   v0.1.7's, and it is the part worth a root cause. Commit cadence (§2, 238x more
   transactions) remains the prime suspect for it.

**Revised first deliverable.** Before any cadence experiment: state whether v0.1.7's
**0.94x CPU/wall is expected**. A single-threaded save path would explain it and would
mean the honest comparison is *CPU work* (1.88x), not wall. If instead v0.1.7 is
supposed to be parallel and reports 0.94x, that is a separate finding about the
replacement core's parallelism, and it is larger than the commit-cadence question.

**Peak RSS is a second, independent signal**: 524.8 MB against 72.2 MB. The row holds
300 MB of constructed content in a `TreeStore` before the timer (the C2 supplied-object
rule), so this is expected in kind — but 7.3x is worth confirming against
`payload-create-500m`'s 527 MB for 500 MiB, and it matters for §5's memory rules.

## 4. Questions this handoff must answer

1. **Is v0.1.7's 0.94x CPU/wall expected?** The ratio is computed in §3.6 from
   receipts that already exist: if a single-threaded save path is intended, the honest
   comparison is CPU work (1.88x) and not wall (3.80x). If it is not intended, that is
   a finding in its own right. (§3.1, §3.6)
2. **How much is the construction boundary?** Build the phase matrix. (§3.2)
3. **How much is commit cadence?** One difference at a time: hold object count and
   bytes fixed and vary only the commit cadence — the writer budget
   (`store_policy.max_concurrent_writes`), or the transaction byte limit — and report
   the movement in the instrument's own units.
4. **Does the 3-operation split itself cost anything?** Compare 3x4096-binding
   operations against a single operation at a raised ceiling *in a diagnostic arm only*,
   labelled as a diagnostic, never as a gate sample.
5. **Where does the time actually go inside v0.1.7's timer?** `SaveOutcome.profile`
   already splits the accept path into seven disjoint buckets
   (`core/crates/layerfs-storage/src/cas/store.rs:68-73`). **Read it first** — this is
   the cheapest evidence in the tree and may answer the question without a campaign.

## 5. Method rules that bind this work

- One sample per case per arm; fresh `--out` / `--output`; receipts are append-only and
  are never overwritten. **No best-of, no re-running until a number looks right.**
- Pin identities: source commit/seal/tree, product, compilation and dependency seals,
  image, harness and workload hashes. The two arms are **different workspaces**
  (`crates/` vs `core/`) and their seals differ by construction — say so rather than
  implying a matched build.
- **Do not raise worker counts, timeouts or cache sizes to move a number.** §3.1's
  worker question is answered by *reporting*, not by re-running v0.1.7 with more
  workers, unless the owner rules an explicit diagnostic arm.
- Cache state is declared and enforced equally; cold and warm are never pooled.
- **Never retune, relabel or promote a historical receipt**, including the v0.1.6 row
  and its `cache_contract: null`.
- `core/` and `crates/` build separately; a rebuild on one side invalidates any paired
  arm on that side.
- Budgets: a performance selection's complete command is <= 15 s with a declared
  exception list to 25 s; verification <= 60 s. `pipeline-namespace-10000` measures
  **3,585.5 ms** of operation time and its complete command must be checked against the
  limit — if it does not fit, it is recorded `NOT_RUN` with its wall time, never made to
  fit.
- This repository runs no CI and no aggregate gate. Report exactly which commands ran,
  which did not, and why.

## 6. Where to start, in order

1. **Confirm or refute that v0.1.7's save path is single-threaded** (§3.6). Half the
   gap is here and it is a yes/no question about intent.
2. `SaveOutcome.profile`'s seven buckets for the v0.1.7 row (§4.5). Cheapest way to
   localize the remaining 1.88x of CPU work.
3. The phase matrix (§3.2).
4. Only then, a one-difference commit-cadence arm (§4.3).

## 7. Definition of done

- The 3.8x is **attributed by phase with a mechanism**, or each candidate cause is
  recorded as refuted with the arm that refuted it — or the analysis is declared
  `INCOMPLETE` with the reason.
- The worker-count confound is **quantified or explicitly declared unquantified**;
  §3.6's 1.88x / 2.01x decomposition is confirmed, corrected, or refuted.
- Every cell in the phase matrix is sourced or `NOT_MEASURED`.
- #219 (and #218 where the store path is implicated) carries a status comment with the
  numbers and the gaps. **Nothing is closed on this handoff alone.**
