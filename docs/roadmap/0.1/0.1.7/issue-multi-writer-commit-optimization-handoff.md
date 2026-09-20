# Handoff prompt — #209: optimise `commit_ns`, and keep multi-writer while you do it

> Status: Research; informative and not a product contract. Dated continuation
> checkpoint, 2026-09-20, after `6e0d52606`. This prompt carries an attributed
> regression and a bounded investigation; it is **not** a new measurement, not a design
> freeze, and not a release claim.

## Mission

Continue [#209](https://github.com/Ephemeral-AI-Lab/layerfs/issues/209). The root cause of
the 2.06× single-writer regression is **found, fixed and closed**: the locator query's
`ORDER BY … LIMIT ?` cost 12.1 µs of every one of 380,380 calls, and removing it took the
stride10 operation from **33.116 s to 26.467 s** with the multi-writer cadence untouched.
That work is done and is not to be redone.

**What is left is the cadence's own price, and `commit_ns` is the largest single term in
it.** The task is to make stride10 faster **by attacking `commit_ns` and the per-step
transaction machinery** — without giving up W > 1.

| term, shipped arm | seconds | of the 10.107 s gap | owner of this task |
| --- | ---: | ---: | --- |
| `commit_ns` | **3.154** | **+2.717 s** | **this prompt** |
| `resolve_ns` (cadence's share of it) | 6.848 | ≈+0.82 s | this prompt, second |
| `sql_ns` + uncharged per-iteration work | 2.282 | +1.034 s | this prompt, third |
| `filesystem` | 6.788 | +3.349 s | **not this prompt** — separate round |
| everything else | — | +0.13 s | unexplained |

`commit_ns` is **3.154 s of a 26.467 s operation — 11.9 % of the whole thing**, and it is
the one term that is pure overhead: no byte of user data is stored by committing.

## 1. What is already known — do not re-derive

**The statement is not the problem; the sequence is.** 48,446 commits at **65.1 µs**
against 1,149 at 380 µs. Priced per unit of work the regression is **6.8×**: 380 µs per
~40-append transaction = **9.5 µs of commit per append** before, against **65.1 µs per
append** now. **Optimising the `COMMIT` statement alone cannot recover more than the
2.8 s it occupies.**

**Inside the bucket.** `write::commit` (the `COMMIT` statement) is **2.809 s (58.0 µs)**;
`ownership::advance_pack` — an `UPDATE store_policy` issued once per commit — is
**0.345 s**; the remainder is bookkeeping.

**With the cadence removed** the same source's `commit_ns` is **0.060 s** over 34
commits, and the whole operation is **17.950 s** against the held previous-model
**16.360 s**. That arm has no multi-writer capability; it is a bound, never a candidate.

**The step cannot be widened.** `LAYERFS_STORAGE_COMMIT_EVERY` = 8, 64 and 100000 all
**fail in round 0** with `CleanupFailed { original: OwnershipUnavailable, cleanup:
OwnershipUnavailable }`, because `busy_timeout` is zero by declared profile and the
refused save's cleanup then needs the lock it cannot get. N=1 passes 5/5 with zero
`OwnershipUnavailable`. **A widened step does not slow the second writer — it loses the
second writer's save.**

**The connection profile is a real lever and has never been tested.** `configure`
(`sqlite/connection.rs`) sets `journal_mode = MEMORY`, `synchronous = OFF`,
`temp_store = MEMORY`, `foreign_keys = ON`, `busy_timeout = 0`. It **never sets
`cache_size`, `mmap_size` or `cache_spill`** — the product only reads those back "for
evidence only". Measured on the run's own Store: **`cache_size` is the engine default
(`-2000`, 2 MiB)**, `mmap_size` is 0, `cache_spill` is enabled, and the Store is **49 MB
with 255 packs averaging 178 KB**. `COMMIT` cost tracks the **dirty page set**, and this
run writes **4.18 GiB** of pack body across 45,794 appends. **A 2 MiB page cache against
a 49 MB Store is the first hypothesis to test**, and it is a declared-profile change, not
a format or contract change.

## 2. Pre-register before you write anything

Pick **one** treatment from §3, state in advance what it must move and by how much, and
name the falsifier. Then measure. Do not implement two things and attribute the sum.

**Acceptance bar.** stride10 operation **strictly below 26.467 s** with `commit_ns`
**strictly below 3.154 s**, achieved **without** serialising writers. A change that
recovers time by making the second writer wait, fail, or be refused **has failed the
task**, however fast it is.

## 3. Candidate directions, in the order the numbers support them

1. **Right-size the page cache and stop the spill.** Set `cache_size` (and test
   `mmap_size` / `cache_spill`) in the declared profile so the dirty page set is not
   written out mid-transaction. This is the only direction that attacks the *bytes*
   `COMMIT` writes rather than the number of commits. It has a memory cost: read it from
   cgroup/`process_peak_rss` per the standing rules, keep `memory_bounds.rs` green, and
   report the space axis. **`cache_spill` deserves its own arm**: `COMMIT` is cheap if
   the dirty pages are still resident and expensive if they were flushed early.
2. **Move the watermark update out of the commit path.** `advance_pack` costs 0.345 s as
   one `UPDATE store_policy` per commit. The watermark only has to be correct at
   publication, so establish whether it can be advanced once per save, or once per pack
   allocation, without weakening the arbitration invariant (the pack id handed out must
   never collide with one another writer has used — check that against
   `sqlite/ownership.rs` and `cas/lifecycle.rs` before proposing anything).
3. **Make the per-step transaction cheaper rather than rarer.** The previous model's
   ~40-append transaction is what amortised the cost; the cadence forbids spanning
   steps, so the only remaining room is inside one step: fewer statements per step,
   fewer page touches per append, less journaled state.
4. **The other terms.** `resolve_ns` still carries ≈6.5 µs per locator call that is
   cadence-caused; `sql_ns` and the uncharged per-iteration work carry ≈1.03 s. Both are
   fair game **after** `commit_ns` moves, and the same measurement rules apply.

## 4. Measurement requirements

- **Both writers, every time.** single-writer: stride10 operation **and** the seven
  buckets, before and after; multi-writer: the actual concurrent path with
  **second-writer latency and throughput**, and `multi_writer.rs` green. The capability
  that was bought must be shown still bought.
- **One sample per case per arm; fresh `--output` per run; receipts append-only;
  failures and deferrals retained.** Both global flocks for every resource command — a
  held lock defers, it never waits.
- **Quiet preflight**: no named `cargo`/`rustc`/`fs-bench` competitor and ≥ 70 % CPU idle
  on the second of two one-second observations. **Watch for your own build's `cargo`
  outliving the build** — it deferred two attempts in the round that found this.
- Rust 1.85.1, `--locked`, `LAYERFS_CONSTRUCTION_WORKERS=1`, one construction worker.
  Caps unchanged and not promotable: **120 s stride10 / 240 s stride3 / 720 s stride1**.
  **Sample all arms from one binary and record its sha256** — a cross-binary comparison
  produced a false 33 % effect earlier in this lane and had to be withdrawn. **A harness
  change invalidates both arms of a pair.**
- **Byte-for-byte equivalence is the strongest evidence there is here.** All 31 workload
  counters must be identical between arms and the saved Store must hash identically
  (`7ea2fe6ccf13bc5aee7ba59bddef2d60d61d50d6b90329ee1488b1f096228358` for stride10 on
  this content). **A change that moves the Store is a different operation, not a faster
  one** — say so and start again.
- **Report space as well as time** wherever the two trade, per the standing ruling.

## 5. Checks required

Product change: `cargo +1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml --locked`,
`core/tools/check_product_boundary.py` and its self-tests. Harness change: the harness
suite (**117 tests**) and a release build. **No CI; `tools/preflight.sh` is permanently
retired and must not be used or restored.** `multi_writer.rs`, `memory_bounds.rs`,
`visibility.rs` and `persistence_failure.rs` must all stay green — a speed fix that
weakens publication scoping, collision checking, the ownership watermark, failure
cleanup, or the memory bound has failed.

## 6. Deliverables

1. **One treatment**, pre-registered from §3, measured against matched arms.
2. **`commit_ns` before and after**, in seconds and per append, with the same-binary
   sha256 for both arms and the Store hash for both.
3. **Both-writer evidence**: second-writer latency and throughput on the shipped step,
   not only single-writer buckets.
4. **An append-only ledger entry** (next free is **L57**) and an update on #209.
5. Production LOC for every commit, with core/reference subtotals.

## 7. Do not

- **Do not serialise writers, widen the step, or batch across steps.** Measured: the
  wider step *fails*, it does not wait. Recovering speed by refusing the second writer
  fails the task outright.
- Do not weaken publication scoping, collision checking, the ownership watermark, the
  memory bound or failure cleanup to win time.
- **Do not touch the locator query again.** It is fixed, verified byte-identical, and
  now cheaper than the previous model's was (3.488 µs against 4.559 µs).
- Do not treat one sample as a result, or compare across binaries, or re-run for a
  better number.
- Do not re-derive the root cause; §1 is measured and closed. Do not spend this round on
  `filesystem` (+3.35 s) — it is a separate, larger round with its own handoff.
- Do not close #190, #205, #208 or #209.
- Do not present any `history.*` row as admission evidence — all are diagnostics,
  admission `INELIGIBLE`, every budget class `NOT_RUN`.
