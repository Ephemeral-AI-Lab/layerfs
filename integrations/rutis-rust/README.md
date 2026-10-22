# layerfs-rutis

Tier 0 and Tier 1 of the Rust rutis plugin over one existing LayerFS
project.

- Tier 0: `fs.*`, `branch.*` and `project.list_branches` through one
  `call` method.
- Tier 1: `workspace_mount`, `workspace_exec`, `workspace_status`,
  `workspace_commit` and `workspace_unmount` over the shared
  [`layerfs-session`](../../sdk/session) layer.
- No Init, no daemon in the data path, no Node runtime and no IPC: the host
  process opens the Store directly and talks to Sandbox daemons itself.
- Linux only.
- Text/JSON only; binary `fs.write`/`fs.apply` input is out of scope.

The designs are [Tier 0](../../docs/roadmap/1.0/rutis/tier-0.md) and
[Tier 1](../../docs/roadmap/1.0/rutis/tier-1.md).

## Use

Tier 0:

```rust
use layerfs_rutis::{Config, LayerFs, LayerFsPlugin};
use rutis::Ctx;

let ctx = Ctx::root()?;
let view = ctx.plugin(LayerFsPlugin::new(Config {
    project: "website".into(),
    home: "/srv/layerfs/home".into(),
    store_path: "/var/lib/docker/volumes/layerfs-store/_data/global/store.sqlite".into(),
    deploy: None,
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

Tier 1 adds the deployment:

```rust
use layerfs_rutis::{Config, Deploy};
use serde_json::json;

let config = Config {
    project: "website".into(),
    home: "/srv/layerfs/home".into(),
    store_path: "/var/lib/docker/volumes/layerfs-store/_data/global/store.sqlite".into(),
    deploy: Some(Deploy {
        docker: "/var/run/docker.sock".into(),
        image: "sha256:<64 hex>".into(),
        volume: "layerfs-store".into(),
        daemon: "/usr/local/bin/layerfs-daemon".into(),
        daemon_host: None,
        container_prefix: None,
    }),
};

let mounted = layerfs.workspace_mount(json!({ "branch": "main" })).await?;
let sandbox = mounted["sandbox"].as_str().unwrap();
let path = mounted["path"].as_str().unwrap();
let exec = layerfs
    .workspace_exec(json!({
        "sandbox": sandbox,
        "path": path,
        "command": "make test",
        "sink": { "stdout": "/tmp/test.stdout", "stderr": "/tmp/test.stderr" }
    }))
    .await?;
let status = layerfs
    .workspace_status(json!({ "sandbox": sandbox, "path": path }))
    .await?;
let commit = layerfs
    .workspace_commit(json!({
        "sandbox": sandbox,
        "path": path,
        "message": "run tests"
    }))
    .await?;
layerfs
    .workspace_unmount(json!({ "sandbox": sandbox, "path": path }))
    .await?;
```

Every Workspace call runs once. There is no implicit Commit, unmount or
discard. Without `deploy`, Store calls keep working and every
`workspace_*` call refuses `InvalidInput`.

`workspace_exec` without `sink` returns the SDK result unchanged. If a
stream exceeds the SDK's inline window, the result names a spool but this
service exposes no page method; use a sink for large output. With `sink`,
its absolute `stdout`/`stderr` paths receive every page in order (a slow file
holds the command back) and the result carries the byte counts.

The project must already be installed by the existing SDK/CLI/dsh. The plugin
reads `<home>/<project>/project.record` and opens the Store at `store_path`.

## Test

```sh
cargo test
```

The tests initialize a temporary Store, copy the sealed file to the installed
path and exercise Store calls, refusals, validation and a real rutis host
load. Tier 1 tests add deployment validation, missing-deployment refusals,
unknown-Sandbox refusals and sink argument validation without requiring
Docker. A live Engine path (mount, exec, status, commit, unmount) is not run
by this command; `install_local` itself requires root and is out of scope. A
non-Linux test asserts `Unsupported` without touching a Store.
