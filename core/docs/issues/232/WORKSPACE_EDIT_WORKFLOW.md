# #232 Workspace Exec, FUSE edit and Commit workflow

> **Status:** Current planning checklist; no release candidate exists.

This diagram shows the implemented #241 Linux **4 KiB inline range ioctl**
route and the publication boundary for #232. The public SDK method is
`WorkspaceApi::exec(command)`; the daemon currently launches `/bin/sh -c`
with the mounted Workspace as its working directory. A cooperating tool opens
the mounted file and issues the ioctl. LayerFS does not infer a range edit
from shell text or automatically convert an ordinary POSIX write.

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

WorkspaceApi::exec("edit-tool ...")
       │
       └── SandboxOwner control call ──────────> daemon: WorkspaceExec
                                                   │
                                                   └── /bin/sh -c "edit-tool ..."
                                                       cwd = mounted Workspace
                                                               │
                         tool opens /layerfs/workspace/<id>/note
                         tool requests STATE, then range EDIT ioctl
                         tool checks STATE, fstat and boundary readback
                                                               │
                                                               ▼
                                                   Linux VFS → FUSE callback
                                                               │
                                                   check handle and stamp
                                                               │
                                                   Workspace range splice
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

The splice changes the mounted Workspace view and increments its local
revision; it does **not** publish a Commit or move the Branch head. Commit
captures the private view, saves changed content and metadata through the
host Service, and advances History only if the expected Branch head still
matches. An unmount detaches FUSE; Sandbox deletion removes the owned
container and its named volume through the public SDK.

The [#232 unified plan](UNIFIED_IOCTL_IMPLEMENTATION_PLAN.md) proposes a
versioned staged carrier for replacements above the current 4 KiB inline
limit. Staged DATA fragments would remain private until one APPLY creates one
Workspace edit. That carrier and the new all-56-case scenario are **planned,
not implemented**. The proposed ioctl replacement limit is 8 MiB; the
file/result limit remains 4 GiB. The [#241 rollout](../241/ROLLOUT_PHASE1.md)
records the implemented Linux proof and its performance-evidence limits.
