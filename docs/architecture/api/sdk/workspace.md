# workspace family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `workspace` calls: list mounted Workspaces, mount a Branch as a writable tree in a running Sandbox, run and cancel commands in it, commit it, read its status and unmount it, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`crates/layerfs-api/src/families/workspace`](../../../../api/crates/layerfs-api/src/families/workspace) (one file per call), [`crates/layerfs-workspace/src`](../../../../control/crates/layerfs-workspace/src) (`calls.rs`, `answers.rs`, `exec.rs`, `list.rs`, `serve.rs`), [`layerfs-sandbox/src/exec`](../../../../control/crates/layerfs-sandbox/src/exec), [`daemon/refusal.rs`](../../../../control/crates/layerfs-sandbox/src/route/daemon/refusal.rs) |
| Runs by default | records in the caller's process; `mount`, `commit`, `unmount`, `status` reach the Sandbox's daemon (directly when its port is forwarded, else through the machine: this process or its worker); `exec` and `cancel` run in the Sandbox's runtime on its machine, never in the daemon. `list` reads Workspace records and asks no daemon |
| Design | [Sandboxes: daemon channel and exec](../../control/sandboxes/daemon-channel-and-exec.md), [Engine: daemon](../../engine/daemon/README.md), [Commit](../../engine/commit/README.md), [Workspace flow](../../flows/workspace.md), [Commit flow](../../flows/commit.md) |

## Calls

```text
 sandbox create (running Sandbox)      branch fork (a Branch, not main)
            \                              /
             mount <sandbox> <branch> --> workspace = <sandbox>:<name>
                |
   +---> exec <workspace> <command>   (bash -c in the Sandbox runtime)
   |        |   cancel <workspace> [--exec E]  (from any process)
   |     status <workspace>           (changes, Branch moved?, state)
   +---- commit <workspace>           (one Commit; merges a moved head)
            v
         unmount <workspace>          (Dirty unless committed or --discard)

 list [--sandbox S]                   (the handles, from records)
```

| Call | Short answer | MCP |
| --- | --- | --- |
| [`workspace.list`](#workspacelist) | `{ items: [{ workspace, branch }] }` | `work` |
| [`workspace.mount`](#workspacemount) | `{ workspace, path }` | `admin` |
| [`workspace.exec`](#workspaceexec) | `{ exit_code }` + `stdout`, `stderr` when not empty | `work` |
| [`workspace.cancel`](#workspacecancel) | `{ signal }`, `{ already_ended }` or `{ unknown }` | `work` |
| [`workspace.commit`](#workspacecommit) | `{ commit }` | `work` |
| [`workspace.unmount`](#workspaceunmount) | `{ ok }` | `admin` |
| [`workspace.status`](#workspacestatus) | `{ changes }` | `work` |

A Workspace is named by its handle `<sandbox>:<name>`, the `workspace`
`mount` answers; every other `workspace` call takes it. The path form of the
handle (`<sandbox>:/path`) is refused `InvalidInput`
([`names.rs`](../../../../control/crates/layerfs-home/src/names.rs)). A
Sandbox holds at most 16 Workspaces
([`serve.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle/serve.rs)).
Under a folder scope, `mount` takes only `main`, mounted as the Branch
`<namespace>.main`; `exec` starts the command in the scope's folder (a
prefix, not confinement); and the Sandbox must belong to the scope's project
([`guard/scope.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/scope.rs)).

On `lima`, `mount`, `commit`, `unmount` and `status` go from the caller
straight to the Sandbox's daemon; `exec` and `cancel` go through the
machine's worker. On `docker` every call goes through its worker. A call
routed to a machine that is not ready is refused `Unavailable` with
`fix: layerfs machine start <machine>` (a worker of another build:
`layerfs machine stop <machine> && layerfs machine start <machine>`); no
call starts a machine.

### workspace.list

Every mounted Workspace, or one Sandbox's.

```text
CLI     layerfs workspace list [--sandbox S] [--limit 100] [--cursor C]
Node    await layerfs.workspace.list({ sandbox, limit, cursor })
Python  layerfs.workspace.list(*, sandbox=None, limit=None, cursor=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `sandbox` option | name | list only this Sandbox's Workspaces |
| `limit` option | int | the most rows in one page, 1 to 1000; default `100` |
| `cursor` option | text | the `next_cursor` of the page before; it carries the filters |

| Answer | Keys |
| --- | --- |
| short | `items`; when set: `next_cursor`, `not_listed` |
| long | `items, next_cursor, not_listed` |
| `items[]` short | `workspace`, `branch` |
| `items[]` long | `workspace, branch, sandbox, project, path, mounted_at` |
| `not_listed[]` short | `sandbox`, `message`; when set: `blocked_by`, `reason` |
| `not_listed[]` long | `sandbox, blocked_by, reason, message` |
| call | pages by `limit` and `cursor`; writes one call-log line; MCP `work` |

The handle of every mounted Workspace, to find one again. It reads Workspace
records: the home's own for a Sandbox on the host machine or one reached directly,
and one request to each other ready machine's worker for the records it
keeps ([`src/list.rs`](../../../../control/crates/layerfs-workspace/src/list.rs)).
A Sandbox whose machine, or which itself, is not ready is named in
`not_listed` with `blocked_by`; one whose worker did not answer with
`reason` (`timeout`, `busy`, `lost`, or `refused` with its refusal in
`message`). Rows carry no state: `workspace status` and `sandbox get` ask
the daemon. Rows are ordered by Sandbox, then name; the cursor carries
`--sandbox`.

```console
$ layerfs workspace list
{"items":[{"workspace":"tests:agent","branch":"agent"}]}
```

### workspace.mount

Mount a Branch in a Sandbox.

```text
CLI     layerfs workspace mount <sandbox> <branch> [--name N]
Node    await layerfs.workspace.mount(sandbox, branch, { name })
Python  layerfs.workspace.mount(sandbox, branch, *, name=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `sandbox` | name | a Sandbox name |
| `branch` | name | a Branch other than `main` |
| `name` option | name | the Workspace's name within its Sandbox (1 to 63 lowercase letters, digits, `_`, `.` or `-`, starting and ending with a letter or digit); default: the Branch's name made valid: lowercased, each character outside the Workspace name alphabet (lowercase letters, digits, `_`, `.`, `-`) replaced by `-` |

| Answer | Keys |
| --- | --- |
| short | `workspace`, `path` |
| long | `workspace, path, outcome, branch, commit, base_layer, root, id, namespace` |
| call | writes one call-log line; MCP `admin` |

Mounts a Branch as a new Workspace, a complete writable view of its head,
and answers its handle and the directory the daemon mounted it at
(`/workspaces/<n>` in the Sandbox; a folder under its run folder on macOS
`macos-host`). Without `--name` the name is the Branch's made valid: lowercased,
each character outside `a-z 0-9 _ . -` replaced by `-`, cut to 63 and
`_ . -` trimmed from the ends (`Feature/Login` mounts as `feature-login`).
The long form adds `outcome` (`mounted`), `branch`, `commit` (the Commit it
stands on; on a fresh fork, the publication it was forked at), `base_layer`,
`root`, `id` (the 64-hex Workspace id) and `namespace`.

The caller reads no Store: it sends the project and Branch names and the
daemon finds the Branch as it binds. The mount binds the whole root without
a scan or copy; its only new file is the Workspace's overlay database,
copied from the template. It is recorded as `workspace/<id>.json` in the
Sandbox's folder
([`machine/serve.rs`](../../../../control/crates/layerfs-workspace/src/machine/serve.rs)).
After an uncertain mount, `workspace list` or `sandbox get` shows what is
mounted.

| Refusal | When | Next |
| --- | --- | --- |
| `Exists` | a Workspace of that name is mounted in the Sandbox | `fix` names `--name` |
| `InvalidInput` | the Branch is `main`, or its default name is empty or reserved | `fix`: `layerfs branch fork <project> <name> main` for `main` |
| `Capacity` | the Sandbox already holds 16 Workspaces | |
| `NotFound` | no such Sandbox, project or Branch (the daemon's lookup, [`bind/run.rs`](../../../../engine/crates/layerfs-store-service/src/workspace/bind/run.rs)) | |

Run on macOS (`macos-host`) with a debug build:

```console
$ layerfs workspace mount tests main
{"error":"InvalidInput","message":"main is the project's published line, not a Branch, and is not mounted: fork a Branch from it and mount that","fix":"layerfs branch fork shop/api <name> main"}
$ layerfs workspace mount tests agent
{"workspace":"tests:agent","path":"/tmp/demo/home/machine/macos-host/run/layerfs-8a7387b2-tests/workspaces/1"}
```

### workspace.exec

Run one bash command in a Workspace.

```text
CLI     layerfs workspace exec <workspace> <command> [--cwd D] [--env K=V]... [--stdin] [--stream] [--progress]
Node    await layerfs.workspace.exec(workspace, command, { cwd, env, stdin, stream, stdout, stderr, on_progress })
Python  layerfs.workspace.exec(workspace, command, *, cwd=None, env=None, stdin=None, stream=None, stdout=None, stderr=None, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `workspace` | text | the handle `<sandbox>:<name>` that `workspace mount` answers |
| `command` | text | one bash command line |
| `cwd` option | text | the folder the command starts in, as a path from the Workspace's root; default: the root |
| `env` option | json | one environment variable for the command, `K=V`; repeat the option for more (Node and Python: one object of names to text) |
| `stdin` option | content | the command's stdin: in the shell, this process's stdin read whole; in Node and Python, text or bytes |
| `stream` option | bool | write the command's output to this process's stdout and stderr instead of answering it; the shell prints no result |
| `stdout` option | function | Node and Python: a function given the command's stdout |
| `stderr` option | function | Node and Python: a function given the command's stderr |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | `exit_code`; when set: `stdout`, `stderr`, `stdout_file`, `stderr_file`, `cancelled` |
| long | `exit_code, stdout, stderr, stdout_file, stderr_file, cancelled, exec, stop, stdout_bytes, stderr_bytes, duration_ms` |
| call | sends progress; writes one call-log line; MCP `work` |

Runs one `bash -c` command in the Workspace's directory, as the Sandbox's
command identity with `HOME` its scratch folder, and waits for it; it does
not commit ([`exec/run.rs`](../../../../control/crates/layerfs-workspace/src/exec/run.rs)).
A non-zero exit is not an error. The long form adds `stdout_file`,
`stderr_file`, `cancelled`, `exec`, `stop` (`cgroup` on Linux kernels,
`group` on `macos-host`), `stdout_bytes`, `stderr_bytes` and
`duration_ms`. A stream of at most 1 MiB is answered whole; a longer one
answers its last 1 MiB and names the file that holds all of it,
`<run folder>/exec/<workspace id>/<exec id>.stdout`, which `sandbox files`
reads and `unmount` deletes. No total is capped.

LayerFS makes every exec id (`exec-` and 16 hex digits); `--progress` sends
`{"phase":"start","done":0,"total":1,"exec":"exec-…"}` before the command
starts: the id `workspace cancel --exec` takes. While the command runs, its
record `sandbox/<name>/workspace/<id>/exec/<exec id>.json` lets any process
cancel it, makes `unmount` and `sandbox remove` refuse `Busy` unless
`--force`, and is what `sandbox get` lists under the Workspace's `running`,
with the call's log tag and the command's program; a record whose owner
process is gone is ended and removed by the next call that finds it
([`record/exec.rs`](../../../../control/crates/layerfs-sandbox/src/record/exec.rs)).
There is no timeout, no implicit Commit and no automatic unmount.

`--cwd` is a folder of the Workspace as a path from its root; `--env K=V`
repeats (Node and Python pass one object of names to text); `--stdin` gives
the command this process's stdin, read whole before it starts (Node and
Python: text or bytes); without it the command reads `/dev/null`.
`--stream` writes the output to this process's stdout and stderr after the
command ends and prints no result (Node and Python: the `stdout` and
`stderr` functions). The shell exits with the command's own code (0 when it
has none, 1 for a code outside 0 to 255) and prints the result JSON on
stdout; a refusal exits 125 with nothing on stdout.

| Refusal | When | Next |
| --- | --- | --- |
| `Unknown` | the reply was lost: the command may have started and may still run; it is never run again | `exec` names it; `fix`: `layerfs workspace cancel <workspace> --exec <exec>` |
| `Failed` | the runtime could not run the command, or spooling or draining failed | |

Run on macOS (`macos-host`) with a debug build:

```console
$ layerfs workspace exec tests:agent 'echo hello > out.txt && uname -s'
{"exit_code":0,"stdout":"Darwin\n"}
$ layerfs workspace exec tests:agent 'exit 3'
{"exit_code":3}
```

The second exits 3.

### workspace.cancel

End the commands running in a Workspace, or one of them.

```text
CLI     layerfs workspace cancel <workspace> [--exec E]
Node    await layerfs.workspace.cancel(workspace, { exec })
Python  layerfs.workspace.cancel(workspace, *, exec=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `workspace` | text | the handle `<sandbox>:<name>` that `workspace mount` answers |
| `exec` option | text | end only this command: the exec id from its progress event, `sandbox get` or the long form; default: every running command |

| Answer | Keys |
| --- | --- |
| short | each when set: `signal`, `already_ended`, `unknown` |
| long | `signal, already_ended, unknown, outcome, commands` |
| `commands[]` short | `{}` |
| `commands[]` long | `exec, outcome, signal` |
| call | writes one call-log line; MCP `work` |

Ends every command running in the Workspace, or the one `--exec` names:
SIGTERM to everything it started, then SIGKILL after 5 s
([`exec/settle.rs`](../../../../control/crates/layerfs-sandbox/src/exec/settle.rs)).
Three short forms, all exit 0: `{"signal":"SIGTERM"}` (a command was
signalled and has ended; `signal` is the last one sent),
`{"already_ended":true}` (nothing was running: the named command had
finished, or has no record) and `{"unknown":true}` (the machine's worker was
not reached or did not answer, so nothing is known to have changed; cancel
again once it answers). With several commands `signal` and `unknown` can
stand together; the long form's `outcome` is the worst of
`commands[].outcome` (`unknown`, `cancelled`, `already_ended`) and
`commands` is `[{exec, outcome, signal}]`. A cancelled command's `exec`
answers `cancelled: true` and exit code 128 plus the signal.

On `namespaces` each command runs in its own cgroup
`<sandbox cgroup>/ws-<workspace id>/exec-<exec id>`, so children that left
its process group end too; on `macos-host` it is the command's process
group, which a child calling `setsid` escapes. With no record, a pending
mark is kept for 30 s and cancels a command registered with that id in that
time ([`exec/pending.rs`](../../../../control/crates/layerfs-sandbox/src/exec/pending.rs)).
It was `workspace.exec_cancel`, removed with no alias; its grace is fixed at
5 s.

```console
$ layerfs workspace cancel tests:agent
{"already_ended":true}
```

### workspace.commit

Commit a Workspace's changes to its Branch.

```text
CLI     layerfs workspace commit <workspace> [-m msg] [--expected C] [--force]
Node    await layerfs.workspace.commit(workspace, { message, expected, force })
Python  layerfs.workspace.commit(workspace, *, message=None, expected=None, force=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `workspace` | text | the handle `<sandbox>:<name>` that `workspace mount` answers |
| `message` option | text | the Commit message; `-m` in the shell |
| `expected` option | name | refuse `Conflict` unless the Branch's head is this Commit |
| `force` option | bool | commit over a head that moved since the mount instead of merging with it |

| Answer | Keys |
| --- | --- |
| short | `commit`; when set: `unchanged`, `merged`, `overwritten`, `overwritten_count` |
| long | `commit, unchanged, merged, overwritten, overwritten_count, outcome, branch, parent, root, message, created_at, merged_from, displaced` |
| `overwritten[]` short | each a `path` |
| call | writes one call-log line; MCP `work` |

Commits everything changed in the Workspace since its base, whichever
process changed it, as one Commit on its Branch; `commit` is the Branch's
new head. The daemon captures the Workspace's published frontier (not
filtered by exec), saves, merges a moved head and publishes; only the
publish step holds the Store's lane, and later writes stay in the Workspace
([`daemon/refusal.rs`](../../../../control/crates/layerfs-sandbox/src/route/daemon/refusal.rs)).
The Workspace stays mounted.

When the Branch moved since the mount the changes are merged first
(`merged: true`; `overwritten` lists the paths a newer-write-wins rule
decided, the first 20, then `overwritten_count`); `--force` commits over the
moved head instead, and the long form's `displaced` is the head it
replaced. Nothing to commit answers the head with `unchanged: true`. The
long form adds `outcome`, `branch`, `parent` (the Commit before it, or for a
Branch's first Commit the publication it was forked at; null when
unchanged), `root`, `message`, `created_at` and `merged_from`. `--expected
C` refuses `Conflict` unless the Branch's head is `C`, an id or a prefix of
8 or more characters resolved as `commit get` resolves it. It is checked
before the Commit and again inside the publication's own transaction: a head
that moves after the first check is refused the same way, with nothing
committed, and is never merged with. `--force --expected C` replaces only
`C`.

| Refusal | When |
| --- | --- |
| `Conflict` | a path conflicts, or the head is not `--expected`; nothing is committed. It carries `commit` (the head), `conflicts` (the first 20 paths) and `conflicts_count` |
| `InvalidInput` | an `--expected` prefix several Commits start with (`matches`) |
| `Busy` | another control operation runs on the Workspace ([`workspace/registry.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/registry.rs)) |
| `Unknown` | an earlier Commit's outcome was unknown or its install failed, and the one resolution this Commit ran first did not settle it (`detail.phase: "admission"`, the reason in `detail`): the daemon keeps that custody and refuses new Commits; or the reply was lost |

After an uncertain Commit read `commit list` (a new head?) and
`workspace status`. A lost reply does not remove a published Commit.

```console
$ layerfs workspace commit tests:agent -m 'test output'
{"commit":"12bb5eaecf807d89c86f1bc55f01e1a3e6ebb17d2fb2de1de6a3ccba3cf2573e67"}
```

### workspace.unmount

End a Workspace.

```text
CLI     layerfs workspace unmount <workspace> [--discard] [--force]
Node    await layerfs.workspace.unmount(workspace, { discard, force })
Python  layerfs.workspace.unmount(workspace, *, discard=None, force=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `workspace` | text | the handle `<sandbox>:<name>` that `workspace mount` answers |
| `discard` option | bool | drop its uncommitted changes |
| `force` option | bool | cancel its running commands and abort its connection |

| Answer | Keys |
| --- | --- |
| short | `ok` |
| long | `ok, outcome, abort, detach, cleanup` |
| call | writes one call-log line; MCP `admin` |

Ends a Workspace: the daemon drains its requests, detaches the mount,
closes its overlay connection and deletes its overlay file; the Branch's
history stays. `--discard` drops uncommitted changes instead of refusing
`Dirty`. `--force` cancels each running command first, then one abort write
ends the connection (also after a normal unmount stalled); on the `macos-host` NFS
route one forced client unmount takes its place. The long form's `outcome`
is `unmounted` or `force_unmounted`, with a forced unmount's `abort`,
`detach` and `cleanup`.

The machine writes `workspace/<id>.unmount` before it calls the daemon and
records the outcome after, so a repeated unmount answers what the first one
established without dialling again. The one exception is a record the daemon
answered `Retained` at stage `Detach`: its export is still held and one
forced client unmount finishes it, so `--force` may replace that record with
a forced attempt. A Workspace the daemon no longer knows answers `ok`
([`unmount_guard.rs`](../../../../control/crates/layerfs-sandbox/src/record/unmount_guard.rs)).

| Refusal | When |
| --- | --- |
| `Dirty` | uncommitted changes and no `--discard`: `changes` (the first 20 entries, as `status` lists them), `changes_count`, `pending_merge`, and a `fix` naming `workspace commit` |
| `Busy` | without `--force`, commands run (`running`, the exec ids); another unmount is in progress; a process holds the mount |
| `Unknown` | the worker's answer was lost; or with `--force` a command's cancel was not confirmed (`running`). Call `unmount` again: a recorded outcome is answered as it was |

```console
$ layerfs workspace unmount tests:agent
{"ok":true}
```

### workspace.status

A Workspace's uncommitted changes, and whether its Branch moved.

```text
CLI     layerfs workspace status <workspace> [--path P]
Node    await layerfs.workspace.status(workspace, { path })
Python  layerfs.workspace.status(workspace, *, path=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `workspace` | text | the handle `<sandbox>:<name>` that `workspace mount` answers |
| `path` option | text | list only changes under this folder of the Workspace; it filters within the same bounded read |

| Answer | Keys |
| --- | --- |
| short | `changes`; when set: `changes_count`, `partial`, `branch_moved`, `state`, `op`, `reason`, `fix` |
| long | `changes, changes_count, partial, branch_moved, state, op, reason, fix, branch, commit, base_layer, branch_head, activity, overlay, pending_merge` |
| `changes[]` short | `path`, `change`; when set: `type` |
| `changes[]` long | `path, change, type` |
| `overlay[]` short | `{}` |
| `overlay[]` long | `inode_count, entry_count` |
| call | writes one call-log line; MCP `work` |

What a Commit would capture now, from one bounded read of the overlay (256
rows). `changes` is the first 20 entries read, each `{path, change}`
(`added`, `modified`, `removed`) with `type: "dir"` for a folder; an entry is
not a file count: a renamed or removed folder is one entry.
`changes_count` is given when more than the 20 shown were read, and
`partial: true` when the bound was hit. `branch_moved: true` when the Branch
gained Commits since the mount (a Commit will merge). `state` is left out
when `ready`: `busy` with its `op` (`{name, phase, started_at}`: `mount` in
`attach`, `commit` in `flush`, `publish` or `invalidate`, `unmount` in
`close`) and `broken` with a `fix` after a failure, from the daemon's
`activity`. The long form adds `branch`, `commit`, `base_layer`,
`branch_head`, `activity`, `overlay` (`{inode_count, entry_count}`) and
`pending_merge`. `--path` narrows the answer to one folder within the same
read. It changes nothing, is answered while a `sandbox create` or `remove`
holds the Sandbox, and is the read after an uncertain `exec` or `commit`.

Run on macOS (`macos-host`) with a debug build:

```console
$ layerfs workspace status tests:agent
{"changes":[{"path":"/out.txt","change":"added"},{"path":"/x.txt","change":"removed"}]}
```

## Family errors

| Error | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad handle (a path handle included) or option; `main` as the Branch; out of scope | [`machine/serve.rs`](../../../../control/crates/layerfs-workspace/src/machine/serve.rs) |
| `NotFound` | no such Sandbox (or its daemon ended itself when idle); the handle is not mounted (raised `NoSuchSandbox`, `NotMounted`) | same |
| `Busy`, `Unresolved` | a `sandbox create` or `remove` holds the Sandbox (not for `status`); no daemon connection lent in time | [`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs), [`channel/pool.rs`](../../../../control/crates/layerfs-sandbox/src/route/daemon/channel/pool.rs) |
| `Unavailable` | the machine's worker is not recorded or does not answer; a saved daemon that is not ready (raised `StaleSandbox`) | [`send.rs`](../../../../control/crates/layerfs-sandbox/src/route/machine/send.rs), `daemon/pool.rs` |
| `InvalidInput` | a request over one control record (raised `TooLarge`) | [`daemon/call.rs`](../../../../control/crates/layerfs-sandbox/src/route/daemon/call.rs) |
| daemon codes | a daemon control refusal keeps its own code with `detail.phase`, folded to its public code | `daemon/values.rs` |
| `Failed` | a daemon exchange failed, in words. When the request was sent and no answer settled it, the message ends "the request was sent; its outcome is not known" and the refusal carries `attempted: true` (beside `error`, and in the long form's `detail`): read the Workspace's state before acting again | [`daemon/refusal.rs`](../../../../control/crates/layerfs-sandbox/src/route/daemon/refusal.rs) `failure`, [`daemon/failure.rs`](../../../../control/crates/layerfs-sandbox/src/route/daemon/failure.rs) |
| `Unknown` | the worker's answer was lost after the request was sent | [`guest/client.rs`](../../../../control/crates/layerfs-machine/src/route/guest/client.rs) |

## Related

- [sandbox family](sandbox.md), [commit family](commit.md), [branch family](branch.md).
- [Daemon channel and exec](../../control/sandboxes/daemon-channel-and-exec.md), [bridge](../../engine/bridge/README.md).
- [Errors and events](../calls/errors-and-events.md).
