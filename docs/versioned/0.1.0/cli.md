# LayerFS 0.1.0 CLI reference

> **Status:** Release-candidate CLI reference for LayerFS 0.1.0.

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

## Store and context

```text
layerfs db create <store-path>
layerfs db connect <store-path>
layerfs context use --store <store-path>
layerfs context show
```

`db create` refuses an existing path. `db connect` verifies an existing Store.

## Managed containers

```text
layerfs container create --name <name> --image <image>
  [--memory-mib <n>] [--cpus <n>] [--pids-limit <n>]
layerfs container start <name-or-id>
layerfs container status <name-or-id>
layerfs container stop <name-or-id>
layerfs container remove <name-or-id>
```

The defaults are 512 MiB, 2 CPUs, and 512 PIDs. Remove requires a stopped
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

`workspace output --follow` waits until the execution has a terminal receipt.
`workspace stop` targets the exact execution and may be called from another
CLI process.

The SDK can own an interactive terminal directly. The detached CLI owner in
0.1.0 does not forward the caller's PTY; use `workspace exec -- /bin/sh -c
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

Store retrieval is paged internally, but the 0.1.0 CLI drains every page into
one response. Use the SDK query cursor for caller-bounded consumption of large
entity sets. In 0.1.0, `--json` emits an envelope with `schema_version: 4`, but
the `result` field contains the same preview text representation as ordinary
CLI output. The envelope is bounded and valid JSON; the operation-specific
result string is not a stable typed machine API.
