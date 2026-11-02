# workspace calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. **Not run in this pass** (no Sandbox was started).
> Shapes are read from `api/crates/layerfs-api/src/family/sandboxes.rs`,
> `host/workspace.rs`, `host/exec.rs` and `sandbox/values.rs`, with keys in
> snake_case as the library returns them. Only `workspace status` on a missing
> Sandbox was run (`NoSuchSandbox`).

```js
workspace.mount(sandbox, branch)
workspace.exec(ws, command, { message, stream, stdout, stderr })
workspace.commit(ws, { message, force })
workspace.unmount(ws, { discard, force })
workspace.status(ws)
```

A Workspace is a complete, writable view of a Branch where ordinary commands
run. **Use one for Bash commands**; every file a command changes is recorded
at the Commit. For one small read or edit, [`file.*`](file.md) is cheaper.

## Shared rules

- A Workspace is named `<sandbox>:<path>`, such as `tests:/workspaces/1`:
  the `ws` field of `mount`'s result. Pass that string to every other call. It
  works from any later process with the same `LAYERFS_HOME`. Never build one
  yourself and never use one after its unmount.
- **Each mount is a new Workspace**, even on the same Branch, with its own
  uncommitted changes. Mounting doesn't copy or scan the tree.
- **A Workspace per Bash call is the default:** mount, exec, commit, unmount.
  Longer work is fine too: keep one mounted across several `exec` calls and
  commit at any point, or mount several at once.
- **The view is fixed at mount.** It does not follow later Commits; `commit`
  merges in what moved. To *see* it, mount again.
- **Everything in it is committed:** sources, `.git`, `node_modules`, build
  output and caches. Scratch files belong in `/tmp`, outside the Workspace.
- Keep each Workspace's calls sequential.

```js
const { ws } = await layerfs.workspace.mount("tests", "work");   // a NEW Workspace
try {
  const run = await layerfs.workspace.exec(ws, "timeout 110 npm test 2>&1", { message: "step 12" });
  await layerfs.workspace.commit(ws, { message: `step 12: npm test (exit ${run.exit_code})` });
} finally {
  await layerfs.workspace.unmount(ws);                            // ws is now invalid
}
```

```bash
ws=$(layerfs workspace mount tests work | jq -r .ws)
layerfs workspace exec "$ws" "timeout 110 npm test 2>&1" --stream; code=$?
layerfs workspace commit "$ws" -m "step 12: npm test (exit $code)"
layerfs workspace unmount "$ws"
```

## `workspace.mount`

`workspace.mount(sandbox, branch)`

Mounts the Branch's head as it is now, in the Sandbox named `sandbox`. It never
starts a Sandbox.

**Returns** (from source):

| Field | Meaning |
| --- | --- |
| `ws` | `<sandbox>:<path>`: pass this on |
| `kind` | `"Mounted"` |
| `path` | The mount path inside the Sandbox |
| `branch`, `base`, `base_layer`, `root` | The Branch, the Commit it shows, the `main` Layer under it, its root id |
| `workspace`, `namespace` | Daemon ids |
| `sandbox`, `container` | The host's internal Sandbox id and container (or daemon folder) name, **not** the Sandbox's name |
| `image`, `platform`, `runtime` | What the Sandbox runs on |

| Code | Meaning | Do |
| --- | --- | --- |
| `NoSuchSandbox` | No Sandbox of that name, or it closed itself | `sandbox.create` (after `sandbox.close(name)` for one that closed itself; see [sandbox](sandbox.md#idle-close)) |
| `NoSuchBranch` | No such Branch, or `main` | Fork first |
| `Capacity` | The Sandbox holds 16 Workspaces. (Its message still names 1.1.0's `sandbox.list_workspaces`; use `sandbox.get`) | Create another Sandbox, or unmount Workspaces you're done with |
| `Failed` | The mount failed in the daemon | Stop and report |

## `workspace.exec`

`workspace.exec(ws, command, { message, stream, stdout, stderr })`

Runs one command with `bash -c` in the Workspace and waits for it to end. The
working directory is the Workspace root, so use relative paths
(`src/cart.js`), never the mount path.

**Returns:**

| Field | Meaning |
| --- | --- |
| `kind` | `"exit:<code>"` |
| `exit_code` | The command's exit code. Non-zero is **not** a refusal |
| `stdout`, `stderr` | Each stream, whole, up to 1 MiB (bytes when it is not UTF-8). A stream over 1 MiB is `null` here, with `stdout_bytes`/`stderr_bytes` and `streamed: true`: its bytes went to the functions or `stream` only |
| `message` | Your message, echoed; it creates no history |
| `truncated` | `false`: no output is cut |

**Output streaming.** In this build the library sends the output as events
**after the command ends**, not live (from source and the commits' live
tests, `f67d5a9f6f`, `771823d728`, `94671c974c`, `37f1bbd2b9`):

- Node: `{ stdout: (piece) => …, stderr: (piece) => … }` are called with each
  piece: a string when it is UTF-8, else a `Buffer`. `{ stream: true }`
  without them writes the output to your process's own stdout and stderr.
- Python: `stdout=`, `stderr=` functions, the same way (`str` or `bytes`);
  `stream=True` alone writes to `sys.stdout`/`sys.stderr`.
- Shell: `--stream` writes the output to stdout and stderr and exits with the
  command's own code (0 when the result has none; 125 still means LayerFS
  failed). Without `--stream`, the JSON result goes to stdout and the exit
  code is 0.
- Output over 1 MiB arrives only this way, 1 MiB per piece; without a
  function, `stream` or `--stream` those bytes are not kept.

**Limits**

- No timeout: put `timeout 110` inside the command.
- It waits until everything holding its output has ended, background jobs
  included. Start a server, probe it and stop it in **one** call, with its
  output redirected to `/tmp`.
- Output up to 1 MiB per stream is returned whole; more comes only through
  the functions or `stream`. For a long log, redirect to a file in `/tmp` and
  read the end in a second call.

```js
await layerfs.workspace.exec(ws, "timeout 110 npm run build >/tmp/build.log 2>&1; echo $? >/tmp/build.code", { message: "build" });
const tail = await layerfs.workspace.exec(ws, "cat /tmp/build.code; tail -n 60 /tmp/build.log", { message: "build log" });
```

**Write a file inside a mounted Workspace** only when this Workspace must see
it before its next Commit; otherwise use `file.*` and mount again. Pass the
content as base64 so the shell doesn't reinterpret it (fine up to a few tens
of KiB):

```js
const b64 = Buffer.from("export const TAX = 0.08;\n").toString("base64");
const run = await layerfs.workspace.exec(ws, `printf '%s' ${b64} | base64 -d > src/tax.js`, { message: "write src/tax.js" });
```

**Refusals:** `NoSuchSandbox` (the Sandbox is gone: its Workspaces are gone
too; mount again elsewhere), `NotMounted`, `Unknown` (the exit code is not
known: the command may or may not have run; stop).

## `workspace.commit`

`workspace.commit(ws, { message, force })`

Saves everything changed in the Workspace as one Commit on its Branch. If the
Branch moved since the mount, it merges first. The Workspace stays mounted.

**Returns:**

| `kind` | Fields | Meaning |
| --- | --- | --- |
| `Committed` | `head`, `commit`, `displaced_head`, `merged`, `lww`, `lww_truncated` | New Commit. `merged` is `{ from, head }` when a merge ran, else `null`. `lww` lists files decided by the head's [`/.lww`](../rules/lww.md), each `{ path, kept, deleted, rule }` |
| `UpToDate` | `head`, `root`, `merged`, `lww`, `lww_truncated` | Nothing changed; no Commit |

`commit` is the record [`commit.list`](commit.md#commitlist) shows, with
`via: "exec"`.

**`force`** (passed to the daemon since `37f1bbd2b9`): publishes over a
moved head instead of merging, replacing what the other writer committed.
Use it only when you mean to discard that.

| Code | Meaning | Do |
| --- | --- | --- |
| `Conflict` | The merge with the moved head overlaps. Markers are written into the Workspace; nothing is committed | See [resolving conflicts](../rules/conflicts.md#commit-conflicts) |
| `NotMounted` | Not mounted | Use the Workspace from `mount` |

A Commit `Conflict`'s `detail` (camelCase, from source): `head`, `base`,
`markers`, `conflicts`, `lww`, `conflictsTruncated`, `lwwTruncated`, `written`,
`truncated`. Full marker files stay in the Workspace even when the reply
leaves their text out.

## `workspace.unmount`

`workspace.unmount(ws, { discard, force })`

Ends a Workspace. A Sandbox left with no Workspace closes itself 120 s later.

| Option | Meaning |
| --- | --- |
| `discard` | Drop uncommitted changes instead of refusing |
| `force` | Abort the Workspace's kernel connection; drops uncommitted changes. A normal unmount that stalls ends itself with one abort write; use `force` only for a live connection, never to repeat an attempted abort |

**Returns:** `{ kind: "Unmounted" }`, or with `force`
`{ kind: "ForceUnmounted", abort, detach, cleanup }`.

| Code | Meaning | Do |
| --- | --- | --- |
| `Dirty` | Uncommitted changes; `detail.changed` lists paths, with `pendingMerge` and `truncated` | Commit them so the step is recorded; discard only a Workspace you're abandoning |
| `Busy` | A process still runs there (`detail.processes`) | Wait for it, or end it in a command |
| `NotMounted` | Already unmounted, or the wrong `ws` | Use the Workspace from `mount` |
| `Failed` with `TeardownCustody` in the message | The mount is gone but its drain did not finish | Preserve the message; don't repeat the abort; replace the Sandbox if its connection can't be ended |

## `workspace.status`

`workspace.status(ws)`

What a Workspace holds before you commit. New in the SDK surface for 2.0
(1.1.0 had it only in `@layerfs/backend`). From source:

| Field | Meaning |
| --- | --- |
| `kind` | `"ok"` |
| `branch` | The Branch it was mounted from |
| `base`, `base_layer` | The Commit and `main` Layer it is based on |
| `branch_head` | The Branch's head now (`null` when that read failed) |
| `head_moved_since_mount` | `true` when a Commit now would merge first |
| `activity` | The daemon's activity state |
| `overlay` | `{ inodes, entries, pending_merge }`: changed files and directory entries, or `null` |

Use it before a Commit you might want to keep separate from other writers'
changes, or to decide whether a leftover Workspace holds anything.
