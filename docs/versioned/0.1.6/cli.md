# LayerFS 0.1.6 CLI reference

> **Status:** LayerFS 0.1.6 Developer Preview manual.

The `layerfs` binary is a thin adapter over the public Rust SDK. It stores no
application SQL and has no alternate storage path.

## Invocation

```text
layerfs [--json] <command>
layerfs --help
layerfs --version
```

Set `LAYERFS_CONTEXT` to select the context file. A context records one Store
path; use an absolute path so commands remain independent of the current
directory. The CLI keeps active Workspace workers in one local owner process
so separate CLI invocations can address the same ephemeral Workspace and
execution IDs.

`layerfs --version` prints `layerfs 0.1.6`. The CLI surface is unchanged from v0.1.5; this release has no compaction
command: there is no `layerfs-store-compact` binary and no CLI route that
rewrites an existing Store.

## Store and context

```text
layerfs db create <store-path>
layerfs db connect <store-path>
layerfs context use --store <store-path>
layerfs context show
```

`db create` refuses an existing path and produces a schema-10 Store.
`db connect` verifies an existing supported Store without promoting it.

## Managed containers

```text
layerfs container create --name <name> --image <image>
  [--memory-mib <n>] [--cpus <n>] [--pids-limit <n>]
layerfs container start <name-or-id>
layerfs container status <name-or-id>
layerfs container stop <name-or-id>
layerfs container remove <name-or-id>
```

The defaults are 512 MiB, 2 CPUs, and 512 PIDs. These product defaults differ
from the limits used in individual benchmark campaigns. Remove requires a stopped
container. See [Container runtime](container-runtime.md) for the security and
lifecycle contract.

## LayerStacks and Layers

```text
layerfs layerstack init --name <name> --empty
layerfs layerstack init --name <name> <directory>
layerfs layerstack diff --from <layer-id> --to <layer-id>
layerfs layerstack add <branch-id>
```

`layerstack add` publishes the Branch head as the next immutable Layer when it
contains content beyond its base.

## Branches

```text
layerfs branch fork --name <name> --layer <layer-id>
layerfs branch fork --name <name>
  --branch <source-branch-id> --commit <source-commit-id>
layerfs branch diff --branch <branch-id>
  --from <commit-id> --to <commit-id>
layerfs branch diff --branch <branch-id> --layer <layer-id>
```

Fork creates a new Branch ID and reuses the selected immutable root.

## Workspaces and executions

```text
layerfs workspace create <branch-id> --at <absolute-path>
  [--container <name-or-id>] [--projection fuse|materialize]
layerfs workspace exec <workspace-id> -- <program> [arguments...]
layerfs workspace shell <workspace-id>
layerfs workspace output <execution-id> [--follow]
layerfs workspace stop <execution-id>
layerfs workspace conflicts <workspace-id> [--after <cursor>]
layerfs workspace resolve <workspace-id> <conflict-id>
  --branch|--layer|--working-tree
layerfs workspace commit <workspace-id>
layerfs workspace end <workspace-id> [--discard]
```

Projection selection defaults to FUSE for containers and Linux hosts, and to
materialization on macOS. Explicit `--projection materialize` selects the host
path without FUSE. Container placement requires FUSE.

On a live FUSE Workspace, Commit captures a filesystem-operation boundary while
commands may remain running on the same mount, cwd and handles. Conflicting
operations wait across the cut and resume afterward. This is not an
application-level transaction boundary for the whole command. A materialized
Workspace still requires active executions to finish before Commit; a busy
Commit returns `Busy` rather than publishing.

End never commits implicitly. Finish or stop active executions before End;
`--discard` abandons dirty state but does not bypass execution cleanup.

`workspace output --follow` waits until the execution has a terminal receipt.
`workspace stop` targets the exact execution and may be called from another
CLI process.

The SDK can own an interactive terminal directly. The detached CLI owner in
0.1.6 does not forward the caller's PTY; use `workspace exec -- /bin/sh -c
'<command>'` for CLI automation.

## Monitor and queries

```text
layerfs monitor snapshot
layerfs monitor analyze-dedup

layerfs query layerstacks
layerfs query layers
layerfs query branches [--layerstack <layer-stack-id>]
layerfs query commits
layerfs query workspaces
layerfs query monitor
```

Store retrieval is paged internally, but the 0.1.6 CLI drains every page into
one response. Use the SDK query cursor for caller-bounded consumption of large
entity sets. In 0.1.6, `--json` emits an envelope with `schema_version: 4`, but
the `result` field contains the same preview text representation as ordinary
CLI output. The envelope is bounded and valid JSON; the operation-specific
result string is not a stable typed machine API.

## API and version boundaries

Range-edit batching and presentation recovery are Rust SDK operations; the CLI
has no corresponding range-edit or recovery command. `workspace commit` uses
`commit_workspace_session` and prints its result. Applications that must inspect
`presentation_failed` should use the [SDK status API](sdk.md#commit-and-recovery).

Use the CLI and its context owner from the same release. Finish active sessions
before upgrading a running owner. The JSON envelope's `schema_version: 4` is an
output-envelope version, not the SQLite Store schema version. Store compatibility
is specified in the [storage manual](storage-format.md).
