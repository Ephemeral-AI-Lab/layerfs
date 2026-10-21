# Tier 0: Rust Store service without LayerFS changes

> **Status:** Proposal; target LayerFS 1.0.0; not a released contract.

## Scope

A Rust rutis plugin that exposes one project's Store operations:

- `project.init`
- `project.list_branches`
- `fs.*` (14 methods)
- `branch.*` (6 methods)
- a generic `call(method, args)` over the same Store table

No Sandbox, no Workspace, no operator, no UI, no remote worker. No change to
`core/`, the daemon, the Store or the native binding.

## Topology

```text
Rust rutis host process
  └─ layerfs-rutis plugin
       └─ StoreService (direct, Linux)
            └─ installed Store file
```

The plugin is a built-in Rust plugin. Consumers in the same process call it
through the rutis typed registry. There is no Node runtime and no IPC on the
data path.

Stock `rutis-host` cannot load it; a custom Rust host or a dylib variant is
required. Hot replacement is not part of Tier 0.

## Crate

`layerfs-rutis` lives outside `core/` as an integration crate, for example
`integrations/rutis-rust/`. It depends on:

- `rutis` 0.13
- `layerfs-api` for the public Store method table
- `layerfs-store-service` for `StoreService` and `install_local`
- `layerfs-bridge` for `SdkValue`, `SdkCall`, `SdkRefusal` and `StoreManifest`
- `layerfs-history` for project/branch names
- `layerfs-persistence` for the Disposable Store configuration
- `layerfs-telemetry` for the Init timing scope
- `serde`, `serde_json`, `thiserror`, `tokio`

The crate's `rust-version` is 1.88 (rutis); it does not change the core
workspace's 1.85 floor. It must not depend on `napi` or `layerfs-node`.

## Service surface

One service, `layerfs`:

| Method | Arguments | Result |
| --- | --- | --- |
| `info` | none | `{ project, ready, store, capabilities }` |
| `project_init` | `{ source, policy? }` | `{ kind: "Initialized", project, layer, root, store }` |
| `call` | `{ method, args }` | the Store method's result |

`call` is the only method-table entry point; Tier 0 does not duplicate 27
typed methods. A typed Rust client can wrap `call` later without changing the
service contract. Allowed method prefixes are `fs.`, `branch.` and
`project.list_branches`; anything else is `UnknownMethod`.

`info.ready` is false while the Store is not open. `project_init` does not
require a ready Store.

## Configuration

```text
project       LayerFS project name (required)
home          absolute LAYERFS_HOME (required)
store_path    installed Store file (required)
store_policy  optional Init policy
trace         optional SDK trace file
```

Validation before `apply` succeeds:

- project name matches the SDK's 1-63 character alphabet;
- `home` and `store_path` are absolute;
- `home` is created 0700 if missing; an existing home is owned by the
  effective user with no group/other bits;
- the Store destination's parent is writable or creatable as 0700;
- the profile is Disposable; Durable is never selected.

No `deploy`, Docker socket or daemon path is needed.

## Data layout and permissions

The plugin uses the existing project records, not a second format:

```text
<home>/<project>/project.record     SdkValue map: manifest, installed
<home>/<project>/sealed.sqlite      sealed Store before installation
<store_path>                        installed Store opened by StoreService
```

`project.record` is encoded with the public `SdkValue::encode`/`decode` and
`StoreManifest::encode`/`decode`. The plugin repeats the native binding's
private-directory checks: non-symlink, owner is the effective user, mode has no
group/other bits. It does not invent a new record.

The overlay template for `StoreService::open_direct` goes in a private
temporary directory and is removed when the service closes.

## Init

`project_init` follows the existing native sequence:

1. Check the policy, source, home and Store destination before creating
   anything.
2. Create `<home>/<project>` as 0700; an existing entry is `Exists`.
3. Serialize with a per-directory in-process turn plus an OS `flock`, as the
   native discovery layer does. The wait is bounded and is not a retry.
4. Build `InitRequest`: source, sealed SQLite at `sealed.sqlite`, Disposable
   profile, locator `/layerfs-store/global/store.sqlite`, frozen policy,
   random catalog binding/cursor, stack/branch identities, 60-second deadline.
5. Run `ProjectApi::init_observed` under
   `layerfs_telemetry::timer::Timing::disabled`.
6. Install the sealed file with `layerfs_store_service::install::install_local`
   at `store_path`.
7. Write `project.record` with the manifest and installed manifest.
8. Return the same value shape as the JavaScript SDK.

A failed or unknown install keeps the original failure and any files it
created; it is not retried and not guessed to be installed.

## Store open and call

On the first Store call:

1. Load and validate `project.record`.
2. Require the Disposable profile and an installed manifest.
3. Call `StoreService::open_direct(store_path, manifest.binding,
   manifest.cursor_key, overlay_template)`.
4. Keep one `Arc<StoreService>` per project in the plugin.
5. Convert the call's serde arguments to `SdkValue` with the SDK's rules:
   null/bool/safe integer/text/bytes/list/ordered map, no floats, no
   functions, no symbols, no big integers, depth at most 16.
6. Run `layerfs_api::call(&service, &SdkCall { method, args })`.
7. Convert the result back; a `SdkRefusal` becomes `LayerFsError`.

The plugin never calls the operator table. It never runs `fs`/`branch` work
through a daemon or another process.

## Lifecycle

- `apply`: validate config, construct the project state, provide `layerfs`.
  Do not open the Store.
- First Store call: open the Store.
- `project_init`: create and install the Store, then the next call opens it.
- Cleanup: drop the `StoreService`, close it, remove the overlay directory.
  No Commit, no data deletion.
- Row reload: a new plugin instance opens the same Store; the Store file and
  project record persist.

## Concurrency

- `StoreService` is shared behind `Arc`; Store calls are synchronous. Run them
  on `tokio::task::spawn_blocking`.
- Do not hold a plugin mutex across a Store call.
- Concurrent Store calls are allowed; LayerFS's own admission queue and
  publication lane provide ordering. The plugin adds no second lock.
- `project_init` is exclusive for a project through the directory lock; a
  second Init waits only for the bounded discovery turn, then reports `Busy`.

## Errors

`LayerFsError` carries:

```text
code      stable LayerFS code, for example NoSuchPath or Conflict
message   human-readable
data      structured detail
```

Rust consumers match the typed error. For a `#[rutis::service]` export, exact
codes need a hand-written `HostDispatch` with
`rutis_bridge::exports::failure` or a rutis error-code extension; otherwise
the macro's `code` is the error category and `data.code` holds the exact code.
Tier 0 therefore defaults to a Rust-only service.

## Security

- Linux only; Store access requires the kernel guard and a local filesystem.
- The Store path and LayerFS home must be private and owned by the effective
  user.
- No Docker socket, no daemon executable and no network listener in Tier 0.
- The optional trace file contains call metadata, never file content or
  messages beyond what the SDK already traces.

## Tests and acceptance

- Init from a small fixture; read/write/edit/apply; fork/list/diff/merge/
  publish; each refusal keeps its code and detail.
- Init on an existing project returns `Exists`; a failed Init is not retried.
- Store calls fail with `Unsupported` on a non-Linux host.
- Permission failures (home not 0700, wrong owner, Store parent not writable)
  fail before any effect.
- Plugin load/unload through a Rust rutis host; unload closes the Store.
- No test starts Docker, a Sandbox or a daemon.
- Production LOC: new integration scope; no core diff.

## Risks

| Risk | Mitigation |
| --- | --- |
| Project record format drifts | Use `SdkValue`/`StoreManifest` encode/decode; test against native records |
| Init locking differs from native | Reuse the same in-process turn + `flock`; add a concurrent Init test |
| Store service is not `Send + Sync` | Verify at compile time; otherwise use one actor per project |
| Exact error codes through rutis | Keep Tier 0 Rust-only; decide the cross-language mapping before export |
| Duplicate Init logic | Extract a small shared init helper later if Tier 1 extracts the session layer |
