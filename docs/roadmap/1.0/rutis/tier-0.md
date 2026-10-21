# Tier 0: existing Store, no LayerFS changes

> **Status:** Proposal; target LayerFS 1.0.0; not a released contract.

## Scope

One Rust rutis plugin over one **already installed** LayerFS project:

- `call(method, args)` for `fs.*`, `branch.*` and `project.list_branches`
- `info()` for the project name

No Init, no Workspace, no Sandbox, no operator, no UI, no remote worker. No
change to `core/`, the daemon, the Store or the native binding.

Projects are initialized by the existing SDK/CLI/dsh, not by this plugin.

## Topology

```text
Rust rutis host process
  └─ layerfs-rutis plugin
       └─ StoreService (direct, Linux)
            └─ installed Store file
```

Built-in Rust plugin, same process as its consumers. No Node, no IPC on the
data path. Stock `rutis-host` cannot load it; a Rust host is required.

## Crate

`layerfs-rutis` outside `core/`, for example `integrations/rutis-rust/`:

```text
rutis 0.13
layerfs-api           Store method table
layerfs-store-service StoreService
layerfs-bridge        SdkValue, SdkCall, SdkRefusal, StoreManifest
serde, serde_json, thiserror, tokio
```

Rust 1.88 (rutis). No `napi`, no `layerfs-node`, no `layerfs-project`.

## Service

One service, `layerfs`:

| Method | Arguments | Result |
| --- | --- | --- |
| `info` | none | `{ project }` |
| `call` | `{ method, args }` | the Store method's result |

Allowed methods: `fs.*`, `branch.*`, `project.list_branches`. Everything else
is `UnknownMethod`.

`call` is the contract; no typed wrappers in Tier 0. Rust consumers can add a
typed client around it without changing the service.

## Configuration

```text
project     LayerFS project name (required)
home        absolute LAYERFS_HOME (required)
store_path  installed Store file (required)
```

Validation:

- project name matches the SDK alphabet (1-63, lowercase/digits/`_-.`);
- `home` and `store_path` are absolute;
- `home` is private, owned by the effective user, no group/other bits;
- `store_path` exists and is a file;
- the project record has a Disposable manifest and an installed manifest.

No Docker socket, daemon path or image.

## Open the Store

On the first call:

1. Read `<home>/<project>/project.record` with `SdkValue::decode`.
2. Decode `manifest` and `installed` with `StoreManifest::decode`.
3. Require `manifest.profile == Disposable` and `installed.is_some()`.
4. Create a private temporary directory for the overlay template.
5. `StoreService::open_direct(store_path, manifest.binding,
   manifest.cursor_key, overlay_template)`.
6. Keep one `Arc<StoreService>`; remove the overlay directory on close.

The plugin never writes the project record and never installs a Store.

## Call

- Convert serde JSON to `SdkValue`: null/bool/safe integer/text/bytes/list/
  ordered map; no floats, functions, symbols or big integers; depth at most 16.
- Run `layerfs_api::call(&service, &SdkCall { method, args })`.
- Convert the result back; `SdkRefusal` becomes `LayerFsError { code, message,
  data }`.
- Run the synchronous call on `spawn_blocking`; no mutex held across it.
- One attempt. No retry.

## Lifecycle

- `apply`: validate config, construct state, provide `layerfs`. No Store I/O.
- First `call`: open the Store.
- Cleanup: drop and close the StoreService, remove the overlay directory.
- Reload: a new plugin opens the same Store.
- No Commit, no data deletion, no project mutation.

## Errors

Rust-only typed error with the stable LayerFS code, message and data. Do not
design a cross-language code mapping in Tier 0; that is a Tier 2 concern.

## Security

- Linux only; Store access requires the kernel guard and a local filesystem.
- Private home and project record; no group/other access.
- No Docker socket, no daemon, no network listener.
- Do not log file content or arguments.

## Tests and acceptance

- Open an existing installed Store from a fixture; `fs.read`, `fs.write`,
  `fs.apply`, `branch.fork`, `branch.list_commits`, `branch.publish`.
- Refusals keep `code` and `data` (`NoSuchPath`, `Conflict`, `InvalidInput`).
- Non-Linux host fails `Unsupported` before opening.
- Wrong owner/mode or missing installed manifest fails before any Store effect.
- Plugin load/unload through a Rust rutis host; unload closes the Store.
- No Docker, Sandbox or daemon test.
- No core diff.

## Risks

| Risk | Mitigation |
| --- | --- |
| Project record format drifts | Use the public `SdkValue`/`StoreManifest` codecs; test against a native record |
| `StoreService` is not `Send + Sync` | Compile-time assertion; one actor per project if needed |
| Large `fs.write` content | Caller sends bytes; document the size cost; no streaming in Tier 0 |
| Error mapping later | Keep Rust-only; decide cross-language in Tier 2 |
