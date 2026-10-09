# E04 statement diet — implementation brief

> **Status:** Research; informative and not a product contract.

Implementation brief the previous R7 lead gave a subagent on 2026-10-09. Its first stage (D1, D2, D4, D5, D6, D8, D9, D11) landed in `c31c2a42c`; the rest is open. Paths under `/private/tmp` in it no longer exist: the report it names is `statement-diet-sql-trace-3ae26648f.md` beside this file.

You are the implementation engineer for one product change in
/Users/yifanxu/Ephemeral-AI-Lab/layerfs (branch main, one checkout, clean HEAD
when you start). Do not commit, push, stash, or start a worktree. Do not touch
Docker. Work only in `core/crates/layerfs-overlay` (src, sql, tests),
`core/crates/layerfs-workspace` (src, tests), and — only where a signature
change forces it — `core/crates/layerfs-daemon` and `core/crates/layerfs-fuse`
(src, tests). Update the affected architecture notes under
`core/docs/architecture/` with a short dated "R7 update" paragraph next to the
existing ones (see 19-daemon-overlay.md for the style).

Read first: `AGENTS.md`, `core/AGENTS.md` (product source rules: no test hooks,
999-line file ceiling, 200-line `lib.rs`/`mod.rs` ceiling, no retry/replay, one
attempted operation), `docs/general/optimization-guide.md`, and the research
report this brief implements:
/private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/5590953f-a1d7-48dd-91d2-01dfcf7e82bf/tasks/a5ebc1dcbbef747b0.output.final.md
(exact per-job SQL trace at 3ae26648f with file:line, and the diet items
D1-D15). Its D12 is ALREADY DONE differently: reply tickets are in memory
(`src/lifetime/tickets.rs`), the `request` table and its statements are gone,
and the reply-attempted jobs no longer exist on the request path. Schema
version is 21; you bump it to 22.

## Goal
Benchmark cell C01 (`echo $i > f$i` 1000 times in the mount root) costs, per
created file, 5 owner jobs: GETATTR(root), LOOKUP(root,name) negative,
CREATE, WRITE (2 bytes), RELEASE. Cut the SQL of those five jobs to the
minimum without changing what any of them decides. Target per created file:
about 25 statement attempts and 3 write transactions (CREATE, WRITE,
RELEASE), from about 88 attempts today. Every remaining statement must be a
point seek or a single-row write. No boundary may weaken: unlimited files,
file size and mutation count stay bounded by resources only (state in indexed
SQLite rows, no resident container that grows with the workload); permission
decisions unchanged; no retry.

## Items (owner decisions already taken; implement all unless you prove one unsafe)
- D1 one workspace state read per job: thread the state the job read first
  through `source_rows`, `apply_checked`, `check_file`, `queue_closed`.
- D2 no `check_file` re-read after the visit's own handle read; keep the
  read-only-descriptor refusal as a check on the row already read.
- D3 job-scoped row memo (at most 2 inode rows and 2 name-layer results,
  fixed size, valid only inside one atomic job and dropped at that job's
  first writing statement and at its end) so round 2 of a visit and the
  `inherited` computation do not re-read what round 1 read. If restructuring
  the callers to pass the rows is cleaner than a memo, do that instead.
- D4 `name_inheritance` computed once per name per job.
- D5 active + lower name seek in one statement (report gives the SQL).
- D6 skip the orphan probe unless an engine-lifetime `orphan_seen` flag is
  set (set at the only `INSERT INTO orphan`, never cleared; a rolled-back
  insert leaves a harmless false positive).
- D7 `INODE_LOOKUP` returns the layer columns; a fresh reserved serial is a
  plain INSERT, a known active row a plain UPDATE; no `LAYER_ACTIVE` /
  `LAYER_LOWER` re-reads for rows the job already holds. A serial collision
  must be a definite error, never a silent update.
- D8 no pre-read before inserting the kernel lookup reference of an inode
  this job just created.
- D9 the visit fence (workspace state + native mount + kernel reference or
  handle) in one SELECT, with the same Stale/Closed/refusal mapping.
- D10 stop writing `lease` rows of kinds 7 and 9 (native handle / native
  lookup owners): nobody reads them by key. `queue_closed` must still hold a
  closed namespace while ANY custody exists — native and non-native
  (`open_file`, `lookup_owner`): replace the any-lease EXISTS by EXISTS on
  the tables that actually hold that custody. Audit every other reader of
  `lease` (maintenance ready checks, operation records, accounting
  `owner_rows`) and keep them exact.
- D11 one custody upsert for create+open; drop + remaining-reference count in
  one `UPDATE ... RETURNING`.
- D13 accounting rows narrowed: `accounting(ns, counter, value)` WITHOUT
  ROWID with one row per (namespace, counter), rows created with the
  namespace, each trigger doing one point UPDATE per affected row instead of
  rewriting an 18-column row. Keep both the per-namespace and the ns=0
  aggregate semantics and `Overlay::resources` results exactly as today.
- D14 fold `native_file` (mount, request) into `file_handle` if and only if
  the retire window (`maintenance/native.rs`), per-request uniqueness and
  forced-unmount cleanup stay exact; otherwise leave it and say why.
- D15 skip `CELL_LOOKUP` for a cell that cannot hold earlier bytes — only
  after auditing every writer/mover of payload cells (`maintenance/orphan.rs`,
  `lifetime/composition.rs`, shrink/regrow). If the invariant is not
  provable from source, leave it.
- Also: replace CHECK constraints on hot tables that compile to an ephemeral
  IN-table (`kind IN (1,2,3)`) by equivalent range/OR forms; remove
  `sql::LEASE_LOOKUP`-style dead statements you orphan.

Not in scope: changing transaction framing (BEGIN/COMMIT/admission), the
payload cell format, the owner/queue, the Fuse layer's behaviour, cluster
one, or the legacy source-holding mutation path's semantics (it must keep
compiling and passing its tests; share helpers where natural).

## Proof required
- EXPLAIN QUERY PLAN for every new or merged statement (add to the existing
  explain accessors/tests): all SEARCH, no SCAN, no temp b-tree.
- A cost test (`layerfs-workspace/tests` or `layerfs-overlay/tests/costs.rs`
  style) that drives exactly the five C01 jobs through the public visit API
  in steady state (parent = base root with a local row, absent name, 2-byte
  write, last close with the kernel lookup still held) and asserts the exact
  per-family attempts and executions for each job, plus that the totals do
  not change between the 10th and the 4,000th file in one directory.
- Every existing exact-count test updated to the new exact numbers with a
  truthful comment; no assertion loosened to an inequality.
- Behaviour tests for each removed re-validation: e.g. a handle of another
  mount, a revoked mount, a closed workspace, a read-only descriptor, a
  missing kernel reference, a serial collision, an orphaned inode still
  found after `orphan_seen`, closed-namespace cleanup held by a native
  handle / native lookup / non-native open file / lookup owner and released
  after the last one.

## How to build and run (host macOS; one command at a time; all bounded)
- `bash core/target/r4-build.sh check layerfs-overlay layerfs-workspace layerfs-daemon layerfs-fuse`
- `bash core/target/r4-build.sh test-no-run <package>` prints the test executables.
- One bounded run of one test binary with an append-only receipt:
  `RX_STAGE=r7-optimization-20261009 bash core/target/rx-run.sh <track> <attempt> <package> <binary-file-name> [test filter]`
  Use tracks `320-diet-overlay`, `320-diet-workspace`, `320-diet-daemon`,
  `320-diet-fuse`; a rerun of the same binary needs the next attempt number.
  Never loop a failing test; diagnose from the receipt and source first.
- `bash core/target/r4-build.sh clippy layerfs-overlay layerfs-workspace layerfs-daemon layerfs-fuse`
  (must be clean with `-D warnings`), `bash core/target/r4-build.sh fmt <packages>`,
  `bash core/target/r4-build.sh guard`.
- Known unrelated host failures: daemon `complete_installed_roots` and
  `host_handoff` (missing preconditions). Do not run Linux/Docker; the lead does.

## Report (final message)
The new ordered statement list of each of the five jobs with per-family
attempts/executions (measured by your cost test, not estimated); which D
items you implemented, which you left and exactly why; every schema change;
every invariant argument for a removed statement in one line each with the
test that covers it; the final result line of every binary of the four host
suites; clippy/fmt/guard results; files changed; anything unfinished.
