# Addendum — the `write_pack` attribution, and a correction of L54's ≈4.8 s

> Status: Research; **diagnostic evidence, not release admission**. Same round,
> same branch. This addendum **refutes the hypothesis §6 of [`README.md`](README.md)
> named** and **withdraws the ≈4.8 s figure it carried**.

## 1. The hypothesis, and its answer: 1.440 s, not several seconds

§6 named `cas::placement::MutationOwner::write_pack` as the largest uncharged
candidate — a whole-pack-body `UPDATE` on every append, charged to no bucket. It was
instrumented directly: the call, the pack body bytes handed to `INSERT`/`UPDATE`, the
cache eviction, the body statement, and the `UPDATE saves SET pack_ceiling` beside it.

Two matched arms, one sample each, fresh `--output`, both global flocks held, the same
binary per arm, `LAYERFS_STORAGE_PACK_PROBE=1` declared in `extra_environment`:

| arm | operation | pack writes | pack body bytes | evict | body | ceiling | **`write_pack` total** |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `packprobe` (shipped cadence + locator fix) | 23.445 s | 46,049 | 4,487,746,188 (4.18 GiB) | 0.003 s | 1.255 s | 0.182 s | **1.440 s** |
| `packprobe-nocommit` (same, step commits disabled) | 17.950 s | 46,049 | 4,487,746,188 (4.18 GiB) | 0.011 s | 0.940 s | 0.180 s | **1.131 s** |

**`write_pack` is 1.440 s — 34 % of the `sql_ns` bucket that already charges it, and
0.22 s of genuinely uncharged time. The hypothesis is refuted, and the 4.18 GiB of pack
body rewritten for a 49 MB Store is real but cheap** (`body` 1.255 s over 45,794
appends = 27 µs and 98 KB each; SQLite rewrites the BLOB faster than any account of
"8.1 GB of I/O" suggests, which is why arithmetic over counters was not allowed to
stand as a result).

## 2. The correction: §6's "≈4.8 s of uncharged accept growth" was wrong

§6 computed that figure by subtracting the **retained #205 instrumented arm's** bucket
sum (6.79 s) from the **new binary's** accept span. That compares two different
instruments across two different binaries and is not a measurement. **Withdrawn.**

The remainder can be read within one arm, and it is small:

| arm | `storage.accept_loop` | seven buckets | remainder, uncharged |
| --- | ---: | ---: | ---: |
| `instr0` | 22.255 s | 18.756 s | 3.499 s |
| `c1nocommit` | 14.552 s | 12.090 s | 2.463 s |
| `c2locator` | 16.263 s | 14.341 s | 1.922 s |
| `treatment2` | 16.875 s | 14.818 s | 2.057 s |
| `packprobe` | 14.696 s | 13.026 s | 1.670 s |
| `packprobe-nocommit` | 8.998 s | 8.148 s | **0.850 s** |

In the shipped-cadence arm the remainder is 1.670 s, of which **1.440 s is
`write_pack`** — so the accept path's genuinely unattributed work is **0.23 s**, not
4.8 s. The remainder is not even cadence-independent: it is 3.499 s in the shipped
cadence and 0.850 s with step commits disabled, so most of it is a *per-step* engine
cost the instrument does not name, and it disappears with the cadence that causes it.

## 3. What this leaves, and the arm that bounds it

With **both** effects removed — the locator `LIMIT` fixed *and* step commits disabled —
the operation reads **17.950 s** against the held previous-model figure of **16.360 s**:

| arm | operation | vs 16.360 s | vs shipped 33.116 s |
| --- | ---: | ---: | ---: |
| `instr0` — shipped model | 33.116 s | +16.756 s (2.02×) | — |
| `treatment2` — locator fix, shipped cadence | **26.467 s** | +10.107 s (1.62×) | **−6.649 s (−20.1 %)** |
| `packprobe-nocommit` — locator fix **and** no step commits | **17.950 s** | **+1.590 s (1.10×)** | **−15.166 s (−45.8 %)** |

So the two effects account for **15.17 s of the 16.76 s**, and what is left over is
**1.59 s (1.10×)**, not 10.1 s. **That is the real remaining regression.** The
`packprobe-nocommit` arm is an **experiment, never a candidate**: it has no
multi-writer capability at all, and the whole point of the model is that it does.

The reason the shipped arm still reads 26.467 s is therefore not an unexplained
residual — it is the cadence, which is a contract:

| cost | seconds | removable? |
| --- | ---: | --- |
| locator `LIMIT` | 6.65 (clean pair) / 7.49 (instrumented pair) | **removed** — this round's treatment |
| `commit_ns` | 3.154 s shipped against 0.060 s unbounded | no: the transaction must close before the lock is released |
| per-step engine work the buckets do not name | ~2.6 s (3.499 → 0.850 s) | no: it is a consequence of the same cadence |
| everything else | ~1.59 s | not characterised; the only unexplained part left |

**Nothing about this changes the treatment or the capability claim.** The locator fix
is still the one treatment, multi-writer is still retained and measured, and the
step is still not widen-able. What changes is the size of the remaining problem: it is
**1.59 s**, and the honest next question is what the last 1.59 s is, not what 10.1 s is.
