# Handoff prompt — #209: root-cause the commit-time defect with SQLite's own diagnostics

> Status: Research; informative and not a product contract. Dated continuation
> checkpoint, 2026-09-21, after `b65d09a81`. This prompt carries closed findings, one kept
> treatment, one withdrawn treatment and a bounded investigation; it is **not** a new
> measurement, not a design freeze, and not a release claim.

## Mission

Continue [#209](https://github.com/Ephemeral-AI-Lab/layerfs/issues/209). **The commit-time
defect is the largest term in the remaining regression and it has never been diagnosed
with SQLite's own instruments** — every measurement so far has been taken *around* the
step (buckets, wall clock, timing probes on a copy of the Store), never *inside* it. The
task is to root-cause what one step's transaction actually costs the engine, prove it with
`EXPLAIN QUERY PLAN`, `sqlite3_stmt_status`, `sqlite3_db_status` and a per-statement charge
split, and only then pre-register **one** treatment — **without giving up W > 1**.

The committed state to start from is **`b65d09a81`**. Its product tree is byte-identical to
`704580673` (the kept treatment) and to `f3e84c073` — verify with `git diff --stat 704580673
HEAD -- core/crates crates`, which must print nothing. The round after the kept one was
measured and **withdrawn** (it did not move the wall clock) and is not in the tree. Read
these before touching anything:

- [`issue-multi-writer-commit-optimization-handoff.md`](issue-multi-writer-commit-optimization-handoff.md)
  — the previous prompt, now largely closed;
- [`evidence/stage-6-history-209-commit-20260920T194819Z/README.md`](evidence/stage-6-history-209-commit-20260920T194819Z/README.md)
  — the profile refutation and the per-step policy-write treatment (kept);
- [`evidence/stage-6-history-209-stmtcache-20260920T210202Z/README.md`](evidence/stage-6-history-209-stmtcache-20260920T210202Z/README.md)
  — the withdrawn statement-cache round, its diagnostics and its unexplained
  redistribution;
- [`evidence/stage-6-history-209-confirm-20260920T212625Z/README.md`](evidence/stage-6-history-209-confirm-20260920T212625Z/README.md)
  — the confirmation window: the kept change reproduces on `commit_ns` (−5.0 % against
  −5.9 % in its own window) and still does not resolve on the operation;
- ledger **L57**, **L58**, **L59**, **L60** in
  [`0.1.6/evidence/issue151-experiment-ledger.md`](0.1.6/evidence/issue151-experiment-ledger.md).

## 1. What is already known — do not re-derive

**Where it stands today.** The three most recent windows, same code, same corpus, one
sample per arm each: **16.5 s / `commit_ns` 1.81 s** (the kept treatment's own window),
**22.5 s / 2.49 s** (the withdrawn round's window), **17.7 s / 2.05 s** (the confirmation
window, whose treatment arm read **1.95 s**). **The machine's level moves by more than 30 %
between windows with the work held constant** — an archived, unchanged binary read 26.467 s
in its own session and 19.908 s in another — so any bar must be re-derived in-window and no
cross-window difference is an effect. What *is* stable across the two windows that measured
it is the kept change's bucket contrast: **−5.9 % and −5.0 % on `commit_ns`**, arms
non-overlapping in both, against an operation contrast of −0.24 s and −0.04 s that is inside
the window drift in both. **The kept change is kept for the statements it provably removes,
not for a wall-clock claim.**

**`commit_ns` is the pack body's pages at the write syscall's price.**
`SQLITE_DBSTATUS_CACHE_WRITE` equals the payload's page count **exactly** in every arm
measured (15.30 pages/commit in the probe), with **zero spills**. 4.18 GiB of pack body is
1,095,642 pages, and at the measured 1.65–1.72 µs per 4 KiB `pwrite` that is the whole
bucket. `sqlite3BtreeInsert` overwrites a row in place **only when the new payload is the
same size as the old** (*"New entry is the same size as the old. Do an overwrite."*), and a
pack grows on every append, so every append reallocates the overflow chain: measured
**+17.8 µs per append for growth against a fixed body**, independent of the statement cache.

**The declared profile is exhausted.** Seven profiles — declared, `cache_size = 64 MiB`,
`cache_spill = 0`, both, `mmap_size = 256 MiB`, and a contract-breaking `journal_mode = OFF`
diagnostic — all write **exactly the same number of pages** at a per-page cost spanning
**1.8 %** end to end. Rollback-mode mmap is read-only
(`sqlite3PagerWrite` asserts the page is not `PGHDR_MMAP`), and `pager_write_pagelist`
writes one page per `sqlite3OsWrite`. **Do not re-test the profile.**

**The cadence cannot be widened.** `COMMIT_EVERY` = 8, 64 and 100000 all **fail in round
0** with `CleanupFailed { original: OwnershipUnavailable, cleanup: OwnershipUnavailable }`
— `busy_timeout = 0` by declared profile and the refused save's cleanup needs the lock it
cannot get. The second writer does not wait; it loses the save.

**The per-append multiple is 7.7×, measured in one window.** Previous model **5.14 µs** per
append against this model **39.43 µs**, balanced over two orders, Store byte-identical on
each side (`4af37932…` / `7ea2fe6c…`); the same ratio in the earlier same-session pair was
7.17×. That is the amortisation the cadence gives up (1,149 transactions over 45,791
appends before, 48,446 now), not a defect in the `COMMIT` statement.

**Two per-step policy writes are already gone** (kept, L57): the watermark is advanced only
when it moved and the save's pack ceiling is written only by the append that creates a pack
— 48,191 of 48,446 and 45,794 of 46,049 statements removed, Store byte-identical.

**One treatment is withdrawn** (L59): preparing the six per-step texts once instead of per
call moved `sql_ns` by **−0.275 s** exactly as predicted and the operation by **−0.024 s**,
because `commit_ns` rose **+0.087 s**. Why removing a parse from the pack `UPDATE` adds time
to the commit that follows it **is not established**; the probe says a cached statement is
4,575 ns *cheaper* than a fresh one on the same Store with the product's parameter shape.

**The prepared-statement cache is not a lever.** 99.0 % of the 380,444 locator calls are
one identifier wide and 99.6 % of the 44,334 object-insert calls are, so the hot entries stay
resident in the 16-entry LRU; text construction and cache thrash are both refuted as
explanations for the ≈6.5 µs per locator call that separates the in-run cost (10.00 µs) from
the isolated one (3.488 µs). **That 6.5 µs × 380,444 ≈ 2.5 s is still unexplained** and it is
`resolve_ns`'s, not `commit_ns`'s — a later round's target unless your diagnostics say
otherwise.

## 2. The RCA this round owes — instrument the step, do not time around it

Build the instruments below **before** choosing a treatment, and keep every one of them
behind a measurement-only switch declared in `extra_environment`, removed before the final
commit (the RCA counters of L54 and the probes of L57/L59 are the precedent). Record the
exact SQL and the exact call site with every number.

1. **`EXPLAIN QUERY PLAN`, and the bytecode, for every statement one step issues**, against
   the run's own Store with the declared profile: `BEGIN IMMEDIATE`; `SELECT next_pack_id
   FROM store_policy WHERE id = 1`; `UPDATE object_packs SET data = ?2 WHERE pack_id = ?1 AND
   save_id = (SELECT save_id FROM temp.layerfs_read_scope) AND EXISTS (SELECT 1 FROM saves
   WHERE saves.save_id = object_packs.save_id AND publication IS NULL)`; `INSERT INTO
   object_packs …`; the object `INSERT … VALUES (?,?,?,?,?,?,(SELECT save_id FROM
   temp.layerfs_read_scope))` chunk; `UPDATE saves SET pack_ceiling …`; `UPDATE store_policy
   SET next_pack_id …`; `COMMIT`. **State for each whether it seeks or scans, whether the
   `EXISTS` subquery is a coroutine per row, whether the `save_id` subqueries are evaluated
   per row, and whether the plan uses the `packs_save` / `objects_save` indexes.**
   `EXPLAIN`'s opcode listing matters as much as the plan: `OP_SeekGE`/`OP_Rewind`/
   `OP_Next`/`OP_Column` counts per statement.
2. **`sqlite3_stmt_status` after a real run**, per statement: `SQLITE_STMTSTATUS_FULLSCAN_STEP`
   (a full scan hidden inside a step would be invisible in every bucket so far), `SORT`,
   `AUTOINDEX`, `VM_STEP`, `RUN`, `REPREPARE`. `REPREPARE` also decides whether the
   statement cache is doing anything.
3. **`sqlite3_db_status` on the *product's own* connection**, per save and per step:
   `CACHE_WRITE` (pages written), `CACHE_SPILL`, `CACHE_HIT`/`CACHE_MISS`,
   `CACHE_USED`, `STMT_USED`. **This has never been read for the product's connection** —
   the "15.30 pages per commit" figure comes from a harness-owned replay of the same
   statement shape. If a real run writes materially more than `ceil(pack body / 4096)`
   pages, the excess is a target; if it writes exactly that, `COMMIT` is closed for good.
4. **`sqlite3_status(SQLITE_STATUS_PAGECACHE_* )` and the file's own `page_count` /
   `freelist_count`** across a run: is the append reusing freed pages or growing the file,
   and does the freelist churn show up as extra dirty pages?
5. **A per-statement charge split** inside the step: today `commit_ns` is one region around
   `advance_pack` + `COMMIT` and `sql_ns` is one region around the pack write plus the object
   insert. Split them per statement (a temporary `SaveProfile` field, or a harness-visible
   counter) so the next treatment can name the statement it removes and prove where the time
   went. This is what L59 could not resolve.
6. **Re-measure the `BEGIN IMMEDIATE` / `COMMIT` boundary on their own**, without the parse
   (the previous session priced `BEGIN` at 8.9 µs per call, 0.433 s over the run). Say what
   the locking and journal open/close actually cost on this machine, and how much of it is
   the advisory lock the arbitration design deliberately takes per step.

**The questions the round must answer, in writing, with evidence:**

- Q1. What does one step's transaction write, page for page, on the product's connection?
- Q2. Does any statement in the step do more work than its rows and indexes require?
- Q3. Why did removing a parse from the pack `UPDATE` add 0.087 s to `commit_ns` (L59)?
  A second A B B A window on the reverted tree, with the split of (5), should settle whether
  it is an arm effect or a window effect.
- Q4. What is `BEGIN IMMEDIATE` per step made of, and is any of it removable without holding
  a transaction across steps?

## 3. Candidate directions, in the order the numbers support them

1. **Whatever §2 finds.** A hidden full scan, a per-row `EXISTS` coroutine, a plan that
   ignores an index or a `REPREPARE` on every call are all larger than anything left in the
   profile, and none of them has been looked for.
2. **The uncharged per-step work**: `BEGIN`/`COMMIT` lock and journal transitions, the
   page-cache bookkeeping between statements, and whatever the split in §2.5 attributes to
   no bucket. In L57's treatment this was the term that absorbed a bucket saving.
3. **`resolve_ns`'s ≈6.5 µs per locator call (≈2.5 s)**: same instrument set (`EXPLAIN`,
   `stmt_status`, `db_status`), different statement. Cheaper to reach than it looks now that
   the cache hypotheses are dead.
4. **The format** — chunked packs, a pre-allocated row or incremental blob writes end the
   whole-chain rewrite and are worth **≈1.5 s of `commit_ns`** by the growth penalty
   measured in L59 (17.8 µs per append). **This moves the Store hash and is therefore an
   owner decision, not an agent's**: propose it with numbers, do not ship it as an
   optimisation.

## 4. Measurement requirements

- **Both writers, every time**: stride10 operation and the seven buckets before and after;
  the concurrent path with **second-writer latency and throughput**; `multi_writer.rs`
  green. A change that recovers time by making the second writer wait, fail or be refused
  **has failed the task**, however fast it is.
- **One sample per case per arm; fresh `--output` per run; receipts append-only; failures,
  deferrals and diagnostics retained.** Both global flocks for every resource command — a
  held lock defers, it never waits.
- **Every absolute bar must be derived in the same window as the sample.** The level moved
  16.5 s → 22.5 s for identical work between two consecutive rounds. Compare arms, not
  sessions; if a figure from another window is needed, re-measure it in this one (L58's
  rebuilt previous-model binary and its Store constant are the precedent).
- **Both arms from one binary wherever the treatment allows**, with the arm behind a
  declared measurement-only lever, plus an **A B B A** diagnostic when the effect is smaller
  than the window's drift. Record every binary's sha256 and archive it.
- **Byte-for-byte equivalence is the strongest evidence there is here.** All 582 workload
  counters (38 `delta.*` plus 544 per-state) must be identical between arms and the saved
  Store must hash `7ea2fe6ccf13bc5aee7ba59bddef2d60d61d50d6b90329ee1488b1f096228358` for
  stride10 on this content. **A change that moves the Store is a different operation** —
  say so and start again. Report space as well as time wherever the two trade.
- Rust 1.85.1, `--locked`, `LAYERFS_CONSTRUCTION_WORKERS=1`. Caps unchanged and not
  promotable: **120 s stride10 / 240 s stride3 / 720 s stride1**. Watch for another
  worktree's `cargo` outliving its build — the preflight defers on it, and two deferrals in
  one round are cheap.
- **Run the diagnostics on the recon worktree**, whose 878 MB target directory and rebuilt
  previous-model binary are already in place at
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-205-prev` if a previous-model row is needed again.

## 5. Checks required

Product change: `cargo +1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml --locked`,
`core/tools/check_product_boundary.py` and its self-tests. Harness change: the harness suite
(**117 tests**) and a release build. **No CI; `tools/preflight.sh` is permanently retired and
must not be used or restored.** `multi_writer.rs`, `memory_bounds.rs`, `visibility.rs` and
`persistence_failure.rs` must all stay green — a speed fix that weakens publication scoping,
collision checking, the ownership watermark, failure cleanup or the memory bound has failed.
Any new case that pins an invariant (as `tests/pack_watermark.rs` does for the watermark)
must be shown to **fail** on a structurally broken build before it is claimed as a guard.

## 6. Deliverables

1. **The RCA**, with the instruments of §2 retained as sources (`*.rs.txt`) and their raw
   outputs, including the exact SQL, the plans, the opcode counts and the statement statuses.
2. **One treatment**, pre-registered from §3 with its prediction, its falsifier and its
   per-statement attribution — or **a withdrawal with the same evidence**, which this lane
   counts as a result.
3. **`commit_ns` (and whichever term the treatment targets) before and after**, in seconds
   and per append, with the binary sha256 for both arms and the Store hash for both.
4. **Both-writer evidence**: second-writer latency and throughput on the shipped step.
5. **An append-only ledger entry** (next free is **L61**) and an update on #209.
6. Production LOC for every commit, with core/reference subtotals.

## 7. Do not

- **Do not re-derive §1.** The profile is refuted, the cadence is closed, the payload's
  pages are the format's price and the statement cache is not a lever.
- **Do not serialise writers, widen the step, or batch across steps.** Measured: the wider
  step fails, it does not wait.
- **Do not ship a format change as an optimisation**; it moves the Store and needs an owner.
- Do not weaken publication scoping, collision checking, the ownership watermark, the memory
  bound or failure cleanup to win time.
- Do not compare across windows or binaries for an effect size, do not re-run for a better
  number, and do not quote a diagnostic row's time when the diagnostic wrote to stderr
  during the run.
- Do not spend this round on `filesystem` (+0.98 s in the last decomposition) — it has its
  own round.
- Do not close #190, #205, #208 or #209.
- Do not present any `history.*` row as admission evidence — all are diagnostics, admission
  `INELIGIBLE`, every budget class `NOT_RUN`.
