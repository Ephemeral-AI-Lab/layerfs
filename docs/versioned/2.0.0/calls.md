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
{"home":"/tmp/home1","created":true,"converted":false,"layout":3}
$ layerfs home get
{"home":"/tmp/home1","state":"ready","version":"2.0.0","generation":2}
$ layerfs store create shop
{"name":"shop","kind":"sqlite","machine":"native","profile":"disposable","created":1795008741370,"sqlite":"3.53.2","projects":[]}
$ layerfs home get --with tree
{"home":"/tmp/home1","state":"ready","version":"2.0.0","generation":4,"tree":{"machines":[{"machine":"docker","state":"stopped"},{"machine":"native","stores":[{"store":"shop"}]}],"stores":[],"buckets":[]}}
$ layerfs home get --with live
{"error":"InvalidInput","message":"home.get: `with` names `live`; this call takes platforms, tree"}
```

## The calls

`[x]` is an optional positional. Every family's results are snake_case.

### `home` (4)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `init` | — | — | creates this home, converts an older layout to the current one, and leaves a current home as it is |
| `get` | — | `--with platforms,tree` | the home's state and build; with `--with`, the platforms this computer supports or the tree of everything in the home |
| `log` | — | `call`, `project`, `client`, `tag`, `outcome`, `since`, `limit`, `cursor` | the calls made against this home, newest first |
| `clear_log` | — | — | clears this home's call log |

- `init` and `get` are not opened by the dispatcher: `init` opens or
  creates the home itself, and `get` never opens it. `get`, `log` and
  `clear_log` write no line to the call log, and all four are `admin` in the
  MCP server. `init` is not migrated: it returns
  `{ home, created, converted, layout }`.
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
  Workspace record) and never for a read: a caller that kept a tree reads it
  again only when the number differs.
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
  buckets. A node's short form is its name, and its state only when it is
  not `ready`; a Store adds `projects` and, when it is not sqlite,
  `engine`; a Sandbox adds `workspaces`, each `{ workspace, branch }`. The
  long forms are: machine `machine, platform, cpus, memory_bytes, state, op,
  reason, fix, stores, sandboxes`; Store `store, engine, state, op,
  blocked_by, reason, fix, projects, connections` (`connections`: the
  machines a PostgreSQL Store is connected from); Sandbox `sandbox,
  project, image, state, op, blocked_by, reason, fix, workspaces`; bucket
  `bucket, provider`. A Store or Sandbox whose machine is stopped, busy or
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
- `home.log` and `home.clear_log` were `logs.list` and `logs.reset`; the old
  names are gone, with no alias. A row's `at`, `ms`, `lines`, `has_more`
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
| `init` | `project`, `folder` | `timeout` (ms, default 1 hour), `on_progress` | imports a folder on the caller's host as a new project into an existing Store |
| `list` | `[store]` | `page_size`, `page_pos` | every project in the home, or one Store's |
| `get` | `project` | — | a project's record: its Store, kind and machine |
| `stats` | `project`, `[branch]`, `[commit]` | — | entry counts of one pinned saved tree |
| `export` | `project`, `destination` | `branch` (default `main`), `on_progress` | exports a pinned committed Branch snapshot to a local directory |
| `delete` | `project` | — | retires one project in its Store and removes its home entry |
| `remove_file` | `project`, `path` | — | hides one absolute path from every node at or below its marker frontier |

- `init` never creates a Store; a missing one is `NotFound`. `folder` is
  always on the caller's host. Into a sqlite Store on `native` or a
  PostgreSQL Store, Init runs in the caller's process. Into a sqlite Store
  on another machine (which must be running), the caller copies the folder
  to that machine's worker, which runs Init on the copy and then deletes it
  (#536 H11, H11a): the machine needs free room for the copy while Init
  runs (`Capacity` otherwise), and progress shows phase `send` with bytes
  sent before Init's own counts. A missing folder, or one with a socket,
  FIFO or device file, is refused before anything is sent. Run: answers
  `project`, `store`, `name`, `kind`, `machine`, `created`, `layer` and
  `root`.
- `delete` is refused `Busy` while a Sandbox of the project is recorded
  (`detail.related` lists each as `{kind: "sandbox", name}`).
- `remove_file` is visibility-only in 2.0: it hides the path across the
  project's history and frees no bytes; the marker frontier governs later
  re-creations.

`project export` ([reference](../../architecture/api/sdk/project.md#projectexport)) exports a committed Branch snapshot. `destination` is an absolute path
on the caller’s host, even for a worker Store. It must be absent or an empty
real directory, outside the home, with an existing parent. A nonempty destination
is `InvalidInput` (raised as `NotEmpty`); files and symlinks are refused. Same-parent staging is published
by atomic rename only after all bytes and metadata are written; failures clean
up this call’s staging without deleting unrelated files.

Export takes a Branch name, with no `commit` option or `--commit` flag. It pins
the node/root once, excludes uncommitted Workspace changes and is unaffected by
later publications. SQLite reads locally or streams bounded frames from its
worker; PostgreSQL reads in the caller’s process, for local or remote servers.
A required stopped worker is `Unavailable` and is never started automatically.
The fidelity contract includes binary files, empty directories, `.git`, ignored
files, symlinks, permission bits and nanosecond `mtime` where the host supports
them. Host name/case collisions are `Conflict`, rather than silent overwrites.

```bash
# Example, not run: /exports exists and /exports/api is absent or empty.
layerfs project export shop/api /exports/api --branch agent --progress
```

The final JSON result contains `project`, `store`, `name`, `kind`, `machine`,
`branch`, `snapshot`, `node`, `root`, `destination`, `files`, `directories`,
`symlinks` and `bytes`. Progress on stderr is JSON Lines with `prepare`, `export`,
`finalize`, local `files`/`directories`/`symlinks`/`bytes`, `done = bytes` during
writing and `total: null`; `finalize` is the last event and comes only once the
folder is in place. Events arrive at most every 200 ms.

### `branch` (9)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `list` | `project` | `page_size`, `page_pos` | a project's Branches |
| `get` | `project`, `branch` | — | one Branch and its head Commit |
| `commits` | `project`, `branch` | `page`, `page_size`, `at_commit`, `include_storage` | a Branch's Commits, newest first |
| `get_commit` | `project`, `branch`, `commit` | — | one saved Commit or `main` publication |
| `fork` | `project`, `name`, `from_branch`, `[from_commit]` | — | a new Branch from another Branch's head or Commit |
| `merge` | `project`, `name`, `a`, `b` | `message`, `units` (JSON) | merges two Branches into a new one |
| `publish` | `project`, `branch`, `[commit]` | `expected_base`, `expected_head`, `force` | a Branch's head or Commit as `main`'s next Layer, refusing a stale base |
| `diff` | `project`, `from_branch`, `to_branch` | `paths_only`, `page_size`, `page_pos`, `cursor` | files that differ between two Branches' heads |
| `remove` | `project`, `branch` | — | soft-removes one Branch; its name stays taken until compaction |

Run: `fork` answers `{kind: "Forked", branch, base, head}`; `publish`
answers `{kind: "Added", source, published_layer, based_on, overwrote_count,
overwrote, overwrote_has_more}`; `merge` answers `{kind: "Merged",
fast_forward, branch, head, root, sources, merge_base, conflicts, lww,
units, …}`. `publish` is refused `HeadMoved` when `main` gained a Layer since
the Branch forked (merge with `main` into a new Branch and publish that), or
when `main` or the Branch head is not the `expected_base` or `expected_head`
given; `force` overwrites instead. A Commit already published answers
`UpToDate`, and a tree `main` already holds answers `NoChanges`.

### `commit` (3)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `list` | `project`, `branch` | `page_size`, `page_pos` | a Branch's Commits, newest first |
| `get` | `project`, `branch`, `commit` | — | one Commit of a Branch |
| `diff` | `project`, `from_commit`, `to_commit` | `paths_only`, `page_size`, `page_pos`, `cursor` | files that differ between two Commits |

A Commit record has `id`, `branch`, `parent`, `second_parent`, `base`,
`root`, `via`, `message`, `merged_from`, `time`, `logical`,
`new_canonical`, `new_stored`, `new_metadata`, `displaced`, `lww` and
`lww_truncated`. `commit.get` accepts an id prefix of 8 or more characters.

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

### `workspace` (6)

A Workspace is `<sandbox>:<path>`, or `<sandbox>:<name>` when its mount
named it, answered as `ws` by `mount`. Every `workspace` call takes either
form.

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `mount` | `sandbox`, `branch` | `name` | mounts a side Branch in a Sandbox; `main` is refused `InvalidInput` (fork a Branch from it) |
| `exec` | `ws`, `command` | `message`, `stream`, `stdout`, `stderr`, `exec_id`, `on_progress` | runs one bash command in a Workspace |
| `exec_cancel` | `ws`, `exec_id` | `grace_ms` | ends a running command: SIGTERM, then SIGKILL after `grace_ms` |
| `commit` | `ws` | `message`, `force` | commits a Workspace's changes to its Branch |
| `unmount` | `ws` | `discard`, `force` | ends a Workspace |
| `status` | `ws` | — | a Workspace's changes since its base (O14) |

- `mount` with `name` names the Workspace (a name, as for Sandboxes);
  `ws` is then `<sandbox>:<name>`. A name is unique within its Sandbox: one
  in use is `Exists`. It is free again once that Workspace is unmounted.
  `mount` answers `name` (or `null`) beside `path`. A name no mounted
  Workspace carries is `NotMounted`.
- `exec` answers `kind` (`exit:<code>`), `exit_code`, `stdout`, `stderr`,
  `message`, `truncated`, `exec_id` and `stop` (`cgroup` on Linux kernels,
  the docker machine included; `group`, the command's own process group,
  on macOS `native`). A command that exits non-zero is still exit 0
  for the shell unless `--stream` is given. Inline output that is not UTF-8
  comes back as bytes.
- A stream over 1 MiB is not inline: it is sent as output events, one
  1 MiB page at a time, and the reply has that stream `null` with
  `stdout_bytes` or `stderr_bytes` and `streamed: true`. Only the
  `stdout`/`stderr` functions, `stream: true` (Node, Python) and `--stream`
  (shell) receive those bytes; without one of them they are not kept.
- `stdout` and `stderr` (Node, Python) are functions given the command's
  output. `stream: true` without them writes it to the calling process's
  own stdout and stderr. The library sends the output after the command
  ends; there is no live output stream yet.
- Every command has an exec id: `exec_id` names it (1 to 64 letters,
  digits, `.`, `_` or `-`, not starting with `.` or `-`), else LayerFS
  makes one (`exec-` and 16 hex digits). Either way it is the call's first
  progress event, `{exec: {id}}`, given to `on_progress` (Node, Python)
  and printed by `--progress` (shell) before the command starts. An id a
  running command of the same Workspace carries is `Exists`.
- While a command runs, its record lets any process end it:
  `workspace exec-cancel <ws> <exec-id> [--grace-ms N]` sends SIGTERM to
  everything it started, waits `grace_ms` (default 5000), then SIGKILL. It
  answers `{outcome: "cancelled", signal}` (`signal` is the last one sent),
  `{outcome: "ended"}` when the command had already finished (or has no
  record), or `{outcome: "unknown"}` when the machine's worker was not
  reached or did not answer: nothing is known to have changed. The
  cancelled command's `exec` answers `kind: "cancelled"` with exit code
  128 + the signal. A cancel that comes before the command starts answers
  `cancelled` with `SIGKILL`, and the command is ended as it starts (exit
  code 137). After `unknown` the record keeps the cancel's mark: cancel
  again once the worker answers; if the command then ends by a signal,
  its `exec` answers `kind: "cancelled"`. On Linux each command runs in its own cgroup, so
  children that left its process group end too; on macOS a child that
  calls `setsid` itself escapes the group.
- A command record whose owning process is gone is stale. The next exec of
  its id, cancel, unmount or close ends its command (no grace), removes it
  and logs it to `log/exec.jsonl`; `layerfs home doctor` reports it. On a
  Sandbox the caller reaches directly, a stale record whose worker cannot
  be reached is kept as it was.
- `commit` merges first when the Branch moved since the mount; `force`
  publishes over a moved head instead.
- `unmount` with `discard` drops uncommitted changes; `force` aborts its
  connection. `unmount` is `Busy` while a command runs in the Workspace
  (`detail.execs`); with `force` each one is cancelled first, and one whose
  cancel is not confirmed refuses it `Unknown`.
- `status` starts no machine (#535 P4): on a Sandbox whose machine is down
  it is `Unavailable`, naming `machine start`. `mount`, `exec`, `commit`
  and `unmount` on a stopped machine are refused the same way, as Store
  calls are; nothing starts a machine but `machine start` (#535 D11).
- On `lima`, `mount`, `commit`, `unmount` and
  `status` go from the caller straight to the Sandbox's daemon (#535 G4);
  `exec` goes through the machine's worker. On `docker` every call goes
  through its worker (D-8). See machines-and-stores.md,
  Routes.
- Not run for this manual.

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
| `list` | — | — | every Store, from the home's records, each with its `state` |
| `get` | `name` | — | a Store's record |
| `status` | `name` | `machine` | a Store's live state; with `machine`, whether that machine reaches a PostgreSQL Store (`check`) |
| `stats` | `name` | — | Store-wide publication and allocation counters |
| `create` | `name` | — | an empty sqlite Store on `native` |
| `connect` | `name`, `url` | `password`, `synchronous_commit` | connects a PostgreSQL database as a Store; its Sandboxes run on every machine that supports them |
| `update` | `name` | `machine`, `url`, `password`, `synchronous_commit`, `clear` | changes a PostgreSQL Store's connection; with `machine`, gives that machine a connection of its own (checked from it first), or removes it with `clear` |
| `push` | `name`, `machine` | `on_progress` | sends a sqlite Store from `native` to a machine |
| `pull` | `name` | `on_progress` | brings a sqlite Store from its machine to `native` |
| `remove` | `name` | — | deletes a sqlite Store with every project in it |
| `compact` | `name` | — | reclaims the dead bytes of one Store: the `none` path marks only, the `rebuild` path installs a new generation |
| `disconnect` | `name` | — | forgets a PostgreSQL Store; its data stays |

Run:

```console
$ layerfs store get shop --text
name  shop
kind  sqlite
machine  native
profile  disposable
created  1793574130507
sqlite  3.53.2

PROJECTS  (none)

$ layerfs store status shop
{"name":"shop","kind":"sqlite","state":"idle","holders":[],"op":null,"machine":"native","file":{"exists":true,"bytes":212992,"wal_bytes":271952}}
```

`state` is `idle`, `in_use`, `busy`, `down` or `unresolved`; `op` is the
move or remove running on the Store, `{id, name, phase, started,
uncertain}`, or `null`. `store status` starts no
machine: a Store whose machine is stopped, or whose recorded worker does
not answer, is `down`, with `down_reason` and `file.unavailable` (O34).
`store status`, `store list` and `store get` change nothing: they take no
lock, and `holders` leaves out a stale lease without removing it. See
[Stores](machines-and-stores.md#stores).

Each `store list` item is the Store's record (as `store get` shows it) with
the same `state` last, derived as `store status` derives it (#536 §10b):
it starts no machine, dials a machine's recorded worker once for all its
Stores, and is `down` for a Store whose kind or machine its record does
not tell. Not run for this manual (from source and tests).

### `backup` (5)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `connect` | `name`, `url` | `key`, `region`, `part_size` (size) | connects an r2 or MinIO bucket |
| `disconnect` | `name` | — | forgets a bucket and this home's links to it |
| `list` | `[bucket]` | — | every Store file on the connected buckets |
| `create` | `store`, `bucket`, `[file]` | `on_progress` | uploads a sqlite Store to a bucket |
| `restore` | `store`, `bucket`, `[file]` | `on_progress` | downloads a backup as a sqlite Store on `native` |

See [backups](machines-and-stores.md#backups). Not run for this manual.

## The doctors

Six table calls check one thing each and change nothing: they take no
lock, write no record, remove no stale lease and start no machine.

```bash
layerfs home doctor                 # the home, then each machine, PostgreSQL Store and bucket, shallowly
layerfs machine doctor <machine>    # the machine, then each Store and Sandbox on it
layerfs store doctor <store> [--machine <machine>]   # the Store, then each of its projects
layerfs project doctor <project>    # the project's record against its Store's own list
layerfs sandbox doctor <sandbox>    # the Sandbox: record, runtime, daemon
layerfs backup doctor <bucket> [--full]               # the bucket's record and reach; --full writes and reads one object
```

Each answers the same report. The short form is `{"ok":true}` alone when
every check was made and nothing was found; otherwise `findings` and
`not_checked`, each present only with entries:

```json
{"findings":[{"store":"bad","check":"host.records","severity":"error","message":"…","fix":"…"}],
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
  of its checks is tried; a stopped machine is checked by its records only.
  Each thing has its own time budget; a check still running when it ends is
  `not_checked` with `timeout`.
- `--progress` (`on_progress`) sends one event per finished thing:
  `{ phase, done, total, node }`, `phase` `self` then the kind of thing
  (`machines`, `stores`, `projects`, `sandboxes`, `buckets`). `backup doctor`
  sends none.
- `project doctor --full` is refused `Unsupported` (not built yet). `store
  doctor --machine` is for a PostgreSQL Store; a sqlite Store refuses it.
- `--text` prints a count line (`N error(s), N warning(s), N notice(s), N
  not checked`, or `ok` alone), then each finding, its `from` and its `fix`,
  then each `not_checked` entry.
- Exit 0 once the report is made, whatever it finds; 125 with `NotFound`
  when the thing is not in the home; 2 for a wrong command line. Leftovers
  of interrupted operations are in no doctor's report.

### `layerfs doctor fix <id>`

The one `doctor` command that changes something, and a call of the table
(`doctor.fix`; `layerfs.doctor.fix(id)` in Node and Python). It removes the
one leftover whose `id` it is given (`<machine>:<kind>:<name>`, never a
path), once, on that machine, under that kind's guard:

```bash
layerfs doctor fix native:image-part:<64 hex>
# {"kind":"image-part","path":"…/machine/native/run/images/.recv-<64 hex>.part","removed":true,"bytes":50,"machine":"native","id":"native:image-part:<64 hex>"}
```

| Kind | Removed when (else `InUse`, nothing removed) |
| --- | --- |
| `run-folder` | no Sandbox record names it, its `owner` marker is there and older than a minute, no daemon of it runs and nothing is mounted at or under it or its `.root` |
| `empty-root` | its run folder is gone and it is not a mount point; `rmdir` only |
| `init-copy` | a line of `log/uncertain.jsonl` names the machine and the copy, and its deadline (when the lost request went out, plus the Init's `timeoutMs`) passed more than an hour ago; a copy no line names is only reported (`InvalidInput`) |
| `backup-staging` | no `backup create` of that Store is running (its `backup.lock` is taken for the removal) |
| `image-tmp` | no unpack of that image is running (its lock is taken; the lock file stays) |
| `image-part` | it has not been written for an hour |

Other refusals: `NotFound` (no leftover has that id now), `InvalidInput`
(an image lock or an unmount marker, which are never removed), `Unavailable`
(the machine is stopped) and `Unknown` (the worker's answer was lost). The MCP server has no tool for it. Run on
macOS `native` against leftovers made by hand; not run on a VM machine for
this manual.

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
