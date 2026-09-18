# P2-1 receipt — the 32 MiB / spilling-OFF profile: measured and declined

> **Status:** Item receipt, **no product commit**. The item was implemented in a
> scratch tree, measured against the frozen set, and **declined**: the measurement
> shows the change moves no instrument this phase has, while it costs 16× the page
> cache per connection. Under the handoff's five states this is a completed item.
> Tree: the parent is `0a593084c` (`../p2-2/after/`); the candidate was built at
> `/tmp/p21-profile` from the same tree with exactly the patch in
> [`attempt/candidate.patch`](attempt/candidate.patch).

## 1. What was tried

Ruling 3 authorises the reference profile: `cache_size = -32768` (32 MiB) and
`cache_spill = OFF` on the product connection, verified by read-back like the
existing pragmas. The candidate applies both in `sqlite/connection.rs::configure`
and fails the open if either does not take — the shape the plan asked for.

## 2. The A/B, with the profile confirmed by V7's accessor

| Row | counter | parent (`p2-2/after`) | candidate | verdict |
| --- | --- | --- | --- | --- |
| **D28** `c2.ceiling` | `cache_size` / `cache_spill` (read on the save's own connection) | 2,000 / 20,000 | **-32,768 / 0** | the profile changed ✔ |
| **D28** | `inserted` / `reused` / `packs_created` / `pack_appends` / `commits` / `statements` / `presence_queries` / all pool counters / `canonical_bytes` | 8,191 / 0 / 33 / 8,169 / 31 / 72 / 0 / … | **identical** | no work moves |
| **D29** `c2.small` | the same set | 1,023 / 0 / 5 / 1,020 / 4 / 9 / 0 | **identical** | no work moves |
| **D21–D24** `edits.pipeline.*` (the read-path half, measured first per ruling 3) | `save:` line, `readback bytes`, `readback group decodes`, `readback connection opens` | 3/2/1/1 | **identical** | no work moves |

Logs: [`attempt/after-D28-c2-ceiling.log`](attempt/after-D28-c2-ceiling.log),
[`attempt/after-D29-c2-small.log`](attempt/after-D29-c2-small.log),
[`attempt/after-pipeline-*.log`](attempt/). The parent values are the `p2-2` arm's
own logs.

## 3. Why it is declined

1. **No instrument moves.** Every product-level work counter on the write path and
   the read path is identical; only `elapsed_ns` differs, and that is
   diagnostic-grade (`CONTRACT.md` §2.4: +17.6% spread on one binary and input).
   `AGENTS.md` states the rule this runs into: "A configuration or statement-count
   difference is not an effect."
2. **The premise was already measured false.** P0-2 proved `SQLITE_DBSTATUS_CACHE_SPILL`
   reads **0** at 8,191 rows and at 4× that, under both the 2 MiB/ON and 32 MiB/OFF
   profiles, with a live control (1,032 spills at a forced 8-page cache). The write
   path does not spill, so `cache_spill = OFF` removes nothing.
3. **The observable that would show a page-cache effect cannot be read.** SQLite's
   own page-cache reads and spills live behind `sqlite3_db_status`, and V7 recorded
   that the product cannot call it (`rusqlite` has no safe binding; the crate's
   audited-`unsafe` boundary allows one FFI module). So the item's effect class is
   not merely unmeasured here — it is unmeasurable with this product's instruments.
4. **It is not free.** 32 MiB of page cache is claimed per connection that takes the
   profile, against 2 MiB today, for a benefit this tree cannot demonstrate.

The plan allowed exactly this outcome ("may end measured-and-declined"), and ruling
3 authorises the profile without requiring it: if the owner wants the reference's
configuration on its own authority, the patch is in
[`attempt/candidate.patch`](attempt/candidate.patch) and applies cleanly to
`0a593084c`.

## 4. What is left on the product today

The declared profile stays as `05-storage.md` and `11-optimization-study.md` §16.6
describe it (no `cache_size`, no `cache_spill`, `mmap_size` unset), and V7's
`SaveOperation::connection_profile()` reports it on the save's own connection —
2,000 pages and spilling on at the threshold, on this toolchain.

## 5. Checks

No product source changed, so no check was run for this item; the parent commit's
eight checks and its 469-test run stand, and the round that carries this receipt
runs its own eight checks (they cover the evidence-only change).

## 6. UNVERIFIED

* **Wall-clock behaviour of the candidate was not measured** beyond the diagnostic
  `elapsed_ns` fields in the logs above; no timing claim is made.
* **Read-heavy workloads with a working set above 2 MiB** are not in the frozen set,
  so a shape where a 32 MiB cache *would* change page-read counts is not exercised —
  the decline is about this tree's instruments and this workload set, and the
  receipt says which, rather than claiming the profile can never matter.
* **The per-connection memory figure is the declared pragma**, not a measured
  resident set (`SQLITE_DBSTATUS_CACHE_USED` is FFI-only, as V7 recorded).
