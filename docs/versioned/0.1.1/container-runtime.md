# LayerFS 0.1.1 container runtime

> **Status:** Released compatibility record for `v0.1.1`.

The managed-container contract remains the
[0.1.0 container runtime contract](../0.1.0/container-runtime.md): a compatible
Linux container, real `/dev/fuse`, `CAP_SYS_ADMIN`, loopback daemon endpoint,
release-matched daemon and FUSE helpers, bounded resources, mount readiness,
and explicit cleanup. The durable Store remains in the host SDK process.

Release verification includes managed create/start/attach/execute/Commit/
End/stop/remove, post-attachment failure, disconnect cleanup, and a census for
mounts, processes, output readers, Workspaces, Branch leases, and containers.
No runtime image is official until its digest is recorded in the
[artifact manifest](../../../release-notes/0.1.1/artifacts.md).
