# #209 — `commit_ns`: the page cache is refuted, and the per-step policy write is removed

> Status: Research; **diagnostic evidence, not release admission**. Round of
> 2026-09-20/21 continuing [#209](https://github.com/Ephemeral-AI-Lab/layerfs/issues/209).
> Admission `INELIGIBLE`, every budget class `NOT_RUN`. Nothing here closes #190,
> #205, #208 or #209.

## The answer, in one paragraph

**The first hypothesis is refuted, and it is refuted by the engine's own page
accounting, not by a timing.** `COMMIT` writes exactly the pages the step dirtied,
one `pwrite` each, and **no pragma the declared profile could set changes either the
count or the price**: seven profiles — the declared one, `cache_size = 64 MiB`,
`cache_spill = 0`, both together, `mmap_size = 256 MiB`, and a contract-breaking
`journal_mode = OFF` diagnostic — all write **15.30 pages per commit** and all cost
**1.86–1.92 µs per page**, an 1.8 % spread end to end. The page count is what it is
because `sqlite3BtreeInsert` overwrites a row in place only when the new payload is
**the same size** as the old, and a pack grows on every append, so SQLite frees the
overflow chain and writes a fresh one: 4.18 GiB of pack body is 1,095,640 pages, and
at ~2.5 µs a page that *is* the 2.809 s `COMMIT`. **The payload's pages are the
stored format's price, not the cadence's and not the profile's.** What is left inside
the charged region is the per-step policy write, and the instrumented arm of the
previous round priced it at **0.345 s over 48,446 calls** for a value that moves
**255** times. The one treatment therefore removes the per-step policy writes whose
value the step did not change: the pack watermark is advanced only when it moves, and
the save's pack ceiling is written only by the append that creates a pack. Measured
on a matched pair sampled **from one binary** (sha256
`106f181b31e70808cc224c8e00a6dbf9e2b1b232002818eb7025dd0dfe5cd36d`), the saved Store
is **byte-identical** and all **582** workload counters are identical, while
`commit_ns` falls **1.884 → 1.773 s pooled (−0.111 s, −5.9 %)** and the operation
falls **16.754 → 16.517 s (−0.237 s)**. **The arms separate without overlap in
`commit_ns` — the term this round owns — in all seven rows measured**, and they do
not separate in the operation once the shipped binary's own row is included. The
round's honest bound is that, on this machine today, the effect is at the edge of a
single pair's resolution; §4 shows why, with the *same archived binary* that read
26.467 s in the previous session reading 19.908 s in this one.

## 1. Two diagnostics, before any treatment existed

Both are diagnostics, not samples of the operation. Source retained as
[`commit-cost-probe.rs.txt`](commit-cost-probe.rs.txt), outputs in
[`scratch/`](scratch). They replay the step's statement shape — a whole pack body
handed back through one `UPDATE`, plus the `store_policy` watermark `UPDATE` — on a
copy of the measured run's own Store, **interleaved round-robin in one process** so
machine drift is shared by every arm.

**D1 — the declared profile does not move `COMMIT`.** Six rounds of 2,000 commits
per arm:

| arm | pages written per commit | ns per page |
| --- | ---: | ---: |
| declared (today's profile) | 15.30 | 1,922.5 |
| declared, repeated | 15.30 | 1,889.9 |
| `cache_size = -65536` | 15.30 | 1,897.8 |
| `cache_spill = 0` | 15.30 | 1,889.9 |
| `cache_size = -65536`, `cache_spill = 0` | 15.30 | 1,896.2 |
| `mmap_size = 268435456` | 15.30 | 1,899.6 |
| `journal_mode = OFF` (diagnostic only, not a candidate) | 15.30 | 1,893.6 |

Zero spills in every arm. Calibration on the same volume: a bare 4 KiB `pwrite`
**1.723 µs**, a 4 KiB `pread` 0.886 µs, a 4 KiB `memcpy` 0.130 µs — so the per-page
cost *is* the write syscall. The diagnostic was run twice (the first run's figures
are the ones quoted in the [pre-registration](PREREGISTRATION.md); the second is
[`scratch/profile-arms.txt`](scratch/profile-arms.txt)) and the conclusion held in
both: identical page counts, spread under 2 %. A `mmap_size` arm that looked 23 %
faster in the first, sequential version of the probe did not survive interleaving —
it is drift, and it is recorded here because it was seen.

**D2 — why the page count is what it is.** `sqlite3BtreeInsert` (SQLite 3.51) takes
the in-place path only when `pCur->info.nPayload == pX->nData + pX->nZero`; its own
comment is *"New entry is the same size as the old. Do an overwrite."* A growing pack
never satisfies it, so every append reallocates the overflow chain and every page of
the new body is dirtied. `pages written == ceil(pack body / 4096)`, exactly as D1
measures. **A change that made this cheaper would change the stored bytes** — a
chunked pack, a pre-allocated row or an incremental-blob write all move the Store
hash — and per this lane's rule that is a different operation, not a faster one.

**D3 — what one per-step statement costs.** Five arms, four rounds of 20,000
commits, interleaved; each issues the same append and `COMMIT` and differs only in
the one extra statement:

| arm | step cost | pages per commit |
| --- | ---: | ---: |
| append only | 131.42 µs | 41.59 |
| + watermark `UPDATE`, value unchanged | +4.27 µs | **41.59** |
| + watermark `UPDATE`, value moved | +6.90 µs | **42.59** |
| + save ceiling `UPDATE`, value unchanged | +3.72 µs | **41.59** |
| + `SELECT next_pack_id` | +4.70 µs | 41.59 |

A policy `UPDATE` that re-asserts the value the row already holds costs its statement
and **no page** — SQLite's overwrite path compares content — and the same statement
with a moved value costs exactly one. Separately measured on the same connection:
preparing `UPDATE store_policy …` freshly costs **1,966 ns** against **84 ns** from
the statement cache, `UPDATE saves SET pack_ceiling …` **1,926 ns** against **86 ns**,
and `UPDATE object_packs SET data …` **4,723 ns** against **102 ns**. (The per-arm
step deltas are noisy across repeats — 1.6–4.3 µs for the unchanged watermark, 1.9–10.4
µs for the moved one — and no claim here rests on them; the page counts and the
preparation costs are stable in every run.)

## 2. The one treatment, pre-registered

**A step commits the policy state it changed, not the policy state it re-asserted.**

| per-step write | fires | value it writes | moves |
| --- | ---: | --- | ---: |
| `ownership::advance_pack` (`store_policy.next_pack_id`) | 48,446 | the save's in-memory next pack id | **255** |
| `placement::write_pack` (`saves.pack_ceiling`) | 46,049 | `MAX(pack_ceiling, pack_id)` | **255** |

`next_pack_id` moves only when `LanePlacement::select_many` starts a new pack, and
`begin_write` re-reads the row under the write lock, so **48,191 of 48,446** watermark
statements write back the value already there; the ceiling moves only on the write
that creates a pack, so **45,794 of 46,049** of those do the same. The treatment:

- `cas/lifecycle.rs::advance_pack_if_moved` — `maybe_commit` and `finish_inner` call
  `ownership::advance_pack` **only when the save's pack id has moved** since the value
  was read at `begin_write`;
- `cas/placement.rs::write_pack` — the `saves.pack_ceiling` `UPDATE` runs **only on
  the write that creates the pack**.

Nothing else changes: the same `COMMIT` at the same point in the same transaction,
the same counters, the same statements otherwise.

**The watermark is not deferred to publication.** It must be correct at every step
boundary a second writer can observe, and deferring it is exactly the change that
would let a second writer hand out a pack id this save already used. The treatment
writes the value the row already holds *only* when it is the value the row already
holds, so the committed state after every step is unchanged. Two new cases in
[`core/crates/layerfs-storage/tests/pack_watermark.rs`](../../../../../../core/crates/layerfs-storage/tests/pack_watermark.rs)
pin it from outside the crate — reading `store_policy` through an independent
connection between steps, and running two writers interleaved between steps — and
**both fail on a structurally deferred watermark** (`if reassert && false`): the first
on `watermark > highest`, the second on a pack-identifier collision. That red run is
retained in §7.

## 3. The measured pair, and the drift-cancelling diagnostic

One binary, sha256 `106f181b31e70808cc224c8e00a6dbf9e2b1b232002818eb7025dd0dfe5cd36d`,
arm selected by a measurement-only `LAYERFS_STORAGE_POLICY_REASSERT` lever (unset =
the behaviour the treatment replaces, `=0` = the treatment), declared in
`extra_environment` of every receipt and **removed before this commit**.

| row | arm | operation | `commit_ns` | per append | `resolve_ns` | `sql_ns` | `filesystem` |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| gate control (`control2`) | control | 17.105 s | 1.933 s | 39.9 µs | 4.460 s | 1.222 s | 4.498 s |
| gate treatment (`treat2`) | treatment | **16.499 s** | **1.768 s** | **36.5 µs** | 4.386 s | 1.170 s | 4.436 s |
| ABBA 1 a | control | 16.634 s | 1.847 s | 38.1 µs | 4.353 s | 1.167 s | 4.469 s |
| ABBA 2 b | treatment | 16.451 s | 1.746 s | 36.0 µs | 4.367 s | 1.161 s | 4.470 s |
| ABBA 3 b | treatment | 16.381 s | 1.749 s | 36.1 µs | 4.362 s | 1.149 s | 4.438 s |
| ABBA 4 a | control | 16.524 s | 1.872 s | 38.6 µs | 4.340 s | 1.145 s | 4.398 s |
| shipped (`shipped`) | treatment | 16.739 s | 1.829 s | 37.7 µs | — | — | — |

The gate pair is one sample per arm. The **A B B A** rows are a declared
**diagnostic** ([`abba_diagnostic.py`](abba_diagnostic.py)), run because §4 shows the
machine's between-sample level moving by more than the effect: ABBA cancels a linear
drift over the window, every row is retained, and no row was dropped or re-selected.

- **ABBA contrast**: operation **−0.163 s**, `commit_ns` **−0.112 s**; the drift the
  window actually had is −0.110 s operation and +0.025 s commit (4a − 1a).
- **Pooled (3 control rows against 4 treatment rows)**: operation **−0.237 s
  (−1.41 %)**, `commit_ns` **−0.111 s (−5.90 %)**, `resolve_ns` **+0.001 s**,
  `sql_ns` −0.012 s, `filesystem` +0.020 s — the treatment touches `commit_ns` and
  nothing else, which is what a treatment of this shape must look like.
- **`commit_ns` separates without overlap in all seven rows**: control
  {1.847, 1.872, 1.933} s against treatment {1.746, 1.749, 1.768, 1.829} s. The
  **operation does not** once the shipped row is included: control {16.524, 16.634,
  17.105} s against treatment {16.381, 16.451, 16.499, 16.739} s.
- The pre-registered prediction was `commit_ns` ≤ 2.82 s and operation ≤ 26.05 s,
  stated against the previous session's scale (3.154 s / 26.467 s). Rescaled to
  today's level (×0.60) it is −0.21 s / −0.25 s; measured, −0.111 s / −0.237 s. **The
  direction is confirmed and the size is between half and the whole of the rescaled
  prediction**, which one sample per arm cannot narrow further.

## 4. The machine level moved, and this is measured, not assumed

The acceptance bar is written against the held 26.467 s. **That number is not
reproducible on this machine today, and the shipped code is below it without the
treatment.** The archived binary of the previous round's shipped arm — sha256
`63ef5919ad5d11d1a09fe2b3bf54d179584af5a5a212a78be8f54e87f13cb597`, unchanged, run
again under the same protocol — reads:

| row | operation | `commit_ns` | `resolve_ns` | `filesystem` | corpus read |
| --- | ---: | ---: | ---: | ---: | ---: |
| previous session, 2026-09-20 | 26.467 s | 3.154 s | 6.848 s | 6.788 s | 17.497 s |
| **this session, same binary** | **19.908 s** | **2.254 s** | **5.395 s** | **5.151 s** | 16.271 s |

Every workload counter and the Store hash are identical between those two rows. So
the level moved by **6.6 s** with the binary held fixed, and two binaries with the
*same product behaviour* read 17.105 s and 19.908 s fifteen minutes apart. The corpus
residency probe reads **0 resident pages** in every row of this round against 5,141
in the previous session, so this round's rows are the *colder* ones and the direction
of the shift is not a warm-cache credit. Consequences, stated plainly:

- **The bar as a number is met by the treatment arm** (16.4–16.7 s < 26.467 s;
  1.75–1.83 s < 3.154 s) **and by the control arm too.** It therefore does not
  discriminate on today's machine, and it is not claimed as the round's evidence.
- **The round's evidence is the matched contrast of §3**, which is why the pair is
  sampled from one binary and why the ABBA diagnostic exists.
- The corpus read — outside the timed operation — drifts monotonically down across
  the ABBA window (14.27 → 13.06 → 12.90 → 12.86 s) with the arm assignment balanced
  in time, and the operation's own `content` and `filesystem` phases are unchanged
  between arms (+0.020 s), so the corpus drift does not carry into the contrast.

## 5. The second writer, measured on the shipped step

Instrument: [`step_latency_probe.rs.txt`](step_latency_probe.rs.txt) — the previous
round's probe adapted to select its arm from the same measurement-only lever, so both
arms run from one test binary. Retained source, removed from the product tree after
this round. Same test binary for both arms, 24 MiB
payload, 1,306 objects, three rounds, `solo` = one writer into a fresh Store, `pair` =
two writers into one fresh Store started together. A refusal would panic the probe;
none did.

| arm | round | solo mean / p99 / max | pair mean (a, b) | pair p99 (a, b) | ≥10 ms |
| --- | --- | --- | --- | --- | ---: |
| control | 0 | 101.9 / 2370.6 / 19422.8 µs | 245.5, 264.2 µs | 6701.7, 6704.7 µs | 0, 1 |
| control | 1 | 86.1 / 2060.5 / 14673.2 µs | 264.7, 240.1 µs | 6531.6, 6428.9 µs | 1, 0 |
| control | 2 | 84.6 / 2085.5 / 12547.2 µs | 299.5, 271.1 µs | 7104.7, 7084.7 µs | 5, 2 |
| treatment | 0 | 100.9 / 2722.8 / 5521.5 µs | 259.2, 262.9 µs | 7112.2, 7065.4 µs | 0, 0 |
| treatment | 1 | 79.1 / 2178.4 / 3181.4 µs | 249.0, 244.7 µs | 6832.2, 6778.7 µs | 0, 0 |
| treatment | 2 | 77.9 / 2119.5 / 2947.1 µs | 247.5, 252.5 µs | 6691.8, 6735.1 µs | 0, 0 |

**Both writers completed in every round, in both arms, with zero
`OwnershipUnavailable`**, and the treatment arm's tail is no worse than the control's
(p99 6.7–7.1 ms against 6.4–7.1 ms; worst observed 9.8 ms against 36.5 ms). The ~50
calls per round above 1 ms are the group seals — the same events that commit — and
they are 50 in every row, so the step the second writer can be locked out of is
unchanged in shape. `multi_writer.rs` is 5/5 and `pack_watermark.rs` 2/2 on the
shipped tree. **The capability that was bought is still bought.**

## 6. Equivalence and custody

- **The saved Store is byte-identical in all seven rows**:
  `7ea2fe6ccf13bc5aee7ba59bddef2d60d61d50d6b90329ee1488b1f096228358`, 51,867,648 bytes
  — the same constant the previous round recorded, and the same in the row produced
  by the *archived* previous-round binary. A change that moved the Store would have
  been a different operation and would have been withdrawn.
- **All 582 workload counters are identical across all seven rows** — the 38
  `delta.*` counters and the 544 per-state `save.*`/`inserted` counters, of which the
  previous round compared 31: `commits` 48,446, `pack_appends` 45,794,
  `packs_created` 255, `statements` 44,334, `reused` 1,121, `inserted` 52,032,
  `full_records` 12,952, `prefix_records` 39,080, `chain.objects` 107,628,
  `pool.leaves` 1,738 and every derived counter. The only keys that differ are
  environmental (`store.path`, `heap.peak_incremental_bytes` by ±2 bytes,
  `space.allocated_bytes`, corpus residency and disk-read bytes).
- **Every arm is one sample, fresh `--output`, both global flocks held for every
  resource command** (`with_locks.py`), quiet preflight per `collect.py`. Four
  preflight deferrals were written and retained across the round
  (`runs/*/deferred-*.json`), caused by a busy machine: CPU idle 58.15 % / 61.59 % /
  72.38 % / 74.50 % at load 10.5 / 5.5 / 6.2 / 7.7, and in two of them another
  worktree's `cargo` was the named competitor. Each was retried into a fresh
  `--output`; a deferral consumes no sample and is never a row.
- Caps unchanged and not promoted (120 s stride10), `LAYERFS_CONSTRUCTION_WORKERS=1`,
  Rust 1.85.1, `--locked`, corpus manifest
  `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271` in every row,
  harness source seal `04bcfab573ab40544981ad4358883cad97af7a8eae0a46e2c095285b40d301e0`
  in every row. Every binary is archived under [`binary-archive/`](binary-archive)
  with its sha256. No cell dropped, no case re-run for a better number, no
  cross-binary effect size quoted.
- **Interference, declared**: two long-running `grep` processes at ~99 % CPU were
  present through the whole round (≈2 of the machine's cores), and desktop activity
  (Spotlight/`duetexpertd`) was present intermittently. They are shared by every row
  rather than charged to an arm; the level shift of §4 is the observable they are
  suspected of, and it is reported as unidentified rather than attributed.

## 7. Checks as run

- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` — exit 0.
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace` —
  **537 passed, 0 failed** (535 before this round plus the 2 new `pack_watermark`
  cases); `multi_writer` 5/5, `memory_bounds` 10/10, `visibility` 9/9,
  `persistence_failure` 8/8.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace
  --all-targets -- -D warnings` — exit 0.
- `core/tools/check_product_boundary.py` — PASS, 175 production Rust/SQL files;
  `core/tools/test_check_product_boundary.py` — 6/6 OK.
- Harness: `cargo +1.85.1 test --manifest-path
  core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked` — **117 passed,
  0 failed**; release build exit 0. The harness source is **unchanged** this round
  (one pre-existing `unused_mut` warning at `ops/history.rs:1762` is retained).
- **Structural red run, retained**: with the watermark advance replaced by
  `if reassert && false`, both new `pack_watermark` cases fail —
  `a_step_never_leaves_the_watermark_behind_a_pack_it_wrote` at
  `pack_watermark.rs:52` and
  `two_writers_interleaved_between_steps_never_share_a_pack_identifier` at
  `pack_watermark.rs:93` (the second on a pack-identifier collision inside
  `accept`). The condition was restored and both pass.
- **Not run:** verification mode. Every row here is a diagnostic, admission
  `INELIGIBLE`, every budget class `NOT_RUN`. No CI; `tools/preflight.sh` is
  permanently retired and was not used.

## 8. What this round does not do

- **It does not attribute `resolve_ns`'s cadence share (≈0.82 s) or the `sql_ns` +
  uncharged term (≈1.03 s).** Both remain, and the diagnostic that names the next
  one is D3: the hot per-step statements are prepared **fresh on every call** —
  `UPDATE object_packs SET data …` 4,723 ns against 102 ns cached, 46,049 calls a
  run — which is a 0.2–0.4 s term inside `sql_ns` and the uncharged work, and it is a
  *separate* treatment from this one.
- **It does not touch `filesystem`** (+3.35 s in the previous session's accounting),
  which is a separate round.
- **It does not touch the locator query**, the step size, the cadence, or the
  arbitration invariants.

## 9. Addendum — the per-append multiple, and the regression, re-measured in one window

[`previous-model-window.md`](previous-model-window.md) re-measures the **previous**
model against this one, back to back in a single window, because the previous-model
figure this round compared against was from a different session. The previous model's
binary was never archived; it was rebuilt from `f039bcaf2`, validated by saving the
byte-identical retained Store constant `4af37932…`, and both sides carry the same
harness seal. Balanced over the two orders (`prev → shipped → shipped → prev`), four
rows:

| | previous | shipped | multiple |
| --- | ---: | ---: | ---: |
| operation | 10.977 s | 16.598 s | **1.51×** |
| `commit_ns` | 0.235 s | 1.806 s | **7.67×** |
| per append | 5.14 µs | 39.43 µs | **7.67×** |

That corrects two numbers in circulation: the per-append commit multiple is **~7.7×,
not ~4×** (the 4× divided this window's numerator by the 08:10 window's denominator;
the 08:10 same-session pair gives 7.17×, so the ratio is window-stable while the
absolute prices are not), and `gap-attribution.md` §3's **6.8× is a units mismatch**
— it divided `commit_ns` per *commit* by a per-*append* figure; per append it is 7.2×.
It also records that the **operation-level regression is 1.51× in this window against
the 2.02× the #205 pair measured in its own**: both models got faster, by different
factors, so the commit multiple held and the operation multiple did not.

## 10. Reproduce

```sh
# build the measured binary (the lever is gone from the shipped tree)
cargo +1.85.1 build --release --locked \
  --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml

# one sample per case per arm, both global flocks, fresh --output
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-commit-20260920T194819Z/with_locks.py \
  shipped-stride10 \
  python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-commit-20260920T194819Z/collect.py \
    shipped history-stride10 \
    --binary core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content \
    --cwd "$PWD" --seal-repo "$PWD" --pre-execute

# every derived number in this report
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-commit-20260920T194819Z/analyze.py
```

The matched pair and the ABBA diagnostic were sampled from the lever binary
(`binary-archive/lever-pair`, sha256 `106f181b…`), the treatment arm adding
`--env LAYERFS_STORAGE_POLICY_REASSERT=0`; the driver is
[`abba_diagnostic.py`](abba_diagnostic.py). The shipped confirmation row is
`binary-archive/shipped-treatment` (sha256 `3a6c1c20…`). The diagnostics need a copy
of a measured run's own Store, taken from the previous round:
`runs/treatment2-history-stride10/raw/sample.sqlite` of
[`stage-6-history-209-rca-20260920T191016Z`](../stage-6-history-209-rca-20260920T191016Z).
