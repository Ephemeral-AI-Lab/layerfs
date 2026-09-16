# LayerFS 0.1.6 Rust SDK

> **Status:** LayerFS 0.1.6 Developer Preview manual.

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
LayerStackStore::upgrade_format(path)
Client::connect(store: Arc<LayerStackStore>)
Client::connect_with_container(store: Arc<LayerStackStore>, binding: ContainerBinding)
```

One `Client` owns one Monitor and one Workspace manager for one Store. `create`
produces a schema-10 Store; `connect` validates and opens a supported existing
Store without promoting it. `upgrade_format` is the explicit offline promotion
of a closed schema-7/8/9 Store to schema 9 and is unrelated to Store creation;
see the [storage contract](storage-format.md#compatibility-and-migration).
There is no compaction entry point in this release.

**The public SDK surface is unchanged in v0.1.6.** The only additions to `Client`
since v0.1.5 are two methods behind the `test-instrumentation` feature
(`arm_remote_verification_fault`, `take_remote_verification_fault_receipt`), which
exist so a fault-injection proof can arm and read back a one-shot append fault in
the *sandbox* owner, where the payload append now happens. Default builds see the
same surface as v0.1.5, and no existing request, outcome or error type changed.

## Published Store operations

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
    let status = client.commit_workspace_session_with_status(workspace.id)?;
    if status.presentation_failed {
        client.recover_workspace_presentation(workspace.id)?;
    }
    match status.result {
        layerfs_sdk::WorkspaceCommitResult::Created { .. }
        | layerfs_sdk::WorkspaceCommitResult::UpToDate { .. } => {}
        other => return Err(format!("Commit did not publish: {other:?}").into()),
    }
    client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    Ok(())
}
```

Use a cleanup guard in long-running applications so errors cannot strand a
Workspace or a specifically created container.

## File-range edits

The public exports include `WorkspaceFileRangeEdit` and
`WorkspaceFileReplacement::{Inline(Vec<u8>), Zero(u64)}`. The edit fields are:

```rust
WorkspaceFileRangeEdit {
    workspace_id,
    path: "hello.txt".to_owned(),
    start: 0,
    delete_len: 5,
    replacement: WorkspaceFileReplacement::Inline(b"world".to_vec()),
}
```

Pass one edit to `Client::edit_workspace_file_range(edit)`, or a non-empty
`Vec<WorkspaceFileRangeEdit>` to `Client::edit_workspace_file_ranges(edits)`.
Every member must address the same Workspace and file. Each member replaces
`delete_len` bytes at `start`; `Zero` inserts a logical zero range. A rejected
batch member leaves the file unchanged. Edits are prepared as one batch before
the final live view is installed. Runtime/cache reconciliation failures are
reported; do not treat an infrastructure error as a successful edit.

An equal-length overwrite of already-committed content is retained as one base
root plus one bounded splice descriptor and converted to canonical pieces when
the edit is captured; other shapes retain pending per-piece state. The
representation is internal and does not change the SDK grammar, the resulting
canonical identity or the acknowledged edit semantics.

Live FUSE edits use the same authoritative owner as mounted filesystem calls.
The owner coordinates SDK changes, mapped pages and ordinary writes at a
filesystem-operation cut. Materialized projections retain their execution and
writer-quiescence requirements and can return `WorkspaceBusy` while commands
remain active. Range edits are SDK-only; no equivalent CLI grammar was added.

## Commit and recovery

```rust
Client::commit_workspace_session(workspace_id)
Client::commit_workspace_session_with_status(workspace_id)
Client::recover_workspace_presentation(workspace_id)
```

`WorkspaceCommitResult` is one of `Created { previous_head, commit_id }`,
`UpToDate { head }`, `Busy`, or `HeadMoved { expected, actual }`. An `Ok` return
containing `Busy` or `HeadMoved` is not a newly published Commit.

On a live FUSE projection, running commands and open writable handles do not by
themselves prevent Commit. The owner drains filesystem operations and required
kernel/backing work, freezes known state, publishes it, installs the checkpoint
into the same live nodes, and resumes later mutations. Mount, cwd and handle
identity continue. The cut captures filesystem state; it does not atomically
capture the application-level intent of a running command. Materialized
projections still return `Busy` for active executions and wait for writers.

`WorkspaceCommitStatus` contains `result` and `presentation_failed`. When
publication succeeds but updating/resuming the projection fails, `result` remains
authoritative. Do not retry an already-published Commit as if publication failed.
Finish or stop active executions, then call `recover_workspace_presentation`
before continuing. Recovery requires a presentation failure and no active
executions; it can recreate an ended projection and finish pending installation.
The legacy `commit_workspace_session` returns only `status.result`, so use the
status-bearing API when presentation health matters.

A successful live write may still own buffered bytes awaiting a backing fence.
Fsync and Commit perform their required acknowledgements and error checks. These
operations do not add a crash or power-loss durability guarantee beyond the
[Store contract](storage-format.md) and [limitations](limitations.md).

Finish or stop active executions before `end_workspace_session`. Clean End
requires clean state; Discard explicitly abandons uncommitted state and supports
cleanup of a failed owner. Neither mode publishes a Commit implicitly.

## Resource observations and compatibility

`start_workspace_resource_sample(workspace_id)` and
`finish_workspace_resource_sample(workspace_id, t0_unix_ns, t3_unix_ns,
uncertainty_ns)` provide container resource observations through the bound
daemon. The sample clock, cgroup receipts and diagnostic counters have distinct
host/container scopes and are not portable performance guarantees. Build the
SDK, daemon and runtime from the same release. The `test-instrumentation` feature
and `verification_*` helpers belong to verification, not application flows.

The documented Client entry points and CLI grammar remain available and unchanged
from v0.1.5. The live runtime architecture changed in 0.1.6: the sandbox owner holds
the live mutable state and the host sees only Commit-time transfer, so old
descriptions of a host-only mutable Workspace, of a paused/quiesced Commit, or of a
fresh helper process per mount do not apply to this release. Do not mix active runtime
components from different versions. See [container runtime](container-runtime.md),
[storage](storage-format.md) and the [qualification report](../../../release-notes/0.1.6/verification.md)
for their separate compatibility and evidence boundaries.
