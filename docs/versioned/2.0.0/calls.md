# The calls

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](README.md). The source is the one call table,
[`engine/crates/layerfs-api/src/calls/table.rs`](../../../engine/crates/layerfs-api/src/calls/table.rs),
and the command in `engine/crates/layerfs-cli/src/`. The shell parser, the
help, and the Node and Python functions are all built from that table, so
the three front ends take the same arguments.

There are 61 calls in nine families. `layerfs help` lists them, and
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

## The 61 calls

`[x]` is an optional positional. Every family's results are snake_case.

### `project` (4)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `init` | `project`, `folder` | `timeout` (ms, default 1 hour), `on_progress` | imports a host folder as a new project into an existing Store |
| `list` | `[store]` | `page_size`, `page_pos` | every project in the home, or one Store's |
| `get` | `project` | — | a project's record: its Store, kind and machine |
| `remove` | `project` | — | removes one project from its Store |

- `init` never creates a Store; a missing one is `NotFound`. Into a sqlite
  Store on `native` or a PostgreSQL Store it runs in the caller's process
  and reads `folder` on this host; into a sqlite Store on another machine
  it runs in that machine's worker and reads `folder` on that machine
  (#536 H11), which must be running. Run: answers `project`, `store`, `name`,
  `kind`, `machine`, `created`, `layer` and `root`.
- `remove` is refused while a Sandbox of the project runs, and is
  `Unsupported` for a PostgreSQL Store.

### `branch` (6)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `list` | `project` | `page_size`, `page_pos` | a project's Branches |
| `get` | `project`, `branch` | — | one Branch and its head Commit |
| `fork` | `project`, `name`, `from_branch`, `[from_commit]` | — | a new Branch from another Branch's head or Commit |
| `merge` | `project`, `name`, `a`, `b` | `message`, `units` (JSON) | merges two Branches into a new one |
| `publish` | `project`, `branch`, `[commit]` | — | a Branch's head or Commit as `main`'s next Layer |
| `diff` | `project`, `from_branch`, `to_branch` | `paths_only`, `page_size`, `page_pos`, `cursor` | files that differ between two Branches' heads |

Run: `fork` answers `{kind: "Forked", branch, base, head}`; `publish`
answers `{kind: "Added", source, published_layer, based_on, overwrote_count,
overwrote, overwrote_has_more}`; `merge` answers `{kind: "Merged",
fast_forward, branch, head, root, sources, merge_base, conflicts, lww,
units, …}`.

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

### `file` (14)

1.1.0's `fs` family, renamed (O13). Paths are absolute, starting with `/`.

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `read` | `project`, `branch`, `path`, `[commit]` | `start_line`, `end_line` | reads a text file, or a window of its lines |
| `stat` | `project`, `branch`, `path`, `[commit]` | — | a path's kind, size and mode |
| `list` | `project`, `branch`, `path`, `[commit]` | `page_size`, `page_pos` | a folder's entries |
| `glob` | `project`, `branch`, `path`, `pattern`, `[commit]` | `page_size`, `page_pos` | paths under `path` matching a glob |
| `grep` | `project`, `branch`, `path`, `pattern`, `[commit]` | `page_size`, `page_pos` | lines under `path` matching a regular expression |
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
  A file that is not text is refused `Binary`.
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

### `sandbox` (4)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `create` | `project`, `name`, `machine` (optional) | `image` | starts a named Sandbox of a project on a machine; with none, its sqlite Store's machine, or `native` for a PostgreSQL Store |
| `list` | `[project]` | `page_size`, `page_pos` | every Sandbox in the home, or one project's |
| `get` | `name` | `page_size`, `page_pos` | a Sandbox and its Workspaces |
| `close` | `name` | `force` | stops a Sandbox |

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
- A sqlite project's Sandbox runs on the machine that holds its Store;
  another machine is `Unsupported`, naming the Store's machine.
- On `docker`, the image is `image`, else the one `machine update docker
  --image` recorded, else the sealed bundle's `sandbox-image.tar`, loaded
  into Docker once and recorded on the docker machine as `loaded_image`
  (`{id, bundle}`; another bundle's image is loaded again). The docker
  worker's own container runs the same default. A bundle without the
  archive is `Unsupported`, naming it (#535 P5).
- `list` and `get` page: `list` its Sandboxes, `get` the Sandbox's
  `workspaces` (`page_size` 1 to 1000, default 50; `page_pos`), with
  `next_page_pos`, `null` after the last page. Both show each Sandbox's
  `state` and, while it has one, its `op` (`{id, name, phase, started,
  uncertain}`, H6); reading never clears it. `get` asks no daemon for a
  Sandbox with no id. Each Workspace carries `ws`
  (`<sandbox>:<name>` when named, else `<sandbox>:<path>`), `name` (or
  `null`) and `path`, beside the daemon's fields.
- `list` and `get` start no machine (#535 P4). `get` on a Sandbox whose
  machine is down answers its record with `workspaces: []` and
  `down_reason`, the reason its worker was not reached.
- `close` records its `op` (`sandbox.close`) before it stops the
  Sandbox and removes the record at the end; a lost answer (`Unknown`)
  keeps the record with the `op` uncertain until another `close`, and any
  other refusal leaves the Sandbox as it was.
- `close` without `force` is `MountsActive` while a Workspace is mounted.
  With `force` (O31) it stops the Sandbox through its backend anyway and
  answers `{kind: "Closed", sandbox, unknown}`: `unknown` lists the
  Workspaces it could not drain, whose uncommitted changes and processes
  end with the Sandbox. Closing a Sandbox that is already gone answers
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

### `workspace` (5)

A Workspace is `<sandbox>:<path>`, or `<sandbox>:<name>` when its mount
named it, answered as `ws` by `mount`. Every `workspace` call takes either
form.

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `mount` | `sandbox`, `branch` | `name` | mounts a Branch in a Sandbox |
| `exec` | `ws`, `command` | `message`, `stream`, `stdout`, `stderr` | runs one bash command in a Workspace |
| `commit` | `ws` | `message`, `force` | commits a Workspace's changes to its Branch |
| `unmount` | `ws` | `discard`, `force` | ends a Workspace |
| `status` | `ws` | — | a Workspace's changes since its base (O14) |

- `mount` with `name` names the Workspace (a name, as for Sandboxes);
  `ws` is then `<sandbox>:<name>`. A name is unique within its Sandbox: one
  in use is `Exists`. It is free again once that Workspace is unmounted.
  `mount` answers `name` (or `null`) beside `path`. A name no mounted
  Workspace carries is `NotMounted`.
- `exec` answers `kind` (`exit:<code>`), `exit_code`, `stdout`, `stderr`,
  `message` and `truncated`. A command that exits non-zero is still exit 0
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
- `commit` merges first when the Branch moved since the mount; `force`
  publishes over a moved head instead.
- `unmount` with `discard` drops uncommitted changes; `force` aborts its
  connection.
- `status` starts no machine (#535 P4): on a Sandbox whose machine is down
  it is `Unavailable`, naming `machine start`. `mount`, `exec`, `commit`
  and `unmount` on a stopped machine are refused the same way, as Store
  calls are; nothing starts a machine but `machine start` (#535 D11).
- On `ssh`, `lima` and `docker`, `mount`, `commit`, `unmount` and
  `status` go from the caller straight to the Sandbox's daemon (#535 G4);
  `exec` goes through the machine's worker. See machines-and-stores.md,
  Routes.
- Not run for this manual.

### `machine` (11)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `list` | — | — | every machine, with what it supports (`supports`) |
| `platforms` | — | — | the platforms this host and build support: backend, kernel route, where Store calls run, whether a PostgreSQL Sandbox runs and why not, readiness and options (#535; the rows of `connect/SUPPORT.json`) |
| `get` | `name` | — | a machine's record |
| `status` | `name` | — | a machine's live state, free disk and tool version; `op` while a create, connect, remove or disconnect holds it (#536 H6) |
| `start` | `name` | — | starts a stopped machine and its worker and waits until ready; a running one, and `native`, answer at once (#535 P1) |
| `stop` | `name` | — | stops a machine's worker and, where it has one, its VM; disks stay; `InUse` while Sandboxes run on it (#535 P1) |
| `create` | `name`, `platform` | `cpus`, `memory` (size), `disk` (size), `image`, `nested`, `host` | creates a VM machine |
| `connect` | `name`, `host` | `user`, `port`, `identity`, `known_hosts`, `root` | connects an ssh host as a machine |
| `remove` | `name` | — | deletes a VM machine and its disk |
| `disconnect` | `name` | — | forgets an ssh machine; its data root stays |
| `update` | `name` | `cpus`, `memory`, `image`, `docker`, `wsl` | sets a built-in machine's image, Docker socket and WSL distribution (O11); `InUse` while Sandboxes run on the machine; resizing a VM is not built |

Run:

```console
$ layerfs machine list
{"machines":[{"name":"docker","platform":"docker","backend":"docker","state":"ready","cpus":null,"memory":null,"disk":null,"supports":{"sqlite":true,"postgresql_sandbox":true,"reason":null},"stores":[],"sandboxes":[]},{"name":"native","platform":"native","backend":"native","state":"ready",…,"stores":["shop"],"sandboxes":[]}]}

$ layerfs machine platforms
{"host":"macos","platforms":[{"platform":"native","how":"built_in","backend":"native","kernel":"nfs","sqlite":"in_process","postgresql":{"store_calls":"in_process","sandbox":true,"reason":null},"ready":true,"missing":[],"version":null,"options":[],"description":"…"},…,{"platform":"lima","how":"created","backend":"namespaces","kernel":"fuse","sqlite":"worker","postgresql":{"store_calls":"in_process","sandbox":false,"reason":"lima host alias not verified (G7)"},"ready":true,"missing":[],"version":"limactl version 2.2.1","options":[{"name":"cpus","kind":"int"},…],"description":"…"},…]}

$ layerfs machine status native
{"name":"native","platform":"native","state":"running","free_disk":null,"tool_version":null,"down_reason":null,"stores":["shop"]}
```

- `start` and `stop` answer the machine's summary as `get` does. They are
  the only lifecycle calls (#535 D11): no other call starts a machine, and
  nothing stops one when it is unused. `stop` is refused `InUse` while
  Sandboxes are recorded on the machine (or on a Firecracker machine it
  hosts); `detail.related` lists them as `{kind: "sandbox", name}`. It
  runs under the machine's `start.lock`, so no `machine start` runs
  meanwhile, and removes `worker.json`; a later call that needs the machine
  is refused `Unavailable` until `machine start`.

See [machines](machines-and-stores.md#machines) for kinds, defaults and
refusals.

### `store` (9)

| Call | Positionals | Options | Does |
| --- | --- | --- | --- |
| `list` | — | — | every Store, from the home's records, each with its `state` |
| `get` | `name` | — | a Store's record |
| `status` | `name` | — | a Store's live state |
| `create` | `name` | — | an empty sqlite Store on `native` |
| `connect` | `name`, `url` | `password`, `synchronous_commit` | attaches a PostgreSQL database as a Store |
| `push` | `name`, `machine` | `on_progress` | sends a sqlite Store from `native` to a machine |
| `pull` | `name` | `on_progress` | brings a sqlite Store from its machine to `native` |
| `remove` | `name` | — | deletes a sqlite Store with every project in it |
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
not answer, is `down`, with `down_reason` and `file.unavailable` (O34). See
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
  modes other than 0700 (0701 where a namespaces Sandbox passes through) and
  record modes other than 0600; broken references (a Sandbox whose Store,
  project or machine is gone, a Store on a machine with no record, a project
  in a Store folder with no record, a Firecracker machine whose Lima host is
  gone); an `op` a gone process left uncertain (an error naming the call
  that clears it) or certain (a warning); stale Store leases; what refuses
  a layout 2 home's conversion; a partial `store.sqlite.incoming` on
  `native`; and each running machine's worker build (its hello alone, no
  request; another build is a warning naming `machine stop` then `machine
  start`, a worker that does not answer one naming `machine start`).
- Not checked: partial copies and leftover layout 2 folders on other
  machines (no worker operation reports them).
- Exit 0 once the report is made, whatever it finds; 2 for any argument
  but `--text`; 125 when the home cannot be named (`LAYERFS_HOME` not
  absolute).

## Results

- **stdout** carries the call's result as one line of JSON.
- **`--text`** prints a readable form instead: a file's text as it is; a
  result's list of records as a table with upper-case headers; other fields
  as `key  value` lines. It is one generic form, not a layout per call.
- Paged results carry `items`, `has_more` (most families) and
  `next_page_pos`; pass `next_page_pos` back as `page_pos`. The diff calls
  also carry `next_cursor`, passed back as `cursor`.
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

A refusal is `{"code", "message", "detail"}`. Run:

```console
$ layerfs file read shop/api
layerfs: file.read takes 3 to 4 positional arguments, not 1
$ echo $?
2

$ layerfs file read shop/api agent /nope
{"code":"NoSuchPath","message":"/nope does not exist at agent@71e889e68390","detail":{"path":"/nope"}}
$ echo $?
125
```

Codes seen in the 2.0 library include:

| Code | Meaning |
| --- | --- |
| `InvalidInput` | the request is wrong for this name or kind; nothing happened |
| `NotFound` | no machine, bucket or Store of that name, for a call that names it directly (`store get`, `machine get`, `backup create`, the Store of `project init`, …) |
| `NoSuchStore`, `NoSuchProject` | a call that names `<store>/<project>` whose Store or project is missing (1.1.0's codes, S §2.9) |
| `NoSuchBranch`, `NoSuchCommit` | no such Branch, or no such Commit on it (`branch.get`, `commit.get`, …) |
| `NoSuchPath`, `NoSuchSandbox`, `NoSuchProcess`, … | 1.1.0's codes for paths, Sandboxes and processes |
| `Exists` | the name is taken, the same host or database is already connected (under any name), or a bucket object was exported from another home |
| `InUse` | held: running Sandboxes, processes with the Store open, Stores on a machine; a layout 2 home with any of these, or with a move, removal or create still running, which is not converted yet ([the home](home.md#a-layout-2-home)) |
| `Busy` | an operation runs on the entity: a Store being pushed, pulled or removed, a machine being made, connected or removed, a Sandbox being created or closed; the message names it (`op` id and phase) and its process (#536 §7.1); `detail` is `{entity: {kind, name}, phase, op}`. Also: another project is being added to the same Store |
| `Unresolved` | an earlier operation on the entity ended with its outcome not known; its process is gone and its `op` stays. The message names the call that shows it (`store status`, `machine status`, `sandbox get`) and the one that clears it (`store remove`, `machine remove` or `disconnect`, `sandbox close`); `detail` as for `Busy` |
| `Unsupported` | not possible here, or not built (see [limitations](limitations.md)); a missing tool is listed in `detail.missing` |
| `Unavailable`, `Denied` | a server, bucket, worker or host cannot be reached, or refused the login or secret |
| `Conflict` | a merge or Commit conflict; for `backup.restore`, the Store changed since its last backup or restore |
| `Binary` | `file.read` of a file that is not text |
| `Capacity` | a Sandbox's Workspace limit; a `store push` or `store pull` target without room for the Store (`detail.needed`, `detail.free`) |
| `Failed` | it was tried and did not happen; whatever exists is kept |
| `Unknown` | the outcome is not known; both sides are kept and logged in `log/uncertain.jsonl`, and the entity keeps an uncertain `op` (later calls on it are `Unresolved`) |

Every call is one attempt. After `Unknown`, read the state with `list`,
`get` or `status` before acting again.

## Shell switches

| Switch | Calls | Does |
| --- | --- | --- |
| `--text` | all | a readable result instead of JSON |
| `--progress` | `project init`, `store push`, `store pull`, `backup create`, `backup restore` | progress as one JSON line per update on stderr, `{"progress": {…}}` |
| `--stream` | `workspace exec` | passes the command's stdout and stderr through (output over 1 MiB too) and exits with its code |
| `--from <file>` | `file write`, `file apply` | reads the content or JSON from a file instead of stdin |
| `--<option>-from <file>` | any optional text option (`--old-from`, `--new-from`, `--message-from`) | reads the option's value from a file |
| `-m <text>` | calls with `message` | the same as `--message` |
| `--scope <folder>` | all | a folder scope (see below); needs `--scope-project` |
| `--scope-project <store>/<project>` | all, with `--scope` | the one project the scope is bound to |
| `--scope-branch <name>` | all, with `--scope` | the scope's Branch namespace |
| `--` | all | every later word is a positional |

`--option=value` works for every option. `machine create … --nested` alone
is true; `--nested=false` is false.

`project init --progress` printed no lines for a small folder in the run
for this manual; `store push` and `store pull` report their phases
(`check`, `snapshot`, `send`, `verify`, `install`, `record`, `retire`) as
`{"progress": {"phase", "done", "total"}}`: `done` and `total` are bytes
during `send` and null in the other phases.

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
{"code":"InvalidInput","message":"out of scope: history","detail":null}
$ layerfs file list shop/api agent /src --scope /src --scope-project shop/api
{"code":"InvalidInput","message":"out of scope: branch","detail":null}
```
