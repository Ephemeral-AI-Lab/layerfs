# sandbox calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. **Not run in this pass** (no Sandbox was started):
> everything here is read from `api/crates/layerfs-api/src/family/sandboxes.rs`,
> `host/sandbox.rs` and the table, except `sandbox.list` on an empty home
> (`{"items":[]}`, seen live).

```js
sandbox.create(project, name, machine, { image })
sandbox.list(project?, { page_size, page_pos })
sandbox.get(name, { page_size, page_pos })
sandbox.close(name, { force })
```

A Sandbox is named (unique in the home), bound to one project when created, and
runs one LayerFS daemon on one machine. It holds up to 16 Workspaces of any
Branches of its project. `sandbox.create` is the only call that starts one;
`workspace.mount(sandbox, branch)` mounts in the one it names.

```js
import layerfs from "layerfs";

const P = "shop/api";
await layerfs.sandbox.create(P, "build-1", "native");
try {
  const { ws } = await layerfs.workspace.mount("build-1", "work");
  try {
    const run = await layerfs.workspace.exec(ws, "timeout 110 npm run build", { message: "step 20" });
    await layerfs.workspace.commit(ws, { message: `step 20: npm run build (exit ${run.exit_code})` });
  } finally {
    await layerfs.workspace.unmount(ws);
  }
} finally {
  await layerfs.sandbox.close("build-1");
}
```

```bash
layerfs sandbox create shop/api build-1 native
```

## `sandbox.create`

`sandbox.create(project, name, machine, { image })`

Starts a Sandbox, binds it to the project and returns its record when it is
ready.

| Arg | Meaning |
| --- | --- |
| `project` | `<store>/<project>` |
| `name` | 1–63 lowercase letters, digits, `_ . -`; unique in the home |
| `machine` | Where it runs. For a sqlite Store it must be the machine the Store is on (`store.get` names it) |
| `image` | An image digest, where the machine allows one |

**What runs where in this build** (from source):

| Machine | Sandbox |
| --- | --- |
| `native` on macOS | host processes, Workspaces over loopback NFS. Needs the daemon: the native machine record's `daemon`, else `~/.cache/layerfs/bundle/<version>/<os>-<arch>/layerfs-daemon` (`Unavailable` when missing) |
| `native` on Linux | Linux namespaces |
| a VM or ssh machine | sent to that machine's worker, which runs it on its own `native` |
| `docker`, sqlite Store on `docker` | a container on the docker machine's image (`machine update docker --image`); on macOS the docker worker starts it beside itself, mounting only the Store's folder; on Linux this process starts it (root only) |
| `native` or `docker`, PostgreSQL Store | from this process; the native daemon needs the `postgresql` build |
| a VM or ssh machine, PostgreSQL Store | `Unsupported` with the support table's reason (`machine.platforms`) |

**Returns:** the Sandbox's record: `{ name, project, machine, state: "running",
created, id, platform, runtime, image }`. `name` is what you pass to every other
call; `id` is the host's internal number.

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `Exists` | A Sandbox with that name is recorded in the home (even one that closed itself) | Pick another name, or `sandbox.close(name)` first to drop the old record |
| `NotFound` | No machine of that name, or no such Store | Check `machine.list`, `project.get` |
| `Unsupported` | The machine is not the Store's (message names the Store's machine), `docker`, or a PostgreSQL Store | Use the Store's machine, or `store.move` the Store |
| `Unavailable` | The daemon bundle is missing | Report it; the home's owner installs the bundle |
| `InvalidInput` | A bad name or project, or the name `native` or `docker` (they name the built-in machines) | Fix the call |
| `Failed` | Starting it failed; its record is removed | Stop and report the message |
| `Unknown` | The start's outcome is unknown; the record is kept with `state: "unknown"` | Stop and report; don't create it again |

## `sandbox.list`

`sandbox.list(project?, { page_size, page_pos })`

Every Sandbox record in the home, or one project's, one page at a time:
`{ items: [record, …], next_page_pos }` (`page_size` 1–1000, default 50;
`next_page_pos` is `null` after the last page). It reads the home's records
only; it does not ask the daemons whether they still run.

## `sandbox.get`

`sandbox.get(name, { page_size, page_pos })`

The Sandbox's record plus `workspaces`, one page of the daemon's list of
mounted Workspaces, and `next_page_pos` (`page_size`, `page_pos` as for
`list`). From source, each entry has `workspace`, `namespace`, `branch`,
`base`, `base_layer`, `root`, `activity`, `mounted`, `directory` (its path,
the part after `:` in `ws`), `head_moved_since_mount`, `dirty`
(`{ inodes, entries }` or `null`), `pending_merge` and `revision`. Not
checked live.

`activity` is `Idle` when nothing runs in the Workspace; `Busy` while a Commit
or mount runs; `Closing`, `Uncertain` or `Failed` need a look before you
unmount.

## `sandbox.close`

`sandbox.close(name, { force })`

Stops the Sandbox now (stop, then delete, each attempted once), then removes
its record.

- **Returns:** `{ kind: "Closed", sandbox: <id>, closed: <name> }`. For a
  Sandbox that was already gone (it closed itself), `{ closed, was_gone: true }`
  and its record is removed.
- `MountsActive` while any Workspace is mounted in it (`detail.workspaces`
  lists them): commit or unmount them first.
- `force` (O31, built in `37f1bbd2b9`, from source): stops the Sandbox
  through its backend despite mounted Workspaces and answers
  `{ kind: "Closed", sandbox, unknown: [path, …] }`, the Workspaces it could
  not drain. Their uncommitted changes and processes end with the Sandbox:
  commit first what you want to keep.
- A record with no `id` (a create still running, one whose outcome was
  unknown, or one whose process ended) closes too: `InUse` while the
  creating process lives, otherwise the record goes and the name is free
  (`{ closed, was }`; for an unknown create, `left` says a Sandbox may remain
  on its machine).

## Idle close

A Sandbox's daemon exits by itself 120 seconds after its last Workspace is
unmounted with no call in flight (`IDLE_EXIT_S` in `host/sandbox.rs`). Its
record stays in the home. A mount there is then refused `NoSuchSandbox`, and
`sandbox.create` with the same name is refused `Exists` until
`sandbox.close(name)` drops the record. So after an idle close: close, then
create again (or use a new name).

## Clearing idle Workspaces

Nothing unmounts a Workspace for you: a script that exits or throws before its
`unmount` leaves it mounted, and its Sandbox never closes. To clear leftovers,
list each Sandbox's Workspaces and unmount only those that are idle, hold
nothing uncommitted and that no agent is keeping on purpose (a build Workspace,
one running a dev server). Field names are from source:

```js
const keep = new Set([/* "<sandbox>:<path>" Workspaces agents said they keep */]);
for (const sb of (await layerfs.sandbox.list("shop/api")).items) {
  const got = await layerfs.sandbox.get(sb.name);
  for (const w of got.workspaces ?? []) {
    const ws = `${sb.name}:${w.directory}`;
    if (w.activity === "Idle" && w.dirty === null && !w.pending_merge && !keep.has(ws)) {
      await layerfs.workspace.unmount(ws);   // no discard: a Dirty one stays for its owner
    }
  }
}
```

Don't pass `discard` for a Workspace another agent mounted. Without it, a
Workspace with uncommitted changes is refused `Dirty` and stays mounted.
(`dirty` may be `{ inodes: 0, entries: 0 }` rather than `null` when clean; the
unmount without `discard` is the real check.)
