# Ported SDK (`@layerfs/port`)

> **Status:** Current guide.

LayerFS makes Store calls only in a Linux process. On macOS, an adapter's process uses this ported SDK instead: the real SDK of a Linux LayerFS build, whose native calls go to one long-lived Linux container per LayerFS home, the Store container. Its worker makes each call with the real Linux binding and keeps a project's Store and daemon connections open while the project is in use.

It is not the product SDK ([`sdk/js`](../sdk/js/README.md)), which does not change. It exists under the owner's exception of 2026-10-20 to the direct Store rule ([AGENTS.md](../AGENTS.md)); nothing else may route Store calls this way.

Used by [LayerFS × DSH](../dsh/README.md), whose Host links it as `@layerfs/port`, and by [LayerFS × Pi](../pi/README.md#macos), which imports it from beside itself.

| Entry | What |
| --- | --- |
| `@layerfs/port/sdk`, `@layerfs/port/operator` | The SDK and operator of the build in `LAYERFS_BUNDLE` (else `DSH_LAYERFS_BUNDLE`), with their calls made in the Store container `LAYERFS_STORE_CONTAINER` (else `DSH_LAYERFS_STORE_CONTAINER`) |
| `@layerfs/port` | `useStoreContainer(report)` serves this process's calls from the Store container of the deployment in its `LAYERFS_*` settings (`HOME`, `BUNDLE`, `IMAGE`, `DOCKER`, `CONTAINER_PREFIX`): it starts the container unless it runs and again when a call finds it down. For a host that configures it itself (the dsh Host): `ensureBox` starts it, one start at a time, and `onUnreachable` tells a hook when a call finds it down. `storeName`, `storePathOf`, `storeFile`, `DAEMON_HOST` and `VOLUMES` name the container, a project's Store, the connection file and where calls reach |

- **The Store container** is `<prefix>store`, labelled `layerfs.role=store` with its home, worker and build. It mounts only the engine's Docker socket, Docker's volumes directory, the LayerFS home, the build and this package's `src/`, each at its own path, so the home and the build must be in a folder Docker's VM shares (colima shares only the home folder).
- **Docker** is reached from the Mac at the deployment's socket (`LAYERFS_DOCKER`, Docker Desktop's `/var/run/docker.sock` by default) and from the containers at the engine's own `/var/run/docker.sock`. Its worker listens on the Mac's loopback only, behind a secret written to `<home>/.layerfs-store.json`.
- **Init** runs in a container of its own with its source folder mounted read-only.
- **A call that finds the container down** keeps its own answer (`Failed`, or `Unknown` when the connection broke mid-call) and is never sent again; the adapter's hook starts the container again.
