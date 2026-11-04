# workspace.commit

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [workspace](README.md) › commit

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Commits everything published in a Workspace so far, from every process
that wrote to it, as one Commit on its Branch; if the Branch's head moved,
the changes are merged first. The Workspace stays mounted.

| | |
| --- | --- |
| Family | [workspace](README.md) |
| Library entry | [`api/crates/layerfs-api/src/workspace/commit.rs`](../../../../../api/crates/layerfs-api/src/workspace/commit.rs) |
| Implemented in | `layerfs_control::workspace::calls::workspace` → host operation `workspace.commit` → `workspace::serve::commit` ([`workspace/serve.rs`](../../../../../api/crates/layerfs-control/src/workspace/serve.rs)) |
| Runs in | the Sandbox's daemon, which opens the Store and saves and publishes in-process |

## Signature

```js
// JavaScript
await layerfs.workspace.commit(ws, { message, force });
```

```python
# Python
layerfs.workspace.commit(ws, message=None, force=None)
```

```bash
# CLI
layerfs workspace commit <ws> [--message TEXT] [--force]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `ws` | Text | a mounted Workspace, `<sandbox>:<path>` or `<sandbox>:<name>` | [`common/names.rs`](../../../../../api/crates/layerfs-control/src/common/names.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `message` | Text | none | the Commit's message | [`workspace/serve.rs`](../../../../../api/crates/layerfs-control/src/workspace/serve.rs) |
| `force` | Bool | false | publish over a moved head instead of merging (the head is displaced) | same, `CommitControl` |

## Returns

```text
 Committed: { kind: "Committed", head, commit: { ...commit },
              displaced_head | null,
              merged: null | { from, head }, lww: [..], lww_truncated }
 UpToDate:  { kind: "UpToDate", head | null, root,
              merged, lww, lww_truncated }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `head`, `commit` | text, map | the new head and the Commit, as on [commit.list](../commit/list.md#returns) | [`daemon/values.rs`](../../../../../api/crates/layerfs-control/src/daemon/values.rs) `commit` |
| `displaced_head` | text or null | the head it replaced, when it overwrote one | same |
| `merged` | map or null | the head had moved: merged from `from` onto `head` | same |
| `lww`, `lww_truncated` | list, bool | paths settled by `/.lww` rules in that merge | same |
| `root` | text | `UpToDate`: nothing changed since the head | same |

## Where it runs

```text
 caller process          machine (here | worker)          Sandbox daemon
 --------------          -----------------------          --------------
 ws, sandbox_of, free
 forwarded? direct::call ---------------------------->    commit_with
 else on_machine ------> serve::commit --------------->   { token, force,
                                                            message }
                                                          admission: idle?
                                                          capture frontier
                                                          build + Save
                                                          head moved? merge
                                                          lane: publish
                                                          install new base
                         <-- CommitReply ---------------
                         values::commit
```

The capture takes the Workspace's locally published frontier, whichever
process wrote it; it is not filtered by `exec`. Later writes keep going
during the Commit and stay in the Workspace. Only the publish step holds
the Store's lane. See the [routing table](../../routing/routing-table.md),
[Engine: Commit](../../../engine/commit/README.md) and
[A Commit](../../../flows/commit.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad `ws` | no | [`common/names.rs`](../../../../../api/crates/layerfs-control/src/common/names.rs) |
| `NoSuchSandbox`, `NotMounted` | no such Sandbox or Workspace | no | [`workspace/serve.rs`](../../../../../api/crates/layerfs-control/src/workspace/serve.rs) |
| `Conflict` | the head moved and files conflict: markers are written into the Workspace and nothing is committed; or files still hold markers (`detail.head`, `base`, `markers`, `conflicts`, `written`, `lww`) | no | [`daemon/values.rs`](../../../../../api/crates/layerfs-control/src/daemon/values.rs) |
| `Busy` | another control operation runs on the Workspace; or a `sandbox.create`/`close` holds the Sandbox | no | [`registry.rs`](../../../../../engine/crates/layerfs-daemon/src/control/registry.rs) `idle` |
| `Unresolved` | the Sandbox's intent is unresolved | no | [`sandbox/record.rs`](../../../../../api/crates/layerfs-control/src/sandbox/record.rs) |
| `Unknown`, phase `admission` | **an earlier Commit's install was uncertain or failed; the daemon keeps that custody and refuses new Commits** | **yes (the earlier one)** | [`registry.rs`](../../../../../engine/crates/layerfs-daemon/src/control/registry.rs) `idle` |
| other daemon refusal codes | refused with `detail.phase` | no | [`daemon/values.rs`](../../../../../api/crates/layerfs-control/src/daemon/values.rs) `refusal` |
| `Unavailable` | the worker is not recorded or does not answer | no | [`sandbox/route.rs`](../../../../../api/crates/layerfs-control/src/sandbox/route.rs) |
| `Failed` | **a daemon exchange failed; `attempted` in the message says whether it may have published** | **maybe** | [`sandbox/serve.rs`](../../../../../api/crates/layerfs-control/src/sandbox/serve.rs) `operation` |
| `Unknown` | **the worker's answer was lost** | **yes** | [`machine/worker/client.rs`](../../../../../api/crates/layerfs-control/src/machine/worker/client.rs) |

**After an uncertain Commit**, read [commit.list](../commit/list.md) for the
Branch (did a new head appear?) and [workspace.status](status.md) (its
`activity`, `base` and `pending_merge`) before committing again. A lost
reply does not remove a published Commit.

## Examples

Not run; written from the call table.

```js
try {
  const c = await layerfs.workspace.commit(ws, { message: "tests pass" });
  console.log(c.kind, c.head);
} catch (e) { if (e.code === "Conflict") console.log(e.detail.written); }
```

```python
layerfs.workspace.commit(ws, message="build output")
```

```bash
layerfs workspace commit s1:build --message 'tests pass'
```

## Related

- Before: [workspace.exec](exec.md), [workspace.status](status.md).
  After: [branch.publish](../branch/publish.md), [workspace.unmount](unmount.md).
- [A Commit](../../../flows/commit.md),
  [Engine: Commit, publish and merge](../../../engine/commit/publish-and-merge.md).

---

← [workspace.exec](exec.md) · [Up: workspace](README.md) · [workspace.unmount](unmount.md) →
