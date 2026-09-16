# LayerFS 0.1.6 quickstart

> **Status:** LayerFS 0.1.6 Developer Preview manual.

This guide creates one Store, one LayerStack, one Branch, and one
host-materialized Workspace. This path needs Rust but not Docker.

## Requirements

- macOS or Linux;
- Rust 1.85 or newer;
- a local checkout of the LayerFS repository.

Use the `v0.1.6` tag or its source archive from the [GitHub release](https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.6).

## Build

From the repository root of that checkout:

```bash
cargo build --locked --release -p layerfs-cli
export LAYERFS_BIN="$PWD/target/release/layerfs"
export LAYERFS_CONTEXT="$PWD/.layerfs/context"
mkdir -p "$PWD/.layerfs"

"$LAYERFS_BIN" --version
```

## Create the Store

Use absolute paths so later commands are independent of the current directory:

```bash
"$LAYERFS_BIN" db create "$PWD/.layerfs/store.sqlite"
"$LAYERFS_BIN" context use --store "$PWD/.layerfs/store.sqlite"
"$LAYERFS_BIN" context show
```

The context output is:

```text
store=/absolute/path/to/checkout/.layerfs/store.sqlite
```

`db create` writes a schema-10 Store with 4 KiB pages.

## Initialize content

Create an empty LayerStack:

```bash
"$LAYERFS_BIN" layerstack init --name demo --empty
```

Save the printed `genesis_layer_id`, then create a Branch:

```bash
"$LAYERFS_BIN" branch fork --name main --layer <genesis-layer-id>
```

Save the printed Branch ID. Names are immutable and follow:

```regex
^[a-z0-9](?:[a-z0-9._-]{0,61}[a-z0-9])?$
```

Typed IDs are the authoritative operation inputs.

To initialize from an existing directory instead:

```bash
mkdir -p "$PWD/import-root"
printf 'hello\n' > "$PWD/import-root/hello.txt"
"$LAYERFS_BIN" layerstack init --name imported "$PWD/import-root"
```

Do not place the Store file inside the imported directory.

## Use a Workspace

```bash
WORKSPACE_ROOT="$PWD/.layerfs/workspace"

WORKSPACE_ID=$("$LAYERFS_BIN" workspace create <branch-id> \
  --at "$WORKSPACE_ROOT" \
  --projection materialize)

EXECUTION_ID=$("$LAYERFS_BIN" workspace exec "$WORKSPACE_ID" -- \
  /bin/sh -c 'printf "hello from LayerFS\n" > hello.txt; printf done')

"$LAYERFS_BIN" workspace output "$EXECUTION_ID" --follow
"$LAYERFS_BIN" workspace commit "$WORKSPACE_ID"
"$LAYERFS_BIN" workspace end "$WORKSPACE_ID"
```

Each `workspace exec` starts a fresh process. After the execution finishes, Commit publishes the captured
filesystem state to the Branch. Inspect its printed result: `Created` or
`UpToDate` indicates success; `Busy` and `HeadMoved` require resolving the
reported condition. End removes the ephemeral projection and never
creates a Commit implicitly.

To abandon uncommitted state explicitly:

```bash
"$LAYERFS_BIN" workspace end "$WORKSPACE_ID" --discard
```

## Inspect the Store

```bash
"$LAYERFS_BIN" query layerstacks
"$LAYERFS_BIN" query layers
"$LAYERFS_BIN" query branches
"$LAYERFS_BIN" query commits
"$LAYERFS_BIN" monitor snapshot
"$LAYERFS_BIN" monitor analyze-dedup
```

For real container FUSE setup, continue with
[Container runtime](container-runtime.md). For all commands, see the
[CLI reference](cli.md).

## Live commands and checkpoints

The walkthrough explicitly uses materialization and waits for execution to end.
For a live FUSE Workspace, Commit can run while commands and writable handles
remain active. It checkpoints a filesystem-operation cut and resumes later
writes as new work. This does not make a running command atomic. See the
[container walkthrough](container-runtime.md) and [SDK lifecycle rules](sdk.md).
