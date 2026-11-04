# Flows

[Architecture](../README.md) › Flows

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

A flow follows one operation across layers, in the order it happens. Each step
links to the component that explains it.

| Flow | What it follows |
| --- | --- |
| [A call, end to end](call.md) | From a JS, Python or CLI call to its result: the call table, control, routing, and the process the call runs in. |
| [Project Init](init.md) | `project.init`: reading a native directory and publishing the project's first Commit. |
| [A Workspace session](workspace.md) | `sandbox.create`, `workspace.mount`, ordinary file access, `exec` and `unmount`. |
| [A Commit](commit.md) | `workspace.commit`: capture, construction, publish, and install of the new base. |
| [Fork and merge](merge.md) | `branch.fork`, changes on the new Branch, `branch.merge` and `branch.publish`. |
| [Backup and restore](backup.md) | `backup.create` and `backup.restore`: from a Store to a bucket and back. |

---

← [Front ends: Python](../api/front-ends/python.md) · [Up: Architecture](../README.md) · [Flow: A call, end to end](call.md) →
