# The calls

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](README.md). The source is the one call table,
one file per call under its family in
[`api/crates/layerfs-api/src/`](../../../api/crates/layerfs-api/src/)
(`<family>/<call>.rs`), and the command in `api/crates/layerfs-cli/src/`. The shell parser, the
help, and the Node and Python functions are all built from that table, so
the three front ends take the same arguments.

There are 84 calls in twelve families. `layerfs help` lists them, and
`layerfs help <family> [<call>]` prints one family or one call.

## How a call is written

```
layerfs <family> <call> <positionals> [--options] [--text]
```

```js
await layerfs.<family>.<call>(<positionals>, { <options> })
```

```python
layerfs.<family>.<call>(<positionals>, <option>=...)
```

- **Positionals go from the outside in:** the owner (project, Store,
  Sandbox or Workspace), then the Branch, then the path or new name, then
  other inputs. An optional Commit is last (D7).
- **Options say how.** A shell option is the option's name with `-` for
  `_`: `--start-line` is `start_line`.
- An optional positional is simply left off. In Python, optional
  positionals and options may be keywords.
- A function option (`on_progress`, and `stdout`/`stderr` of
  `workspace.exec`) exists only in Node and Python. The shell has
  `--progress` and `--stream` instead.
- A call name with `_` is written with `-` in the shell:
  `layerfs workspace exec-cancel`, `layerfs home clear-log`.
- Three options are every call's: `detail` and `with` say which
  [form of the answer](#the-form-of-an-answer) is wanted, and `log_tag`
  tags the call's line in the home's call log.

### Names

| Thing | Form | Unique in |
| --- | --- | --- |
| machine | `dev` | the home |
| Store | `shop` | the home |
| bucket | `r2-main` | the home (apart from Stores) |
| project | `shop/api` | always written `<store>/<project>`; a bare `api` is `InvalidInput` |
| Branch | `main`, `agent` | its project |
| Commit | an id, or a prefix of 8 or more characters for `commit.get` | its project |
| Sandbox | `tests` | the home |
| Workspace | `tests:/…`, answered by `workspace.mount` | — |

Machine, Store, bucket, project and Sandbox names are 1 to 63 lowercase
letters, digits, `_`, `.` and `-`, starting and ending with a letter or
digit. Windows device names (`con`, `nul`, `aux`, `prn`, `com1`–`com9`,
`lpt1`–`lpt9`) are refused. `native` and `docker` name the built-in
machines and are refused (`InvalidInput`) as new machine names and as
Sandbox names (H §7.6).

### Argument kinds

| Kind | Accepts | Shell |
| --- | --- | --- |
| name | a machine, Store, bucket, Branch or Sandbox name, or a Commit id | a word |
| project | `<store>/<project>` | a word |
| text | a path, URL or other text | a word |
| whole number | an integer | a word; not a number is exit 2 |
| bool | true or false | `--flag` alone is true; `--flag=false` |
| JSON | any JSON value | JSON text; `file apply`'s changes come from stdin or `--from` |
| content | text or bytes | stdin, or `--from <file>`; text when UTF-8, else bytes |
| size | bytes as a number, or text with `K`, `M` or `G` (`512M`, `8G`) | a word |
| function | a callback (Node, Python only) | — |

## The form of an answer

The call surface is being moved, family by family, to one form of answer
(the [call surface specification](../../../core/docs/issues/call-surface-2-0/README.md);
its rows are `calls.json`, which a contract test reads). A call that has
moved is **migrated**: today `home.get`, `home.log` and `home.clear_log`.
Every other call still answers as its own section below says, and takes
`detail` without changing its answer. The refusal shape below is already
every call's.

**Short and long form.** A migrated call answers its short form: the few
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
  `{"ok":true}`; `ok` is never beside another key in the short form.
- **Sections.** `with` adds a named part of the answer that costs a read of
  its own: `home get --with platforms`, `--with tree`. A call takes only the
  sections its row lists; another name is refused `InvalidInput`, naming the
  ones it takes. A section is in both forms, and only when asked for.
  Python spells the option `with_`, because `with` is a keyword.
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
  null), `unknown` has `observed_at`.
- **Refusals.** `{"error", "message"}`, `fix` when there is one, and the
  fields the next call needs flat beside them; `detail` adds `fix` (null
  when there is none) and `detail`. See [errors](#errors-and-exit-codes).

Run for this manual on a scratch home (macOS, a debug build; the home's
path is shortened):

```console
$ layerfs home get
{"home":"/tmp/home1","state":"broken","version":"2.0.0","reason":"missing","fix":"layerfs home init"}
$ layerfs home init
{"home":"/tmp/home1"}
$ layerfs home get
{"home":"/tmp/home1","state":"ready","version":"2.0.0","generation":2}
$ layerfs store create shop
{"name":"shop","kind":"sqlite","machine":"native","profile":"disposable","created":1795008741370,"sqlite":"3.53.2","projects":[]}
$ layerfs home get --with tree
{"home":"/tmp/home1","state":"ready","version":"2.0.0","generation":4,"tree":{"machines":[{"machine":"docker","platform":"docker","state":"stopped"},{"machine":"native","platform":"native","stores":[{"store":"shop","engine":"sqlite"}]}],"stores":[],"buckets":[]}}
$ layerfs home get --with live
{"error":"InvalidInput","message":"home.get: `with` names `live`; this call takes platforms, tree"}
```

## The calls

`[x]` is an optional positional. Every family's results are snake_case.

### `home` (7)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `init` | — | — | creates this home, converts an older layout to the current one, and leaves a current home as it is |
| `get` | — | `--with platforms,tree` | the home's state and build; with `--with`, the platforms this computer supports or the tree of everything in the home |
| `log` | — | `call`, `project`, `client`, `tag`, `outcome`, `since`, `limit`, `cursor` | the calls made against this home, newest first |
| `clear_log` | — | — | clears this home's call log |
| `doctor` | — | — | checks the home, then each machine, PostgreSQL Store and bucket, shallowly ([the doctors](#the-doctors)) |
| `leftovers` | — | `machine` | what earlier runs left behind on this computer and on each running machine |
| `prune` | `leftover` | — | removes one leftover that `leftovers` listed |

- `init`, `get`, `doctor` and `leftovers` are not opened by the
  dispatcher: `init` opens or creates the home itself, and the others never
  open it. `get`, `log` and `clear_log` write no line to the call log, and
  all seven are `admin` in the MCP server.
- **`init`**, short: `{ home }`, with `unchanged: true` when the home was
  already current or `converted: true` when it was an older layout. Long:
  `home, unchanged, converted, outcome, layout, version`: `outcome` is
  `created`, `converted` or `unchanged`, and `version` is this build's.
- **`get`**, short: `{ home, state, version }`, with `generation` when the
  home is ready, and `reason` and `fix` when it is broken (`missing`: no
  home there; `outdated`: an older layout; both fixed by
  `layerfs home init`; `failed`: `layerfs.json` cannot be read, or is of a
  newer layout; the long form's `message` says which). Long:
  `home, state, version, generation, reason, fix, message, layout, mode,
  bundle, daemon, cache`. `mode` is the folder's permission bits as text
  (`"0700"`); `bundle` and `daemon` are
  `{ path, source, present, commit, dirty, matches }` for the Linux bundle
  and the native daemon this build would use (`matches` is null when either
  side cannot tell); `cache` is `{ path }`. It reads `layerfs.json`, the
  folder's mode, `bundle.json`, the native daemon's record and the daemon
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
  needs no home. (`machine.platforms` answers the same rows until the
  `machine` family is migrated.)
- **`get --with tree`** adds `tree: { machines, stores, buckets }`, read
  from the home's records only: each machine with its sqlite `stores` and
  its `sandboxes`, the PostgreSQL Stores (which live on no machine) and the
  buckets. A node's short form is what names it (machine `machine,
  platform`; Store `store, engine`; Sandbox `sandbox, project`; bucket
  `bucket, provider`), then what is news: its state only when it is not
  `ready`, a machine's `cpus` and `memory_bytes`, a Store's `projects` and
  `connections`, a Sandbox's `image`, `workspaces` (each `{ workspace,
  branch }`) and `workspaces_not_listed`. The long forms are: machine
  `machine, platform, cpus, memory_bytes, state, op, reason, fix, stores,
  sandboxes`; Store `store, engine, projects, connections, state, op,
  blocked_by, reason, fix`; Sandbox `sandbox, project, image, workspaces,
  workspaces_not_listed, state, op, blocked_by, reason, fix`; bucket
  `bucket, provider`. A PostgreSQL Store's `connections` are `{ machine,
  url }`: the home's own address first (`machine` null), then each machine
  that has its own; a sqlite Store's are null. A Workspace is in the home's
  records only on `native` and for a Sandbox reached directly: any other
  Sandbox answers `workspaces` null and `workspaces_not_listed: true`. A Store or Sandbox whose machine is stopped, busy or
  broken is `blocked`, and `blocked_by` names that machine. A machine that
  only a Store or Sandbox names, with no record of its own, is listed too
  (`broken`, `missing`, unless it is `native` or `docker`), so nothing in the
  home is left out.
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
- **`leftovers`** lists what interrupted operations left: one bounded walk
  of the home's own folders on `native` (20,000 entries or 2 s), and one
  through the worker of each running machine; `--machine M` walks one.
  Nothing is started: a machine that is stopped, held by an operation or
  does not answer is named in `not_listed` (`{ machine, message }` with
  `blocked_by`, the thing at the root and its state, or `reason`:
  `timeout`, `busy` or `refused`), and so is one whose walk stopped at its
  bound (`timeout`; what it found is listed). An item's short form is
  `{ leftover, modified_at }`, with `size` (null when the bound ended the
  walk before it was measured) and `report_only`; its long form adds
  `removable_at`, `kind`, `machine` and `path`. `leftover` is its id,
  `<machine>:<kind>:<name>` (never a path), the kind in snake_case:
  `run_folder`, `empty_root`, `init_copy`, `backup_staging`, `image_tmp`,
  `image_part`. An image's lock file and a Workspace's unmount marker are
  LayerFS's own and never listed. A home that is not current has none; a
  machine the home does not know is `NotFound`.
- **`prune <leftover>`** removes the one leftover whose id it is given,
  once, on that machine, under that kind's guard. Short: `{ freed_bytes }`,
  or `{"ok":true}` when the size was not measured; long: `ok, freed_bytes,
  outcome ("removed"), leftover, kind, machine, path`.

  | Kind | Removed when (else `InUse`, nothing removed) |
  | --- | --- |
  | `run_folder` | no Sandbox record names it, its `owner` marker is there and older than a minute, no daemon of it runs and nothing is mounted at or under it or its `.root` |
  | `empty_root` | its run folder is gone and it is not a mount point; `rmdir` only |
  | `init_copy` | a line of `log/uncertain.jsonl` names the machine and the copy, and its deadline (when the lost request went out, plus the Init's `timeoutMs`) passed more than an hour ago (`removable_at`); a copy no line names is `report_only` and refused `InvalidInput` |
  | `backup_staging` | no `backup create` of that Store is running (its `backup.lock` is taken for the removal) |
  | `image_tmp` | no unpack of that image is running (its lock is taken; the lock file stays) |
  | `image_part` | it has not been written for an hour |

  Other refusals: `NotFound` (no leftover has that id now), `Unavailable`
  (the machine is stopped) and `Unknown` (the worker's answer was lost;
  nothing is sent again).
- `home.log` and `home.clear_log` were `logs.list` and `logs.reset`, and
  `home.prune` was `doctor.fix` (the `doctor` family is gone); the old
  names are gone, with no alias. A leftover's `id`, `bytes`, `age`,
  `fixable` and `not_before` became `leftover`, `size`, `modified_at`,
  `report_only` and `removable_at`, and its kinds lost their hyphens. A row's `at`, `ms`, `lines`, `has_more`
  and `next` became `logged_at`, `duration_ms`, `items` and `next_cursor`.
  The home's layout and settings are in [the home](home.md).

```console
$ layerfs home log --limit 5
{"items":[{"call":"store.get","outcome":"refused","logged_at":"2026-11-18T13:32:21Z","error":"NotFound"},{"call":"store.create","outcome":"ok","logged_at":"2026-11-18T13:32:21Z"},{"call":"home.init","outcome":"ok","logged_at":"2026-11-18T13:32:21Z"}]}
$ layerfs home log --limit 2 --detail
{"items":[{"call":"store.get","outcome":"refused","logged_at":"2026-11-18T13:32:21Z","project":null,"error":"NotFound","duration_ms":0,"client":"cli","argv":["store","get","nope"],"commit":null,"tag":null},{"call":"store.create","outcome":"ok","logged_at":"2026-11-18T13:32:21Z","project":null,"error":null,"duration_ms":25,"client":"cli","argv":["store","create","shop"],"commit":null,"tag":null}],"next_cursor":"eyJmb3IiOiJob21lLmxvZyIsImF0IjoiYzEuMC40MzM3OTRiMy4zMGMwOGNmMTE1MjBlYTc2LjU3In0","skipped":0}
$ layerfs home clear-log --detail
{"ok":true,"outcome":"cleared","line_count":4}
```

(Four lines: the three above and the refused `file read` shown under
[errors](#errors-and-exit-codes), all from the same run.)

### `project` (7)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `init` | `project`, `folder` | `timeout` (ms, default 1 hour), `on_progress` | imports a folder on this computer as a new project into an existing Store |
| `list` | — | `store`, `limit`, `cursor`, `--with stats` | every project in the home, or one Store's |
| `get` | `project` | — | one project: its Store, state and main's head |
| `stats` | `project` | `branch` (default `main`), `at` | entry counts and size of the tree at a Branch's head or a Commit |
| `export` | `project`, `destination` | `branch` (default `main`), `at`, `on_progress` | exports a Branch's committed tree to a folder on this computer |
| `delete` | `project` | — | retires one project in its Store and removes its home entry |
| `hide_path` | `project`, `path` | — | hides one absolute path from the project's whole history; frees no bytes |

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
  is an absolute path on this computer. Into a sqlite Store on `native` or
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
  work bound (`size` is the length the counted node records, whole even
  then); long adds `partial, commit, root`.
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

### `branch` (6)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `list` | `project` | `prefix`, `exclude_prefix`, `limit`, `cursor` | a project's Branches, `main` included, in name order |
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
  refuses `Conflict` unless `branch`'s head is `C`, its detail's `commit`
  being the head it is at. `main` takes changes only by `publish`; merging
  into it without `--into` is `InvalidInput`. A conflict changes nothing and
  is `Conflict` with the conflicts in its detail. Short: `{ commit }`, with
  `unchanged`, `fast_forward`, `overwritten` (the paths one side's later
  write took) and `overwritten_count` when they are set; long:
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

### `commit` (3)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `list` | `project`, `branch` | `at`, `limit`, `cursor` | a Branch's Commits, newest first; on `main`, its publications |
| `get` | `project`, `commit` | `branch` | one Commit, by its id or a prefix of 8 or more characters |
| `diff` | `project`, `ref` | `from`, `path`, `--with patch`, `limit`, `cursor` | the paths that differ between two Commits or Branches |

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
  later write, cut at 4 KiB), `overwritten_count` and `units`.
- **`diff`** compares `from` to `ref`. Without `--from` the side compared
  from is a Branch's fork point, a Commit's parent, the base of a Branch's
  first Commit, and for `main`'s first publication the empty tree (every
  path `added`). `--path P` keeps only `P` and what is under it. A row's
  short form is `{ path, change }` (`added`, `removed`, `modified`, …);
  its long form adds `type`, `content`, `metadata` and `identity` (which of
  them differ). `--with patch` adds each changed text file's lines as
  `hunks` and `hunks_omitted`. The answer's long form adds `from`, `to`,
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

### `file` (15)

1.1.0's `fs` family, renamed (O13). Paths are absolute, starting with `/`.

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `read` | `project`, `branch`, `path`, `[commit]` | `start_line`, `end_line` | reads a text file, or a window of its lines |
| `stat` | `project`, `branch`, `path`, `[commit]` | — | a path's kind, size and mode |
| `list` | `project`, `branch`, `path`, `[commit]` | `page_size`, `page_pos` | a folder's entries |
| `glob` | `project`, `branch`, `path`, `pattern`, `[commit]` | `page_size`, `page_pos` | paths under `path` matching a glob |
| `grep` | `project`, `branch`, `path`, `pattern`, `[commit]` | `page_size`, `page_pos` | lines under `path` matching a regular expression |
| `revision` | `project`, `branch`, `path` | `at_commit`, `traversal`, `page`, `page_size`, `continuation` | the saved changes to one literal path, oldest first |
| `blame` | `project`, `branch`, `path`, `[commit]` | `from_line` | the Commit of each line |
| `diff` | `project`, `left_branch`, `left_path`, `right_branch`, `right_path` | `left_commit`, `right_commit` | the difference between two files (O29) |
| `write` | `project`, `branch`, `path`, `content` | `message`, `mode` | writes a file; one Commit |
| `edit` | `project`, `branch`, `path` | `old`, `new`, `message` | replaces one unique text in a file; one Commit |
| `rm` | `project`, `branch`, `path` | `message` | removes a path; one Commit |
| `mkdir` | `project`, `branch`, `path` | `message`, `mode` | makes a folder; one Commit |
| `mv` | `project`, `branch`, `from_path`, `to_path` | `message` | moves a path; one Commit |
| `cp` | `project`, `branch`, `from_path`, `to_path` | `message` | copies a path; one Commit |
| `apply` | `project`, `branch`, `changes` (JSON) | `message` | applies a list of changes; one Commit, all or none |

- Reads answer `kind: "ok"`, the `snapshot` they read (such as
  `agent@3a54ebd60426`), its `root` and the call's fields. `read` answers
  `text`, `start_line`, `end_line`, `truncated`, `next_line` and `size`.
  A file that is not text is refused `InvalidInput` (raised as `Binary`).
- Writes answer `{kind: "Committed", branch, head, commit}`. Without a
  message, the Commit message is `Apply filesystem changes`.
- `mode` is a whole number permission mode, 0 to 0o7777 (in the shell, a
  decimal number such as `420` for 0644).
- In the shell, `write` reads its content and `apply` its JSON from stdin,
  or from `--from <file>`. `edit` also takes `--old-from <file>` and
  `--new-from <file>`.
- `apply`'s changes are objects with an `op`: `write` (`path`, `content`
  as text or bytes, `mode`), `mkdir` (`path`, `mode`), `chmod` (`path`,
  `mode`), `rm` (`path`), `cp` and `mv` (`from_path`, `to_path`), and `edit`
  (`path`, `old`, `new`), the same names as the single calls
  (`engine/crates/layerfs-api/src/family/files.rs` maps them to the Store's).
  The Store's own names (`from`, `to`, `oldText`, `newText`, `bytes`) are
  refused `InvalidInput`, so a folder scope sees every path.

Run:

```console
$ printf '%s' '[{"op":"write","path":"/b.txt","content":"b\\n"},{"op":"mkdir","path":"/src"},{"op":"mv","from_path":"/docs/a.txt","to_path":"/src/a.txt"}]' \
    | layerfs file apply shop/api agent -m batch
{"kind":"Committed","branch":"agent","head":"1271…","commit":{…,"message":"batch",…}}
```

### `sandbox` (7)

The family is migrated: each call answers its short form, `--detail` its
long form (see [the form of an answer](#the-form-of-an-answer)).
`sandbox.doctor` is one of [the doctors](#the-doctors). Removed with no
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
| `doctor` | `sandbox` | — | checks one Sandbox (see [the doctors](#the-doctors)) |

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
  layerfs workspace unmount <ws> --discard --force`). The long form adds
  `path`, `commit`, `base_layer`, `mounted_at`, `branch_moved`,
  `pending_merge`, `commands` (`[{exec, started_at, program, tag}]`: no
  argument and no environment value is shown) and `id`. The Sandbox's long
  form adds `sandbox, project, machine, platform, image, backend, route,
  created_at` and `workspace_capacity` (16, whatever its state).
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
- **`create`** checks the Store's machine, a running `store push` or
  `store pull` and the name under one home lock, where it claims the name
  with its `op` (`create`, #536 §7.1); its success records the result and
  drops the `op` in one write, a refused start removes the record. A name
  taken anywhere in the home is `Exists`. With no `machine` it runs on its
  sqlite Store's machine, else on `native`; another machine than a sqlite
  Store's is `Unsupported`. Its short answer is `{"ok":true}`, or with
  `--mount B` the Workspace's `workspace` (its name is the Branch's,
  lowercased, each other character `-`) and `path`. A refused mount keeps
  the Sandbox and answers the mount's refusal with `next.sandbox` and
  `fix: layerfs workspace mount <sandbox> <B>`. Progress: `claim`, `start`
  and, with `--mount`, `mount`. A create whose outcome is not known is
  `Unknown`; its record keeps the `op`, uncertain.
- On `docker` a Sandbox runs `namespaces` in the docker machine's worker
  container (D-8). Its root is the sealed bundle's `sandbox-image.tar`
  unless `image` names another. `image` (call-table kind `image`) is a
  `sha256:` digest, a Docker image Id or a Docker reference
  (`postgres:17-bookworm`), on every machine but macOS `native`, which
  refuses it `Unsupported` `{reason: "host"}`. An image the machine holds
  (unpacked, or in its archive) starts at once; any other is resolved with
  `docker image inspect` through the `docker` machine's recorded socket
  (else Docker's default context), refused `Unsupported`
  `{reason: "arch", value}` for another architecture, `NoSpace`
  `{need, free}` when `2 × size × 1.05` is over the machine's free bytes
  (`size`: what the image's layers unpack to, the sum of `docker history`'s
  layer sizes, or the export container's root if larger),
  and otherwise exported once (`docker create` + `docker export`, without
  the init layer's files), sent in 1 MiB frames and unpacked on the
  machine under `machines/native/run/images/<hex>.2/` with a sidecar
  `<hex>.2.json`; the staged tar is deleted. Docker unreachable is
  `Unavailable` `{what: "docker"}`, the machine stopped `Unavailable`
  `{what: "machine"}`. `--progress` reports `inspect`, `export`, `send`
  (`done`, `total`; `total` is an estimate until the last `send` event,
  which has `done` equal to `total`, the bytes sent), `unpack` and `start`. The record keeps the image's id,
  and every create sets its sidecar's `last_used`. The worker container's own
  image is the one `machine update docker --image` recorded, else that
  archive, loaded into Docker once and recorded on the docker machine as
  `loaded_image` (`{id, bundle}`; another bundle's image is loaded again).
  A bundle without the archive is `Unsupported`, naming it (#535 P5).
  A Sandbox recorded under the retired docker backend is `broken`
  `unsupported`; every call on it but `get` and `remove` is refused
  `Unsupported`, asking to recreate it; `sandbox remove` forgets it
  (`outcome: "forgotten"`) and answers its `container` with `fix: docker rm
  -f <container>`.
- **`remove`** records its `op` (`remove`) before it stops the Sandbox
  and removes the record at the end; progress is its phases `cancel` (with
  `force`, one event per command this home records), `stop` and `delete`.
  Without `force` it is `Busy` while a command runs (`next.running` lists
  their exec ids) and `InUse` while a Workspace is mounted
  (`next.workspaces` lists their handles). With `force`, each running
  command is cancelled first (a cancel not confirmed refuses it `Unknown`),
  then the Sandbox is stopped anyway: `unmounted` lists the Workspaces it
  ended, whose uncommitted changes end with it. `outcome` (long form) is
  `removed`, `already_gone` (its runtime was gone, or it never recorded
  one; `left` says what may remain) or `forgotten` (the retired docker
  backend). A lost answer keeps the record with the `op` uncertain
  (`broken` `unresolved`) until another `remove`; any other refusal leaves
  the Sandbox as it was.
- **`files`**, with no path, answers the Sandbox's roots, each with a
  `role` (long form): one `workspace` per mounted Workspace (`not_read:
  mount`, its handle in `workspace`), `home` (the folder that is `HOME`
  for its commands), `log` (its daemon's log, read with `sandbox log`)
  and, for a Sandbox with a root of its own (every one but a native macOS
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
  does not answer within 8 s is `Unavailable`. The MCP server lists them
  in its admin profile.
- Run for this manual on a scratch home: `sandbox list`, `get`, `files`
  and `log` of a recorded native Sandbox.

### `workspace` (7)

A Workspace is `<sandbox>:<name>`, the handle `mount` answers as
`workspace`; every other `workspace` call takes it. The path form of the
handle is gone: `<sandbox>:/path` is refused `InvalidInput`.

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `list` | — | `sandbox`, `limit`, `cursor` | every mounted Workspace, or one Sandbox's (agent profile) |
| `mount` | `sandbox`, `branch` | `name` | mounts a Branch in a Sandbox |
| `exec` | `workspace`, `command` | `cwd`, `env` (repeat), `stdin`, `stream`, `stdout`, `stderr`, `on_progress` | runs one bash command in a Workspace |
| `cancel` | `workspace` | `exec` | ends the commands running in a Workspace, or one of them |
| `commit` | `workspace` | `message` (`-m`), `expected`, `force` | commits a Workspace's changes to its Branch |
| `unmount` | `workspace` | `discard`, `force` | ends a Workspace |
| `status` | `workspace` | `path` | a Workspace's uncommitted changes, and whether its Branch moved |

- `list` reads Workspace records and asks no daemon: the home's own for a
  Sandbox on `native` or one reached directly, and one `workspace.list`
  request to each other ready machine's worker. A Sandbox whose machine,
  or which itself, is not ready is named in `not_listed` with
  `blocked_by`; one whose worker did not answer with `reason` (`timeout`,
  `busy`, `lost`, or `refused` with its refusal in `message`). Rows are
  `{workspace, branch}` (long: `sandbox`, `project`, `path`, `mounted_at`),
  ordered by Sandbox then name, paged by `limit` (1 to 1000, default 100)
  and `cursor`, which carries `--sandbox`.
- `mount` answers `{workspace, path}` (long: `outcome: mounted`, `branch`,
  `commit` the Commit it stands on, `base_layer`, `root`, `id` the 64-hex
  Workspace id, `namespace`). Without `name` the name is the Branch's made
  valid: lowercased, each character outside `a-z 0-9 _ . -` replaced by
  `-`, cut to 63 and `_ . -` trimmed from the ends (`Feature/Login` mounts
  as `feature-login`). A name in use is `Exists`; a Branch whose default is
  empty or reserved, and `main`, are `InvalidInput`; each with a `fix`. A
  seventeenth Workspace is `Capacity`.
- `exec` answers `{exit_code}` and `stdout`, `stderr` when not empty; long:
  `stdout_file`, `stderr_file`, `cancelled`, `exec`, `stop` (`cgroup` on
  Linux kernels, `group` on macOS `native`), `stdout_bytes`,
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
  `parent`, `root`, `message`, `created_at`, `merged_from`, `displaced`.
  `--expected C` refuses `Conflict` unless the Branch's head is `C`, a
  Commit id or a prefix of 8 or more of its characters resolved as
  `commit get` resolves it (a prefix several Commits start with is
  `InvalidInput` with `matches`); the check is one read before the Commit. A conflict is `Conflict` with
  `commit` (the head), `conflicts` (first 20 paths) and `conflicts_count`.
- `unmount` answers `{"ok":true}`; long: `outcome` (`unmounted` or
  `force_unmounted`) and a forced unmount's `abort`, `detach`, `cleanup`.
  `Dirty` carries `changes` (the first 20 entries, as `status` lists
  them), `changes_count` and `pending_merge`, with a `fix` naming
  `workspace commit`; `Busy` and an unconfirmed forced cancel (`Unknown`)
  carry `running` (the exec ids).
- `status` answers `changes`: the first 20 entries of one bounded read of
  256 overlay rows, each `{path, change}` (`added`, `modified`,
  `removed`) with `type: "dir"` for a folder; `changes_count` past the 20
  shown, `partial: true` at the bound, `branch_moved: true` when the
  Branch gained Commits since the mount, and `state` with its companion
  when not ready (from the daemon's `activity`). `--path` filters within
  the same read. A machine that is not ready refuses it `Unavailable`; it
  starts no machine.
- On `lima`, `mount`, `commit`, `unmount` and `status` go from the caller
  straight to the Sandbox's daemon (#535 G4); `exec` and `cancel` go
  through the machine's worker. On `docker` every call goes through its
  worker (D-8).

### `machine` (13)

The family is migrated: each call answers its short form, `--detail` its
long form (see [the form of an answer](#the-form-of-an-answer)).
`machine.doctor` is one of [the doctors](#the-doctors). Removed with no alias:
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
| `remove` | `machine` | `on_progress` | deletes a VM machine and its disk; one of a removed platform is only forgotten |
| `update` | `machine` | `cpus`, `memory`, `image`, `docker`, `wsl`, `daemon` | sets a machine's image, or a built-in machine's Docker socket, WSL distribution and daemon; no `disk` |

- **State from records.** `list` and `get` read the home's records and
  probe nothing. A machine's `state` is `ready`, `busy` (with `op { name,
  phase, started_at }`: a `create`, `start`, `stop` or `remove` running),
  `stopped` or `broken` (with `reason` and `fix`); `unknown` is never
  recorded. Reasons: `engine_down` (Docker is not running; `fix` starts it:
  `open -a Docker` on macOS, `sudo systemctl start docker` on Linux),
  `not_answering`, `outdated`, `missing`, `failed`, `unresolved` and
  `unsupported`. A records-only read answers the last state word and
  reason a start or a live read recorded, and `observed_at` from the last
  live read.
- **`--with live`** probes the provider and says hello to the worker once
  (`list`: every machine at the same time), within 10 s, starting nothing.
  The state, its companion, `message` and `observed_at` then come from the
  probe; `unknown` means the probe did not answer, and its `observed_at` is
  the last probe that did. The record is written only when the state word
  or reason changed (which moves the home's `generation`); `observed_at` is
  kept in `observed.json` beside the record, which does not. `get --with
  live` adds the `live` section: `disk_free_bytes`, `tool_version` (the
  provider tool's first line of `--version`) and `worker` (`{protocol,
  bundle}` as it answered), each null when not read.
- **Item and long keys.** A `list` row and the `get` answer have `machine,
  platform, state, op, reason, fix, observed_at, backend, supports, cpus,
  memory_bytes, disk_bytes, image, os_image, docker, wsl, daemon, message,
  stores, stores_count, sandboxes, sandboxes_count, created_at` (`get` puts
  `platform` and `state` first). `supports` is a list of words: `sqlite`,
  `postgresql_sandbox`, and `log` when the machine has a worker whose log
  can be read.
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
  machine's own PostgreSQL addresses in Store records; a machine of a
  removed platform (`firecracker`, `ssh`, `microsandbox`), shown as
  `broken`/`unsupported`, is only forgotten: `{"ok":true,"forgotten":true}`.
- **`images`** rows are `{image, refs, size, default}`, long adding
  `architecture, source, sandboxes, sandboxes_count, last_used_at`
  (`sandboxes` are names); the long answer adds `machine, mode,
  architecture, disk_free_bytes`. macOS `native` answers `mode: "host"` with
  none. `--with docker` reads the host's Docker: `images` it could copy,
  each `{image, refs, size, os, architecture, usable, needed_bytes, fits,
  unusable}`, with `reason` and `message` when Docker did not answer. It
  never starts a machine. **`load_image`** answers `{image, reused}`, long
  `outcome` (`loaded` or `reused`), `machine, refs, size`. **`prune`**
  answers `freed_bytes`, long `outcome, machine, removed` (the first 20
  images) and `removed_count`; an image in use is `InUse`.
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
  `native` has no worker: `Unsupported`.
- `stats`, `files` and `log` write no call-log line (`quiet`) and are no
  MCP tool. None of the reads starts a machine.

Run on a scratch home (macOS, a debug build; paths shortened):

```console
$ layerfs machine list
{"items":[{"machine":"docker","platform":"docker","state":"stopped"},{"machine":"native","platform":"native"}]}
$ layerfs machine get docker --with live
{"platform":"docker","state":"stopped","observed_at":"2026-11-18T15:34:11Z","live":{"tool_version":"29.5.2"}}
$ layerfs machine get native --detail --with live
{"platform":"native","state":"ready","op":null,"reason":null,"fix":null,"observed_at":null,"machine":"native","backend":"native","supports":["sqlite","postgresql_sandbox"],"cpus":null,"memory_bytes":null,"disk_bytes":null,"image":null,"os_image":null,"docker":null,"wsl":null,"daemon":null,"message":null,"stores":["shop"],"stores_count":1,"sandboxes":[],"sandboxes_count":0,"created_at":"2026-11-18T15:34:11Z","live":{"disk_free_bytes":null,"tool_version":null,"worker":null}}
$ layerfs machine start native
{"unchanged":true}
$ layerfs machine stats native
{"cpus":14,"memory_used_bytes":25457885184,"memory_total_bytes":38654705664,"disk_free_bytes":213131337728,"disk_total_bytes":994662584320}
$ layerfs machine files native /tmp/home1/store/shop --depth 1
{"items":[{"path":"/tmp/home1/store/shop/lock","type":"file","size":0,"not_read":"restricted"},{"path":"/tmp/home1/store/shop/record.json","type":"file","size":291},{"path":"/tmp/home1/store/shop/store.sqlite","type":"file","size":270336,"not_read":"restricted"}]}
$ layerfs machine get old
{"platform":"ssh","state":"broken","reason":"unsupported","fix":"layerfs machine remove old"}
$ layerfs machine remove old --detail
{"ok":true,"forgotten":true,"outcome":"forgotten","machine":"old","message":"forgot the record only: platform `ssh` was removed in LayerFS 2.0; nothing on its host was touched"}
```

See [machines](machines-and-stores.md#machines) for kinds, defaults and
refusals, and
[removed platforms](machines-and-stores.md#removed-platforms-firecracker-microsandbox-and-ssh).

### `store` (12)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `list` | — | `machine`, `--with stats` | every Store, from the home's records |
| `get` | `store` | `--with live,stats` | a Store's record and state; with `--with`, read where it lives or opened for its counters |
| `create` | `name` | `machine` | an empty sqlite Store, on this computer or made on `machine` directly |
| `connect` | `store`, `url` | `auth`, `machine`, `synchronous_commit` | connects a PostgreSQL database as a Store with a saved login; on a connected Store, sets its address, or with `machine` that machine's own (checked from it first) |
| `disconnect` | `store` | `machine` | forgets a PostgreSQL Store, its data staying; with `machine`, removes only that machine's own address |
| `save_auth` | `auth` | `user`, `password` (`--password-stdin`) | saves or replaces a PostgreSQL login in the home's secret folder |
| `auths` | — | — | the saved logins and the Stores that use each; never a password |
| `remove_auth` | `auth` | — | deletes a saved login no Store uses |
| `update` | `store` | `auth`, `synchronous_commit` | a PostgreSQL Store's login or write setting |
| `move` | `store`, `machine` | `on_progress` | moves a sqlite Store between this computer and a machine |
| `remove` | `store` | — | deletes a sqlite Store with every project in it |
| `compact` | `store` | `on_progress` | reclaims the dead bytes of one Store |

All twelve are migrated. `store.status` and `store.stats` are gone into
`store get` (`--with live`, `--with stats`), `store.push` and `store.pull`
into `store move`, a password in a Store's record or URL into saved
logins (`save_auth`), and `update`'s `machine`, `url` and `clear` into
`connect` and `disconnect` with `machine`, with no alias.

- **Saved logins.** `store save-auth <auth> --user <role>
  --password-stdin` writes `secret/auth/<auth>.json` (mode 0600, in a
  0700 folder) and moves the home's `generation`; the password is read from
  stdin only and never answered, logged or traced. Short `{"ok":true}`;
  long `ok, outcome (saved or rotated), auth, user, stores, stores_count`.
  A Store records only the login's name (`auth`); its URL holds host,
  port, database and TLS settings, and one naming a user or holding a
  password is refused `InvalidInput`. The login goes into the URL only
  when the Store is dialled. `auths` answers `{ items }`, short `{ auth,
  user }` with `stores` (at most 20) and `stores_count` when used, else
  `stores_count: 0`; long
  adds `created_at, updated_at`. `remove_auth` is refused `InUse` (with
  `stores`) while a Store uses the login, `NotFound` when there is none;
  long `ok, outcome: "removed", auth`.
- **`connect`**, first time: `--auth` is required and must be saved;
  `synchronous_commit` defaults to `local`. It creates the LayerFS schema
  in an empty database or reads the Store's id from it (refused `Exists`
  when another Store of this home has that id or name). Short
  `{ projects, projects_count }` (`projects_count` alone for an empty
  database; `{"ok":true}` when only an address changed); long `ok,
  projects, projects_count, outcome (connected or updated), store, engine,
  auth, connections, synchronous_commit`. On a connected Store `connect`
  checks the new address against the same Store's id and takes no `--auth`
  or `--synchronous-commit` (`InvalidInput`, naming `store update`); with
  `--machine M` the address is `M`'s own, checked from `M` with the
  Store's login. `projects` is null when only an address changed.
- **`update`** changes the login (checked against the same Store) or
  `synchronous_commit`; at least one. Long `ok, outcome: "updated", store,
  auth, synchronous_commit`.
- **`disconnect`** long `ok, outcome: "disconnected", store, machine`;
  refused `InUse` while a Sandbox or Workspace uses the Store, and
  `NotFound` for a machine with no address of its own.

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
- **`get`**, short: `{ engine, state }`, with `machine`, `projects`,
  `projects_count`, `backups` (each `{ bucket }`), `connections` and the
  state's companion when they say something. Long: `engine, state,
  machine, auth, connections, projects, projects_count, backups, op,
  blocked_by, reason, fix, observed_at, store, profile, synchronous_commit,
  sqlite_version, created_at, last_transfer`. A sqlite Store answers
  `machine`, `profile` and `sqlite_version`; a PostgreSQL Store
  `connections` (the Store's own address, with no `machine`, then each
  machine's own; never a password) and `synchronous_commit`; the other
  engine's keys are null. `projects` shows at most 20 names;
  `projects_count` counts them all. A `backups` row's long form is
  `bucket, file, sha256, heads_sha256, last, attempt`: `last` the last
  transfer that worked (`{ call, ended_at, size }`), `attempt` the last one
  that did not (`{ call, ended_at, error }`). `auth` is a PostgreSQL
  Store's saved login, by name.
- **`get --with live`** reads the Store where it lives, starting nothing: a
  sqlite Store's file through its machine (in this process on `native`,
  else its recorded worker), a PostgreSQL Store's server from this
  computer. The read gives `state` and `observed_at`: `broken` `missing`
  when no file is where the record says, `broken` `unreachable` when the
  server does not answer, `busy` or `broken` `unresolved` from a
  compaction's own record, `unknown` (with when LayerFS last knew) when the
  machine or worker does not answer. The record is written only when what
  the read found wrong changed (which moves the home's `generation`);
  `observed_at` goes to `store/<store>/observed.json`, which does not. A
  backup or restore whose owner is gone with its outcome unknown is settled
  here: the read drops the `op` and writes what was tried to the bucket
  row's `attempt`. The `live` section: `holders` (pids of other live
  processes holding the Store), `phases` (a running compaction's phases,
  else null), `behind` (the buckets whose backup heads differ from the
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
  `store` first: short `{ store, engine }` plus what says something.
  `--machine M` keeps the sqlite Stores on `M`. `--with stats` adds each
  item's `size` from one stat on its machine, starting nothing; null for a
  PostgreSQL Store and for a machine that does not answer.
- **`create`**, short `{"ok":true}`; long `ok, outcome: "created", store,
  engine, machine, profile, sqlite_version, created_at`. `--machine M`
  makes the Store on `M` directly through its worker (a running machine),
  recording a `create` operation while it runs.
- **`move`** goes to or from this computer only: a Store on a machine moves
  to another machine by `store move <store> native` first (refused
  `InvalidInput` naming that call). Its progress phases are `check,
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
  estimate, zero_reasons`. It records a `compact` operation in the Store's
  record while it runs; one left unresolved is cleared by the next
  `store compact`.

Run (macOS, native; the outputs are this build's):

```console
$ layerfs store create shop
{"ok":true}
$ layerfs store list
{"items":[{"store":"shop","engine":"sqlite","machine":"native","projects_count":0}]}
$ layerfs store get shop
{"engine":"sqlite","state":"ready","machine":"native","projects_count":0}
$ layerfs store get shop --with live --detail
{"engine":"sqlite","state":"ready","machine":"native","auth":null,"connections":null,"projects":[],"projects_count":0,"backups":[],"op":null,"blocked_by":null,"reason":null,"fix":null,"observed_at":"2026-11-18T15:35:10Z","store":"shop","profile":"disposable","synchronous_commit":null,"sqlite_version":"3.53.2","created_at":"2026-11-18T15:35:10Z","last_transfer":null,"live":{"holders":[],"phases":null,"behind":[],"server_version":null,"latency_ms":null}}
$ layerfs store list --with stats
{"items":[{"store":"shop","engine":"sqlite","machine":"native","projects_count":0,"observed_at":"2026-11-18T15:35:10Z","size":270336}]}
$ layerfs store compact shop
{"freed_bytes":0}
$ layerfs store move shop native
{"error":"InvalidInput","message":"store move, check: Store shop is already on native"}
```

PostgreSQL, against a local `postgres:17` (macOS, native; this build's
outputs):

```console
$ printf '%s' "$PG_PASSWORD" | layerfs store save-auth pg-app --user postgres --password-stdin --detail
{"ok":true,"outcome":"saved","auth":"pg-app","user":"postgres","stores":[],"stores_count":0}
$ layerfs store connect shop-pg 'postgresql://127.0.0.1:55231/shop?sslmode=disable' --auth pg-app --detail
{"ok":true,"projects":[],"projects_count":0,"outcome":"connected","store":"shop-pg","engine":"postgresql","auth":"pg-app","connections":[{"machine":null,"url":"postgresql://127.0.0.1:55231/shop?sslmode=disable"}],"synchronous_commit":"local"}
$ layerfs store auths
{"items":[{"auth":"pg-app","user":"postgres","stores":["shop-pg"],"stores_count":1}]}
$ layerfs store get shop-pg
{"engine":"postgresql","state":"ready","auth":"pg-app","connections":[{"url":"postgresql://127.0.0.1:55231/shop?sslmode=disable"}],"projects_count":0}
$ layerfs store connect shop-pg 'postgresql://postgres:pw@127.0.0.1:55231/shop'
{"error":"InvalidInput","message":"the URL names a user or holds a password; give the address alone and the login with --auth (`layerfs store save-auth` saves one)"}
$ layerfs store update shop-pg --synchronous-commit off --detail
{"ok":true,"outcome":"updated","store":"shop-pg","auth":"pg-app","synchronous_commit":"off"}
$ layerfs store remove-auth pg-app
{"error":"InUse","message":"the login pg-app is used by shop-pg: `layerfs store update <store> --auth <other>` or `layerfs store disconnect <store>` first","stores":["shop-pg"]}
$ layerfs store disconnect shop-pg --detail
{"ok":true,"outcome":"disconnected","store":"shop-pg","machine":null}
```

See [Stores](machines-and-stores.md#stores).

### `backup` (6)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `connect` | `name`, `url` | `access_key`, `secret_key` (shell: `--key-stdin`) | connects a bucket (r2, or an S3 endpoint) for Store backups |
| `disconnect` | `bucket` | — | forgets a bucket and its key; its files stay |
| `list` | — | `bucket` | the Store files in the connected buckets, with the Store each is linked to |
| `create` | `store`, `bucket` | `file`, `on_progress` | backs up a sqlite Store to a bucket |
| `restore` | `bucket`, `file`, `store` | `on_progress` | restores a Store file from a bucket as a sqlite Store on this computer |
| `doctor` | `bucket` | `full` | checks a bucket; `--full` also tests a write |

All six are migrated. Today's `--key` reference (`env:` or `file:`),
`--region` and `--part-size` are gone with no alias; `list`'s bucket and
`create`'s file are options, and `restore` names the bucket, its file, then
the Store.

- **The key.** `--access-key` is the key's id; its secret comes on stdin
  with `--key-stdin` and no other way (Node and Python pass `secret_key`).
  The secret is kept in the home's secret folder (`secret/bucket/<bucket>`,
  0600), never answered, and the call log records its byte count only, as
  it does for the URL. `connect` makes one listing before it records
  anything: a bucket that does not answer or refuses the key is
  `Unavailable`; a name that is taken is `Exists` (to change a key,
  disconnect and connect). A bucket connected before 2.0 keeps the key
  reference its record names.
- **`list`** answers one row per Store file in each bucket, and one per
  link whose file the bucket does not hold (`missing: true`). `behind` says
  the Store changed since its last backup (its heads read where it lives:
  in this process on this computer, through the machine's worker
  otherwise); `changed` says the bucket's file is not the one the link last
  moved; each is null with a `message` when it could not be read. A bucket
  that cannot be listed is named in `not_listed` (`reason` `refused`,
  `timeout` or `lost`) and the others are still listed.
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
  `bucket.credentials`, `bucket.transport`, `bucket.list`; `--full` adds
  the write test (`bucket.write_capability`, `bucket.put`, `bucket.get`,
  `bucket.delete`, and `bucket.reconcile` after a put whose reply was
  lost). Without `--full` those checks are left out of the report.

See [backups](machines-and-stores.md#backups).

## The doctors

Six table calls check one thing each and change nothing: they take no
lock, write no record, remove no stale lease and start no machine.

```bash
layerfs home doctor                 # the home, then each machine, PostgreSQL Store and bucket, shallowly
layerfs machine doctor <machine>    # the machine, then each Store and Sandbox on it
layerfs store doctor <store> [--machine <machine>]   # the Store, then each of its projects
layerfs project doctor <project>    # the project's record against its Store's own list
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
- The long form (`--detail`) adds `status` (the worst of `error`, `warning`,
  `not_checked`, `notice`, `ok`), `counts` (things per status),
  `observed_at` and `tree`: the doctor's own thing with `status`,
  `coverage` (`none`, `shallow` or `full`), its `checks` (`{ check,
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
  `backup doctor` sends none.
- A Workspace has no doctor of its own: `sandbox doctor` reads its daemon's
  list once and gives each Workspace it lists or the home records one node,
  `<sandbox>:<name>`, with `workspace.mounted` and `workspace.activity`; a
  failed or uncertain one is an error fixed by `layerfs workspace unmount
  <workspace> --discard --force`. A daemon that is not ready leaves its
  Workspaces `not_checked`, blocked by the Sandbox (or its machine's
  worker), and none is tried.
- `project doctor --full` is refused `Unsupported` (not built yet). `store
  doctor --machine` is for a PostgreSQL Store; a sqlite Store refuses it.
- A Store's checks, in this order: `store.record` (its record reads and
  names it), `store.operation` (an operation whose owner is gone with its
  outcome unknown is an error whose `fix` is the call that clears it),
  `store.connection` (a sqlite Store's file is where its record says,
  read through its machine; a PostgreSQL Store's server answers this
  computer with the saved login), `store.identity` (the Store's id is the
  recorded one), `store.schema` (its schema is this build's) and, for a
  sqlite Store, `store.backups` (a notice per bucket whose backup heads
  differ from the Store's, `fix` `layerfs backup create <store>
  <bucket>`). That is the whole cut: no Store data is scanned. A sqlite
  Store's id and schema are read together, so a mismatch shows as
  `store.schema`. A PostgreSQL Store is then probed from each machine
  that has a Sandbox of it and from `--machine` (its own address where it
  has one, else the Store's), each as one more `store.connection` row
  with `from`. A Store whose machine is not ready shows its
  `store.connection` as `not_checked` with `blocked_by`, and nothing after
  it is tried.
- `--text` prints a count line (`N error(s), N warning(s), N notice(s), N
  not checked`, or `ok` alone), then each finding, its `from` and its `fix`,
  then each `not_checked` entry.
- Exit 0 once the report is made, whatever it finds; 125 with `NotFound`
  when the thing is not in the home; 2 for a wrong command line. Leftovers
  of interrupted operations are in no doctor's report: `home leftovers`
  lists them and `home prune` removes one (see [`home`](#home-7)).

## Results

- **stdout** carries the call's result as one line of JSON.
- **`--text`** prints a readable form instead: a file's text as it is; a
  result's list of records as a table with upper-case headers; other fields
  as `key  value` lines. It is one generic form, not a layout per call.
- A migrated call answers its short form, or its long form with `--detail`
  ([the form of an answer](#the-form-of-an-answer)); its lists carry `items`
  and `next_cursor`.
- The paged results of a call that is not migrated carry `items`,
  `has_more` (most families) and `next_page_pos`; pass `next_page_pos` back
  as `page_pos`. The diff calls also carry `next_cursor`, passed back as
  `cursor`.
- Result keys are snake_case in every front end. Only identifier-like keys
  change; a key that holds a path or a name never does.
- Bytes are `Buffer`s in Node and `bytes` in Python.

## Errors and exit codes

| Exit | Meaning |
| --- | --- |
| 0 | the call succeeded |
| 2 | the command line is wrong (wrong number of positionals, unknown option, a value that is not a whole number or not `true`/`false`, bad JSON, an unreadable `--from` file); nothing was called |
| 125 | LayerFS refused or failed the call; the refusal is JSON on stderr |

`workspace exec --stream` exits with the command's own code instead (0 for
a result with no exit code, 1 for a code outside 0 to 255); 125 still means
LayerFS refused or failed. `layerfs help` and `layerfs --version` need no
home.

A refusal is one line of JSON: `{"error", "message"}`, then `fix` when
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
layerfs: file.read takes 3 to 4 positional arguments, not 1
$ echo $?
2

$ layerfs store get nope
{"error":"NotFound","message":"no Store named nope"}
$ echo $?
125

$ layerfs file read nope/p main /a --detail
{"error":"NotFound","message":"no Store named nope","fix":null,"detail":{"code":"NoSuchStore"}}
```

`error` is one of thirteen codes:

| Code | Meaning |
| --- | --- |
| `NotFound` | no machine, bucket, Store, project, Branch, Commit, path, Sandbox or Workspace of that name; a path a project hid |
| `Exists` | the name is taken, the same database or bucket is already connected (under any name), or a bucket object was exported from another home |
| `InUse` | held: running Sandboxes, mounted Workspaces, processes with the Store open, Stores on a machine; a layout 2 home with any of these, or with a move, removal or create still running, which is not converted yet ([the home](home.md#a-layout-2-home)) |
| `Busy` | an operation runs on the entity: a Store being pushed, pulled or removed, a machine being made or removed, a Sandbox being created or closed; the message names it (`op` id and phase) and its process (#536 §7.1); the long form's `detail` is `{entity: {kind, name}, phase, op}`. Also: another project is being added to the same Store; a Store's writable open met SQLite's busy lock before any effect (the open wrote nothing, so the caller may repeat the call) |
| `Unresolved` | an earlier operation on the entity ended with its outcome not known; its process is gone and its `op` stays. The message names the call that shows it (`store status`, `machine status`, `sandbox get`) and the one that clears it (`store remove`, `machine remove`, `sandbox close`); `detail` as for `Busy` |
| `Conflict` | a merge or Commit conflict, or a head that moved; for `backup.restore`, the Store changed since its last backup or restore |
| `Dirty` | uncommitted changes stand in the way |
| `Capacity` | a Sandbox's Workspace limit; a `store push` or `store pull` target without room for the Store (the long form's `detail.needed`, `detail.free`); no space left |
| `InvalidInput` | the request is wrong for this name or kind (a file where a folder is needed, a binary file read as text, a reserved name, a write to `main`, an unknown `with` section, a cursor of another call); nothing happened |
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
| `Unavailable` | `Denied`, `Unauthorized`, `StaleSandbox`, `StaleStore`, `Timeout` |
| `Failed` | `UnknownMethod`, and any code this table does not name |

Every call is one attempt. After `Unknown`, read the state with `list`,
`get` or `status` before acting again.

## Shell switches

| Switch | Calls | Does |
| --- | --- | --- |
| `--text` | all | a readable result instead of JSON |
| `--detail` | all | the long form of a migrated call's answer, and of any call's refusal ([the form of an answer](#the-form-of-an-answer)) |
| `--with <a,b>` | the calls whose row lists sections (`home get`) | adds the named sections to the answer |
| `--progress` | `project init`, `project export`, `workspace exec`, `store push`, `store pull`, `backup create`, `backup restore` | progress as one JSON line per event on stderr, the event itself: `{"phase", "done", "total", …}`; for `workspace exec`, the exec id first |
| `--stream` | `workspace exec` | passes the command's stdout and stderr through (output over 1 MiB too) and exits with its code |
| `--from <file>` | `file write`, `file apply` | reads the content or JSON from a file instead of stdin |
| `--<option>-from <file>` | any optional text option (`--old-from`, `--new-from`, `--message-from`) | reads the option's value from a file |
| `-m <text>` | calls with `message` | the same as `--message` |
| `--scope <folder>` | all | a folder scope (see below); needs `--scope-project` |
| `--scope-project <store>/<project>` | all, with `--scope` | the one project the scope is bound to |
| `--scope-branch <name>` | all, with `--scope` | the scope's Branch namespace |
| `--` | all | every later word is a positional |

`--option=value` works for every option. A bool option alone
(`sandbox close … --force`) is true; `--force=false` is false.

`store push` and `store pull` report their phases (`check`, `snapshot`,
`send`, `verify`, `install`, `record`, `retire`) as `{"phase", "done",
"total"}`: `done` and `total` are bytes during `send` and null in
the other phases. `project init` reports the same shape on every route:
`check`, `send` (only into a sqlite Store on another machine, while the
folder is copied), `install` (Init adds the folder) and `record` (from
source and tests; the run for this manual predates it).

## Folder scope

A folder scope (O32) binds a client to one project, one folder of it and,
optionally, one Branch namespace: `{ project, folder, branch? }`. Node:
`layerfs.client({ scope: { project, folder, branch } })`. Python:
`layerfs.client(scope={"project": ..., "folder": ..., "branch": ...})`.
Shell: `--scope <folder> --scope-project <store>/<project> [--scope-branch
<name>]`; `--scope` without `--scope-project` is exit 2.

- It is an allow list: the `file` calls, `branch` `list`, `get`, `fork`,
  `merge` and `publish`, and the `workspace` calls. Every other call
  (Stores, machines, backups, projects, Sandboxes) is refused `out of
  scope: <call>`, such as `out of scope: store.remove`.
- Every `file` and `branch` call must name the scope's project (`out of
  scope: project`); a `workspace` call's Sandbox must be recorded for that
  project (`out of scope: sandbox`).

- `folder` is a canonical absolute project path. Every path a `file` call
  names must be the folder or beneath it; paths stay absolute.
- With `branch`, Branch names are relative to the namespace and composed as
  `<branch>.<name>`; `branch.list` answers only that namespace's Branches,
  named relative to it. Without `branch`, every call that names a Branch is
  refused. A 66-character Commit id is left as it is.
- `commit` calls and `branch.diff` are refused as history. A scoped
  `branch.list` reads every page of the project's Branches and pages the
  namespace's own.
- `workspace.mount` mounts only the namespace's `main`; `workspace.exec`
  starts the command in the scope's folder.
- A refusal is `InvalidInput` with `out of scope: path`, `branch`,
  `history`, `project`, `sandbox` or the call's name, before any effect.
- A scope guards calls. It is not filesystem isolation: a command can still
  `cd` anywhere in the mounted tree.

Run before the scope named its project (`797ecca2c7`), shown here in the
current form; the answers follow from the code but were not rerun:

```console
$ layerfs commit list shop/api agent --scope /src --scope-project shop/api
{"error":"InvalidInput","message":"out of scope: history"}
$ layerfs file list shop/api agent /src --scope /src --scope-project shop/api
{"error":"InvalidInput","message":"out of scope: branch"}
```
