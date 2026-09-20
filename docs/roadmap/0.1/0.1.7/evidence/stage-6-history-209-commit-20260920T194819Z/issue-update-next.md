## #209 — what is left in `commit_ns`, why it is stuck, and what the next round attacks

Diagnostic; not release admission. Follow-up to the previous two comments.

**`commit_ns` is at its floor.** Today it is 1.806 s of a 16.598 s operation (10.9 %),
39.43 µs per append. That is 1,095,642 pages — the pack body's own pages, 4.18 GiB of
them — at 1.65 µs a page, against a bare 4 KiB `pwrite` of 1.723 µs on this volume.
Measured directly: `SQLITE_DBSTATUS_CACHE_WRITE` equals the payload's page count
**exactly** in every profile tested, with zero spills.

**Three mechanisms block the rest, each with a measurement:**

1. **The bytes are the format.** Each append hands SQLite the whole pack body and
   SQLite rewrites the entire overflow chain, because `sqlite3BtreeInsert` overwrites
   in place only when the new payload is the *same size* as the old. Writing fewer
   pages means chunked packs, a pre-allocated row or incremental blob writes — each
   moves the Store hash, which this lane's rules call a different operation. Worth
   ≈1.5 s; needs an owner decision on a format change, not more engineering.
2. **The cadence cannot be amortised, and that is measured, not assumed.** The 7.7×
   per append *is* the amortisation: 1,149 transactions over 45,791 appends before,
   48,446 now. Widening the step was tried at 8, 64 and 100000 appends and **all three
   fail in round 0** with `CleanupFailed { original: OwnershipUnavailable, cleanup:
   OwnershipUnavailable }` — `busy_timeout = 0` by declared profile, and the refused
   save's cleanup then needs the lock it cannot get.
3. **The engine writes one page per syscall and may not be patched.**
   `pager_write_pagelist` calls `sqlite3OsWrite` per dirty page — no `writev` — and
   rollback-mode mmap is read-only (`sqlite3PagerWrite` asserts the page is not
   `PGHDR_MMAP`). The profile has nothing left either: seven pragma profiles, identical
   page counts, 1.8 % spread, including a contract-breaking `journal_mode = OFF`
   diagnostic at −1.3 %.

**The next round is not `commit_ns`.** The same-window measurement (previous comment)
puts `commit_ns` at 27.9 % of the gap and `resolve_ns` at 27.2 % — within 3 % of each
other — so the queue is:

| target | measured | status |
| --- | ---: | --- |
| fresh statement preparation on the per-step path | **0.22–0.33 s** | `UPDATE object_packs SET data …` costs 4,723 ns prepared fresh against 102 ns cached, ×46,049 calls; `SELECT next_pack_id` 1,106 ns against 83 ns; `BEGIN IMMEDIATE` and `COMMIT` likewise. **This is the next round's treatment** |
| `resolve_ns` cadence share | **≈0.82 s** | not understood. The locator is 6.5 µs slower inside a per-step transaction (28.32 vs 21.78 µs with step commits disabled) and nobody has established why. The candidate hypothesis the next round also tests: the connection's prepared-statement cache holds **16** entries by default, while the locator's SQL text carries one placeholder per page width (`LOOKUP_PAGE_IDS = 128`) and the object-insert text one per chunk size — so the LRU may be missing on most calls and paying a fresh parse of a 128-placeholder query inside the timed region |
| `BEGIN IMMEDIATE` | ≈0.43 s (that session) | lock syscalls per step; removable only by holding a transaction across steps, which is the cadence again |
| `filesystem` | **+0.98 s** | a separate round; not examined |

Proceeding with the statement-cache round now: pre-registered as one rule — *a
statement the operation issues repeatedly is prepared once, and the cache is sized to
hold the ones that are* — sampled from one binary with the arm behind a
measurement-only lever, both writers measured, and the Store required to stay
byte-identical.
