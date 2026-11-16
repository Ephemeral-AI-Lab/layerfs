# #42 decisions: GC 2.0 contract amendments and SQLite implementation deviations

> **Status:** Proposal for layerfs-project#42 owner sign-off; not a released
> contract. The frozen [`LayerFS-GC-spec.md`](LayerFS-GC-spec.md) (frozen for
> layerfs-project#41 on 2026-11-15, baseline `ed14ed53b`) is **byte-identical**
> on this branch: nothing here is written into it. On sign-off the accepted
> entries become spec text (or a new #42 decision entry), the spec's decision
> table indexes them, and this file is retired.

**Why.** The #41 implementation and the #711 fix-branch integration need a
different reading than the frozen sentences in eleven places: seven contract
amendments (A1–A7, mostly refusal detail shapes, one additive sidecar field,
one `gc_op` column and the snake_case rendering rule) and four SQLite
implementation deviations (D1–D4) that the frozen text either forbids or does
not describe. `gc-spec-gaps.md` (one of the implementation notes moved out of
the tree; its decisions are folded into these entries where they carry contract
text) already flagged two of the seven as "#42 correction"; this note is the
complete list, and it changes no product behaviour.

The seven A entries were written directly into the frozen file while #41 was
implemented and were backed out before review (commit `122a168502`). The four D
entries are present in the shipped SQLite code and are recorded here so the
contract and the implementation do not drift apart.

---

## A. Contract amendments (7)

### A1. §4.1 `project.delete` sidecar gains `op_id` (additive)

- **Frozen:** `<home>/store/<store>/delete/<project>.json` is
  `{store, project, stack_id, deleted_ms, attempt_ms, state}`.
- **Proposed:** add `op_id`, the project op the admit/mark attempt ran under.
  An older sidecar without `op_id` stays valid. The retry then settles a lost
  admit/mark reply instead of leaving it `Unknown` (§5.1, §10).
- **Reason:** the retry cannot otherwise name the op whose reply was lost.

### A2. §8 `gc_op` gains the `usename` column

- **Frozen:** `gc_op` records `artifact, pid, backend_start, incarnation`.
- **Proposed:** also record `usename`, so the lost-COMMIT recovery statement
  can terminate the owner's backend by `(pid, backend_start, usename, datname)`
  and not merely by pid/backend_start.
- **Reason:** `pg_terminate_backend` recovery in §8 matches all four fields.

### A3. §4.3 merge whiteout refusal detail is explicit

- **Frozen:** a merge that cannot materialize the whiteout is refused
  `Removed` with `detail {paths: [...]}`.
- **Proposed:** `detail {paths: [{path, frontier_serial}]}`, listing **every**
  unmaterializable path as a full element, deduplicated and sorted by
  normalized path bytes.
- **Reason:** a merge can find several paths at once; the client needs the
  exact shape and a stable order.

### A4. §4.3 `Removed` refusal detail is one shape on every call

- **Frozen:** "Every refusal carries one detail shape: `{paths: [{path,
  frontier_serial}]}` (one element for read/stat; the governing maximum
  frontier and the request path)."
- **Proposed:** a `Removed` refusal always carries that one shape. For
  `file.read`/`file.stat` it is one element: the request path with its
  governing maximum frontier. A merge lists every unmaterializable path,
  sorted by normalized path bytes.
- **Reason:** makes the one-element vs many-element cases explicit instead of
  parenthetical.

### A5. §4.3 hidden-path read surface: `cp`/`edit` vs `mv`/`rm`

- **Frozen:** "`project.export` omits it; a cp-from source is `Removed`;
  `file.edit` is `Removed`; `file.mv` from a hidden path is `NotFound` and to a
  hidden path is a visible re-creation; `mkdir`/`cp`/`apply` follow the same
  construction rule; `file.rm` of a hidden path is `NotFound`."
- **Proposed:** "`project.export` omits it; `file.cp` from a hidden source and
  `file.edit` are `Removed`; `file.mv`/`file.rm` from a hidden path return the
  `NotFound`/`NoSuchPath` wire code, not `Removed`; `file.mv` to a hidden path
  is a visible re-creation; `mkdir`/`apply` follow the same construction rule."
- **Reason:** the wire code for a hidden `mv`/`rm` source is `NotFound` /
  `NoSuchPath`; naming `cp` in the construction list contradicted its own
  `Removed` sentence.

### A6. §10 code table gains `Unknown`

- **Frozen:** §10's table has no `Unknown` row, although §4/§4.2 use it.
- **Proposed:** `Unknown` is a service/control-plane outcome, not a
  `HistoryError` code: a lost reply, or a Store listing that cannot be
  determined. Detail `{entity: {kind, name}, phase, op}`, with `phase`/`op`
  `null` when no durable op is known.
- **Reason:** callers must be able to distinguish a lost reply from a refusal;
  the row makes the existing §4 use explicit.

### A7. §10/§11: §11 names are logical; wire/SDK/CLI keys are snake_case

- **Frozen:** "The SDK/CLI render every key snake_case."
- **Proposed:** the §11 names (e.g. `deadBytesBound`,
  `keptWaste.deadInRetainedPacksBytes`) are logical Rust names only; the wire,
  SDK and CLI keys are snake_case (`dead_bytes_bound`,
  `kept_waste.dead_in_retained_packs_bytes`), per §10.
- **Reason:** the single sentence did not say which names are logical and
  which are rendered; examples remove the ambiguity.

---

## D. SQLite implementation deviations (4)

These four are in the shipped code
(`engine/crates/layerfs-persistence/src/backend/sqlite/compact.rs`) and cannot
be described as "the frozen text is followed". Each paragraph states the frozen
sentence, the behavior, and the decision requested on #42. None changes
behaviour on this branch: the code stays as measured, and the entry either
matches the frozen text or records the deviation.

### D1. §7 step 4: the prune runs with `foreign_keys=OFF` and `defer_foreign_keys=ON`

- **Frozen:** "`PRAGMA foreign_keys=ON` outside the transaction (inside it the
  pragma is a no-op), then one rollback-journal transaction."
- **Implemented:** the artifact is opened with `foreign_keys=OFF` and
  `defer_foreign_keys=ON` for the prune. With enforcement on, SQLite's
  deferred-constraint counter survives the delete order the staged copy needs
  and the `COMMIT` is refused `SQLITE_CONSTRAINT_FOREIGNKEY`, even though
  `pragma_foreign_key_check` reports the final row set clean. The final state
  is proven by the §7 step 6 `foreign_key_check` on an `immutable=1` open
  before the rename.
- **Decision requested:** record the deviation (the pragma is off for the
  prune, the post-prune check is the guard) or require a delete order that
  commits under enforcement. The current code is the tested, reproduced
  behavior.

### D2. S5/§3/§7: keep the whole-Store `VACUUM INTO` + delete rebuild; peak is 2.9× the Store

- **Frozen:** S5 says "A rebuild copies only retained rows into a fresh Store";
  §7 step 4 describes the payload sweep; §3 says a rebuild copies at least
  nine live bytes per byte freed below the 10% threshold; §7's cost sentence
  bounds peak disk as old Store + new Store + journal/WAL + visited.
- **Implemented:** the build writes the whole sealed Store with `VACUUM INTO`
  to `<store>.compact`, prunes dead rows with a journaled `DELETE`, then
  `VACUUM`s the staged file into its final size. Measured on a 49.999 GB Store
  at 10.0% dead: `rebuild` 5,131.835 s (85.5 min), real temp peak
  **143.1 GB above the pre-rebuild baseline (~2.9× the Store)**, receipt bound
  **147.609 GB**; total footprint old + peak ≈ 198 GB (3.9×). The §13 /
  #42 item 24 target was about 2–3 minutes of downtime and ≤ old + 1.1 × live.
- **Decision (owner, this branch): do not change the algorithm.** S5's
  "fresh Store" is satisfied by the staged copy (pack rows are never updated
  or deleted in the live Store), and the measured 2.9× peak is the number the
  operator docs and runbook must state. The speed/peak gap is recorded as a
  follow-up (#42), not fixed here.
- **Alternative recorded, not adopted:** "clone + delete + incremental
  vacuum" — `fs::copy`/`copy_file_range` (reflink on xfs/btrfs) the sealed
  file, delete dead rows in the clone with `journal_mode=OFF`, run
  `incremental_vacuum` until the free list is empty, verify, rename. It follows
  dead bytes instead of live bytes and can hold peak near one Store copy. It
  needs a #42 S5/§3 entry, and it is proposed **only below a size threshold**
  (reflink filesystems; ext4 falls back to a single ordered live-row copy),
  where it does not pay for the extra ordering complexity. Not in this PR: the
  fix branches stabilize the current algorithm.
- **Runbook consequence:** a 50 GB `rebuild` at ~10% dead needs **≥150 GB free
  (≥155 GB with margin)**, and ≥115 GB at ≤30% dead [E]; re-check free space
  before every rebuild. The runbook carries this (see its "Disk headroom"
  section).

### D3. §3/§7 step 1.5: the `none` path writes a transient visited scratch

- **Frozen:** §3's `none` row is "Mark only; nothing is written"; §7 step 1.5
  says "On `none`, delete the scratch and stop: no `.compact`, no
  `.compact-visited`, no residue."
- **Implemented:** both paths spill the closure into `<store>.compact-visited`
  (the metadata walk needs a bounded-memory visited set). On `none` the scratch
  is written, deleted before the call replies, and counted in `peakTempBytes`;
  no `.compact`, no residue and no generation change survive the call. The
  mark's transient `kept_node` rows are cleared and the intent released.
- **Decision requested:** say "nothing is written **that survives the call**"
  and count the transient visited scratch in `peakTempBytes` (the §11 table
  already names the visited scratch as part of `peakTempBytes`). Alternative is
  an in-memory visited set, which the measurement rules out for large Stores.

### D4. §11: SQLite records a fifth observable phase, `cleanup`, during settle

- **Frozen:** "`mark`, `build`, `verify`, `install` on SQLite; `mark`, `build`,
  `swap`, `cleanup` on PostgreSQL."
- **Implemented:** the SQLite settle writes phase `cleanup` to the home op
  while it removes the artifact set and clears the intent, so a crash in the
  settle reads `cleanup` (the next `store.compact` adopts the op). The four
  build phases are unchanged; `cleanup` is the settle phase, not a build step.
- **Decision requested:** add `cleanup` to §11's SQLite phase list (it already
  exists on the PostgreSQL list), or restrict SQLite to four phases and lose
  the settle-phase diagnosis.
- **Runbook consequence:** `store.status` shows `cleanup` on SQLite after the
  install point; the runbook's phase list includes it.

---

## Evidence basis used by D2 and the runbook

Measured on the host class **Dell PowerEdge R730, PERC H730 Mini controller,
rotational (HDD) backing — not NVMe** (88 vCPU across two Xeon E5-2699C v4,
125–135 GB visible RAM, ext4, 4.0 TB free). The rotational device explains the
~33 MB/s build copy rate; do not read the 50 GB rates onto an NVMe host.

| Step (GB = 10^9 B) | 10 GB arm [M] | 50 GB arm [M] |
|---|---|---|
| Mark (`none`) | 14.9 s/stored GB, 16.4 s/live GB | 35.9 s/live GB (32.7 s/stored GB) |
| Mark (`rebuild`) | 16.4 s/live GB | 17.2 s/live GB |
| Build (`VACUUM INTO` + prune + `VACUUM`) | 10.3 s/new GB | 93.0 s/new GB (moves ~141 GB to keep 45.8 GB, ~33 MB/s) |
| Verify | 0.034 s/new GB | 1.87 s/new GB |
| Install | ~1.9 s fixed | 7.311 s |

- **10 GB is the medium arm** every agent can qualify: three runs per path
  (9.987 GB pack bytes; `none` median 149.318 s, `rebuild` median 246.371 s;
  one `Durable` rebuild 284.078 s as a control).
- **50 GB is the owner-final arm** (#42 item 44): fixtures 20–50 GB, run once
  at the final identity, one run per path (49.999 GB, 384,850 packs, 8.876%
  dead; `none` 1,633.585 s, `rebuild` 5,131.835 s). It is not part of a normal
  PR run.
- The 50 GB numbers do **not** scale linearly from the 10 GB ones (the 10 GB
  Store and its artifacts fitted the page cache; the 50 GB one did not), so a
  capacity claim states per-GB rates with its arm and host.

## Request

Please record A1–A7 as #42 items (or amend the frozen text) and accept or
reject D1–D4 individually when the owner signs off. Until then the
implementation follows the frozen text except where D1–D4 are named, and the
spec file in this branch is byte-identical to the frozen version.
