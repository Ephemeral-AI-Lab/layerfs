# LayerFS 0.1.2 container runtime

> **Status:** Release candidate draft for LayerFS 0.1.2.

The managed-container contract remains the
[0.1.1 container runtime contract](../0.1.1/container-runtime.md): a compatible
Linux container, real `/dev/fuse`, `CAP_SYS_ADMIN`, loopback daemon endpoint,
release-matched daemon and FUSE helpers, bounded resources, mount readiness,
and explicit cleanup. The durable Store remains in the host SDK process.

Release verification includes managed create/start/attach/execute/Commit/
End/stop/remove, post-attachment failure, disconnect cleanup, and a census for
mounts, processes, output readers, Workspaces, Branch leases, and containers.
No runtime image is published as a v0.1.2 release artifact.
