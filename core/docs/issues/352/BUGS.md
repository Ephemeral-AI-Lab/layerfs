# #352 — Bugs

> **Status:** Current, kept by the lead and the fixer. One line per stop finding:
> range, playbook step, cause, fix kind, fix commit, recovery.

| Range.attempt | Step | Cause | Fix kind | Fix commit | Recovery |
| --- | --- | --- | --- | --- | --- |

## Open findings (before any range)

- **F-001 · fs request size cap.** One fs request carries at most 65,519 bytes, so
  `fs.write` cannot write a file over ~64 KiB. AGENTS.md rules out transport frames
  as a total-file cap. Workaround in the plan: those files go through a Sandbox
  `apply` step. Owner of the fix: the fixer, before P3's 9 : 1 split is measured.
  **Fixed** (fixer, D0, kind 1): write bodies stream after the call record and
  are written a record at a time (`layerfs-api` `base/stream.rs`, daemon
  `sdk/body.rs`); 64 MiB `fs.write` round-trips in ~1 s, daemon peak +14 MiB.
  Daemon `ed66aa0d`, addon `3da0f665`. The planner can drop its size clause.
  [fixes/000-pre-f001-streamed-write-bodies.md](fixes/000-pre-f001-streamed-write-bodies.md).
- **F-002 · full-scan steps on hot paths (lead, not yet a gate finding).** Dev runs
  show reader `fullscan_steps` of 58 at mount bind, 242 at a one-shot Commit and
  668 at a merge. To be read against depth in the pilot (playbook D4).
- **F-002, fixer's reading (002.a1).** The 242/668 at Commit and merge are each
  Save's read of the content-signature ring (`metadata_signatures.rs`, a scan and
  sort of at most 8,193 ring rows; 273 now). Bounded by the ring, not by depth;
  the 58 at mount bind is flat (schema checks). Left as is; the merge part is in
  [fixes/002-a1-merge-ancestry.md](fixes/002-a1-merge-ancestry.md).
- **F-003 · diff_commits walks both whole trees (fixer, 002.a1, not yet a gate
  finding).** `compare.rs` `changes()` reads both trees; probes give VM-step
  exponent 0.59 against depth with one changed file. A change-proportional diff
  needs per-directory change information the format lacks (SPEC §6 format kind).
  **Fixed** in `49499abac` (Store format: `binding_hint`, walk in
  `layerfs-content::compare`): [fixes/002-a1-diff-commits-change-walk.md](fixes/002-a1-diff-commits-change-walk.md).
- **F-004 · report.mjs rewrites earlier ranges' recorded reports (tooling, lead,
  2026-10-18).** After the same-binary gate change, `play` regenerated
  `ranges/range-000.md` and `range-001.md`, replacing their recorded check
  results and reasons with a re-evaluation under the new gate. The lead
  restored the committed versions; recorded outcomes are never edited. Fix
  (builder): `report.mjs` writes only the report of the range it just played and
  keeps every earlier attempt's recorded check rows as written. **Fixed** in
  `badd7494d` (stored gate results, `report.mjs check`, format rule F1–F8).
- **F-005 · chunks are never stored as deltas (lead, 2026-10-18, owner-tracked).**
  Range 4's chunk depth histogram is [0: 906], and [0: 2,419] since Init: every
  chunk object is FULL. Is it no base found, or are files rarely chunked
  (≥ 128 KiB)? It feeds the delta decision (SPEC §8). Owner: the fixer,
  diagnosis first.
  **Diagnosis (fixer, D5):** wiring defect. The chunk selector only takes the
  base its caller offers (`layerfs-storage/src/encoding/delta/select.rs:311-312,
  383-390`) and captured construction never offers one (`construct_runs` passes
  `None`, `layerfs-content/src/file/construction/runs.rs:93,100,103,125`;
  `stream_combined`, `file/edit/apply.rs:517`). Fix accepted: offer the old
  file's chunk at the same position as base; no Store format change. Lands after
  F-001, before P2's measured run.
  **Fixed** (fixer, D5, kind 1): the localized edit route offers each new chunk
  the old chunk sharing most of its bytes, aligned on unchanged chunks (fallback:
  the unchanged neighbour's successor). Range-4 fork, 1 MiB file: insert near the
  start 37,029 → 3,941 stored bytes (0/3 → 3/3 chunks as deltas), middle edit
  25,649 → 4,785; the append was already a delta. Files built without a base
  (new inodes) still store FULL. Daemon `cbfa5e7f`;
  [fixes/004-dev-f005-chunk-delta-base.md](fixes/004-dev-f005-chunk-delta-base.md).
- **F-006 · health cannot judge overlay files left (lead, 2026-10-18).** The
  health row reads "? overlays", so a leaked Workspace overlay would go unseen.
  Owner: the builder (measure it per range, REPORT_FORMAT.md). **Done** in
  `a1d975ffa` (measured per range in the report's health row).
- **F-007 · Trends were never written (lead, 2026-10-18).** §6 has been empty in
  every report; it becomes the headline's "moved most" line (REPORT_FORMAT.md).
  Owner: the builder. **Done** in `a1d975ffa` (the headline's top movers).
- **F-008 · Sandbox Commit refused `HeadMoved` under concurrent same-lane
  Commits (builder's P2 dev run, 2026-10-18; stop class, blocks P2).** A
  Sandbox Commit merges against the Branch head it read and fails at publish
  when another agent committed in between; the fs write path refuses a head
  move the same way. Range 5 (dev run `p2-dev`) stopped twice at r005.s0051.
  The plan expects such a Commit to merge cleanly (SPEC §2, stale clean merge).
  Owner: the fixer, first priority. A dev-only `--serialize-commits 1` aid
  exists and is refused for measured runs.
  **Fixed** (fixer, D1, code path): head read, merge and publish run inside the
  Store's publication lane (`layerfs-persistence` `lane.rs`,
  `HistoryCatalog::publish_lane`); fs writes merge too and conflict as
  `Conflict`. Daemon `18ab5415`; range 5 on a range-4 fork passes (old: STOP
  r005.s0048). [fixes/005-dev-f008-merge-at-publish.md](fixes/005-dev-f008-merge-at-publish.md).
- **F-009 · concurrent unmounts of two Workspaces in one Sandbox fail
  "Workspace absent" (builder, 2026-10-18).** Worked around in the player with
  one Sandbox per agent. Owner: the fixer.
  **Fixed** (fixer, D1, SDK addon): `refresh` skips a Workspace the daemon reports
  absent, and a process's own discovery mutations take turns before the OS lock.
  Addon `4e141e51`; [fixes/005-dev-f009-concurrent-unmount.md](fixes/005-dev-f009-concurrent-unmount.md).
- **F-010 · the checkpoint `diff_commits` is not O(changed) (builder,
  2026-10-18; owner's point 7).** Pages return ~20–27 entries; page 0 grew
  127 → 213 ms (401k → 703k VM steps) from range 5 to 7 (dev run); per entry
  3.3 → 6.5 → 13.0 ms against the ~11 ms one-file probe; later pages re-skip
  from the start. Dev ranges 7–11 gated optimize on it. Owner: the fixer.
  **Fixed** (fixer, D3, kind 1): `nextCursor`/`cursor` resume the change walk and
  `hunks: false` lists paths only; range-4 checkpoint 2,541 → 411 ms (2 pages
  instead of 12) with both. The player should adopt them.
  [fixes/007-dev-f010-diff-cursor.md](fixes/007-dev-f010-diff-cursor.md).
- **F-011 · old-format Stores are refused (lead, 2026-10-18, REBUILD.md).**
  `binding_hint` (002.a1) was handled as a Store-format change (kind 3, fast
  rebuild) although `diff_commits` already falls back to a full walk for roots
  without hints: it should have been kind 2. Forward fix: open older Stores,
  create the table when missing, record hints from then on. Nothing is rebuilt.
  Owner: the fixer, after F-005.
  **Fixed** (fixer, D0, kind 2): a writable open adds the table to a Store
  whose recorded schema is exactly the earlier one and records the current
  schema; trees saved before it diff by the full walk. p1-pilot's range-1
  snapshot (`0ef24121`) was refused by `cbfa5e7f` and passes its whole-tree
  checkpoint with `71001f65`; REBUILD.md §4 for the batch passes on a range-4
  fork written by `511dceed`.
  [fixes/002-a1-f011-binding-hint-migration.md](fixes/002-a1-f011-binding-hint-migration.md).
- **F-012 · checkpoint `diff_commits` per changed entry rises with depth
  (builder's re-baseline, daemon `71001f65`, 2026-10-18).** Per entry
  0.44 → 0.72 → 1.00 ms over ranges 0–2 (depth exponent 0.80, optimize in
  `p1-rebase` and `p1-control-b`; 0.61 and 0.42 with ranges 3 and 4); the
  one-file probe is flat (~11 ms, vm_steps 18–21k). Owner: the fixer, counter
  diagnosis before P2.
  **Diagnosis (fixer, D2/D3):** not depth. Replayed at depth 3,497, each
  checkpoint's counters equal the original's at depths 571–2,611 (range 1
  17,532 → 17,646 statements, range 3 27,462 → 27,624). Two causes: every page
  rebuilds the whole checkpoint's plan (range 3: 3 pages × ~26k statements), and
  statements per entry track tree files (~0.25 × files), which grows with depth
  in this replay. Actions: the fixer carries the plan in the cursor (kind 1); the
  builder makes the gate measure per checkpoint (all pages) per changed entry,
  keeping the one-file probe as the depth control. Not normalised by tree files:
  the per-entry tree-size growth is itself a §4 violation and is fixed in F-012.
  **Fixed** (fixer, D3, kind 1): the daemon keeps the 4 latest plans by root
  pair, so a later page reuses its difference's plan, and a bounded per-call
  object memo stops lookups re-reading B-tree upper pages. p1-rebase
  checkpoints, statements per entry 69 / 74 / 90 / 118 / 120 → 5.7 / 6.3 / 7.3 /
  6.6 / 8.0 (ranges 0–4); range 3 733 → 42 ms. Same 103 changes in a 400- and a
  30,100-file tree: 477 and 549 statements. Daemon `361bb09e`;
  [fixes/002-rebase-f012-diff-plan-memo.md](fixes/002-rebase-f012-diff-plan-memo.md).
- **F-013 · per-entry gate trips on a fixed per-checkpoint cost (tooling,
  lead, 2026-10-18).** `p1-rebase-b` (daemon `361bb09e`) gated optimize at
  range 2: checkpoint `diff_commits` ms per entry against tree files, exponent
  0.952. Total statements were flat (1,557 → 1,389 → 1,413 for 327, 240, 208
  entries) while tree files grew 255 → 373: per-entry cost = a/n + b rises as n
  falls. Fix (builder): judge the fixed part a and the marginal part b
  separately, each flat with tree size and depth; never normalise by tree files;
  unseparable points are "not judged" (watch). The recorded optimize verdict of
  that baseline run stays as recorded.
- **F-014 · the "one-file" `diff_commits` probe had a drifting input
  (tooling, builder, 2026-10-18).** It diffed the lane's newest Commit, which
  changed 4, 5, 15 and 60 files at ranges 0–3 (`p1-rebase-b`), so its raw
  statements (152, 146, 140, 239) tripped Counters against tree files (0.694).
  SPEC §3c defines one changed file. Fix (builder): a true one-file diff on the
  probe's own Branch, with a probe version in the metrics; gates never fit
  across versions; the other probes are checked for the same drift. Recorded
  values stay; the baseline is re-measured as `p1-rebase-c`.
- **F-016 · gate reads a bounded, periodic chain cost as depth growth
  (tooling, 007.a1, 2026-10-18).** `list_small` cycles with the metadata depth
  cap (8). Owner: the builder, after F-017; an unbounded series must still trip.
  **Fixed** (builder, gate): chain-reading probes are judged as a bound, the
  max over the latest cap period against the first (1.0× bounded), not a depth
  exponent; fewer than a period plus one point is not judged. `list_largest`
  is fitted per entry of its directory. gates.mjs header, SPEC §5.
  **Extended to `list_largest`** (liaison, 2026-10-19, after r017.a1). It lists
  a real directory (`/scripts`) whose entries are read through the same
  per-path metadata delta chains. The first exponent fit over 3 same-binary
  points (r015–r017) read the bounded chain residual as growth:
  - statements 72 → 92 → 103 and blob reads 153 → 207 → 240, at 45, 50 and 50
    entries;
  - Scaling 2.22 and Counters 1.188.
  The probe is now judged by the same `chainBound` code path, per entry of
  its directory, with no separate threshold. Growth past a cap period, or a
  rise per entry at a fixed chain depth, still trips (tests/p2.mjs, r017.a1).
- **F-017 · fs calls of all agents serialize on one Sandbox control
  connection (product, 007.a1 S1, 2026-10-18).** `registry.rs`
  `control: Mutex<Control>`; reads queue behind Commits (range 7: 1 ms work,
  19 ms p50 client). Owner: the fixer, now. **Fixed** (SDK addon `5baa0f5d`,
  kind 1): a bounded pool of up to 6 connections per Sandbox, queue wait in the
  trace; [fixes/007-a1-f017-control-connections.md](fixes/007-a1-f017-control-connections.md).
- **F-018 · SDK reads queue behind a Commit's publication lane (product,
  007.a1 F-017 tail, 2026-10-18).** History lookups (project, Branch, Commit,
  Layer) ran on the writer session, and traced calls read SQL counters
  through the writer's admission. Both waited for any Commit holding the lane,
  merges included; past 2 s a read failed `Busy`. **Fixed** (daemon
  `c25119ac`, kind 1): lookups use the control reader's read-only session, and
  counters are read under the state lock alone;
  [fixes/007-a1-f018-reads-behind-publication-lane.md](fixes/007-a1-f018-reads-behind-publication-lane.md).
- **F-020 · Store calls cross a Sandbox for no reason (product, owner direction
  2026-10-18, after 007.a1 F-017/F-018).** Every fs, branch, project and
  operator call takes a control round trip into a daemon in a Sandbox; on
  range 4 (`c25119ac`/`5baa0f5d`) fs.read is 4.48 ms p50 at the client for
  0.12 ms of traced work. Owner: the fixer, now (the builder keeps playing P2
  on the current route until the switch). Kind 1. Spec:
  [DIRECT_STORE_IMPL.md](DIRECT_STORE_IMPL.md); record:
  [fixes/007-a1-f020-direct-store-access.md](fixes/007-a1-f020-direct-store-access.md).
  One route, Linux only (owner): the daemon stops serving Store calls and the
  old-route tests are deleted, not converted. **Fixed on `dsh-352-f020`**
  (fixer; steps 1–3 `91cde4d50`, daemon host `ce83289f1`, proofs `373b0357c`),
  awaiting merge. Step 6 on range 12: read p50 6.08 → 2.84 ms, write p50
  14.73 → 11.81 ms, Workspace calls unchanged; isolated-call SQL equal; Store
  bytes +0.10% (interleaving; serial sequence equal). fs-bench-pro is a known
  break left for the owner.
- **F-021 · F4 judged the with-vs-without table by the before/after shape
  (tooling, builder, 2026-10-18, range 007 attempt 2).** The check read every
  table line after the before/after header, so the attempt-vs-rerun table
  failed the eight-column rule. **Fixed** (builder, `f061aab65`): each Finding
  table is checked by its own shape (REPORT_FORMAT.md §5 Findings items 3–4).
- **F-022 · a mount listed every Branch to find one (product, 009.a1,
  2026-10-18).** `layerfs-api` `mount_branch` paged `project.list_branches` and
  searched the items for the name, so a one-shot mount's VM steps grew with the
  Branches (about 7 per range). **Fixed** (daemon `58a60046`, addon
  `799e3ca6`, kind 1): `project.list_branches` takes `name` and answers from
  the index;
  [fixes/009-a1-mount-branch-lookup.md](fixes/009-a1-mount-branch-lookup.md).
- **F-025 · Commits queue behind in-lane merges whose path search walks the
  tree (product, owner direction 2026-10-18, from 007.a2's `fs.write` ≤512 KiB
  watch).** 20–30% of fs.* Commits merge at publish inside the lane, and
  `merge.paths` names changed inodes with two breadth-first walks from the
  root (`merge/decide.rs:147-148`), so its cost grows with the tree's
  directories (p50 28.6 → 56.1 ms from r9a2 to r11 on `58a60046`). Merged
  writes run 240 ms p50 at r11, and unmerged writes queued behind them reach
  271 ms p95. Owner: the fixer, now; fix 1 (top-down path naming) first,
  then lane narrowing, reads off the writer queue, and fixed per-call costs.
  Kind 1. The thin-baseline gate question goes to F-024 and does not close
  this. [fixes/007-a2-f025-merge-in-lane.md](fixes/007-a2-f025-merge-in-lane.md).
  **Fix 0 done** (daemon `bd6fbeee`, diagnostic): the untraced gap is the
  Store writer's admission queue. On a range-11 fork with six concurrent
  writers, 161 of a write's ~208 ms p50 is `wait.writer`: 100 ms in
  construct, 29 ms in the private Workspace's serial reservation (the gap),
  31 ms in producer, begin and lane.
  Owner 2026-10-18: one database, one writer, short holds is the design;
  fix 1 and fix 4 go ahead, fixes 2–3 are deferred; judged on the uncontended
  and contended series (SPEC §5).
  **Fix 1 done** (`3dff9c5cb`, merged `9815dbc95`; daemon `57141b57`, addon
  `438bae05`): changed inodes are named from the recorded bindings, not
  by walking the tree. On the same range-14 input, measured once per arm:
  - in-Commit merge p50 fell from 49.5 to 3.5 ms;
  - merged-Commit lane hold p50 fell from 51.6 to 6.3 ms;
  - merged-Commit p50 fell from 242 to 30 ms;
  - fs.write p95 fell from 428 to 42 ms;
  - the uncontended series is unchanged.
  All 35 recorded `branch.merge`s replay to the root they recorded, on both
  addons. Fix 4 is next.
  **Fix 4 done** (`04d07a293`; daemon `128a959c`, addon `74d08425`). An SDK
  file call reuses an idle reader, the Store's idle producer and pooled
  serials (4,096 per catalog reservation). Mounted Workspaces are unchanged.
  `fs.write` p50 went from 8.04 to 6.79 ms (create) and from 8.60 to 7.07 ms
  (edit). Merge naming counters were added: 0 of 35 recorded merges fall
  back, and per-inode naming cost is flat.
- **F-026 · a listing re-read the inode table per entry (product, 011.a1,
  2026-10-18).** `fs.list` looked each entry up separately from the inode
  table's root, so the cost per entry grew with the table (`list_largest`
  16.6 → 24.3 statements per entry, ranges 9 → 11). **Fixed** (daemon
  `0cc2a732`, kind 1): one memo per call, and each page's entries looked up
  together; [fixes/011-a1-list-entry-lookups.md](fixes/011-a1-list-entry-lookups.md).
- **F-028 · an apply step rewrote files it does not change (harness, lead,
  2026-10-18).** The Sandbox apply script rewrote and chmodded every regular
  file. That sets mtime, which the root records, so an all-identical apply
  (first r012.s0021; 22 in plans 0–98) would Commit where UpToDate is
  expected. **Fixed** (harness, owner-approved): a regular file whose blob and
  mode equal the lane's tree is not sent, rewritten or chmodded. `sandboxExpect`
  is unchanged. Whether mtime alone is a change stays with the owner;
  [fixes/012-pre-f028-apply-identical-files.md](fixes/012-pre-f028-apply-identical-files.md).
- **F-029 · every SDK call on a Sandbox refreshes discovery for all its
  mounts (product, liaison during 013.a1, 2026-10-18).** `registry::sandbox()`
  calls `discovery::refresh()` (`sdk/js/native/src/discovery.rs:411`), which
  reads the Sandbox's record folder and sends one daemon `workspace.status`
  per mounted Workspace before every call, exec included. Traced exec p50 is
  49.6 / 57.0 / 70.9 / 107.0 ms at 1 / 4 / 8 / 16 mounts in one Sandbox
  (untraced 46.3 → 51.6). It was not 013.a1's cause (the probe Sandbox has one
  mount), but it grows when agents share a Sandbox. Candidate fix: exec takes
  its mount's directory from the in-memory mount table, or refreshes only the
  named mount. Owner: the fixer, after F-027, together with Engine connection
  reuse. Exec launch itself, a fresh Docker exec per call (three Engine
  exchanges with `Connection: close`, then dockerd → containerd → runc), is
  the Sandbox's cost. A long-lived launcher is the owner's call.
  Owner read (2026-10-18): the growth is a reasonable range for 16 mounts.
  Untraced it is +5 ms (46.3 → 51.6, about 0.35 ms per mount), so this is
  low-priority cleanup at the end of the fixer's queue, not a performance
  problem.
  **Fixed** (`3e632716a`, addon `2e0c53df`, daemon unchanged). A call that
  names a mount takes it from the process's table, checks its record with
  one `stat`, and makes no daemon call. Close, unmount and the process calls
  still refresh in full. One sample per arm, old against new (p50):
  - 16 concurrent traced calls across 16 mounts: 845.9 → 56.0 ms;
  - sequential exec at 16 mounts: 109 → 19 ms.
  [fixes/013-a1-f029-sandbox-call-discovery.md](fixes/013-a1-f029-sandbox-call-discovery.md).
  **Engine connection reuse done** (`0126695f9`, addon `604df2b2`, daemon
  unchanged). Exec create and inspect share one kept-alive Engine
  connection; Start keeps its own Upgrade. Over 30 `exec true` calls, one
  sample per arm, p50/p90 went from 43.9/48.6 to 35.1/46.8 ms.
  [fixes/013-a1-engine-connection-reuse.md](fixes/013-a1-engine-connection-reuse.md).
- **F-030 · `daemon[].fuse.served` on traced exec lines is cumulative
  (tooling, liaison, 2026-10-18).** It rose 35 → 248 across calls, including
  `true` calls that make no requests. Readers must take differences per
  Workspace. Owner: the builder checks the `oneshot_exec` probe split
  (08de4f038) before range 13's rerun.
- **F-031 · the replay's Workspaces have no `.git` (harness coverage gap,
  liaison observation, builder check, 2026-10-18).** `git.mjs` checks out
  with `git --git-dir <bare> --work-tree`, so Init never saw a `.git`. The
  player never runs git, and SPEC does not mention it. AGENTS says a Workspace
  includes `.git` and its index. **Owner decision 2026-10-18 (replaces the
  lead's depth-1 plan, which stopped before any edit; corrected the same day):**
  from P3, a seed holding the full history up to range 15's start commit only
  (nothing later; proved by `git rev-list --all`; 1,545 commits, 9.35 MiB pack),
  checked out at the range start, and one `git commit` per
  replayed dsh commit with the original message, author and dates, so HEAD
  equals the replayed id. The proof adds HEAD = the checkpoint commit and a
  clean `git status`. See [SPEC §2, `.git` in Workspaces](SPEC.md). Owner: the
  builder, after the P3 dev checks, measuring Store growth per git commit
  (index, loose objects, packs) and the wall time added per range.
  **Design gate (owner, 2026-10-18): F-031 is not built until a short design is approved by the owner.** It must say (1) how lanes commit: which Workspace and Branch runs each `git commit`, and on which Git branch; (2) how a LayerFS merge of two Branches that both changed `.git` treats `.git/index`, `HEAD`, refs, `logs/`, `ORIG_HEAD` and packs written by auto-repack (conflict, rule or rebuild, with no retry or guessed resolution); (3) how dsh's merge commits are reproduced with their original parents (both parents' objects in one `.git`). The design names its proof and its expected Store and wall-time cost. **Owner decision 2026-10-18: design approved with a revision** ([F031_DESIGN.md](F031_DESIGN.md) `4b7a3ec1a`). Per replayed dsh commit, in its lane's one-shot Workspace: a lane-private index (`GIT_INDEX_FILE` under `.git/lanes/<lane>/`) set to the dsh parent with `git read-tree`, real `git add` / `git rm --cached` of the commit's changed paths, real `git write-tree` (tree ids must equal dsh), then `git commit-tree` with the original author, committer, dates, message and parents (the id must equal dsh; no commit after the seed is signed) and `git update-ref` of the lane's own ref. The lead's end-of-range exec is as designed (merge commits, main, shared index, one explicit `repack -d`). Auto-gc stays off, recorded as a departure from git defaults. A fixed share of one-shot calls runs `git status --porcelain` (agents do), which also checks that fresh mounts keep stat fields stable. Storage cost accepted; `.git` bytes are reported separately by source. Proof on a dev range first, adding stored bytes per index rewrite and `git status` time in a fresh one-shot Workspace. **Owner decision 2026-10-18, option (c), superseding the lane-commit part above:** the frozen lane plan lets only 10 of 1,346 dsh commits (ranges 15–20) be committed whole in a lane, so lane commits no longer reproduce dsh ids. Each lane step that changes tracked files runs real `git add` / `git rm --cached` and `git commit` of its own changes, with the lane's private index and its own ref (`refs/heads/lanes/<lane>`); those commits get new ids. At range end the lead writes main's history exactly as dsh has it (trees and commits with the original metadata and parents, real git), so HEAD equals the checkpoint commit; proof: the checkpoint's tree id equals dsh's, and `rev-parse HEAD` equals the checkpoint. Lane commits' objects are reported separately in the `.git` storage split; lane refs are deleted at range end, as designed. The frozen plan is unchanged. **Native baseline (2026-10-19, [GIT_RULE.md](GIT_RULE.md)):** range 15's git work without LayerFS, in the runtime image: lane git exec p50 26.5 ms (123 execs, 4.0 s), lead 5.7 s, every id equal to dsh's. Expected on LayerFS: lane exec about 150–250 ms p50, lead 10–20 s. Lane commits stay (Q7 declined), expected at lanes × dsh commits × 0.3–0.7. Finding for the builder: `statusProbe`'s `@@sumLstat` reads only `refresh/sum_lstat` and misses git's preload lstats; add `preload/sum_lstat`.
- **F-032 · deleting a base tree over FUSE costs ~0.7 ms per entry (product,
  builder, item 9 dev range, 2026-10-18).** On dev fork `dev352-p3f`, range 15,
  the clean reinstall's `rm -rf node_modules` made 40,145 unlinks and 6,187
  rmdirs, plus lookup +43,951, readdir +18,014, opendir +12,307 and forget
  +45,966 against the first install. Inferred time is about 31.6 s, since the
  install's own create and write counts are identical (38,155 and 42,483); there
  is no per-op timing. The reinstall exec took 44.7 s against 13.0 s. A native
  `rm -rf` of the same tree takes about a second. Clean installs run only at
  milestones in measured ranges, but agents' `rm -rf` of large trees is
  ordinary work. Owner: the fixer, after item 7, with per-op timing first
  (D0). Not folded into F-027.
  **Owner decision 2026-10-18:** fixed before measured range 15 (moved ahead
  of measured P3; per-op timing first).
  **GitHub #356 (owner, 2026-10-18):** generic, exact POSIX semantics.
  D0 per-op timing: 16,460 ms against 323 ms native (51×); lookup 4.06 s,
  unlink 2.25 s, getattr 1.68 s, forget 1.41 s; ~4.3 s outside callbacks.
  With `default_permissions` the kernel sends one parent GETATTR per delete;
  READDIRPLUS can remove the per-entry LOOKUP; ~3 requests per entry at
  ~19 µs per round trip floor at ~2.6 s. **Owner decision (revised target):**
  keep `default_permissions` (no permission checks moved into the daemon);
  `rm -rf` of this tree in about 3 s, down from 16.5 s, with the same
  per-entry cost in small and large folders; no slower than creating the
  same tree; about 10× native instead of 51×.
- **F-033 · `MAXIMUM_LINEAGE_ROWS` (4,096) refuses history walks with
  `Capacity` (product, liaison research, owner decision 2026-10-18).** The
  constant (`layerfs-history/src/contract/records.rs:24`) bounds three walks,
  each failing `Capacity("lineage work")` past 4,096 rows:
  (a) Commits since a Branch's base Layer, for publish and merge
  (`layerfs-persistence/src/history/ancestry.rs:70, :220`); the replay's
  forecast puts ranges 98, 112 and 129 over it with F-031;
  (b) Layers between two sides (`ancestry.rs:105, :125`,
  `layerstack.rs:389`), which grows with history: a Branch forked more than
  4,096 publishes ago cannot merge;
  (c) a Commit's first-parent depth (`commit.rs:68`), for fork-from-Commit
  (`branch.rs:185`) and Commit start checks.
  Finding 002 (`3f127ee3e`) removed the walks' growth with total history but
  kept the bound; v0.1.7 kept it deliberately (validation H02); it is not in
  the declared limits ([06-limits](../../architecture/06-limits.md)).
  **Owner decision:** none of the three walks may fail `Capacity` on history
  or Commit counts; only resource limits (disk, memory as admitted) remain.
  Answers stay exact (no guessed or negative answer for an unfinished walk),
  and the publication-lane work rule still holds (architecture 83): a fix may
  not move history-sized work into the lane. Tests that assert the 4,096
  refusal (v0.1.7 H02 and any successor) are deleted or replaced by tests of
  the new behaviour. Evidence first: the scale-experiments session
  (`dsh-352-scale-experiments`) confirms each case (a)–(c) with a bounded
  test. **Owner: the fixer, queued after that confirmation**; the lead
  places it in the item order. See
  [crazy_work_load_watch_out.md](crazy_work_load_watch_out.md) S1.
  **Moved out of the fixer's queue (owner, 2026-10-18):** GitHub issue #353,
  worked now in its own worktree session on branch `issue353-lineage-cap`
  (design note first, `core/docs/issues/353/DESIGN.md`); merging into this
  branch is a later owner decision. The before-range-90 gate stays.
- **F-034 · a fresh mount path per mount leaves absolute paths stale (product,
  builder, P3 dev range r016, 2026-10-18).** `sandbox.mount` takes no path, and
  the daemon picks a fresh directory for each mount and never reuses one
  (`layerfs-daemon/src/control/serving.rs:31`). Tools that record absolute paths
  in the tree keep the previous mount's root after a remount. pnpm's
  `node_modules/.bin` shims and workspace state are examples; so would be a Python
  venv, or any config holding `$PWD`. In `dev352-p3h`, r016's install on top of
  r015's committed `node_modules` left untouched shims pointing at `/workspaces/23`,
  a path that no longer exists. Per-tool-call mounting is the expected common case
  (AGENTS), so this affects ordinary agent work. **Owner decision needed:** a stable
  path per Workspace identity (for example, a mount path the caller names, or a
  stable bind such as `/workspace` inside the Sandbox onto the fresh directory),
  or accept it and document it. Until then dev runs count stale roots as their
  own category, and measured P3 does not start with it open.
  **Owner decision 2026-10-18: by design, no product change.** Each mount gets
  a new directory; that is the correct model. Absolute paths a tool records
  in the tree name the mount that ran it, as when a checkout is moved; a tool
  that needs them current is re-run in the new mount. Consequences for the
  replay: (1) stale roots are expected state, not a finding; (2) the oracle
  for an install on top of a previous one replays the same sequence natively
  and substitutes each recorded mount root by exact string (one listed entry
  per root, counted per install, INSTALLS.md), and any other difference stays
  a mismatch; (3) dev runs keep running one affected `.bin` entry, recording
  whether it works, as a fact about the model (not a gate); (4) the merge
  rule for `node_modules` state changed on two sides (watch-out S4) is still
  open. F-034 no longer blocks measured P3.
- **F-035 · fs read calls scan one byte at a time and re-walk pages (product,
  lead's `grep_tree` diagnosis plus liaison code read, 2026-10-18).** One
  whole-tree `fs.grep` (49.7k paths, 427 MB) takes ~9.3 s, ~46 MB/s:
  `window::lines` reads 16 KiB windows and runs a per-byte loop (pattern state
  machine, line count, prefix copy); `fs.read` shares the loop; paging in
  `grep`, `glob` and `list` re-walks from the start (O(R²/k)); `fs.glob` walks
  the whole tree whatever the pattern. **Owner decision:** optimize the calls
  first; the proposed `fs.grep` exclude-patterns option is dropped. Generic
  only (no special-cased paths), results byte-identical to today.
  [fixes/015-pre-f035-fs-read-calls.md](fixes/015-pre-f035-fs-read-calls.md),
  GitHub #354, worked in its own worktree on `issue354-fs-read-calls`. Until it
  lands `grep_tree` stays the whole-tree series.
  **Scope extended (owner, 2026-10-18):** #354 also optimizes the Store read
  path (delta-chain resolution, ~28 SQL statements per object fetch,
  `FileView::open`); no delta-depth or storage policy change. See the fix
  record.

- **F-036 · concurrent first installs from two daemons: one Commit fails
  "object … is not stored" (product, correctness, scale-experiments S4,
  2026-10-18).** Two daemons (Sandboxes a and b) committed a first-ever
  `node_modules` install into the same Store at the same time. One Commit
  succeeded (88.4 MB new stored); the other failed after 15.7 s in phase
  `Commit Construct` with `object e8775b327467431049f20b8e2fa215332c9f380ab02c13e79ac914560af7ea37
  is not stored`. Its Workspace stays mounted with 256 uncommitted changes
  (unmount refused `Dirty`). A run from a Store that already held the
  `node_modules` content did not fail. Likely a cross-process race between
  two Saves sharing new identical objects (inferred, not diagnosed). State is
  frozen for diagnosis: volume `layerfs-issue352-scale-s4`, container
  `layerfs-issue352-scale-s4-b`, evidence `~/lfs-history/scale-experiments/s4/lww/`
  (REPRO.md to follow). Concurrent installs are expected in P3 (lanes on a
  4-Sandbox pool), so this blocks measured P3. **Owner decision
  (2026-10-18):** the fixer takes F-036 right after F-032; until then it only
  reads source, logs and REPRO-F036.md, with no product change; frozen-volume
  access goes through the lead. Measured P3 waits for it.
  **Fixed (correctness lane, 2026-10-18, branch
  `claude/issue352-correctness-644bbc`, unmerged):** cause proved. A Save's
  per-wave "absent" cache answered a delta base that the other writer
  published after the negative was taken, while that writer's PREFIX chain was
  walked: `ObjectMissing(base)`. e8775b32 is a base of 11 of daemon a's chains;
  daemon b's 1,610 sealed packs are the other block. Base lookups now re-ask.
  A deterministic test fails without the fix. Record:
  [fixes/c01-f036-stale-negative-base.md](fixes/c01-f036-stale-negative-base.md).
- **Limits audit findings (owner decision 2026-10-18).** From the limits audit
  (branch `dsh-352-limits-audit`, `core/docs/issues/352/LIMITS_AUDIT.md` §1,
  `17148cacd`). The F-033 rule applies to each: no `Capacity` or `Busy` on a
  count or a wait that is not a resource limit; only resource limits remain;
  answers stay exact. Owner of each fix: assigned by the owner.
  - **F-037 · merge path cap (audit A2).** `MAXIMUM_MERGE_PATHS` = 262,144
    (`layerfs-content/src/merge/decide.rs:32`, enforced `:176-182`,
    `:211-214`, `:220-221`): a merge with more candidate paths, or a path
    search listing that many directories, fails `Capacity`
    (`merge.locate`/`merge.paths`). Reachable with `node_modules` plus `.git`
    on both sides.
  - **F-038 · merge result build budget (audit A3, not yet test-confirmed).**
    `merge/build.rs:172,177` builds the merged root on the resident route
    with the default 64 MiB ordering budget: past 65,536 topology entries or
    4.19M declared rows it fails `ObjectLimitExceeded`
    (`filesystem/input.rs:76-85`, `validate/backed.rs:119-125`). Workspace
    Commit construction uses the backed route and is not affected. First
    step: a bounded merge test moving a directory with more than 65,536
    descendants.
  - **F-039 · `/.lww` includes past 16 are silently ignored (audit A7,
    correctness).** `MAXIMUM_RULE_FILES` = 16 (`merge/lww.rs:18, :72`): later
    `%include` files are dropped with no error, so a merge applies a partial
    rule set. The replay's recursive `/.lww` is one file today, but any
    caller using includes can hit it.
  - **F-040 · 2 s writer admission window (audit A8; watch-out S2, S3).**
    `WINDOW` = 2 s (`layerfs-persistence/src/backend/sqlite/admission.rs:12`;
    `transaction.rs:146, :194`; `seal.rs:23`; `lane.rs:5`; Linux writer lock
    `writer_admission.rs:41-55`): a writer queued longer than 2 s fails
    `Busy`; a mount's inode serial refill turns it into `EAGAIN`
    (`layerfs-daemon/src/service/filesystem_port.rs:388-398`). A time limit
    on correct, queued work.
    **Status: FIXED** (branch `f040-writer-admission`, off
    `dsh-history-replay` `0609aaeaa`). Admission is an untimed FIFO: a
    queued writer blocks on the queue's condition variable, and on Linux in
    one blocking `flock` after its turn, with no sleep-poll and no retry.
    `WINDOW` and the `WAITERS` = 1,024 count (audit A11) are removed: each
    queued entry is a blocked thread. The named exits without the writer
    are `Closed` (new `Handles::close`), `Cancelled` (a signal interrupts
    the kernel wait), `Integrity` (the thread already holds its own turn)
    and `Unknown` (poisoned queue). The serial refill now queues; its
    `EAGAIN` remains only for an exact SQLite `Busy` from a writer outside
    the cooperative lock. Tests written first (`c1502c9b3`) failed `Busy`
    on macOS and Linux.
    [fixes/038-f040-untimed-writer-admission.md](fixes/038-f040-untimed-writer-admission.md).
  - **F-041 · merge report says `truncated: false` when it was truncated
    (correctness).** `WRITTEN_PATHS` = 4,096
    (`layerfs-store-service/src/store/commit_merge.rs:20, :141`) truncates
    the written-path list; `root_changes`' `truncated` flag is dropped, and
    `layerfs-daemon/src/control/operations.rs:39-44` reports `false`. Fix: the
    flag is carried through, or the list is returned paged with no cap.
  - **F-037, F-038 and F-041 are grouped in GitHub #355 (merge at scale) with
    watch-out S2 risk 1, deferred by the owner as DEFER.md D-6.** F-039 and
    F-040 stay unassigned. Reopened 2026-10-18 (owner): #355 is implemented
    in the #353 worktree on #353's branch, separate commits, unmerged until
    the owner calls integration.
- **F-042 · exec over committed content is slow: vitest 3.7×, `tsc -b` 1.5×
  native; base reads grow with directory size (product, P3 dev range 15 and
  the #356 flatness probe, 2026-10-18).** RCA first (per-op counts and times,
  `ENOENT` lookups, cache hits, statements per op, cold vs steady, strace
  native vs FUSE), then fix the top causes. **Owner: the #352 lead and fixer**;
  the lead places it. GitHub #357,
  [fixes/015-pre-f042-exec-base-reads.md](fixes/015-pre-f042-exec-base-reads.md).
  Builds on #356's cache change and #354's read-path work.

- **F-043 · the replay runner's process holds 1.5–1.9 GB of native memory
  (product, SDK in-process Store/addon; builder dev ranges, 2026-10-18).**
  Peak RSS per fast dev range (dev352-p3p): 1,874 MB (r015), 1,712 MB (r016),
  1,548 MB (r017), still resident at range end. JS heap 16–52 MB used and
  Buffers 17–39 MB, so over 1.4 GB is native: the SDK's in-process Store
  (F-020 direct route) and addon, not tool data. It caused the dev352-p3j OOM
  kill (Docker VM 4.0 GB; kernel "Killed process … anon-rss 1,251,740 kB").
  Next diagnosis: a heap profile of the native addon (which allocator, which
  caches, whether it grows with calls or Stores). No owner assigned; the
  owner assigns.
  **Split recorded (correctness lane, 2026-10-18, before the fix):** in-process
  probe of the runner's call mix on a range-14 Store copy, base addon
  `413ae60d`: RSS 1,246 MB = anonymous 1,198 MB + file-backed 48 MB (binaries;
  the Store has `mmap_size=0`, so no SQLite file or WAL mapping); JS heap 6 MB.
  It plateaus per process (1,222 MB at range 0, 1,437 MB at p3p), so it does
  not grow per range. **Cause:** each Save allocates a zero-filled 16 MiB zstd
  encode arena, which glibc keeps in the arena of each pool thread (dynamic mmap
  threshold). **Fixed** (branch `claude/issue352-correctness-644bbc`, unmerged):
  arenas are reused: 281 MB, flat over 3,600 writes. Diagnostics, not the
  fix: `MALLOC_ARENA_MAX=1` cuts per-write growth from 21 MB to 0.4 MB, and a
  fixed `MALLOC_MMAP_THRESHOLD_` cuts it to 2 MB.

  **Reworked to the liaison ruling** (`926543b10`): plain encode regions,
  bounded to one per CPU, no new `unsafe`, decode reuse held. Real fast dev
  ranges (c43, base → fix, runner peak):
  - r15: 1,444 → 445 MB and 1,287 → 274 MB;
  - r16: 1,398 → 462 MB;
  - file-backed 44–49 MB throughout.

  **Decode regions as well** (liaison approval): reused regions are zeroed
  on hand-out, because zstd's static decoder init does not clear its own
  struct. Range-14 probe: 286 / 238 MB, against encode-only 363 / 315 MB.

  **Follow-up:** release the idle regions when the Store closes. Wire it after
  integration, once `store/handle.rs` is free.

  Record:
  [fixes/c03-f043-runner-memory.md](fixes/c03-f043-runner-memory.md).

- **F-044 · a kernel reply computed before a merged install can land after
  its invalidation (product, FUSE coherence; #359 D-9 sizing, 2026-10-18).**
  - A merged install sends `inval_inode` / `inval_entry` for what changed
    (store-service `commit_merge.rs`, `stale_after_install`).
  - A LOOKUP, GETATTR or READDIR reply computed from the old view, but
    delivered after that invalidation, is not fenced. The kernel caches the
    stale answer.
  - Entry and attribute replies carry a TTL, so their staleness is bounded by
    it.
  - A listing cached with `FOPEN_CACHE_DIR` (owner decision D-9) has no TTL.
    The kernel's `iversion` check covers only changes made through the
    mount, not installs. So after D-9 a stale listing could last until the
    directory's next change, `inval_inode` or page reclaim.

  Not observed in any sample or test; found by reading the order of reply
  and invalidation. Needs a fence before D-9 is called exact: for example, a
  per-mount install epoch checked before a listing reply is sent, with the
  invalidation sent again when the epoch moved.
  **Owner: #359** (D-9 lane).
  [fixes/021-359-d9-sizing.md](fixes/021-359-d9-sizing.md).

- **F-045 · hard-linked files are never entry-cached on the pinned image's
  kernel (host behaviour, not LayerFS; #359 batch 4, 2026-10-18).**
  - On `6.12.76-linuxkit` (Docker Desktop), a non-directory with
    `nlink > 1` gets dentry time 0 on every LOOKUP reply, whatever entry TTL
    is sent (7 s, 60 s and 300 s were tried).
  - So every path resolution of a hard-linked file is a FUSE LOOKUP.
  - Vanilla 6.12.76 `fs/fuse` has no such rule, and fuser 0.18 forwards the
    TTL unchanged.
  - Traced with a private ftrace instance and kprobes, removed afterwards
    (`~/lfs-history/runs/issue359/batch4/trace*.sh`).

  It matters for hard-link-heavy workloads (pnpm's store). Confirm or exclude
  it on a vanilla kernel before attributing pnpm costs to LayerFS. No product
  change.
  [fixes/018-359-b4-readdirplus.md](fixes/018-359-b4-readdirplus.md).

- **F-046 · a terminal unmount in two mount tests fails intermittently
  (test harness; #359 §4.8 test runs, 2026-10-18; fixed 2026-10-19).**
  - `tests/support/mounted.rs:133` (`unmount`, `try_unmount(..).unwrap()`)
    panicked twice, in different targets. Both runs were on `4db0ceb9e`'s
    test build in the pinned image, at host load 4.7–8:
    - `mounted_commit`
      `r5_9_stores_through_shared_mappings_are_committed_once_written_back`,
      in a run where `r5_8` panicked in the same process;
    - `mounted_cycles`
      `a_caller_blocked_on_a_full_pipe_neither_blocks_commit_nor_is_touched_by_unmount`.
  - **The error text was not captured.** The run script kept only the panic
    line, and it now keeps the two lines after it.
  - Each test passed on one diagnostic rerun, and the next full runs of both
    targets passed. That does not close the finding and is not a pass.
  - Both tests leave a process or a dirty shared mapping alive across the
    unmount. The candidate is a refused or after-failure unmount
    (`Failure::Rejected` / `Failure::After`) racing that process, not a
    stale-share or build effect.

  - **Third occurrence, error text captured** (`89a2aa480` test build,
    2026-10-19, host load 4.6–5.6). `mounted_commit`
    `r5_3_commit_captures_what_every_launch_published_whatever_became_of_the_process`:
    `Kernel(KernelFailure { code: Busy, phase: "unmount:kernel", detail:
    "kernel reported the mount busy; no terminal effect" })`. This is
    `Detach::Busy` in `control/detach.rs`: the plain (not lazy) unmount got
    EBUSY, and the mount was returned to the registry unchanged. So the
    product refused correctly, and the kernel held a real reference into the
    mount at that moment. It passed on one diagnostic rerun run alone (no
    sibling test threads).
  - **Candidate (inferred from source, not proven): a test-harness fork
    race.** Every `bash` in the test is waited for before `unmount`. But
    `support/mounted_commit.rs` `command()` sets `uid`/`gid`, so std's
    `Command` forks rather than using `posix_spawn`. A fork in another test
    thread of the same process copies the whole descriptor table, including
    a mount file this test holds open for a moment (`fs::read`, the `host()`
    walk). The child keeps the copy until its `exec`, which under load can
    land after this test's unmount. Whether the same race explains the
    two earlier occurrences is not known.
  - **Fourth occurrence** (2026-10-19, scratch build `75bfde677`, record
    029): `mounted_commit`
    `r5_9_stores_through_shared_mappings_are_committed_once_written_back`,
    with the same `Busy` / `unmount:kernel` text. Not rerun as a pass. The
    liaison (2026-10-19) finds the fork race plausible and leaves the
    finding with its owner, still unassigned.
  - **Fifth and sixth occurrences** (2026-10-19, `issue359-facts-window`,
    record 032), both with the same `Busy` / `unmount:kernel` text at
    `mounted.rs:133`. Neither was rerun as a pass.
    - `mounted_install` `a_directory_of_more_than_one_name_window_is_listed`,
      on `dce9aea51`'s test build. This is the first occurrence outside
      `mounted_commit` and `mounted_cycles`.
    - `mounted_commit` `r5_3`, again, on `1c0cf4b0d`'s test build.


  - **Seventh occurrence** (staging step 8): `kernel_coherence`
    `mutations_racing_the_unmount_probe_keep_normal_results_and_persist`,
    at its final normal unmount, with the same text.
  - **Eighth occurrence** (integration step 11, staging `b2c43386d`, pinned
    image, one container per binary, load 11.1): `mounted_install`
    `a_directory_of_more_than_one_name_window_is_listed`, with the same text
    at `mounted.rs:133`, after all three listings matched (65/64/65). Not
    rerun. That build predates the gate below.
  - **Root cause (test harness, reproduced deterministically 2026-10-19).**
    A fork from any thread of the test process copies the whole descriptor
    table. That holds both for std's fork path (`uid`/`gid` or `pre_exec`)
    and for glibc `posix_spawn`. Until the child's exec closes its
    close-on-exec copies, the child holds every mount file another test
    thread had open at that instant. Libtest runs a binary's tests on
    parallel threads, and every affected binary spawns `bash`, `sh` or
    `sleep` from several tests. So one test's plain unmount can land while
    another test's child still holds this test's file. The kernel's EBUSY is
    then correct, and so is the product's single attempt (`Detach::Busy`).
    The product and the FUSE session open no path in the mount.
  - **Reproduction:** `tests/f046_fork_reference.rs`. Thread A opens a
    mount file. Thread B spawns `true`, whose `pre_exec` waits on a pipe.
    A closes its descriptor and unmounts. One run in the pinned image at
    load 14: `Busy` / `unmount:kernel`, and `/proc/<child>/fd` names
    `mounts/1/held`. After the child is released to exec, the same mount
    unmounts with `Drained`.
  - **Fix (harness only, no product change):** `support/mounted.rs` gets a
    process-wide gate that every `Harness` holds for its whole life, so the
    mounted tests of one binary run one at a time. Nested Harnesses on one
    thread share the gate, and the wait is bounded at 110 s. A test's own
    children are waited for or killed before its unmount. No retry was
    added, and the unmount assertion is unchanged.
  - **Checked once each after the fix** (pinned image, load 21–32):
    `mounted_commit` 9/9 (2 s), `mounted_install` 4/4 (2 s),
    `mounted_cycles` 4/4 (5 s), `kernel_coherence` 6/6 (1 s). One run of
    each is not proof that the intermittent failure is gone. The
    reproduction is the evidence for the cause.

  - **Outside tests** (fix record 036): the daemon never forks, and the SDK
    and runner processes hold no mount file. So F-046 cannot occur in the
    product. Forked copies of Store descriptors cannot affect SQLite
    `fcntl` locks or the writer-lane `flock`. One narrow product window
    remained: the SDK discovery `flock` was released by close, not by
    `LOCK_UN`. It is filed and fixed as F-053.

  **Status: FIXED (test harness).** Tests in the same binary that spawn
  processes without a `Harness` are outside the gate. None of the
  affected binaries has one that runs alongside a mount.
  [fixes/036-f046-fork-copied-mount-file.md](fixes/036-f046-fork-copied-mount-file.md)
  (cause and fix); [fixes/022-359-mtime-only-live-rows.md](fixes/022-359-mtime-only-live-rows.md)
  (first occurrences).

- **F-047 · `fp21_detach_stages` fails 0/2 after the A1 merge (test premise,
  likely; #359 head run, 2026-10-18).**
  - Both cases expire at "the request is inside its Store read"
    (`fp21_detach_stages.rs:95`).
  - The test stages a request inside a provider read by gating the Store
    after a lookup. Under #354 A1, the Workspace reader keeps that lookup's
    objects (`KeptReads`), so the later `open` or listing is served without
    a provider call and never enters the gate.
  - It passed 2/2 before the merge (record 018).
  - The cause is inferred from source, not re-run on a pre-A1 build.
    Product detach behaviour is not shown to be wrong.

  Needs a stage that misses `KeptReads` (a test change). **Owner:
  unassigned** (A1 owner #354 or the lead).
  [fixes/026-359-head-full-sample.md](fixes/026-359-head-full-sample.md).

- **F-048 · an Unmount without `discard` is refused after an mtime-only
  Commit (product; #359 E10, 2026-10-19).**
  - `keep_live_row` (§4.8 option A, record 022, `4db0ceb9e`) counted the
    row it carries into the active generation as one dirty inode.
  - So after a Commit whose only change to a file was its mtime,
    `control/dirty.rs` reported the Workspace dirty. `Unmount { discard:
    false }` answered `Dirty { paths: ["/src/main.rs"], inodes: 1 }`
    although nothing uncommitted remained. §4.8's live time ends at
    unmount.
  - The same count kept c04's UpToDate shortcut from ever applying after
    such a Commit.

  **Status: FIXED** in `9bee52d27` (`issue359-impl`, not pushed): a
  journaled `kept_live` set replaces the count, and every writer of a kept
  serial counts it. Regression test: `mounted_commit`
  `an_unmount_without_discard_succeeds_after_an_mtime_only_commit`. It fails
  on the old code with the `Dirty` reply above.
  [fixes/029-359-e10-kept-live-rows.md](fixes/029-359-e10-kept-live-rows.md).

- **F-049 · `fp31` TooSmall's staged premise races an early kernel FORGET
  (test premise, likely; #359 F-A2 test runs, 2026-10-19).**
  - `fp31_forget_underflow`
    `fp31_a_kernel_forget_of_two_against_a_row_of_one_underflows_and_the_row_keeps_exactly_one`
    failed once on `dace472a3`'s test build (pre-reword `24910c806`). The first of
    two failed expectations was: "the kernel's unit was not retained:
    MountWork { .. retained: 1, completed: 3, served: 4, .. forget_units: 1 }
    -> MountWork { ..identical.. }". The run script did not keep the second
    one.
  - The snapshot taken at staging already counts the FORGET unit as served
    and retained. So the counts the test compares after the refusal cannot
    rise by one. The kernel delivered its FORGET before the test took the
    snapshot that should precede it.
  - Inferred from the two identical snapshots and the source, not proven.
    No product behaviour is shown wrong: the unit was retained and served
    once. It passed on one diagnostic run alone, which is not a pass. The
    other `fp31` case passed in the same run.

  - **Recurrences and candidate cause** (2026-10-19): the TooSmall case
    failed again on `0efe2b772`'s test build, and the Absent case failed
    once on `50b04e1ab`'s, with the same identical-snapshot text. Each
    passed alone or in other runs.
  - **Candidate (inferred from source, not proven):** the two cases run as
    parallel test threads in one container, and each calls
    `reclaim.request()`, which reclaims across the whole cgroup. One case's
    reclaim can therefore make the kernel FORGET the other case's inode
    before that case stages its snapshot.

  **Status: OPEN. Owner: unassigned.** Next step: stage the snapshot before
  the kernel can deliver the FORGET, or wait for the unit by its own count;
  or run the two cases serially. Do not weaken the retained assertion.
  [fixes/032-359-fa2-facts-window.md](fixes/032-359-fa2-facts-window.md).

- **F-050 · a full-case `cost.mjs` run warms the daemon cache for its
  LayerFS arm (harness; #359 F-A2 controls, 2026-10-19).**
  - Without `--part`, a case runs `native.mount` and `native.copy` before
    the `lfs` arm, through the same daemon and Store open. The native copy
    reads the tree through the daemon's `CanonicalCache` (one allowance per
    Store open, `store/open.rs`), so the LayerFS arm starts with a warmed
    and churned cache. A `--part lfs` arm starts cold.
  - Evidence: git commit reads 223 Store readers in the head full run (seq
    349's case set) and **371** in every `--part lfs` arm, including the
    no-window control (seq 585, `caa59784`). Head's status-cold shows 7,488
    evictions; every `--part lfs` arm shows 0.
  - AGENTS §1: an earlier phase's warmth credits a measured phase. Small
    trees are affected most. rm-rf-nm counts per entry are unchanged
    (38.16 hits, 4.21 misses in both), so its LOOKUP counts are not
    confounded, but its timings are not like-for-like.
  - Affected: record 026's head table, which is used as the head arm in
    records 025–032, and #362's baseline comparisons wherever they compare
    a full run against an `lfs`-only run.
  - Harness and run selection only. No product change, and no historical
    verdict is changed.

  **Status: OPEN. Owner: unassigned** (the cost harness owner, or #362).
  Until fixed, compare `--part lfs` arms only with `--part lfs` arms. Next
  step: give each arm its own daemon, or declare the shared cache state per
  arm.
  [fixes/032-359-fa2-facts-window.md](fixes/032-359-fa2-facts-window.md).

- **F-051 · `kernel_coherence` `size_never_shrinks_under_concurrent_append_and_refusals_stay_refused`
  failed once (not yet diagnosed; #359 F-A2 test run, 2026-10-19).**
  - On `51133cdcf`'s test build at host load 15–19, the reader thread
    panicked at `kernel_coherence.rs:459` with "read length: 0 after 1000".
    An `fs::read` of the growing file returned no bytes after a size of
    1000 had been seen.
  - The file is created in the overlay, so its LOOKUP asks no base facts,
    and the read-ahead and window code under test there does not run for
    it. Not attributed to that change.
  - It passed on one diagnostic run alone, which is not a pass. There is no
    earlier recorded failure.

  **Status: OPEN. Owner: unassigned.** Next step: record the reader's
  OPEN/READ replies and the kernel's cached size at the failing step.
  Do not weaken the assertion.
  [fixes/032-359-fa2-facts-window.md](fixes/032-359-fa2-facts-window.md).

- **F-052 · a chmod is lost when the other side edits the same file
  (product; owner, 2026-10-19).**
  - A merge kept the decided entry's metadata whole when both sides' metadata
    differed from the base. A content edit always moves the mtime, so a chmod
    on one side and an edit on the other both "changed the metadata". The
    edit was decided and its base mode won. The chmod was dropped without a
    conflict, on the SDK merge and the Commit merge, for root-form,
    inline-form and hard-linked files.
  - Modes changed differently on both sides were merged silently.

  **Status: FIXED** in `6c66519aa` (`claude/issue353-metadata-merge`, off
  staging `d679c84ee`). Mode and mtime now merge three-way each, and modes
  changed differently on both sides are a `mode` conflict. Regression test:
  `merge_metadata_only` (6 tests); 5 fail on the old code with `mode=644` or
  no conflict. The 35-merge corpus is unchanged (140/140).
  [fixes/035-f052-metadata-only-merge.md](fixes/035-f052-metadata-only-merge.md).

- **F-053 · the SDK discovery lock outlives its Guard across a fork→exec
  window (product; found by the F-046 RCA, filed by the owner 2026-10-19).**
  - `sdk/js/native/src/discovery.rs` `os_lock` takes
    `flock(LOCK_EX|LOCK_NB)` on `<project>/ownership.lock`. `Guard` released
    it only by closing its file.
  - A flock belongs to the open file description. A child forked by the
    host process (Node `child_process`, or any other thread's spawn) while
    a Guard is held shares that description until its exec. So after the
    Guard dropped, the lock stayed held, and another process's discovery
    mutation (project init, Sandbox create or close, exec records) was
    refused `Busy` ("another process owns this discovery mutation"). Busy
    is not retried.
  - Regression test, written first:
    `discovery::tests::a_dropped_guard_releases_the_lock_while_a_forked_child_holds_a_copy`
    (`c8767ac5f`). On the old code it FAILED with exactly that `Busy`.

  **Status: FIXED** (branch `claude/f053-discovery-flock`). The Guard's
  drop now calls `flock(LOCK_UN)` before its file closes. No other flock in
  `sdk/js/native` or `core/crates` relies on close alone: the writer lane
  and the telemetry retention lock use nix `Flock`, whose drop unlocks.
  [fixes/037-f053-discovery-lock-fork.md](fixes/037-f053-discovery-lock-fork.md).

- **F-054 · a Sandbox's PID 1 never reaps orphaned processes (product;
  found by the F-031 dev slice, filed by the owner 2026-10-19).**
  - The Sandbox container ran `layerfs-daemon` as PID 1 with no init.
    Processes orphaned by a `sandbox.exec` command (an Engine exec of
    `bash -c`) were reparented to it and stayed zombies until the Sandbox
    closed. Natively, PID 1 is `sh`, which reaps.
  - Count, the test's own sequence 20 times: **19 of 20** grandchildren were
    left as zombies under PID 1 in a Sandbox, **0 of 20** natively.
  - Exposed test: dsh `packages/bash/bash-local/tests/run.spec.ts`, "kills
    the whole process group (grandchildren die too)". It polls
    `kill(pid, 0)` on the killed grandchild, a zombie answers, and the test
    hits vitest's 5 s timeout. Seen at the slice point
    `r015-1-docs-rfc.end` (P5 stop, 4 of 5 replays; native always passes).
    It is the same leak as #347 BUGS row 30 (290 Chromium zombies).

  **Status: FIXED** (branch `dsh-352-f054-reaper`, `99728208c5`, from
  `sandbox-init-reaper` `fb71dcca4`). Fix kind: a Sandbox container setting,
  HostConfig `"Init": true`, with Docker's init as PID 1 and the daemon its
  child; not a Store format change. The endpoint check requires it, so
  Sandboxes created by an earlier build are refused on reopen and must be
  recreated.
  - Checks:
    - `reap_real.mjs` passes, with PID 1 `docker-init`, and fails on the old binaries.
    - engine_lifecycle and sandbox_operations pass, 7 each, on the host and on Linux; Clippy is clean.
    - The slice point, rerun once on the new binaries: pass. `run.spec.ts` 35/35, the grandchild test 22 ms, 0 zombies under PID 1 after it.
  [fixes/039-f054-sandbox-init-reaper.md](fixes/039-f054-sandbox-init-reaper.md).

- **F-055 · direct_locking_real's oracle assumes an order the test does not
  make (test oracle, not a product defect; filed by the owner 2026-10-19).**
  - Test: `sdk/js/test/verify/direct_locking_real.mjs outer` (#352 F-020
    step 5.1). Eight publishes start in one `Promise.allSettled`, among them
    `daemonA.shared` (Sandbox A commits Workspace `wsA1`, which set
    `/same.txt` to `A`) and `direct.same` (in-process `fs.write("shared",
    "/same.txt", "D\n")`). The oracle asserts that at most one of the two
    publishes (`both writers of /same.txt published`).
  - Run 1 **FAILED** (2026-10-19, branch `issue352-api-shape` at
    `04f2576961`, new Linux addon and daemon): `daemonA.shared` and
    `direct.same` both Committed and every other op as expected, so
    `/same.txt` ends as `D`. Run 2 on the same binaries passed (`direct.same`
    refused `Conflict`, 58 reads during publication none refused, integrity
    ok). Run 2 is a second sample only, not the result.
  - Cause: nothing orders `direct.same`'s bind against daemon A's publish.
    `wsA1` was mounted and written during setup, so A's base is the `shared`
    head from before the publishes. `fs.write` binds its private Workspace
    when its native task runs (libuv pool, alongside a read loop) and takes
    the head at that moment as its base. If it binds before A publishes, one
    side merges against the other and is refused `Conflict` (the recorded
    passes). If A publishes first, `direct.same`'s base already holds
    `/same.txt = A`; its write is a plain overwrite with no merge, and both
    Commits are correct. Run 1's outcomes match the second order.
  - On main as well: `git diff 63ecca0ae6..450f171e11` is empty for
    store-service `store/` and `overlay/` (bind, Commit, merge),
    `layerfs-history`, `layerfs-workspace`, `layerfs-sandbox`, `sdk/js/src`
    and `sdk/js/test`. The moved `write/files.rs` differs from main's
    `sdk/files.rs` only in where arguments are read; bind, change and
    `commit_captured_via` run in the same order. The test is unchanged since
    `373b0357cd`.
  - Options for the owner (not chosen):
    1. The oracle accepts the second order: when `direct.same` bound after
       A's publish, both Committed and `/same.txt = D` are expected (for
       example, compare `direct.same`'s parent with A's Commit).
    2. The test orders the bind explicitly, so `direct.same` always binds
       before daemon A publishes and exactly one of the two is refused
       `Conflict`.

  **Status: FIXED** (branch `issue352-f055-locking-order`, test only).
  daemon A's Commit is released once `direct.same` has settled, so
  `direct.same` binds and publishes first and A's Commit, on its setup base,
  is refused `Conflict` every run. No sleeps. Existing assertions are kept,
  plus one for the ordered pair. The SDK cannot pause an `fs.write` between
  its bind and its Commit, so the refused writer is now always daemon A.
  One run: pass, `direct.same` Committed and `daemonA.shared` refused
  `Conflict`, 66 reads none refused, integrity ok.
  [fixes/040-f055-locking-oracle-order.md](fixes/040-f055-locking-oracle-order.md).

- **F-056 · a build that rewrites identical outputs is redone after every
  remount (product, a finding against design 8; R1 slice `f031-r1-slice-h`
  devcheck, 2026-10-19).**
  - STORAGE_BATCH_SPEC §4 design 8 (owner, 2026-10-18): an mtime-only change
    is not a change, and after a remount the committed mtime is seen. Its
    proof "Builds across a remount" says the second `tsc -b` must report
    nothing to build, or what a native rerun reports. A difference is a
    finding against design 8.
  - Lane step `r015.s0010` edits `tsconfig.base.json` (mtime 1792399609.86).
    The vendor projects' outputs date from the lead's first build
    (`vendor/cosmokit/lib/tsconfig.tsbuildinfo` 1792399582.24), so `tsc -b`
    finds them out of date ("older than input 'tsconfig.base.json'").
  - The repair build rebuilds them. The bytes are identical, and only mtimes
    move (in the mount, tsbuildinfo 1792400139.26 and `lib/index.js`
    1792400143.75). The Commit keeps the base rows, and after a remount the
    old mtimes are back (probe `utimes.json`: before, after, Commit, remount).
  - So `tsc -b --verbose` after the remount builds `vendor/cordis`,
    `cosmokit`, `loader`, `logger-console` and `schemastery` again. A native
    rerun on the same chain finds 77 of 77 up to date
    (`devcheck-oracle-015.json` rebuild states). Every later build in a fresh
    Workspace repeats that work until a real change lands.
  - Receipts: `~/lfs-history/dev/devcheck-r015-20261019/` (`tsc.json`,
    `mt.json`, `dry.json`, `utimes.json`) and the run's
    `metrics/devcheck-015.stop.log`.

  **Status: OPEN, owner decision.** Not fixed: changing design 8 reverses an
  owner decision.

- **F-057 · newest-wins over pnpm's install state lets the repair keep stale
  hoisting (product; measured range 15 a3, `p3-main-c`, 2026-10-19).**
  - The lead's merge decides `node_modules/**` by newest-wins (`/.lww`, owner
    S4). In range 15 the root lane installs a changed lockfile, while every
    other lane refreshes the base inputs at its own mount path. Both rewrite
    `node_modules/.modules.yaml`. A later refresh's copy (base state) can win,
    while `node_modules/.pnpm/lock.yaml`, changed only on the root lane, comes
    from the root lane. The merged install state mixes the two (a2 showed the
    same on `.modules.yaml`).
  - The repair install keeps that mix. In a3 the repaired lead's
    `.modules.yaml` `hoistedDependencies` still lists the stale
    `@cordisjs/plugin-loader@1.0.0-rc.4(cordis@4.0.0-rc.6)` beside
    `@cordisjs/plugin-loader@1.0.0-rc.5(cordis@4.0.0-rc.7)(node-addon-require-builtin@0.1.0)`.
    Natively only rc.5 is listed.
  - Result: the public hoist `node_modules/.pnpm/node_modules/@cordisjs/plugin-loader`
    resolves to the workspace package `vendor/loader` in LayerFS, and to
    `node_modules/.pnpm/@cordisjs+plugin-loader@1.0.0-rc.5_cordis@4.0.0-rc.7_node-addon-require-builtin@0.1.0/node_modules/@cordisjs/plugin-loader`
    natively. It is the only difference among 761 entries of that group, and
    every other group is equal (lister v6). P4 passes: 503 emitted files,
    exit 0.
  - Impact: a package that relies on hoisting (an undeclared dependency
    resolved through `.pnpm/node_modules`) loads a different plugin-loader in
    LayerFS than in a native checkout of the same tree.
  - Proof: `lead.repair` P6 stop, "P1 stop (missing 0, extra 0, differ 1:
    node_modules/.pnpm/node_modules)". Receipts:
    `~/lfs-history/dev/r015-a3-p6-20261019/` (`lfs.out` from the repair
    Commit `12880da1…`, `native.out` from the repair chain, both listing the
    group entry by entry with the `hoistedDependencies` section), and a2's
    `~/lfs-history/dev/r015-a2-p6-20261019/`.
  - Candidate fixes, none chosen: merge pnpm's install-state files
    differently, or make the lead's repair a clean relink of `node_modules`.
    Changing the repair command changes the workload, so it is the owner's
    call (liaison under owner delegation 2026-10-19).

  **Status: OPEN, product finding.** Owner decision 2026-10-19: the workload
  now avoids it. The lead's repair is a clean relink of `node_modules` on the
  merged tree, and native runs the same (NODE_RULE §4). The product fix is
  still owed: lanes still merge their install output by newest-wins, and a
  Workspace that installs incrementally on such a merge keeps the stale
  hoisting.

  **Design approved 2026-10-20 (owner, option B):** `branch.merge` takes
  caller-declared merge units. When both sides changed a unit, the merge
  takes the whole unit from the side with the latest write and reports it.
  See [F057_MERGE_UNITS.md](F057_MERGE_UNITS.md). Not built yet; the build
  follows #364 items 3 and 4.

- **F-058 · the native repair chain ran the lanes on top of each other
  (harness oracle, not a product defect; range 18 a1, `p3-main-g`,
  2026-10-19).**
  - `nodeproof.mjs` `repairChain` built the repair's native oracle in one
    folder, running each lane point's install and build after the previous
    lane's. LayerFS runs each lane on its own fork of main and merges the
    lanes' changes into main. So a later lane's build could write back bytes
    that an earlier lane had changed.
  - In r018 the plan's path lanes split dsh `cc546c4580`, which moved
    `tool-pairing.ts` from `packages/core/session` to `packages/compact/compact`.
    Lane 2 still edits the old source, so its build emitted new
    `core/session/lib/types/tool-pairing.{d.ts,js}`. At the checkpoint the
    source is gone and `tsc -b` leaves the orphan output. LayerFS kept lane 2's
    output; the native folder ended with the older bytes. The repair's P6
    stopped on P4 with "2 emitted file(s) differ".
  - Fix ([fix 042](fixes/042-r018-a1-native-lane-merge.md), owner decision
    2026-10-19, option A): each lane runs from the lanes' base, and its
    changes are captured and applied in the consolidation's merge order
    before the repair. No gate, proof scope or workload change.

  **Status: OPEN (harness).** Range 18 a2 found a silent failure in fix 042
  (links followed; corrected in `029db0950f`). Range 18 a3 ran the merge
  whole and moved the difference to two-sided paths: 19 paths both lanes
  wrote (18 `tsconfig.tsbuildinfo`, `vendor/cordis/bin.js`).
  - The rule the liaison set (later-merged lane wins) took examples' copies.
  - The product's `/.lww` takes the newer write, so packages-core's copies,
    because its build finished last in the play.
  - The native repair build then failed `TS2694` (exit 2, LayerFS 0).
  - Owner decision 2026-10-19 (option 1): the native merge applies the
    lanes' captures in their recorded write order, so the newest writer wins
    as `/.lww` does; two-sided paths stay listed (fix 042). Range 18 a4 plays
    with it.

- **F-059 · lanes' stale build output outlives the repair, and the point
  oracle's base cannot carry it (harness oracle and workload, not a product
  defect; range 19 a1, `p3-main-j`, 2026-10-20).**
  - The repair's build exited 2: `agent-spine-demo` reads
    `Config` from `packages/bash/tool-bash/lib/types/index.d.ts`, and that
    file has none. At the checkpoint, tool-bash's `src/index.ts` exports
    `Config`; lane 3 (`packages-support`) built tool-bash at an older
    side-branch commit without it. Lane 3 was the only lane that wrote the
    emitted file, so the merge kept it. Its `tsconfig.tsbuildinfo` was
    two-sided and the newer write won. The repair's `tsc -b` lists tool-bash
    (its tests changed) but does not rebuild it.
  - The native repair chain (fix 042, write order) gives the same result: tsc
    exit 2 with the same `TS2694`/`TS2339` errors on `Config` (diagnostic run,
    not recorded as a proof; 22 two-sided paths). LayerFS matches its merge model.
  - The lane-2 point's P4 stop (`core/session/lib/types/tool-pairing.{d.ts,js}`)
    has the same cause, one range back. LayerFS's lane 2 starts from r18's
    repaired main, whose `tool-pairing` is the r18 repair's output
    `c233d58c` (r18 a4 P6 equal to native). The point's `tsc -b` finds it up
    to date. The native point oracle's base (`chain0`) is the r18 closure on
    a fresh checkout of the r18 tree, which builds `2352ae25`. That base cannot
    carry a lane's output from an earlier range.
  - Not fixable without a rule decision: the repair's build exit and the
    point base are workload and oracle rules. Sent to the liaison.

  **Status: FIXED (harness and workload, owner decision 2026-10-20, option A).**
  The repair's build first removes `lib/` and every `*.tsbuildinfo` in every
  `tsconfig.json` package and in every `packages/<group>/<name>` folder (orphans
  pruned), then runs dsh's whole build; native does the same
  ([fix 043](fixes/043-r019-a1-repair-build-clean.md), NODE_RULE §4). This is
  an owner-approved workload change. Range 18 replays as a5 so range 19 starts
  from clean output, then range 19 a2. Option B (a carried point base, a
  relaxed build exit) is out.

- **F-060 · the native repair chain never applied the consolidation's
  reconcile (harness oracle, not a product defect; range 19 a3, `p3-main-m`,
  2026-10-20).**
  - After the lanes' merge, LayerFS's lead applies the plan's reconcile:
    range 19 has 217 files, 12 `rm`. That removes the tracked files the lanes
    brought that the checkpoint has not. The native chain (fix 042) only
    deleted what git deletes from the lanes' base tree to the checkpoint.
    Files a lane created and the checkpoint deleted stayed natively:
    `packages/core/agent-core/README.md` and `packages/ui/stdio-agent/{src,tests}`.
  - With fix 043's clean, tsdown took `packages/ui/stdio-agent` as a package
    without `lib/types` natively: `UNRESOLVED_ENTRY`, native exit 1 against
    LayerFS 0. The repair's P6 stopped: "P4 stop (tsc exit 0, native 1)".
  - Fix (fix 045): `repairChain` gives the repair state the reconcile's `rm`
    paths (`remove`), and `nativeChain` removes them before the state.
    `LANE_MERGE_VERSION` 3 re-keys the chains. Natively r19 a3's repair build
    then exits 0 with 721 emitted files, all equal to LayerFS's (diagnostic). No
    gate, scope or workload change.

  **Status: FIXED (harness).** Range 19 a4 plays from `p3-main-l`'s r018
  snapshot.

