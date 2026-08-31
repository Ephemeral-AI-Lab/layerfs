# LayerFS 0.1.0 container runtime

> **Status:** Release-candidate container-runtime contract for LayerFS 0.1.0.

LayerFS can project a Workspace through real FUSE inside one prepared Linux
container while the SDK and Store remain on the host.

## Runtime image contract

A compatible image contains:

```text
/usr/local/bin/layerfs-daemon
/usr/local/bin/layerfs-fuse
/usr/local/bin/layerfs-daemon-entrypoint
TCP port 41273
/dev/fuse support
```

The daemon is a control and execution transport. It owns no Store, SQLite
connection, decoded-object cache, shell pool, or precreated Workspace. Each
Workspace launches a fresh FUSE helper and mount. Each Exec launches the exact
requested process from a fresh process boundary.

## Create and connect

```bash
layerfs container create \
  --name agent-runtime \
  --image layerfs-runtime:dev \
  --memory-mib 512 \
  --cpus 2 \
  --pids-limit 512

layerfs container status agent-runtime
layerfs container start agent-runtime
```

Managed containers are created with:

- `/dev/fuse` exposed;
- `CAP_SYS_ADMIN` for the mount;
- `privileged=false`;
- no host bind mounts;
- resource limits from `ContainerLimits`;
- daemon port 41273 published to an ephemeral host port on `127.0.0.1` only;
- label `dev.layerfs.managed=true`.

The default limits are 512 MiB, 2 CPUs, and 512 PIDs. Accepted limits are
64 MiB through 64 GiB, 1 through 256 CPUs, and 32 through 65,535 PIDs.

At Start or Connect, the host resolves the exact 64-hex Docker container ID,
copies the 32-byte daemon capability into an owner-only runtime directory, and
authenticates one owner connection. FUSE and Exec streams prove possession of
that capability and bind their request to the daemon boot and owner identity.

## Create a FUSE Workspace

```bash
WORKSPACE_ID=$(layerfs workspace create <branch-id> \
  --container agent-runtime \
  --at /workspace \
  --projection fuse)

EXECUTION_ID=$(layerfs workspace exec "$WORKSPACE_ID" -- \
  /bin/sh -c 'printf "container workspace\n" > result.txt')

layerfs workspace output "$EXECUTION_ID" --follow
layerfs workspace commit "$WORKSPACE_ID"
layerfs workspace end "$WORKSPACE_ID"
```

The SDK owns the native Store and runtime state. The container receives only
authenticated filesystem and execution traffic; the Store file and host
Workspace runtime are not bind-mounted into it.

## Stop and remove

All Workspaces and executions must end before stopping the managed container:

```bash
layerfs container stop agent-runtime
layerfs container remove agent-runtime
```

Remove refuses a running container and cleans its copied capability file.

## Measurement boundary

Image construction, container creation, daemon readiness, and fixture setup
are preparation. A complete public Workspace measurement begins immediately
before Workspace Create and ends after Workspace End. It includes mount
readiness, fresh-process execution, Commit, Store visibility, and cleanup.
