## #209 — the step's transaction, read with SQLite's own instruments: the commit path is closed, and the round withdraws

Round of 2026-09-21 from `b65d09a81`, ledger **L61**. Report:
[`docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-instr-20260920T213740Z/README.md`](https://github.com/Ephemeral-AI-Lab/layerfs/blob/codex/190-pooled-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-instr-20260920T213740Z/README.md).

**No product line is kept and no treatment is pre-registered.** The tree is byte-identical to `704580673` over `core/crates` and `crates`. This is a withdrawal with the evidence, which this lane counts as a result.

### What the instruments were

`SQLITE_TRACE_PROFILE` on the save's **own** connection (per statement: its runtime **and** that handle's live `sqlite3_stmt_status` counters), `sqlite3_db_status`/`sqlite3_status` around every `COMMIT` and every save, `EXPLAIN QUERY PLAN` plus the bytecode of all nineteen statements one step issues, and a micro-probe that prices the parts one statement cannot separate. Both are removed before the commit; sources are retained in the campaign directory.

One thing worth recording on its own: **SQLite's profile duration is `(sqlite3OsCurrentTimeInt64() - startTime) * 1000000` and that clock is milliseconds**, so the per-statement figures are a straddle estimator of total execution time, not per-call times.

### The four answers

- **Q1.** A step's transaction writes **27.839 pages per commit** on the product's connection: **24.433** the pack body it grew, **4.74** the structural pages the growth forces (page 1's change counter, the `objects` primary-key btree, the `objects_save` index, the free-list trunk, the pack's own leaf — ≈245,000 pages predicted against 229,788 measured). **Zero spills.** The file grows 976 pages while 4.18 GiB are written. **`COMMIT` executes 3 VDBE opcodes and costs 56.124 µs, so its price is the page writes: the commit path is closed for good.**
- **Q2.** **No.** `FULLSCAN_STEP`, `SORT`, `AUTOINDEX` and `REPREPARE` are **0 in all 21 buckets**, every plan is a seek, the `EXISTS` subquery is a coroutine evaluated once for the one matched row, and no plan needs `packs_save`/`objects_save`. The pack cache is not thrashing either: **97.33 % hit rate** (719,722 hits against 19,771 fetches over 227 packs).
- **Q3.** The second A B B A was **not run** — owner direction was to keep the round cheap, so it was started and stopped and is retained marked, consuming no sample. The answer is offered as mechanism instead: six full-step micro-probe arms all write 46.14 pages per commit, `commit_ns` is 99.8 % the `COMMIT` statement, and three arms of *identical code* span 3.5 µs by position alone while the parse arms sit 10.5–11.0 µs above them. **Reading: a time effect inside L59's window, not an arm effect.**
- **Q4.** `BEGIN IMMEDIATE` is **9.738 µs of every step and 0.473 s of the run**, 5 VDBE opcodes, and a deferred-`BEGIN` arm prices **≈3.8 µs of it as the eager RESERVED file lock**. Not removable: the watermark read that opens the step must happen under the write lock.

### What is left, and the owner decision this hands over

The only removable term in the step is **re-parsing and finalizing the pack `UPDATE` on every append (≈0.49 s)** — L59's mechanism, whose saving this round places in `sql_ns` and the uncharged remainder and **never in `commit_ns`** — plus **copying the body at bind (≈0.17 s)**, which rusqlite's hardcoded `SQLITE_TRANSIENT` makes unavailable without patching a dependency.

**The format's price is ≈3.07 s, not the ≈1.5 s in circulation**: 24.433 body pages per append at the measured page price is **2.13 s inside `commit_ns`** (73 % of the bucket) and the btree delete-and-insert of those pages is **0.95 s inside `append_pack`**. A chunked pack, a pre-allocated row or an incremental-blob write would end it and each moves the Store hash — **an owner's decision, recorded with its numbers and not shipped.**

### Custody

Two diagnostic rows, one sample each, fresh `--output`, both global flocks, Store **byte-identical in both** (`7ea2fe6ccf13bc5a…`) with **identical engine accounting to the page** and a **1.47×** difference in wall clock — the window problem this handoff opens with, shown on the same work. Both are diagnostics: admission `INELIGIBLE`, every budget class `NOT_RUN`. `multi_writer.rs` 5/5 and `pack_watermark.rs` 2/2; `check_product_boundary.py` PASS, `fmt --check` exit 0. The full workspace test/clippy runs and the harness suite were not re-run because no product line and no harness line changed; that is stated rather than implied.

Nothing here closes #190, #205, #208 or #209.
