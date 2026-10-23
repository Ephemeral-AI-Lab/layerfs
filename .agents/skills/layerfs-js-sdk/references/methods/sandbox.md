# sandbox methods

> **Status:** Current general guide. Part of the `layerfs-js-sdk` skill. The methods
> are #428's; behaviour is from the source and its Docker checks
> (`sdk/js/packages/agent/test/verify/sandbox_real.mjs`).

Examples use `layerfs` as connected in [SKILL.md](../../SKILL.md#connect-to-a-project).

**Placement is the default.** `workspace.mount(branch)` puts each Workspace in one of
the project's Sandboxes and starts one when every one is full; you don't need these
methods to work. Use them when you want a Sandbox of your own: one your Workspaces
don't share with other agents' placed ones, such as for a dev server or a long build.
Close it when you're done.

```js
const own = await layerfs.sandbox.create();                    // { sandbox: "2", name, image }
try {
  const ws = await layerfs.workspace.mount("work", { sandbox: own.sandbox });
  try {
    const run = await layerfs.workspace.exec(ws, "timeout 110 npm run build", "step 20");
    await layerfs.workspace.commit(ws, `step 20: npm run build (exit ${run.exitCode})`);
  } finally {
    await layerfs.workspace.unmount(ws);
  }
} finally {
  await layerfs.sandbox.close(own.sandbox);
}
```

Placement may also put another agent's Workspace in a Sandbox you created when it has
room; `close` then refuses `MountsActive` until it's unmounted.

## `sandbox.create(options?)`

Starts the project's next Sandbox and waits for its daemon.

| Arg | Type | Required | Meaning |
| --- | --- | --- | --- |
| `options.image` | string | no | Exact local image digest `sha256:<64 hex>`; default `LAYERFS_IMAGE` |

**Returns:** `{ sandbox, name, image }`: its number, its container name
(`<prefix><project>-<n>`) and its image.

**Refusals:** as a mount that starts a Sandbox: `Failed` (with `detail.removed: true`
nothing of it is left), `Unknown` (an earlier start's outcome is unknown; report it),
`Busy`, `InvalidInput`.

**Notes:** like any Sandbox, it closes itself two minutes after its last Workspace is
unmounted, with no call in flight.

## `sandbox.list_workspaces(sandbox, pageSize?, pagePos?)`

The mounted Workspaces of one running Sandbox, from its daemon.

**Returns:** a page `{ items, hasMore, nextPagePos, version }`. Each item:

| Field | Meaning |
| --- | --- |
| `sandbox`, `path` | The Workspace, as `mount` returned it |
| `branch`, `base` | Its Branch and the Commit it's based on |
| `activity` | `Idle`; `Busy` while a Commit or mount runs in it; `Closing`, `Uncertain` or `Failed` (unmount it only after a look); any other daemon state as the daemon names it |
| `changed` | Whether it holds uncommitted changes (`null` when unknown) |
| `headMovedSinceMount`, `pendingMerge` | As in the backend's `workspace.status` |

**Refusals:** `NoSuchSandbox` (not running), `InvalidInput`.

## `sandbox.close(sandbox)`

Stops a Sandbox now: stop, then delete, each attempted once, then its records.

**Returns:** `{ kind: "Closed", sandbox }`.

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `MountsActive` | A Workspace is still mounted there; `detail.workspaces` lists them | Commit or unmount them first (yours), or leave the Sandbox to close itself |
| `NoSuchSandbox` | No such Sandbox, or it already closed itself | Nothing to do |
| `Failed`, `Unknown` | The stop or delete failed, or its outcome is uncertain | Stop and report the message |
