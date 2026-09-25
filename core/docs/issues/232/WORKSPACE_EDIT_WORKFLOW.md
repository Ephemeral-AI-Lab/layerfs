# #232 Workspace Exec, FUSE edit and Commit workflow

> **Status:** Current planning checklist; no release candidate exists.

This diagram shows the **generic shell-command route** and the publication
boundary. The public SDK method is currently `WorkspaceApi::exec(command)`;
there is no separate `WorkspaceApi::shell` method. The daemon launches
`/bin/sh -c` with the mounted Workspace as its working directory. FUSE sees
the file syscalls made by that command, but neither the kernel nor LayerFS
infers a range-replacement intent from its shell text. The #241 ioctl route
is a separate opt-in path used by a cooperating benchmark tool.

```text
HOST / SDK                                      SANDBOX / DOCKER CONTAINER
─────────────────────────────────────────────────────────────────────────────
Project and Branch already exist
Sandbox already created
WorkspaceApi::mount(..., commit_id=None)
       │
       └── authenticated control ──────────────> daemon resolves Branch head
                                                   and mounts FUSE at
                                                   /layerfs/workspace/<id>

WorkspaceApi::exec("arbitrary shell command ...")
       │
       └── SandboxOwner control call ──────────> daemon: WorkspaceExec
                                                   │
                                                   └── /bin/sh -c "command ..."
                                                       cwd = mounted Workspace
                                                               │
                         command opens /layerfs/workspace/<id>/note
                         command chooses its own file syscalls:
                           write/pwrite  → FUSE WRITE
                           truncate     → FUSE SETATTR/size path
                           rename       → FUSE RENAME
                           explicit ioctl → FUSE IOCTL (opt-in only)
                                                               │
                                                               ▼
                                                   Linux VFS → FUSE callback
                                                               │
                                                   check operation and handle
                                                               │
                                                   update private Workspace
                                                   according to that callback
                                                               │
                                                               ▼
                                             PRIVATE WORKSPACE OVERLAY
                                             Base / Local / Zero pieces
                                             Local payload and metadata in
                                             /layerfs/private-backing/<id>/...
                                                               │
                         Exec returns status and output       │
                         this edit has not moved Branch head   │

WorkspaceApi::commit(workspace_id)
       │
       └── authenticated control ──────────────> daemon: WorkspaceCommit
                                                   │
                                                   ├── capture private overlay
                                                   ├── lower changed pieces
                                                   ├── send EditFile and
                                                   │   replacement bytes to host
                                                   ├── save portable metadata
                                                   └── HistoryCommand::Commit
                                                               │
                                                               ▼
HOST SERVICE                               C1 constructs changed content
                                           → C2 saves canonical CAS objects
                                           → C5 checks expected Branch head
                                           → publishes Commit and Branch head
                                                               │
                         typed Commit result <─────────────────┘
```

## What the private overlay owns

Docker mounts **one named volume per Sandbox** at `/layerfs`. The volume is
real writable storage, not another name for FUSE. The FUSE mountpoint and each
Workspace's private backing directory are distinct paths within that one
Sandbox volume; there is no Docker volume per Workspace.

```text
Docker named volume <container>-root → /layerfs
  /layerfs/workspace/<id>/          FUSE mount seen by the command
  /layerfs/private-backing/<id>/   private payload and overlay metadata

Logical file after a 4 KiB middle overwrite:
  [ Base: unchanged prefix ][ Local: new 4 KiB ][ Base: unchanged suffix ]

Read through FUSE:
  Base  → selected canonical bytes from the host Service, as needed
  Local → private backing in the Sandbox volume
  Zero  → synthesized zero bytes
```

Successful FUSE mutations change the mounted Workspace view and its local
revision; they do **not** publish a Commit or move the Branch head. Commit
captures the private view, saves changed content and metadata through the
host Service, and advances History only if the expected Branch head still
matches. An unmount detaches FUSE; Sandbox deletion removes the owned
container and its named volume through the public SDK.

The [#241 rollout](../241/ROLLOUT_PHASE1.md) records the separate Linux
**4 KiB opt-in ioctl** proof. The later [#232 v3 campaign](evidence/phase2-all-ioctl/REPORT.md)
used a cooperating tool and an implemented staged ioctl for larger payloads.
Its 56 functional results do not establish arbitrary-shell performance. The
[mounted POSIX diagnostic and corrected phases](SHELL_ROUTE_CORRECTION.md)
show the ordinary WRITE route and keep the two claims separate.
