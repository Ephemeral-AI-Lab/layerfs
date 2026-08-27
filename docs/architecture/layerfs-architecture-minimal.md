# Architecture

**One canonical filesystem. Two persistence boundaries.**

```text
1 · CLIENTS
────────────────────────────────────────────────────────────────────────
Agents · applications · CLI                    Immutable snapshot reads
Bash · Git · editors · builds · tests           stat · list · read_range
                                  │
                                  │ begin_operation
                                  ▼

2 · PRIVATE OPERATION
────────────────────────────────────────────────────────────────────────
OperationWorkspace · temporary workspace wrapper
┌──────────────────────────────────────────────────────────────────────┐
│ Direct              Linux FUSE              APFS                    │
│ no path             private mount + spool   private physical view   │
│ one exact base · recovery · private changes                         │
│ quiescence · bounded spool · cleanup · candidate disposition        │
└──────────────────────────────────────────────────────────────────────┘
                                  │
                                  │ quiesce → candidate transition
                                  ▼

3 · CANONICAL MODEL
────────────────────────────────────────────────────────────────────────
Canonical Core
resolve · read · mutate · diff · merge      FastCDC · CAS · COW · RootId
platform-free · path-free · SQLite-free
                                  │
                                  │ OperationCommit
                                  ▼

4 · WORKING AUTHORITY
────────────────────────────────────────────────────────────────────────
WorkingStore                               WorkingRecorded or Conflict
Branch: OperationVersion ──▶ OperationVersion ──▶ HEAD
nested child Branches · immediate-parent merge · rollback
                                  │
                                  │ explicit Push / Fetch
                                  ▼

5 · SHARED DURABILITY
────────────────────────────────────────────────────────────────────────
Authenticated service  ───────────────────────────────▶  DurableStore
missing objects and accepted history only

Durable Branch:     OperationVersion ──▶ OperationVersion ──▶ HEAD
LayerStack:         immutable Layer ──▶ immutable Layer ──▶ HEAD
                    separate LayerStackMerge · backup · restore
```

## Boundaries

- Direct access, Linux FUSE, and APFS materialization use the same canonical
  logical core.
- `OperationWorkspace` is temporary and host-local. Its paths, markers, mounts,
  spools, and native files never enter canonical identity or Sync.
- `OperationCommit` advances the nearby, disk-backed WorkingStore; it does not
  perform an implicit Durable RPC.
- A Branch is an active line of Operation history. `OperationCommit` advances
  its exact expected head; a stale head preserves the candidate as a conflict.
- `Push` may create or advance a durable Branch without changing a LayerStack.
  `LayerStackMerge` is a separate exact-head promotion of a Branch result into
  that Branch's inherited originating LayerStack.
- Only explicit `Push` and `Fetch` cross the authenticated service boundary.
- Sync transfers canonical objects and accepted history—not paths, mounts,
  spools, native files, processes, or SQLite pages.

## Branch and LayerStack

| | Branch | LayerStack |
|---|---|---|
| Represents | Active, continuing work | Promoted immutable Layers |
| Head names | `OperationVersion` + `RootId` | `Layer` + `RootId` |
| Advanced by | `OperationCommit`, child merge, rollback, or durable Branch Push | Separate `LayerStackMerge` or rollback |
| Nesting | Child Branches may nest recursively and merge only to their immediate parent | Every descendant inherits one originating LayerStack |
| After merge | Source Branch remains active | Exact stack head advances or conflicts |

`Branch Push` and `LayerStackMerge` are deliberately independent. Pushing work
does not promote it into the LayerStack, and promoting a Branch result does not
delete or close the source Branch.

## Visual direction

- Black and warm white only; one muted accent may identify the Push/Fetch path.
- Large sans-serif layer titles; monospace only for API and command names.
- Thin rules and arrows; no gradients, shadows, mascots, icons, or decorative
  infrastructure logos.
- Five equal-width horizontal sections with generous vertical whitespace.
- Keep the component count low: one primary concept per persistence boundary.
