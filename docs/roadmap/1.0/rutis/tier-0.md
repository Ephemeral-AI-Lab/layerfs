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
layerfs-history       project name validation
serde_json, tokio
libc                  Linux owner and O_NOFOLLOW checks
```

Rust 1.88 (rutis). No `napi`, no `layerfs-node`, no `layerfs-project`.

## Service

One service, `layerfs`:

| Method | Arguments | Result |
| --- | --- | --- |
| `info` | none | `{ project }` |
| `call` | `{ method, args }` | the Store method's result |

Allowed methods are the exact public Store table: `fs.*` (14), `branch.*` (6)
and `project.list_branches`. Everything else is `UnknownMethod`.

`call` is the contract; no typed wrappers in Tier 0. Rust consumers can add a
typed client around it without changing the service.

## Configuration

```text
project     LayerFS project name (required)
home        absolute LAYERFS_HOME (required)
store_path  installed Store file (required)
```

Validation:

- project name matches the SDK alphabet (1-63, lowercase/digits/`_-.`, with
  alphanumeric first and last characters);
- `home` and `store_path` are absolute;
- `home` is private, owned by the effective user, no group/other bits;
- `store_path` exists and is a regular file (not a symlink);
- the project record exists and is a regular non-symlink file; its content,
  Disposable profile and installed manifest are checked on the first call.

No Docker socket, daemon path or image.

## Open the Store

On the first call:

1. Read `<home>/<project>/project.record` and decode the control
   `Answer`; its `Reply::Sdk` value is a map.
2. Decode `manifest` and `installed` with `StoreManifest::decode`.
3. Require a Disposable sealed manifest with no `daemon_sqlite`, and an
   installed manifest that equals the sealed one except for a present
   `daemon_sqlite`.
4. Create a private temporary directory for the overlay template.
5. `StoreService::open_direct(store_path, manifest.binding,
   manifest.cursor_key, overlay_template)`.
6. Keep one `Arc<Opened>`; its drop closes the Store and removes the overlay
   directory.

The plugin never writes the project record and never installs a Store.

## Call

- Add the configured `project` to the argument map; the Store table expects
  that field.
- Convert serde JSON to `SdkValue`: null/bool/safe integer/text/list/ordered
  map; no floats, functions, symbols or big integers; depth at most 16.
- Tier 0 is text/JSON only. Binary `SdkValue::Bytes` output is returned as an
  array of numbers; binary `fs.write`/`fs.apply` input is not supported.
- Run `layerfs_api::call(&service, &SdkCall { method, args })`.
- Convert the result back inside the blocking task; `SdkRefusal` becomes
  `LayerFsError { code, message, data }`.
- Run the synchronous call on `spawn_blocking`; no mutex held across it.
- One attempt. No retry.

## Lifecycle

- `apply`: validate config, construct state, provide `layerfs`. No Store I/O.
- First `call`: open the Store.
- Unload: disable new Store calls; `info` remains available. An in-flight call
  may finish. A sole-owner close reports Store/overlay cleanup errors; while a
  call still holds the `Opened`, cleanup is deferred to its last drop.
  The overlay directory is removed when the last `Arc<Opened>` drops.
- Reload: a new plugin opens the same Store.
- No Workspace Commit, no unmount, no implicit Commit; `fs.*`/`branch.*`
  writes and deletes happen only when the caller asks for them.

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
  `branch.fork`, `branch.list_commits`, `branch.publish`.
- The test copies a sealed Store to the installed path because
  `install_local` requires the daemons' user (root); installation itself is
  out of Tier 0.
- Refusals keep `code` and `data` (`NoSuchPath` with its path).
- Unknown methods return `UnknownMethod` before the Store is opened.
- Missing installed manifest returns `NoStore`; a non-private record returns
  `Unauthorized`; a symlinked record is rejected at `apply`; a mismatched
  installed manifest returns `Failed`.
- Plugin load/unload through a Rust rutis host; after unload `call` fails
  closed while `info` remains available.
- A non-Linux build has a separate `Unsupported` test; Linux tests run under
  `CARGO_TARGET_TMPDIR`.
- No Docker, Sandbox or daemon test.
- No core diff.

## Risks

| Risk | Mitigation |
| --- | --- |
| Project record format drifts | Use the public `SdkValue`/`StoreManifest` codecs; test against a native record |
| `StoreService` is not `Send + Sync` | Compile-time assertion; one actor per project if needed |
| Large `fs.write` content | Tier 0 accepts text/JSON only; no binary or streaming; document the size cost |
| Error mapping later | Keep Rust-only; decide cross-language in Tier 2 |
