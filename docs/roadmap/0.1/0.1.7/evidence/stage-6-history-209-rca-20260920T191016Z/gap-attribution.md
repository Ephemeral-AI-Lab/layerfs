# Where the 10.107 s is — the stride10 operation against the held previous-model row

> Status: Research; **diagnostic evidence, not release admission**. Arithmetic over the
> receipts in this directory plus the retained #205 row. Same round as
> [`README.md`](README.md); this file supersedes §6 of it and the attribution L55
> bounded.

## 1. The two rows

Both columns are **seven disjoint buckets billed by the same instrument**, so the
comparison is like for like.

| term | previous model | shipped now | Δ | source |
| --- | ---: | ---: | ---: | --- |
| operation | 16.360 s | **26.467 s** | **+10.107 s** | retained `cp-explicit1` / `treatment2` |
| `accept_loop` | 8.949 s | 16.875 s | +7.926 s | retained timing / `treatment2` timing |
| `filesystem` | 3.439 s | 6.788 s | +3.349 s | retained timing / `treatment2` timing |
| `commit_ns` | 0.437 s | 3.154 s | **+2.717 s** | bucket |
| `resolve_ns` | 4.857 s | 6.848 s | **+1.991 s** | bucket |
| `sql_ns` | 1.087 s | 1.995 s | **+0.908 s** | bucket |
| `full_ns` | 1.612 s | 1.732 s | +0.121 s | bucket |
| `place_ns` | 0.267 s | 0.338 s | +0.071 s | bucket |
| `delta_ns` | 0.649 s | 0.700 s | +0.051 s | bucket |
| `group_ns` | 0.044 s | 0.051 s | +0.007 s | bucket |
| uncharged in accept (accept span − seven buckets) | 2.161 s | 2.287 s | **+0.126 s** | derived; the previous model's bucket sum is the retained row's |

**The Δ values reconcile to the millisecond.** accept span (+7.926) + filesystem
(+3.349) − the accept path's own unnamed growth (+0.126) = **+11.149 s**; the other
spans outside `accept_loop` (`content`, `storage.begin`, `storage.finish`,
`harness.*`, `store.create`) moved by **−1.042 s**; 11.149 − 1.042 = **+10.107 s**. The
seven bucket Δ values sum to +5.871 s, and 7.926 − 5.871 = 2.055 s, which is exactly the
unnamed growth inside the accept span plus the two spans outside `accept_loop`
(0.126 + 1.929).

## 2. The uncharged term, now measured rather than hypothesised

`write_pack` was the largest uncharged candidate. It is **1.440 s** over 46,049 calls
writing 4,487,746,188 bytes (4.18 GiB) of pack body; 1.22 s of it was already inside
`sql_ns`, so the genuinely uncharged part is **0.22 s**. The arm's whole remainder is
1.670 s, so **0.23 s of the accept path is attributed to nothing at all** — and it is
cadence-driven, not stable (3.499 s shipped-cadence in the instrumented baseline,
0.850 s with step commits disabled). See [`pack-write-addendum.md`](pack-write-addendum.md).

## 3. The three big terms, and why each is there

**`commit_ns` +2.717 s — the sequence, not the statement.** 48,446 commits at 65.1 µs,
against 1,149 at 380 µs. The statement got cheaper; the sequence got 42× longer. Priced
per *unit of work* rather than per commit, the regression is 6.8×: 380 µs / ~40 appends
= **9.5 µs of commit per append** before, against **65.1 µs per append** now. Inside the
bucket, `write::commit` alone is 2.809 s (58.0 µs) and the `ownership::advance_pack`
UPDATE beside it is 0.345 s.

**`resolve_ns` +1.991 s — one query, two multipliers.** 380,380 locator calls. The
`LIMIT` clause is **+12.1 µs per call** (isolated: 15.543 µs against 3.488 µs on the same
Store and the same plan); the cadence is **+6.5 µs per call** (28.32 µs shipped against
21.78 µs with step commits disabled). The first is removed, worth 6.65 s on the clean
pair; the second is cadence.

**`sql_ns` +0.908 s plus the uncharged per-iteration work — same bytes, 42× the
transactions.** Object-row inserts, pack-body rewrites and the value-group rows all
happen once per seal now instead of accumulating inside one long transaction.

## 4. The one sentence that produces all three

The model **kept every byte of work and removed the amortisation**. One transaction used
to cover ~40 appends: rows batched, a page dirtied 40 times written once, one `COMMIT`
retiring the set. Now each append carries its own transaction, so a page dirtied 40
times can be written up to 40 times and the open/commit/close sequence is paid 42× as
often.

## 5. Where the numbers say to look next, in order

1. **`commit_ns` 3.154 s, of which 2.809 s is the `COMMIT` statement.** The connection
   profile is `journal_mode = MEMORY`, `synchronous = OFF`, `busy_timeout = 0`, and
   **`cache_size` is the engine default (`-2000` = 2 MiB)** while the Store is 49 MB and
   `PRAGMA cache_spill` is enabled. `COMMIT` cost tracks the dirty page set, so a page
   cache that small is the first thing to test — the product already reads
   `cache_size`, `mmap_size` and `cache_spill` back "for evidence only" and never sets
   them, so this is a declared-profile question, not a contract change. Peak RSS in all
   five arms is 250–257 MB, and `memory_bounds.rs` is the test that guards the ceiling.
2. **`advance_pack` inside the same bucket, 0.345 s**, one UPDATE on `store_policy` per
   commit for a watermark that only needs to move when a pack is allocated.
3. **`filesystem` +3.349 s**, the largest single unexamined term and outside this RCA.
4. **The last 1.59 s**, bounded by the `packprobe-nocommit` arm (17.950 s against the
   held 16.360 s) — an experiment with no multi-writer capability, never a candidate.
