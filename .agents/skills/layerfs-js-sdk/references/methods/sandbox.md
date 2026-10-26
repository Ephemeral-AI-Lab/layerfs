# sandbox methods

> **Status:** Current general guide. Part of the `layerfs-js-sdk` skill. The methods
> are #428's; behaviour is from the source and its Docker checks
> (`api/agent/test/verify/sandbox_real.mjs`).

Examples use `layerfs` as connected in [SKILL.md](../../SKILL.md#connect-to-a-project).

**A mount names its Sandbox.** There is no placement: `sandbox.create()` is the only call
that starts a Sandbox, and `workspace.mount(sandbox, branch)` mounts in the one it
names. A Sandbox holds up to 16 Workspaces of any Branches; reuse one with room (see
`project.list_sandboxes()`), or create one, and close it when you're done.

```js
const own = await layerfs.sandbox.create();                    // { sandbox: "2", image }
try {
  const ws = await layerfs.workspace.mount(own.sandbox, "work");
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

Another agent may mount in a Sandbox you created when it names it; `close` then refuses
`MountsActive` until that Workspace is unmounted.

## `sandbox.create(options?)`

Starts the project's next Sandbox and waits for its daemon. The only call that starts a
Sandbox. It first reclaims the project's Sandboxes that closed themselves, so their
numbers are used again.

| Arg | Type | Required | Meaning |
| --- | --- | --- | --- |
| `options.image` | string | no | Exact local image digest `sha256:<64 hex>`; default `LAYERFS_IMAGE`. Every Workspace mounted there runs on it |

**Returns:** `{ sandbox, image }`: its number (pass it to `workspace.mount`) and its
image. Its container is named `<prefix><project>-<n>`.

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `Failed` | Starting it failed. With `detail.removed: true` its container is already removed and nothing of it is left, and `detail.log` holds its daemon's last log lines (also in the message); otherwise it names what stays | Stop and report the message |
| `Unknown` | An earlier Sandbox start of this project has an unknown outcome; every start is refused until it is resolved | Stop and report the message, which says what a person must remove; don't remove or retry anything yourself |
| `Busy` | Another call held the project's lock for 60 seconds | Stop and report |
| `InvalidInput` | A missing `LAYERFS_*` deployment variable or an image that isn't a digest | Check the environment ([SKILL.md](../../SKILL.md#connect-to-a-project)) |

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
