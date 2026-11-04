# Errors

[Architecture](../../README.md) › [API](../README.md) › [Call table](README.md) › Errors

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The error catalog and uncertain outcomes.

| | |
| --- | --- |
| Component | [Call table](README.md) |
| Source | [`api/crates/layerfs-control/src/common/outcome.rs`](../../../../api/crates/layerfs-control/src/common/outcome.rs), [`api/crates/layerfs-api/src/lib.rs`](../../../../api/crates/layerfs-api/src/lib.rs), [`common/op.rs`](../../../../api/crates/layerfs-control/src/common/op.rs), [`calls/text.rs`](../../../../api/crates/layerfs-control/src/calls/text.rs), [`store-service history/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/history/lookup.rs), [`store-service write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |

Every call answers `Outcome = Result<SdkValue, SdkRefusal>`
([`outcome.rs`](../../../../api/crates/layerfs-control/src/common/outcome.rs)).
A refusal is `{code, message, detail}`: `code` is a stable text, `message`
is for people, `detail` is a value or null. The front ends carry it unchanged.

## Error kinds

```text
 SdkRefusal
 +---------+-----------------------------------------------------+
 | code    | stable text: "InvalidInput", "Unknown", ...          |
 | message | for people; names the next call where there is one  |
 | detail  | null, or a map: {entity, phase, op}, {missing},     |
 |         | {worker, expected}, {needed, free}, {path}, ...      |
 +---------+-----------------------------------------------------+
   Node:   throws LayerFSError {code, message, detail}
   Python: raises layerfs.LayerFSError (.code, .message, .detail)
   CLI:    JSON on stderr, exit 125
   JSON:   {"refused": {"code", "message", "detail"}}   (call_json)
```

The catalog below is built from the codes the source raises: `refused("…")`
and `code: "…"` in `layerfs-control`, the daemon's `ControlCode`, and the
Store engine's refusals that pass through Store calls.

| Code | Meaning | Raised in (examples) |
| --- | --- | --- |
| `InvalidInput` | the request is wrong for this call or name; nothing happened. Also every folder-scope refusal (`out of scope: …`) | [`calls/check.rs`](../../../../api/crates/layerfs-control/src/calls/check.rs), [`calls/scope.rs`](../../../../api/crates/layerfs-control/src/calls/scope.rs), [`calls/dispatch.rs`](../../../../api/crates/layerfs-control/src/calls/dispatch.rs) |
| `NotFound` | no machine, bucket or Store of that name, for a call that names it directly | [`machine/record.rs`](../../../../api/crates/layerfs-control/src/machine/record.rs), [`backup/record.rs`](../../../../api/crates/layerfs-control/src/backup/record.rs), [`inventory.rs`](../../../../api/crates/layerfs-control/src/inventory.rs) |
| `NoSuchStore`, `NoSuchProject` | the Store or project of a `<store>/<project>` argument is missing | [`store/resolve.rs`](../../../../api/crates/layerfs-control/src/store/resolve.rs) |
| `NoSuchBranch`, `NoSuchCommit` | no such Branch, or no such Commit on it | [`store/calls.rs`](../../../../api/crates/layerfs-control/src/store/calls.rs), `history/lookup.rs` |
| `NoSuchSandbox`, `NotMounted` | no Sandbox of that name; no Workspace of that name or path | [`sandbox/family.rs`](../../../../api/crates/layerfs-control/src/sandbox/family.rs), [`workspace/serve.rs`](../../../../api/crates/layerfs-control/src/workspace/serve.rs) |
| `NoSuchPath`, `IsDirectory`, `NotDirectory`, `NotFile`, `NotEmpty`, `Binary` | path refusals of the `file` calls | `write/mutation.rs`, [`store-service read/`](../../../../engine/crates/layerfs-store-service/src/read) |
| `Exists`, `NameTaken`, `ReservedName` | a name is taken or reserved; a host or database already connected | [`machine/connect.rs`](../../../../api/crates/layerfs-control/src/machine/connect.rs), [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `InUse` | held: running Sandboxes, processes with the Store open, Stores on a machine | [`store/open.rs`](../../../../api/crates/layerfs-control/src/store/open.rs), `inventory.rs` |
| `Busy` | another operation runs on the entity (`detail {entity, phase, op}`), or no daemon connection came free | [`common/op.rs`](../../../../api/crates/layerfs-control/src/common/op.rs), [`daemon/pool.rs`](../../../../api/crates/layerfs-control/src/daemon/pool.rs) |
| `Unresolved` | an earlier operation on the entity ended with its outcome unknown and its process is gone | `common/op.rs` |
| `Conflict`, `HeadMoved` | a merge or Commit conflict, or the Branch head moved; for `backup.restore`, the Store changed since its last backup | [`daemon/values.rs`](../../../../api/crates/layerfs-control/src/daemon/values.rs), `history/lookup.rs` |
| `Dirty`, `MountsActive` | unmount with uncommitted changes; `sandbox.close` while Workspaces are mounted | `daemon/values.rs`, [`sandbox/serve.rs`](../../../../api/crates/layerfs-control/src/sandbox/serve.rs) |
| `Capacity` | no room: a full Sandbox (16 Workspaces), a full disk (`detail {needed, free}`), a bounded engine limit | `sandbox/serve.rs`, [`store/transfer/phases.rs`](../../../../api/crates/layerfs-control/src/store/transfer/phases.rs) |
| `TooLarge` | a worker frame or daemon control record would be too large; nothing was sent | [`worker/frame.rs`](../../../../api/crates/layerfs-control/src/machine/worker/frame.rs), [`daemon/call.rs`](../../../../api/crates/layerfs-control/src/daemon/call.rs) |
| `Unsupported` | not possible on this host or not built (`detail.missing` names tools) | [`machine/bundle.rs`](../../../../api/crates/layerfs-control/src/machine/bundle.rs), `machine/create.rs` |
| `Unavailable`, `Denied` | a server, bucket, worker or host cannot be reached, or refused the login or secret | [`common/place.rs`](../../../../api/crates/layerfs-control/src/common/place.rs), [`worker/server.rs`](../../../../api/crates/layerfs-control/src/machine/worker/server.rs) |
| `StaleSandbox`, `NoStore`, `Unauthorized`, `UnknownMethod` | a saved daemon not ready; a Store file missing where it should be; a Sandbox record not owned by this user or not private; an operation no one serves | `daemon/pool.rs`, [`store/service.rs`](../../../../api/crates/layerfs-control/src/store/service.rs), [`sandbox/files.rs`](../../../../api/crates/layerfs-control/src/sandbox/files.rs) |
| `Failed` | tried and did not happen, or no code of its own (`outcome::failed`); whatever exists is kept | `common/outcome.rs` |
| `Unknown` | the outcome is not known; see below | below |

Daemon refusals of a Workspace call carry the daemon's `ControlCode` as the
code (`Busy`, `HeadMoved`, `Missing`, `Capacity`, `Invalid`, `Failed`,
`Unknown`) with `detail.phase`
([`daemon/values.rs`](../../../../api/crates/layerfs-control/src/daemon/values.rs) `refusal`).
The 2.0.0 manual keeps a reader's version of this table in
[calls.md](../../../versioned/2.0.0/calls.md#errors-and-exit-codes).

## Uncertain outcomes

```text
 call --> work starts --> request sent / effect may have happened
                               |
                  answer lost, or engine cannot tell
                               v
                       refusal "Unknown"
                               |
     +-------------------------+---------------------------+
     | operation with an intent (op)                       | Store call
     | store push/pull/remove, machine create/connect/     | (file write,
     | remove, sandbox create/close                        |  merge, ...)
     v                                                     v
 record keeps op {uncertain: true}               engine keeps the private
 + line in log/uncertain.jsonl                   Workspace; detail names it
     |
     v  its process gone, later calls on the entity:
 "Unresolved"  --> show: store status | machine status | sandbox get
                   clear: store remove | machine remove or disconnect
                          | sandbox close
```

| Code | Outcome | Source |
| --- | --- | --- |
| `Unknown` | may have happened. Every call is one attempt; nothing is resent. Read state with `list`, `get` or `status` before acting again. | [`worker/client.rs`](../../../../api/crates/layerfs-control/src/machine/worker/client.rs) `lost`, `answer`; [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs) `retain`; [`backup/record.rs`](../../../../api/crates/layerfs-control/src/backup/record.rs) |
| `Unresolved` | an earlier `Unknown` on this entity stands; this call did nothing | [`common/op.rs`](../../../../api/crates/layerfs-control/src/common/op.rs) `blocks` |
| daemon `Unknown` | a Workspace operation's outcome is uncertain at the daemon | [`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs) `ControlCode::Unknown` |
| `Failed` after a daemon exchange with `attempted: true` | uncertain, though the code is `Failed`: a lost daemon reply is not mapped to `Unknown` | [`sandbox/serve.rs`](../../../../api/crates/layerfs-control/src/sandbox/serve.rs) `operation` |
| `Busy` | known: nothing was attempted | `common/op.rs`, `daemon/pool.rs` |
| `Unavailable`, `Denied`, `TooLarge` from a worker | known: the request was not sent | `worker/client.rs` (`Unreached`) |

Intent sources: [`store/family.rs`](../../../../api/crates/layerfs-control/src/store/family.rs),
[`store/remove.rs`](../../../../api/crates/layerfs-control/src/store/remove.rs),
[`store/transfer/phases.rs`](../../../../api/crates/layerfs-control/src/store/transfer/phases.rs),
[`machine/create.rs`](../../../../api/crates/layerfs-control/src/machine/create.rs),
[`machine/remove.rs`](../../../../api/crates/layerfs-control/src/machine/remove.rs),
[`sandbox/create.rs`](../../../../api/crates/layerfs-control/src/sandbox/create.rs),
[`sandbox/close.rs`](../../../../api/crates/layerfs-control/src/sandbox/close.rs).
A read never clears an intent. An engine read that fails is a known `Failed`,
never `Unknown` (`read_failure` in `write/mutation.rs`). After an `Unknown`
from `project.init` on a worker, the worker removes its staged copy when its
Init ends ([`project/family.rs`](../../../../api/crates/layerfs-control/src/project/family.rs)).

## Refusals

Refusals that come before any effect, in the order a call meets them
([`calls/dispatch.rs`](../../../../api/crates/layerfs-control/src/calls/dispatch.rs)):

| Step | Refusal | Source |
| --- | --- | --- |
| 1. JSON front ends parse arguments and scope | `InvalidInput` (bad JSON) | [`calls/text.rs`](../../../../api/crates/layerfs-control/src/calls/text.rs) |
| 2. Find `family.call` | `InvalidInput` "no call named …" | `table::find` |
| 3. Check arguments | `InvalidInput`: not named fields, unknown argument, a function crossing in, missing positional, wrong kind, bad size | [`calls/check.rs`](../../../../api/crates/layerfs-control/src/calls/check.rs) |
| 4. Apply folder scope | `InvalidInput` `out of scope: history`, `path`, `branch`, `project` or the call's name | [`calls/scope.rs`](../../../../api/crates/layerfs-control/src/calls/scope.rs) `apply` |
| 5. Resolve the home | `InvalidInput`: not absolute, or neither `LAYERFS_HOME` nor `HOME` set | `dispatch::home` |
| 6. Open the home | the home's refusal (layout, migration, lock), also noted by `note_failure` | [`common/home`](../../../../api/crates/layerfs-control/src/common/home) |
| 7. Scope owns the Sandbox | `InvalidInput` `out of scope: sandbox` | `Scope::owns` |
| 8. Family checks | `NoSuch*`, `NotFound`, `Busy`, `Unresolved`, `InUse`, worker `Unavailable` | family code |

The shell refuses a wrong command line itself with exit 2 before any call;
see [Front ends: CLI](../front-ends/cli.md).

## Related

- [Table](table.md): the argument kinds that step 3 checks.
- [Routing and workers](../routing/README.md): where `Unknown` can arise across a process boundary.
- [Daemon channel](../routing/daemon-channel.md#failures): daemon exchange failures.
- [Control: Home and records](../control/home-and-records.md): the `op` intent in records.
- [Front ends](../front-ends/README.md): how each front end shows a refusal.

---

← [Call table: Table](table.md) · [Up: Call table](README.md) · [API: SDK](../sdk/README.md) →
