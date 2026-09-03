# LayerFS 0.1.2 quickstart

> **Status:** Draft instructions for the withdrawn `v0.1.2` candidate; the tag
> used below is not currently published.

Build from the immutable release tag:

```bash
git clone --branch v0.1.2 --depth 1 https://github.com/Ephemeral-AI-Lab/layerfs.git
cd layerfs
cargo build --release -p layerfs-cli
export LAYERFS_BIN="$PWD/target/release/layerfs"
export LAYERFS_CONTEXT="$PWD/.layerfs/context"
mkdir -p "$PWD/.layerfs"

"$LAYERFS_BIN" db create "$PWD/.layerfs/store.sqlite"
"$LAYERFS_BIN" context use --store "$PWD/.layerfs/store.sqlite"
"$LAYERFS_BIN" layerstack init --name demo --empty
"$LAYERFS_BIN" query layerstacks
```

For the complete Branch, Workspace, Commit, End, directory-import, and
managed-container walkthrough, follow the compatible
[0.1.1 quickstart](../0.1.1/quickstart.md). Keep the Store outside any imported
or projected directory.
