# LayerFS 0.1.0 Rust SDK

> **Status:** Released Rust SDK reference for LayerFS 0.1.0 Developer Preview.

The SDK package is available from this repository:

```toml
[dependencies]
layerfs-sdk = { path = "/absolute/path/to/layerfs/crates/layerfs-sdk" }
```

## Managed container lifecycle

```rust
ContainerManager::open(runtime_root)
ContainerManager::create(request)
ContainerManager::start(name_or_id)
ContainerManager::connect(name_or_id)
ContainerManager::status(name_or_id)
ContainerManager::stop(name_or_id)
ContainerManager::remove(name_or_id)
RunningContainer::binding()
ContainerBinding::container_id()
```

Principal container types:

```rust
ContainerCreate
ContainerLimits
CreatedContainer
RunningContainer
ContainerStatus
ContainerBinding
ContainerError
```

Container setup is concrete Docker lifecycle support. `create` configures a
stopped non-privileged container with `/dev/fuse`, `CAP_SYS_ADMIN`, loopback
daemon publication, no host binds, and bounded memory, CPU, and PID settings.
`start` authenticates the daemon and returns an exact immutable container
binding suitable for `Client::connect_with_container`.

## Client construction

```rust
LayerStackStore::create(path)
LayerStackStore::connect(path)
Client::connect(store: Arc<LayerStackStore>)
Client::connect_with_container(store: Arc<LayerStackStore>, binding: ContainerBinding)
```

One `Client` owns one Monitor and one Workspace manager for one Store.

## Durable operations

```rust
Client::initialize_layerstack(name, source)
Client::fork_branch(name, source)
Client::diff(request)
Client::add_layer(branch_id)
```

Principal value types:

```rust
EntityName
LayerStackId
LayerId
BranchId
CommitId

LayerStackInitialization::Empty
LayerStackInitialization::Directory(path)

LocalForkSource::Layer { layer_id }
LocalForkSource::Branch { branch_id, commit_id }

DiffRequest::Layers { from_layer_id, to_layer_id }
DiffRequest::BranchCommits { branch_id, from_commit_id, to_commit_id }
DiffRequest::BranchLayer { branch_id, layer_id }
```

`OperationHandle::next_diff_page` yields at most 128 Diff entries at a time.

## Workspace and execution operations

```rust
Client::create_workspace_session(request)
Client::workspace_conflicts(workspace_id, cursor)
Client::resolve_workspace_conflict(workspace_id, conflict_id, choice)
Client::commit_workspace_session(workspace_id)
Client::end_workspace_session(workspace_id, mode)
Client::exec_workspace_session(workspace_id, argv)
Client::shell_workspace_session(workspace_id)
Client::workspace_output(execution_id)
Client::stop_workspace_execution(execution_id)
Client::active_workspace_count()
Client::active_execution_count()
```

Execution arguments use `NonEmpty<Vec<OsString>>`. `OutputReader::read(after,
follow)` returns bounded output pages and the next sequence cursor.

Conflict choices are `ResolveChoice::Branch`, `ResolveChoice::Layer`, and
`ResolveChoice::WorkingTree`. End modes are `EndWorkspaceMode::Clean` and
`EndWorkspaceMode::Discard`.

## Queries and monitoring

```rust
Client::query(query)
Client::monitor_snapshot()
Client::analyze_dedup()
```

`QueryKind` contains `LayerStacks`, `Layers`, `Branches`, `Commits`,
`Workspaces`, and `Monitor`. `Query::limit` accepts the product page limit;
`QueryPage::into_next_query` carries its opaque continuation forward.

## Minimal host example

```rust
use layerfs_sdk::{
    Client, CreateWorkspaceSession, EndWorkspaceMode, EntityName,
    LayerStackInitialization, LayerStackStore, LocalForkSource,
    WorkspacePlacement, WorkspaceProjection,
};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::current_dir()?.join(".layerfs-sdk");
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
    std::fs::write(view.join("hello.txt"), b"hello\n")?;
    client.commit_workspace_session(workspace.id)?;
    client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    Ok(())
}
```

Use a cleanup guard in long-running applications so errors cannot strand a
Workspace or a specifically created container.
