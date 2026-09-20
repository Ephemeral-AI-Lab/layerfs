# Handoff prompt — #209 keep multi-writer, recover the speed: root cause first

> Status: Research; informative and not a product contract. Dated continuation
> checkpoint, 2026-09-20, after `a3da01604`. This prompt carries one measured
> regression and a bounded investigation; it is **not** a new measurement, not a
> design freeze, and not a release claim.

## Mission

Continue [#209](https://github.com/Ephemeral-AI-Lab/layerfs/issues/209). `eb319aaa9`
landed the multi-writer storage model, which buys concurrent writers by committing at
every locked step. On the **single-writer** path it costs **2.06×** on `history-stride10`
— **16.76 s** of operation, against the standing owner rule that **one second** of
stride10 reduction is worthwhile.

**The order is fixed: root cause analysis FIRST (§2), before any fix is designed or
written.** Do not start by changing the commit cadence. The measured buckets do not yet
agree on what the dominant cost is, and one of them is larger than the one the commit
message blames. Confirm the mechanism from measurement, then treat exactly one thing.

**The acceptance bar is not "restore 16.4 s".** It is: **multi-writer capability
retained, both writers measured, single-writer cost explained and minimised.** A fix that
recovers the speed by serialising writers has failed the task.

## 1. The measurement (do not re-derive)

| run | binary | operation | `accept_loop` | commits |
| --- | --- | ---: | ---: | ---: |
| `mw` (main `eb319aaa9`) | `eef60277083f541e` | **33.123 s** | **21.901 s** | **48,446** |
| `cp-explicit1` (previous main) | `418ee5085664a696` | 16.360 s | 8.949 s | 1,149 |

| bucket | new main | previous | ratio | absolute Δ |
| --- | ---: | ---: | ---: | ---: |
| `commit_ns` | 3.133 s | 0.437 s | 7.17× | +2.696 s |
| `resolve_ns` | 10.412 s | 4.304 s | 2.42× | **+6.108 s** |
| `sql_ns` | 1.953 s | 1.087 s | 1.80× | +0.866 s |
| `place_ns` | 0.327 s | 0.267 s | 1.23× | +0.060 s |
| `full_ns` | 1.723 s | 1.612 s | 1.07× | +0.111 s |
| `delta_ns` | 0.705 s | 0.649 s | 1.09× | +0.056 s |
| `group_ns` | 0.049 s | 0.044 s | 1.12× | +0.005 s |

**Work volume is unchanged:** `statements` 44,334 vs 44,331; `packs_created` 255 vs 255;
`pack_appends` 45,794 vs 45,791 — all 1.00×.

**The sharpest single number:** commits per pack append went **0.025 → 1.058**. The new
model commits once per append; the old committed once per ~40.

## 2. Root cause analysis — the deliverable of the first phase

### 2.1 The two competing explanations, and why they must be separated

The commit message blames the cadence: *"Commits now happen at every locked step… batching
stays inside a step."* The counters agree that cadence changed 42×.

**But `commit_ns` accounts for only +2.696 s of the +16.76 s regression. `resolve_ns`
accounts for +6.108 s — more than twice as much.** So "commits are expensive" is *not*
the whole story, and a fix aimed only at commit frequency would leave most of the
regression in place.

The second explanation, from `eb319aaa9`'s own merge notes: the catalogue query became
**publication-scoped**, and *"the publication-scoped query legitimately issues two SELECT
actions per preparation"*. A commit at every step invalidates prepared-statement reuse.
That is a **consequence** of the cadence rather than the cadence itself — which is why the
two must be separated before either is treated.

### 2.2 Questions the RCA must answer with evidence

1. **What is the commit actually tied to?** `maybe_commit` has four call sites —
   `lifecycle.rs:179` (candidate flush), `placement.rs:199` (group seal),
   `pool_lane.rs:122`, `pool_lane.rs:355`. Establish **which one fires ~2,850 times per
   state** (48,446 / 17) and whether that frequency is required by the arbitration
   contract or is an implementation choice.
2. **Is `resolve_ns` 2.42× a cadence consequence or an independent regression?** Test
   directly: if prepared-statement reuse is restored *without* changing commit frequency,
   does `resolve_ns` recover? If it does, the cadence is the single root and `resolve_ns`
   is a symptom. If it does not, there are **two** causes and one treatment will not fix
   both.
3. **What does the arbitration lock actually protect, and how long is it held?** Read
   `ownership::lock`, `advance_pack`, `publish` (`cas/lifecycle.rs`). State the exact
   invariant that forces a commit before the lock is released.
4. **Is the second writer's wait a real requirement or an assumption?** The code comment
   says *"the other writer must not have to wait for this one's whole upload"*. Quantify:
   how long would a second writer wait if the transaction spanned N appends instead of 1?
   **Measure it; do not assume it.**
5. **What is the per-commit fixed cost?** `commit_ns`/commits: 3.133/48,446 = **64.7 µs**
   new vs 0.437/1,149 = **380 µs** old. Each commit is ~6× cheaper and there are 42× more.
   Establish whether the 64.7 µs is SQLite `COMMIT` (fsync-class) or something avoidable
   per step.

### 2.3 What the RCA must produce before any fix

- The **dominant cause with its share of the 16.76 s**, stated in seconds.
- Whether `resolve_ns` is a symptom or a second root.
- The **step boundary's definition**, and whether it can be enlarged without violating the
  arbitration invariant — with the invariant quoted from the code.
- The **second writer's latency as a function of step size**, measured.
- An explicit statement of what remains unexplained.

## 3. The design tension to resolve

```text
  SQLite: one writer per store file
      │
      ├── long transaction  ──► second writer blocks (OwnershipUnavailable)
      │                         ...but batching amortises commit cost
      │
      └── commit per step   ──► second writer streams (W > 1 works)
                                ...but 42x more commits, and reuse dies
```

`cas/lifecycle.rs:138-155` states the contract: *"A write transaction therefore never
outlives the step that opened it under the arbitration lock, so every step commits before
that lock is released. Batching stays inside a step; it cannot span steps."*

**The lever is the size of a step, not the existence of the commit.** Enlarging a step
means holding the arbitration lock across more appends: fewer commits, better amortisation,
longer second-writer wait. That is a genuine trade with a measurable curve — find it.

### Candidate directions (do not implement any before §2 completes)

- **Batch more inside a step.** If a step is "one physical group", committing once per
  group rather than once per append may already be the design intent — check whether the
  1.058 ratio is a bug in *where* `maybe_commit` is called rather than a design decision.
- **Keep prepared statements across a commit.** If `resolve_ns` is a cadence symptom,
  this is the cheap fix and it does not touch the arbitration contract at all.
- **Coalesce the publication-scoped catalogue query.** Two SELECT actions per preparation
  is stated as legitimate; check whether they can be one round trip.
- **Adaptive step size.** Small steps under contention, large steps when a save is the
  only writer. Attractive but the most complex; require evidence before proposing it.

## 4. Measurement requirements

**Both writers must be measured.** The regression is single-writer; the capability is
multi-writer. A fix must report:

- **single-writer**: `history-stride10` operation + the seven buckets, before and after;
- **multi-writer**: the actual concurrent-writer path (`multi_writer.rs` tests exist),
  with second-writer latency and throughput — **the capability that was bought must be
  shown still bought**;
- **both axes** where space trades for time, per the standing ruling.

One sample per case per arm; fresh `--output` per run; receipts append-only; failures and
deferrals retained. Both global flocks for every resource command — a held lock defers.
Quiet preflight: no named `cargo`/`rustc`/`fs-bench` competitor and ≥ 70 % CPU idle on the
second of two one-second observations. **Watch for your own build's `cargo` outliving the
build** — it deferred two attempts in the round that found this.

Rust 1.85.1, `--locked`, `LAYERFS_CONSTRUCTION_WORKERS=1`, one construction worker. Caps
unchanged and not promotable: **120 s stride10 / 240 s stride3 / 720 s stride1**. Sample
all arms from **one binary** and record its sha256 — a cross-binary comparison produced a
false 33 % effect earlier in this lane and had to be withdrawn.

## 5. Checks required

Product change: `cargo +1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml --locked`,
`core/tools/check_product_boundary.py` and its self-tests. Harness change: the harness
suite (117 tests) and a release build. **No CI; `tools/preflight.sh` is permanently
retired and must not be used or restored.**

The new model's own tests must keep passing, in particular `multi_writer.rs`,
`memory_bounds.rs`, `visibility.rs` and `persistence_failure.rs` — a speed fix that
weakens publication scoping, collision checking or failure cleanup has failed.

## 6. Deliverables

1. **The RCA**, with the dominant cause in seconds and `resolve_ns` classified as symptom
   or second root.
2. **One treatment**, pre-registered from what the RCA names, measured against matched arms.
3. **Both-writer evidence**: single-writer buckets and multi-writer latency/throughput.
4. **An append-only ledger entry** (next free is **L54**) and an update on #209.
5. Production LOC for every commit, with core/reference subtotals.

## 7. Do not

- Do not fix before the RCA says which cause dominates — `resolve_ns` is larger than
  `commit_ns` and the commit message blames the smaller one.
- Do not recover single-writer speed by serialising writers; the capability is the point.
- Do not weaken publication scoping, collision checking, the ownership watermark or
  failure cleanup to win time.
- Do not treat one sample as a result, or compare across binaries.
- Do not close #190, #205, #208 or #209.
- Do not present any `history.*` row as admission evidence — all are diagnostics,
  admission `INELIGIBLE`, every budget class `NOT_RUN`.
