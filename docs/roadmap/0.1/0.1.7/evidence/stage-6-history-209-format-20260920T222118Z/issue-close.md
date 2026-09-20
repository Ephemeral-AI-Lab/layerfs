## #209 closing summary — what the multi-writer regression actually was, and where the time is

Closing by owner decision on 2026-09-21, after ledger entries **L54–L62**. Rounds L57–L62 are on `codex/190-pooled-scope`; the last three commits are `73e0b961c` (the instrument round), `1bfb0c5dc` (the format pre-registration) and `f876242b2` (the format round, reverted). The product tree is byte-identical to `704580673` over `core/crates` and `crates`.

### Why the operation regressed when multi-writer landed

Three causes, in order of size, each measured in its own window:

1. **The locator `LIMIT`** — save-scoped publication means `objects` can hold up to `SAVE_SLOTS` rows per `object_id`, so the locator query gained `ORDER BY o.object_id,o.save_id LIMIT ?` to guard a duplicate case this corpus never has. The scoping itself is *cheaper* than the previous model's query (3.488 vs 4.559 µs); the `LIMIT` cost **15.5 µs per call × 380,380 calls**. Removing it recovered **6.65 s** (L54). **Fixed and kept.**
2. **The cadence multiplies how often the same bytes are written.** A pack grows on every append, so `sqlite3BtreeInsert` compiles to `OP_Delete` + `OP_Insert` and rewrites the whole overflow chain every time. Inside a ~40-append transaction 39 of every 40 rewrites are superseded in the page cache; with one commit per append every one of them is flushed. The old model's commit was **5.5× cheaper per transaction** (204.5 µs vs 37.3 µs) and still **7.67× more expensive per append** (L58).
3. **The fixed per-transaction cost, paid 42× more often** — `BEGIN IMMEDIATE` 9.7 µs (≈3.8 µs of it the eager RESERVED file lock), the `COMMIT` statement's setup, and the per-step statement parses ≈ 25 µs × 48,446 ≈ **1.2 s**.

**The cadence is not a tuning choice.** With `busy_timeout = 0` by declared profile any overlap refuses the second writer rather than delaying it: `COMMIT_EVERY` = 8, 64 and 100000 all fail in round 0 with `CleanupFailed { OwnershipUnavailable }`. Widening the step does not slow the second writer, it loses its save.

### The commit path, read with SQLite's own instruments (L61)

- **Q1.** A step's transaction writes **27.839 pages per commit** on the product's own connection (`SQLITE_DBSTATUS_CACHE_WRITE` = 1,348,771 pages over 48,446 commits): **24.433** the pack body it grew and **4.74** the structural pages the growth forces — page 1's change counter, the `objects` primary-key btree, the `objects_save` index, the free-list trunk and the pack's own leaf (≈245,000 predicted against 229,788 measured). **Zero spills.** `COMMIT` executes **3 VDBE opcodes** and costs 56.1 µs, so its price is the page writes and nothing else.
- **Q2.** **No statement does more than its rows and indexes require.** `FULLSCAN_STEP`, `SORT`, `AUTOINDEX` and `REPREPARE` read **0 in all 21 buckets**; every plan is a seek; the `EXISTS` subquery is a coroutine evaluated once for the one matched row. The 4 MiB pack cache is not thrashing either: **97.33 % hit rate** over 227 distinct packs.
- **Q4.** `BEGIN IMMEDIATE` is **9.7 µs of every step and 0.47 s of the run** for 5 VDBE opcodes; a deferred-`BEGIN` arm prices **≈3.8 µs of it as the eager RESERVED lock**. Not removable — the watermark read that opens the step must happen under the write lock.
- **Kept from L57:** the per-step policy write removal — 48,191 of 48,446 watermark statements and 45,794 ceiling statements gone, Store byte-identical, `commit_ns` −5.9 % / −5.0 % in two windows.
- **Withdrawn:** the per-step statement cache (L59) — the bucket moved, the wall clock did not.

### The format: measured at −52.8 % on `commit_ns`, and reverted by owner decision (L62)

The whole-chain rewrite is the format's price. Reserving each lane's pack directory area at a fixed offset, recording the assembled length in the header and pre-allocating the row in reservation steps makes an append keep the payload the same size, so SQLite takes `btreeOverwriteCell` and its content comparison and dirties only the pages whose bytes changed: **3.353 pages/commit against 34.158** in a four-arm design probe. Measured on a matched pair in one window: **operation 26.021 → 24.720 s (−1.300 s, −5.0 %)**, **`commit_ns` 2.938 → 1.388 s (−1.550 s, −52.8 %)**, per append 64.16 → 30.30 µs, every other bucket inside ±0.11 s. The prediction was beaten on direction and missed on level (1.388 s against ≤ 1.0 s), and the residual is recorded as unattributed.

**It was reverted by owner direction and is not shipped.** The change is retained whole at [`scratch/format.diff`](https://github.com/Ephemeral-AI-Lab/layerfs/blob/codex/190-pooled-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-format-20260920T222118Z/scratch/format.diff), so it can be re-applied without re-deriving it. It moves the Store hash and needs a format-profile and schema bump, which is why it was always an owner decision.

### A measurement-protocol finding that outlives this issue

**The machine's level moves by 55 % and the protocol's quiet preflight does not predict it.** The archived, unchanged profile-1 binary read **16.739 s in L57's window, 17.550 s in the confirmation window and 26.021 s in the last window** — same executable, same 45,794 appends, same 48,446 commits. All six rows compared passed the same gate (no named `cargo`/`rustc`/`fs-bench` competitor, ≥ 70 % idle) and the **fastest window carried the highest load** (8.38 against 5.42). Every future round must re-derive its absolute bar in-window, and no preflight reading currently available will tell it whether the window is a fast one.

### What is not done

- **`resolve_ns`** — the locator is 2.5 s of engine time over 297,082 save-path calls (8.48 µs/call against 3.488 µs isolated) and `pack_bytes` is a further 1.1 s over 19,771 whole-body reads at 56.1 µs each. The cache hypotheses are dead; the gap is not explained.
- **`filesystem`** — 6.7 s, its own round, never attributed.
- **The format patch** — retained, unshipped, worth 1.3 s as measured and ≈3.07 s if the append's btree work collapses with it.
- **#190, #205 and #208 remain open.**

### Custody

Every round is receipted under `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-*` with its raw `trace.jsonl`, `timing.json` and saved Store, its archived binaries with sha256, and its checks. Ledger entries L54–L62 are in [`0.1.6/evidence/issue151-experiment-ledger.md`](https://github.com/Ephemeral-AI-Lab/layerfs/blob/codex/190-pooled-scope/docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md). All rows are diagnostics: admission `INELIGIBLE`, every budget class `NOT_RUN`.
