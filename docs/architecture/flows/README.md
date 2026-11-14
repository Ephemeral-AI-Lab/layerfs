# Flows

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

A flow follows one operation across layers, in the order it happens; each step links to the topic that explains it.

```text
 caller --> api: Calls --> control: where it runs --> engine --> Store
                                     |
                                     +--> machine worker (sqlite elsewhere)
                                     +--> Sandbox daemon (Workspace calls)
```

| Flow | What it follows |
| --- | --- |
| [A call, end to end](call.md) | From a JS, Python or CLI call to its result: the call table, control, and the process the call runs in. |
| [Project Init](init.md) | `project.init`: reading a native directory and publishing the project's first Commit. |
| [A Workspace session](workspace.md) | `sandbox.create`, `workspace.mount`, ordinary file access, `exec` and `unmount`. |
| [A Commit](commit.md) | `workspace.commit`: capture, construction, publish, and install of the new base. |
| [Fork and merge](merge.md) | `branch.fork`, changes on the new Branch, `branch.merge` and `branch.publish`. |

Backup and restore is followed in [Backups](../control/backups/README.md).

## Related

- [Architecture](../README.md): the four layers and every topic.
- [Control: where each call runs](../control/README.md#where-each-call-runs): the one routing table every flow relies on.
- [Glossary](../glossary.md)
