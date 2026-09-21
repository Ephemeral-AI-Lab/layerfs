# Handoff: root-cause and close the `namespace-10000` single-thread gap

> **Status:** Handoff prompt. Filed from [#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219)
> after `pipeline-namespace-10000` first passed (`cc3f91989`, 2026-09-21). It commissions
> an **attribution, root-cause analysis and optimization campaign**, run by subagents.
> **No performance claim is made by this page**; the numbers below are **one sample per
> arm on two different workspaces** and are a hypothesis to attribute, not a result.
>
> **Owner direction (2026-09-21), which sets the frame:** v0.1.7 runs **single-threaded
> by design, and stays single-threaded.** The question is **how far optimization can go
> within one thread** — not how to match v0.1.6's parallelism.

## 1. The two rows

| | v0.1.6 `init_namespace` / `namespace-10000` | v0.1.7 `pipeline.*` / `pipeline-namespace-10000` |
| --- | --- | --- |
| timer | `layerstack_init_ns` | `operation_ns` (root `pipeline`) |
| wall | **944.9 ms** | **3,585.5 ms** |
| CPU (user+system) | 1,788.8 ms | **3,354.4 ms** |
| CPU per wall | 1.89x | **0.94x** |
| canonical bytes | 302,182,831 | 301,171,810 |
| objects | 25,158 | 25,245 |
| transactions | 73 admission transactions | **17,378 commits** |
| peak RSS | 72.2 MB | 524.8 MB |
| operations stating bindings | 1 | 3 (`batches(4096)`: 4096/4096/1908) |
| cache declaration | `reused-first-sample-uncontrolled`, `cache_contract: null` | `prepared-dewarmed`, 0 resident pages |
| receipt | `benchmark-results/issue219/s1-ns10000-candidate-20260921T031259Z/perf.jsonl` | `core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/ns17-pinned2-20260921T031259Z/pipeline-namespace-10000/receipt.json` |

Both arms: source `b0260df3a` (dirty=false), product seal `b3e3cb7453…`, image `sha256:d152ced8d21c…`, harness `8a6d76dd…`. The two arms live in **different workspaces** (`crates/` vs `core/`), so their compilation/dependency seals differ by construction.

## 2. The target, computed — the parallelism term is NOT a defect

**Owner direction: single thread is intended, so the 2.01x parallelism term is out of
scope.** That changes what "closing the gap" means, and it is worth stating the
arithmetic before anyone optimizes:

| term | value | status |
| --- | ---: | --- |
| wall gap | 3.80x | observed |
| **parallelism** (1.89x vs 0.94x) | **2.01x** | **intended — do not "fix"** |
| **CPU work** (1,788.8 vs 3,354.4 ms) | **1.88x** | **the target** |

At the current 0.94x CPU/wall, driving CPU work to v0.1.6's figure would give
`1,788.8 / 0.94 = ~1,903 ms` of wall against today's 3,585.5 ms — a **1.88x wall
improvement reachable without adding a single thread.** That is the prize, and it is
the number to hold the campaign to.

**v0.1.7 does strictly less work inside its timer and still spends 1.88x the CPU.**
v0.1.6's `layerstack_init_ns` **includes** scanning and chunking the 300 MB; v0.1.7's
**excludes** it, because the C2 supplied-object rule
(`core/benchmark/fs-bench-pro-storage-content/src/ops/c2.rs:1-22`) requires content to be
constructed before the timed phase. So the 1.88x of extra CPU is spent by a timer that
never touched construction. **That is the anomaly to explain.**

## 3. Where to look — ranked, with what is already known

Owner direction names the suspects: **database operations, chunking, scanning,
decoding.** Ranked by expected yield, with the facts already established:

### 3.1 SQL and database operations — first, because it is cheapest and most likely

`SaveOutcome` already publishes the instrumentation:

- `statements` — `INSERT`s issued for object rows, "the counter an INSERT-batching change moves"
- `presence_queries` — presence queries for offered objects' direct references
- `commits` = **17,378**, against v0.1.6's 73 admission transactions
- `packs_created`, `pack_appends`, `full_records`, `prefix_records`, `delta`, `chain`, `pool`

**Required first action: `EXPLAIN QUERY PLAN` on every statement the save path issues at
this scale.** Specifically the locator/presence queries and the pack-append rewrite.
A precedent already exists in this tree and should be **read before re-deriving
anything**: `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-rca-20260920T191016Z/`
found the dominant cost of a different lane was **a single SQL clause** — the query form
`ORDER BY o.object_id,o.save_id LIMIT ?` measured at **15.543 us/call** (line 110), and
`63fa15c49` ("the locator query's LIMIT costs 15.5 us a call, and #209's regression is
mostly that") is the fix. **The same class of defect may be here.** Report, per
statement: the plan, the call count, and ns/call.

`page_count`/`freelist_count`/`allocated_bytes` are already recorded in the receipt's
`resources.space`; `sqlite_t1_page_cache_overflow_peak_bytes` and
`sqlite_t1_connection_cache_used_bytes` are recorded per row and should be read before
any page-size treatment.

### 3.2 Commit cadence — the transaction term, which is large

17,378 commits against 73. The mechanism is already attributed for a different row:
`7075f338db36b209b59031e3e55ae11cf87eed57` ("replace the fixed two-save model with a
configured per-Store writer budget") made **every step commit before releasing the
arbitration lock** (`core/crates/layerfs-storage/src/cas/lifecycle.rs:166-175`), where
the retired model acknowledged once. **But do not assume this is the gap.** It is one
of four named suspects and it is the one with a known owner decision behind it, so it
may be intended. Measure its share before proposing a change to it.

### 3.3 Chunking — probably NOT the differentiator, and that is worth knowing

Object counts are near-identical (25,245 vs 25,158) and bytes within 0.33 %, so the
**chunk count and average chunk size already agree**: ~12.1 KB/object vs ~12.0 KB/object.
Chunk *policy* is therefore unlikely to be the cause. What is **not** established:
the chunk-size **distribution** (v0.1.6's fixture is deliberately skewed —
7,899 tiny of 1–8 bytes, 1,500 small of 32–256, 500 medium of 1,024–8,192, one 100 MB
anchor), and whether the same distribution is reproduced on the v0.1.7 side. §5 requires
that be settled before chunking is either blamed or cleared.

### 3.4 Scanning and decoding

The `SaveProfile` instrument (`core/crates/layerfs-storage/src/cas/owner.rs:44-76`)
splits the accept path into **seven disjoint buckets**, and reading it is the cheapest
evidence in the tree:

| bucket | covers |
| --- | --- |
| `resolve` | the disjoint parts of resolution (see `ResolveProfile`) |
| `full_ns` | FULL representation encode: tree-role and payload frames / pooled leaf body |
| `delta_ns` | delta representation encode: prefix frames, COPY/INSERT program construction |
| `group_ns` | group codec: group framing, pooled value-group compression |
| `place_ns` | pack placement: lane selection and the write it produces |
| `sql_ns` | object rows, pack bodies, value-group rows, content-signature flush, watermark |
| `commit_ns` | `COMMIT`, `ROLLBACK`, and the `BEGIN IMMEDIATE` that restarts a bounded transaction |

Its own doc records the important caveat: the caller's per-object work (presence
validation, group assembly outside the codec, `raw_payload`) is **outside all seven and
reported as the remainder**. `reuse_repeat` is a **count**, not an eighth bucket.

So decoding is `resolve` + `delta_ns` + `group_ns`; scanning of supplied objects is
`resolve`; SQL is `sql_ns`; cadence is `commit_ns`. **Read this split for the v0.1.7 row
before designing any treatment** — it may localize the 1.88x without a campaign. Note
the row's receipt did not publish it; the campaign's first act is to publish it.

## 4. Confounds to settle, in order

1. **Construction boundary** (§2). The two timers do not cover the same phases. A phase
   matrix — one row per phase (construction, open, build, handoff, accept, SQL,
   publication, commit, cleanup), one column per arm — is required before any
   like-for-like claim. Every cell sourced or `NOT_MEASURED`.
2. **Cache declarations differ and must not be pooled**: `reused-first-sample-uncontrolled`
   with `cache_contract: null` versus `prepared-dewarmed` with 0 resident pages.
3. **The forced 3-way split** (`MAXIMUM_WALK_ENTRIES = 4_096`,
   `core/crates/layerfs-content/src/filesystem/limits.rs:86`). Whether it costs anything
   is `NOT_MEASURED`. Note it also forced the *oracle* to be batched — a ceiling that
   applies to both paths doubles its cost.
4. **One sample per arm.** No spread is known for either arm; the machine shows
   16.7–26.0 s window-to-window variation on another lane.
5. **Memory.** v0.1.7's 524.8 MB peak against 72.2 MB is expected in kind (300 MB of
   supplied content held before the timer) but should be confirmed against
   `payload-create-500m`'s 527 MB for 500 MiB, and it must not become a
   file-size-proportional excuse (`AGENTS.md` §1).

**Not a confound, per owner direction:** the 0.94x CPU/wall. It is the intended design.

## 5. Subagent structure

One **coordinator** owns the worktree, serializes builds and measurements, and holds the
measurement lock. Each squad writes its own evidence directory with raw receipts beside
its report; a read-only reviewer re-derives the arithmetic from the raw files. Follow the
repository's existing evidence style
(`docs/roadmap/0.1/0.1.7/evidence/<topic>-<UTC timestamp>/README.md`).

| Squad | Owns | Must produce | Stop condition |
| --- | --- | --- | --- |
| **A. Profile** | §3.4 — publish `SaveProfile`'s seven buckets + remainder for the v0.1.7 row, and the phase matrix | The bucket split in ns and as a share, plus the phase matrix with every cell sourced or `NOT_MEASURED` | Every bucket and the remainder attributed, or a bucket the product does not report is named as unreported |
| **B. SQL** | §3.1 — `EXPLAIN QUERY PLAN` on every save-path statement at this scale; per-statement plan, call count, ns/call | A ranked statement table: plan, calls, ns/call, total share; and whether any plan is a scan the tree already knows how to avoid | Every statement the save issues is either in the table or declared unexamined, with the reason |
| **C. Transactions** | §3.2 — the cadence term, measured, not assumed | The ns share of `commit_ns` and the commit count's contribution, at fixed objects and bytes | A measured share, or a documented refutation that cadence matters |
| **D. Chunk/scan/decode** | §3.3 and §3.4 — chunk-size distribution equality between arms; scan and decode cost | Distribution comparison plus `resolve`/`delta_ns`/`group_ns` shares; state plainly if chunking is cleared | Either a mechanism or an explicit clearing of chunking as a cause |
| **E. Review** | Independent re-derivation from raw files | PASS / FAIL / INCOMPLETE per row; non-passing rows kept | Reviewer signs the arithmetic or records what could not be checked |

**Pre-registration rule.** Every treatment is registered in the squad's report **before
it runs**: one difference per arm, the identity it will be compared against, and the
expected movement in the instrument's own units. A treatment that was not pre-registered
is a diagnostic and must be labelled one.

## 6. Method rules that bind this work

- One sample per case per arm; fresh `--out`/`--output`; receipts are append-only and are
  never overwritten. **No best-of, no re-running until a number looks right.**
- **Single thread is the declared configuration.** Do not raise
  `LAYERFS_CONSTRUCTION_WORKERS`, add a second lane, or add helper threads to move a
  number — `AGENTS.md` §3.8. Optimization is inside one thread.
- **Do not raise worker counts, timeouts, cache sizes or buffer policies to move a
  number.** No shrinking the workload either.
- Pin identities: source commit/seal/tree, product, compilation and dependency seals,
  image, harness and workload hashes. The two arms are different workspaces and their
  seals differ by construction — say so rather than implying a matched build.
- Never retune, relabel or promote a historical receipt, including the v0.1.6 row and its
  `cache_contract: null`.
- Budgets: a performance selection's complete command is <= 15 s with a declared
  exception list to 25 s; verification <= 60 s. `pipeline-namespace-10000` currently
  measures 3,585.5 ms of operation and **4.84 s of declared wall** — it fits today, but a
  treatment that grows it past the limit must be recorded `NOT_RUN` with its wall time.
- No Store-format change without an explicit owner ruling; no new durability; no
  third-party patching; no WAL/fsync.
- This repository runs no CI and no aggregate gate. Report exactly which commands ran,
  which did not, and why.

## 7. Start order

1. **Squad A, `SaveProfile` only** (§3.4). It is already instrumented, needs no new code,
   and may localize the 1.88x outright.
2. **Squad B, `EXPLAIN QUERY PLAN`** (§3.1). The historical precedent in this tree says a
   single clause can dominate.
3. Squad A completes the phase matrix (§4.1).
4. Squads C and D, then E's review.

## 8. Definition of done

- The **1.88x CPU term** is attributed by bucket and by phase **with a mechanism**, or
  each candidate is recorded as refuted with the arm that refuted it, or the analysis is
  declared `INCOMPLETE` with the reason.
- Every cell of the phase matrix and the statement table is sourced or `NOT_MEASURED`.
- Chunking is either implicated with evidence or explicitly cleared (§3.3) — including
  the chunk-size distribution comparison.
- Any treatment is pre-registered and reports its movement in the instrument's own units,
  **with unchanged canonical results and unchanged refusal behaviour**.
- The 1.88x target in §2 is either met, or the residue is attributed and the achievable
  figure restated.
- #219 (and #218 where the store path is implicated) carries a status comment with the
  numbers and the gaps. **Nothing is closed on this handoff alone.**
