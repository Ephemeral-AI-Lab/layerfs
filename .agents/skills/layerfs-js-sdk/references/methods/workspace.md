# workspace calls

> **Status:** Current general guide for the LayerFS 2.0 call surface. Part of
> the `layerfs-js-sdk` skill. Outputs marked "seen" were printed by the built
> `layerfs` command on a throwaway home on 2026-11-18 (macOS, a `native`
> Sandbox; ids shortened); the rest is from the table, the spec rows and the
> manual, not run. `workspace.exec_cancel` is gone (now `workspace.cancel`),
> with no alias.

```js
workspace.list({ sandbox, limit, cursor })
workspace.mount(sandbox, branch, { name })
workspace.exec(workspace, command, { cwd, env, stdin, stream, stdout, stderr, on_progress })
workspace.cancel(workspace, { exec })
workspace.commit(workspace, { message, expected, force })
workspace.unmount(workspace, { discard, force })
workspace.status(workspace, { path })
```

A Workspace is a complete, writable view of a Branch where ordinary commands
run. **Use one for Bash commands**; every file a command changes is recorded
at the Commit. For one small read or edit, [`file.*`](file.md) is cheaper.

## Shared rules

- A Workspace is named `<sandbox>:<name>`, such as `tests:agent`: the
  `workspace` key of `mount`'s answer. Pass that handle to every other call.
  The path form `<sandbox>:/path` is refused `InvalidInput`. The handle works
  from any later process with the same `LAYERFS_HOME`; never use one after
  its unmount.
- **Each mount is a new Workspace**, with its own uncommitted changes.
  Mounting doesn't copy or scan the tree.
- **A Workspace per Bash call is the default:** mount, exec, commit, unmount.
  Longer work is fine too: keep one mounted across several `exec` calls and
  commit at any point, or mount several at once.
- **The view is fixed at mount.** It does not follow later Commits:
  `status` answers `branch_moved: true` and `commit` merges in what moved.
  To *see* it, mount again.
- **Everything in it is committed:** sources, `.git`, `node_modules`, build
  output and caches. Scratch files belong in `/tmp`, outside the Workspace.
- Keep each Workspace's calls sequential.
- A call routed to a machine whose worker is not running is refused
  `Unavailable` with `fix: layerfs machine start <machine>`; none of these
  calls starts a machine.

```js
const { workspace } = await layerfs.workspace.mount("tests", "agent");   // a NEW Workspace
try {
  const run = await layerfs.workspace.exec(workspace, "timeout 110 npm test 2>&1");
  await layerfs.workspace.commit(workspace, { message: `step 12: npm test (exit ${run.exit_code})` });
} finally {
  await layerfs.workspace.unmount(workspace);                           // the handle is now invalid
}
```

```bash
ws=$(layerfs workspace mount tests agent | jq -r .workspace)
layerfs workspace exec "$ws" "timeout 110 npm test 2>&1" --stream; code=$?
layerfs workspace commit "$ws" -m "step 12: npm test (exit $code)"
layerfs workspace unmount "$ws"
```

## `workspace.list`

Every mounted Workspace in the home, or one Sandbox's (`sandbox`), from
records; it asks no daemon. Rows `{ workspace, branch }` (long: `sandbox`,
`project`, `path`, `mounted_at`), by Sandbox then name, paged by `limit`
(1 to 1000, default 100) and `cursor`. A Sandbox whose Workspaces could not
be listed (its machine is not ready, or its worker did not answer) is named
in `not_listed` with `blocked_by` or `reason`.

```bash
$ layerfs workspace list                                                       # seen
{"items":[{"workspace":"tests:agent","branch":"agent"}]}
```

## `workspace.mount`

Mounts the Branch's head as it is now, in the Sandbox named `sandbox`. It
never starts a Sandbox. Answers `{ workspace, path }`; long adds
`outcome: "mounted"`, `branch`, `commit` (the Commit it stands on),
`base_layer`, `root`, `id` (the 64-hex Workspace id) and `namespace`.
Without `name` the name is the Branch's made valid: lowercased, each
character outside `a-z 0-9 _ . -` replaced by `-`, cut to 63, and `_ . -`
trimmed from the ends (`Feature/Login` mounts as `feature-login`).

```bash
$ layerfs workspace mount tests agent                                          # seen
{"workspace":"tests:agent","path":"/tmp/demo/home/machine/native/run/layerfs-8a7387b2-tests/workspaces/1"}
$ layerfs workspace mount tests main                                           # seen, exit 125
{"error":"InvalidInput","message":"main is the project's published line, not a Branch, and is not mounted: fork a Branch from it and mount that","fix":"layerfs branch fork shop/api work && layerfs workspace mount tests work"}
```

| `error` | When | Do |
| --- | --- | --- |
| `InvalidInput` | `main`, or a Branch whose default name is empty or reserved (`fix` names `--name`) | Fork a Branch; pass `name` |
| `Exists` | A Workspace of that name is mounted in the Sandbox (`fix` names `--name`) | Use that one, or pick another `name` |
| `Capacity` | The Sandbox already holds 16 Workspaces | Unmount Workspaces you're done with, or use another Sandbox |
| `NotFound` | No such Sandbox or Branch | `sandbox.list`, `branch.list` |

## `workspace.exec`

Runs one bash command in the Workspace and waits for it to end. The working
directory is the Workspace root, or `cwd` (a folder of the Workspace, as a
path from its root), so use relative paths (`src/cart.js`), never the mount
path.

Answers `{ exit_code }`, with `stdout` and `stderr` when not empty. **A
non-zero exit is not a refusal.** The long form adds `stdout_file`,
`stderr_file`, `cancelled`, `exec` (its id), `stop` (`cgroup` on Linux
kernels, `group` on macOS `native`), `stdout_bytes`, `stderr_bytes` and
`duration_ms`.

```bash
$ layerfs workspace exec tests:agent 'echo hello > out.txt && uname -s'        # seen
{"exit_code":0,"stdout":"Darwin\n"}
$ layerfs workspace exec tests:agent 'exit 3'                                  # seen, exit 3
{"exit_code":3}
```

- **Output:** a stream of at most 1 MiB is answered whole; a longer one
  answers its last 1 MiB and names, as `stdout_file`/`stderr_file`, the file
  in the Sandbox's run folder that holds all of it (`sandbox.files` reads it,
  `unmount` deletes it). Output that is not UTF-8 is bytes.
- **Node and Python** also take `stdout` and `stderr` functions, called with
  the output's pieces, and `stream: true`, which writes it to your process's
  own stdout and stderr. Output is delivered after the command ends, never live.
- **Shell:** the command exits with the command's own code and prints the
  answer on stdout; `--stream` writes the output to stdout and stderr and
  prints no answer; a refusal exits 125.
- `env` is one object in Node and Python (shell: `--env K=V`, repeated).
  `stdin` gives the command text or bytes (shell: `--stdin` passes this
  process's stdin, read whole); without it the command reads `/dev/null`.
- LayerFS makes every exec id (`exec-` and 16 hex digits); `on_progress`
  receives `{ phase: "start", done: 0, total: 1, exec: "exec-…" }` before
  the command starts.
- **No timeout:** put `timeout 110` inside the command.
- It waits until everything holding its output has ended, background jobs
  included. Start a server, probe it and stop it in **one** call, with its
  output redirected to `/tmp`.

```js
await layerfs.workspace.exec(workspace, "timeout 110 npm run build >/tmp/build.log 2>&1; echo $? >/tmp/build.code");
const tail = await layerfs.workspace.exec(workspace, "cat /tmp/build.code; tail -n 60 /tmp/build.log");
```

**Write a file inside a mounted Workspace** only when this Workspace must see
it before its next Commit; otherwise use `file.*` and mount again. `stdin`
carries the content without shell quoting:

```js
await layerfs.workspace.exec(workspace, "cat > src/tax.js", { stdin: "export const TAX = 0.08;\n" });
```

**Refusals:** `Unknown` when the reply was lost: the command may have started
and may still run; it is never run again. The refusal carries `exec` and a
`fix` naming `workspace cancel --exec`. `NotFound` for a Workspace that is
not mounted.

## `workspace.cancel`

Ends every command running in the Workspace, or the one `exec` names:
SIGTERM, then SIGKILL after 5 s. Short answers: `{ signal: "SIGTERM" }` (or
`"SIGKILL"`), `{ already_ended: true }`, or `{ unknown: true }` (the worker
was not reached or did not answer: cancel again once it answers). Long:
`outcome` (the worst of `commands[].outcome`: `unknown`, `cancelled`,
`already_ended`) and `commands: [{ exec, outcome, signal }]`. A cancelled
command's `exec` answers `cancelled: true` and exit code 128 plus the signal.

```bash
$ layerfs workspace cancel tests:agent                                         # seen: nothing ran
{"already_ended":true}
```

## `workspace.commit`

Saves everything changed in the Workspace as one Commit on its Branch. If
the Branch moved since the mount, it merges first. The Workspace stays
mounted.

Answers `{ commit }` (the Branch's new head) with `unchanged` (nothing
changed; no Commit), `merged` (a merge ran), `overwritten` (paths
[`/.lww`](../rules/lww.md) rules decided, the first 20) and
`overwritten_count` past those; long adds `outcome`, `branch`, `parent`,
`root`, `message`, `created_at`, `merged_from` (the head merged in) and
`displaced`.

```bash
$ layerfs workspace commit tests:agent -m 'test output'                        # seen
{"commit":"12bb5eae…"}
```

- `expected: <commit>` refuses `Conflict` unless the Branch's head is that
  Commit (an id or a prefix of 8 or more), checked once before the Commit.
- `force: true` commits over a head that moved since the mount instead of
  merging with it, replacing what the other writer committed (`displaced`
  names it). Use it only when you mean to discard that.

| `error` | When | Do |
| --- | --- | --- |
| `Conflict` | The merge with the moved head overlaps: markers are written into the Workspace and nothing is committed; or the head is not `expected`. Flat: `commit` (the head), `conflicts` (the first 20 paths), `conflicts_count` | See [resolving conflicts](../rules/conflicts.md#commit-conflicts) |
| `NotFound` | The Workspace is not mounted | Use the handle from `mount` |

## `workspace.unmount`

Ends a Workspace. The Sandbox keeps running until `sandbox.remove`.

| Option | Meaning |
| --- | --- |
| `discard` | Drop uncommitted changes instead of refusing `Dirty` |
| `force` | End running commands and abort the Workspace's kernel connection; drops uncommitted changes. A normal unmount that stalls ends itself; use `force` only for a live connection |

Answers `{"ok":true}`; long: `outcome` (`unmounted` or `force_unmounted`)
and a forced unmount's `abort`, `detach` and `cleanup`.

```bash
$ layerfs workspace unmount tests:agent                                        # seen
{"ok":true}
```

| `error` | When | Do |
| --- | --- | --- |
| `Dirty` | Uncommitted changes; flat `changes` (the first 20, as `status` lists them), `changes_count`, `pending_merge`, and a `fix` naming `workspace commit` | Commit them so the step is recorded; discard only a Workspace you're abandoning |
| `Busy` | A command still runs there; flat `running` (exec ids) | `workspace.cancel`, or wait |
| `Unknown` | `force`, and a command's cancel was not confirmed; flat `running` | Stop and report |

## `workspace.status`

What a Workspace holds before you commit. Short: `changes`, the first 20
entries of one bounded read of 256 overlay rows, each `{ path, change }`
(`added`, `modified`, `removed`; a renamed or removed folder is one entry),
plus when set `changes_count` (past the 20 shown), `partial: true` (the read
hit its bound), `branch_moved: true` (the Branch gained Commits since the
mount) and `state` with its companion when not `ready`. `path` filters within
the same read. Long adds `type` per change, `branch`, `commit`, `base_layer`,
`branch_head`, `activity` (the daemon's own word), `overlay`
(`{ inode_count, entry_count }`) and `pending_merge`.

```bash
$ layerfs workspace status tests:agent                                         # seen
{"changes":[{"path":"/out.txt","change":"added"},{"path":"/x.txt","change":"removed"}]}
$ layerfs workspace status tests:agent --detail                                # seen
{"changes":[{"path":"/out.txt","change":"added","type":"file"},{"path":"/x.txt","change":"removed","type":"file"}],"changes_count":null,"partial":false,"branch_moved":false,"state":"ready","op":null,"reason":null,"fix":null,"branch":"agent","commit":"12d12fbd…","base_layer":"32919f55…","branch_head":"12d12fbd…","activity":"idle","overlay":{"inode_count":3,"entry_count":2},"pending_merge":false}
```

A busy Workspace's `op` is `{ name, phase, started_at }`: `mount`, `commit`
or `unmount`. A `broken` one (`unresolved`, `failed`) carries
`fix: layerfs workspace unmount <workspace> --discard --force`. Use `status`
before a Commit you might want to keep separate from other writers' changes,
or to decide whether a leftover Workspace holds anything.
