# layerfs-rutis

Tier 0 of the Rust rutis plugin: one existing LayerFS project exposed as
`fs.*`, `branch.*` and `project.list_branches` through `call`.

- No Init, no Workspace, no Sandbox, no daemon.
- No Node runtime and no IPC: the host process opens the Store directly.
- Linux only.
- Text/JSON only; binary `fs.write`/`fs.apply` input is out of Tier 0.

The design is [docs/roadmap/1.0/rutis/tier-0.md](../../docs/roadmap/1.0/rutis/tier-0.md).

## Use

```rust
use layerfs_rutis::{Config, LayerFs, LayerFsPlugin};
use rutis::Ctx;

let ctx = Ctx::root()?;
let view = ctx.plugin(LayerFsPlugin::new(Config {
    project: "website".into(),
    home: "/srv/layerfs/home".into(),
    store_path: "/var/lib/docker/volumes/layerfs-store/_data/global/store.sqlite".into(),
}));
(&view).await?;
let layerfs = ctx.get::<LayerFs>().expect("layerfs service");
let value = layerfs
    .call("fs.read".into(), serde_json::json!({
        "branch": "main",
        "path": "/README.md",
    }))
    .await?;
```

The project must already be installed by the existing SDK/CLI/dsh. The plugin
reads `<home>/<project>/project.record` and opens the Store at `store_path`.

## Test

```sh
cargo test --test tier0
```

The test initializes a temporary Store, copies the sealed file to the
installed path and exercises read, write, fork, list, publish, refusals,
permissions and a real rutis host load. It does not need Docker;
`install_local` itself requires root and is out of Tier 0. A non-Linux test
asserts `Unsupported` without touching a Store.
