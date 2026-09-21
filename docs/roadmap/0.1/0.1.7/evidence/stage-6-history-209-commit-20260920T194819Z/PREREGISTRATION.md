# Pre-registration — #209 commit path, written before the treatment existed

> Status: Research; diagnostic. Written after this round's separation diagnostics
> were sampled and **before** the treatment was written into the tree. It carries
> the diagnostics' numbers, not the treatment's.

## What the diagnostics measured first

Two diagnostics ran before any treatment existed. Both are diagnostics, not
samples of the operation; both are retained under [`scratch/`](scratch) with their
source in [`commit-cost-probe.rs.txt`](commit-cost-probe.rs.txt).

**D1 — does the declared connection profile move `COMMIT`? No.** The step's
statement shape (a whole pack body handed back through one `UPDATE`, plus the
`store_policy` watermark `UPDATE`) was replayed on a copy of the measured run's
own Store under seven profiles, **interleaved round-robin in one process**, six
rounds of 2,000 commits each:

| arm | pages written per commit | ns per page written |
| --- | ---: | ---: |
| declared (today's profile) | 15.30 | 1,897.5 |
| declared, repeated | 15.30 | 1,886.5 |
| `cache_size = -65536` | 15.30 | 1,882.2 |
| `cache_spill = 0` | 15.30 | 1,883.1 |
| `cache_size = -65536`, `cache_spill = 0` | 15.30 | 1,894.3 |
| `mmap_size = 268435456` | 15.30 | 1,864.5 |
| `journal_mode = OFF` (diagnostic only) | 15.30 | 1,873.0 |

**The page count is identical to two decimal places in every arm**, and the
per-page cost spans 1.8 % end to end — including the arm that removes the rollback
journal entirely and is not a candidate. Zero spills in every arm. Calibration on
the same volume: a bare 4 KiB `pwrite` costs 1.723 µs, a 4 KiB `memcpy` 0.130 µs.
So `COMMIT` **is** its dirty page set at the price of one `pwrite` each, and no
pragma the profile could declare changes either the count or the price.

**D2 — why the page count is what it is.** `sqlite3BtreeInsert` overwrites a row in
place only when `pCur->info.nPayload == pX->nData + pX->nZero` — SQLite's own
comment: *"New entry is the same size as the old. Do an overwrite."* A pack that
**grows** on every append therefore frees its overflow chain and writes a fresh
one: `pages written == ceil(pack body / 4096)`, exactly what D1 shows. The 4.18 GiB
of pack body the round before measured is 1,095,640 pages, and at 2.56 µs a page
that is the 2.809 s `COMMIT`. **The payload's pages are a property of the stored
format, not of the profile or the cadence.**

**D3 — what one per-step statement costs.** Same probe, five arms, each issuing the
same append and `COMMIT` and differing only in the one extra statement the step
issues (four rounds of 20,000 commits, interleaved):

| arm | step cost | pages per commit |
| --- | ---: | ---: |
| append only | 146.01 µs | 41.59 |
| + watermark `UPDATE`, value unchanged | +1.98 µs | 41.59 |
| + watermark `UPDATE`, value moved | +1.90 µs | 42.59 |
| + save ceiling `UPDATE`, value unchanged | +1.64 µs | 41.59 |
| + `SELECT next_pack_id` | +3.18 µs | 41.59 |

A policy `UPDATE` that re-asserts the value already in the row costs its statement
and **no page**; the same statement with a moved value costs one page. Separately,
statement preparation on this connection: `UPDATE store_policy …` 1,949 ns freshly
prepared against 84 ns cached, `UPDATE saves SET pack_ceiling …` 1,970 ns against
87 ns.

## What the shipped code does with that, per step

| per-step write | fires | value it writes | moves |
| --- | ---: | --- | ---: |
| `ownership::advance_pack` (`store_policy.next_pack_id`) | 48,446 | the save's in-memory next pack id | **255** |
| `placement::write_pack` (`saves.pack_ceiling`) | 46,049 | `MAX(pack_ceiling, pack_id)` | **255** |

`next_pack_id` advances only when `LanePlacement::select_many` starts a new pack,
and `self.next_pack_id` is re-read from the row at every `begin_write`. So on
48,191 of the 48,446 commits the watermark `UPDATE` writes back the value that is
already there, and on 45,794 of the 46,049 writes the ceiling `UPDATE` does the
same. The instrumented arm of the previous round priced the watermark statement at
**0.345 s over 48,446 calls**.

## The one treatment, pre-registered

**A step commits the policy state it changed, not the policy state it re-asserted.**

1. `cas/lifecycle.rs::maybe_commit` and `::finish_inner` call
   `ownership::advance_pack` **only when the save's pack id has moved** since the
   value was read at `begin_write`.
2. `cas/placement.rs::write_pack` issues the `saves.pack_ceiling` `UPDATE`
   **only on the write that creates the pack** (`write.created`), which is the only
   write whose `pack_id` can raise that row's ceiling.

Both are the same rule at the two places the per-step path breaks it. Nothing else
changes: the same `COMMIT` at the same point in the same transaction, the same
statements otherwise, the same counters.

## What the treatment must not change

- **The arbitration invariant.** The watermark must be correct at every step
  boundary a second writer can observe, not merely at publication. `begin_write`
  re-reads the row under the write lock and the treatment writes it back only when
  it moved, so the committed value after every step is exactly the value the
  shipped code leaves there. It is **not** deferred to publication: deferring it
  is what would let a second writer hand out a pack id this save already used.
- **Publication scoping, collision checking, failure cleanup, the memory bound.**
  Untouched.
- **The commit cadence and the step boundary.** Untouched; W > 1 is not traded.
- **Stored bytes.** Every workload counter and the saved Store must be identical.

## What it must move, stated in advance

| term | shipped | predicted | why |
| --- | ---: | ---: | --- |
| `commit_ns` | 3.154 s | **≤ 2.82 s** | 48,191 of 48,446 watermark statements removed; the previous round priced the statement at 0.345 s |
| stride10 operation | 26.467 s | **≤ 26.05 s** | the same 0.34 s, plus ~0.08 s of the 45,794 skipped ceiling statements (1.64 µs each, uncharged today) |

`commit_ns` per append: 65.1 µs → **≤ 58.2 µs**.

## The falsifier

- If the saved Store does not hash
  `7ea2fe6ccf13bc5aee7ba59bddef2d60d61d50d6b90329ee1488b1f096228358`, the treatment
  changed the operation rather than its cost: **withdrawn, start again**.
- If any of the 31 workload counters differs between arms: **withdrawn**.
- If `commit_ns` does not fall strictly below 3.154 s: the per-step policy write is
  not what the instrumented arm priced, the diagnosis is wrong, and the treatment
  is withdrawn.
- If the second writer is refused, or its measured step latency leaves the retained
  band (p99 ≈ 11 ms, worst observed 37.6 ms), or `multi_writer.rs` is not green:
  **the task has failed**, however fast the single-writer number is.
- If the operation is not strictly below 26.467 s: **failed**.

## How the pair is measured

Both arms come from **one binary**, selected by a measurement-only environment
lever that is removed before the round closes, exactly as the previous round's
`LAYERFS_STORAGE_COMMIT_EVERY` was:

- control: `LAYERFS_STORAGE_POLICY_REASSERT` unset — today's per-step re-assertion;
- treatment: `LAYERFS_STORAGE_POLICY_REASSERT=0` — the rule above.

The shipped tree then removes the lever and makes the treatment the default, and one
further sample from that shipped binary confirms the default reproduces the
treatment arm. One sample per arm, fresh `--output`, both global flocks, quiet
preflight, caps unchanged.
