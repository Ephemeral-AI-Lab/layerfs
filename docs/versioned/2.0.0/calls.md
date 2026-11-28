# The calls

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](README.md). The source is the one call table,
one file per call under its family in
[`api/crates/layerfs-api/src/`](../../../api/crates/layerfs-api/src/)
(`<family>/<call>.rs`), and the command in `api/crates/layerfs-cli/src/`. The shell parser, the
help, and the Node and Python functions are all built from that table, so
the three front ends take the same arguments.

There are 84 calls in ten families: `home` 6, `project` 8, `branch` 6,
`commit` 3, `file` 14, `sandbox` 7, `workspace` 7, `machine` 14, `store` 13
and `backup` 6. `layerfs help` lists them, `layerfs help <family> [<call>]`
prints one family or one call, and `layerfs help --json` prints the whole
table (each call's arguments and the form of its answer). Outside the table
the command has `worker` (a machine's worker process), `help` and
`--version`.

## How a call is written

```
layerfs <family> <call> <positionals> [--options] [--text | --json]
```

```js
await layerfs.<family>.<call>(<positionals>, { <options> })
```

```python
layerfs.<family>.<call>(<positionals>, <option>=...)
```

- **Positionals go from the outside in:** the owner (project, Store,
  Sandbox or Workspace), then the Branch, then the path or new name, then
  other inputs. A Commit is the option `--at` (or `--expected` for a head
  guard), never a positional, except `commit get`'s `<commit>`.
- **Options say how.** A shell option is the option's name with `-` for
  `_`: `--old-string` is `old_string`. Every list takes `--limit N --cursor C`.
- An optional positional is simply left off. In Python, optional
  positionals and options may be keywords.
- A function option (`on_progress`, and `stdout`/`stderr` of
  `workspace.exec`) exists only in Node and Python. The shell has
  `--progress` and `--stream` instead.
- A call name with `_` is written with `-` in the shell:
  `layerfs machine load-image`, `layerfs home clear-log`.
- `detail` is every call's option and `with` the option of the calls whose
  row lists sections; they say which [form of the answer](#the-form-of-an-answer)
  is wanted. `log_tag` tags the call's line in the home's call log: an
  argument in Node and Python; without it (and always in the shell) the
  caller's `LAYERFS_LOGS_TAG` setting tags it.

### Profile and call log

Each call's row says its profile. The 17 `agent` calls are what an agent
works with, and the MCP server's default `work` profile lists only them:
`branch.fork`, `commit.list`, `commit.diff`, `file.read`, `file.list`,
`file.glob`, `file.grep`, `file.write`, `file.edit`, `file.rm`, `file.mv`,
`sandbox.create`, `workspace.list`, `workspace.exec`, `workspace.cancel`,
`workspace.commit` and `workspace.status`. The other 67 are `full`: the MCP
`admin` profile lists them with the 17. Every call is in the shell, Node
and Python; an MCP tool is named `<family>_<call>` (`workspace_exec`). Nine `quiet` calls write no line to the home's call log:
`home.get`, `home.log`, `home.clear_log`, `home.move`, `sandbox.files`, `sandbox.log`,
`machine.stats`, `machine.files` and `machine.log`.

A process that only reads a directly opened SQLite Store uses a read-only
session. The first mutation opens the writer. If that open fails, the call
returns the refusal and a later mutation can make its own attempt. On close,
the process releases its readers before closing its writer if one started.

### Names

| Thing | Form | Unique in |
| --- | --- | --- |
| machine | `dev` | the home |
| Store | `shop` | the home |
| bucket | `r2-main` | the home (apart from Stores) |
| project | `shop/api` | always written `<store>/<project>`; a bare `api` is `InvalidInput` |
| Branch | `main`, `agent` | its project |
| Commit | the full id (66 hex characters), or a prefix of 8 or more, wherever a call takes one | its project |
| Sandbox | `tests` | the home |
| Workspace | `tests:agent`, `<sandbox>:<name>`, answered by `workspace.mount` | its Sandbox |

Machine, Store, bucket, project and Sandbox names are 1 to 63 lowercase
letters, digits, `_`, `.` and `-`, starting and ending with a letter or
digit. Windows device names (`con`, `nul`, `aux`, `prn`, `com1`–`com9`,
`lpt1`–`lpt9`) are refused. The host machine (`macos-host` on macOS, `linux-host` on Linux) and `docker` name the built-in
machines and are refused (`InvalidInput`) as new machine names and as
Sandbox names (H §7.6).

### Argument kinds

| Kind | Accepts | Shell |
| --- | --- | --- |
| name | a machine, Store, bucket, Branch or Sandbox name, or a Commit id | a word |
| project | `<store>/<project>` | a word |
| image | a `sha256:` digest, a Docker image Id or a Docker reference (`postgres:17-bookworm`) | a word |
| text | a path, URL or other text | a word |
| whole number | an integer | a word; not a number is exit 2 |
| bool | true or false | `--flag` alone is true; `--flag=false` |
| JSON | any JSON value | JSON text; `file apply`'s changes come from stdin or `--from` |
| content | text or bytes | stdin, or `--from <file>`; text when UTF-8, else bytes |
| size | bytes as a number, or text with `K`, `M` or `G` (`512M`, `8G`) | a word |
| function | a callback (Node, Python only) | — |

## The form of an answer

Every call answers in one form, described in the
[call surface specification](../../../core/docs/issues/call-surface-2-0/README.md)
(its rows are `calls.json`, which a contract test compares with the built
table) and printed by `layerfs help --json` as each call's `form`.

**Short and long form.** A call answers its short form: the few
keys an agent needs, with a key left out when it is null, false or empty.
`detail` asks for the long form: every key the call declares, in a fixed
order, null where there is no value. Neither form reads anything the other
does not.

| | Shell | Node | Python |
| --- | --- | --- | --- |
| the long form | `--detail` | `{ detail: true }` | `detail=True` |
| sections | `--with platforms,tree` | `{ with: ["platforms", "tree"] }` | `with_=["platforms", "tree"]` |

- A short key is one of two sorts: always there (`home`, `state`), or there
  only when it is news (`reason`, `fix`, a `state` that is not `ready`).
- A call that changed something and has nothing to say answers
  `{"ok":true}`; `ok` is never beside another key in the short form. A
  call whose row has no `ok` (a read) answers `{}` when nothing is news.
- **Sections.** `with` adds a named part of the answer that costs a read of
  its own: `home get --with platforms`, `--with tree`. A call takes only the
  sections its row lists; another name is refused `InvalidInput`, naming the
  ones it takes (in the shell it is a wrong command line, exit 2). A section is there only when asked for: always in the long form, and in the short form only when it is not empty.
  Python spells the option `with_`, because `with` is a keyword.
- **Counts.** A list inside an answer (`overwritten`, `projects`,
  `stores`) holds its first 20 entries; its `<list>_count` is set only when
  the list was cut, else it is left out of the short form and null in the
  long form.
- **Lists.** A list call answers `{ items, next_cursor }`: `next_cursor` is
  left out of the short form on the last page and null in the long form.
  Pass it back as `cursor`. It carries the filters of the call that made it:
  a filter given again must be the same, and a cursor given to another call
  is refused `InvalidInput`. A page ends at `limit` items, or early past
  64 KiB, with a cursor.
- **State.** A thing's `state` is one of `ready`, `busy`, `stopped`,
  `blocked`, `broken` and `unknown`, each with its companion and no other:
  `busy` has `op { name, phase, started_at }`, `blocked` has
  `blocked_by { <machine|store|sandbox>, reason }` (the thing at the root of
  the block), `broken` has `reason` (a code) and `fix` (one shell line, or
  null), `unknown` has `observed_at`. A records-only read (`machine list`,
  `machine get`, `store list`, `store get`) shows `observed_at` in its short
  form only beside `unknown` or with `--with live`; the long form keeps it.
- **Refusals.** `{"error", "message"}`, `fix` when there is one, and the
  fields the next call needs flat beside them; `detail` adds `fix` (null
  when there is none) and `detail`. See [errors](#errors-and-exit-codes).

Run for this manual on a scratch home (macOS, a debug build of
`61960e3f5f`; the home's path is shortened to `/tmp/demo/home`):

```console
$ layerfs home get
{"home":"/tmp/demo/home","state":"broken","version":"2.0.0","reason":"missing","fix":"layerfs home init"}
$ layerfs home init
{"home":"/tmp/demo/home"}
$ layerfs home get
{"home":"/tmp/demo/home","state":"ready","version":"2.0.0","generation":2}
$ layerfs store create shop
{"ok":true}
$ layerfs home get --with tree
{"home":"/tmp/demo/home","state":"ready","version":"2.0.0","generation":43,"tree":{"machines":[{"machine":"docker","platform":"docker","state":"stopped"},{"machine":"macos-host","platform":"host","stores":[{"store":"shop","engine":"sqlite","projects":["api"]}]}],"stores":[],"buckets":[]}}
$ layerfs home get --with live
layerfs: home.get: --with names `live`; this call takes platforms, tree
$ echo $?
2
```

(`generation` 43: the tree was read after the rest of this page's run.)

## The calls

`[x]` is an optional positional. Every family's results are snake_case.

### `home` (6)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `init` | — | — | creates this home; a current home is left as it is |
| `get` | — | `--with platforms,tree` | the home's state and build; with `--with`, the platforms this computer supports or the tree of everything in the home |
| `log` | — | `call`, `project`, `client`, `tag`, `outcome`, `since`, `limit` (200), `cursor` | the calls made against this home, newest first |
| `clear_log` | — | — | clears this home's call log |
| `doctor` | — | `on_progress` | checks the home, then each machine, PostgreSQL Store and bucket, shallowly ([the doctors](#the-doctors)) |
| `move` | `to` | — | moves this home into another folder on the same filesystem; client folders stay |

- `init`, `get`, `doctor` and `move` are not opened by the dispatcher:
  `init` opens or creates the home itself, and the others never open it.
  `get`, `log`, `clear_log` and `move` write no line to the call log, and
  all six are `admin` in the MCP server.
- **`init`**, short: `{ home }`, with `unchanged: true` when the home was
  already current. Long: `home, unchanged, outcome, layout, version`:
  `outcome` is `created` or `unchanged`, and `version` is this build's.
  A folder that is not a LayerFS 2.0 home (a `layerfs.json` of another
  layout, or files and no `layerfs.json`) is never converted: it is
  refused `Unsupported` with the fix
  `mv <home> <home>.old && layerfs home init`, and nothing in it is written.
  A LayerFS root opened as a home (no `layerfs.json`, only folders, none
  of LayerFS's own names, such as `~/.layerfs` holding `home/` and `dsh/`)
  is refused `InvalidInput` with the fix `export LAYERFS_ROOT=<folder>`; it
  is never moved aside.
- **`get`**, short: `{ home, state, version }`, with `generation` when the
  home is ready, and `reason` and `fix` when it is broken (`missing`: no
  home there, fixed by `layerfs home init`; `outdated`: a folder that is
  not a LayerFS 2.0 home, or one whose `layerfs.json` cannot be parsed or
  names no layout this build knows, fixed by
  `mv <home> <home>.old && layerfs home init`; `failed`, with no fix: the
  system refused to read `layerfs.json` or list the folder, a permission or
  I/O error, which the long form's `message` names; `unsupported`: a
  LayerFS root opened as a home, fixed by `export LAYERFS_ROOT=<folder>`). Long:
  `home, state, version, generation, reason, fix, message, layout, mode,
  bundle, daemon, cache`. `mode` is the folder's permission bits as text
  (`"0700"`); `bundle` and `daemon` are
  `{ path, source, present, commit, dirty, matches }` for the Linux bundle
  and the host machine's daemon this build would use (`matches` is null when either
  side cannot tell); the bundle's `source` is `env:LAYERFS_BUNDLE`,
  `runtime` (an installed runtime's own, `<runtime>/bundle/linux-<arch>/`
  beside `<runtime>/bin/layerfs`, chosen after `LAYERFS_BUNDLE` and before
  the cache) or `cache`; `cache` is `{ path }`. It reads `layerfs.json`, the
  folder's mode, `bundle.json`, the host machine's record and the daemon
  file, and changes nothing. The answer has no `warnings` list any more:
  what is wrong with a bundle, a daemon or the folder's mode is the home
  doctor's to say.
- `generation` is a counter that moves each time a record of the home is
  written or removed (a machine, Store, project, bucket, Sandbox or
  Workspace record, a saved login) and never for a read, an observation
  file (`observed_at`) or a command's own records: a caller that kept a
  tree reads it again only when the number differs. Every entity record
  write moves it once, in the one function that writes records.
- **`get --with platforms`** adds `platforms: { host, items }`: `host` is
  this computer's system and `items` one row per platform this build knows.
  A row's short form is `{ platform, ready }`, with `missing` when something
  is; its long form has `platform, how, backend, route, sqlite, postgresql,
  ready, missing, version, options, description, socket, candidates`. It
  needs no home. (1.x's `machine.platforms` is gone, with no alias; these
  are its rows.)
- **`get --with tree`** adds `tree: { machines, stores, buckets }`, read
  from the home's records only: each machine with its sqlite `stores` and
  its `sandboxes`, the PostgreSQL Stores (which live on no machine) and the
  buckets. A node's short form is what names it (machine `machine,
  platform`; Store `store, engine`; Sandbox `sandbox, project`; bucket
  `bucket, provider`), then what is news: its state only when it is not
  `ready`, a machine's `cpus` and `memory_bytes`, a Store's `projects` and
  `machines`, a Sandbox's `image`, `workspaces` (each `{ workspace,
  branch }`) and `workspaces_not_listed`. The long forms are: machine
  `machine, platform, cpus, memory_bytes, state, op, reason, fix, stores,
  sandboxes`; Store `store, engine, projects, machines, state, op,
  blocked_by, reason, fix`; Sandbox `sandbox, project, image, workspaces,
  workspaces_not_listed, state, op, blocked_by, reason, fix`; bucket
  `bucket, provider`. A PostgreSQL Store's `machines` are the names of
  the machines attached to it (below); a sqlite Store's are null. A Workspace is in the home's
  records only on the host machine and for a Sandbox reached directly: any other
  Sandbox answers `workspaces` null and `workspaces_not_listed: true`. A Store or Sandbox whose machine is stopped, busy or
  broken is `blocked`, and `blocked_by` names that machine. A machine that
  only a Store or Sandbox names, with no record of its own, is listed too
  (`broken`, `missing`, unless it is the host machine or `docker`), so nothing in the
  home is left out. A record that cannot be read does not fail the tree:
  an unreadable machine record is a `broken` `failed` machine, a Store
  record whose machine is not known (unreadable, or of a kind this build
  does not read) is among the `stores` on no machine with its state, and
  an unreadable Sandbox or Workspace record is left out; each kind's
  doctor names the record.
- **`log`** reads the home's call log (`trace/calls.jsonl`, then the one
  file it rotates to), newest first. A row's short form is
  `{ call, outcome, logged_at }`, with `project` when the call named one and
  `error` when it was refused; the long form adds `duration_ms`, `client`,
  `argv`, `commit` and `tag`. `outcome` is `refused` for a refused call
  (its public code is `error`), else the result's word in snake_case, else
  `ok`. `logged_at` is UTC, to the second. The filters: `call` (such as
  `file.write`), `project`, `client` (`cli`, `node`, `python`, `rust`),
  `tag` (exactly, or a prefix when it ends with `:`), `outcome` (a row's
  `outcome` or its `error`) and `since` (milliseconds since 1970). `limit` is 1 to 1000, 200 by
  default. The long form adds `skipped`: the lines of the page that could
  not be read. A cursor from before a rotation or a `clear_log` is stale
  and refused `InvalidInput`. An argument the log redacts (a path, a
  command, a message, edit text, a URL, a pattern, content) is in `argv` as
  its byte count, never its value.
- **`clear_log`** truncates the call log in place and removes the rotated
  file. Short: `{"ok":true}`; long: `{ ok, outcome: "cleared", line_count }`.
- What interrupted operations left on disk (a run folder or private root
  of a Sandbox start or removal, an image unpack temporary or export
  container of a killed copy, a staged backup snapshot, a project Init's
  folder copy on a machine) LayerFS removes itself, at fixed points in
  later calls and only where it can prove nothing owns it: a machine's
  worker sweeps its run root when it starts and before each `sandbox
  create`, and the host machine before each `sandbox create`; the next image copy
  to a machine sweeps the temporaries and export containers of every image
  no copy holds; the next `backup create` of a Store, or a `backup restore`
  that replaces it, removes its staged snapshots; and the call that settles
  a project Init (the Init itself, or `project delete` of a project whose
  Init's owner is gone) removes its folder copy. Each sweep is one attempt
  inside 20,000 directory entries or 2 s and never fails the call it rides
  on; each removal, and each one that failed and waits for the next sweep,
  is a line of `log/sweep.jsonl`. `machine stats --with space` still counts
  run folders as `other_runs`.
- `home leftovers` and `home prune` were removed in spec v20 (owner
  2026-11-19) with no alias; both exit 2.
- `home.log` and `home.clear_log` were `logs.list` and `logs.reset`, and
  `doctor.fix` is gone (the `doctor` family is gone); the old names are
  gone, with no alias. A row's `at`, `ms`, `lines`, `has_more`
  and `next` became `logged_at`, `duration_ms`, `items` and `next_cursor`.
  The home's layout and settings are in [the home](home.md).

```console
$ layerfs home log --limit 5
{"items":[{"call":"store.get","outcome":"refused","logged_at":"2026-11-18T13:32:21Z","error":"NotFound"},{"call":"store.create","outcome":"created","logged_at":"2026-11-18T13:32:21Z"},{"call":"home.init","outcome":"created","logged_at":"2026-11-18T13:32:21Z"}]}
$ layerfs home log --limit 2 --detail
{"items":[{"call":"store.get","outcome":"refused","logged_at":"2026-11-18T13:32:21Z","project":null,"error":"NotFound","duration_ms":0,"client":"cli","argv":["store","get","nope"],"commit":null,"tag":null},{"call":"store.create","outcome":"created","logged_at":"2026-11-18T13:32:21Z","project":null,"error":null,"duration_ms":25,"client":"cli","argv":["store","create","shop"],"commit":null,"tag":null}],"next_cursor":"eyJmb3IiOiJob21lLmxvZyIsImF0IjoiYzEuMC40MzM3OTRiMy4zMGMwOGNmMTE1MjBlYTc2LjU3In0","skipped":0}
$ layerfs home clear-log --detail
{"ok":true,"outcome":"cleared","line_count":4}
```

(Four lines: the three above and the refused `file read` shown under
[errors](#errors-and-exit-codes), all from the same run.)

- **`move <to>`**, short: `{ home }`, with `left` when anything stayed in
  the old folder. Long: `home, from, outcome, moved, left, rewritten`. It
  renames the home's own entries (LayerFS's names and `runtime/`, `job/`,
  `stat/`, `preset/`) into `<to>`, a new or empty folder on the same
  filesystem that may sit inside the old one, and rewrites the paths into
  them that records hold (a machine's `daemon` and `image_archive`, a
  Sandbox's `daemon`, each run folder's `owner`). A client's `dsh/` or
  `connect/` stays. Refused before any effect while a host-machine Sandbox
  is open, a Store is open in another process or the host worker runs
  (`InUse`), when `<to>` is not empty (`Exists`) or on another filesystem
  (`Unsupported`). It never copies: a copy keeps the id and would drive the
  same machines.

```console
$ layerfs home move ~/.layerfs/home
{"home":"/Users/me/.layerfs/home","left":["dsh"]}
```

### `project` (8)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `init` | `project`, `folder` | `timeout` (ms, 3600000: one hour), `on_progress` | imports a folder on this computer as a new project into an existing Store |
| `list` | — | `store`, `limit` (100), `cursor`, `--with stats` | every project in the home, or one Store's |
| `get` | `project` | — | one project: its Store, state and main's head |
| `stats` | `project` | `branch` (default `main`), `at` | entry counts and size of the tree at a Branch's head or a Commit |
| `export` | `project`, `destination` | `branch` (default `main`), `at`, `on_progress` | exports a Branch's committed tree to a folder on this computer |
| `delete` | `project` | — | retires one project in its Store and removes its home entry |
| `hide_path` | `project`, `path` | — | hides one absolute path from the project's whole history; frees no bytes |
| `doctor` | `project` | `full`, `on_progress` | checks one project's record against its Store's own list; `--full` reads its data ([the doctors](#the-doctors)) |

A project's `state` is `ready`, `busy` (an Init or a delete of it runs:
`op {name, phase, started_at}`, `name` being `init` or `delete`),
`blocked` (its Store, or that Store's machine, is not ready, or the Store
cannot be read: `blocked_by {store | machine, reason}`) or `broken` with
`reason: unresolved` (a delete, or an Init, that stopped part-way, with
`fix: layerfs project delete <project>`). A project is never `stopped` or
`unknown`. While a call runs on a project its record holds the call's `op`;
a Store whose records say it is not ready is not asked, and any other
Store's own list is read where its calls run. A project on record that its
Store does not list is hidden from `list` and `NotFound` to `get`, as
before (`project doctor` reports it as a notice). An export is a read and
records no `op`.

- **`init`** never creates a Store; a missing one is `NotFound`. `folder`
  is an absolute path on this computer. Into a sqlite Store on the host machine or
  a PostgreSQL Store, Init runs in the caller's process. Into a sqlite
  Store on another machine (which must be running), the caller copies the
  folder to that machine's worker, which runs Init on the copy and then
  deletes it (#536 H11, H11a): the machine needs free room for the copy
  while Init runs (`Capacity` otherwise). A missing folder, or one with a
  socket, FIFO or device file, is refused before anything is sent. The
  project's entry is claimed first (a name the home already holds is
  `Exists`), so the project is listed `busy` while Init runs; a refusal
  removes the entry, and an outcome that is not known (a lost reply) keeps
  it `broken` `unresolved`. Progress events are `{phase, done, total}`:
  `check`, `send` (bytes, to a worker only), `install`, `record`. Short:
  `{ commit }`, main's first Layer; long: `commit, outcome ("created"),
  project, store, engine, machine, root, created_at`.
- **`list`** pages by Store, then name: `limit` is 1 to 1000, 100 by
  default, and a page also ends early past 64 KiB of rows. The
  `next_cursor` carries the `--store` filter only; one given with another
  filter, or to another call, is `InvalidInput`. A row's short form is
  `{ project }` with its `state` and companion when not `ready`; its long
  form adds `store, engine, machine, commit` (main's head, null for a
  project whose Store was not read) and `created_at`. `--with stats` adds
  `size` to each row: the length main's head Layer records, one record per
  project and no tree walk; it is left out for a blocked project.
- **`get`** short: `{ store, state }` with `commit` (main's head) and the
  state's companion; long: `store, state, commit, op, blocked_by, reason,
  fix, project, engine, machine, created_at`. It takes no `--with`.
- **`stats`** counts files, folders and symlinks by a bounded walk of the
  tree at the Branch's head or, with `--at`, at one Commit of its history
  (a full id or a prefix of 8 or more; a prefix more than one Commit starts
  with is `InvalidInput` with `matches`). Short: `{ file_count, dir_count,
  symlink_count, size }`, with `partial: true` when the walk stopped at its
  work bound (`size` is the length the counted node records, less the
  files `project hide-path` hides, as the counts leave them out; whole even
  when the walk stops, less only the hidden files it reached); long adds
  `partial, commit, root`.
- **`delete`** is refused `Busy` while a Sandbox of the project is recorded
  (`detail.related` lists each as `{kind: "sandbox", name}`) or while
  another call's `op` holds the project. Short: `{"ok":true}`, or
  `{"home_entry_kept":true}` when the home's entry could not be removed
  (the project is retired; run `delete` again to remove the entry); long:
  `ok, home_entry_kept, outcome ("deleted"), deleted_at`. Running it again
  clears a `broken` `unresolved` project, including the entry of an Init
  that never reached the Store.
- **`hide_path`** (was `remove_file`, removed with no alias) writes a
  marker, not a Commit: it hides the path across the project's history and
  frees no bytes; the marker frontier governs later re-creations. Short:
  `{"ok":true}`, or `{"unchanged":true}` for a path already hidden; long:
  `ok, unchanged, outcome ("hidden" or "unchanged"), path,
  frontier_serial, hidden_at`.
- `list`'s `[store]`, `page_size` and `page_pos` became `--store`,
  `--limit` and `--cursor`; `stats`'s `[branch]` and `[commit]` became
  `--branch` and `--at`.

`project export` ([reference](../../architecture/api/sdk/project.md#projectexport)) exports a committed Branch snapshot. `destination` is an absolute path
on this computer, even for a worker Store. It must be absent or an empty
real directory, outside the home, with an existing parent. A nonempty destination
is `InvalidInput` (raised as `NotEmpty`); files and symlinks are refused. Same-parent staging is published
by atomic rename only after all bytes and metadata are written; failures clean
up this call’s staging without deleting unrelated files.

Export takes a Branch (`main` by default) and pins its head, or with `--at`
one Commit of its history (a full id or a prefix of 8 or more, as `stats`
takes it). It pins the node/root once, excludes uncommitted Workspace
changes and is unaffected by later publications. SQLite reads locally or
streams bounded frames from its worker; PostgreSQL reads in the caller’s
process, for local or remote servers. A required stopped worker is
`Unavailable` and is never started automatically. The fidelity contract
includes binary files, empty directories, `.git`, ignored files, symlinks,
permission bits and nanosecond `mtime` where the host supports them. Host
name/case collisions are `Conflict`, rather than silent overwrites.

```bash
# Example, not run: /exports exists and /exports/api is absent or empty.
layerfs project export shop/api /exports/api --branch agent --progress
```

Short: `{ file_count, size }` (`size` the bytes written); long adds
`dir_count, symlink_count, commit` (the exported node, its full id) and
`root`. Progress on stderr is JSON Lines of `{phase, done, total}`:
`prepare`, `export` and `finalize`, `done` the bytes written so far and
`total` null; `finalize` is the last event and comes only once the folder
is in place. Events arrive at most every 200 ms.

Run for this manual (macOS `macos-host`, a debug build of `61960e3f5f`, a scratch
home; ids shortened to 8 characters and `…`):

```console
$ layerfs project init shop/api /tmp/demo/src --progress
{"phase":"check","done":null,"total":null}
{"phase":"install","done":null,"total":null}
{"phase":"record","done":null,"total":null}
{"commit":"32919f55…"}
$ layerfs project list
{"items":[{"project":"shop/api"}]}
$ layerfs project list --with stats --detail
{"items":[{"project":"shop/api","state":"ready","op":null,"blocked_by":null,"reason":null,"fix":null,"store":"shop","engine":"sqlite","machine":"macos-host","commit":"32919f55…","created_at":"2026-11-18T22:04:02Z","size":19}],"next_cursor":null}
$ layerfs project get shop/api
{"store":"shop","state":"ready","commit":"32919f55…"}
$ layerfs project stats shop/api
{"file_count":2,"dir_count":1,"symlink_count":0,"size":19}
$ layerfs project hide-path shop/api /x.txt
{"ok":true}
$ layerfs project export shop/api /tmp/demo/export --branch agent --progress
{"phase":"prepare","done":null,"total":null}
{"phase":"export","done":0,"total":null}
{"phase":"export","done":14,"total":null}
{"phase":"finalize","done":14,"total":null}
{"file_count":2,"size":14}
$ layerfs project doctor shop/api --full --progress
{"phase":"data","done":0,"total":3}
{"phase":"data","done":1,"total":3}
{"phase":"data","done":2,"total":3}
{"phase":"data","done":3,"total":3}
{"phase":"self","done":1,"total":1,"node":{"project":"shop/api","status":"ok","coverage":"full","checks":[{"check":"project.record","outcome":"pass","from":null},{"check":"project.listed","outcome":"pass","from":null},{"check":"project.data","outcome":"pass","from":null}],"blocked_by":null,"reason":null}}
{"ok":true}
```

`symlink_count: 0` stays in the short form: an integer 0 is a value; only
a `<list>_count` beside a list that was not cut is left out.

### `branch` (6)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `list` | `project` | `prefix`, `exclude_prefix`, `limit` (100), `cursor` | a project's Branches, `main` included, in name order |
| `get` | `project`, `branch` | — | one Branch and its head Commit |
| `fork` | `project`, `name`, `[from]` | `at` | a new Branch from another Branch's head, or from a Commit |
| `merge` | `project`, `branch`, `from` | `into`, `units` (JSON), `expected`, `message` (`-m`) | merges one Branch into another, or into a new Branch |
| `publish` | `project`, `branch` | `at`, `expected`, `force` | a Branch's head or Commit as `main`'s next head, refusing a stale base |
| `remove` | `project`, `branch` | — | removes one Branch; its name stays taken until compaction |

Where a call takes a Commit (`--at`, `--expected`, `commit get`, a
`commit diff` side), it is a full id (66 hex characters) or a prefix of 8 or
more, resolved by the Store in one call. A prefix more than one Commit
starts with is refused `InvalidInput`, its detail's `matches` saying how
many; one no Commit starts with is `NotFound`. Where a call takes a ref or
`from`, a Branch of that name comes first, then a Commit. On `main` a
publication answers as a Commit. Every id in an answer is the full id.

- **`list`** pages by name. A row's short form is `{ branch, commit }`
  (`commit`: the head Commit, or the publication a Branch with no Commit of
  its own stands on); its long form adds `forked_from`, `id`, `base` and
  `root`. `--prefix` keeps only the names that start with it,
  `--exclude-prefix` leaves those out (such as `dsh.`), and the page does
  not walk the names it leaves out. `limit` is 1 to 1000, 100 by default.
  The `next_cursor` carries the filters only; a page always lists the
  Branches there are now. `get` answers one row, by its name.
- **`fork`** forks from the head of `from` (`main` when it is left off),
  or with `--at` from one Commit of `from`'s history (`NotFound` when it is
  not in it). Short: `{ commit }`, the Commit the new Branch stands on;
  long: `commit, outcome ("forked"), branch, base, root`.
- **`merge`** merges `from` into `branch`: a fast-forward when `branch`
  holds nothing `from` lacks, else one merge Commit on `branch`, its first
  parent `branch`'s head. When `branch` already holds `from` nothing changes
  and the answer says `unchanged`. `--into NEW` writes the result to a new
  Branch and leaves `branch` as it is (1.1.0's form). `--expected C`
  refuses `Conflict` unless `branch`'s head is `C`, its `commit` being the
  head it is at. `main` takes changes only by `publish`; merging
  into it without `--into` is `InvalidInput`, with the fix
  `layerfs branch publish <project> <from>`. A conflict changes nothing and
  is `Conflict` with `commit` (`branch`'s head), `conflicts` (the first 20
  paths) and `conflicts_count` only past those, beside the code. Short: `{ commit }`, with
  `unchanged`, `fast_forward`, `overwritten` (the paths one side's later
  write took, the first 20) when they are set and `overwritten_count` only
  past those; long:
  `commit, unchanged, fast_forward, overwritten, overwritten_count, outcome
  ("merged", "fast_forward" or "unchanged"), branch, parent, second_parent,
  merge_base, root, units, units_count`.
- **`publish`** publishes the Branch's head, or with `--at` one Commit of
  its history (a Commit `main` already holds answers `unchanged`).
  `--expected C` refuses `Conflict` unless `main`'s head is `C`; without it
  the call is refused `Conflict` when `main` gained a publication since the
  Branch forked (merge `main` in and publish again), and `--force` publishes
  over it. Either refusal's detail names `main`'s head as `commit`. Short:
  `{ commit }` (the new publication), with `unchanged` and `previous`
  (`main`'s head before, when it was not the Branch's base); long:
  `commit, unchanged, previous, outcome ("published" or "unchanged"), branch,
  source_commit, base, root, size`.
- **`remove`** short: `{"ok":true}`; long: `{ ok, outcome: "removed",
  branch, removed_at }`.
- `branch.commits`, `branch.get_commit` and `branch.diff` are gone, with no
  alias: they are `commit.list`, `commit.get` and `commit.diff`. `fork`'s
  `from_branch` and `from_commit` became `from` and `--at`; `merge`'s
  `name`, `a` and `b` became `branch`, `from` and `--into`; `publish`'s
  `commit`, `expected_base` and `expected_head` became `--at` and
  `--expected` (the Branch's head is no longer checked); `list`'s
  `page_size` and `page_pos` became `limit` and `cursor`.

Run for this manual (macOS `macos-host`, a debug build of `61960e3f5f`, a scratch
home; ids shortened to 8 characters and `…`):

```console
$ layerfs branch fork shop/api agent main
{"commit":"32919f55…"}
$ layerfs branch list shop/api
{"items":[{"branch":"agent","commit":"32919f55…"},{"branch":"main","commit":"32919f55…"}]}
$ layerfs branch merge shop/api agent main --into merged
{"commit":"12d12fbd…","fast_forward":true}
$ layerfs branch publish shop/api agent
{"commit":"32220c54…"}
$ layerfs branch publish shop/api agent --detail
{"commit":"32220c54…","unchanged":true,"previous":null,"outcome":"unchanged","branch":"agent","source_commit":null,"base":null,"root":null,"size":null}
$ layerfs branch remove shop/api merged
{"ok":true}
```

### `commit` (3)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `list` | `project`, `branch` | `at`, `limit` (100), `cursor` | a Branch's Commits, newest first; on `main`, its publications |
| `get` | `project`, `commit` | `branch` | one Commit, by its id or a prefix of 8 or more characters |
| `diff` | `project`, `ref` | `from`, `path`, `--with patch`, `limit` (100), `cursor` | the paths that differ between two Commits or Branches |

- **`list`** starts at the Branch's head, or at `--at` (`NotFound` when it
  is not in the Branch's history). A row's short form is `{ commit }`, with
  `message`, `message_truncated` (the message is cut at 128 characters in a
  list; `get` answers it whole) and `source_branch` (a publication's Branch)
  when they are set. The long form adds `branch, parent, second_parent,
  base, root, via, created_at, merged_from, source_commit, size,
  new_canonical_bytes, new_stored_bytes, new_metadata_bytes`. `via` is how
  the Commit was made: `workspace`, `file` or `merge`, and on `main`
  `publish` or `init`. `limit` is 1 to 1000, 100 by default; a page also
  ends early at 64 KiB of rows. The `next_cursor` carries the Commit the
  first page started at, so a later page lists the same history while the
  Branch moves on, and is `NotFound` once that Commit is gone.
- **`get`** reads one Commit of the project, in one Store call, without
  naming its Branch; `--branch B` refuses `NotFound` unless the Commit is in
  `B`'s history. Short: `{ branch, parent, message }`, each when it is set.
  The long form has the list row's keys (no `message_truncated`) and
  `displaced`, `overwritten` (the paths a merge took from the side with the
  later write, the first 20), `overwritten_count` (only when that list was
  cut) and `units`.
- **`diff`** compares `from` to `ref`. Without `--from` the side compared
  from is a Branch's fork point, a Commit's parent, the base of a Branch's
  first Commit, and for `main`'s first publication the empty tree (every
  path `added`). `--path P` keeps only `P` and what is under it. A row's
  short form is `{ path, change }` (`added`, `removed`, `modified`, …);
  its long form adds `type`, `content`, `metadata` and `identity` (which of
  them differ). `--with patch` adds each changed text file's lines as
  `hunks` (rows `{op, text}`, `op` one of `' '`, `-`, `+`; both keys in
  the short form, a blank line's empty `text` too; an added file's lines
  all `+`, a removed file's all `-`; every `\n` ends a line) and
  `hunks_omitted`. The answer's long form adds `from`, `to`,
  `from_root` and `to_root` (`from` and `from_root` null for the empty
  tree). The `next_cursor` carries both Commits the first page compared and
  the filters, so a later page compares the same two Commits; a cursor given
  with other filters, or to another call, is `InvalidInput`.
- `commit.list` absorbs `branch.commits` (`page`, `page_size`,
  `at_commit` and `include_storage` became `--at`, `limit` and `cursor`; the
  storage figures are always in the long form), `commit.get` absorbs
  `branch.get_commit` and no longer takes a Branch positional, and
  `commit.diff` absorbs `branch.diff` (`from_branch`, `to_branch`,
  `paths_only`, `page_size` and `page_pos` became `ref`, `--from`, `--with
  patch`, `limit` and `cursor`).

Run for this manual (macOS `macos-host`, a debug build of `61960e3f5f`, a scratch
home; ids shortened to 8 characters and `…`):

```console
$ layerfs commit list shop/api agent --limit 2
{"items":[{"commit":"12d12fbd…","message":"Apply filesystem changes"},{"commit":"1244862d…","message":"Apply filesystem changes"}],"next_cursor":"eyJmb3Ii…"}
$ layerfs commit list shop/api main
{"items":[{"commit":"32220c54…","source_branch":"agent"},{"commit":"32919f55…"}]}
$ layerfs commit diff shop/api agent
{"items":[{"path":"/README.md","change":"modified"},{"path":"/docs/a.txt","change":"modified"},{"path":"/x.txt","change":"added"},{"path":"/y","change":"added"}]}
$ layerfs commit diff shop/api agent --with patch --path /docs/a.txt
{"items":[{"path":"/docs/a.txt","change":"modified","hunks":[{"op":" ","text":"one"},{"op":"-","text":"two"},{"op":"+","text":"three"},{"op":" ","text":""}]}]}
```

### `file` (14)

1.1.0's `fs` family, renamed (O13). Paths are absolute, starting with `/`.
On macOS and Linux, the CLI uses the host worker for `file read`,
`file stat`, and one bounded `file list` page on local SQLite when the
socket path and request fit. It selects the direct route before dialing
when they do not fit. An error identifying the worker's home refuses the
call before any send. The CLI reports an uncertain exchange as `Unknown`
with a request ID, including when the success payload has an invalid
shape. If writing stdout fails after a valid result, the CLI reports the
known successful call on stderr and exits 125. The same three reads on a
PostgreSQL Store go to the same worker, which keeps their connections open
between calls ([PostgreSQL Stores](machines-and-stores.md#postgresql-stores));
a Store connected before this release stays direct until `store connect` is
run again. So do the CLI's file writes on such a PostgreSQL Store (below).
SDK local calls and other CLI PostgreSQL calls run in the caller, and a SQLite Store on another machine runs through that machine's
worker. See [routes](machines-and-stores.md#routes).
A call that finds no ready worker (none running yet, one exiting after
its idle minute, or one still starting) runs directly in the CLI, chosen
before anything is sent, and starts the worker in the background without
waiting, so the next call uses it; `LAYERFS_TRACE` records the reason
(`worker.route`). A start that fails does not fail the call; `machine
doctor` names it.

The CLI's writes on the same local SQLite Stores (`write`, `edit`, `mkdir`,
`rm`, `cp`, `mv`, `apply`) also run in the host worker, each one call
with one Commit (or none when nothing changed). `file write`'s content and
an unscoped `file apply`'s changes stream from stdin or `--from` in bounded
frames, so neither the CLI nor the worker holds the whole input; there is
no new size limit. Input that is not a regular file (a pipe, FIFO or
terminal) is first copied into a private unnamed file under the home
(`machine/<host>/spool/`, removed on any exit), which takes disk equal to
the input on the home's filesystem until it is sent (or, on the direct
route, read); so the worker reads at disk speed, and a slow producer is
never timed out. The copy is made without checking free space first: on a
nearly full disk it fails and the call runs in the caller, but a copy that
fits can leave too little room for the Commit itself, which is then
refused `Capacity`, nothing committed. One `file
apply` change is still built whole by the CLI (the engine needs an edit's
text whole), a write's content included; its content then streams. A list
with any other change past the worker's budget for one change (4 MiB
decoded) runs in the caller instead, decided before anything is sent. A
scoped `file apply --scope` runs in the caller. A write that streams
input (`write`, an unscoped `apply`) goes to the worker's own write
socket, `writes.sock`, and runs on 4 write threads of their own; the
single-change writes (`edit`, `mkdir`, `rm`, `cp`, `mv`) go to the
ordinary socket and run as reads do. So held writes never take a place or
thread of reads. Calls wait: a write queued behind busy write threads
sends nothing until the worker asks for its input, and is never timed out
for waiting; past 32 held writes further writers wait to connect, and only
past their connect bound (30 s) are refused `Unavailable`, naming the
load, before sending anything. A held writer that is killed gives its
place back within seconds. A worker that is stuck leaves the CLI waiting
with nothing effective sent: until the input's end is sent nothing can
commit, so Ctrl-C is safe.
While the worker drains, a write not yet running is refused `Unavailable`
before any effect; a running one finishes. Input that delivers less than
262,076 bytes (four engine records) in 30 s of the worker reading it is
refused `Unavailable`, nothing committed; keepalives do not count as
input. The CLI copies only pipes, FIFOs and terminals, never a regular
file, so a regular file on a stalled filesystem (a hung network mount, for
one) can still meet this rule.
A call whose Store or socket path does not qualify reads its input whole
and runs in the caller, as before; so does a single-change write whose
arguments pass 64 KiB. Input that stops before its end (a killed CLI, a
broken pipe, JSON that does not parse) commits nothing: the CLI refuses
the call, with the worker's refusal when it refused a change early. Once
the whole input was sent, a lost answer is `Unknown` with a request ID;
check the Branch before writing again, since nothing is resent. The worker
keeps the outcome of a call it could not answer, and an outcome its Store
could not settle, in `machine/<host>/retained/<request id>.json`;
`machine doctor` reports them as a `machine.worker.retained` notice, whose
fix is `rm` of the records once their Branches were checked. At most 256
are kept: the oldest `undelivered` records make room, `uncertain` ones are
never removed by the worker. A record the worker cannot write is not
retried: the `Unknown` answer says `recorded: false`, and the `machine
stop` that ends the worker lists those up to its drain
(`unrecorded_writes`). `machine stop` never stops a running write: it
reports `running_writes` and "N writes still running; see worker.log"
(`running_writes_note`), and `InUse` while such a write holds its Store;
the write finishes, and the worker exits after it. A definite refusal is
never recorded. CLI writes on a PostgreSQL Store run in the same worker,
on writable connections it keeps open between calls, with the same
streaming, waiting, outcome records and `Unknown` boundary; a Store
connected before this release, or with the host machine stopped, keeps
them in the caller
([PostgreSQL Stores](machines-and-stores.md#postgresql-stores)).

The host worker's SQLite cache keeps up to 16 Stores open per home.
It checks the Store's current record before serving a call, including a
call on a warm Store. A maintenance operation can ask the worker to close
an idle Store and release its lease. The worker acknowledges the close
after it removes its lease. A Store that is opening, serving a call,
checkpointing or closing reports `InUse` without waiting. The worker
leaves WAL frames from other openers in place.

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `read` | `project`, `branch`, `path` | `at`, `line` (1), `limit` (2000) | read a text file, or a window of its lines |
| `stat` | `project`, `branch`, `path` | `at` | a path's type, size and mode |
| `list` | `project`, `branch`, `path` | `at`, `limit` (100), `cursor` | a folder's entries |
| `glob` | `project`, `branch`, `pattern`, `[path]` (`/`) | `exclude`, `at`, `limit` (100), `cursor` | paths under a folder that match a glob |
| `grep` | `project`, `branch`, `pattern`, `[path]` (`/`) | `include`, `exclude`, `ignore_case`, `at`, `limit` (100), `cursor` | lines under a folder that contain a literal text |
| `history` | `project`, `branch`, `path` | `at`, `traversal` (`provenance`), `limit` (100), `cursor` | the Commits that changed one path, newest first |
| `blame` | `project`, `branch`, `path` | `at`, `line` (1), `limit` (2000) | the Commit that last changed each line of a text file |
| `write` | `project`, `branch`, `path`, `content` | `mode`, `expected`, `message` | write a file; one Commit |
| `edit` | `project`, `branch`, `path` | `old_string`, `new_string` (both required), `replace_all`, `expected`, `message` | replace a text in a file; one Commit |
| `rm` | `project`, `branch`, `path` | `recursive`, `expected`, `message` | remove a path; one Commit |
| `mkdir` | `project`, `branch`, `path` | `mode`, `expected`, `message` | make a folder and its missing parents; one Commit |
| `mv` | `project`, `branch`, `from`, `to` | `expected`, `message` | move a path; one Commit |
| `cp` | `project`, `branch`, `from`, `to` | `expected`, `message` | copy a path, a folder with everything in it; one Commit |
| `apply` | `project`, `branch`, `changes` (JSON) | `expected`, `message` | apply a list of changes; one Commit, all or none |

- **Reads** read the Branch's head, or `--at C`: a full Commit id or a
  prefix of 8 or more of its characters (a prefix more than one Commit
  starts with is `InvalidInput` with `matches`, a count; a Branch name or
  a shorter prefix is `InvalidInput`, never read as a Branch). Every long form
  names the Commit read as `commit` (its full 66-character id), its `root`
  and the `path`. `read` answers `text` (an empty file `""`), `next_line`
  while lines are left (none after a cut last line) and `line_truncated`
  when a long line was cut; a file that is not text answers `binary: true`
  and its `size` instead of `text` (the short form shows `size` only then;
  the long form always). `stat` answers `type` (`file`, `dir` or `symlink`), `size` and
  `mode` as octal text (`"0644"`); the long form adds `inode` and
  `modified_at`. `blame` answers `ranges` (`{start, end, commit}`, runs of
  lines with one owner; a range without `commit` has none found) and
  `commits` (each owner once: `{commit, message}`, the long form adds
  `created_at`), `next_line`, and `partial` when its history budget ran out
  or a line was cut.
- **Pages**: `list`, `glob`, `grep` and `history` answer `items` and
  `next_cursor`. `limit` is 1 to 1000; a page also ends early at the reply
  budget, with a cursor. The cursor carries the call's filters and the
  Commit its first page read, so a later page reads the same tree while the
  Branch moves on. A cursor given with other filters, or to another call,
  is `InvalidInput`; one whose Commit is gone, `NotFound`. A `list` row is
  `{path, type}` (long: `inode`), a `glob` row a path, a `grep` row `{path,
  line, text}` (the line without its end) with `line_truncated`; the long forms add `skipped_hidden`
  (entries `project hide-path` hides) or, for `grep`, `skipped_binary` (the
  files that are not text). `grep` searches literal text; `--ignore-case`
  folds ASCII letters. `--include` and `--exclude` take a glob in the
  `.lww` rules syntax that matches a path or one of its folders, so
  `--exclude node_modules` leaves out every `node_modules` folder (and the
  walk does not enter it). A `history` row is `{commit, change}` with its
  `message`; `change` is `added`, `modified` or `removed`; the long row adds
  `parent`, `created_at`, the path's `type`, `size` and `mode` after the
  Commit, and which of `content`, `metadata` and `identity` changed.
  `--traversal branch` stays on the Branch's own Commits.
- **Writes** never write `main` (refused with a `fix` that forks a
  Branch). Each answers `commit`, the new Commit's full id; `unchanged`
  when nothing changed (`commit` is then the head); `merged` when the
  Branch moved during the call and the change was merged; `overwritten`
  (and its count) for the paths `.lww` rules took. The long form adds
  `outcome` (`committed`, `merged` or `unchanged`), `branch`, `parent`,
  `root`, `message` and `created_at`. `--expected C` is the head guard: the
  head the call's private Workspace bound to must be `C` (an id or prefix),
  else `Conflict` naming the head as `commit`. Without a message, the
  Commit message is `Apply filesystem changes` (`-m` in the shell).
- `mode` is octal text, four digits `0000` to `7777` (`0644`), as `stat`
  answers it. A new file is `0644` and an existing one keeps its mode; a
  folder `mkdir` makes gets its `mode`, missing parents `0755`.
- `edit` without `--replace-all` needs exactly one occurrence: none is
  `NotFound`, more than one `InvalidInput` with `matches` and a `fix` that
  adds `--replace-all`. With it, every occurrence is replaced. The long
  form says how many as `replaced` (1 for a plain edit); the short form
  only when more than one. `rm` of a folder that is not empty needs
  `--recursive` (`InvalidInput` with a `fix` that adds it).
- In the shell, `write` reads its content and `apply` its JSON from stdin,
  or from `--from <file>`.
- `apply`'s changes are objects with an `op` and that single call's fields:
  `write` (`path`, `content` as text or bytes, `mode`), `mkdir` (`path`,
  `mode`), `chmod` (`path`, `mode`), `rm` (`path`, `recursive`), `cp` and
  `mv` (`from`, `to`), and `edit` (`path`, `old_string`, `new_string`,
  `replace_all`). Any other field is refused `InvalidInput`, so a folder
  scope sees every path.
- `file.history` was `file.revision` (`at_commit`, `page`, `page_size` and
  `continuation` became `--at`, `limit` and `cursor`); `file.diff` is gone:
  one path between two Commits is `commit diff --path`. Reads lost their
  optional `[commit]` positional for `--at`, the windows `start_line`,
  `end_line`, `from_line` and `line_count` became `--line` and `--limit`,
  and `page_size`/`page_pos` became `--limit` and `--cursor`.

Also run for this manual (macOS `macos-host`, a debug build of `61960e3f5f`, a scratch
home; ids shortened to 8 characters and `…`):

```console
$ echo "# Shop API, agent" | layerfs file write shop/api agent /README.md -m "Update the title"
{"commit":"12c19c06…"}
$ echo "# x" | layerfs file write shop/api agent /README.md --detail
{"commit":"123bef1a…","unchanged":false,"merged":false,"overwritten":[],"overwritten_count":null,"outcome":"committed","branch":"agent","parent":"12c19c06…","root":"74647323…","message":"Apply filesystem changes","created_at":"2026-11-18T22:04:15Z"}
$ layerfs file read shop/api agent /README.md
{"text":"# x\n"}
$ layerfs file read shop/api agent /docs/a.txt --line 2 --limit 1 --detail
{"text":"two\n","next_line":null,"line_truncated":false,"binary":false,"size":8,"commit":"123bef1a…","root":"74647323…","path":"/docs/a.txt","start_line":2,"end_line":2}
$ layerfs file stat shop/api agent /docs/a.txt
{"type":"file","size":8,"mode":"0644"}
$ layerfs file list shop/api agent / --text
PATH        TYPE
/README.md  file
/docs       dir
$ layerfs file glob shop/api agent '**/*.txt'
{"items":["/docs/a.txt"]}
$ layerfs file grep shop/api agent two
{"items":[{"path":"/docs/a.txt","line":2,"text":"two"}]}
$ layerfs file edit shop/api agent /docs/a.txt --old-string two --new-string three -m "edit a"
{"commit":"125470c8…"}
$ layerfs file edit shop/api agent /docs/a.txt --old-string nope --new-string three
{"error":"NotFound","message":"the old text is not in /docs/a.txt"}
$ layerfs file mkdir shop/api agent /src/lib
{"commit":"128ee09b…"}
$ layerfs file rm shop/api agent /src --recursive
{"commit":"1244862d…"}
$ echo '[{"op":"write","path":"/x.txt","content":"x\n"},{"op":"mkdir","path":"/y"}]' | layerfs file apply shop/api agent
{"commit":"12d12fbd…"}
$ layerfs file history shop/api agent /docs/a.txt
{"items":[{"commit":"125470c8…","change":"modified","message":"edit a"},{"commit":"32919f55…","change":"added"}]}
$ layerfs file blame shop/api agent /docs/a.txt
{"ranges":[{"start":1,"end":1,"commit":"32919f55…"},{"start":2,"end":2,"commit":"125470c8…"}],"commits":[{"commit":"32919f55…"},{"commit":"125470c8…","message":"edit a"}]}
```

### `sandbox` (7)

Each call answers its short form, `--detail` its long form (see
[the form of an answer](#the-form-of-an-answer)). `sandbox.doctor` is one of [the doctors](#the-doctors). Removed with no
alias: `sandbox.close` (now `remove`) and `logs.sandbox` (now `log`); the
`logs` family is gone (`logs.machine` is `machine log`). Every call but
`create` names the Sandbox by the positional `sandbox`.

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `create` | `project`, `name` | `machine`, `image`, `mount`, `on_progress` | starts a named Sandbox of a project on a machine; with `--mount B`, also mounts Branch `B` in it |
| `list` | — | `project`, `machine`, `limit` (100), `cursor` | every Sandbox in the home, or one project's or one machine's, from records |
| `get` | `sandbox` | `--with stats` | one Sandbox: its state and its Workspaces, a live read of its daemon |
| `remove` | `sandbox` | `force`, `on_progress` | stops a Sandbox and removes it |
| `files` | `sandbox`, `[path]` | `depth` (1), `limit` (100), `cursor`, `byte_offset` (0), `byte_length` (262144) | a Sandbox's roots, one page of a folder, or one byte window of a file |
| `log` | `sandbox` | `limit` (200), `cursor` | a Sandbox's daemon log: its last lines, then the lines before a cursor |
| `doctor` | `sandbox` | `on_progress` | checks one Sandbox and its Workspaces (see [the doctors](#the-doctors)) |

- **State from records.** A Sandbox's `state` is derived from its record
  and its machine's record, with no probe, in this order: `busy` with `op
  { name, phase, started_at }` while a `create` or `remove` runs;
  `broken` `unresolved` once one lost its answer and its process ended (or
  a machine stop's outcome was not known); `broken` `missing` with no
  runtime id, or a runtime a confirmed machine stop ended; `blocked` with
  `blocked_by { machine, reason }` while its machine is not ready;
  `broken` `unsupported` for the retired docker backend; `broken` `lost` or
  `not_answering` when a `get` recorded it; else `ready`. Every broken
  Sandbox's `fix` is `layerfs sandbox remove <sandbox>`; the long form's
  `message` says why. `list` reads records only and asks no machine or
  daemon; `list` and `get` start no machine (#535 P4).
- **`get`** starts from the records and, for a Sandbox they say is ready
  (or `lost`, or `not_answering`), asks its daemon for its Workspaces once,
  on the route its other calls take. A list is `ready`; a machine that says
  the Sandbox does not run is `broken` `lost`; a daemon that admits no
  control connection is `broken` `not_answering`, its `message` carrying
  the refusal's counts (`(Capacity on daemon_control_slot: 0 pool and 1
  reserved slots vacant, 3 failures retained)`); a machine or forward that
  does not answer is `unknown`, with `observed_at` (when its state was last
  known) and why in `message`. The record is written only when that
  changes its `reason` (which moves the home's `generation`); when it was
  observed is kept in `observed.json` beside the record, which does not.
  Reading never clears an `op`.
- **`get`'s Workspaces**: each by its handle `workspace`
  (`<sandbox>:<name>` when named, else `<sandbox>:<path>`), `branch`, the
  exec ids `running` in it now (whichever client started them; one whose
  client is gone is not listed) and, when not ready, its `state` (`busy`
  with `op.name` `mount`, `commit` or `unmount`; `broken` with `fix:
  layerfs workspace unmount <workspace> --discard --force`). The long form adds
  `path`, `commit`, `base_layer`, `mounted_at`, `branch_moved`,
  `pending_merge`, `commands` (`[{exec, started_at, program, tag}]`: no
  argument and no environment value is shown) and `id`. The Sandbox's long
  form adds `sandbox, project, machine, platform, image, backend, route,
  created_at`, `workspace_capacity` (16, whatever its state) and `network`
  (a namespaces Sandbox's host end and its commands' address,
  `lfs<id12> <address>/31`; null for a `process` one).
  ```text
  $ layerfs sandbox get s1
  {"workspaces":[{"workspace":"s1:w","branch":"dev","running":["exec-9c0f6f0d2a1b4e77"]}]}
  ```
- **`get --with stats`** adds `stats`, null unless the Sandbox is ready:
  `daemon` (`{pid, started_at}` of the process its record names),
  `usage` (`{cpu_ms, memory_bytes, memory_peak_bytes, memory_file_bytes,
  pid_count, oom_kill_count}` of its cgroup, or null with `usage_reason`:
  `host_process`, `not_supported`, `cgroup_v1`, `no_cgroup`,
  `not_observed`, `no_controller`), `daemon_capacity` (the daemon's
  control-capacity model, one more bounded call) and `workspaces`
  (`[{workspace, overlay_bytes, usage, usage_reason}]`; `overlay_bytes` is
  the overlay file's size from its folder entry, null when it could not be
  read).
- **`list`** answers `items` by name, each with `sandbox`, `project`, the
  Workspaces this home records (`[{workspace, branch}]`) and its state
  when not ready; `workspaces_not_listed` is true for a Sandbox whose
  Workspace records live in its machine. The long form adds `message,
  machine, platform, image, created_at`. A page ends after `limit` rows
  (1 to 1000) or early at 64 KiB, with `next_cursor`, which carries the
  filters; a cursor of other filters is `InvalidInput`.
- **`create`** checks the Store's machine, a running `store move` and the
  name under one home lock, where it claims the name
  with its `op` (`create`, #536 §7.1); its success records the result and
  drops the `op` in one write, a refused start removes the record. A name
  taken anywhere in the home is `Exists`. With no `machine` it runs on its
  sqlite Store's machine, or on the one machine attached to its PostgreSQL
  Store (none or several attached: refused, [below](#store-11)); another
  machine than a sqlite Store's is `Unsupported`. Its short answer is `{"ok":true}`, or with
  `--mount B` the Workspace's `workspace` (its name is the Branch's,
  lowercased, each other character `-`) and `path`. A refused mount keeps
  the Sandbox and answers the mount's refusal with `next.sandbox` and
  `fix: layerfs workspace mount <sandbox> <B>`. Progress: with `--image`, the
  copy's `inspect, lock, export, send, unpack`; then `start` and, with
  `--mount`, `mount`. A create whose outcome is not known is
  `Unknown`; its record keeps the `op`, uncertain.
- On `docker` a Sandbox runs `namespaces` in the docker machine's worker
  container (D-8). Its root is the sealed bundle's `sandbox-image.tar`
  unless `image` names another. `image` (call-table kind `image`) is a
  `sha256:` digest, a Docker image Id or a Docker reference
  (`postgres:17-bookworm`), on every machine but macOS `macos-host`, which
  refuses it `Unsupported` `{reason: "host"}`. An image the machine holds
  (unpacked, or in its archive) starts at once; any other is resolved with
  `docker image inspect` through the `docker` machine's recorded socket
  (else Docker's default context), refused `Unsupported`
  `{architecture}` for another architecture (`{os}` for another OS), `Capacity`
  `{needed_bytes, free_bytes, path}` with `fix: layerfs machine prune <m>
  --unused` when `2 × size × 1.05`, less the bytes a resumed
  copy already staged, is over the machine's free bytes (`size`: what the
  image's layers unpack to, the sum of `docker history`'s layer sizes),
  and otherwise exported once into an exclusive per-export container (`docker
  create` + `docker export`, without the init layer's files and with the mtimes
  of the directories it stamps (the root, `dev/`, `etc/`, `proc/`, `sys/`)
  normalized to epoch 0 (`1970-01-01`) so resume across separate exports
  sees identical bytes; the container and its anonymous volumes are removed on
  completion or error; one a `SIGKILL`ed copy left is swept by the next copy
  of the same image to the same machine, and any other by the next copy of
  an image no copy holds), sent in 1 MiB frames and unpacked on the
  machine under `machine/linux-guest/run/images/<hex>.2/` with a sidecar
  `<hex>.2.json`; the staged tar is deleted. Docker unreachable is
  `Unavailable` `{what: "docker"}`, the machine stopped `Unavailable`
  `{what: "machine"}`. One copy of an image on one machine at a time: a
  second waits for the first (a `{phase: "lock"}` event once a second; the
  copy it waits for is the `busy` item of `machine images`), then reuses
  what it published. A create that copied holds the copy lock until the
  Sandbox is made, so a prune cannot remove the image in between. `--progress` reports
  `inspect`, `lock`, `export`, `send`
  (`done`, `total`; `total` is an estimate until the last `send` event,
  which has `done` equal to `total`, the bytes sent), `unpack` and `start`. The record keeps the image's id,
  and every create sets its sidecar's `last_used`. The worker container's own
  image is the one `machine update docker --image` recorded, else that
  archive, loaded into Docker once and recorded on the docker machine as
  `loaded_image` (`{id, bundle}`; another bundle's image is loaded again).
  A bundle without the archive is `Unsupported`, naming it (#535 P5).
  A Sandbox recorded under the retired docker backend is `broken`
  `unsupported`; every call on it but `get` and `remove` is refused
  `Unsupported`, asking to recreate it; `sandbox remove` removes its
  records (`outcome: "already_gone"`), and `left` says its container was
  not touched: `docker rm -f <container>` when its record keeps the name,
  else that the record does not name it.
- **`remove`** records its `op` (`remove`) before it stops the Sandbox
  and removes the record at the end; progress is its phases `cancel` (with
  `force`, one event per command this home records), `stop` and `delete`.
  Without `force` it is `Busy` while a command runs (`next.running` lists
  their exec ids) and `InUse` while a Workspace is mounted
  (`next.workspaces` lists their handles). With `force`, each running
  command is cancelled first (a cancel not confirmed refuses it `Unknown`),
  then the Sandbox is stopped anyway: `unmounted` lists the Workspaces it
  ended, whose uncommitted changes end with it. Once its runtime is
  confirmed stopped, the mount intent each of those Workspaces held in the
  Store is ended, once each; on a PostgreSQL Store whose URL its machine
  cannot dial, the machine hands those intents back in its answer and the
  caller ends them with the Store's own address. Nothing else records
  them: if that answer is lost (`Unknown`), the machine cannot remove the
  Sandbox's folder after stopping it (`Failed`), or the caller ends before
  it ends them, they stay, and a retried remove answers `already_gone`
  with an empty `left`. A PostgreSQL Store whose password this home does
  not hold refuses the remove, forced or not, with its fix. One it could not end (its Store could not be
  opened, say) is a sentence of `left` that says why: `branch remove` of
  its Branch and `store compact` are refused while it remains, and only the
  Store's admission ends it later, once it can prove the owner gone (for a
  sqlite Store on a machine, after that machine's kernel restarts; never on
  macOS `macos-host` or a PostgreSQL Store). `outcome` (long form) is
  `removed`, `already_gone` (its runtime was gone, or it never recorded
  one, or it was a Sandbox of the retired docker backend; `left` says what
  may remain). A lost answer keeps the record with the `op` uncertain
  (`broken` `unresolved`) until another `remove`; any other refusal leaves
  the Sandbox as it was.
- **`files`**, with no path, answers the Sandbox's roots, each with a
  `role` (long form): one `workspace` per mounted Workspace (`not_read:
  mount`, its handle in `workspace`), `home` (the folder that is `HOME`
  for its commands), `log` (its daemon's log, read with `sandbox log`)
  and, for a Sandbox with a root of its own (every one but a `process` macOS
  Sandbox), `/` with role `root`. With the path of a folder it answers
  `items` (`path`, `type` `file`, `dir`, `symlink` or `other`, `size` and,
  for an entry listed and not read, `not_read`: `system`, `mount`,
  `unreadable` or `restricted`; the long form adds `role, depth, mode,
  modified_at, target, fs, workspace`) in tree order to `depth` levels, at
  most `limit` (1 to 1000) a page, with `next_cursor`. With the path of a
  file it answers a window: `text` and `next_byte_offset` while more
  remains, or `binary: true` with its `size`. A path is absolute, as the
  Sandbox's commands see it; under its home no link is followed and no
  mount entered, and outside it the path is read in the Sandbox's root
  (`/proc`, `/sys`, `/dev` are `system`; a Workspace mount is never
  entered). A log is refused `InvalidInput` (it is read with `sandbox
  log`), so is a path with `..`, a link on the way, a FIFO, socket or
  device, and a file LayerFS keeps private. A path that is not there is
  `NotFound`.
  ```text
  $ layerfs sandbox files s1 "$HOME_PATH" --depth 2 --limit 2
  {"items":[{"path":"/…/tmp/a.txt","type":"file","size":6},{"path":"/…/tmp/src","type":"dir"}],"next_cursor":"/…/tmp/src"}
  ```
- **`log`** answers `items` (`[{text, continued?}]`), oldest first: the
  last `limit` lines (200 by default, 1 to 1000), or with `cursor` the
  lines before it, at most 64 KiB a call; `next_cursor` continues with
  older lines and is absent at the start of the log. Control characters in
  `text` are escaped (`\u{1b}`).
- `files` and `log` take no lock or lease, ask no daemon, start nothing
  and write nothing, the call log included; at most two inspection reads
  run on a machine at a time (`Busy`), and a machine that is stopped or
  does not answer within 8 s is `Unavailable`. They are `full` profile
  calls: the MCP server lists them in its `admin` profile.


Run for this manual (macOS `macos-host`, a debug build of `61960e3f5f`, a scratch
home; ids shortened to 8 characters and `…`):

```console
$ layerfs sandbox create shop/api tests --progress
{"phase":"start","done":0,"total":1}
{"phase":"start","done":1,"total":1}
{"ok":true}
$ layerfs sandbox list
{"items":[{"sandbox":"tests","project":"shop/api"}]}
$ layerfs sandbox get tests
{"state":"ready"}
$ layerfs sandbox get tests --with stats
{"state":"ready","workspaces":[{"workspace":"tests:agent","branch":"agent"}],"stats":{"daemon":{"pid":75696,"started_at":"2026-11-18T22:05:16Z"},"usage_reason":"host_process","daemon_capacity":{"admission":{"pool_vacant":7,"pool_running":1,"reserved_vacant":1,"reserved_running":0},"retained":{"limit":64,"ttl_ms":3600000,"count":0,"dropped":0,"recorded":0}},"workspaces":[{"workspace":"tests:agent","overlay_bytes":77824,"usage_reason":"host_process"}]}}
$ layerfs sandbox log tests --limit 2
{"items":[{"text":"LAYERFS_DAEMON_LISTEN 127.0.0.1:62804"}]}
$ layerfs sandbox doctor tests
{"ok":true}
$ layerfs sandbox remove tests
{"error":"InUse","message":"Sandbox tests still holds 1 Workspace(s); commit or unmount them first","workspaces":["tests:agent"]}
$ layerfs workspace unmount tests:agent
{"ok":true}
$ layerfs sandbox remove tests --progress
{"phase":"stop","done":1,"total":3}
{"phase":"delete","done":2,"total":3}
{"phase":"delete","done":3,"total":3}
{"ok":true}
```

The host machine's daemon was this build's (`layerfs machine update macos-host --daemon
target/debug/layerfs-daemon` on the scratch home).

### `workspace` (7)

A Workspace is `<sandbox>:<name>`, the handle `mount` answers as
`workspace`; every other `workspace` call takes it. The path form of the
handle is gone: `<sandbox>:/path` is refused `InvalidInput`.

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `list` | — | `sandbox`, `limit` (100), `cursor` | every mounted Workspace, or one Sandbox's (agent profile) |
| `mount` | `sandbox`, `branch` | `name` | mounts a Branch in a Sandbox |
| `exec` | `workspace`, `command` | `cwd`, `env` (repeat), `stdin`, `stream`, `stdout`, `stderr`, `on_progress` | runs one bash command in a Workspace |
| `cancel` | `workspace` | `exec` | ends the commands running in a Workspace, or one of them |
| `commit` | `workspace` | `message` (`-m`), `expected`, `force` | commits a Workspace's changes to its Branch |
| `unmount` | `workspace` | `discard`, `force` | ends a Workspace |
| `status` | `workspace` | `path` | a Workspace's uncommitted changes, and whether its Branch moved |

- `list` reads Workspace records and asks no daemon: the home's own for a
  Sandbox on the host machine or one reached directly, and one `workspace.list`
  request to each other ready machine's worker. A Sandbox whose machine,
  or which itself, is not ready is named in `not_listed` with
  `blocked_by`; one whose worker did not answer with `reason` (`timeout`,
  `busy`, `lost`, or `refused` with its refusal in `message`). Rows are
  `{workspace, branch}` (long: `sandbox`, `project`, `path`, `mounted_at`),
  ordered by Sandbox then name, paged by `limit` (1 to 1000, default 100)
  and `cursor`, which carries `--sandbox`.
- `mount` answers `{workspace, path}` (long: `outcome: mounted`, `branch`,
  `commit` the Commit it stands on (on a fresh fork, the Layer it was
  forked at, as `branch get` names it), `base_layer`, `root`, `id` the 64-hex
  Workspace id, `namespace`). Without `name` the name is the Branch's made
  valid: lowercased, each character outside `a-z 0-9 _ . -` replaced by
  `-`, cut to 63 and `_ . -` trimmed from the ends (`Feature/Login` mounts
  as `feature-login`). A name in use is `Exists`; a Branch whose default is
  empty or reserved, and `main`, are `InvalidInput`; each with a `fix`. A
  seventeenth Workspace is `Capacity`.
- `exec` answers `{exit_code}` and `stdout`, `stderr` when not empty; long:
  `stdout_file`, `stderr_file`, `cancelled`, `exec`, `stop` (`cgroup` on
  Linux kernels, `group` on macOS `macos-host`), `stdout_bytes`,
  `stderr_bytes`, `duration_ms`. A stream of at most 1 MiB is answered
  whole; a longer one answers its last 1 MiB and names the file that holds
  all of it, `<run folder>/exec/<workspace id>/<exec id>.stdout`, which
  `sandbox files` reads and `unmount` deletes. The shell exits with the
  command's own code in every form and prints the result JSON on stdout; a
  refusal exits 125. `--cwd` is a folder of the Workspace as a path from
  its root; `--env K=V` repeats (Node and Python: one object); `--stdin`
  gives the command this process's stdin, read whole (Node and Python:
  text or bytes); without it the command reads `/dev/null`. `--stream`
  writes the output to this process's stdout and stderr and prints no
  result. There is no timeout.
- LayerFS makes every exec id (`exec-` and 16 hex digits); `--progress`
  sends `{"phase":"start","done":0,"total":1,"exec":"exec-…"}` before the
  command starts. A lost reply is the refusal `Unknown` with `exec` and a
  `fix` naming `workspace cancel --exec`; the command is never run again.
- `cancel` ends every command running in the Workspace, or the one
  `--exec` names: SIGTERM, then SIGKILL after 5 s. Short answers:
  `{"signal":"SIGTERM"}`, `{"already_ended":true}`, `{"unknown":true}`
  (the worker was not reached or did not answer: cancel again once it
  answers). Long: `outcome` (the worst of `commands[].outcome`: unknown,
  cancelled, already_ended) and `commands: [{exec, outcome, signal}]`. A
  cancelled command's `exec` answers `cancelled: true` and exit code 128
  plus the signal.
- `commit` answers `{commit}` (the Branch's new head) with `unchanged`,
  `merged`, `overwritten` (paths a newer-write-wins rule decided, the
  first 20) and `overwritten_count` past those; long: `outcome`, `branch`,
  `parent` (the Commit before it, or for a Branch's first Commit the
  Layer it was forked at; null when unchanged), `root`, `message`,
  `created_at`, `merged_from`, `displaced`.
  `--expected C` refuses `Conflict` unless the Branch's head is `C`, a
  Commit id or a prefix of 8 or more of its characters resolved as
  `commit get` resolves it (a prefix several Commits start with is
  `InvalidInput` with `matches`); the check is one read before the Commit. A conflict is `Conflict` with
  `commit` (the head), `conflicts` (first 20 paths) and `conflicts_count`.
- `unmount` answers `{"ok":true}`; long: `outcome` (`unmounted` or
  `force_unmounted`) and a forced unmount's `abort`, `detach`, `cleanup`.
  `Dirty` carries `changes` (the first 20 entries, as `status`'s short
  form lists them), `changes_count` past the 20 shown and `pending_merge`,
  with the fix `layerfs workspace commit <workspace>`; `Busy` and an unconfirmed forced cancel (`Unknown`)
  carry `running` (the exec ids).
- `status` answers `changes`: the first 20 entries of one bounded read of
  256 overlay rows, each `{path, change}` (`added`, `modified`,
  `removed`) with `type: "dir"` for a folder; `changes_count` past the 20
  shown, `partial: true` at the bound, `branch_moved: true` when the
  Branch gained Commits since the mount, and `state` with its companion
  when not ready (from the daemon's `activity`); a busy Workspace's `op`
  is `{name, phase, started_at}`: `mount` in `attach`, `commit` in `flush`,
  `publish` (capture, construction, Save, publication and install) or
  `invalidate`, `unmount` in `close`, and when the daemon admitted it. `--path` filters within
  the same read. A machine that is not ready refuses it `Unavailable` with
  `fix: layerfs machine start <machine>` (any call routed to a machine whose
  worker is not running carries that fix; a worker of another build carries
  `layerfs machine stop <m> && layerfs machine start <m>`); it starts no
  machine.
- On `lima`, `mount`, `commit`, `unmount` and `status` go from the caller
  straight to the Sandbox's daemon (#535 G4); `exec` and `cancel` go
  through the machine's worker. On `docker` every call goes through its
  worker (D-8).

Run for this manual (macOS `macos-host`, a debug build of `61960e3f5f`, a scratch
home; ids shortened to 8 characters and `…`):

```console
$ layerfs workspace mount tests agent
{"workspace":"tests:agent","path":"/tmp/demo/home/sandbox/tests/workspaces/agent"}
$ layerfs workspace list
{"items":[{"workspace":"tests:agent","branch":"agent"}]}
$ layerfs workspace exec tests:agent 'echo hello > out.txt && uname -s'
{"exit_code":0,"stdout":"Darwin\n"}
$ layerfs workspace exec tests:agent 'exit 3'
{"exit_code":3}
$ echo $?
3
$ layerfs workspace status tests:agent
{"changes":[{"path":"/out.txt","change":"added"},{"path":"/x.txt","change":"removed"}]}
$ layerfs workspace commit tests:agent -m "test output"
{"commit":"12bb5eae…"}
$ layerfs workspace cancel tests:agent
{"already_ended":true}
```

(`/x.txt` is listed `removed` because `project hide-path` hid it after the
Branch's head was written; see [limitations](limitations.md#observed-in-the-runs-for-this-manual).)

### `machine` (14)

Each call answers its short form, `--detail` its long form (see
[the form of an answer](#the-form-of-an-answer)). `machine.doctor` is one of [the doctors](#the-doctors). Removed with no alias:
`machine.status` (now `get`, with `--with live` for the probe),
`machine.space` (now `stats --with space`), `machine.load` (now
`load_image`), `logs.machine` (now `log`) and `machine.platforms` (its rows
are `home get --with platforms`). Every call names the machine by the
positional `machine`, except `create`, which takes `name` and `platform`.

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `list` | — | `--with live` | every machine, from the home's records |
| `get` | `machine` | `--with live` | a machine's record and state |
| `images` | `machine` | `--with docker` | the Sandbox images a machine holds |
| `load_image` | `machine`, `image` | `on_progress` | copies an image from the host's Docker to a machine without creating a Sandbox |
| `prune` | `machine`, `[image]` | `unused` | removes a Sandbox image from a machine, or every unused one |
| `stats` | `machine` | `--with space` | a machine's processors, memory and data disk, its Stores' sizes and live Sandboxes |
| `files` | `machine`, `[path]` | `depth` (1), `limit` (100), `cursor`, `byte_offset` (0), `byte_length` (262144) | a machine's LayerFS folders by role, or a folder or file of the machine |
| `log` | `machine` | `limit` (200), `cursor` | a machine's worker log: its last lines, then the lines before a cursor |
| `start` | `machine` | `on_progress` | starts a stopped machine and its worker; a running one answers at once |
| `stop` | `machine` | `force`, `on_progress` | stops a machine's worker and VM; its disks stay |
| `create` | `name`, `platform` | `cpus` (2), `memory` (4G), `disk` (20G), `image`, `os_image`, `on_progress` | creates a VM machine (`lima`, `apple-container`) and starts it |
| `remove` | `machine` | `on_progress` | deletes a VM machine and its disk |
| `update` | `machine` | `cpus`, `memory`, `image`, `docker`, `wsl`, `daemon` | sets a machine's image, or a built-in machine's Docker socket, WSL distribution and daemon; no `disk` |
| `doctor` | `machine` | `on_progress` | checks a machine, then each Store and Sandbox on it ([the doctors](#the-doctors)) |

- **State from records.** `list` and `get` read the home's records and
  probe nothing. A machine's `state` is `ready`, `busy` (with `op { name,
  phase, started_at }`: a `create`, `start`, `stop` or `remove` running),
  `stopped` or `broken` (with `reason` and `fix`); `unknown` is never
  recorded. Reasons: `engine_down` (Docker is not running; `fix` starts it:
  `open -a Docker` on macOS, `sudo systemctl start docker` on Linux),
  `not_answering`, `outdated`, `missing`, `failed`, `unresolved` and
  `unsupported`. A records-only read answers the last state word and
  reason a start or a live read recorded, and `observed_at` from the last
  live read. A record with no `vm_name` (an earlier build's, `outdated`)
  that still holds Sandboxes, sqlite Stores or old Firecracker records has
  as its `fix` the chain its remove's `InUse` refusal carries, built from
  the records already read: `layerfs sandbox remove <s> && … && layerfs
  store remove <st> && layerfs machine remove <m>`, shell-quoted, in that
  order (each step takes records alone); with nothing on it, `layerfs
  machine remove <m>`. `home get --with tree` and `machine doctor` show the
  same line.
- **`--with live`** probes the provider and says hello to the worker once
  (`list`: every machine at the same time), within 10 s, starting nothing.
  The state, its companion, `message` and `observed_at` then come from the
  probe; `unknown` means the probe did not answer, and its `observed_at` is
  the last probe that did. A machine whose record alone makes it broken (a
  removed platform's, `unsupported`; one with no `vm_name`, `outdated`) is
  not probed: it keeps its record's state, `reason` and `fix`. The record is written only when the state word
  or reason changed (which moves the home's `generation`); `observed_at` is
  kept in `observed.json` beside the record, which does not. `get --with
  live` adds the `live` section: `disk_free_bytes`, `tool_version` (the
  provider tool's first line of `--version`) and `worker` (`{protocol,
  bundle}` as it answered), each null when not read.
- **Item and long keys.** A `list` row and the `get` answer have `machine,
  platform, state, op, reason, fix, observed_at, backend, supports,
  loopback, cpus, memory_bytes, disk_bytes, image, os_image, docker, wsl, daemon, message,
  stores, stores_count, sandboxes, sandboxes_count, created_at` (each list
  the first 20 names, its count only past those; `get` puts
  `platform` and `state` first). `supports` is a list of words: `sqlite`,
  `postgresql_sandbox`, and `log` when the machine has a worker whose log
  can be read. `loopback` is `shared` or `apart`: whether the machine
  shares this computer's loopback, as its platform declares it (null for a
  removed platform); clients read it and never test platform names.
- **`start`, `stop`, `create`, `remove`** record their `op` in the machine's
  record before their first effect, move its `phase` as they go and report
  `{"progress":{"phase","done","total"}}` events: `start` `bundle`,
  `image` (the docker machine loading the bundle's image), `boot`, `worker`;
  `stop` `check`, `stop`; `create` `bundle`, `boot`, `worker`; `remove`
  `check`, `delete`. Another call on the machine meanwhile is `Busy`, naming
  the op. A `start` of a running machine or a `stop` of a stopped one
  answers `{"unchanged":true}`. A start or stop whose provider reply was lost
  leaves the machine `broken`/`unresolved` with `fix: layerfs machine get
  <m> --with live`, which settles it by observing; an uncertain create or
  remove is cleared by `machine remove`.
- **A bundle of another build.** `create`, `start` and every call that
  starts a worker refuse `Unavailable`, before anything starts, a Linux
  bundle whose `bundle.json` `source.head` is not the commit this build was
  compiled from (a build that names no commit cannot tell and uses it): a
  worker started from it would not answer this build's calls. Its `fix`
  reaches this build's own bundle when another place the CLI looks holds
  one: `unset LAYERFS_BUNDLE` when `LAYERFS_BUNDLE` named the refused
  bundle and the runtime's (else the cache's) is this build's; `export
  LAYERFS_BUNDLE=<folder>` when only a later place holds it (the runtime's
  bundle refused, the cache's this build's). With none there is no `fix`,
  and the message says so: rebuild it.
- **An outdated worker.** `start` refuses a worker of another build and
  leaves the machine `broken`/`outdated`, with `fix: layerfs machine stop
  <m> && layerfs machine start <m>`, or `layerfs machine stop <m> --force &&
  layerfs machine start <m>` when Sandboxes are recorded on it (`message`
  names how many).
- **`stop`** is refused `InUse` while Sandboxes are recorded on the machine
  (`detail.related` lists them); `--force` stops them first and answers
  `stopped_sandboxes`, `unknown_workspaces` and their counts. It runs under
  the machine's `start.lock` and removes `worker.json`: a later call that
  needs the machine is `Unavailable` until `machine start`, the only call
  that starts one.
- **`create`** answers `{"ok":true}`; long `ok, outcome, machine, platform,
  backend, cpus, memory_bytes, disk_bytes, image, os_image, created_at`.
  **`update`** answers `{"ok":true}`; long `ok, outcome, machine, platform,
  cpus, memory_bytes, disk_bytes, image, docker, wsl, daemon`; it is `InUse`
  while Sandboxes run on the machine. **`remove`** also forgets the
  machine's own PostgreSQL addresses in Store records and deletes the
  home's image copy locks of it (`run/image-lock/<machine>/`); a machine of a
  removed platform (`firecracker`, `ssh`, `microsandbox`), shown as
  `broken`/`unsupported`, has only its record removed: `{"ok":true}`, the
  long form's `message` saying nothing on its host was touched.
- **`images`** rows are `{image}` with `refs, size, default, state, op,
  reason, fix` when they apply, long adding `message, staged_bytes,
  architecture, source, sandboxes, sandboxes_count, last_used_at`
  (`sandboxes` are names); the long answer adds `machine, mode,
  architecture, disk_free_bytes`. One item per image, with a state (v19): a
  published image is `ready`; a part a live copy holds is `busy` with `op
  {name: load_image, phase, started_at}` (the phase the copy recorded in its
  lock's owner record, `lock` before it named one); a part no copy holds,
  or an unpack that stopped before it was recorded, is `broken`/`partial`
  with `fix: layerfs machine load-image <m> sha256:<hex>`, which resumes
  or rebuilds it, and `size` null. `staged_bytes` is how much of a part is
  staged; a stale part beside a published image is in that image's row,
  which stays `ready`. macOS `macos-host` answers `mode: "host"` with
  none. `--with docker` reads the host's Docker: `images` it could copy
  (an image the machine holds `ready` is not offered),
  each `{image, refs, size, os, architecture, usable, needed_bytes, fits,
  unusable}`, with `reason` and `message` when Docker did not answer. It
  never starts a machine. **`load_image`** answers `{image}` with `reused`
  and `dropped` (the FIFO and device paths the copy left out, first 20)
  when they apply, long `outcome` (`loaded` or `reused`), `machine, refs,
  size, dropped_count` (only when the list was cut) and `expanded_bytes`;
  its refusals are `sandbox create --image`'s. **`prune`**
  answers `freed_bytes` and `skipped` (`[{image, message}]`, what
  `--unused` could not remove, first 20), long `outcome, machine, removed`
  (the first 20 images), `removed_count` and `skipped_count` (only when
  cut). `--unused` also removes what copies left: parts no copy holds (even
  beside an image a Sandbox uses), unpacks that were not recorded,
  sidecars with no image and `.tmp-*` folders. A named image a live copy
  holds is `Busy` with that copy's `op`; one a Sandbox uses is `InUse`
  `{sandbox_count}`; a copy lock that cannot be checked is `Failed`.
  **`doctor`**'s `machine.capacity` evidence adds `reclaimable_image_bytes`
  (what `prune --unused` frees); that prune is its fix only when there are
  such bytes.
- **`stats`** short: `cpus, memory_used_bytes, memory_total_bytes,
  disk_free_bytes, disk_total_bytes`, and `incomplete` when the 4 s budget
  ended first. Long adds `disk_used_bytes, reasons, machine, platform,
  observed_at, scope, cpu_busy_ms, cpu_total_ms, worker, data_disk, stores`
  (`[{store, size, allocated_bytes}]`) and `sandboxes`. `--with space` walks
  LayerFS's folders where the machine's data is: `space { observed_at,
  layerfs_bytes, outside_bytes, parts: [{part, bytes}], stores, sandboxes,
  unreadable_count, incomplete, reasons }`. It starts nothing and takes no
  lock.
- **`files`** answers a page of `items` (`{path, type, size, not_read}`;
  long adds `role, depth, mode` as octal text, `modified_at, target, fs,
  workspace, disk_free_bytes`) with `next_cursor`, or for a file one byte
  window: `text` (null when binary), `binary`, `size` (binary only) and
  `next_byte_offset`. `not_read` says why an entry is listed but not read
  (`restricted`, `unreadable`, `system`, `mount`). The roots, the private
  files and what is entered are those of the
  [architecture guide](../../architecture/api/sdk/machine.md#machinefiles).
- **`log`** answers `items: [{text, continued?}]`, newest page first, and
  `next_cursor` for the lines before it (left out at the start of the log).
  A cursor from before the worker's log rotated is stale (`InvalidInput`).
  A machine whose calls run in this process (`macos-host` on macOS and `linux-host` on Linux)
  keeps no machine worker log: `Unsupported`. Its host worker, which serves
  the CLI's Store calls, writes the home's `log/worker.log`.
- `stats`, `files` and `log` write no call-log line (`quiet`); like every
  `machine` call they are `full` profile calls, in the MCP `admin` profile.
  None of the reads starts a machine.

Run on a scratch home (macOS, a debug build; paths shortened):

```console
$ layerfs machine list
{"items":[{"machine":"docker","platform":"docker","state":"stopped"},{"machine":"macos-host","platform":"host"}]}
$ layerfs machine get docker --with live
{"platform":"docker","state":"stopped","observed_at":"2026-11-18T15:34:11Z","live":{"tool_version":"29.5.2"}}
$ layerfs machine get macos-host --detail --with live
{"platform":"host","state":"ready","op":null,"reason":null,"fix":null,"observed_at":null,"machine":"macos-host","backend":"process","supports":["sqlite","postgresql_sandbox"],"loopback":"shared","cpus":null,"memory_bytes":null,"disk_bytes":null,"image":null,"os_image":null,"docker":null,"wsl":null,"daemon":null,"message":null,"stores":["shop"],"stores_count":null,"sandboxes":[],"sandboxes_count":null,"created_at":"2026-11-18T15:34:11Z","live":{"disk_free_bytes":null,"tool_version":null,"worker":null}}
$ layerfs machine start macos-host
{"unchanged":true}
$ layerfs machine stats macos-host
{"cpus":14,"memory_used_bytes":25457885184,"memory_total_bytes":38654705664,"disk_free_bytes":213131337728,"disk_total_bytes":994662584320}
$ layerfs machine files macos-host /tmp/home1/store/shop --depth 1
{"items":[{"path":"/tmp/home1/store/shop/lock","type":"file","size":0,"not_read":"restricted"},{"path":"/tmp/home1/store/shop/record.json","type":"file","size":291},{"path":"/tmp/home1/store/shop/store.sqlite","type":"file","size":270336,"not_read":"restricted"}]}
$ layerfs machine get old
{"platform":"ssh","state":"broken","reason":"unsupported","fix":"layerfs machine remove old"}
$ layerfs machine remove old --detail
{"ok":true,"outcome":"removed","machine":"old","message":"only its record was removed (platform `ssh` was removed in LayerFS 2.0); nothing on its host was touched"}
```

See [machines](machines-and-stores.md#machines) for kinds, defaults and
refusals, and
[removed platforms](machines-and-stores.md#removed-platforms-firecracker-microsandbox-and-ssh).

### `store` (13)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `list` | — | `machine`, `--with stats,format` | every Store, from the home's records |
| `get` | `store` | `--with live,stats,format` | a Store's record and state; with `--with`, read where it lives, opened for its counters, or its format read without opening it |
| `create` | `name` | `machine` | an empty sqlite Store, on this computer or made on `machine` directly |
| `connect` | `store`, `url` | `user`, `password` (`--password-stdin`; both required on a first connect), `synchronous_commit` (`local`) | connects a PostgreSQL database as a Store with its login; on a connected Store, changes its address, login or write setting |
| `disconnect` | `store` | — | forgets a PostgreSQL Store with its saved login and attachments, its data staying |
| `attach` | `store` | `machine` (required) | lets a machine run Sandboxes of a PostgreSQL Store, checked from that machine first |
| `detach` | `store` | `machine` (required) | stops a machine running Sandboxes of a PostgreSQL Store; refused while one runs there |
| `move` | `store`, `machine` | `on_progress` | moves a sqlite Store between this computer and a machine |
| `remove` | `store` | — | deletes a sqlite Store with every project in it |
| `compact` | `store` | `on_progress` | reclaims the dead bytes of one Store; a sqlite Store on another machine is compacted by that machine's worker |
| `upgrade` | `store` | `on_progress` | brings an older-format Store to the latest format, in place, in one transaction |
| `repair_mount_holders` | `project`, `branch`, `action` | `op`, `legacy`, `force`, `operator`, `limit` (100), `cursor` | the operator's explicit repair of the admissions that hold one Branch: list, preview, settle, release, settle_legacy or audit; answers `status` and `operation` per row |
| `doctor` | `store` | `machine`, `on_progress` | checks a Store, then each of its projects ([the doctors](#the-doctors)) |

`store.doctor` is one of [the doctors](#the-doctors). `store.status` and `store.stats` are gone into
`store get` (`--with live`, `--with stats`), `store.push` and `store.pull`
into `store move`, and a password in a Store's record or URL into the
login `connect` saves, with no alias. The saved logins (`save_auth`,
`auths`, `remove_auth`), `update`, `--auth`, and `--machine` on `connect`
and `disconnect` are gone (owner, 2026-11-24): this computer is the one
place a PostgreSQL Store's address and login are saved, and machines are
attached to the Store and dial with both.

- **The login.** A PostgreSQL Store's record keeps its URL exactly as
  given, its `user`, `synchronous_commit` and binding. Its password is kept
  in the home's secret folder as `secret/login/<store>.json` (mode 0600, in
  a 0700 folder), read from stdin only (`--password-stdin`) and never
  answered, logged, traced or written into a record; user and password go
  into the URL only when the Store is dialled, and the URL made is read
  back by the client first. `get` answers `password_saved` and
  `password_changed_at`, never the password. There is no migration and no
  state for a record an earlier build wrote: a record without these fields
  fails through the normal path, a call that dials it refused
  `InvalidInput` (a record naming no `user`; a password this home does not
  hold, with the fix `layerfs store connect <store> <url>
  --password-stdin`). The URL holds host, port, database and TLS settings,
  read only as the PostgreSQL client reads it, and is recorded and shown
  exactly as given: no URL is ever cleaned or rewritten. A URL that may
  hold a login is refused `InvalidInput`: one holding an `@`, one whose
  client reads a user, a password or a host holding an `@` (a login written
  percent-encoded, `postgres://alice%3Ap%40db/app`), one the client does
  not read (an
  unknown key such as `sslrootcert`, `sslkey` or `sslcert`, or an
  `sslmode` other than `disable`, `prefer` or `require`), or one whose
  percent-decoded text holds, in any case, `user`, `password`, `passfile`,
  `sslkey`, `sslcert`, `sslpassword`, `oauth_client_secret`,
  `scram_client_key` or `scram_server_key` followed by
  `=` anywhere (`postgres://h/db&password=p` and
  `postgres://h/db?application_name=me?password=p` too). One holding a `#`
  is refused too (one in a database name or a value is written `%40` or
  `%23`; no host holds an `@`). The fix
  names the address `<url>` and the user `<user>`: `layerfs store connect
  <store> <url> --user <user> --password-stdin`. Other text the client
  reads as part of a host, database or value is the client's
  (`postgres://h/db&sslmode=require` names the database
  `db&sslmode=require`). The refusal names the TLS key or `sslmode` value
  it met (`sslmode=verify-full`), or the `#` ending a value it would
  otherwise read (`sslmode=require#f`), and `sslmode=require` verifies the
  server's certificate chain and host name; a private CA is trusted where
  the Store is dialled: on Linux (OpenSSL) in the system's trusted roots
  (`SSL_CERT_FILE` reaches only calls in the process that reads it: a
  Sandbox's daemon starts with a cleared environment), on macOS in the
  login or system keychain.
- **`connect`**, first time: `--user` and the password are required and
  are checked from this process before anything is saved;
  `synchronous_commit` defaults to `local`. It creates the LayerFS schema
  in an empty database or reads the Store's id from it (refused `Exists`
  when another Store of this home has that id or name, a sqlite Store
  included). Short `{ projects }` (the first 20 names, with
  `projects_count` only past those; `{"ok":true}` for an empty database or
  a connected Store changed); long `ok, projects, projects_count, outcome
  (connected or updated), store, engine, url, user, synchronous_commit,
  machines` (`machines` the attached machines' names). On a connected
  Store `connect` changes its address, user, password or
  `synchronous_commit`: an omitted user or password keeps the saved one,
  and the result must reach the same Store (its binding) before anything
  is saved; `projects` is null. Attachments are kept. A connect that is
  saved, first or not, records its check as this computer's reach (`get`'s
  `reach`), as `store doctor` does; a refused one records nothing.
- **`disconnect`** long `ok, outcome: "disconnected", store`; it forgets
  the Store's saved password and attachments too. Refused `InUse` while a
  Sandbox or Workspace uses the Store.
- **`attach --machine M`** checks the Store from `M` with the URL and login
  this computer saved, as its Sandboxes dial them (`M` must exist and its
  platform run PostgreSQL Sandboxes here, else `NotFound` or
  `Unsupported`), and only then records `M` in the Store's `attached` list
  and its reach. Short `{ ok, outcome }` with `latency_ms`; long `ok,
  outcome (attached, or checked for an attached machine checked again),
  store, machine, latency_ms, server_version`. A sqlite Store is refused
  `InvalidInput` naming `store move`. The host machine is attached like any
  other, its platform deciding whether a loopback URL is allowed, and its
  reach is its own, apart from this computer's; `connect` attaches
  nothing. A machine that reaches the server only
  through this computer's loopback is refused with advice to connect the
  Store again with an address the machine reaches. `sandbox create` on a
  machine that is not attached is refused `InvalidInput` with `layerfs
  store attach <store> --machine <machine>` as its fix; with no
  `--machine` it uses the one attached machine (none: the same refusal
  naming `<machine>`; several: `InvalidInput` listing them in
  `next.machines`).
- **`detach --machine M`** long `ok, outcome: "detached", store, machine`;
  forgets `M`'s reach. Refused `InUse` while a Sandbox of the Store's
  projects is on `M` (counted and named, the removals as `fix`), `NotFound`
  when `M` is not attached.

- **A Store's state** comes from its records only, with no probe: `busy`
  while an operation it records (`op: { name, phase, started_at }`: a
  move, a remove, a create on a machine, a compaction, a backup or a
  restore) has a live owner; `broken` `unresolved` once a gone owner left
  that operation's outcome unknown, with `fix` naming the call that clears
  it (`store move` again for a move, `store remove` for a remove,
  `store compact` for a compaction, `store get <store> --with live` for a
  backup or a restore); `blocked` when its machine is not ready, with
  `blocked_by: { machine, reason }`; `broken` with the reason the last live
  read found (`missing`, `unreachable`, `unresolved`) or `unsupported` for
  a record kind this build does not read; else `ready`. A move or remove
  holds the Store exclusively; a compaction, backup or restore does not
  refuse reads.
- **`get`**, short: `{ engine, state }`, with `machine`, `url`, `user`,
  `machines`, `projects`, `projects_count`, `backups` (each `{ bucket }`)
  and the state's companion when they say something. Long: `engine, state,
  machine, url, user, password_saved, password_changed_at, binding, reach,
  machines, projects, projects_count, backups, op, blocked_by, reason, fix,
  observed_at, store, profile, synchronous_commit, sqlite_version,
  created_at, last_transfer`. A sqlite Store answers `machine`, `profile`
  and `sqlite_version`; a PostgreSQL Store `url` (exactly as connected, no
  login), `user`, `password_saved`, `password_changed_at`, `binding` (the
  first 8 bytes of the id in its schema as 16 hex: one database behind two
  addresses has one binding), `reach` (this computer's last one),
  `machines` and `synchronous_commit`; the other engine's keys are null.
  `machines` has one row per attached machine: short `{ machine }` with
  `reached` (`ok` or `failed`; absent when nothing was seen), long
  `machine, reached, reach`; a reach is `{ at, ok, latency_ms,
  server_version, error }`, or null. A reach is written beside the
  Store's record (its observation file, which moves no `generation`) by
  `store attach`, every `store doctor` reach and each Sandbox made on a
  machine. `projects` shows at most 20 names;
  `projects_count` counts them all, only when there are more. A `backups` row's long form is
  `bucket, file, sha256, heads_sha256, last, attempt`: `last` the last
  transfer that worked (`{ call, ended_at, size }`), `attempt` the last one
  that did not (`{ call, ended_at, error }`).
- **`get --with live`** reads the Store where it lives, starting nothing: a
  sqlite Store's file through its machine (in this process on the host machine,
  else its recorded worker), a PostgreSQL Store's server from this
  computer. The read gives `state` and `observed_at`: `blocked` first,
  with nothing asked, while a sqlite Store's machine is not ready; `broken` `missing`
  when no file is where the record says, `broken` `unreachable` when the
  server does not answer, `busy` or `broken` `unresolved` from a
  compaction's own record beside the file (a run live there is `busy` even
  when this home's record holds it unresolved after a lost reply), `unknown` (with when LayerFS last knew) when the
  machine or worker does not answer. The record is written only when what
  the read found wrong changed (which moves the home's `generation`);
  `observed_at` goes to `store/<store>/observed.json`, which does not. A
  backup or restore whose owner is gone with its outcome unknown is settled
  here: the read drops the `op` and writes what was tried to the bucket
  row's `attempt`. The `live` section: `holders` (pids of other live
  processes holding the Store), `phases` (the steps a running compaction's
  record can show, in order: on SQLite `mark, build, verify, install,
  cleanup`, the record writing `cleanup` in its final step, while `store
  compact`'s progress events are `mark, build, verify, install`; else null), `behind` (the buckets whose backup heads differ from the
  Store's; null for PostgreSQL or when the heads could not be read),
  `server_version` and `latency_ms` (PostgreSQL only).
- **`get --with stats`** opens the Store, as a Store call does (refused
  while a move or remove holds it): `size` (the file with its write-ahead
  log, or the PostgreSQL schema), `logical_bytes`, `canonical_bytes`,
  `stored_bytes`, `metadata_bytes`, `file_bytes`, `reusable_bytes` (null
  for PostgreSQL), `dedupe` and `compression` (ratios as text, six
  decimals) and `init` (the first project init's
  `{ entries, files, size, canonical_bytes, stored_bytes, metadata_bytes,
  duration_ms }`, or null).
- **`list`** answers `{ items }`, each item as `get` answers it with
  `store` first, a PostgreSQL Store's `machines` as names: short
  `{ store, engine }` plus what says something (`user` and `machines` for
  a PostgreSQL Store).
  `--machine M` keeps the sqlite Stores on `M`. `--with stats` adds each
  item's `size` from one stat on its machine, starting nothing; null for a
  PostgreSQL Store and for a machine that does not answer. `--with format`
  adds each item's `format`, as `get --with format` answers it.
- **`get --with format`** (#804) adds `format: { version, latest, status,
  fix }`: the Store's format, one integer per engine (SQLite 2, 3;
  PostgreSQL 3, 4; null when unrecognized), the format this build writes,
  and `current`, `outdated` (an upgrade path exists; `fix: layerfs store
  upgrade <store>`), `unsupported` (none; `fix: layerfs store remove
  <store>` for sqlite, `layerfs store disconnect <store>` for PostgreSQL)
  or `newer` (written by a newer build; no fix). It reads only the version,
  a sqlite Store's header on a raw read-only connection (through its
  machine's worker on another machine) or one `SELECT` on PostgreSQL, so it
  starts nothing and answers on a Store this build refuses to open, where
  `--with live` and `--with stats` fail. `layerfs --version --json` says
  the formats this build writes: `formats: {"sqlite": 3, "postgresql": 4}`.
- **`create`**, short `{"ok":true}`; long `ok, outcome: "created", store,
  engine, machine, profile, sqlite_version, created_at`. Without
  `--machine` the Store goes on the machine whose calls run in this process
  (`macos-host` on macOS and `linux-host` on Linux; `Unsupported` on a host with none).
  `--machine M` makes the Store on `M` directly through its worker (a
  running machine), recording a `create` operation while it runs.
- **`move`** always has the machine whose calls run in this process
  (`macos-host` on macOS and `linux-host` on Linux) as one end: a Store on another machine
  moves on by `store move <store> macos-host` first (refused `InvalidInput`
  naming that call); on a host with no such machine it is `Unsupported`. Its progress phases are `check,
  snapshot, send, verify, install, record, retire`. Short
  `{ moved_bytes }`; long `moved_bytes, outcome: "moved", store, from, to,
  heads_sha256`. A move whose owner is gone is cleared by the same move or
  by `store remove`.
- **`remove`** deletes a sqlite Store (refused for a PostgreSQL Store:
  `store disconnect` forgets one). Short `{"ok":true}`; long
  `ok, outcome: "removed", store`.
- **`compact`**, short `{ freed_bytes }`; long `freed_bytes, outcome
  (compacted or unchanged), store, generation_before, generation_after,
  dead_bytes, discarded_bytes, reusable_bytes, kept_waste,
  delta_base_references (at most 20), delta_base_references_count,
  peak_temp_bytes, residue_bytes, residue_artifacts, residue_reason,
  estimate, zero_reasons`. `kept_waste` is `{dead_in_retained_packs_bytes,
  delta_bases_bytes, inode_blocks_bytes}`; on SQLite `dead_bytes` includes
  the dead records inside packs the run keeps, and `unchanged` means they
  are under 10% of the Store. Phases: `mark, build, verify, install`
  (sqlite; an `unchanged` run sends `mark` only), `mark, build, swap,
  cleanup` (PostgreSQL). It records a `compact` operation in the Store's
  record while it runs; one left unresolved (its own record beside the
  Store) is adopted and settled by the next `store compact`. A sqlite Store
  on another machine is compacted by that machine's worker, which sends the
  same phases; the call starts no machine. Refusals: `Unavailable` when the
  machine is stopped, records no worker or does not answer (`fix: layerfs
  machine start <m>`), or its worker is of another build (`fix: layerfs
  machine stop <m> && layerfs machine start <m>`, `--force` on the stop when
  Sandboxes are on it); `Busy` with `op` while a machine operation holds
  the machine or a compaction runs (here or in the worker); `Unknown` when
  the reply is lost and `Unresolved` when an earlier run left its outcome
  unknown (both `fix: layerfs store compact <store>`); `InUse` while
  Workspaces are mounted; `Unavailable` with `blocked_by {machine, reason:
  outdated}` on a machine whose record has no `vm_name`, with the chain of
  removes of what is on it as its fix. A
  compaction ends no mount intent itself: it settles only an intent whose
  owner the Store's admission proves gone, and a mount whose Sandbox this
  home no longer records still holds it. While a compaction runs on a machine, a move's or
  remove's steps there are `Busy` naming it.
- **`upgrade`** (#804) brings a Store written in an earlier format in
  place, in one transaction, to the latest format this build writes,
  keeping everything it holds; nothing else converts a Store, open never
  does. Today's steps: SQLite 2 → 3 and PostgreSQL 3 → 4. One argument and
  nothing to choose; it runs where the Store's record says (a sqlite Store
  on another machine in that machine's worker). Short `{ outcome }`
  (`upgraded`, or `current` for a Store already at the latest format, which
  is not changed); long `outcome, store, engine, format_before,
  format_after, steps, warnings` (`steps` the steps applied, in order, as
  `"2->3"`; `warnings` what failed after COMMIT, which never fails the
  call; a `-shm` an earlier read-only call left beside the Store is not
  one). Phases: `check, convert, verify, commit` (SQLite), `check,
  convert` (PostgreSQL); `commit` is the point of no return. Ctrl-C
  (SIGINT) or SIGTERM cancels it before COMMIT: `Failed` with `detail.code:
  "Cancelled"`, "cancelled before COMMIT; nothing changed"; a second signal
  exits at once, `Unknown`. Refusals, each meaning nothing changed:
  `InUse` (a Workspace mounted, a lease held, a Sandbox using the Store, or
  another process with the file open), `Busy` (another Store op, the lane
  or a lock not granted at once, live old-build mount rows), `Unresolved`
  (an earlier upgrade's outcome unknown, its owner gone; `fix: layerfs store
  upgrade <store>`, which adopts and settles it), `Unavailable` (the
  machine stopped or its worker of another build, fixes as for `compact`),
  `Unsupported` (an unrecognized source, a version with no step, a pre-2.0
  Store, or one newer than this build; `fix: layerfs store remove <store>`
  or `layerfs store disconnect <store>`, none for a newer Store), `Failed`
  (a check before COMMIT, named). `Unknown` (a lost reply, a killed
  process) does not say whether the Store changed; its fix `layerfs store
  upgrade <store>` then answers `current` or does the work.

Run (macOS, `macos-host`; rerun at `61960e3f5f` on a fresh scratch home, all but
the `--detail` line, which is from an earlier run of the same calls):

```console
$ layerfs store create shop
{"ok":true}
$ layerfs store list
{"items":[{"store":"shop","engine":"sqlite","machine":"macos-host"}]}
$ layerfs store get shop
{"engine":"sqlite","state":"ready","machine":"macos-host"}
$ layerfs store get shop --with live --detail
{"engine":"sqlite","state":"ready","machine":"macos-host","url":null,"user":null,"password_saved":null,"password_changed_at":null,"binding":null,"reach":null,"machines":null,"projects":[],"projects_count":null,"backups":[],"op":null,"blocked_by":null,"reason":null,"fix":null,"observed_at":"2026-11-18T15:35:10Z","store":"shop","profile":"normal","synchronous_commit":null,"sqlite_version":"3.53.2","created_at":"2026-11-18T15:35:10Z","last_transfer":null,"live":{"holders":[],"phases":null,"behind":[],"server_version":null,"latency_ms":null}}
$ layerfs store list --with stats
{"items":[{"store":"shop","engine":"sqlite","machine":"macos-host","size":270336}]}
$ layerfs store compact shop
{"freed_bytes":0}
$ layerfs store move shop macos-host
{"error":"InvalidInput","message":"store move, check: Store shop is already on macos-host"}
```

PostgreSQL, against a local `postgres:17` (macOS, `macos-host`; this build's
outputs, run 2026-11-24 on a scratch home after the machine native correction):

```console
$ layerfs store connect shop-pg 'postgresql://postgres:pw@127.0.0.1:55437/shop'
{"error":"InvalidInput","message":"the URL names a user or holds a password (or a password file); give the address alone and the login with --user and --password-stdin; an `@` or `#` that is part of the address is written %40 or %23","fix":"layerfs store connect shop-pg <url> --user <user> --password-stdin"}
$ printf '%s' "$PG_PASSWORD" | layerfs store connect shop-pg 'postgresql://127.0.0.1:55437/shop?sslmode=disable' --user postgres --password-stdin --detail
{"ok":true,"projects":[],"projects_count":null,"outcome":"connected","store":"shop-pg","engine":"postgresql","url":"postgresql://127.0.0.1:55437/shop?sslmode=disable","user":"postgres","synchronous_commit":"local","machines":[]}
$ layerfs store list
{"items":[{"store":"shop-pg","engine":"postgresql","user":"postgres"}]}
$ layerfs store get shop-pg
{"engine":"postgresql","state":"ready","url":"postgresql://127.0.0.1:55437/shop?sslmode=disable","user":"postgres"}
$ layerfs store connect shop-pg 'postgresql://127.0.0.1:55437/shop?sslmode=disable' --synchronous-commit off --detail
{"ok":true,"projects":null,"projects_count":null,"outcome":"updated","store":"shop-pg","engine":"postgresql","url":"postgresql://127.0.0.1:55437/shop?sslmode=disable","user":"postgres","synchronous_commit":"off","machines":[]}
$ layerfs store doctor shop-pg --machine macos-host
{"error":"InvalidInput","message":"machine macos-host is not attached to PostgreSQL Store shop-pg","fix":"layerfs store attach shop-pg --machine macos-host"}
$ layerfs store attach shop-pg --machine macos-host
{"outcome":"attached","latency_ms":1}
$ layerfs store attach shop-pg --machine macos-host --detail
{"ok":true,"outcome":"checked","store":"shop-pg","machine":"macos-host","latency_ms":1,"server_version":"17.11 (Debian 17.11-1.pgdg13+2)"}
$ layerfs store doctor shop-pg
{"ok":true}
$ layerfs store get shop-pg
{"engine":"postgresql","state":"ready","url":"postgresql://127.0.0.1:55437/shop?sslmode=disable","user":"postgres","machines":[{"machine":"macos-host","reached":"ok"}]}
$ layerfs store detach shop-pg --machine macos-host
{"ok":true}
$ layerfs store disconnect shop-pg --detail
{"ok":true,"outcome":"disconnected","store":"shop-pg"}
```

The URL above names `127.0.0.1`, which works only where the Sandbox's
machine shares this computer's loopback: the host machine on macOS and Linux
and `docker` on a Linux Engine. On `docker` with Docker Desktop, `lima` or `apple-container`,
`store attach --machine` refuses it, naming what that machine calls this
computer: connect the Store again with a host every attached machine
reaches (`layerfs store connect shop-pg '<url>'`). That check, like the
first connect, needs no privilege beyond what a Sandbox's reads and writes
need: what only `store compact` needs (`pg_read_all_stats`, `CREATE` on
the database) is checked when a compaction starts.

Every refused reach check, from this computer or a machine, names the
connection `step` it stopped at, flat beside `error` and `message`, with the
doctor's words (`configuration`, `transport`, `tls`, `authentication`,
`database`, `schema`, `identity`); one from a machine also names the
`machine`. `store attach --dry-run` makes attach's check and records
nothing (`outcome: "tested"`). `store connect` of a new address, user or
password is checked from each attached machine before anything is saved,
refused `Conflict` (with `machines` and a detach as its fix) when one does
not reach it; `--dry-run` makes the checks and answers `{ ok, outcome:
"tested", store, url, here, machines }`.

See [Stores](machines-and-stores.md#stores).

### `backup` (6)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `connect` | `name`, `url` | `access_key`, `secret_key` (both required; shell: `--key-stdin`) | connects a bucket (r2, or an S3 endpoint) for Store backups |
| `disconnect` | `bucket` | — | forgets a bucket and its key; its files stay |
| `list` | — | `bucket` | the Store files in the connected buckets, with the Store each is linked to |
| `create` | `store`, `bucket` | `file`, `on_progress` | backs up a sqlite Store to a bucket |
| `restore` | `bucket`, `file`, `store` | `on_progress` | restores a Store file from a bucket as a sqlite Store on this computer |
| `doctor` | `bucket` | `full` | checks a bucket; `--full` also tests a write |

`backup.doctor` is one of [the doctors](#the-doctors). 1.x's `--key` reference (`env:` or `file:`),
`--region` and `--part-size` are gone with no alias; `list`'s bucket and
`create`'s file are options, and `restore` names the bucket, its file, then
the Store.

- **The key.** `--access-key` is the key's id; its secret comes on stdin
  with `--key-stdin` and no other way (Node and Python pass `secret_key`).
  Both are required: a command line without one is exit 2, and a Node,
  Python or MCP call without one is refused `InvalidInput` before anything
  runs.
  The secret is kept in the home's secret folder (`secret/bucket/<bucket>`,
  0600), never answered, and the call log records its byte count only, as
  it does for the URL. `connect` makes one listing before it records
  anything: a bucket that does not answer or refuses the key is
  `Unavailable`. Its outcome is `connected`.
- **A new key (spec v21).** `connect` under a taken name at the same URL
  (the same provider, bucket and prefix after parsing, with r2's account
  and `?jurisdiction=`) replaces the key: the one listing proves the new
  key first, and on `Unavailable` nothing changes. Then the secret and the
  record's `access_key` are replaced, and every Store's link to the bucket
  stays. The long form's `outcome` is `rotated`; the short form is
  `{"ok":true}`, as for a first connect. A taken name at another URL is
  `Exists`, with the fix `layerfs backup disconnect <name>`; a new name
  works too. A bucket record written before 2.0 has no key id: every call
  that needs its key is refused with the fix that connects it again at its
  URL.
- **`list`** answers one row per Store file in each bucket, and one per
  link whose file the bucket does not hold (`missing: true`). `behind` says
  the Store changed since its last backup (its heads read where it lives:
  in this process on this computer, through the machine's worker
  otherwise); `changed` says the bucket's file is not the one the link last
  moved; each is null with a `message` when it could not be read. A bucket
  that cannot be listed is named in `not_listed` (`reason` `refused`,
  `timeout` or `lost`) and the others are still listed. An entry whose
  bucket was not asked for want of its key, or refused its key (HTTP 401
  or 403), carries `fix`: `layerfs backup connect <name> <url>
  --access-key <key> --key-stdin`, which keeps its Stores' links.
- **`create` and `restore`** are recorded as the Store's `op` (`backup`,
  or `restore` when it replaces a Store), so every client reads the Store
  `busy`; a backup runs beside the Store's readers, a restore holds the
  Store. Progress is `{ phase, done, total }`: `check`, `snapshot`, `send`
  (from a machine), `hash`, `upload` for a backup; `check`, `download`,
  `verify`, `install` for a restore. A failure leaves the link as it was
  and writes what was tried to its `attempt` (`store get --detail` shows
  it); a lost reply to the upload, or a restore killed at its install,
  leaves the Store `broken` `unresolved` until `layerfs store get <store>
  --with live` settles it.
- **`doctor`** runs the `bucket.*` checks: `bucket.configuration`,
  `bucket.credentials` (a record with no key id or a home with no saved
  secret fails with the fix `layerfs backup connect <bucket> <url>
  --access-key <key> --key-stdin`, as `backup create` and `restore`
  refuse it), `bucket.transport`, `bucket.list`; `--full` adds
  the write test (`bucket.write_capability`, `bucket.put`, `bucket.get`,
  `bucket.delete`, and `bucket.reconcile` after a put whose reply was
  lost). Without `--full` those checks are left out of the report.

See [backups](machines-and-stores.md#backups).

## The doctors

Six table calls check one thing each and change nothing: they take no
lock, write no record, remove no stale lease and start no machine.

For how to read a report and run a fix, see [the doctors, for users](doctor.md).

```bash
layerfs home doctor                 # the home, then each machine, PostgreSQL Store and bucket, shallowly
layerfs machine doctor <machine>    # the machine, then each Store and Sandbox on it
layerfs store doctor <store> [--machine M]   # the Store, then each of its projects
layerfs project doctor <project> [--full]   # the project's record against its Store's own list; --full reads its data
layerfs sandbox doctor <sandbox>    # the Sandbox: record, runtime, daemon; then each of its Workspaces
layerfs backup doctor <bucket> [--full]               # the bucket's record and reach; --full writes and reads one object
```

Each answers the same report. The short form is `{"ok":true}` alone when
every check was made and nothing was found; otherwise `findings` and
`not_checked`, each present only with entries:

```json
{"findings":[{"store":"bad","check":"home.records","severity":"error","message":"…"}],
 "not_checked":[{"sandbox":"sb","blocked_by":{"machine":"docker","reason":"stopped"}}]}
```

- A finding names its thing by its own word (`home`, `machine`, `store`,
  `project`, `sandbox`, `workspace`, `bucket`), then `check`, `severity`
  (`error`, `warning` or `notice`) and `message`; `fix` (one command) and
  `from` (the machine a check was made from) when there is one.
- A `not_checked` entry names a thing, or one of its checks, that gave no
  verdict: `blocked_by` (the thing at the root of the block and its state,
  as in the lists) or `reason` (`timeout`, `busy`, `lost`), never both.
  `lost` is only a reply that did not arrive; a check that waits on one
  that failed is `blocked_by` that thing with `failed` (a Store whose id is
  not its record's leaves its projects' checks `blocked_by {store,
  reason: failed}`).
- The long form (`--detail`) adds `status` (the worst of `error`, `warning`,
  `not_checked`, `notice`, `ok`), `counts` (things per status),
  `observed_at` and `tree`: the doctor's own thing with `status`,
  `coverage` (`none`, `shallow` or `full`; `full` only when a check of
  the Full list was made), its `checks` (`{ check,
  outcome, from }`, outcome `pass`, `notice`, `warning`, `error` or
  `not_checked`), `blocked_by`, `reason` and one list per kind of thing on
  it, each checked shallowly. Its `ok` is false whenever anything was found
  or could not be checked.
- The doctor checks its own thing, then each thing on it. A thing whose
  parent is stopped or failed is `not_checked` with `blocked_by`, and none
  of its checks is tried; a stopped machine gets no worker checks.
  Each thing has its own time budget; a check still running when it ends is
  `not_checked` with `timeout`.
- `--progress` (`on_progress`) sends one event per finished thing:
  `{ phase, done, total, node }`, `phase` `self` then the kind of thing
  (`machines`, `stores`, `projects`, `sandboxes`, `workspaces`, `buckets`).
  The Full check of `project doctor` (and of `backup doctor`, which sends
  none today) may also send `{ phase: "data", done, total }` events, with
  no `node`, while it reads.
- A Workspace has no doctor of its own: `sandbox doctor` reads its daemon's
  list once and gives each Workspace it lists or the home records one node,
  `<sandbox>:<name>`, with `workspace.mounted` and `workspace.activity`; a
  failed or uncertain one is an error fixed by `layerfs workspace unmount
  <workspace> --discard --force`. A daemon that is not ready leaves its
  Workspaces `not_checked`, blocked by the Sandbox (or its machine's
  worker), and none is tried.
- A project's checks, in this order: `project.record` (its record and its
  Store's record read), `project.listed` (the Store's own list holds it, by
  its stack id, or by its name for a record without one; a record the Store
  does not list is a `notice` whose `fix` `layerfs project delete
  <project>` removes the entry) and, with `--full` only, `project.data`:
  everything main's head and each live Branch's head reach (what a mount
  can reach) is read and checked where the Store's calls run, in this
  process or in the machine's worker. It writes nothing to the Store; its
  visited set is a temporary database outside the Store, removed when the
  walk ends, so memory does not grow with the project. An object two
  Branches share is read once; two projects each read it. Each problem is
  one `project.data` finding naming the object and the head whose tree
  reached it (the first 64, then a count); an object that could not be read
  (a damaged pack) is one such error finding, with no `not_checked` entry
  beside it. While it reads it sends `{ phase: "data",
  done, total }` events counting trees. `store doctor` never runs
  `project.data`.
- `store doctor --machine` is for a PostgreSQL Store and a machine
  attached to it (else refused `InvalidInput` with `layerfs store attach
  <store> --machine <machine>` as its fix); a sqlite Store refuses it.
- A Store's checks, in this order: `store.record` (its record reads and
  names it), `store.operation` (an operation whose owner is gone with its
  outcome unknown is an error whose `fix` is the call that clears it; for a
  sqlite Store it also reads the compaction record beside the file through
  its machine: a run live there passes, one left unresolved is an error
  with `fix: layerfs store compact <store>`; under a machine that is not
  ready it waits with `store.connection`),
  `store.connection` (a sqlite Store's file is where its record says,
  read through its machine; a PostgreSQL Store's server answers this
  computer with the Store's user and saved password), `store.identity` (the Store's id is the
  recorded one), `store.schema` (its schema is this build's) and, for a
  sqlite Store, `store.backups` (a notice per bucket whose backup heads
  differ from the Store's, `fix` `layerfs backup create <store>
  <bucket>`). That is the whole cut: no Store data is scanned. A sqlite
  Store file whose id is not the recorded one fails `store.identity`, and
  `store.schema` is then not made; a file of the right id whose schema is
  not this build's fails `store.schema`. A PostgreSQL Store that this
  computer reached is then checked from every attached machine, or from
  `--machine` alone, each as one more `store.connection` row with `from`;
  when this computer cannot connect no machine is checked. A Store whose machine is not ready shows its
  `store.connection` as `not_checked` with `blocked_by`, and nothing after
  it is tried.
- `--text` prints a count line (`N error(s), N warning(s), N notice(s), N
  not checked`, or `ok` alone), then each finding, its `from` and its `fix`,
  then each `not_checked` entry.
- Exit 0 once the report is made, whatever it finds; 125 with `NotFound`
  when the thing is not in the home (a machine that only a Store or Sandbox
  record names, with no record of its own, is not: that record's own
  check reports the reference, an error in `home.records` as in `sandbox
  doctor`); 2 for a wrong command line. Leftovers
  of interrupted operations are in no doctor's report: LayerFS removes them
  itself (see [`home`](#home-5)).

## Results

- **stdout** carries the call's result as one line of JSON when it is not
  a terminal (a pipe, a file, a program), or with `--json`.
- **On a terminal**, or with `--text`, it prints a readable form instead:
  a file's text as it is; `done` for a result that only says it worked;
  aligned `label  value` lines (`memory_used_bytes` reads `memory used`,
  byte counts as sizes, a Commit id by its first 14 characters, which every
  lookup takes); a map as an indented block under its label; a list of
  records as a table with upper-case headers (a list other than `items`
  under its label); a list of plain values one per line. A row whose short
  form left `state: ready` out shows `ready` in its STATE column, which
  also says the row's reason (`broken: outdated`) and what blocks it
  (`blocked by machine docker (outdated)`); a row's `fix` and `message`
  are lines under the table (`fix <name>: <command>`); any other value a
  row leaves out shows `-`. A `*_not_listed` flag is a note under
  the table. The next page or line window is printed on stderr as the
  command line that reads it (`more: … --cursor <c>`, `--line <n>`).
  Three calls read their own way: `workspace exec` writes the command's
  stdout and stderr as they are; `file blame` is one row per range of
  lines with its Commit and message; `commit diff --with patch` is each
  path with its change as a letter (`A`, `M`, `D`) and its `-` and `+`
  lines. Bold headings and red and green diff lines appear only on a
  terminal without `NO_COLOR`.
- Every call answers its short form, or its long form with `--detail`
  ([the form of an answer](#the-form-of-an-answer)); its lists carry `items`
  and `next_cursor`, passed back as `cursor`. 1.x's `page`, `page_size`,
  `page_pos`, `continuation`, `has_more` and `next_page_pos` are gone, with
  no alias.
- Result keys are snake_case in every front end. Only identifier-like keys
  change; a key that holds a path or a name never does.
- Bytes are `Buffer`s in Node and `bytes` in Python.

## Errors and exit codes

| Exit | Meaning |
| --- | --- |
| 0 | the call succeeded; a doctor exits 0 once its report is made, whatever it found |
| 2 | the command line is wrong (wrong number of positionals, unknown option, a required option missing, a `--with` section the call does not take, a value that is not a whole number or not `true`/`false`, bad JSON, an unreadable `--from` file); nothing was called |
| 125 | LayerFS refused or failed the call; the refusal is on stderr, as JSON or readably, the form the result would take |

`workspace exec` exits with the command's own code in every form (0 for a
result with no exit code, 1 for a code outside 0 to 255) and prints its
result JSON on stdout (readably, and with `--stream`, the command's own
output instead); 125 still
means LayerFS refused or failed, with the refusal on stderr and nothing on
stdout. `layerfs help` and `layerfs --version` need no
home.

A wrong command line names what is wrong, then the call's usage line and
`layerfs help <family> <call>`. A refusal is one line of JSON (read on a
terminal as `error: <message>  (<code>)`, then `fix:` and the fields, one
per line): `{"error", "message"}`, then `fix` when
LayerFS knows one runnable shell line that clears it, then the fields the
next call needs, flat (a `Conflict` names the head as `commit`, a `Busy` or
`Unresolved` its `op`, a `Dirty` its paths). `--detail` asks for its long
form: `fix` is there even when null, and `detail` holds what else the
refusing code recorded. Node and Python raise `LayerFSError` with the same
fields as properties (`error`, also readable as `code`; `message`; `fix`;
`detail`, null unless the call asked `detail`; and each flat field; Python
also keeps them in `fields`). Run:

```console
$ layerfs file read shop/api
layerfs: file read is missing <branch> <path>
usage: layerfs file read <project> <branch> <path> [--at C] [--line 1] [--limit 2000]
`layerfs help file read` for more
$ echo $?
2

$ layerfs store remove nope
{"error":"NotFound","message":"no Store named nope"}
$ echo $?
125

$ layerfs file read nope/p main /a --detail
{"error":"NotFound","message":"no Store named nope","fix":null,"detail":{"code":"NoSuchStore"}}

$ layerfs workspace mount tests main
{"error":"InvalidInput","message":"main is the project's published line, not a Branch, and is not mounted: fork a Branch from it and mount that","fix":"layerfs branch fork shop/api work && layerfs workspace mount tests work"}

$ layerfs sandbox remove tests
{"error":"InUse","message":"Sandbox tests still holds 1 Workspace(s); commit or unmount them first","workspaces":["tests:agent"]}
```

`error` is one of thirteen codes:

| Code | Meaning |
| --- | --- |
| `NotFound` | no machine, bucket, Store, project, Branch, Commit, path, Sandbox or Workspace of that name; a path a project hid |
| `Exists` | the name is taken, the same database or bucket is already connected (under any name), or a bucket object was exported from another home |
| `InUse` | held: running Sandboxes, mounted Workspaces, processes with the Store open, Stores on a machine |
| `Busy` | an operation runs on the entity: a Store being moved, compacted, backed up, restored or removed, a machine being made, started, stopped or removed, a Sandbox being created or removed, a command running where a call needs none; the message names it (`op` id and phase) and its process (#536 §7.1), and the refusal carries `op` (`{name, phase, started_at}`) where the row says so; the long form's `detail` is `{entity: {kind, name}, phase, op}`. Also: another project is being added to the same Store; a Store's writable open met SQLite's busy lock before any effect (the open wrote nothing) |
| `Unresolved` | an earlier operation on the entity ended with its outcome not known; its process is gone and its `op` stays. The message names the call that shows it (`store get`, `machine get`, `sandbox get`) and the one that clears it (`store remove`, `machine remove`, `sandbox remove`; a compaction's is `store compact`, with that `fix`); `detail` as for `Busy` |
| `Conflict` | a merge or Commit conflict, or a head that moved; for `backup.restore`, the Store changed since its last backup or restore |
| `Dirty` | uncommitted changes stand in the way |
| `Capacity` | a Sandbox's Workspace limit (a seventeenth Workspace); a `store move` target without room for the Store (the long form's `detail.needed`, `detail.free`); copying an image to a machine without room for it (`needed_bytes`, `free_bytes`, `path`, with `fix: layerfs machine prune <m> --unused`); no space left |
| `InvalidInput` | the request is wrong for this name or kind (a file where a folder is needed, a reserved name, a write to `main`, a Commit prefix several Commits start with (`matches`), an edit text found more than once (`matches`), a cursor of another call or of other filters); nothing happened |
| `Unsupported` | not possible here, or not built (see [limitations](limitations.md)); a missing tool is listed in the long form's `detail.missing` |
| `Unavailable` | a server, bucket, worker or host cannot be reached, refused the login or secret, or did not answer in time |
| `Failed` | it was tried and did not happen; whatever exists is kept |
| `Unknown` | the outcome is not known; both sides are kept and logged in `log/uncertain.jsonl`, and the entity keeps an uncertain `op` (later calls on it are `Unresolved`) |

The control and engine crates raise more specific codes; the dispatcher
folds each into its public one, and the long form keeps the raised one as
`detail.code`. The home's call log stores the raised code and `home.log`
answers the public one.

| Public code | Raised codes it holds |
| --- | --- |
| `NotFound` | `NoSuchStore`, `NoSuchProject`, `NoSuchBranch`, `NoSuchCommit`, `NoSuchPath`, `NoSuchSandbox`, `NoStore`, `NoCommit`, `NotInHistory`, `NotMounted`, `Removed`, `NoMatch` |
| `Exists` | `NameTaken` |
| `InUse` | `MountsActive` |
| `Conflict` | `HeadMoved` |
| `InvalidInput` | `NotFile`, `IsDirectory`, `NotDirectory`, `NotEmpty`, `Binary`, `ReservedName`, `ReadOnly`, `NoMergeBase`, `TooLarge`, `AmbiguousMatch` |
| `Capacity` | `NoSpace` |
| `Unsupported` | `OldStore` |
| `Unavailable` | `Denied`, `Unauthorized`, `StaleSandbox`, `StaleStore`, `Timeout` |
| `Failed` | `UnknownMethod`, `Cancelled` (`store upgrade` cancelled before COMMIT), and any code this table does not name |

Every call is one attempt. After `Unknown`, read the state with `list`,
`get` or `status` before acting again. A refusal that carries
`attempted: true` beside its code (a `Failed`, or an `Unavailable` whose
daemon ended the connection before it answered) is uncertain the same way:
the request was sent and no answer settled it, and its message says so
("the request was sent; its outcome is not known").

## Shell switches

| Switch | Calls | Does |
| --- | --- | --- |
| `--text` | all | the readable form, also when stdout is not a terminal |
| `--json` | all | one line of JSON, also on a terminal; not with `--text` |
| `-h`, `--help` | all | help: after a family its calls, after a call its arguments, options, answer and an example |
| `--detail` | all | the long form of the answer, and of a refusal ([the form of an answer](#the-form-of-an-answer)) |
| `--with <a,b>` | `home get` (`platforms`, `tree`), `project list` (`stats`), `commit diff` (`patch`), `sandbox get` (`stats`), `machine list` and `machine get` (`live`), `machine images` (`docker`), `machine stats` (`space`), `store list` (`stats`, `format`), `store get` (`live`, `stats`, `format`) | adds the named sections to the answer; a name the call does not take is exit 2 |
| `--progress` | `home doctor`, `project init`, `project export`, `project doctor`, `sandbox create`, `sandbox remove`, `sandbox doctor`, `workspace exec`, `machine load-image`, `machine start`, `machine stop`, `machine create`, `machine remove`, `machine doctor`, `store move`, `store compact`, `store upgrade`, `store doctor`, `backup create`, `backup restore` | progress as one JSON line per event on stderr, the event itself: `{"phase", "done", "total", …}` (a doctor adds `node`; `workspace exec` adds `exec` to its one `start` event). On a terminal it is one status line instead: `phase: <phase>  done/total`, in byte units (with a rate) for the byte phases `send`, `snapshot`, `hash`, `upload` and `download`, plain counts for every other phase; `LAYERFS_PROGRESS_JSON` set keeps the JSON lines on a terminal. `backup doctor` sends no progress |
| `--stream` | `workspace exec` | passes the command's stdout and stderr through (output over 1 MiB too), prints no result and exits with the command's code |
| `--from <file>` | `file write`, `file apply` | reads the content or JSON from a file instead of stdin |
| `--<option>-from <file>` | a text option (`--message-from`, `--old-string-from`, `--new-string-from`, `--cursor-from`, `--path-from` and the others `layerfs help --json` lists under `switches`) | reads the option's value from a file |
| `-m <text>` | `branch merge`, the seven `file` writes, `workspace commit` | the commit message, the same as `--message`; help shows it as `[-m msg]` |
| `--scope <folder>` | all | a folder scope (see below); needs `--scope-project` |
| `--scope-project <store>/<project>` | all, with `--scope` | the one project the scope is bound to |
| `--scope-branch <name>` | all, with `--scope` | the scope's Branch namespace |
| `--` | all | every later word is a positional |

`--option=value` works for every option. A bool option alone
(`sandbox remove … --force`) is true; `--force=false` is false. An option
the table marks required (`backup connect`'s `--access-key` and
`--key-stdin`, `store attach`'s and `store detach`'s `--machine`, `file
edit`'s `--old-string` and `--new-string`) is shown without brackets in
help, and a line without it is exit 2; the table's JSON says `required:
true`, and the Node typings, Python stubs and MCP schemas mark it.

Each call's phases are in its family's section: `store move` reports
`check`, `snapshot`, `send`, `verify`, `install`, `record`, `retire`
(`done` and `total` bytes during `send`, null in the other phases);
`project init` reports `check`, `send` (only into a sqlite Store on
another machine, while the folder is copied), `install` and `record` on
every route.

## Folder scope

A folder scope (O32) binds a client to one project, one folder of it and,
optionally, one Branch namespace: `{ project, folder, branch? }`. Node:
`layerfs.client({ scope: { project, folder, branch } })`. Python:
`layerfs.client(scope={"project": ..., "folder": ..., "branch": ...})`.
Shell: `--scope <folder> --scope-project <store>/<project> [--scope-branch
<name>]`; `--scope` without `--scope-project` is exit 2.

- It is an allow list: the `file` calls but `history`, `branch` `list`,
  `get`, `fork`, `merge` and `publish`, and the `workspace` calls but
  `list`. Every other call (home, Stores, machines, backups, projects,
  Sandboxes, `branch remove`, `workspace list`) is refused `out of scope:
  <call>`, such as `out of scope: store.remove`.
- Every `file` and `branch` call must name the scope's project (`out of
  scope: project`); a `workspace` call's Sandbox must be recorded for that
  project (`out of scope: sandbox`).

- `folder` is a canonical absolute project path. Every path a `file` call
  names must be the folder or beneath it; paths stay absolute.
- With `branch`, Branch names are relative to the namespace and composed as
  `<branch>.<name>`; `branch.list` answers only that namespace's Branches,
  named relative to it. Without `branch`, every call that names a Branch is
  refused. A 66-character Commit id is left as it is.
- The `commit` calls and `file history` are refused as history. A scoped
  `branch list` reads every page of the project's Branches and pages the
  namespace's own.
- `workspace.mount` mounts only the namespace's `main`; `workspace.exec`
  starts the command in the scope's folder.
- A refusal is `InvalidInput` with `out of scope: path`, `branch`,
  `history`, `project`, `sandbox` or the call's name, before any effect.
- A scope guards calls. It is not filesystem isolation: a command can still
  `cd` anywhere in the mounted tree.

Run for this manual (the scratch home of the runs above):

```console
$ layerfs commit list shop/api agent --scope /docs --scope-project shop/api
{"error":"InvalidInput","message":"out of scope: history"}
$ layerfs file list shop/api agent /docs --scope /docs --scope-project shop/api
{"error":"InvalidInput","message":"out of scope: branch"}
$ layerfs file list shop/api w1 / --scope /docs --scope-project shop/api --scope-branch agent2
{"error":"InvalidInput","message":"out of scope: path"}
$ layerfs file list shop/api w1 /docs --scope /docs --scope-project shop/api --scope-branch agent2
{"error":"NotFound","message":"Branch agent2.w1 does not exist"}
$ layerfs branch list shop/api --scope /docs --scope-project shop/api --scope-branch agent2
{"items":[]}
$ layerfs workspace list --scope /docs --scope-project shop/api
{"error":"InvalidInput","message":"out of scope: workspace.list"}
```
