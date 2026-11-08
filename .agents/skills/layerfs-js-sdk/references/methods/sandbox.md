# sandbox calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. **Not run in this pass** (no Sandbox was started):
> everything here is read from `api/crates/layerfs-api/src/family/sandboxes.rs`,
> `host/sandbox.rs` and the table, except `sandbox.list` on an empty home
> (`{"items":[]}`, seen live).

```js
sandbox.create(project, name, machine?, { image })
sandbox.list(project?, { page_size, page_pos })
sandbox.get(name, { page_size, page_pos })
sandbox.close(name, { force })
```

A Sandbox is named (unique in the home), bound to one project when created, and
runs one LayerFS daemon on one machine. It holds up to 16 Workspaces of any
Branches of its project. `sandbox.create` is the only call that starts one;
`workspace.mount(sandbox, branch)` mounts in the one it names.

```js
import layerfs from "@layerfs/node";

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

`sandbox.create(project, name, machine?, { image })`

With no `machine`, the Sandbox runs on its sqlite Store's machine, or on
`native` for a PostgreSQL Store.

Starts a Sandbox, binds it to the project and returns its record when it is
ready.

| Arg | Meaning |
| --- | --- |
| `project` | `<store>/<project>` |
| `name` | 1–63 lowercase letters, digits, `_ . -`; unique in the home |
| `machine` | Where it runs. For a sqlite Store it must be the machine the Store is on (`store.get` names it) |
| `image` | An image digest, where the machine allows one; on `docker` it overrides the machine's image and the bundle's default |

**What runs where in this build** (from source):

| Machine | Sandbox |
| --- | --- |
| `native` on macOS | host processes, Workspaces over loopback NFS. Needs the daemon: the native machine record's `daemon`, else `~/.cache/layerfs/bundle/<version>/<os>-<arch>/layerfs-daemon` (`Unavailable` when missing) |
| `native` on Linux | Linux namespaces |
| a VM machine | sent to that machine's worker, which runs it on its own `native`. On `lima` this process then forwards the daemon's port to its loopback and keeps its keys in the home (#535 G4): the Workspace calls but `exec` go to the daemon directly. A forward that fails leaves the Sandbox on the worker route and the answer says `forward_failed` |
| `docker`, sqlite Store on `docker` | a container on `image`, else the docker machine's image (`machine update docker --image`), else the bundle's Sandbox image, loaded into Docker once (#535 P5); on macOS and Linux the docker worker starts it beside itself, mounting only the Store's folder (G8). Its daemon's port is published on this host's loopback: the Workspace calls but `exec` go to it directly (#535 G4) |
| PostgreSQL Store, a platform the support table allows (`native`, `docker`, `lima`, `apple-container` on macOS) | where that machine runs Sandboxes (rows above). This process resolves the password and rewrites a loopback host for the machine (`docker`: `host.docker.internal`); the create first checks, from that machine, that the server answers with the Store's schema binding. The daemon needs the `postgresql` build |
| PostgreSQL Store, a platform the table refuses (`apple-container` off macOS) | `Unsupported` with the support table's reason (`machine.platforms`) |

**Returns:** the Sandbox's record: `{ name, project, machine, created, id,
platform, runtime, image, backend, route, state: "running" }`. `name` is what
you pass to every other call; `id` is the same name, as its machine knows it;
`route` is the kernel route (`fuse`, or `nfs` on macOS `native`).

**State and intent** (#536 §7): a Sandbox's record stores facts and, while a
`sandbox.create` or `sandbox.close` runs, one `op` intent; `state` is derived
when asked:

| `state` | When |
| --- | --- |
| `creating` | a `sandbox.create`'s process is still running it |
| `closing` | a `sandbox.close`'s process is still running it |
| `running` | it has an id and no live or unresolved intent |
| `gone` | it has no id (its create's process ended before recording one), or `sandbox.get` heard from its machine that it no longer runs |
| `unresolved` | a create or close lost its answer and its process has ended |

While an intent's process lives, `sandbox.close` and every `workspace` call
but `status` are refused `Busy` (naming the operation). An `unresolved`
Sandbox refuses them `Unresolved`, naming `layerfs sandbox get <name>` (shows
the `op`) and `layerfs sandbox close <name>`, the call that clears it.

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `Exists` | A Sandbox with that name is recorded in the home (even one that closed itself) | Pick another name, or `sandbox.close(name)` first to drop the old record |
| `NotFound` | No machine of that name, or no such Store | Check `machine.list`, `project.get` |
| `Unsupported` | The machine's platform was removed in 2.0 (`firecracker`, `ssh`, `microsandbox`): ``platform `<name>` was removed in LayerFS 2.0`` | Use another machine; `machine.remove(name)` forgets the old record |
| `Unsupported` | The machine is not the sqlite Store's (message names the Store's machine), the support table refuses the platform for the Store's kind, or the PostgreSQL URL cannot be rewritten for the machine (`sslmode=verify-full` with a rewrite) | Use the Store's machine, `store.pull` / `store.push` the Store, or give the Store a URL the machine can dial |
| `Unavailable` | The machine cannot reach the PostgreSQL server, or reaches a server holding another Store (message names the host tried) | Make the server reachable from that machine |
| `Unavailable` | The daemon bundle is missing | Report it; the home's owner installs the bundle |
| `InvalidInput` | A bad name or project, or the name `native` or `docker` (they name the built-in machines) | Fix the call |
| `Failed` | Starting it failed; its record is removed | Stop and report the message |
| `Unknown` | The machine's answer was lost: a Sandbox may or may not have started. The record keeps its `op`, marked `uncertain` (`state: "unresolved"`) | Stop and report; don't create it again. `sandbox.close(name)` clears it (a Sandbox it left on its machine is not recorded) |

## `sandbox.list`

`sandbox.list(project?, { page_size, page_pos })`

Every Sandbox record in the home, or one project's, one page at a time:
`{ items: [record, …], next_page_pos }` (`page_size` 1–1000, default 50;
`next_page_pos` is `null` after the last page). Each item carries its
derived `state` and, while it has one, its `op` (`{ id, name, phase,
started, uncertain }`). It reads the home's records only; it does not ask
the daemons whether they still run (a Sandbox whose daemon ended reads
`running` here).

## `sandbox.get`

`sandbox.get(name, { page_size, page_pos })`

The Sandbox's record with its derived `state` and its `op` while it has one
(`{ id, name, phase, started, uncertain }`; reading never clears it), plus
`workspaces`, one page of the daemon's list of mounted Workspaces, and
`next_page_pos` (`page_size`, `page_pos` as for `list`). A Sandbox with no
`id` asks no daemon; one whose machine answers that it no longer runs reads
`state: "gone"` with `workspaces: []`. From source, each entry has `ws` (`<sandbox>:<name>` when its mount
named it, else `<sandbox>:<path>`: pass it to the `workspace` calls), `name`
(or `null`), `path`, and the daemon's `workspace`, `namespace`, `branch`,
`base`, `base_layer`, `root`, `activity`, `mounted`, `directory` (the same
path), `head_moved_since_mount`, `dirty` (`{ inodes, entries }` or `null`),
`pending_merge` and `revision`. Not checked live.

`sandbox.list` and `sandbox.get` never start a machine (#535 P4). When the
Sandbox's machine is down, `get` answers its record with `workspaces: []` and
`down_reason` (why its worker was not reached); `machine.start(machine)`
starts it.

`activity` is `Idle` when nothing runs in the Workspace; `Busy` while a Commit
or mount runs; `Closing`, `Uncertain` or `Failed` need a look before you
unmount.

## `sandbox.close`

`sandbox.close(name, { force })`

Stops the Sandbox now (stop, then delete, each attempted once), then removes
its record. Its `op` (`sandbox.close`) is recorded before the stop: a lost
answer (`Unknown`) keeps the record with that `op` uncertain
(`state: "unresolved"`) until another `sandbox.close` clears it; any other
refusal leaves the Sandbox as it was. `Busy` while another create or close
of it runs.

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
  unknown, or one whose process ended) closes too: `Busy` while the
  creating process lives, otherwise the record goes and the name is free
  (`{ closed, was }`, `was` being `gone` or `unresolved`; for an unresolved
  create, `left` says a Sandbox may remain on its machine).

## Sandbox lifetime

A Sandbox runs until `sandbox.close(name)`; `machine.stop` is refused
`InUse` while it runs. Its daemon does not exit when its last
Workspace is unmounted: the 120-second idle close (#373) is removed (#535
D11). A Sandbox whose daemon ended for another reason (a crash, a kill)
keeps its record: a mount there is refused `NoSuchSandbox`, and
`sandbox.create` with the same name is `Exists` until `sandbox.close(name)`
drops the record.

## Clearing idle Workspaces

Nothing unmounts a Workspace for you: a script that exits or throws before its
`unmount` leaves it mounted, and its Sandbox never closes. To clear leftovers,
list each Sandbox's Workspaces and unmount only those that are idle, hold
nothing uncommitted and that no agent is keeping on purpose (a build Workspace,
one running a dev server). Field names are from source:

```js
const keep = new Set([/* the `ws` of Workspaces agents said they keep */]);
for (const sb of (await layerfs.sandbox.list("shop/api")).items) {
  const got = await layerfs.sandbox.get(sb.name);
  for (const w of got.workspaces ?? []) {
    const ws = w.ws;
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
