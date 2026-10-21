# Rust rutis plugin: minimal tiers

> **Status:** Proposal; target LayerFS 1.0.0; not a released contract.

Only three things are added, in this order:

| Tier | Adds | LayerFS changes | Process |
| --- | --- | --- | --- |
| 0 | `fs.*`, `branch.*`, `call` over an existing Store | None | in-process Rust plugin |
| 1 | `workspace.*` | Reuse/extract the existing session layer | in-process Rust plugin |
| 2 | Remote consumers | None beyond Tier 1 | remote Rust worker + node link |

Everything else is out of scope until asked for: Init, operator, UI, background
processes, multi-project workers, admin surface, health protocol, dylib
packaging.

## Host compatibility

| Host | Tier 0/1 | Tier 2 |
| --- | --- | --- |
| Rust app embedding `rutis` | Yes | Yes |
| Stock `rutis-host` | No; it does not load Rust built-ins | Yes, as a peer to a Rust worker |
| `rutis-cli` / `rutis-dylib` | Only as a dylib variant | Same |

## Shared rules

- The process running the LayerFS session opens the Store directly.
- One call once: no retry, replay, refresh or busy handler.
- No implicit Commit, unmount or discard.
- One Workspace's calls are sequential; different Workspaces are parallel.
- Linux only. macOS stays dsh-only.
- Rust-only by default. Cross-process export makes the Store call a proxy and
  needs an explicit owner decision (Tier 2).

## Decision gates

| Gate | Effect |
| --- | --- |
| Is Workspace support required? | If no, ship Tier 0 and stop. |
| May another process call Store services? | If no, Tier 2 is not shipped. |
| Is hot replacement required? | Optional dylib variant; otherwise rebuild the host. |
| Is macOS required? | No, unless the owner changes the dsh-only exception. |

## Work order

1. Tier 0 plugin: existing Store only.
2. Tier 1: session extraction, only if Workspace is required.
3. Tier 2: worker and link, only after the contract gate.

Each step is its own PR. Tier 0 does not wait for the session refactor.
