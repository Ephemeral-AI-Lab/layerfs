# Project and Workspace API contracts

> **Status:** Project Init implemented in Core; Workspace and exec remain proposals.

This document defines the agent-facing vocabulary. `init_project` is a current
SDK export. Workspace and exec signatures describe future behavior.

## `init_project(project_name, path)` — first implementation

Inputs are an authority-local project name and one host-visible directory.
The path identifies the source to scan **inside** the call. The caller does not
supply a LayerStack ID, scope seed, pre-saved file roots, worker count or Store
path. The backend validates the source, allocates identities, builds file
content and portable metadata through C1, saves through C2, and publishes one
C5 genesis LayerStack. The result contains the project/LayerStack ID, genesis
Layer ID, root ID and root serial. It creates no Branch or Workspace.
For a fresh local authority, `layerfs_sdk::Host::create` owns Store/history
setup and lends `host.client()`; the Init call still takes only name and path.

The implementation reuses the production native-directory importer. The
current [command](../../crates/layerfs-bridge/src/contract/history.rs) is
`HistoryCommand::ImportNativeDirectory`; the similarly named `InitLayerStack`
uses a pathless manifest with pre-saved file roots and cannot implement this
method. The current [Service](../../crates/layerfs-service/src/owner.rs) has a
single startup-bound `import_root`; the host-direct SDK passes a validated
source binding to one authorized call without changing that field. Source
scan, file reads, construction, saves and C5 publication stay inside the
public operation boundary.

Success means the returned root and genesis record were published. A known
failure, unknown outcome and cleanup failure stay distinguishable; the SDK
does not automatically repeat a possibly published Init. Recovery may query
the authority-supplied project identity/name. It never treats a timed-out
request as proof that no project was created.

The first implementation proves the method through the 100-file/5 MB and
1,000-file/20 MB `init_namespace` fixtures. Use fresh output for each, check
the returned LayerStack and root, then reopen the Store/history and verify
every path, metadata value, size and file SHA-256 through public product
readers. Retain any failed attempt. These are functional proof selections;
they do not replace #231's cache-qualified performance and four-tier gate.

## Workspace operations — signatures only in the first slice

| Agent method | Intended meaning | Current Core boundary |
| --- | --- | --- |
| `project.mount_workspace()` | Select/create the writable Branch, provision or select a Linux daemon, attach and mount, return exact Workspace ID/incarnation and execution location. | Existing daemon startup selects a fixed profile; control `WorkspaceMount` only remounts an attached Workspace. |
| `workspace.commit()` | Capture and publish one explicit Commit; return committed/up-to-date or typed retained/unknown failure. | Daemon Workspace Commit control. |
| `workspace.unmount()` | Detach FUSE while retaining Workspace state. No implicit commit, discard or close. | Daemon Workspace Unmount control. |
| `workspace.close_clean()` | Release only a clean, unmounted Workspace. | Daemon Workspace CloseClean control. |
| `workspace.exec(shell_command)` | Run a shell where the Workspace is mounted, with that mount as working directory; return exit code and bounded output. No implicit commit. | **Not implemented in Core.** Requires authenticated execution-side ownership, deadline, cancellation and output bounds. |

`mount_workspace` must do more than forward the current daemon `WorkspaceMount`
opcode: initial daemon launch/selection is not an arbitrary mount RPC today.
Writable mounting also requires a Branch, while Init publishes only a genesis
LayerStack. The first API issue may define the method signatures and return
explicit `Unsupported` placeholders, but must not claim these operations work.
Future implementation owns exact placement, authentication and process
lifecycle. Exec belongs beside the mounted Workspace, not in the host Service.

## Dependency direction

```text
MCP / CLI (later)
       |
       v
agent SDK → API contract → existing bridge types
       |                    |
       +→ host Service ─────+→ C1/C2/C5          project Init/history
       +→ daemon control ───+→ Workspace/FUSE     mount/commit/unmount
       +→ execution-side daemon                 exec, once implemented
```

MCP and CLI adapters may format input and output, but they must not create a
second importer, SQL writer, mount lifecycle or shell protocol. The shared API
must report what the backend actually completed. No new numeric performance
claim follows from adding this facade.
