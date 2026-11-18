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

### `sandbox` (5)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `create` | `project`, `name`, `machine` (optional) | `image` | starts a named Sandbox of a project on a machine; with none, its sqlite Store's machine, or `native` for a PostgreSQL Store |
| `list` | `[project]` | `page_size`, `page_pos`, `live` | every Sandbox in the home, or one project's; with `live`, whether each one's daemon process runs now |
| `get` | `name` | `page_size`, `page_pos` | a Sandbox and its Workspaces |
| `close` | `name` | `force` | stops a Sandbox |
| `files` | `sandbox`, `[path]` | `depth`, `cursor`, `limit`, `offset`, `length` (int) | a Sandbox's roots, one page of a folder, or one window of a file, in its home or (a Sandbox with a root of its own) its root |

- `create` answers the Sandbox's record: `name`, `project`, `machine`,
  `created`, `id` (the Sandbox's name, as its machine knows it),
  `platform`, `runtime`, `image`, `backend`, `route` (the kernel route,
  `fuse` or `nfs`) and `state`. A Sandbox name taken anywhere in the home
  is `Exists`. The Store's machine, a running `store push` or `store pull` and the name
  are checked under one home lock, where the create claims the name with
  its `op` intent (#536 §7.1); its success records the result and drops
  the `op` in one write, a refused start removes the record. A create
  whose outcome is not known (its machine's answer was lost) is `Unknown`
  and its record keeps the `op`, marked `uncertain`.
- Every Sandbox's `state` is derived, never stored: `creating` or
  `closing` while that call's process runs it, `unresolved` once a create
  or close lost its answer and its process ended, `gone` with no id (or
  when `get` hears from its machine that it no longer runs), else
  `running`. While a create or close runs, `close` and the `workspace`
  calls but `status` are `Busy`; an `unresolved` Sandbox refuses them
  `Unresolved`, naming `layerfs sandbox get <name>` and `layerfs sandbox
  close <name>`, the call that clears it.
- `lost` (layerfs-project#113): the record has an id but the Sandbox no
  longer runs. `get` reports it when the Sandbox's process tree was ended
  outside LayerFS (on `native`, its recorded daemon is gone); `list` reads
  only records and does not ask the machine. `sandbox close` clears it:
  the record goes and the name is free (`was_gone: true`).
- A sqlite project's Sandbox runs on the machine that holds its Store;
  another machine is `Unsupported`, naming the Store's machine.
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
  A Sandbox recorded under the retired docker backend is refused
  `Unsupported`, asking to recreate it; `sandbox close` forgets it and
  names its container for `docker rm`.
- `list` and `get` page: `list` its Sandboxes, `get` the Sandbox's
  `workspaces` (`page_size` 1 to 1000, default 50; `page_pos`), with
  `next_page_pos`, `null` after the last page. Both show each Sandbox's
  `state` and, while it has one, its `op` (`{id, name, phase, started,
  uncertain}`, H6); reading never clears it. `get` asks no daemon for a
  Sandbox with no id. Each Workspace carries `ws`
  (`<sandbox>:<name>` when named, else `<sandbox>:<path>`), `name` (or
  `null`) and `path`, beside the daemon's fields.
- `get` also answers `workspace_capacity`, how many Workspaces one Sandbox
  holds at once (16), whatever the Sandbox's state, and for each Workspace:
  - `running`: the commands running in it now, oldest first, whichever
    client started them: `[{exec_id, started, tag, program}]`. `started`
    is ms since the epoch; `tag` is the log tag of the `workspace exec`
    call that started it (`log_tag`, else `LAYERFS_LOGS_TAG`) or `null`;
    `program` is the command's first word that is not a `NAME=value`
    assignment, or `null`. No argument and no environment value is shown.
    A command that ended is not listed, nor is one whose client is gone
    (the next cancel, unmount or close ends that one). `null` when the
    command records could not be read.
  - `overlay_bytes`: the size of the Workspace's overlay file, read from
    its folder entry (the file is not opened). It includes the size of
    the empty overlay and only grows until the Workspace is unmounted.
    `null` when it could not be read. For a Sandbox on a `lima` machine
    whose daemon this host reaches directly, the sizes are read by that
    machine's worker, and are `null` when it does not answer.
  - `usage`: what the Workspace's commands have used, in the shape of the
    Sandbox's `usage` below, from the Workspace's own cgroup in a
    `namespaces` Sandbox (the Sandbox's own processes are in a leaf group,
    so its Workspace groups get the memory and pids controllers it has).
    `null` with `usage_reason` `no_command` when no command runs in the
    Workspace, and with the Sandbox's own reason when none of its
    Workspaces can have a cgroup; a Sandbox an earlier build started keeps
    only `cpu_usec` per Workspace (`no_controller`).
  They come from files, with no daemon call. A machine whose worker runs
  an earlier build answers Workspaces without these fields.
  ```text
  {"name":"s1","project":"shop/api","machine":"native",…,"state":"running","workspaces":[{"workspace":"5f…","namespace":1,"branch":"dev",…,"path":"/…/workspaces/1","name":"w","ws":"s1:w","running":[{"exec_id":"exec-9c0f6f0d2a1b4e77","started":1794000000000,"tag":"dsh:s9:t1","program":"npm"}],"overlay_bytes":188416}],"daemon_capacity":{…},"workspace_capacity":16,"next_page_pos":null}
  ```
- `get` also answers the Sandbox's daemon process and what its cgroup has
  used (#164), read with `overlay_bytes` on the side that runs the Sandbox,
  with no daemon call and no further round trip:
  - `daemon`: `{pid, started_at}` of the process the Sandbox's record
    names (the daemon of a `native` Sandbox; the supervisor a
    `namespaces` Sandbox's daemon runs under), with `started_at` in ms
    since the epoch, to about a second, while the process runs and
    `null` once it has ended; `null`
    when its machine did not answer.
  - `usage`: `{cpu_usec, memory_current, memory_peak, memory_file, pids,
    oom_kills}` of the Sandbox's own cgroup, the daemon and every command
    in it (`memory_file`: file cache and tmpfs, inside `memory_current`;
    `oom_kills`: out-of-memory kills in it and its Workspace and command
    groups; #182); a value the
    kernel does not keep there is `null`, and when that is the memory
    or the process count `usage_reason` is `no_controller` (where the
    machine gives Sandboxes no memory or pids controller; docker, Lima and
    Apple Container machines of this build give both). `usage` is
    `null`, with `usage_reason`, for a `native` Sandbox, which is a plain process
    (`host_process`), for a runtime this build reads no usage of
    (`not_supported`), on a machine without cgroup v2 (`cgroup_v1`), when
    the cgroup is not there (`no_cgroup`) and when the machine did not
    answer (`not_observed`).
  ```text
  {"name":"s1",…,"state":"running","workspaces":[],"daemon_capacity":{…},"workspace_capacity":16,"daemon":{"pid":37823,"started_at":1794776690403},"usage":null,"usage_reason":"host_process","next_page_pos":null}
  ```
- `list --live` adds `live` to each Sandbox of the page (#164): `true`
  when the process its daemon record names still runs, `false` when it has
  ended, `null` with `live_reason` when that is not known
  (`machine_stopped`, `deadline`, or the refusal its machine's request
  met, as `unavailable`). `state` is unchanged: it comes from records, so
  a Sandbox whose daemon died reads `state: "running"`, `live: false`
  until `get` or `close`. Each machine among the page's Sandboxes is asked
  once; a machine that does not answer fails no list. Without `--live`
  the answer has no such field.
  ```text
  $ layerfs sandbox list --live
  {"items":[{"name":"s1","project":"shop/api","machine":"native",…,"state":"running","live":true}],"next_page_pos":null}
  ```
- `list` and `get` start no machine (#535 P4). `get` on a Sandbox whose
  machine is down answers its record with `workspaces: []` and
  `down_reason`, the reason its worker was not reached.
- `close` records its `op` (`sandbox.close`) before it stops the
  Sandbox and removes the record at the end; a lost answer (`Unknown`)
  keeps the record with the `op` uncertain until another `close`, and any
  other refusal leaves the Sandbox as it was.
- `close` without `force` is `Busy` while a command runs in one of its
  Workspaces (`detail.execs` names their exec ids), and `MountsActive`
  while a Workspace is mounted. With `force`, each running command is
  cancelled first, as `workspace exec-cancel` does with its default grace;
  a cancel that is not confirmed refuses the close `Unknown` and nothing
  more is done.
  With `force` (O31) it stops the Sandbox through its backend anyway and
  answers `{kind: "Closed", sandbox, unknown}`: `unknown` lists every
  Workspace still mounted, since a forced close drains none of them; their
  uncommitted changes and processes end with the Sandbox. Closing a Sandbox that is already gone answers
  `{closed, was_gone: true}`; a gone native Sandbox's stopped runtime and
  local directory are deleted by that close.
- A record with no id yet (a create in flight, one whose outcome was
  unknown, or one whose process ended) is closed too: `Busy` while the
  creating process lives, else the record goes and the name is free
  (`{closed, was}`, `was` being `gone` or `unresolved`; for
  `unresolved`, `left` says a Sandbox may remain on its machine). A Sandbox the layout 3 conversion found ended is closed this
  way too, with `was: "gone"` ([the home](home.md#a-layout-2-home)).
- Not run for this manual. See
  [limitations](limitations.md#not-built-refused-as-unsupported).

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
  and logs it to `log/exec.jsonl`; `layerfs doctor` reports it. On a
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

### `machine` (15)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `list` | — | — | every machine, with what it supports (`supports`: `sqlite`, `postgresql_sandbox`, `reason`, and `log`, whether it has a worker whose log can be read; never on `native`) |
| `platforms` | — | — | the platforms this host and build support: backend, kernel route, where Store calls run, whether a PostgreSQL Sandbox runs and why not, readiness and options (#535; the rows of `control/SUPPORT.json`) |
| `get` | `name` | — | a machine's record |
| `images` | `name` | — | the Sandbox images the machine holds (`images`: `id`, `refs`, `size`, `architecture`, `source` `bundle`/`machine`/`imported`, `sandboxes`, `last_used`), its `default`, `free` bytes and `architecture`; and the host's tagged Docker images it could copy (`docker`, `available`: `id`, `refs`, `size`, `os`, `architecture`, `usable`, `reason`, `need`, `fits`). macOS `native` answers `mode: "host"` with none. Never starts a machine |
| `load` | `name`, `image` | `on_progress` | copies an image to the machine as `sandbox create --image` does, without a Sandbox: `{id, refs, size, reused}` |
| `prune` | `name`, `image`? | `unused` | removes one unpacked image, or with `unused` every one no Sandbox uses but the machine's default: `{removed, freed}` (`freed` measured on the filesystem); an image in use is `InUse` |
| `status` | `name` | — | a machine's live state, free disk and tool version; `op` while a create or remove holds it (#536 H6) |
| `stats` | `machine` | — | what a machine has and uses now: processors, memory and data disk, each sqlite Store's size, how many of its Sandboxes have a running daemon (#164). For a UI that polls: it writes no call-log line |
| `space` | `machine` | — | where a machine's data disk goes: each Store, each Sandbox's run folder, the image cache and LayerFS's logs, and what the disk uses that is not LayerFS's. For a UI: it writes no call-log line |
| `files` | `machine`, `[path]` | `depth`, `cursor`, `limit`, `offset`, `length` (int) | a machine's LayerFS folders by role, one page of a folder, or one window of a file (#178). For a UI that shows a tree: it writes no call-log line |
| `start` | `name` | — | starts a stopped machine and its worker and waits until ready; a running one, and `native`, answer at once (#535 P1) |
| `stop` | `name` | — | stops a machine's worker and, where it has one, its VM; disks stay; `InUse` while Sandboxes run on it (#535 P1) |
| `create` | `name`, `platform` | `cpus`, `memory` (size), `disk` (size), `image` | creates an enabled VM machine (`apple-container`, `lima`) |
| `remove` | `name` | — | deletes a VM machine and its disk; one of a removed platform (`firecracker`, `ssh`, `microsandbox`) is only forgotten |
| `update` | `name` | `cpus`, `memory`, `image`, `docker`, `wsl`, `daemon` | sets a built-in machine's image, Docker socket, WSL distribution and daemon (an absolute path to a file; it replaces what `import-env` recorded) (O11); `InUse` while Sandboxes run on the machine; resizing a VM is not built |

Run:

```console
$ layerfs machine list
{"machines":[{"name":"docker","platform":"docker","backend":"namespaces","state":"ready","cpus":null,"memory":null,"disk":null,"supports":{"sqlite":true,"postgresql_sandbox":true,"reason":null,"log":true},"stores":[],"sandboxes":[]},{"name":"native","platform":"native","backend":"native","state":"ready",…,"stores":["shop"],"sandboxes":[]}]}

$ layerfs machine platforms
{"host":"macos","platforms":[{"platform":"native","how":"built_in","backend":"native","kernel":"nfs","sqlite":"in_process","postgresql":{"store_calls":"in_process","sandbox":true,"reason":null},"ready":true,"missing":[],"version":null,"options":[],"description":"…"},…,{"platform":"lima","how":"created","backend":"namespaces","kernel":"fuse","sqlite":"worker","postgresql":{"store_calls":"in_process","sandbox":true,"reason":null},"ready":true,"missing":[],"version":"limactl version 2.2.1","options":[{"name":"cpus","kind":"int"},…],"description":"…"},…]}

$ layerfs machine status native
{"name":"native","platform":"native","state":"running","free_disk":null,"tool_version":null,"down_reason":null,"stores":["shop"]}
```

- `stats` answers `{machine, platform, observed_at, worker, resources,
  data_disk, stores, sandboxes, complete}` (#164, #182). It answers the
  present only; a client keeps its own history:
  - `worker`: the machine's worker process, `{version, pid, started_at,
    protocol, bundle}`; `null` on `native`, whose calls run in the caller.
  - `resources`: `{cpus, memory_total, memory_used, disk_total,
    disk_used, disk_free, cpu_busy_usec, cpu_total_usec, scope}`, sizes in
    bytes, observed now. `cpu_busy_usec` and `cpu_total_usec` are the
    machine's processor time since boot in microseconds (`/proc/stat`;
    steal counts as busy); the processor share between two reads is
    Δbusy / Δtotal. They are `null` on macOS (`cpu_unsupported`). `scope`
    says whose processors and memory they are: `host` (`native`), `vm`
    (`lima`, `apple-container`) or `engine_host` (`docker`: the Docker
    Engine's kernel, a VM on macOS and Windows). A value that was not read
    is `null` and `reason` names it: `memory_not_read` (`memory_*`),
    `disk_not_read` (`disk_*`), `cpu_unsupported` or `cpu_not_read`
    (`cpu_*`), joined by `, `.
  - `data_disk`: `{path, fs, scope, volume, inodes_free}`, the filesystem
    the `disk_*` values measure: `host_fs` (`native`, and `docker` on
    Linux), `vm_disk` (`docker` on macOS and Windows, one disk every
    image and volume shares) or `volume` (a VM machine's own disk), with
    the provider's name of it.
  - `stores`: `[{name, bytes, allocated}]` of the machine's sqlite
    Stores, the length and the blocks of `store.sqlite` and its `-wal`.
    The files are not opened.
  - `sandboxes`: `{recorded, live, items}`: the Sandboxes recorded on the
    machine, how many have a running daemon, and one row per recorded
    Sandbox, `{name, live, live_reason?, started_at, usage,
    usage_reason?}`, with `usage` as `sandbox get`'s. A row the 4 s budget
    did not reach is kept, `null` with `deadline`. A worker of an older
    build answers no `items`.
  - `complete`: `false`, with `reason: "deadline"`, when the Stores or
    Sandboxes were not all read within 4 s; the rest is answered.
  It starts nothing and takes no lock: a stopped machine is `Unavailable`
  and a machine an operation holds is still answered.
  ```text
  $ layerfs machine stats native
  {"machine":"native","platform":"native","observed_at":1794776707857,"worker":null,"resources":{"cpus":14,"memory_total":38654705664,"memory_used":31340068864,"disk_total":994662584320,"disk_used":641705943040,"disk_free":352956641280,"cpu_busy_usec":null,"cpu_total_usec":null,"scope":"host","reason":"cpu_unsupported"},"data_disk":{"path":"/Users/me/.layerfs","fs":"apfs","scope":"host_fs","volume":null,"inodes_free":3446842200},"stores":[{"name":"shop","bytes":217088,"allocated":217088}],"sandboxes":{"recorded":1,"live":1,"items":[{"name":"web","live":true,"started_at":1794776100211,"usage":null,"usage_reason":"host_process"}]},"complete":true}
  ```
- `space` answers `{machine, platform, observed_at, disk, layerfs,
  outside, parts, stores, sandboxes, unreadable, complete}`: one bounded
  walk of LayerFS's own folders where the machine's data is, every size the
  blocks it takes (a file with several names once).
  - `disk`: `{total, used, free, path, fs, scope}`, the filesystem `stats`
    measures. `layerfs` is the sum of `parts`, and `outside` is `used` less
    `layerfs`: what the disk holds that is not LayerFS's (on `vm_disk`,
    Docker's images, build cache and other volumes). It is `null` unless
    the walk finished.
  - `parts`: `{stores, sandboxes, images, other_runs, logs, other}`: the
    Store folders, the Sandboxes' run folders, the image cache, run folders
    no recorded Sandbox names, `log/` and `trace/`, and the records.
  - `stores`: `[{name, bytes}]`; `sandboxes`: one row per recorded Sandbox,
    `{name, bytes, overlays, writes, home, log, runtime}`: its Workspaces'
    overlay databases, its own writable layer (what its commands wrote
    outside Workspaces), its `HOME`, its daemon log and its copies of the
    daemon and runtime. A row whose run folder was not measured has `null`
    sizes and `reason` (`no_run_folder`, `outside_run_root`,
    `record_unreadable`, `deadline`); a row the bound ended inside keeps
    what it counted, with `deadline`.
  - `complete`: `false`, with `reason: "deadline"`, when the walk stopped at
    250,000 entries or 4 s; every number is then a lower bound. `unreadable`
    counts folders it could not list (`reason` adds `unreadable`).
  A Workspace mount point and a private root are never entered or
  `stat`ed, no file is opened and the home itself is never listed. It
  starts nothing and takes no lock.
  ```text
  $ layerfs machine space native
  {"machine":"native","platform":"native","observed_at":1794817324800,"disk":{"total":994662584320,"used":677787914240,"free":316874670080,"path":"/Users/me/.layerfs","fs":"apfs","scope":"host_fs"},"layerfs":14168064,"outside":677773746176,"parts":{"stores":2277376,"sandboxes":11833344,"images":0,"other_runs":0,"logs":8192,"other":49152},"stores":[{"name":"shop","bytes":2277376}],"sandboxes":[{"name":"web","bytes":11833344,"overlays":8675328,"writes":0,"home":3145728,"log":4096,"runtime":8192}],"unreadable":0,"complete":true}
  ```
- `files` reads what is on a machine (#178); its options, pages and file
  windows are those of [`sandbox files`](#sandbox-files), and the whole call
  is in the
  [architecture guide](../../architecture/api/sdk/machine.md#machinefiles).
  - With no path it answers the machine's roots, each a folder with a
    `role`, in the order a client shows them: `data` (the worker's root
    `/layerfs/data`, with `free_bytes`), `stores`, `sandbox_records`,
    `run_folders`, `images`, `staging`, `logs`, then `state`, `bundle`,
    `image`, and last `/` with role `machine`. Only folders that are there
    are listed. On `native` the roots are the home's `store`, `sandbox`,
    `machine/native/run` and `log`, and there is no `/`: a path outside
    them is `InvalidInput`.
  - In a LayerFS folder no link is followed and no mount entered, and what
    LayerFS keeps private is `restricted`: listed with its size, never
    read (`store.sqlite*`, a run folder's `config/` and `overlay/`,
    `control.json`, `worker.json`, locks and leases, the worker's
    `secret`). A run folder's `workspaces/` entries are `mount`, with the
    Workspace's id, and are never touched.
  - From `/`, `/proc`, `/sys`, `/dev` and `/run` are `system`; a mount is
    entered only when it is a local disk or memory (`ext4`, `xfs`, `btrfs`,
    `zfs`, `f2fs`, `overlay`, `tmpfs`), else it is `mount` with its `fs`;
    and only what every user of the machine may read is opened, the rest
    is `unreadable` (`/etc/shadow`, `/root`). A link on the way resolves
    inside the machine (`/lib/...` opens on a merged-`/usr` system).
  ```text
  $ layerfs machine files native
  {"kind":"folder","entries":[{"path":"/Users/me/.layerfs/store","depth":1,"kind":"folder","size":null,"mode":493,"mtime":1794781376540,"role":"stores"},{"path":"/Users/me/.layerfs/sandbox","depth":1,"kind":"folder","size":null,"mode":493,"mtime":1794781376540,"role":"sandbox_records"},{"path":"/Users/me/.layerfs/machine/native/run","depth":1,"kind":"folder","size":null,"mode":493,"mtime":1794781376540,"role":"run_folders"},{"path":"/Users/me/.layerfs/log","depth":1,"kind":"folder","size":null,"mode":493,"mtime":1794781376540,"role":"logs"}],"next":null,"has_more":false}
  ```
- `start` and `stop` answer the machine's summary as `get` does. They are
  the only lifecycle calls (#535 D11): no other call starts a machine, and
  nothing stops one when it is unused. `stop` is refused `InUse` while
  Sandboxes are recorded on the machine; `detail.related` lists them as `{kind: "sandbox", name}`. It
  runs under the machine's `start.lock`, so no `machine start` runs
  meanwhile, and removes `worker.json`; a later call that needs the machine
  is refused `Unavailable` until `machine start`.

- A record of a removed platform (`firecracker`, `ssh`, `microsandbox`) still shows in
  `list` and `get` with `state: "unsupported"` and the reason; `remove`
  forgets it and touches nothing outside the home. Every other call on it
  is `Unsupported`. See
  [removed platforms](machines-and-stores.md#removed-platforms-firecracker-microsandbox-and-ssh).

See [machines](machines-and-stores.md#machines) for kinds, defaults and
refusals.

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

### `logs` (2)

Two reads for a UI that shows a machine or Sandbox log and follows it
(layerfs-project#178), in the
[architecture guide](../../architecture/api/sdk/logs.md). The home's own
call log is [`home.log` and `home.clear_log`](#home-4).

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `machine` | `machine` | `cursor`, `before`, `limit` (int) | one window of a machine's worker log |
| `sandbox` | `sandbox` | `cursor`, `before`, `limit` (int) | one window of a Sandbox's daemon log |

Each answers `{ lines: [{ text, continued? }], cursor, before?, has_more }`:
with no option the last `limit` lines (200 by default, 1000 at most), with
`cursor` the lines written after it, with `before` the lines before it.
Control characters in `text` are escaped (`\u{1b}`). They take no lock,
start nothing, write nothing and add no line to the call log; at most two
run on a machine at a time (`Busy`), and a machine that is stopped or does
not answer within 8 s is `Unavailable`. `logs machine` on a machine with no
worker (`native`) is `Unsupported`. The MCP server lists neither. Run for
this manual on a scratch home: `layerfs logs sandbox s1`, then `--cursor`.

### `sandbox`: `files`

One more read for a UI: what is in a Sandbox (layerfs-project#178). It is
in the `sandbox` family, after `close`; the whole call is in the
[architecture guide](../../architecture/api/sdk/sandbox.md#sandboxfiles).

```bash
layerfs sandbox files s1                       # its roots: Workspaces, home, log, and / where it has one
layerfs sandbox files s1 "$HOME_PATH" --depth 2 --limit 100
layerfs sandbox files s1 "$HOME_PATH/notes.txt" --offset 0 --length 4096
```

- With no path it answers the Sandbox's roots, each with a `role`: one
  `workspace` per mounted Workspace (kind `mount`, with the Workspace's id
  in `workspace`; taken from the Workspace records and never opened),
  `home` (the folder that is `HOME` for its commands), `log` (its
  daemon's log, which `logs sandbox` reads) and, for a Sandbox with a root
  of its own (every one but a native macOS Sandbox), `/` with role `root`.
  ```text
  {"kind":"folder","entries":[{"path":"/…/workspaces/1","depth":1,"kind":"mount","size":null,"mode":null,"mtime":null,"role":"workspace","workspace":"5f…"},{"path":"/…/tmp","depth":1,"kind":"folder","size":null,"mode":493,"mtime":1794000000000,"role":"home"},{"path":"/…/daemon.log","depth":1,"kind":"file","size":10,"mode":420,"mtime":1794000000000,"role":"log"}],"next":null,"has_more":false}
  ```
- With the path of a folder it answers one page of its entries: names in
  byte order, a folder's entries after it, down `depth` levels (1 by
  default, 32 at most), at most `limit` entries (500 by default, 1000 at
  most). `next` is the path of the page's last entry; while `has_more`,
  pass it as `cursor` for the entries after it. `reason` says why a page
  ended early: `limit`, or `deadline` when the read used its four seconds.
  An entry's `kind` is `file`, `folder`, `symlink` (with its `target`,
  never followed), `mount`, `restricted` or `other`; `size` (regular files
  only), `mode` (permission bits) and `mtime` (milliseconds) come from the
  entry's own `stat`.
- With the path of a file it answers one window: `{kind: "file", size,
  offset, content, binary, next}`, 256 KiB from `offset` by default,
  `length` at most 1 MiB. `content` is text: bytes that are not UTF-8 are
  U+FFFD and control characters other than line breaks and tabs are escaped
  (`\u{1b}`). `binary` is true when the window holds a NUL byte; `content`
  is then empty. `next` is the next window's offset, `null` at the end.
- A path is absolute, as the Sandbox's commands see it. Under the
  Sandbox's home no link is followed and no mount entered. Refused
  `InvalidInput` there: a path with `..`, a symbolic link on the way or at
  the end, a FIFO, a socket, a device, a mount point, a file LayerFS keeps
  private (`store.sqlite*`) and a file with several names that the
  Sandbox's commands do not own. A path that is not there is `NotFound`.
- A path outside the home is read in the Sandbox's root, when its roots
  offer `/`: what its commands see, read through the root of the Sandbox's
  init process on its machine. A link on the way resolves inside the root
  (`/lib/...` opens in a merged-`/usr` image) and the last name is never
  followed; `/proc`, `/sys` and `/dev` are `system`; a mount is entered
  only when it is a local disk or memory, so a Workspace mount is `mount`
  (with the Workspace's id) and is never entered; the run folder's
  `config/` and `overlay/` and a bound Store's `store.sqlite` are
  `restricted`, by their own path or through a link. A Sandbox that has
  stopped answers `Unavailable` for its root and still reads its home. A
  kernel without `openat2`, or a reader that is not root (a native Linux
  home), answers `Unsupported` with its reason. A Sandbox with no root of
  its own (native macOS) refuses a path outside its home `InvalidInput`.
- It takes no lock, asks no daemon, starts nothing and writes nothing, the
  call log included; a Sandbox whose daemon has ended keeps its home
  readable. At most two inspection reads run on a machine at a time
  (`Busy`); a machine that is stopped or does not answer within 8 s is
  `Unavailable`. The MCP server does not list it. Run for this manual on a
  scratch home: the three commands above.

## `layerfs doctor`

A top-level command beside `import-env`, outside the call table (#536 H7);
Node and Python have it as `layerfs.doctor()` ([SDK](sdk.md)). It reports
what is wrong in the home and changes nothing: it takes no lock, writes no
record, removes no stale lease and starts no machine.

```bash
layerfs doctor            # {"ok":false,"findings":[{"severity":"error","entity":{"kind":"store","name":"app"},"message":"…","fix":"…"}]}
layerfs doctor --text     # a count line, then each finding and its fix
```

- `ok` is false when any finding is an `error`; a `warning` is left over or
  out of date and refuses nothing. Each finding's `fix` names the call (or
  the by-hand step, where no call can) that fixes it.
- It checks: records that do not parse, or whose `"v"` is not 1; folder
  modes other than 0700 (0701 where a namespaces Sandbox passes through, but
  never a Store's own folder, which stays 0700) and
  record modes other than 0600; broken references (a Sandbox whose Store,
  project or machine is gone, a Store on a machine with no record, a project
  in a Store folder with no record, a built-in machine whose recorded daemon is not there, an error
  naming `machine update <name> --daemon <path>`); an `op` a gone process left uncertain (an error naming the call
  that clears it) or certain (a warning); stale Store leases (a warning: the
  Store's next call removes them); what refuses
  a layout 2 home's conversion; a partial `store.sqlite.incoming` on
  `native`; a copy a push or pull did not retire (its record's
  `last_transfer` says `retired: false`; a warning naming the next `store
  pull` or `store push`, which deletes it first); command records in this
  home (a stale one, whose owner is gone, is a warning naming `workspace
  exec-cancel`, the unmount or the close that ends it; one that does not
  read is an error; a Workspace folder with no Workspace record beside it is
  a warning); and each running machine's worker build (its hello alone, no
  request; another build is a warning naming `machine stop` then `machine
  start`, a worker that does not answer one naming `machine start`).
- Not checked: partial copies and leftover layout 2 folders on other
  machines (no worker operation reports them).
- It also lists what interrupted operations left on disk, on `native` and
  on each running machine (one bounded walk each: 20,000 entries or 2 s; a
  stopped machine is not asked). Each is a warning on its machine whose
  finding also holds `leftover: { machine, kind, path, bytes, age, id,
  fixable }` (`age` in seconds since it last changed; `bytes` null when the
  walk's bound ended first), and the report's `leftovers` has one row per
  machine walked: `{ machine, complete, image_locks, unmount_markers }`. The
  kinds: `run-folder` (a Sandbox run folder no Sandbox record names),
  `empty-root` (an empty `<name>.root` whose run folder is gone),
  `init-copy` (a `project init` folder copy, `incoming/init-*`),
  `backup-staging` (`backup.staging`, `store.sqlite.backup`), `image-tmp`
  and `image-part` (an interrupted image unpack or load). Image lock files
  and unmount markers are only counted: LayerFS keeps them.
- Exit 0 once the report is made, whatever it finds; 2 for any argument
  but `--text`; 125 when the home cannot be named (`LAYERFS_HOME` not
  absolute).

### `layerfs doctor fix <id>`

The one `doctor` command that changes something, and a call of the table
(`doctor.fix`; `layerfs.doctor.fix(id)` in Node and Python). It removes the
one leftover whose `id` the report gave (`<machine>:<kind>:<name>`, never a
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
(the machine is stopped) and `Unknown` (the worker's answer was lost: run
`layerfs doctor` again to see). The MCP server has no tool for it. Run on
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
