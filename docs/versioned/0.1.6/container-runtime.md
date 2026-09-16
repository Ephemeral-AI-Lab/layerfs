# LayerFS 0.1.6 container runtime

> **Status:** LayerFS 0.1.6 Developer Preview manual.

LayerFS projects live Workspaces through real FUSE inside a prepared Linux
container. In this placement, the Linux daemon owns the shared live Workspace
operation core and mounted filesystem state. The host SDK owns physical backing,
SQLite, canonical construction and Commit publication.

## Runtime image and topology

Use release-matched SDK, CLI, daemon and runtime builds. The daemon owns mounts
directly on its shared live runtime; it does not start a fresh FUSE helper
process for each Workspace. Each Exec still launches the requested program as a
fresh process. Multiple mounted Workspaces retain separate mutable state; bounded
immutable read caching is runtime-owned.

The maintained image contains:

```text
/usr/local/bin/layerfs-daemon
/usr/local/bin/layerfs-fuse
/usr/local/bin/layerfs-daemon-entrypoint
TCP port 41273
/dev/fuse support
```

The entrypoint creates `/run/layerfs/capability` with 32 random bytes and mode
0600, then executes the daemon. The image sets
`LAYERFS_DAEMON_TCP_LISTEN=0.0.0.0:41273`. Managed creation publishes that port to
an ephemeral port on host loopback only. The backing service must also be
reachable from the container at `host.docker.internal`; the documented measured
topology uses Docker Desktop with the SDK/Store on macOS. A Linux Docker Engine
installation must provide that hostname and return connectivity to the host;
container start alone does not prove backing connectivity or FUSE readiness.

## Build a local matched image

From the v0.1.6 checkout with Docker running and Python 3 available:

```bash
cargo build --locked --release -p layerfs-cli
export LAYERFS_BIN="$PWD/target/release/layerfs"
LAYERFS_RUNTIME_IMAGE=$(python3 benchmark/fs-bench-pro/shared/runner.py --build-image)
printf '%s\n' "$LAYERFS_RUNTIME_IMAGE"
```

This build-only entry point uses the maintained
[`Dockerfile.layerfs`](../../../benchmark/fs-bench-pro/Dockerfile.layerfs), records
source/product identity labels and prints the local image tag. It builds the
runtime and workload helper; it does not launch a performance or verification
campaign. The Dockerfile's build-time workload self-check is part of constructing
the image. Keep the resulting image with the matching source checkout. Published
artifacts, if any, are enumerated in the
[release preparation record](README.md); a local
benchmark-infrastructure image tag is not a published release image.

## Create and connect

After creating a Store and context as in the [quickstart](quickstart.md):

```bash
"$LAYERFS_BIN" container create \
  --name agent-runtime \
  --image "$LAYERFS_RUNTIME_IMAGE" \
  --memory-mib 512 \
  --cpus 2 \
  --pids-limit 512

"$LAYERFS_BIN" container status agent-runtime
"$LAYERFS_BIN" container start agent-runtime
```

Managed creation configures `/dev/fuse`, `CAP_SYS_ADMIN`, `privileged=false`, no
host bind mounts, bounded memory/CPU/PIDs, loopback-only daemon publication, and
label `dev.layerfs.managed=true`. The product defaults are 512 MiB, 2 CPUs and
512 PIDs. Accepted values are 64 MiB–64 GiB, 1–256 CPUs and 32–65,535 PIDs.
Benchmark-specific 2 GiB/256-PID limits are distinct from these defaults.

At Start or Connect, the host resolves the exact Docker container ID, copies the
capability into an owner-only runtime directory, and authenticates the owner
connection. Workspace and execution requests bind to the owner and daemon boot.
The Store is not mounted into the container. Image construction, Docker setup
and daemon readiness do not themselves create a Workspace.

For direct Rust integration, use `ContainerManager::open`, `create`, `start` or
`connect`, then pass `RunningContainer::binding()` to
`Client::connect_with_container`. Set `WorkspacePlacement::Container` to that
binding's container ID and the desired absolute container path.

## Create a FUSE Workspace

Replace `<branch-id>` with the Branch ID printed by the quickstart:

```bash
WORKSPACE_ID=$("$LAYERFS_BIN" workspace create <branch-id> \
  --container agent-runtime --at /workspace --projection fuse)

EXECUTION_ID=$("$LAYERFS_BIN" workspace exec "$WORKSPACE_ID" -- \
  /bin/sh -c 'printf "container workspace\n" > result.txt')

"$LAYERFS_BIN" workspace output "$EXECUTION_ID" --follow
"$LAYERFS_BIN" workspace commit "$WORKSPACE_ID"
"$LAYERFS_BIN" workspace end "$WORKSPACE_ID"
```

Successful Workspace creation includes mount readiness. The example waits for
execution, then commits and ends the Workspace. Inspect Commit's result before
End: `Created` and `UpToDate` are successful outcomes; `Busy` or `HeadMoved` need
handling. Container placement requires FUSE, not materialization.

## Sandbox-local mutable state

The sandbox owner holds the live mutable state for a sandbox-owned Workspace:
namespace, file data and a **private packed payload backing** created for that
mount under the daemon's snapshot root, keyed by the Workspace id. The host learns
that state only through Commit-time transfer, and the backing directory is removed
when the Workspace retires. A leftover directory from a crashed mount is refused
rather than reused — the workspace is disposable, its identity is not.

There is no pause or quiesce step: `FREEZE`/`RESUME` survive as wire constants with
no handler in the live owner's dispatch, and the workspace stays continuous across
Commits. Canonical construction runs with one construction worker, so a Commit
publishes with a single producer; `init_namespace` keeps its multi-worker
initialization path.

Bounded memory is part of the contract, not an aspiration: the sandbox spool keeps
a bounded resident window for in-flight payload (measured container residency
≤ 2.6 MiB in the B2 control) instead of holding the payload in page cache, which is
why a Commit-time transfer costs a storage read (~2.1 GiB/s) rather than a
cache-served one (~19 GB/s). Container-scoped memory is the only comparable number
the harness emits; the sandbox *process* RSS is not part of it.

## Live Commit and errors

Commands may remain running across a live FUSE Commit and continue on the same
mount, cwd and open handles. Commit drains a filesystem-operation cut, reconciles
kernel/backing work, publishes the snapshot, updates the existing nodes and
resumes later writes as new dirty work. A command may therefore span several
Commits; its application-level transaction is not automatically atomic.

SDK range edits use the same owner. Kernel cache reconciliation preserves
SDK-edited ranges against delayed stale-page writeback while allowing unrelated
mapped writes to finish. Writable descriptors retain their flush/error path.
Ordinary write acknowledgement can precede backing flush; pending bytes remain
owned and charged, and an equal-length overwrite of committed content is
retained as a bounded base-plus-splice descriptor until capture. Fsync/Commit
fences perform the required backing checks; ordinary write bursts batch a
bounded number of backing fences. Ambiguous backing errors fail the owner rather
than replaying an append.

Use the [SDK status API](sdk.md#commit-and-recovery) to distinguish published
Commit results from projection-update failure. Stop or finish active executions
before presentation recovery. The live implementation uses unmodified upstream
`fuser` 0.18.0. It does not establish power-loss durability or a physical
100-Workspace qualification; see [limitations](limitations.md).

## End, stop and remove

Finish or stop executions, then End each Workspace before stopping its container.
Clean End requires clean state. Use `workspace end <workspace-id> --discard` when
explicitly abandoning uncommitted changes. End never commits implicitly.

```bash
"$LAYERFS_BIN" container stop agent-runtime
"$LAYERFS_BIN" container remove agent-runtime
```

Stop controls Docker lifecycle; it is not a Commit or a substitute for orderly
Workspace cleanup. Remove refuses a running container and removes its copied
capability. The release's measured lifecycle and cleanup evidence are linked from
the [release record](../../../release-notes/0.1.6/README.md); setup, product operations
and independent proofs have separate timing scopes.
