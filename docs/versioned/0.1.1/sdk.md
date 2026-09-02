# LayerFS 0.1.1 Rust SDK reference

> **Status:** Released compatibility record for `v0.1.1`.

The 0.1.1 release preserves the documented Client, Store, LayerStack,
Layer, Branch, Commit, Workspace, Monitor, query, diff, execution, output,
reconciliation, and container lifecycle behavior in the
[0.1.0 SDK reference](../0.1.0/sdk.md).

Initialization, Commit planning, Workspace bootstrap, and read-ahead changes
are private implementation details. Applications must not depend on benchmark
diagnostics, internal object admission, or private Store handles.
