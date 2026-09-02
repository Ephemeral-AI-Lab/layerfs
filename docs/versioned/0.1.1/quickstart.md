# LayerFS 0.1.1 release-candidate quickstart

> **Status:** Candidate instructions; `v0.1.1` is not published.

Build the candidate from its eventual clean release commit:

```bash
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
[0.1.0 quickstart](../0.1.0/quickstart.md). Keep the Store outside any imported
or projected directory. Before publication, this abbreviated candidate guide
must be rechecked against the exact clean `v0.1.1` source.
