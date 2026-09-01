# LayerFS 0.1.0 limitations

> **Status:** Released limitations for LayerFS 0.1.0 Developer Preview.

LayerFS 0.1.0 is a local research release. Use it for evaluation and
development, not as the only copy of important data.

## Distribution

- The CLI and Rust crates are built from a repository checkout.
- There is no prebuilt CLI package or published default runtime image.
- The public programming interface is Rust.

## Storage and durability

- A Client uses one local Store.
- The acknowledgement profile is committed and readable from the same live
  Store process.
- Process-crash, operating-system-crash, power-loss, disaster-recovery, and
  backup guarantees are outside this release.
- Automatic garbage collection and object deletion are outside this release.
- LayerStack and Branch names are immutable.

## Runtime

- Managed execution uses a local Docker-compatible runtime.
- Real FUSE requires `/dev/fuse` and `CAP_SYS_ADMIN` in the container.
- Every execution starts a fresh process; there is no persistent shell or
  worker pool.
- The detached CLI owner does not forward an interactive PTY. CLI automation
  should use `workspace exec`; applications that directly own a terminal may
  use the SDK Shell operation.
- OverlayFS projection is outside this release.

## Scale and performance

- Candidate objects, history, and query results are paged and memory-bounded.
- The Workspace namespace planner constructs complete base and final manifests,
  so tiny-edit planning can scale with the number of visible paths even though
  file-byte reconstruction is incremental.
- Sequential reads through container FUSE cross the host/container transport;
  results depend on the Docker and host environment.
- SDK entity queries are cursor-paged. The 0.1.0 CLI convenience query drains
  all pages into one response, so use the SDK for large result sets.
- Performance claims apply only to their recorded hardware, source, workload,
  cache policy, and acknowledgement boundary. See the
  [0.1.0 benchmark report](../../../release-notes/0.1.0/benchmark-results.md).

## Lifecycle responsibilities

- Clean End refuses a dirty Workspace.
- Discard must be explicit when abandoning uncommitted state.
- End does not create a Commit.
- Applications should use cleanup guards for Workspaces, executions, and any
  specifically created containers.
