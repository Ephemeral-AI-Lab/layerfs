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
  `layerfs home prune <leftover>` removes one it listed. `home get` has no `--with leftovers`.
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
- No raw-text mode for agent reads; JSON stays the default; `--text` stays as today.

## Totals after the freeze

85 calls in 10 families: project 8, file 14, sandbox 7, workspace 7, branch 6, commit 3, home 7,
machine 14, store 13, backup 6.

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
