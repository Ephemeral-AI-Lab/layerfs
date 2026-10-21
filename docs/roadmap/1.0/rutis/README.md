# Rust rutis plugin: tiers 0-2

> **Status:** Proposal; target LayerFS 1.0.0; not a released contract.

Three implementation tiers, one product boundary:

| Tier | Surface | LayerFS changes | Process shape | Document |
| --- | --- | --- | --- | --- |
| 0 | `project.init`, `project.list_branches`, `fs.*`, `branch.*` | None | in-process Rust plugin | [tier-0.md](tier-0.md) |
| 1 | Tier 0 plus `workspace.*` | Reuse/extract the existing session layer | in-process Rust plugin | [tier-1.md](tier-1.md) |
| 2 | Tier 1 plus remote consumers | None beyond Tier 1 | remote Rust worker + node link | [tier-2.md](tier-2.md) |

Tier 0 is a complete Store service, not a stub. Tier 1 is the full LayerFS
surface. Tier 2 changes deployment, not the service contract.

## Host compatibility

| Host | Tier 0/1 | Tier 2 |
| --- | --- | --- |
| Rust application embedding `rutis` / `rutis-loader` | Yes, built-in plugin | Yes, as the main or worker host |
| Stock `rutis-host` CLI | No; it loads Node, Bun, Python, Go and Swift, not Rust built-ins | Yes; the worker is a separate Rust host |
| `rutis-cli` with `dylib-plugins` | Yes, as a dylib variant | Yes |
| Rust host using `rutis-dylib` | Yes, as a dylib variant | Yes |

Rust has no `runtimes.remote` language entry. Tier 2 is a second Rust rutis
host linked to the main host, not a `<runtime>:<plugin>` row.

## Shared rules

- The process that runs the LayerFS session layer opens the Store directly.
  Tier 0/1 run it in the host; Tier 2 runs it in the worker.
- One call is attempted once. No retry, replay, refresh or busy handler.
  A refusal keeps its stable code; an unknown result stays unknown.
- No implicit Commit, unmount or discard. Workspaces end only by an explicit
  call or an explicit worker shutdown policy.
- One Workspace's calls are sequential. Different Workspaces are parallel.
- LayerFS Store calls are Linux-only. Tier 0/1 require a Linux host; Tier 2
  requires a Linux worker. macOS remains dsh-only under the current owner
  exception.
- The service is Rust-first. Cross-language export is a separate decision:
  once another process calls it, the Store call is a proxy (see the contract
  gate in [tier-2.md](tier-2.md)).
- The operator surface, UI, background processes and sandbox management are
  out of scope for all three tiers unless a later tier says otherwise.

## Decision gates

| Gate | Effect |
| --- | --- |
| Is Tier 1 Workspace support required? | If no, ship Tier 0 and stop. |
| Is a public Rust SDK intended? | If no, keep the session crate internal; Tier 0/1 still work. |
| May another process call Store services? | If no, Tier 0/1 stay Rust-only and Tier 2 is not shipped. |
| Is macOS required? | No, unless the owner changes the dsh-only exception. |
| Is hot replacement required? | Add the dylib variant; otherwise rebuild the host. |

## Work order

1. Tier 0 plugin, no core changes.
2. Tier 1 session extraction, only if Workspace is required.
3. Tier 1 Workspace methods.
4. Tier 2 worker and link, only after the contract gate.
5. Optional dylib packaging.

Each step is its own PR with its own tests. Tier 0 does not wait for the
session refactor.
