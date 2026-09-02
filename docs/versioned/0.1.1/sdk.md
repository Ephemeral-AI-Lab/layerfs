# LayerFS 0.1.1 release-candidate Rust SDK reference

> **Status:** Candidate compatibility record; `v0.1.1` is not published.

The 0.1.1 candidate preserves the documented Client, Store, LayerStack,
Layer, Branch, Commit, Workspace, Monitor, query, diff, execution, output,
reconciliation, and container lifecycle behavior in the
[0.1.0 SDK reference](../0.1.0/sdk.md).

Initialization, Commit planning, Workspace bootstrap, and read-ahead changes
are private implementation details. Applications must not depend on benchmark
diagnostics, internal object admission, or private Store handles. The SDK
examples must be rerun against the exact clean release commit before this
reference becomes normative.
