# LayerFS quickstart

LayerFS is a local, content-addressed, copy-on-write filesystem for branchable
agent workspaces. One local SQLite `LayerStackStore` contains named
LayerStacks, immutable Layers, named writable Branches, immutable Commits, and
one deduplicated canonical-object namespace. Workspaces are ephemeral and have
no database.

> [!WARNING]
> LayerFS is a research preview. The Rust SDK, one-Store implementation,
> materialized Workspace path, managed-container FUSE path, and benchmark suite
> are functional. Prebuilt releases, a published runtime image, power-loss
> recovery, and interactive terminal forwarding through the detached CLI
> context owner are incomplete. Do not use LayerFS as the only copy of
> important data.

## 1. Requirements

For Store operations and host-materialized Workspaces:

- macOS or Linux.
- Rust 1.85.1 or newer.
- A local checkout of this repository. The crates are not yet published.

For managed-container FUSE Workspaces:

- Docker Desktop or Docker Engine with a working `docker` CLI.
- Linux `/dev/fuse` support in the Docker VM or host.
- A compatible LayerFS Linux runtime image containing:
  - `/usr/local/bin/layerfs-daemon`;
  - `/usr/local/bin/layerfs-fuse`;
  - `/usr/local/bin/layerfs-daemon-entrypoint`;
  - TCP daemon port `41273`.

LayerFS containers are non-privileged. They receive `/dev/fuse` and
`CAP_SYS_ADMIN`, publish the daemon port only on host loopback, and do not
bind-mount the Store or Workspace payloads.

## 2. Build the CLI

From the repository root:

```bash
cargo build --release -p layerfs-cli
export LAYERFS_BIN="$PWD/target/release/layerfs"
```

Confirm the binary:

```bash
"$LAYERFS_BIN" --version
"$LAYERFS_BIN" --help
```

The examples below use `"$LAYERFS_BIN"`. If the binary is on `PATH`, replace
that with `layerfs`.

## 3. Create one local Store and context

Use an absolute Store path so later invocations do not depend on the current
directory:

```bash
mkdir -p "$PWD/.layerfs"
export LAYERFS_CONTEXT="$PWD/.layerfs/context"

"$LAYERFS_BIN" db create "$PWD/.layerfs/store.sqlite"
"$LAYERFS_BIN" context use --store "$PWD/.layerfs/store.sqlite"
"$LAYERFS_BIN" context show
```

Expected context output:

```text
store=/absolute/path/to/project/.layerfs/store.sqlite
```

One context binds exactly one Store. Changing the context while a Workspace or
managed container context is active is rejected.

## 4. Initialize a LayerStack and Branch

Create an empty LayerStack:

```bash
"$LAYERFS_BIN" layerstack init --name demo --empty
```

The result contains a `layer_stack_id` and `genesis_layer_id`. IDs are
authoritative; names are immutable lookup and presentation metadata.

Alternatively, initialize a LayerStack from a native directory that does not
contain the Store being created:

```bash
mkdir -p "$PWD/import-root"
printf 'hello\n' > "$PWD/import-root/hello.txt"

"$LAYERFS_BIN" layerstack init \
  --name imported \
  "$PWD/import-root"
```

Fork a writable Branch from the genesis Layer:

```bash
"$LAYERFS_BIN" branch fork \
  --name main \
  --layer <genesis-layer-id>
```

The command prints the new Branch ID. Keep it for Workspace operations.

Inspect durable entities:

```bash
"$LAYERFS_BIN" query layerstacks
"$LAYERFS_BIN" query layers
"$LAYERFS_BIN" query branches
"$LAYERFS_BIN" query commits
```

## 5. Fastest setup: host-materialized Workspace

Materialized mode requires no container and is the simplest development path.
The Workspace root must be absolute:

```bash
WORKSPACE_ROOT="$PWD/.layerfs/workspace"

WORKSPACE_ID=$("$LAYERFS_BIN" workspace create <branch-id> \
  --at "$WORKSPACE_ROOT" \
  --projection materialize)

printf 'Workspace: %s\n' "$WORKSPACE_ID"
```

Execute a genuinely fresh process in the Workspace:

```bash
EXECUTION_ID=$("$LAYERFS_BIN" workspace exec "$WORKSPACE_ID" -- \
  /bin/sh -c 'printf "hello from LayerFS\n" > hello.txt; printf done')

printf 'Execution: %s\n' "$EXECUTION_ID"
```

Wait for terminal output and the exit receipt:

```bash
"$LAYERFS_BIN" workspace output "$EXECUTION_ID" --follow
```

Commit the changed frontier:

```bash
"$LAYERFS_BIN" workspace commit "$WORKSPACE_ID"
"$LAYERFS_BIN" query commits
```

End the clean Workspace:

```bash
"$LAYERFS_BIN" workspace end "$WORKSPACE_ID"
```

`workspace end` removes the ephemeral materialized root. The committed content
remains in the Store. End never creates a Commit implicitly.

To discard uncommitted Workspace changes explicitly:

```bash
"$LAYERFS_BIN" workspace end "$WORKSPACE_ID" --discard
```

## 6. Build a development runtime image

LayerFS does not yet publish a default runtime image. The current benchmark
runtime Dockerfile is a compatible development image. It also contains the
benchmark workload, which is harmless but unnecessary for ordinary Workspace
use.

On macOS, build it from the repository root with:

```bash
SOURCE_COMMIT=$(git rev-parse HEAD)
SOURCE_TREE=$(git rev-parse 'HEAD^{tree}')
SOURCE_DIRTY=false
test -z "$(git status --porcelain)" || SOURCE_DIRTY=true
SOURCE_SEAL=$(benchmark/fs-bench-pro/run.sh --source-seal)
WORKLOAD_SHA=$(shasum -a 256 benchmark/fs-bench-pro/workload.rs | awk '{print $1}')

docker build --pull=false \
  --build-arg LAYERFS_SOURCE_COMMIT="$SOURCE_COMMIT" \
  --build-arg LAYERFS_SOURCE_TREE="$SOURCE_TREE" \
  --build-arg LAYERFS_SOURCE_DIRTY="$SOURCE_DIRTY" \
  --build-arg LAYERFS_SOURCE_SEAL="$SOURCE_SEAL" \
  --build-arg WORKLOAD_SOURCE_SHA256="$WORKLOAD_SHA" \
  -f benchmark/fs-bench-pro/Dockerfile.layerfs \
  -t layerfs-runtime:dev \
  .
```

On Linux, replace the `WORKLOAD_SHA` assignment with:

```bash
WORKLOAD_SHA=$(sha256sum benchmark/fs-bench-pro/workload.rs | awk '{print $1}')
```

Container image building is setup and is never part of Workspace lifecycle or
`fs-bench-pro` timing.

## 7. Managed-container FUSE Workspace

Create a stopped, resource-bounded container from the runtime image:

```bash
"$LAYERFS_BIN" container create \
  --name agent-runtime \
  --image layerfs-runtime:dev \
  --memory-mib 512 \
  --cpus 2 \
  --pids-limit 512
```

Inspect the stopped container:

```bash
"$LAYERFS_BIN" container status agent-runtime
```

The status must include:

```text
running=false
privileged=false
fuse=true
sys_admin=true
binds=0
memory=536870912
nano_cpus=2000000000
pids=512
```

Start it and authenticate the daemon:

```bash
"$LAYERFS_BIN" container start agent-runtime
```

Create a real FUSE Workspace inside the container:

```bash
WORKSPACE_ID=$("$LAYERFS_BIN" workspace create <branch-id> \
  --container agent-runtime \
  --at /workspace \
  --projection fuse)
```

The CLI resolves `agent-runtime` to the exact Docker container ID before
calling the SDK. One CLI context binds at most one running managed container.

Execute, follow, Commit, and End:

```bash
EXECUTION_ID=$("$LAYERFS_BIN" workspace exec "$WORKSPACE_ID" -- \
  /bin/sh -c 'printf "container workspace\n" > result.txt; printf executed')

"$LAYERFS_BIN" workspace output "$EXECUTION_ID" --follow
"$LAYERFS_BIN" workspace commit "$WORKSPACE_ID"
"$LAYERFS_BIN" workspace end "$WORKSPACE_ID"
```

Stop and remove the managed container only after all its Workspaces and
executions have ended:

```bash
"$LAYERFS_BIN" container stop agent-runtime
"$LAYERFS_BIN" container remove agent-runtime
```

Container Create and Start are preparation. A measured Workspace lifecycle
starts immediately before `workspace create` and ends after `workspace end`.

## 8. How the CLI retains ephemeral Workspaces

Each ordinary CLI invocation is a fresh process. `workspace create` and
`container start` automatically start one lightweight host context owner that
keeps the public SDK `Client`, Monitor, and Workspace workers alive.

```text
layerfs CLI invocations
        |
        | owner-only local Unix socket
        v
hidden host context owner
  one SDK Client
  one LayerStackStore
  one Monitor
  active Workspace workers
        |
        | mutually authenticated TCP for container mode
        v
prepared Linux container
  layerfs-daemon
  fresh FUSE helper per Workspace
  fresh process per Exec
```

The context owner does not keep a shell, process pool, or payload cache warm.
Host-materialized owners exit after the last Workspace ends. Managed-container
owners exit after `container stop` succeeds.

The local control protocol is bounded to 1,024 arguments, 8 MiB of total
argument bytes, and an 8 MiB response. Its Unix socket lives under an
owner-only `/tmp/layerfs-cli-<uid>` directory and is keyed by the absolute
context path.

## 9. CLI operations

### Global options

```bash
layerfs --help
layerfs --version
layerfs --json <command>
```

### Database and context

```bash
layerfs db create <store-path>
layerfs db connect <store-path>
layerfs context use --store <store-path>
layerfs context show
```

### Managed containers

```bash
layerfs container create --name <name> --image <image> \
  [--memory-mib <n>] [--cpus <n>] [--pids-limit <n>]
layerfs container start <name-or-id>
layerfs container status <name-or-id>
layerfs container stop <name-or-id>
layerfs container remove <name-or-id>
```

### LayerStacks

```bash
layerfs layerstack init --name <name> --empty
layerfs layerstack init --name <name> <directory>
layerfs layerstack diff --from <layer-id> --to <layer-id>
layerfs layerstack add <branch-id>
```

`layerstack add` promotes a Branch head Commit to the next immutable Layer. A
Branch with no Commit has nothing to add.

### Branches

```bash
layerfs branch fork --name <name> --layer <layer-id>
layerfs branch fork --name <name> \
  --branch <source-branch-id> --commit <source-commit-id>
layerfs branch diff --branch <branch-id> \
  --from <commit-id> --to <commit-id>
layerfs branch diff --branch <branch-id> --layer <layer-id>
```

Fork is local and does not copy canonical objects.

### Workspaces and executions

```bash
layerfs workspace create <branch-id> --at <absolute-path> \
  [--container <name-or-id>] [--projection fuse|materialize]
layerfs workspace exec <workspace-id> -- <program> [arguments...]
layerfs workspace shell <workspace-id>
layerfs workspace output <execution-id> [--follow]
layerfs workspace stop <execution-id>
layerfs workspace conflicts <workspace-id> [--after <cursor>]
layerfs workspace resolve <workspace-id> <conflict-id> \
  --branch|--layer|--working-tree
layerfs workspace commit <workspace-id>
layerfs workspace end <workspace-id> [--discard]
```

`workspace output --follow` stays connected until a terminal execution receipt
exists. `workspace stop` can be called concurrently from another CLI process.

The public SDK interactive shell works when the SDK directly owns a terminal.
The detached CLI context owner does not yet forward a caller's PTY, so
`workspace shell` is not a supported interactive CLI path in this preview. Use
`workspace exec -- /bin/sh -c '<command>'` for CLI automation.

### Monitoring

```bash
layerfs monitor snapshot
layerfs monitor analyze-dedup
```

### Queries

```bash
layerfs query layerstacks
layerfs query layers
layerfs query branches [--layerstack <layer-stack-id>]
layerfs query commits
layerfs query workspaces
layerfs query monitor
```

## 10. Rust SDK setup

The SDK is not yet published. In another local Cargo package, point to the
checked-out crate:

```toml
[dependencies]
layerfs-sdk = { path = "/absolute/path/to/layerfs/crates/layerfs-sdk" }
```

### Host-materialized SDK example

```rust
use layerfs_sdk::{
    Client, CreateWorkspaceSession, EndWorkspaceMode, EntityName,
    LayerStackInitialization, LayerStackStore, LocalForkSource,
    WorkspacePlacement, WorkspaceProjection,
};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::current_dir()?.join(".layerfs-sdk-quickstart");
    std::fs::create_dir_all(&root)?;

    let store = Arc::new(LayerStackStore::create(root.join("store.sqlite"))?);
    let client = Client::connect(store)?;

    let initialized = client.initialize_layerstack(
        EntityName::new("demo")?,
        LayerStackInitialization::Empty,
    )?;
    let branch_id = client.fork_branch(
        EntityName::new("main")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;

    let view = root.join("workspace");
    let workspace = client.create_workspace_session(CreateWorkspaceSession {
        branch_id,
        placement: WorkspacePlacement::Host { root: view.clone() },
        projection: Some(WorkspaceProjection::Materialize),
    })?;

    std::fs::write(view.join("hello.txt"), b"hello from LayerFS\n")?;
    println!("{:?}", client.commit_workspace_session(workspace.id)?);
    client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    Ok(())
}
```

### Managed-container SDK example

```rust
use layerfs_sdk::{
    Client, ContainerCreate, ContainerLimits, ContainerManager,
    CreateWorkspaceSession, EndWorkspaceMode, LayerStackStore, NonEmpty,
    WorkspacePlacement, WorkspaceProjection,
};
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

fn run_container(
    store: Arc<LayerStackStore>,
    branch_id: layerfs_sdk::BranchId,
) -> Result<(), Box<dyn std::error::Error>> {
    let containers = ContainerManager::open(".layerfs/runtime")?;
    let created = containers.create(ContainerCreate {
        name: "agent-runtime".to_owned(),
        image: "layerfs-runtime:dev".to_owned(),
        limits: ContainerLimits::default(),
    })?;
    let running = containers.start(&created.name)?;
    let client = Client::connect_with_container(store, running.binding())?;

    let workspace = client.create_workspace_session(CreateWorkspaceSession {
        branch_id,
        placement: WorkspacePlacement::Container {
            container_id: running.id.clone(),
            root: PathBuf::from("/workspace"),
        },
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let execution = client.exec_workspace_session(
        workspace.id,
        NonEmpty::new(vec![
            OsString::from("/bin/sh"),
            OsString::from("-c"),
            OsString::from("printf 'hello\\n' > hello.txt"),
        ])?,
    )?;

    let output = client.workspace_output(execution.id)?;
    let mut after = 0;
    loop {
        let page = output.read(after, true)?;
        after = page.next_sequence;
        if page.exited {
            break;
        }
    }

    println!("{:?}", client.commit_workspace_session(workspace.id)?);
    client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    drop(client);

    containers.stop(&created.name)?;
    containers.remove(&created.name)?;
    Ok(())
}
```

Production code should use an RAII cleanup guard or equivalent error path so a
failed example does not leave its specifically created container behind.

## 11. Public SDK operations

### Container lifecycle

```rust
ContainerManager::open
ContainerManager::create
ContainerManager::start
ContainerManager::connect
ContainerManager::status
ContainerManager::stop
ContainerManager::remove
RunningContainer::binding
ContainerBinding::container_id
```

### Client construction and runtime state

```rust
Client::connect
Client::connect_with_container
Client::active_workspace_count
Client::active_execution_count
```

### LayerStacks, Layers, and Branches

```rust
Client::initialize_layerstack
Client::fork_branch
Client::diff
Client::add_layer
```

### Workspaces and executions

```rust
Client::create_workspace_session
Client::exec_workspace_session
Client::shell_workspace_session
Client::workspace_output
Client::stop_workspace_execution
Client::workspace_conflicts
Client::resolve_workspace_conflict
Client::commit_workspace_session
Client::end_workspace_session
```

### Monitoring and queries

```rust
Client::monitor_snapshot
Client::analyze_dedup
Client::query
```

The query families are:

```rust
QueryKind::LayerStacks
QueryKind::Layers
QueryKind::Branches
QueryKind::Commits
QueryKind::Workspaces
QueryKind::Monitor
```

## 12. Verify the checkout

Format and lint:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

Run the complete bounded native test suite:

```bash
tools/test-fast.sh
```

Run only the real multi-process CLI lifecycle proof:

```bash
cargo test -p layerfs-cli \
  --test v4 \
  standalone_cli_keeps_one_sdk_client_through_workspace_end \
  -- --exact
```

The full managed-container FUSE proof requires Docker and a compatible runtime
image. Container creation, image building, and daemon readiness are setup; the
public performance interval covers Workspace Create, fresh-process Exec,
Commit, and End.

## 13. Reproducibility and current limitations

The following should reproduce semantically but produce new IDs on every run:

- container creation and exact-ID binding;
- daemon mutual authentication;
- real FUSE mount and unmount;
- fresh process execution and terminal receipt;
- Commit visibility;
- missing-only canonical admission and deduplication;
- container Stop and Remove;
- all unit and integration assertions.

LayerStack, Layer, Branch, Commit, Workspace, Execution, and capability IDs are
not expected to be byte-identical across independent runs. Performance is a
statistical range rather than an identical nanosecond value.

Current preview limitations:

- no prebuilt CLI release;
- no published runtime image;
- Rust SDK only;
- local Store and local Docker runtime only;
- no remote Pull, Push, Reference, or Replica modes;
- no interactive PTY forwarding through the detached CLI context owner;
- no power-loss or disaster-recovery guarantee;
- no implicit Commit during Workspace End.

The binding architecture and terminal contracts are in
[`v2-replacement/spec.md`](v2-replacement/spec.md).
