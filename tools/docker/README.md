# Area P Linux images

The Area P development environment uses a dedicated Ubuntu WSL2 control plane and Docker Desktop's Linux engine. The checkout remains at `/mnt/e/layerfs-connect/layerfs`; writable Stores and launcher state stay on Linux filesystems.

Build the images before registering `tools/worktree.py` resources:

```bash
docker pull rust:1.85.1-bookworm
docker pull node:24.21.0-bookworm
docker build \
  --file /mnt/e/layerfs-connect/layerfs/tools/docker/area-p-runtime.Dockerfile \
  --tag layerfs-area-p-runtime:node24-python3 \
  /mnt/e/layerfs-connect/layerfs/tools/docker
```

The runtime image supplies Node 24 and Python 3 for sealed cross-client tests. Registration resolves installed images to immutable IDs and never pulls them implicitly. FUSE capability comes from the product's Docker configuration and the real mount tests; this recipe alone is not FUSE qualification.

All Stores use Disposable/WAL/synchronous=OFF only. Durable execution is NOT_RUN. Builds fetch dependencies only during explicit `prepare`; ordinary builds are locked and offline.
