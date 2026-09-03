# LayerFS 0.1.2 Rust SDK reference

> **Status:** Released compatibility record for `v0.1.2`.

The 0.1.2 release preserves the documented Client, Store, LayerStack,
Layer, Branch, Commit, Workspace, Monitor, query, diff, execution, output,
reconciliation, and container lifecycle behavior in the
[0.1.1 SDK reference](../0.1.1/sdk.md).

The additive v0.1.2 API exports `WorkspaceFileRangeEdit` and
`WorkspaceFileReplacement::{Inline, Zero}`. Use
`Client::edit_workspace_file_range` for one edit or
`Client::edit_workspace_file_ranges` for a non-empty, same-Workspace,
same-file batch. Each edit replaces `delete_len` bytes at `start`; `Inline`
inserts supplied bytes and `Zero` inserts a logical zero range. The batch is
prevalidated and failure-atomic, and publishes one projection refresh only
after the final piece root is ready.

Concurrent execution, Commit, reconciliation, recovery, or projection work can
return `WorkspaceBusy`. The legacy `Client::commit_workspace_session` and its
`WorkspaceCommitResult` remain source-compatible with v0.1.1. Call the additive
`Client::commit_workspace_session_with_status` when presentation health matters:
its `WorkspaceCommitStatus` contains the existing durable `result` plus
`presentation_failed`. If that flag is true, the Commit result is authoritative;
call `Client::recover_workspace_presentation` before continuing and do not
retry the already-published Commit.

The piece tree, physical spool slices, benchmark diagnostics, benchmark-only
initialization seed, presentation-recovery flag, and object admission are
private implementation details. `WorkspaceState`, `WorkspaceCommitResult`,
`OperationFamily`, and `WorkspaceCommitReceipt` retain their v0.1.1 shapes.
