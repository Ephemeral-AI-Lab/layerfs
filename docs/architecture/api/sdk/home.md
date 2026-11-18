# home family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `home` calls: set a home up on purpose, read its state and what this
build would use with it without changing anything, and read and clear its
call log, list and remove what interrupted operations left on disk, within
the [SDK](README.md). The calls are migrated to the call surface 2.0's
[forms](../calls/README.md#forms): the shapes below are their long forms, and
the short form is the projection the entry declares. The family's `doctor`
is one of [the doctors](../../control/doctor/README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/home`](../../../../api/crates/layerfs-api/src/home), [`layerfs-control/src/home.rs`](../../../../control/crates/layerfs-control/src/home.rs), [`layerfs-control/src/home_tree.rs`](../../../../control/crates/layerfs-control/src/home_tree.rs), [`layerfs-home/src/logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs), [`layerfs-home/src/home/generation.rs`](../../../../control/crates/layerfs-home/src/home/generation.rs), [`leftover_list.rs`](../../../../control/crates/layerfs-control/src/doctor/leftover_list.rs), [`leftover_prune.rs`](../../../../control/crates/layerfs-control/src/doctor/leftover_prune.rs) |
| Runs by default | the caller's process; `leftovers` also asks each running machine's worker, and `prune` runs on the machine its leftover names. `init`, `get` and `leftovers` are not opened by the dispatcher (`Call.opens` is false): `init` opens the home itself, `get` never does. `get`, `log` and `clear_log` are `quiet`: they write no call-log line |
| Design | [Home](../../control/home/README.md), [Migration](../../control/home/migration.md) |
| Clients | `lfs.home` is this family (`lfs.home.get()`, `layerfs.home.init()`); a client's home is the one given to `client()` and is no field of it |

## Calls

```text
 home.get        reads layerfs.json, the folder's mode, the generation file,
                 bundle.json, native's record and the daemon file -> changes nothing
   --with platforms   the host's platform rows (what machine.platforms answers)
   --with tree        the home's records: machines > Stores, Sandboxes; buckets
 home.init       migrate::open (create | convert | finish)
                 + the built-in machine records   -> {outcome: created | converted | unchanged}
 home.leftovers  walk native + ask each running machine's worker -> {items, not_listed}
 home.prune      the leftover's machine: guard, remove once     -> {freed_bytes, outcome: "removed"}

 every other call ---- one line when it ends ----> <home>/trace/calls.jsonl
                                                    (calls.1.jsonl past 16 MiB)
 home.log        (filters, newest first, by cursor) <------- reads both files
 home.clear_log  (truncate calls.jsonl, remove calls.1.jsonl) -> { outcome: "cleared" }
```

| Call | Short form | Long form |
| --- | --- | --- |
| [`home.init`](#homeinit) | `{ home }` + `unchanged`, `converted` when true | `{ home, unchanged, converted, outcome, layout, version }` |
| [`home.get`](#homeget) | `{ home, state, version }` + `generation`, `reason`, `fix` when news | `{ home, state, version, generation, reason, fix, message, layout, mode, bundle, daemon, cache }` |
| [`home.log`](#homelog) | `{ items }` + `next_cursor` when there is a next page | `{ items, next_cursor, skipped }` |
| [`home.clear_log`](#homeclear_log) | `{ ok: true }` | `{ ok, outcome: "cleared", line_count }` |
| [`home.leftovers`](#homeleftovers) | `{ items }` + `not_listed` unless empty | `{ items, not_listed, observed_at }` |
| [`home.prune`](#homeprune) | `{ ok: true }` + `freed_bytes` | `{ ok, freed_bytes, outcome: "removed", leftover, kind, machine, path }` |

### home.init

Makes the home ready for calls, as the first call of any other kind does: an
empty or missing folder is created (0700, `layerfs.json`, `machine/native/`
and `machine/docker/` with the `machine` feature), a 1.1.0 or layout 2 home
is converted, and an interrupted conversion to layout 3 is finished. A current
home is left as it is. `outcome` is `created`, `converted` or `unchanged`;
`version` is this build's version and `layout` the layout it left. It writes
one call-log line, and the records it writes move the generation.

```text
JS      await layerfs.home.init()
Python  layerfs.home.init()
CLI     layerfs home init
```

Refused `InvalidInput` inside a machine's worker root (not a home), `InUse`
while a layout 2 home has live Sandboxes (its conversion lists them), and as
opening a home is refused otherwise (a newer layout is `Unsupported`).

### home.get

The home's state and what this build would use with it. It reads files only
(no process, Docker, network, lock or call-log line), so a client can poll it,
and it answers for a home that does not exist yet, creating nothing.

```text
JS      await layerfs.home.get({ detail: true, with: ["platforms", "tree"] })
Python  layerfs.home.get(detail=True, with_=["platforms", "tree"])
CLI     layerfs home get [--with platforms,tree] [--detail] [--text]
```

| Field | Meaning |
| --- | --- |
| `home` | the home's path |
| `state`, `reason`, `fix` | `ready`, or `broken` with why and the one shell line that fixes it ([`state.rs`](../../../../control/crates/layerfs-home/src/state.rs)): `missing` (no `layerfs.json` and no 1.1.0 record; `layerfs home init`), `outdated` (1.1.0, layout 2, or an unfinished conversion; `layerfs home init`), `failed` (a `layerfs.json` this build cannot read, or a newer layout than it knows; `message` says which; no fix line) |
| `version` | this build's version |
| `generation` | the home's [generation](../../control/home/README.md#the-generation-counter): a number that moves when a record is written or removed and never for a read. Null unless the home is ready |
| `message` | a sentence saying what is wrong, null when ready |
| `layout`, `mode` | the layout `layerfs.json` records, and the folder's permission bits as text (`"0700"`); null when there is no folder. Whether the mode is owner-only is the doctor's finding, not this call's |
| `bundle` | The Linux bundle this build would deploy: `{ path, source, present, commit, dirty, matches }`; `source` is `env:LAYERFS_BUNDLE` or `cache` (`<cache>/layerfs/bundle/<version>/linux-<arch>`); `commit` and `dirty` from `bundle.json` `source` |
| `daemon` | macOS: native's daemon, `{ path, source, present, commit, dirty, matches }`, `source` `recorded` (native's record) or `default` (beside the macOS bundle). Null elsewhere |
| `cache` | `{ path }`: `XDG_CACHE_HOME`, else `~/.cache` |

The answer has no `warnings` list any more (it was
`[{ code, message }]`): the home's own trouble is `state`, `reason` and
`fix`, and a missing or mismatched bundle or daemon, or a folder mode others
can read, is for the doctor to report.

`matches` compares a part with the build making the call: false when the
commits differ, true when they are equal and both trees were clean, null when
either commit is unknown or a tree was dirty. The build's commit is the one
the build tools set (`LAYERFS_BUILD_COMMIT`, `tools/worktree.py`); a plain
`cargo build` names none, so its answers are null. The daemon's commit is the
text after `layerfs-build-identity:` in its file, stamped by its build script
the same way; `tools/linux_bundle.py` writes the bundle's `source.dirty`. A
mismatch is reported, not refused: a bundle from another commit with the same
worker protocol still starts, as before.

**`--with platforms`** adds `platforms: { host, items }`: the rows
`machine.platforms` answers (which stays until the `machine` family is
migrated), with `kernel` named `route` and an option's `name` named
`option`. A row's short form is `{ platform, ready }` with `missing` when it
is not empty; `ready` stays when false. It needs no home; a build without
the `machine` feature (a machine's worker) refuses it `Unsupported`.

**`--with tree`** adds `tree: { machines, stores, buckets }` from the home's
records only ([`home_tree.rs`](../../../../control/crates/layerfs-control/src/home_tree.rs)):
it dials nothing, and null unless the home is ready.

| Node | Long form | Short form |
| --- | --- | --- |
| machine | `machine, platform, cpus, memory_bytes, state, op, reason, fix, stores, sandboxes` | `machine`, `platform`; its state and companion unless ready; `stores` and `sandboxes` unless empty |
| Store | `store, engine, projects, connections, state, op, blocked_by, reason, fix` | `store`, `engine`; `projects` and `connections` (`[{ machine, url }]`) unless empty; its state unless ready |
| Sandbox | `sandbox, project, image, workspaces, workspaces_not_listed, state, op, blocked_by, reason, fix` | `sandbox`, `project`; `workspaces` (`[{ workspace, branch }]`) unless empty; `workspaces_not_listed` when true; its state unless ready |
| bucket | `bucket, provider` | `bucket`, `provider` |

A sqlite Store is under its machine; a PostgreSQL Store lives on no machine
and is in the top-level `stores`, with `connections` naming where it is
reached from: first `{ machine: null, url }` (the Store's own URL), then one
row per machine it is connected from. A Sandbox's `workspaces` are listed
only when the home holds them (on `native`, or a Sandbox whose daemon
records are forwarded here); otherwise `workspaces_not_listed` is true, and
for a blocked Sandbox `workspaces` is null. States come from the records: a recorded operation whose
process lives is `busy` with `op { name, phase, started_at }`, one whose
process is gone with its outcome unknown is `broken` `unresolved`; a machine
of a removed platform is `broken` `unsupported`; a machine recorded stopped
is `stopped`; a machine that only a Store or Sandbox names is listed too
(`native` ready and `docker` stopped, which exist with or without a record;
any other `broken` `missing`), so nothing is left out of the tree; a Sandbox recorded gone is `broken` `missing`. A Store or
Sandbox on a machine that is not ready is `blocked`, `blocked_by { machine,
reason }` naming it. A live probe is not this call's: `machine get --with
live` and `store get --with live` are later packages'.

### home.log

The calls made against this home, newest first, from `trace/calls.jsonl`
then `trace/calls.1.jsonl` (it was `logs.list`).

```text
JS      await layerfs.home.log({ call, project, client, tag, outcome, since, limit, cursor, detail })
Python  layerfs.home.log(*, call=None, project=None, client=None, tag=None, outcome=None, since=None, limit=None, cursor=None, detail=None)
CLI     layerfs home log [--call <call>] [--project <project>] [--client <client>] [--tag <tag>] [--outcome <outcome>] [--since <since>] [--limit <limit>] [--cursor <cursor>] [--detail]
```

Every option given must match.

| Option | Kind | Rule |
| --- | --- | --- |
| `call` | Text | `family` or `family.call` (`file`, `file.write`; `-` and `_` are the same) |
| `project` | Project | lines whose `argv` names this project |
| `client` | Name | `cli`, `node`, `python` or `rust` |
| `tag` | Text | an exact tag, or a prefix ending in `:` |
| `outcome` | Text | a row's `outcome` (`ok`, `refused`, a result's word such as `committed`) or its `error` (a public refusal code) |
| `since` | Int | lines that started at or after this time (ms since 1970) |
| `limit` | Int | 1 to 1000 (`MAX_LIMIT`); default 200 (`DEFAULT_LIMIT`) |
| `cursor` | Text | `next_cursor` from a previous page |

A stored line is `{ v, at, ms, client, argv, outcome, kind?, commit?, layer?,
tag? }` ([the call log](../calls/README.md#the-call-log)); the format on disk
did not change. A row answers it as:

| Row key | From the line |
| --- | --- |
| `call` | `family.call`, from the first two words of `argv` |
| `outcome` | `refused` for a refused call; else the line's `kind` in snake_case (`Committed` → `committed`); else `ok` |
| `logged_at` | `at`, as UTC text to the second (`2026-11-18T09:16:59Z`) |
| `project` | the first `<store>/<project>` word of `argv`, or null |
| `error` | a refused call's code, folded to its public one; null otherwise |
| `duration_ms`, `client`, `argv`, `commit`, `tag` | `ms`, `client`, `argv` (redacted arguments as byte counts), `commit`, `tag` |

The short form of a row is `{ call, outcome, logged_at }` with `project` and
`error` when they are not null. `skipped` (long form) counts lines of the
page that do not parse (a torn tail after a crash), which are never
answered. A page ends at `limit` rows, or early past 64 KiB (`PAGE_BYTES`),
with a cursor. `next_cursor` carries the call it is for and the filters that
made it: a later call may leave the filters out, a filter given again must be
the same, and a cursor of another call is refused. Errors: `InvalidInput` (a
bad option, `limit` out of range, a malformed cursor, a cursor given with
other filters, or a stale one: "the cursor is stale (the call log was
rotated or cleared); read from the start"); `Failed` (a log file cannot be
read).

### home.clear_log

Clears the call log: truncates `trace/calls.jsonl` in place and removes
`trace/calls.1.jsonl` (it was `logs.reset`).

```text
JS      await layerfs.home.clear_log()
Python  layerfs.home.clear_log()
CLI     layerfs home clear-log
```

Short form `{ ok: true }`; long form `{ ok, outcome: "cleared", line_count }`,
the lines both files held, torn ones included. It holds `trace/calls.lock`,
waiting for it, so a line another process writes meanwhile lands before or
after the clear and is never torn. A `home.log` cursor from before the clear
is stale afterwards. Error: `Failed` (the lock or a file cannot be opened,
truncated or removed).

### home.leftovers

What interrupted operations left on `native` and on each running machine:
one bounded walk of each side's own folders, the machines' through their
workers' `doctor.sandbox` request. It changes nothing and starts nothing.

```text
JS      await layerfs.home.leftovers({ machine })
Python  layerfs.home.leftovers(machine=None)
CLI     layerfs home leftovers [--machine <machine>]
```

Each item is `{ leftover, modified_at, size, report_only, removable_at, kind,
machine, path }`; the short item is `{ leftover, modified_at }` plus `size`
and `report_only` when set. `leftover` is `<machine>:<kind>:<name>`, the id
`home.prune` takes; it never holds a path. The kinds are `run_folder`,
`empty_root`, `init_copy`, `backup_staging`, `image_tmp` and `image_part`
([Checks: leftovers](../../control/doctor/checks.md#leftovers)). A machine
that was not walked, or not to the end, is in `not_listed` as `{ machine,
blocked_by, reason, message }`: `blocked_by` for a stopped machine or one an
operation holds, `reason` `timeout` (no answer, or the walk stopped at its
bound of 20,000 entries or 2 s), `busy` or `refused`. `--machine` limits the
list to one machine; an unknown one is `NotFound`.

### home.prune

Removes the one leftover `leftover` names, under that kind's guard (it was
`doctor.fix`; the `doctor` family is gone).

```text
JS      await layerfs.home.prune(leftover)
Python  layerfs.home.prune(leftover)
CLI     layerfs home prune <leftover>
```

Long form `{ ok, freed_bytes, outcome: "removed", leftover, kind, machine,
path }`; `freed_bytes` is what the removed files held, null for a folder copy
and an image temporary. One attempt: nothing is retried or sent twice, and
nothing but the one entry is removed.

| Error | When |
| --- | --- |
| `NotFound` | no leftover has that id now: an id no list gives (a path, `..`, an unknown kind or machine), an entry already removed, or a link in its place |
| `InUse` | the kind's guard does not hold: a Sandbox record names the run folder, its `owner` marker is missing or newer than a minute, a daemon of it runs, something is mounted there, a `backup create` of the Store or an unpack of the image is running, the image part was written within the hour, or a folder copy's Init may run until an hour after its deadline. Nothing was removed |
| `InvalidInput` | a folder copy no line of `log/uncertain.jsonl` names (report-only); a missing `leftover` |
| `Unavailable` | the machine is stopped; `layerfs machine start <machine>` starts it |
| `Unknown` | the worker's answer was lost; `layerfs home leftovers` shows whether the entry is still there |
| `Failed` | the mount table cannot be read, or a removal stopped at an I/O error (the message names where) |
