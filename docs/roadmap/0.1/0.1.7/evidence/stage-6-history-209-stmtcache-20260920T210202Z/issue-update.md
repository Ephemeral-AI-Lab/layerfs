## #209 — the per-step statement cache: the bucket moved, the wall clock did not. Withdrawn.

Diagnostic; not release admission. **No product line is kept** — the tree is reverted to
`f3e84c073`. Report:
`docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-stmtcache-20260920T210202Z/README.md`;
ledger **L59**. Successor prompt:
`docs/roadmap/0.1/0.1.7/issue-commit-time-rca-handoff.md`.

**The treatment, pre-registered before it was written.** Every SQL text the per-step path
issues was being parsed again on every call — `UPDATE object_packs SET data …` **4,723 ns
against 102 ns** cached over 46,049 calls, `SELECT next_pack_id …` 1,106 ns against 83 ns,
`BEGIN IMMEDIATE` and `COMMIT` likewise. The treatment put six texts through the
connection's prepared-statement cache, predicting operation **≤ 16.35 s**, `sql_ns`
**≤ 0.96 s**, `commit_ns` **≤ 1.79 s** and `resolve_ns` **not above 4.45 s**, with
withdrawal if the operation or `sql_ns` failed to fall.

**Measured, both arms from one binary** (sha256 `85605a522738e6…`, the control behind a
declared measurement-only lever, A B B A balanced, Store byte-identical in all four rows
with `commits` 48,446 in every row):

| arm | operation | `commit_ns` | `sql_ns` | `resolve_ns` |
| --- | ---: | ---: | ---: | ---: |
| treatment (prepared once) | 22.256 / 22.770 s | 2.485 / 2.494 s | **1.279 / 1.343 s** | 5.985 / 6.170 s |
| control (parsed per call) | 22.536 / 22.537 s | 2.377 / 2.429 s | **1.590 / 1.581 s** | 5.994 / 5.944 s |
| Δ (treatment − control) | **−0.024 s** | **+0.087 s** | **−0.275 s** | +0.109 s |

**Withdrawn under its own falsifier.** `sql_ns` fell by 0.275 s against a predicted 0.22 s
— the parse confirming itself — and **the operation did not fall** (−0.024 s, inside the
window's own −0.514 s drift). The saving is redistributed: `sql_ns` −0.275 s against
`commit_ns` +0.087 s and uncharged per-step work +0.055 s. A B B A makes `commit_ns` a U in
time and `sql_ns` a hump, so two samples per arm cannot separate an arm effect from a
window effect there. **Why removing a parse from the pack `UPDATE` adds time to the commit
that follows it is not established**, and nothing here claims caching makes stride10
slower — only that it does not make it faster.

**Two diagnostics, and they killed two hypotheses.** The connection's statement cache is a
**16-entry LRU** and one width over capacity turns a 108 ns hit into a **10,677 ns** miss —
but a measurement-only counter over a real run counts **380,444 locator calls at 126
distinct widths with 99.0 % one identifier wide** and **44,334 object-insert calls at 68
widths with 99.6 % one wide**, so the hot entries stay resident: **text construction and
cache thrash are both refuted** as explanations of the ≈6.5 µs per locator call between the
in-run cost (10.00 µs) and the isolated one (3.488 µs). That ≈2.5 s remains unexplained and
is `resolve_ns`'s, not `commit_ns`'s. Separately, the pack `UPDATE` replayed with the
product's parameter shape and a realistic body cycle makes a cached statement **4,575 ns
cheaper** than a fresh one and prices **growth at +17.8 µs per append** — the parse saving
is real in isolation, and the whole-chain rewrite is the format's price.

**What the successor prompt asks for.** The commit-time defect has never been diagnosed
with SQLite's own instruments — every number so far was taken *around* the step, never
inside it. The next round owes: `EXPLAIN QUERY PLAN` and the opcode listing for every
statement one step issues against the run's own Store; `sqlite3_stmt_status`
(`FULLSCAN_STEP`, `SORT`, `AUTOINDEX`, `VM_STEP`, `REPREPARE`) after a real run;
`sqlite3_db_status` (`CACHE_WRITE`, `CACHE_SPILL`, `CACHE_HIT`/`MISS`, `CACHE_USED`) **on
the product's own connection**, which has never been read; the file's `page_count` /
`freelist_count` across a run; and a per-statement charge split inside the step, which is
exactly what this round could not resolve. Then one pre-registered treatment — or a
withdrawal with the same evidence.
