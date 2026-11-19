# Decisions applied at the spec freeze (2026-11-18)

The owner delegated the open rulings; the first (recommended) option of each is
decided. Full wording of each ruling and correction is in `rollout-body.html`
(sections "Rulings needed", "Corrections", "Correctness", "Simplicity and
cleanness", "Doctor page"). Every one below is binding on the frozen spec.

## Rulings

- **R1** No aliases for old call or option names, old paging included. No `layerfs doctor` alias.
- **R2** The fix for an outdated worker is `layerfs machine stop <m>` then `layerfs machine start <m>`,
  two commands behind one button; the button is off while Sandboxes run, with the count in its tip.
  `machine start` keeps refusing a mismatched worker.
- **R3** A check that could not be made: a `not_checked` entry carries either `blocked_by` or
  `reason` (`timeout`, `busy` or `lost`). A check that does not apply is left out. The page shows
  one "Could not check" signal with Check as its action.
- **R4** (owner) No doctor takes `--offline`. Opening the Doctor page runs no doctor: it reads
  `home get --with tree`. Records checks run inside `home doctor`; a records finding may name a
  thing at any depth.
- **R5** `layerfs backup doctor <bucket> --full` (not `--with write`). `--with` only ever adds a section.
- **R6** `layerfs home leftovers [--machine M]` is its own call (85 calls in all);
  `layerfs home prune <leftover>` removes one it listed. `home get` has no `--with leftovers`. Superseded by version 20: both calls
  are removed, with no alias, and LayerFS removes leftovers itself.
- **R7** `layerfs workspace cancel <workspace> [--exec E]`: every running command by default.
  `workspace.list` joins the agent profile. A worker that does not answer: the short form answers
  `{"unknown":true}` (decide the exact key in the row and state it).
- **R8** `project doctor --full` reads everything reachable from the head of each of the project's
  Branches (what a mount can reach). Streamed, bounded memory, no writes, no format change. An
  object shared by two projects is read once per project.
- **R9** `store doctor` on a PostgreSQL Store probes from each machine that has a Sandbox of that
  Store. `--machine M` stays for the command line.

## Corrections C1–C14

Apply each as written in `rollout-body.html`. In short: C1 keep `--discard`/`--force` on
`workspace unmount`, `--force` on `workspace commit`, `sandbox remove`, `machine stop`;
C2 `-m` on every call that makes a Commit, `mode` on `file write` and `file mkdir`; C3 rebuild
the "Today" column of the argument table from the code's own table and list every removed
option as dropped; C4 `machine prune <machine> [image] [--unused]`; C5 `machine update` keeps
`--cpus --memory --image --docker --wsl --daemon` (no `--disk`; resizing is not in 2.0);
C6 `machine load-image <machine> <image>` (a Docker reference copied from the host's Docker);
C7 comparing two different paths is dropped with `file.diff`; C8 merging into an existing
Branch is new work, and `--expected` on `branch publish` guards main's head; C9 exit codes 0, 2
and 125 with the refusal on stderr, a doctor exits 0 once its report is made, the refusal code
list completed (adds `InUse`, `Unresolved`, `InvalidInput`, `Unsupported`, `Dirty`, `Failed` to
`NotFound`, `Exists`, `Capacity`, `Conflict`, `Busy`, `Unavailable`, `Unknown`; check the code's
own list and use exactly its set); C10 Workspaces are in the home's records only on `native` and
for a Sandbox reached directly, so `home get --with tree`, `sandbox list` and `workspace list`
say so (a stopped machine's Workspaces are not listed; its Sandboxes answer `blocked`);
C11 one table for state and its companion: `busy` has `op`, `blocked` has `blocked_by`, `broken`
has `reason` and `fix`, `unknown` has `observed_at`, `ready` and `stopped` have none; Docker not
running is `broken` with `reason: engine_down`; every `get` example carries its `state`;
C12 Python spells the option `with_=["live"]`; C13 the partial results are stated: a
`sandbox create --mount` whose mount is refused keeps the Sandbox and names it in the refusal;
`store move` goes to or from this computer only; a lost reply on `machine start` or `stop` is
settled by `machine get --with live`; C14 progress and the shallow Store check are new work;
the project record check is not new (and its finding is a notice with a fix, not an error).

## Simplicity changes adopted

- `--with` is defined once: it adds one named section to an answer. Values: `live`, `stats`,
  `space`, `docker`, `platforms`, `tree`, `patch`. `storage` and `usage` fold into `stats`
  (`project list --with stats`, `sandbox get --with stats`). `project get` takes no `--with`.
  `machine stats` stays a call and keeps `--with space`. `write` is gone (R5), `leftovers` is gone (R6).
- Paging is one convention: `--limit N --cursor C` in, `{"items":[…],"next_cursor":"…"}` out.
  The three log calls take `--cursor` (no `--before`). `file glob` and `file grep` gain `--cursor`.
  Every list call either pages or says in its note why it is bounded. One default limit per kind:
  100 for lists, 200 for logs, 2000 lines for text windows.
- `store connect <store> <url> --auth A` on a Store's first connect; `store connect <store> <url>
  --machine M` adds one machine's own address and refuses `--auth` (a machine never has its own login).
- `backup list [--bucket B]`. `commit diff <project> <ref> [--from R]`: without `--from`, the
  changes since the fork (for a Branch) or since the parent (for a Commit).
- `sandbox.close` is renamed `sandbox.remove`.
- `home.get` answers `generation` (a number that moves when any record changes), not `changes`.
- No default machine recorded on a PostgreSQL Store: the machine is chosen at `sandbox create --machine M`; without it the Sandbox runs on this computer (`native`), as today.
- Examples obey the rules: one key set per call's long form, a `state` on every `get`, no `name`
  for an existing thing, no plural that is a number.

## Open decisions of version 17, decided

- A write to a mounted Branch is allowed; the Workspace's status answers `branch_moved:true`.
- Exit codes: see C9. `workspace exec` exits with the command's code and prints its result JSON on
  stdout; a refusal prints `{"error",…}` on stderr and exits 125.
- `file grep` takes a literal pattern; its help says literal.
- No Workspace file tools. No worker route for browsing a Store this computer cannot reach.
  Transaction-pooling proxies are documented, not detected. No login per machine.
- Blocked projects stay in `project list`, marked `blocked`.
- No raw-text mode for agent reads; JSON stays the default; `--text` stays as today. *Superseded for
  terminals by Version 22: a terminal gets the readable form by default; a pipe, file or program
  (every agent read) still gets JSON.*

## Totals after the freeze

83 calls in 10 families: project 8, file 14, sandbox 7, workspace 7, branch 6, commit 3, home 5,
machine 14, store 13, backup 6. (85 at the freeze, with home 7; version 20 removed `home leftovers`
and `home prune`.)

## Fixes from the exit review

A fresh reviewer checked the frozen spec against the code and found 23 defects. Each is decided
below; the full wording is in `REVIEW-FIXES.md`, beside this file. Where a fix refines a ruling
above (fix 5 refines R2), the fix holds.

- **Fix 1** A `--with live` read writes the record only when the state word or its reason changes,
  and only that write moves `generation`. `observed_at` is kept in an observation file that
  `generation` does not cover. `generation` does not signal the death of an operation's owner. A
  records-only read answers the last written state word and reason, so `engine_down`,
  `not_answering`, `missing` and `unreachable` can appear in a records-only `blocked_by`.
- **Fix 2** Answers carry the full 66-hex Commit id; examples are abbreviated. A prefix that matches
  more than one Commit is refused `InvalidInput` with `matches`. A `<ref>` or `--from R` is a Branch
  name first, then a Commit id or prefix. `commit diff <ref>` of a Commit with no parent compares
  with its Branch's `base`, and with the empty tree when there is none.
- **Fix 3** `project list --with stats` adds `size` only, from the head Layer's recorded length.
  `project stats` keeps its counts from the bounded walk, with `partial`.
- **Fix 4** `workspace.status`: `changes` is the first 20 entries of a bounded read of 256 overlay
  rows; `changes_count` counts the entries read, with `partial:true` at the bound. A renamed or
  removed folder is one entry.
- **Fix 5** An outdated worker's `fix` is `layerfs machine stop <m> && layerfs machine start <m>` when
  no Sandbox is recorded on the machine, and the same with `--force` on the stop when some are; its
  `message` then names the count. The page's button runs only the first form and is off while
  Sandboxes are on the machine, with the tip "N Sandboxes are on it. Copy the fix to stop it
  anyway."; the detail panel's Fix line shows the `--force` command to copy.
- **Fix 6** `sandbox.get` when the machine does not answer: `unknown` with `observed_at`, and
  `workspaces` null. Not `blocked`.
- **Fix 7** `workspaces` is a list of objects everywhere: `[{workspace, branch}]` in list rows and
  tree nodes; `sandbox.get` adds its further keys per Workspace.
- **Fix 8** The refusal fold map gains `NoMatch` to `NotFound`, `AmbiguousMatch` to `InvalidInput`
  (with `matches` and a `fix` adding `--replace-all`), `NoSpace` to `Capacity`, `Timeout` to
  `Unavailable`.
- **Fix 9** An argument may carry `"redact": true`: the call log records its byte count, never its
  value. It is set on every argument today's log redacts, under its new name, and on the new `cwd`,
  `env`, `include`, `exclude` and `stdin`. A `from` that is a path is redacted; a `from` that is a
  Branch is not.
- **Fix 10** A secret is an option of kind `Content`, required and redacted, with `shell` naming the
  shell's stdin flag: `password` (`--password-stdin`) on `store save-auth`, `secret_key`
  (`--key-stdin`) on `backup connect`. The shell reads it from stdin and no other way; Node and
  Python pass the value as the option.
- **Fix 11** `mode` on `file write` and `file mkdir` is kind `Text`, octal (`"0644"`), as answers
  spell it.
- **Fix 12** A Workspace's default name is the Branch name made valid: lowercased, each character
  outside the Workspace name alphabet replaced by `-`. A collision is refused `Exists` with a `fix`
  naming `--name`. `sandbox create --mount B` uses the same default.
- **Fix 13** Pages against rows: tree projects under a Store are bare names; "Backup behind" is a
  notice from `store doctor`, check `store.backups`, on the Store, with `fix`
  `layerfs backup create <store> <bucket>`; the per-entity state table is built from the rows'
  `states` fields (`{reasons, ops}`); a project the Store does not list is hidden in `project list`
  and a notice in `project doctor` and `store doctor`, never "Missing"; removing a machine removes
  its own PostgreSQL addresses, which is new work of `machine remove` (today they stay).
- **Fix 14** The mock uses the frozen check ids, and row facts come only from tree nodes. A tree node
  holds what records hold: a machine `machine, platform, cpus, memory_bytes`; a Store
  `store, engine, projects`, a PostgreSQL Store also `connections`; a Sandbox
  `sandbox, project, image, workspaces`; a bucket `bucket, provider`; each with `state` and its
  companion when not ready.
- **Fix 15** Every doctor row carries `checks`: one closed, ordered list of `{check, what, full_only}`
  per kind of thing, owned by that kind's doctor row (a Workspace's by `sandbox.doctor`). A parent's
  shallow check of a child runs that kind's list without the `full_only` entries. `build.py` collects
  the lists into `calls.json` as `doctor.checks` and refuses an example that names a check outside them.
- **Fix 16** Spellings: `change` is `added`, `modified` or `removed`; a machine's free disk is
  `disk_free_bytes`; a Store's reusable pages are `reusable_bytes`; `home.log` rows name the call as
  `call` and filter with `--call`, and `op` is only ever the state companion; a list that could not
  include everything names what is missing in `not_listed`, `[{<thing>, blocked_by | reason, message}]`;
  a state's `reason` and a `not_checked` entry's `reason` are separate lists.
- **Fix 17** `machine get --with live`: the section does not repeat `observed_at` or `message`.
- **Fix 18** Small rule breaks: `file edit --old-string/--new-string` are required;
  `backup connect <name> …`; `counts` in a doctor's answer is the one plural that is a map;
  `machine stats --with space` answers `[{part, bytes}]`; times inside `last`, `attempt` and
  `last_transfer` end in `_at`; a no-op `machine start` or `stop` answers `{"unchanged":true}`;
  leftover kinds are snake_case.
- **Fix 19** `log_tag` stays a global option: an argument in Node and Python; the shell reads
  `LAYERFS_LOGS_TAG`, a setting of the caller, not call context.
- **Fix 20** A cursor carries its call's filters and, where the call pins a Commit, the pin;
  `project list` and `branch list` cursors carry filters only. A cursor given with different filters
  or pin, or to another call, is refused `InvalidInput`; one whose pinned Commit is gone is refused
  `NotFound`.
- **Fix 21** `backup.list` drops `exported_from` and `binding_match`: reading them is extra I/O.
- **Fix 22** `project doctor --full` writes nothing to the Store. It may keep its visited set in a
  scratch file outside the Store, removed when the call ends, as today's integrity scan does.
- **Fix 23** Lost replies beyond C13: the Store rows say how an uncertain `compact`, `backup` or
  `restore` is cleared, and where nothing clears it today, `store get <store> --with live` settles it
  (new work). `workspace.exec`'s `Unknown` refusal carries `exec`.

Added by the coordinator while the fixes were applied:

- **Cursors in the call log** Every `cursor` option is redacted: a cursor carries its call's filters,
  which may hold a redacted path or pattern. `build.py` sets it in `calls.json`.
- **`refused`** A `not_listed` entry's `reason` is `timeout`, `busy`, `lost` or `refused` (a thing
  that answered and refused, such as a bucket that rejects its key; `message` carries the refusal).
  A doctor's `not_checked` entry stays `timeout`, `busy` or `lost`.
- **`repeat`** An option that may be given more than once is marked `repeat`, on the option or as a
  list on the row (`workspace.exec`: `env`).

## Version 19 (2026-11-19): `layerfs_v2_0_0` #738–#741

The 2.0 rows brought up to `layerfs_v2_0_0` @ `ff114a5265`: GC compaction with D6/D7 (#740),
`store.compact` in the Store's machine worker (#739) and the Sandbox image import hardening
(#738, #741). The plan is `tmp/cs20-v2-sync-plan.md` Parts 2 and 3; where it left a choice open the
recommended option is taken, and each such choice is listed here.

- **V19-1 Image states** The state rule extends to the images a machine holds: a copy that runs or
  stopped partway is a state on its `machine images` item, not a `source` word. Published is `ready`
  (left out of the row); a part a live copy holds is `busy` with `op {name: load_image, phase,
  started_at}`; a part no copy holds, or an unpack that stopped before it was recorded, is `broken`.
  The row carries `states {reasons: [partial], ops: [load_image]}`.
- **V19-2 Reason `partial`** A new broken reason: the thing was left partway by a copy, and its `fix`
  resumes or rebuilds it (`layerfs machine load-image <m> sha256:<hex>`). It is the last reason code.
- **V19-3 One item per image** A stale part beside a published image merges into that image's row,
  which stays `ready` (the image is usable) and shows the part's bytes as `staged_bytes`; `machine
  prune --unused` removes the part. A partial image's `size` is null.
- **V19-4 The busy copy's phase** A copy records its phase (`export`, `send`, `unpack`) in its lock's
  owner record in the home, beside `since`; the busy item's `op.phase` and `op.started_at` come from
  there. A listing reads that record and never takes the copy's lock.
- **V19-5 Not exposed** `source` stays `bundle`, `machine` or `imported`. `staged`, `stranded`,
  `resumable`, `age` (it breaks the `_at` rule), `variant` (always null) and the image's `Env`
  (recorded, never applied) stay out of every 2.0 answer. Applying `Env`/`WorkingDir`/`User`, filling
  or deleting `variant`, and where rebuild temporaries go are the owner's, not this version's.
- **V19-6 `machine load-image`** Short `{image}` with `reused` and `dropped` (non-empty only); long
  adds `dropped` (FIFO and device paths, first 20), `dropped_count` (beside a cut list) and
  `expanded_bytes`. Init files are always dropped and never listed. `size` is the image's tar bytes on
  the machine. Progress `inspect, lock, export, send, unpack`; `lock` is `{phase: lock}`, once a
  second while waiting, with no `waiting` or `holder` (a pid). Refusals: `Capacity` with fix
  `layerfs machine prune <m> --unused` and detail `{needed_bytes, free_bytes, path}`; `Unsupported`
  with `{architecture}` or `{os}`; `InvalidInput` for an ambiguous reference (`{images}`) or an image
  Docker does not have; `Unavailable {what}`; `InUse` for a stranded unpack in use; `Failed` for an
  export that failed or changed, a staged part that does not match, or a frame out of order (the last
  two were `InvalidInput`).
- **V19-7 `machine prune`** `skipped` (`[{image, message}]`, first 20) with `skipped_count`; short
  `{freed_bytes}` with `skipped` when non-empty. `--unused` also removes parts no copy holds (even
  beside an image a Sandbox uses), stranded unpacks, sidecars with no image and `.tmp-*` folders, each
  image in `removed`. A named image a live copy holds is `Busy` with the copy's `op` (was `InUse`); one
  a Sandbox uses is `InUse` with `{sandbox_count}`; a lock check error is `Failed`.
- **V19-8 `sandbox create --image`** Progress `inspect, lock, export, send, unpack, start, mount`; the
  load-image refusals with the same shapes. The copy's lock is held until the create returns, by which
  time the Sandbox's own recorded lower layer protects the image from a prune. The example's `image`
  is an id.
- **V19-9 `home leftovers` / `home prune`** New kind `export_container`, id
  `native:export_container:<container>`: export containers in the host's Docker labelled with this
  home, and legacy `layerfs-export-<m>-<hex>` ones. `report_only` while the copy lock of its (machine,
  image) is held; `modified_at` is its creation time; `path` is null and `size` is not measured.
  `home prune` runs `docker rm -v` under that lock taken without waiting. Docker is asked only once an
  image copy has run from the home (its `run/image-lock/` folder is there). When Docker does not answer,
  `native` goes in `not_listed` with `blocked_by {machine: docker, reason: engine_down}`; a host with
  no Docker installed lists none and names nothing. `image_part` leaves the kinds; a load keeps only
  its own sweep of its (machine, image), and no load sweeps the whole home any more.
- **V19-10 `machine remove`** Also deletes the home's `run/image-lock/<machine>/`.
- **V19-11 `machine doctor`** No new check id. `machine.capacity`'s evidence adds
  `reclaimable_image_bytes` (what `machine prune --unused` frees); its fix is that prune only when
  those bytes exist.
- **V19-12 `store compact`** Note: a sqlite Store on another machine is compacted by that machine's
  worker and starts no machine. Long keys unchanged; on SQLite `dead_bytes` includes dead records in
  packs the run keeps, `unchanged` means `dead_bytes` under 10% of the Store, `freed_bytes` can exceed
  the wholly dead packs, and on a `compacted` run `kept_waste` describes the rebuilt Store. Progress
  `mark, build, verify, install` on SQLite (`unchanged`: `mark` only; v18's `cleanup` had no event),
  `mark, build, swap, cleanup` on PostgreSQL; the worker route forwards the same events. The refusal
  table is the row's `refusals`; the worker's own `Busy` says "on that machine".
- **V19-13 `store get --with live` and `store doctor`** The live read reads the worker's compaction
  record through the machine; a run live there turns `broken`/`unresolved` into `busy` with its phase;
  `blocked` comes first while the machine is stopped. `store.operation` reads it the same way; on a
  stopped machine it is `not_checked` with `blocked_by`.

- **V19-12a `live.phases`** (coordinator ruling after batch 4, applied by package Z) `store get
  --with live`'s `live.phases` lists the steps the compaction record can show, in order: on SQLite
  `mark, build, verify, install, cleanup`, since the record writes `cleanup` in its final step.
  `store compact`'s progress events stay `mark, build, verify, install` (V19-12). No code change.

## Conformance fixes (2026-11-19, review R3)

Coordinator rulings on the R3 conformance review, applied to the rows by package FX.

- **CF-1 `_count` examples** A `<list>_count` is set only beside a list cut at its first 20 (SCHEMA,
  ruling R1 of the rollout). The long examples that showed a count beside an uncut list now show it
  null: `backup disconnect`, `backup restore`, `branch merge`, `commit get`, the seven `file` writes,
  `machine list`/`get`/`images`/`prune`/`stop`, `store list`/`get`/`connect`/`save-auth`/`auths`,
  `store compact`, `workspace commit` and `workspace status`. A plain `machine stop` answers
  `stopped_sandboxes` and `unknown_workspaces` null in its long form, as the shaper does.
- **CF-2 Store reason `unsupported`** `store list` and `store get` add the reason `unsupported`: a Store
  record of a kind this build cannot read is `broken` with that reason (a real state, as on a machine
  and a Sandbox).

## Conformance fixes (2026-11-19, review R4)

Coordinator rulings on the R4 conformance review, applied to the rows by package FX.

- **CF-3 Full-check progress** The doctor progress rule gains one case: the Full check of
  `project doctor` and `backup doctor` may send `{phase: "data", done, total}` events without `node`
  while it reads. `project doctor --full` sends them; `backup doctor --full` sends none today.
- **CF-4 one severity per condition** A Sandbox whose record names a Store, project or machine with no
  record (or no project at all) is an `error` in `home.records`, as in that Sandbox's own reference
  checks; the `home doctor` example's `old-run` finding is now an error. A machine that only a Sandbox
  or Store record names has no record of its own: `machine doctor <m>` answers `NotFound` and the
  record that names it reports the dangling reference.

## Version 20 (owner 2026-11-19): leftovers removed

Owner, verbatim: "can we just remove them, and drop layerfs home leftovers".

- **V20-1** `home.leftovers` and `home.prune` leave the table with no alias (ruling R1): their names
  are refused like every removed 1.x name (exit 2 on the command line). 85 calls become 83; `home`
  has 5. Today's `doctor.fix`, which `home.prune` replaced, is removed with no successor. The
  worker operation that served `home.prune` is gone; an older caller that sends it gets the worker's
  unknown-operation refusal.
- **V20-2** LayerFS removes leftovers itself, only where it can prove nothing owns them: no record
  names it, no live daemon or process holds it, no lock is held. One attempt, no retry; a removal
  that fails is a line of `log/sweep.jsonl` (the home's, or the machine's beside `worker.log`) and is
  left for the next sweep, never reported as success. A sweep runs inside the bounded walk the list
  used (20,000 entries or 2 s) and never fails the call it rides on. Per kind:
  - `run_folder`, `empty_root`: a worker sweeps its run root when it starts (`machine start`) and
    before each `sandbox create` it runs; this process sweeps `native`'s before a `sandbox create`
    there starts its daemon. A run folder goes only when no Sandbox record names it (nor a create in
    flight), its `owner` marker is older than a minute, the record that marker names has no live
    daemon, and nothing is mounted at or under it; an empty private root only when its run folder is
    gone and it is not a mount point.
  - `image_tmp`, `export_container`: `machine load-image` and `sandbox create --image`, when they
    copy an image, sweep under the machine's prune gate for every image whose copy lock is free, not
    only the one being copied: this home's labelled export containers (`docker rm -v` under that
    image's lock) and the machine's unpack temporaries (under their unpack lock).
    `machine prune --unused` keeps removing `.tmp-*`. Unlabelled export containers of builds before
    the labels name no home and are left alone.
  - `backup_staging`: the next `backup create` of that Store, holding the Store's lease and its
    create lock, removes the staged snapshots a killed create left (in the home and on the Store's
    machine); a `backup restore` that replaces the Store removes those in the home, under the lease
    and the create lock taken without waiting.
  - `init_copy`: never removed by a guess. A worker's Init removes its copy when it ends, and holds
    it while it reads it. An Init whose owner is gone leaves its project `broken` `unresolved` with
    the fix `layerfs project delete <project>`; the Init's `op` names its copy and machine, and the
    `project delete` that settles it removes the copy there, unless that Init still holds it.
- **V20-3** Space stays visible without a list: `machine stats --with space` keeps counting
  `other_runs`.
- **V20-4** The doctors are unchanged: no leftover check is added.
- **V20-5** Older state is cut, not carried (package CUT). Owner, verbatim: "cut should be
  aggressive, do not have compactibility against the old" and "cut and cleanup old incompatible
  aggressively, no lfs, and fallback to old 2.0 or 1.x". No path reads, converts or falls back to a
  1.x or an earlier 2.0 record. Old state shows a 2.0 state word or is refused with a fix; no
  record is migrated or rewritten. 2.0-on-2.0 behaviour is unchanged.
- **V20-6** An older home (layout 2, or 1.1.0's records) is never converted: `home.init` and every
  call refuse it `Unsupported`, `home.get` answers it `broken` `outdated`, and `home.doctor`'s
  `home.layout` fails; each gives the fix `mv <home> <home>.old && layerfs home init`.
- **V20-7** A PostgreSQL Store record with no saved auth is `broken` `outdated`, and a call that
  dials it is refused, both with the fix `layerfs store save-auth <auth> --user <user>
  --password-stdin && layerfs store update <store> --auth <auth>`. A bucket record with no
  `access_key` fails `bucket.credentials` with its disconnect and connect fix and sends nothing.
  An unresolved op named `store.push` or `store.pull` is not mapped: `layerfs store get <store>
  --with live` clears it, and that is its fix.
- **V20-8** No `lfs-` names. A machine record with no `vm_name` (the built-in `docker` included)
  reads `broken` `outdated`; every call that needs its VM or container is refused `Failed` (its
  record has no `vm_name`, and no call repairs it) before the provider is asked. Its fix, on the
  state, the refusal and `machine doctor`'s finding, is `layerfs machine remove <machine>`, which
  removes the record alone and touches no VM. For the built-in `docker` the next call records it
  again with its 2.0 name, as a new home's is. A Sandbox of the retired docker backend names its
  container only when its record keeps the name. A Sandbox record whose name this home does not
  give it (an `lfs-` run folder) is refused with the fix `layerfs sandbox remove <sandbox>`, which
  removes its records alone (`already_gone`, `left` naming its run folder and daemon process).
- **V20-8a** A Store record that is not read (no `kind`, or a `machine` that is not text) is
  `broken` `unsupported` with the fix `layerfs store remove <store>`, and calls that need it are
  refused with that fix. `store.remove` and `store.disconnect` remove such a record alone (its
  folder's `record.json` and `project/`), touching no Store file or database.
- **V20-9** `forgotten` is gone. `machine.remove` of a machine of a removed platform answers
  `removed` with a `message` that only its record went; `sandbox.remove` of a Sandbox of the retired
  docker backend answers `already_gone`, with `left` saying its container was not touched. The
  `forgotten`, `container` and `fix` answer keys are removed.
- **V20-10** `workspace.exec`'s `env` is one object (`--env K=V` on the command line); a list is
  refused.
- **V20-11** Older runtime files are refused, not read: a Linux bundle without file identities
  (rebuild with `python3 tools/linux_bundle.py`), a local Sandbox record without `boot` or with
  labels of another shape, and image cache folders other than `<hex>.2`. A machine record's
  reasonless `failed` reads `stopped`.
- **V20-12** Two 2.0 behaviour fixes ride with the cut: a Store with no record in a worker root no
  longer resolves through the old identity path (it misfired on 2.0 worker roots), and a
  PostgreSQL Store record with no `synchronous_commit` falls back to `local` when a connect dials
  it, as an open does (the connect path fell back to `off`). Only that fallback changed; a
  recorded `synchronous_commit` is used as before.
- **V20-13** (coordinator ruling on the CUT review, FAIL 1) The removed answer keys
  `machine.remove` `forgotten`, `sandbox.remove` `container` and `fix`, and `home.init`
  `converted` only ever carried values for old state. They are part of the owner's cut ("cut
  should be aggressive, do not have compactibility against the old"; "cut and cleanup old
  incompatible aggressively, no lfs, and fallback to old 2.0 or 1.x") and stay removed; no
  2.0-on-2.0 answer loses a key.

## Coordinator rulings after the 3080 pass

- **R-LF2-1** An `InUse` refusal may carry the next fields `sandboxes`, `stores` and `machines`,
  naming what holds the thing, with a fix.
- **R-LF2-2** `machine stop`'s `InUse` keeps the removal chain as its fix; `--force` is named in the
  message, not offered as the fix. A fix may join more than two commands.
- **R-LF3-1** An uncertain refusal (a request was sent and its outcome is not known) carries
  `attempted: true` as a flat next field and also in `detail`, so the short form keeps the
  uncertainty.

## Version 21 (owner 2026-11-19): a bucket key rotates in place

Owner, verbatim: "Yes, send it". Rotating a bucket key must not drop its Store links. No call is
added; the table keeps 83 calls.

- **V21-1** `backup connect <name> <url> --access-key K --key-stdin` under a name that is connected
  at the same URL replaces its key. The same URL means the same place after parsing: provider,
  bucket and prefix, with r2's account and `?jurisdiction=` and an endpoint's scheme and host.
  It first proves the new key with the one listing a connect makes; on `Unavailable` (the bucket
  does not answer or refuses the key) nothing changes, and the old secret and the record stay byte
  for byte. Then the secret is saved as always (one rename into the home's secret folder, 0600)
  and the record's `access_key` in the same atomic record write; nothing else in the record
  changes, and no Store's link to the bucket (`backups.<bucket>` with its `last` transfer) is
  read or written. The long form's `outcome` is `rotated`; the short form is a first connect's.
  The call log keeps the secret's byte count only. One attempt, no retry; no Store or record
  format change.
- **V21-2** A taken name at another URL stays `Exists`; its fix is
  `layerfs backup disconnect <name>` (shell-quoted), and the message says a new name works too.
  Nothing is sent to the bucket.
- **V21-3** Buckets get no saved-login model. PostgreSQL saved logins (`store save-auth`) exist
  because one identity spans several addresses: a login is shared by Stores on several servers
  and reached from several machines. A bucket is reached only by the host, at one address, so its
  key belongs to that one bucket record, and connecting it again is how a key changes.
- **V21-4** V20-7's fix for a bucket record with no `access_key` becomes connecting it again at
  its URL (`layerfs backup connect <name> <url> --access-key <key> --key-stdin`), which keeps its
  Stores' links, in place of disconnect and connect. The message of a missing secret says the
  same.
- MIG: owner, verbatim, of the conversion code: "just drop it. we will never need it". The home's
  detection knows no older layout: no folder, or one holding only `layerfs.lock` (and the temporary
  `layerfs.json` is renamed from), the integration folders `runtime/`, `job/`, `stat/`, `preset/` and
  `.DS_Store`, is a new home; `layerfs.json` at layout 3 is current; any other
  `layerfs.json`, or anything else in a folder with no `layerfs.json`, is not a LayerFS 2.0 home
  and gets V20-6's refusal, state and fix, and nothing is written in it.

## RV3 rulings (coordinator, 2026-11-19)

The RV3 review of the cut (CUT + CUT2 + BK2) found three dead ends for state an earlier build left;
the coordinator ruled on each. No call is added; the table keeps 83 calls. No record is migrated or
rewritten, and 2.0 state answers as before.

- **RV3-1** (FAIL 1) A machine whose record has no 2.0 name (`vm_name`; `native` never counts) that
  still holds Stores or Sandboxes is no longer a dead end. `store remove` of a sqlite Store on it,
  and `sandbox remove` of a Sandbox on it, take their records alone: nothing on the machine is
  reached (not its worker), and `left` names what may remain there (the Store's file; the
  Sandbox's daemon and run folder). `store.remove` gains `left` (short: sometimes; long: last),
  answered only when records alone went; its record-not-read case answers it too, naming the Store
  file it keeps with the line that removes it. `machine remove` of such a machine is refused
  `InUse` with the whole chain as its fix: `layerfs sandbox remove <s> && … && layerfs store
  remove <s> && … && layerfs machine remove <m>` (Sandboxes, then Stores, then old Firecracker
  records, then the machine). There is no silent cascade.
- **RV3-2** (FAIL 2) A local Sandbox record that fails the 2.0 parse (a field a 2.0 record always
  has missing or of another shape: `directory`, `name`, `container`, `pid`, `boot`, `start`,
  `port`, `labels`, a namespaces record's `image`) is removed by `sandbox remove`, forced or not,
  by its records alone, with no signal sent, since its identity cannot be checked. It answers as
  the other records-only removes do: `already_gone` with `left` naming its run folder. Every other
  call refuses it `InvalidInput` with the fix `layerfs sandbox remove <s>`. A worker's own record
  of that kind is forgotten the same way when the caller's home holds only the forward to it.
- **RV3-3** (FAIL 3) No 2.0 path creates a native (host-process) Sandbox record on Linux: the
  launch refuses `native` there before any effect (#533 O27), the worker and the daemon start only
  namespaces Sandboxes on Linux, and the in-process SDK in a Linux worker reaches the same launch.
  The host-process backend is therefore macOS only; a native record found on Linux is refused as
  RV3-2 says, its fix the records-only `sandbox remove`.
- **RV3-4** (FAIL 4, NOTE 5) A records-only remove's `left` names a daemon's process id, with its
  `kill` line, only while the process its record identifies (id, start time and boot) still runs;
  otherwise it says the process is gone. Folders in a `left` or `fix` line are one shell word. A
  bucket's same place compares the host (and r2's account) in lower case and leaves out an
  endpoint's default port.

## UI pass 2 rulings (coordinator, 2026-11-19)

- **UI2-1** (F1) A machine whose record alone makes it broken (a removed platform's, `unsupported`;
  one with no `vm_name`, `outdated`) is not probed by a live read: `machine get --with live` and
  `machine list --with live` answer the record's state, `reason` and `fix`, never `unknown`.
- **UI2-2** (F6) V20-7's no-login fix names the user the record's URL names when it names one
  (`--user owner`, shell-quoted), else `<user>`; `store list`, `store get`, a refused dial and
  `store doctor`'s `store.connection` give the same line. `store doctor`'s `store.record` on a
  record of a kind this build does not read carries V20-8a's `layerfs store remove <store>`.
- **UI2-3** A `backup list` `not_listed` entry carries `fix`, V21-4's
  `layerfs backup connect <name> <url> --access-key <key> --key-stdin` (shell-quoted), when its
  bucket was not asked for want of its key (no key id, no saved secret) or refused its key
  (HTTP 401 or 403); any other entry has none (left out of the short form, null in the long).
- **UI2-4** `--text` prints `ready` in a list's STATE column for a row whose short form left
  `state: ready` out, and keeps the column when every row was ready; `-` stays for any other
  absent value. `layerfs help` says `--text` prints a readable form.
- **UI2-5** (item d) `store doctor` on a sqlite Store whose file another writer holds reports
  `store.identity` and `store.schema` as `not_checked` with reason `busy`, never as a schema or id
  error; `store.connection` passes when the file is there.
- **UI2-6** (clibugs, f14fb23da0) `home get --detail`'s `bundle.source` gains the word `runtime`: the
  bundle an installed runtime carries beside its command (`<runtime>/bundle/linux-<arch>/` beside
  `<runtime>/bin/layerfs`, symlinks resolved), chosen after `LAYERFS_BUNDLE` and before the cache.
- **UI2-7** (clibugs, f14fb23da0) `machine create`, `machine start` and every call that starts a
  worker refuse `Unavailable`, before anything starts, a Linux bundle whose `bundle.json`
  `source.head` is not the commit this build was compiled from; a build that names no commit cannot
  tell and uses it.
- **UI2-8** (LF5, RT5 note 3) A machine whose record has no `vm_name` and still holds Sandboxes,
  sqlite Stores or old Firecracker records shows, wherever its broken state shows a fix
  (`machine list`, `machine get`, both with `--with live`; `home get --with tree`; `machine doctor`'s
  `machine.configuration`), the chain its remove's `InUse` refusal carries: each Sandbox's remove,
  each Store's, each old Firecracker record's, then the machine's, shell-quoted and joined by `&&`,
  from the one builder and from records the read already holds (no probe). A call that needs its
  VM (`machine start`, `machine stop`, forced or not, and a PostgreSQL Store's reach check from it)
  is refused `Failed` with that same chain, its holders read on the refusal path alone. With nothing
  on it the fix stays `layerfs machine remove <m>`.
- **UI2-9** (LF5, RT5 note 4) UI2-7's refusal carries a `fix` when another place the CLI looks holds
  a sealed bundle of this build: `unset LAYERFS_BUNDLE` when `LAYERFS_BUNDLE` was the source and the
  place found without it (the runtime's bundle, else the cache's) holds this build's; else
  `export LAYERFS_BUNDLE=<folder>` (shell-quoted) naming it, as when the runtime's bundle was refused
  and the cache's is this build's. With none, no `fix`: the message says no bundle of this build is
  where LayerFS looks, with the rebuild line. No plugin is named.

## Version 22 (owner 2026-11-19): CLI terminal output and help

Owner, after a terminal mock: "Yes, send it" (cliux, f5bb939206), then "Yes, fix it" for the table
rule below (b2eb2de931).

- **V22-1** stdout on a terminal gets the readable form by default; a pipe, a file or a program gets
  the one JSON line as before, unchanged for dsh, MCP and agents.
- **V22-2** `--json` forces the JSON line on a terminal too; `--text` forces the readable form;
  `--text` with `--json` is a wrong command line (exit 2, nothing called).
- **V22-3** A refusal comes in the matching form: the `{"error",…}` line on stderr when stdout gets
  JSON, else readable `error:` and `fix:` lines. The exit codes do not change.
- **V22-4** The next page or line window is printed as `more: …` (the command line that reads it)
  on stderr, in the readable form only; the JSON keeps `next_cursor` as before.
- **V22-5** A list table's STATE cell carries the row's reason (`broken: outdated`) and what blocks
  it (`blocked by machine docker (outdated)`); REASON is not a column. A row's `fix` and `message`
  print as lines under the table (`fix <first cell>: <command>`). Piped JSON is unchanged. The
  manual's "Results" says the rest of the readable form.
- **V22-6** `layerfs <family> --help` works, as `layerfs help <family>` does; each call's help comes
  from one place, `layerfs_api::guide`.
- **V22-7** Two refusals name their fix (cliux, 16a20bd09d): `branch merge` into `main` with no
  `--into` (refused `InvalidInput`) gives `layerfs branch publish <project> <from>`; a `Dirty`
  `workspace unmount` gives `layerfs workspace commit <workspace>`.

## RC rulings (coordinator, 2026-11-19, final conformance RC-A and RC-B)

- **RC-1** (RC-A F1) A path `Conflict` of `branch merge` carries, beside the code, `commit` (the
  head of the Branch merged into), `conflicts` (the first 20 conflicting paths, text) and
  `conflicts_count` only past those, as `workspace commit`'s does. The engine's conflict objects
  stay in `detail`, with `paths` and `total` beside them; no camelCase key is a flat field.
- **RC-2** (RC-A F2) `workspace commit --expected` compares with the head `branch get` answers: a
  Branch with no Commit of its own is at the Commit it was forked at, and the refusal names that
  head as `commit`.
- **RC-3** (RC-A F3 and NOTE) A `Dirty` refusal's `changes` are `workspace status`'s short-form
  entries (`type` left out for a file); `changes_count` only beside a cut list. Its fix is V22-7's
  `layerfs workspace commit <workspace>`, with no `-m` placeholder.
- **RC-4** (RC-A N5, RC-B N15; RV5) The fix of a mount of `main` is runnable:
  `layerfs branch fork <project> <work> && layerfs workspace mount <sandbox> <work>`, where
  `<work>` is `work`, or the first of `work-2`, `work-3`, ... that no Branch of the project has,
  read at refusal time. A placeholder stays only where LayerFS cannot know the value (a secret
  key, a saved login's name).
- **RC-5** (RC-A N9) Unchanged long forms are kept as built, since neither the spec nor the manual
  rules on them: `workspace commit`, the file writes and `branch publish` answer `parent`,
  `message` and `created_at` (publish: `source_commit`, `base`, `root`, `size`) null when nothing
  was made; `branch merge` unchanged fills `parent`, `second_parent` and `merge_base` with the
  head it already holds.
- **RC-6** (RC-B F1, N12; revised after RV5 FAIL-1 and FAIL-2) A forced `sandbox remove`, on
  `native` and on a machine, ends once its runtime is confirmed stopped the mount intent each of
  its Workspaces held, once each (released, or marked uncertain when the Store refuses the
  delete): the call that stopped the intent's owner ends what that owner held. One it could not
  end is a sentence of `left` saying why, what it refuses (`branch remove` of its Branch,
  `store compact`) and what ends it where the Sandbox ran: the Store's own admission once it can
  prove the owner gone (on a machine's sqlite Store after that machine's kernel restarts; never
  on macOS `native` or a PostgreSQL Store), never `store compact` alone. Nothing else ends a
  mount intent: under V20-2 and the persistence admission rule, `store compact` and the other GC
  calls settle only intents whose owner the admission proves dead, and a Sandbox folder that is
  gone is no such proof (`sandbox remove` forgets an unread or retired-docker record without
  stopping its daemon). V20-4 stands: no doctor gains a leftover check; a held Store's `InUse`
  refusal keeps naming its holders' count, with no guessed fix. No Store format change.
- **RC-7** (RC-B F3) A call that needs the worker of a machine whose record has no `vm_name` is
  refused `Failed` before anything is dialled (V20-8), with UI2-8's chain as its fix, built at the
  call boundary on the refusal path alone.
- **RC-8** (RC-B N1; revised after RV5) A home whose `layerfs.json` cannot be parsed, or names no
  layout this build knows, is `broken` `outdated` (MIG: not a 2.0 home) with V20-6's move-aside
  fix. One the system refuses to read (EACCES, EIO on `layerfs.json`, or the folder's listing) may
  be a real 2.0 home and is never advised to be moved aside: it is `broken` `failed`, the row
  note's reason for a `layerfs.json` it cannot read, with the OS error as `message` and no fix
  (the spec gives none for permissions).
- **RC-9** (RC-B N3) Help prints an optional positional as the rows do, `[name]`.
- **RC-10** (RC-B N13) A native Sandbox's `platform` stays `mac`, #529's Sandbox platform word: the
  spec names no Sandbox platform for `native`.

## Version 23 (owner 2026-11-19): PostgreSQL Stores, who connects

Owner, on Stores ▸ PostgreSQL: the screen must show which machines connect to the database that
holds the LayerFS data, and manage logins. Spec `tmp/pg-store-v23/SPEC.md` (owner answers to its
§7 and to the review's three questions, 2026-11-19).

- **V23-1** A login says who, the binding says which database, an address says how. One login may
  serve Stores on several servers (V21-3 stands); a UI groups PostgreSQL Stores by login.
- **V23-2** What was seen when a side dialled a PostgreSQL Store is kept beside its record, in its
  observation file (`store/<s>/observed.json`, with the `at` a live read writes): `here` for this
  computer, `reach.<machine>` per machine, each `{at, ok, latency_ms, server_version, error}`. It is
  written by `store connect --machine` (its check), every `store doctor` reach (this computer's
  included) and each Sandbox made on a machine (`{at, ok: true}`, after the reply); under the
  Store's lock, never after the Store is gone, never failing the call, moving no `generation`.
  `native` with no address of its own dials as this computer and is `here`.
- **V23-3** `store get`, `store list`, the `store connect` answer and the home tree's
  `connections` list every side that connects: this computer first (`machine` null), then by
  name each machine with an address of its own, a reach seen, or a Sandbox of the Store. A row is
  short `{url, machine, reached}` (`reached`: `ok`, `failed`, absent when nothing was seen) and
  long `{machine, url, own, server, reached, reach}`.
- **V23-4** Dropped (owner): `machine get` is unchanged. A UI derives machine → PostgreSQL Stores
  from `store list` rows.
- **V23-5** `store get` and `store list` long forms add `binding`: the first 8 bytes of the id in
  the Store's schema as 16 hex, the open-Store cache's key.
- **V23-6** No compatibility (owner rule of 2026-11-19): a PostgreSQL record holding a `password`,
  a URL with a user or a login parameter, or a `machines.<m>` entry with anything but its `url`
  is `broken` `outdated`, fix `layerfs store update <store> --auth <auth>`. Every PostgreSQL record
  write (`store connect`, `store update`, `store disconnect --machine`, `machine remove`) drops
  those fields; a read never does. Calls are not refused for them.
- **V23-7** A record naming a login the home does not hold is `broken` `missing` (owner: no new
  reason word), fix `layerfs store save-auth <auth> --user <user> --password-stdin && layerfs
  store update <store> --auth <auth>`, which wins over `outdated`. `store update --auth` checks
  the login again under the home lock, closing its race with `store remove-auth`.
- **V23-8** `store auths` lists a login file it cannot read as a row `broken` `failed` with the
  save that writes it again, instead of failing the list; every row has `state`.
- **V23-9** `store doctor` of a PostgreSQL Store dials from this computer, then from every machine
  with an address of its own, a Sandbox of it, or `--machine`, and records each reach.
- **V23-10** `store disconnect --machine M` also forgets M's reach, and succeeds when M has only a
  reach; `machine remove` forgets its reaches too.
- **V23-11** URL hygiene: `passfile` joins `user` and any `password` parameter as a refused login
  (`InvalidInput`); `sslkey` and `sslcert` name files and pass.
- **V23-12** A replica or copy of a Store's database carries its binding, so it counts as the same
  Store (`Exists` for a second name); the manual says so.

## LF10 ruling (overnight integration, 2026-11-20): a Store blocked by a nameless machine

- **LF10** (amends RC-7 and V20-8's "refused `Failed`" for Store calls) A call on a sqlite Store whose machine record has no
  `vm_name` is refused `Unavailable`, as a Store call is for its machine's other not-ready states
  (`store.compact`'s refusal rows), not `Failed`: the Store reads `blocked`, `blocked_by {machine,
  reason: outdated}`, in `store list` and `store get`, so the refusal carries that same
  `blocked_by` flat beside `error` and `message`, and the machine's own fix (UI2-8's chain, built at
  the call boundary on the refusal path alone). Its message says the Store is blocked by the
  machine, then V20-8's sentence. Nothing is dialled. The machine's own calls (`machine start`,
  `machine stop`, `machine stats`, a PostgreSQL Store's reach check from it) stay `Failed` (RC-7):
  the machine itself is `broken`. `store remove` of such a Store still goes by its records alone.

## LF11 (overnight integration, 2026-11-20): the URL-login refusal carries its fix

- **LF11** (completes V23-13) `store connect` refusing a URL that names a user or carries a login
  parameter (`user`, any `password`, `passfile`) answers `InvalidInput` with a `fix` that gives the
  address alone (the URL with its user part and login parameters dropped) and never the password:
  on a first connect `layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store
  connect <s> <address> --auth <auth>` (`<user>` filled from the URL's user when it names one); on a
  connected Store, which takes no `--auth` on connect, `layerfs store save-auth <auth> --user <user>
  --password-stdin && layerfs store update <s> --auth <auth> && layerfs store connect <s> <address>`,
  with `--machine <m>` for a machine's own address (the person who typed a login most likely meant
  to change it; orchestrator review).
- **LF11, conformance v23 fixes** (the overnight conformance review at `7ef14ed641`):
  - F1 (V23-3): a `connections` row's `url` is login-free also for an outdated record whose URL
    still holds a user or a login parameter (the row shows `without_login` of it).
  - F2 (V23-8): `store save-auth` writes a login file that cannot be read again, as the broken
    `auths` row's fix says; its old fields are not kept.
  - F3 (LF10): `store move` off a machine with no `vm_name` is refused like the Store's other
    calls: `Unavailable` with `blocked_by` and the machine's chain.

## LF12 (follow-up round, 2026-11-20): the Store doctor reads the record's own state

- **LF12** (completes UI2-2 and V23-6 for `store doctor`) `store doctor`'s `store.record` fails,
  severity `error`, when a PostgreSQL Store's record alone makes it `broken`: naming no saved login
  (`outdated`), naming one this home does not hold (`missing`), or holding what 2.0 no longer writes
  (`outdated`, V23-6). Its message names the reason; its `fix` is the line `store list` and `store
  get` give for that state (`layerfs store update <s> --auth <a>` for old fields). The remaining
  checks still run, so a record with old fields whose server answers fails `store.record` alone.
