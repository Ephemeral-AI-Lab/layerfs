# #209 — the root cause of the multi-writer speed regression, and the one treatment it named

> Status: Research; **diagnostic evidence, not release admission**. Round of
> 2026-09-20 continuing [#209](https://github.com/Ephemeral-AI-Lab/layerfs/issues/209),
> which was split out of [#205](https://github.com/Ephemeral-AI-Lab/layerfs/issues/205).
> Admission `INELIGIBLE`, every budget class `NOT_RUN`. Nothing here closes #190,
> #205, #208 or #209.

## The answer, in one paragraph

The 16.76 s regression is **two costs, and the commit cadence is not the larger one**.
The dominant cost is a **single SQL clause**: the publication-scoped locator query gained
`ORDER BY o.object_id,o.save_id LIMIT ?`, and on SQLite that form costs **15.5 µs per
call against 3.5 µs for the same query without it** — same Store, same join, same
visibility predicate, same query plan. It is paid **380,380 times** in one stride10 run
and it is paid *inside* the step, so no commit cadence can avoid it. Removing it — the
one treatment, pre-registered in [`PREREGISTRATION.md`](PREREGISTRATION.md) — recovers
**6.65 s of the 16.76 s** on the matched clean-arm pair (33.116 → 26.467 s) and **7.49 s**
on the strictly matched instrumented pair (33.116 → 25.630 s). The second cost is the
cadence: eliminating every step commit recovers **7.59 s**. The two are not additive —
they are the *same* 380,380 queries, slowed by two independent mechanisms, and the
locator fix removes the larger of the two per query. `resolve_ns` is a **symptom**, not a
second root: it is 10.411 s of a 22.255 s accept span, and **10.771 s of engine-measured
locator time sits inside it**.

**Multi-writer capability is retained and measured.** The shipped model still commits at
every step; the second writer still streams (three concurrent rounds over the 24 MiB pair plus five
over the step probe, zero `OwnershipUnavailable` in any of them), and the step it can be blocked by is **p50 0.4 µs / p99 ~11 ms /
worst observed 37.6 ms**. The one arm that tried to widen that step did not make the
second writer wait longer — it made the second writer *fail*.

## 1. Which `maybe_commit` call site fires, and why

48,446 commits over 17 states, and they are not spread evenly: the counts run 280,
863, 1,056, 1,144, 2,438 … 5,456. They track `pack_appends` (45,794, all 17 states
summed), not the state count.

| call site | fires per | total this run |
| --- | --- | ---: |
| `cas/placement.rs:199` — the group seal, one per pack append | 1 per pack append | **45,794** |
| `cas/pool_lane.rs:122` — a fresh-ordinal reservation, one per leaf | 1 per pooled leaf | **1,738** |
| `cas/pool_lane.rs:355` — a value-group placement, one per leaf batch | 1 per `write_value_groups` | **~880** |
| `cas/lifecycle.rs:179` — `flush_candidates` | see below | **~34** |
| `cas/lifecycle.rs` — `finish_inner` (the publication commit) | 1 per save | **17** |

`flush_candidates` is called once per wave and its own comment says the save's
transaction is "normally still open here", so it almost never opens one; the 34
`transactions`-class commits in the no-commit arm are the ordinal reservations and
the publications that survived. **So the cadence is not 42× per *state*; it is one
commit per *pack append***, and it is 94.5 % of all commits.

That frequency is a **design requirement, not an implementation choice**, and the
requirement is stated in the code (`cas/lifecycle.rs::maybe_commit`):

> "Multi-writer: SQLite admits one writer per store file, and the other writer must
> not have to wait for this one's whole upload. A write transaction therefore never
> outlives the step that opened it under the arbitration lock, so every step commits
> before that lock is released. Batching stays inside a step; it cannot span steps."

and in `cas/lifecycle.rs` §"Save ownership outlives short transactions":

> "A write transaction therefore never outlives the step that opened it under the
> arbitration lock, so every step commits before that lock is released. Batching
> stays inside a step; it cannot span steps."

The measured consequence (§5) is that violating it does not make the second writer
slower, it makes the second writer **fail**.

## 2. Per-commit fixed cost: what the 64.7 µs buys

Measured on the instrumented shipped arm, `write::commit` is charged separately from
the `advance_pack` UPDATE beside it:

| | commits | `commit_ns` (bucket) | `COMMIT` alone | `BEGIN IMMEDIATE` | boundary statements |
| --- | ---: | ---: | ---: | ---: | ---: |
| shipped model | 48,446 | 3.015 s | **2.809 s (58.0 µs)** | 0.433 s (8.9 µs) | 97,028 |
| step commits disabled | 34 | 0.065 s | 0.066 s | 0.0003 s | **68** |

So 96 % of the bucket is the `COMMIT` itself — the engine writing the transaction's
dirty pages and discarding the journal. The declared connection profile is
`journal_mode = MEMORY`, `synchronous = OFF`, `busy_timeout = 0`, so there is no fsync
in it; what it writes is the dirty page set, and the dirty page set is large because
every pack append rewrites a whole pack body (this run's own Store: 255 packs,
**45,298,203 bytes, 177,640 bytes average, 262,112 maximum** — about 8.1 GB of pack
BLOB rewritten across 45,794 appends). That is also the mechanism the round's largest
unattributed residual lives in (§6).

## 3. Is `resolve_ns` a symptom or a second root? — a symptom. Two independent arms say so.

Two arms, one difference each, all stride10, one sample per case per arm, fresh
`--output` per run, both global flocks held:

| arm | what it changes | operation | `commit_ns` | `resolve_ns` | locator queries | per locator |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `instr0` | nothing (shipped `eb319aaa9` + RCA counters) | **33.116 s** | 3.015 s | **10.411 s** | 380,380 | **28.32 µs** |
| `c1nocommit` | step commits disabled, nothing else | **25.523 s** | 0.065 s | **7.987 s** | 380,380 | **21.78 µs** |
| `c2locator` | locator `ORDER BY`/`LIMIT` removed, commits untouched | **25.630 s** | 3.039 s | **6.691 s** | 380,380 | **10.00 µs** |
| `treatment2` | `c2locator` without the RCA counters (shipped) | **26.467 s** | 3.154 s | 6.848 s | — | — |

The first arm's engine counter is decisive: **`lookup_ns` = 10.771 s of the 10.411 s
`resolve_ns` bucket** — the bucket *is* the locator query. And the query got slower
for a reason that has nothing to do with committing:

```
previous model   SELECT … FROM objects WHERE object_id IN (?) AND pack_id <= ?        4.559 us/call
new model        SELECT … FROM objects o JOIN saves s USING(save_id),
                 temp.layerfs_read_scope r WHERE … (publication scope, no ORDER/LIMIT) 3.488 us/call
new model + ORDER BY o.object_id,o.save_id LIMIT ?                                   15.543 us/call
new model + ORDER BY only                                                            3.485 us/call
new model + LIMIT only                                                              13.840 us/call
```

30,000 calls each, against this run's own `sample.sqlite`, same pragma profile, one
read scope row. **The publication scoping is not the cost — it is 1.07× *faster* than
the previous model's query.** The `LIMIT` is the cost, it is 4.5× the whole query, and
`EXPLAIN QUERY PLAN` shows the same plan with and without it (`SEARCH o USING PRIMARY
KEY (object_id=?)`, `SEARCH s USING INTEGER PRIMARY KEY (rowid=?)`, `SCAN r`).

`resolve.eligible_ns` (3.584 → 1.892 s) and `resolve.acquire_ns` (5.491 → 3.567 s) are
the two disjoint parts that call it, and they fall together with it. **`resolve_ns` is
a symptom of the locator query, and the locator query was made slow by the `LIMIT`
added to guard a duplicate-locator case that this run never has** — 380,380 queries
returned 388,897 rows, and the eligibility check's own bound is the product's
`SAVE_SLOTS` (2). Removing the clause changes no scoping, no collision check and no
stored byte.

## 4. The step boundary: what it is, and that it cannot be enlarged

The boundary is **one acquisition of the arbitration mutex plus everything that
happens inside it**: the group seal (`placement.rs::seal_group`), the ordinal
reservation and value-group placement (`pool_lane.rs`), the candidate flush
(`lifecycle.rs::flush_candidates`) and the publication (`lifecycle.rs::finish_inner`).
The commit is the last statement of the step, and the guard that makes the next step
serial is released immediately after it. The invariant, quoted from the code, is:

> "A write transaction therefore never outlives the step that opened it under the
> arbitration lock, so every step commits before that lock is released. Batching
> stays inside a step; it cannot span steps."

(`cas/lifecycle.rs`, the doc comment on `maybe_commit`.)

**Enlarging the step is not a trade with a curve. It is a cliff.** A
measurement-only lever (`LAYERFS_STORAGE_COMMIT_EVERY`, default 1, removed after this
round) widened the step from one step per commit to N steps per commit — exactly
"hold the arbitration lock across more appends":

```
COMMIT_EVERY=1        both writers finish, zero OwnershipUnavailable, 5/5 rounds
COMMIT_EVERY=8        FAILED in round 0
COMMIT_EVERY=64       FAILED in round 0
COMMIT_EVERY=100000   FAILED in round 0

panicked: called `Result::unwrap()` on an `Err` value:
CleanupFailed { original: OwnershipUnavailable, cleanup: OwnershipUnavailable }
```

`busy_timeout` is **zero** by declared profile, so a writer arriving while another's
transaction is open is refused immediately; the refused save's own cleanup then needs
the lock it cannot get, and it fails as a cleanup failure. **The second writer does not
wait longer — it loses the save.** That closes the design tension the handoff framed:
there is no step size at which batching is amortised *and* W > 1 still works, so the
lever is not the size of the step. The treatment has to be inside it.

## 5. The second writer's latency, measured on the shipped step

Instrument: [`step_latency_probe.rs.txt`](step_latency_probe.rs.txt), retained source,
removed from the product tree after this round. Same binary, same 24 MiB payload, two
writers into one fresh Store, each reporting the wall time of its own `accept` calls.

| arm | calls | mean | p50 | p90 | p99 | max | ≥1 ms | ≥10 ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| solo (one writer) | 1,307 | 120–134 µs | 0.0 µs | 0.2–0.4 µs | 3.5–4.0 ms | 4.4–16.1 ms | 50 | 0–1 |
| pair, writer A (two writers) | 1,307 | 400–413 µs | 0.0 µs | 0.3–0.5 µs | 10.6–11.2 ms | 17.1–37.6 ms | 50 | 26–32 |
| pair, writer B (two writers) | 1,307 | 381–410 µs | 0.0 µs | 0.3–0.4 µs | 10.4–11.1 ms | 12.5–33.2 ms | 50 | 24–33 |

1,290 of 1,306 calls are sub-millisecond; the ~50 that are not are the group seals —
the same events that commit. The step a second writer can be locked out of is
therefore **p50 0.4 µs, p99 ≈ 11 ms, worst observed 37.6 ms**, and the writer that
owns no lock at that moment pays it as its own latency (`solo` mean 120–134 µs against
`pair` 400–413 µs for the same calls). **Both writers finished in every round and no
round produced a single `OwnershipUnavailable`.** Throughput is therefore the
operation's own: the two writers contend for the same SQLite write lock by design, and
what the model buys is that neither is refused.

## 6. What remains unexplained — stated, not worked around

The treatment recovers **6.65 s of the 16.76 s** on the matched clean-arm pair
(33.116 -> 26.467 s) and **7.49 s** on the strictly matched instrumented pair
(33.116 -> 25.630 s). **10.11 s remains** against the held previous-model figure of
16.360 s. It is not one thing, and this round can price only part of it:

| term | measured | how |
| --- | ---: | --- |
| `commit_ns` growth | **+2.72 s** | 0.437 s (previous model, retained) -> 3.154 s, same 48,446 commits |
| `resolve_ns`, `full_ns`, `sql_ns`, `place_ns`, `commit_ns` — all seven buckets together | **+4.09 s** | 6.79 s of buckets in the retained #205 instrumented arm -> 14.818 s |
| `filesystem` | **+3.35 s** | 3.439 s -> 6.788 s |
| **unnamed accept** — the increase inside `storage.accept_loop` that the seven buckets do not charge | **+4.8 s** | accept grew 8.949 -> 16.875 s (**+7.93 s**); 3.94 s of that is inside the buckets and 0.26 s is `commit_ns`+`sql_ns`, which leaves ≈4.8 s uncharged. The arm's own remainder is 3.499 s of a 22.255 s span, against 2.057 s of a 16.875 s span |

The three terms do not sum to 10.11 s and must not be added: `commit_ns` sits inside
the bucket row, and both `filesystem` and the unnamed remainder are measured against
the retained arm rather than against `instr0`.

**The largest uncharged candidate, named as a hypothesis and not as a result**, is
`cas/placement.rs::write_pack`. Every pack append UPDATEd `object_packs` with the whole
pack body (45,794 times in this run) and the call is charged to no bucket. This run's
own Store holds 255 packs totalling **45,298,203 bytes** (average 177,640, maximum
262,112), so the appends rewrite on the order of 8.1 GB of BLOB for a 49 MB Store. That
is arithmetic over the retained counters, **not a time measurement**: the round that
tests it has to charge `write_pack` and separate the BLOB update from the pack-directory
read. It is also the most likely mechanism behind the `commit_ns` growth above, since
the dirty page set a `COMMIT` writes is that same body.

`filesystem` is outside this RCA's scope. It is the tree-metadata term the earlier
rounds attributed to `validate`/`directories`, it is 39 % of the treatment arm's
operation, and no bucket here covers it. The regression the handoff measured is an
*operation* regression, so a complete account of all 16.76 s has to include it; this
round did not measure it and does not attribute it.

Finally, **`full_ns` and `sql_ns` moved without the workload moving.** `full_ns` reads
2.436 s in `instr0`, 1.691 s in `c2locator` and 1.732 s in `treatment2`, while
`full_records`, `prepared_full` and every other counter are identical in all three;
`sql_ns` reads 1.864 / 1.889 / 1.995 s. The movement is confined to the two arms that
share a build, so it is not attributable to the treatment; it is recorded as
between-sample variance in the class L53 reported for this machine, and **no number
here rests on it.**

## 7. Equivalence and custody

* **All 31 workload counters are identical in all four arms** — `commits` 48,446,
  `statements` 44,334, `pack_appends` 45,794, `packs_created` 255, `reused` 1,121,
  `inserted` 0, `full_records` 12,952, `prefix_records` 39,080, `chain.objects`
  107,628, `pool.leaves` 1,738, and every `save.*` counter derived from them.
* **The saved Store is byte-identical** between the untreatment arm and both treatment
  arms: `7ea2fe6ccf13bc5aee7ba59bddef2d60d61d50d6b90329ee1488b1f096228358`. It is *not*
  the retained campaign constant `4af37932…`, which the `#205` round already recorded
  as expected for a model that moved the watermark and the publication scoping — the
  same-store comparison that matters here is within this round, and it holds.
* **The no-commit arm is different on both**, which is what a model change looks like:
  Store `64de588056101d65…`. It is an experiment, never a candidate.
* **Every arm is one sample, fresh `--output`, one binary per arm with its sha256
  recorded**, both global flocks held for every resource command. All arms were
  sampled from this source tree; no cross-binary comparison is used for any effect
  size. One preflight deferral was written and retained
  (`runs/treatment-history-stride10/deferred-*.json`, 58.88 % idle); the sample was
  taken on retry into a new `--output`, as the protocol requires.
* The Store digests, the corpus manifest and the case caps are the retained #190/#205
  diagnostic protocol unchanged. Caps not promoted, not enlarged, no re-run for a
  better number, no cell dropped.

## 8. Checks as run

* `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` — exit 0.
* `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace` — **535
  passed, 0 failed**; `multi_writer` 5/5, `memory_bounds` 10/10, `visibility` 9/9,
  `persistence_failure` 8/8, `catalogue_statement_reuse` 1/1.
* `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace
  --all-targets -- -D warnings` — exit 0.
* `core/tools/check_product_boundary.py` — PASS, 175 production Rust/SQL files;
  `core/tools/test_check_product_boundary.py` — 6/6 OK.
* Harness: `cargo +1.85.1 test --manifest-path
  core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked` — **117 passed,
  0 failed** (14 binaries); release build exit 0.
* **Not run:** verification mode. Every row here is a diagnostic, admission
  `INELIGIBLE`, every budget class `NOT_RUN`. No CI, and `tools/preflight.sh` is
  permanently retired and was not used.

## 9. Reproduce

```sh
# build the measured binary
cargo +1.85.1 build --release --locked \
  --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml

# one sample per case per arm, both global flocks, fresh --output
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-rca-20260920T191016Z/with_locks.py \
  treatment-stride10 \
  python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-rca-20260920T191016Z/collect.py \
    treatment history-stride10 \
    --binary core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content \
    --cwd "$PWD" --seal-repo "$PWD" --pre-execute
```

The three RCA arms need the instrumented build and the two source variants; their
archived binaries are in `binary-archive/` with their sha256s in
[`results.txt`](results.txt), and their per-state raw traces are under `runs/`.
