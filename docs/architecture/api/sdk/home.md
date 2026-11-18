# home family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `home` calls: set a home up on purpose, read its state and what this
build would use with it without changing anything, read and clear its call
log, check it, and list and remove what interrupted operations left on disk,
within the [SDK](README.md). `home.doctor` is one of
[the doctors](../../control/doctor/README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/home`](../../../../api/crates/layerfs-api/src/home) (one file per call), [`layerfs-control/src/home.rs`](../../../../control/crates/layerfs-control/src/home.rs), [`layerfs-control/src/home_tree.rs`](../../../../control/crates/layerfs-control/src/home_tree.rs), [`layerfs-home/src/logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs), [`layerfs-home/src/home/generation.rs`](../../../../control/crates/layerfs-home/src/home/generation.rs), [`leftover_list.rs`](../../../../control/crates/layerfs-control/src/doctor/leftover_list.rs), [`leftover_prune.rs`](../../../../control/crates/layerfs-control/src/doctor/leftover_prune.rs) |
| Runs by default | the caller's process; `doctor` and `leftovers` also ask each running machine's worker, and `prune` runs on the machine its leftover names. `init`, `get`, `doctor` and `leftovers` are not opened by the dispatcher (`Call.opens` is false): `init` opens the home itself, the others never do. `get`, `log` and `clear_log` are `quiet`: they write no call-log line |
| Design | [Home](../../control/home/README.md), [Migration](../../control/home/migration.md), [Doctors](../../control/doctor/README.md) |
| Clients | `layerfs.home` is this family (`layerfs.home.get()`, `layerfs.home.init()`); a client's home is the one given to `client()` and is no field of it |

## Calls

```text
 home.get        reads layerfs.json, the folder's mode, the generation file,
                 bundle.json, native's record and the daemon file -> changes nothing
   --with platforms   the platforms this computer supports
   --with tree        the home's records: machines > Stores, Sandboxes; buckets
 home.init       migrate::open (create | convert | finish)
                 + the built-in machine records   -> {outcome: created | converted | unchanged}
 home.doctor     the home's own checks, then each machine, PostgreSQL Store
                 and bucket, shallowly                    -> the doctor report
 home.leftovers  walk native + ask each running machine's worker -> {items, not_listed}
 home.prune      the leftover's machine: guard, remove once     -> {freed_bytes}

 every other call ---- one line when it ends ----> <home>/trace/calls.jsonl
                                                    (calls.1.jsonl past 16 MiB)
 home.log        (filters, newest first, by cursor) <------- reads both files
 home.clear_log  (truncate calls.jsonl, remove calls.1.jsonl) -> {ok}
```

| Call | Short form | Long form |
| --- | --- | --- |
| [`home.init`](#homeinit) | `{ home }` + `unchanged`, `converted` when true | `{ home, unchanged, converted, outcome, layout, version }` |
| [`home.get`](#homeget) | `{ home, state, version }` + `generation`, `reason`, `fix` when news | `{ home, state, version, generation, reason, fix, message, layout, mode, bundle, daemon, cache }` |
| [`home.log`](#homelog) | `{ items }` + `next_cursor` when there is a next page | `{ items, next_cursor, skipped }` |
| [`home.clear_log`](#homeclear_log) | `{ ok: true }` | `{ ok, outcome: "cleared", line_count }` |
| [`home.doctor`](#homedoctor) | `{ ok: true }`, or `findings` and `not_checked` | `{ ok, findings, not_checked, status, counts, observed_at, tree }` |
| [`home.leftovers`](#homeleftovers) | `{ items }` + `not_listed` unless empty | `{ items, not_listed, observed_at }` |
| [`home.prune`](#homeprune) | `{ freed_bytes }`, or `{ ok: true }` when the size was not measured | `{ ok, freed_bytes, outcome: "removed", leftover, kind, machine, path }` |

All seven are `admin` in the MCP server.

### home.init

Create this home, or convert an older one; a current home is left as it is.

```text
CLI     layerfs home init
Node    await layerfs.home.init()
Python  layerfs.home.init()
```

| Answer | Keys |
| --- | --- |
| short | `home`; when set: `unchanged`, `converted` |
| long | `home, unchanged, converted, outcome, layout, version` |
| call | writes one call-log line; MCP `admin` |

Makes the home ready for calls, as the first call of any other kind does: an
empty or missing folder is created (0700, `layerfs.json`, `machine/native/`
and `machine/docker/` with the `machine` feature), a 1.1.0 or layout 2 home
is converted, and an interrupted conversion to layout 3 is finished. A current
home is left as it is. `outcome` is `created`, `converted` or `unchanged`;
`version` is this build's version and `layout` the layout it left. It writes
one call-log line, and the records it writes move the generation.

Refused `InvalidInput` inside a machine's worker root (not a home), `InUse`
while a layout 2 home has live Sandboxes (its conversion lists them), and as
opening a home is refused otherwise (a newer layout is `Unsupported`).

```console
$ layerfs home init
{"home":"/tmp/demo/home"}
```

### home.get

The home's state and build; with --with, the platforms this computer supports or the tree of everything in the home.

```text
CLI     layerfs home get [--with platforms,tree]
Node    await layerfs.home.get({ with })
Python  layerfs.home.get(*, with_=None)
```

| Answer | Keys |
| --- | --- |
| short | `home`, `state`, `version`; when set: `generation`, `reason`, `fix` |
| long | `home, state, version, generation, reason, fix, message, layout, mode, bundle, daemon, cache` |
| `--with platforms` | adds `host`, `items` |
| `--with tree` | adds `machines`, `stores`, `buckets` |
| call | writes no call-log line; MCP `admin` |

The home's state and what this build would use with it. It reads files only
(no process, Docker, network, lock or call-log line), so a client can poll it,
and it answers for a home that does not exist yet, creating nothing.

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

What is wrong with a bundle, a daemon or the folder's mode is the home
doctor's to report; this call answers the facts. `matches` compares a part
with the build making the call: false when the commits differ, true when
they are equal and both trees were clean, null when either commit is unknown
or a tree was dirty. The build's commit is the one the build tools set
(`LAYERFS_BUILD_COMMIT`, `tools/worktree.py`); a plain `cargo build` names
none, so its answers are null. The daemon's commit is the text after
`layerfs-build-identity:` in its file, stamped by its build script the same
way; `tools/linux_bundle.py` writes the bundle's `source.dirty`. A mismatch
is reported, not refused: a bundle from another commit with the same worker
protocol still starts.

**`--with platforms`** adds `platforms: { host, items }`: `host` is this
computer's system and `items` one row per platform this build knows (under
ten, not paged). A row's short form is `{ platform, ready }` with `missing`
when it is not empty; `ready` stays when false. Its long form is `platform,
how, backend, route, sqlite, postgresql, ready, missing, version, options,
description, socket, candidates`. It needs no home; a build without the
`machine` feature (a machine's worker) refuses it `Unsupported`. The rows
were `machine.platforms`, removed with no alias.

**`--with tree`** adds `tree: { machines, stores, buckets }` from the home's
records only ([`home_tree.rs`](../../../../control/crates/layerfs-control/src/home_tree.rs)):
it dials nothing, and is null unless the home is ready.

| Node | Long form | Short form |
| --- | --- | --- |
| machine | `machine, platform, cpus, memory_bytes, state, op, reason, fix, stores, sandboxes` | `machine`, `platform`; its state and companion unless ready; `cpus`, `memory_bytes`, `stores` and `sandboxes` when set |
| Store | `store, engine, projects, connections, state, op, blocked_by, reason, fix` | `store`, `engine`; `projects` and `connections` (`[{ machine, url }]`) unless empty; its state unless ready |
| Sandbox | `sandbox, project, image, workspaces, workspaces_not_listed, state, op, blocked_by, reason, fix` | `sandbox`, `project`; `image`, `workspaces` (`[{ workspace, branch }]`) unless empty; `workspaces_not_listed` when true; its state unless ready |
| bucket | `bucket, provider` | `bucket`, `provider` |

A sqlite Store is under its machine; a PostgreSQL Store lives on no machine
and is in the top-level `stores`, with `connections` naming where it is
reached from: first `{ machine: null, url }` (the Store's own URL), then one
row per machine with its own address. A Sandbox's `workspaces` are listed
only where the home keeps their records (on `native`, or a Sandbox reached
directly); otherwise `workspaces_not_listed` is true, and for a blocked
Sandbox `workspaces` is null. States come from the records: a recorded
operation whose process lives is `busy` with `op { name, phase, started_at }`,
one whose process is gone with its outcome unknown is `broken` `unresolved`;
a machine of a removed platform is `broken` `unsupported`; a machine recorded
stopped is `stopped`; a machine that only a Store or Sandbox names is listed
too (`native` ready and `docker` stopped, which exist with or without a
record; any other `broken` `missing`), so nothing is left out of the tree; a
Sandbox recorded gone is `broken` `missing`. A Store or Sandbox on a machine
that is not ready is `blocked`, `blocked_by { machine, reason }` naming it. A
live probe is `machine get --with live`'s and `store get --with live`'s, not
this call's.

Run on macOS native with a debug build:

```console
$ layerfs home get
{"home":"/tmp/demo/home","state":"ready","version":"2.0.0","generation":2}
$ layerfs home get --with tree
{"home":"/tmp/demo/home","state":"ready","version":"2.0.0","generation":43,"tree":{"machines":[{"machine":"docker","platform":"docker","state":"stopped"},{"machine":"native","platform":"native","stores":[{"store":"shop","engine":"sqlite","projects":["api"]}]}],"stores":[],"buckets":[]}}
$ layerfs home get --with platforms
{"home":"/tmp/demo/home","state":"ready","version":"2.0.0","generation":43,"platforms":{"host":"macos","items":[{"platform":"native","ready":true},{"platform":"docker","ready":true},{"platform":"apple-container","ready":true},{"platform":"lima","ready":true}]}}
$ layerfs home get --with live
layerfs: home.get: --with names `live`; this call takes platforms, tree
```

The last is a wrong command line (exit 2); from Node or Python the same
request is refused `InvalidInput`.

### home.log

The calls made against this home, newest first.

```text
CLI     layerfs home log [--call C] [--project P] [--client C] [--tag T] [--outcome O] [--since T] [--limit 200] [--cursor C]
Node    await layerfs.home.log({ call, project, client, tag, outcome, since, limit, cursor })
Python  layerfs.home.log(*, call=None, project=None, client=None, tag=None, outcome=None, since=None, limit=None, cursor=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `call` option | text | only this call: a family alone (`file`), or `family.call` (`file.write`) |
| `project` option | project | only calls that named this project |
| `client` option | name | only calls from this front end: cli, node, python or rust |
| `tag` option | text | only calls made with this log tag: the whole tag, or a prefix that ends in `:` |
| `outcome` option | text | only calls with this outcome word or refusal code |
| `since` option | int | only calls at or after this time, in milliseconds since 1970, as today |
| `limit` option | int | the most lines in one page, 1 to 1000; default `200` |
| `cursor` option | text | the `next_cursor` of the page before; it carries the filters |

| Answer | Keys |
| --- | --- |
| short | `items`; when set: `next_cursor` |
| long | `items, next_cursor, skipped` |
| `items[]` short | `call`, `outcome`, `logged_at`; when set: `project`, `error` |
| `items[]` long | `call, outcome, logged_at, project, error, duration_ms, client, argv, commit, tag` |
| call | pages by `limit` and `cursor`; writes no call-log line; MCP `admin` |

The calls made against this home, newest first, from `trace/calls.jsonl`
then `trace/calls.1.jsonl`. Every option given must match.

| Option | Rule |
| --- | --- |
| `call` | `family` or `family.call` (`file`, `file.write`; `-` and `_` are the same) |
| `project` | lines whose `argv` names this project |
| `client` | `cli`, `node`, `python` or `rust` |
| `tag` | an exact tag, or a prefix ending in `:` |
| `outcome` | a row's `outcome` (`ok`, `refused`, a result's word such as `committed`) or its `error` (a public refusal code) |
| `since` | lines that started at or after this time (ms since 1970) |
| `limit` | 1 to 1000 (`MAX_LIMIT`); default 200 (`DEFAULT_LIMIT`) |
| `cursor` | `next_cursor` from a previous page |

A stored line is `{ v, at, ms, client, argv, outcome, kind?, commit?, layer?,
tag? }` ([the call log](../calls/README.md#the-call-log)). A row answers it
as:

| Row key | From the line |
| --- | --- |
| `call` | `family.call`, from the first two words of `argv` |
| `outcome` | `refused` for a refused call; else the line's `kind` in snake_case (`Committed` → `committed`); else `ok` |
| `logged_at` | `at`, as UTC text to the second (`2026-11-18T09:16:59Z`) |
| `project` | the first `<store>/<project>` word of `argv`, or null |
| `error` | a refused call's code, folded to its public one; null otherwise |
| `duration_ms`, `client`, `argv`, `commit`, `tag` | `ms`, `client`, `argv` (redacted arguments as byte counts), `commit`, `tag` |

`skipped` (long form) counts lines of the page that do not parse (a torn
tail after a crash), which are never answered. A page ends at `limit` rows,
or early past 64 KiB (`PAGE_BYTES`), with a cursor. `next_cursor` carries
the call it is for and the filters that made it: a later call may leave the
filters out, a filter given again must be the same, and a cursor of another
call is refused. Errors: `InvalidInput` (a bad option, `limit` out of range,
a malformed cursor, a cursor given with other filters, or a stale one: "the
cursor is stale (the call log was rotated or cleared); read from the
start"); `Failed` (a log file cannot be read).

```console
$ layerfs home log --limit 3
{"items":[{"call":"home.leftovers","outcome":"ok","logged_at":"2026-11-18T22:05:33Z"},{"call":"home.doctor","outcome":"ok","logged_at":"2026-11-18T22:05:33Z"},{"call":"home.doctor","outcome":"ok","logged_at":"2026-11-18T22:05:32Z"}],"next_cursor":"eyJmb3IiOiJob21lLmxvZyIsImF0IjoiYzEuMC40M2M2YzMzYy4xNWEwOWRiNDRiMDkzZGZjLjM4YzUifQ"}
```

It was `logs.list`, removed with no alias (the shell's `--op` is `--call`).

### home.clear_log

Clear this home's call log.

```text
CLI     layerfs home clear-log
Node    await layerfs.home.clear_log()
Python  layerfs.home.clear_log()
```

| Answer | Keys |
| --- | --- |
| short | `ok` |
| long | `ok, outcome, line_count` |
| call | writes no call-log line; MCP `admin` |

Clears the call log: truncates `trace/calls.jsonl` in place and removes
`trace/calls.1.jsonl`. Machine and Sandbox logs are not touched.
`line_count` (long form) is the lines both files held, torn ones included.
It holds `trace/calls.lock`, waiting for it, so a line another process
writes meanwhile lands before or after the clear and is never torn. A
`home.log` cursor from before the clear is stale afterwards. Error: `Failed`
(the lock or a file cannot be opened, truncated or removed). It was
`logs.reset`, removed with no alias.

### home.doctor

Check the home, then each machine, PostgreSQL Store and bucket, shallowly.

```text
CLI     layerfs home doctor [--progress]
Node    await layerfs.home.doctor({ on_progress })
Python  layerfs.home.doctor(*, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | each when set: `ok`, `findings`, `not_checked` |
| long | `ok, findings, not_checked, status, counts, observed_at, tree` |
| `findings[]` short | `<thing>`, `check`, `severity`, `message`; when set: `fix`, `from` |
| `findings[]` long | `<thing>, check, severity, message, fix, from, evidence` |
| `not_checked[]` short | `<thing>`; when set: `check`, `blocked_by`, `reason` |
| `not_checked[]` long | `<thing>, check, blocked_by, reason` |
| call | sends progress; writes one call-log line; MCP `admin` |

Checks the home itself, then gives each machine, PostgreSQL Store and bucket
its own shallow check; a Store on a machine and a Sandbox are
`machine doctor`'s. The home's checks, in order: `home.build`,
`home.prerequisites`, `home.layout`, `home.records` (every record in the
home: references between records, file modes, leases and half-finished
operations; a finding names the thing it is about, at any depth),
`home.bundle`, `home.native_daemon`, `home.artifacts`, `home.capacity`; a
check that does not apply is left out. A stopped machine is not a problem:
its record and state are checked, and the checks that need it running do not
apply. No option changes what it checks and there is no timeout option: each
thing has its own time budget, and a check still running when it ends is
`not_checked` with `reason: timeout`. `--progress` sends one event per
finished thing, `{phase, done, total, node}`, `phase` `self` then
`machines`, `stores`, `buckets`. It changes nothing (no lock, no record, no
lease removed, no machine started) and exits 0 once its report is made. The
report shape is in [the doctors](../../control/doctor/README.md). There is
no `layerfs doctor` command: it was removed with no alias.

Run on macOS native with a debug build:

```console
$ layerfs home doctor
{"ok":true}
```

### home.leftovers

What earlier runs left behind on this computer and on each running machine.

```text
CLI     layerfs home leftovers [--machine M]
Node    await layerfs.home.leftovers({ machine })
Python  layerfs.home.leftovers(*, machine=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `machine` option | name | list only this machine's leftovers; `native` is this computer |

| Answer | Keys |
| --- | --- |
| short | `items`; when set: `not_listed` |
| long | `items, not_listed, observed_at` |
| `items[]` short | `leftover`, `modified_at`; when set: `size`, `report_only` |
| `items[]` long | `leftover, modified_at, size, report_only, removable_at, kind, machine, path` |
| `not_listed[]` short | `machine`, `message`; when set: `blocked_by`, `reason` |
| `not_listed[]` long | `machine, blocked_by, reason, message` |
| call | writes one call-log line; MCP `admin` |

What interrupted operations left on `native` and on each running machine:
one bounded walk of each side's own folders, the machines' through their
workers. It changes nothing and starts nothing; it is not paged.

`leftover` is `<machine>:<kind>:<name>`, the id `home.prune` takes; it never
holds a path. The kinds are `run_folder`, `empty_root`, `init_copy`,
`backup_staging`, `image_tmp` and `export_container` (an image export
container a killed copy left in the host's Docker, listed with `native`'s
entries once an image copy has run from the home; a Docker that does not
answer puts `native` in `not_listed`, blocked by `docker`'s `engine_down`)
([Checks: leftovers](../../control/doctor/checks.md#leftovers)). `size` is
left out when it was not measured; `report_only` marks one `home prune`
would refuse now (`removable_at` says when, if known). An image's staged
part is not a leftover: `machine images` shows it and `machine prune
--unused` removes it. A machine that was not walked, or not to the end, is in
`not_listed` as `{ machine, blocked_by, reason, message }`: `blocked_by` for
a stopped machine or one an operation holds, `reason` `timeout` (no answer,
or the walk stopped at its bound of 20,000 entries or 2 s), `busy` or
`refused`. `--machine` limits the list to one machine; an unknown one is
`NotFound`.

```console
$ layerfs home leftovers
{"items":[],"not_listed":[{"machine":"docker","blocked_by":{"machine":"docker","reason":"stopped"},"message":"machine docker is stopped"}]}
```

### home.prune

Remove one leftover that home leftovers listed.

```text
CLI     layerfs home prune <leftover>
Node    await layerfs.home.prune(leftover)
Python  layerfs.home.prune(leftover)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `leftover` | text | an id `home leftovers` answers, `<machine>:<kind>:<name>` |

| Answer | Keys |
| --- | --- |
| short | each when set: `ok`, `freed_bytes` |
| long | `ok, freed_bytes, outcome, leftover, kind, machine, path` |
| call | writes one call-log line; MCP `admin` |

Removes the one leftover `leftover` names, under that kind's guard; it is
looked up again and must still be a leftover. `freed_bytes` is what the
removed files held, left out (null in the long form) for a folder copy, an
image temporary and an export container. One attempt: nothing is retried or
sent twice, and nothing but the one entry is removed. It was `doctor.fix`,
removed with no alias.

| Kind | Removed when (else `InUse`, nothing removed) |
| --- | --- |
| `run_folder` | no Sandbox record names it, its `owner` marker is there and older than a minute, no daemon of it runs and nothing is mounted at or under it; one with no marker, or a marker under a minute old, is listed `report_only` |
| `empty_root` | its run folder is gone and it is not a mount point; `rmdir` only |
| `init_copy` | a line of `log/uncertain.jsonl` names the machine and the copy, and its deadline passed more than an hour ago (`removable_at`); a copy no line names is `report_only` and refused `InvalidInput` |
| `backup_staging` | no `backup create` of that Store is running |
| `image_tmp` | no unpack of that image is running |
| `export_container` | no copy of its image to its machine holds the copy lock (taken without waiting); `docker rm -v` removes it with its anonymous volumes |

| Refusal | When |
| --- | --- |
| `NotFound` | no leftover has that id now: an id no list gives (a path, `..`, an unknown kind or machine), an entry already removed, or a link in its place |
| `InUse` | the kind's guard does not hold; nothing was removed |
| `InvalidInput` | a `report_only` leftover; a missing `leftover` |
| `Unavailable` | the machine is stopped or does not answer; `fix: layerfs machine start <machine>` |
| `Unknown` | the worker's answer was lost; `layerfs home leftovers` shows whether the entry is still there |
| `Failed` | the mount table cannot be read, or a removal stopped at an I/O error (the message names where) |

## Related

- [Home](../../control/home/README.md), [Migration](../../control/home/migration.md).
- [Doctors](../../control/doctor/README.md), [checks](../../control/doctor/checks.md).
- [Calls: the call log](../calls/README.md#the-call-log).
