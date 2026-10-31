# #527 Sandbox platform integration and consolidation

> **Status:** Design record; target LayerFS 2.0.0; not a released contract.
> Written 2026-11-01 against `layerfs_v2_0_0` at `81f8758bcf`, after the
> Area B merges (#523, #524, #525 with #427, #526). The owner answered
> D1–D7 (§9) on 2026-11-01 with the recommended options (D1:
> `platform/namespaces/`). The plan was implemented and checked by the
> terminal pass; its results, decisions and open findings are in
> [PLATFORM-PASS.md](PLATFORM-PASS.md). Findings and paths below describe
> the tree at `81f8758bcf`.

## 1. Summary

LayerFS now runs a Sandbox three ways (Docker, native, Linux namespaces) and
places its Store and daemons four ways (this host, Docker Desktop's VM, WSL2,
or a VM per project). The merges that added these left the code arranged by
history rather than by those two choices:

- The Rust crate `layerfs-sandbox` keeps every backend under `backend/`, next
  to OS facts, shared helpers and the helper binary's input format;
  `provider/` reaches back into the runtime and into Docker.
- `@layerfs/runtime` makes 15 Docker calls in seven files, each guarded by a
  flag its caller must remember.
- The backend is validated in two layers and, on a local placement, decided by
  whatever `LAYERFS_SANDBOX*` values happen to be in the environment.

This proposal names the two choices, gives each one owner, reorganizes both
packages around them, and fixes the defects found on the way. Each change is a
small commit; behaviour changes and file moves never share one (§8).

## 2. Two choices

| Word | Question | Values today | Owner |
| --- | --- | --- | --- |
| **Placement** | Whose Linux kernel opens the Store and runs the daemons? | `local` (this host, Docker Desktop's VM or WSL2), `vm:apple-container`, `vm:microsandbox`, `vm:ssh` | `@layerfs/runtime`: [`settings.js`][settings], `platform/` |
| **Backend** | How is one Sandbox run on that kernel? | `docker`, `native`, `namespaces` | `layerfs-sandbox`: `platform/` |

The backend fixes the isolation: a container, Linux namespaces with a cgroup,
or none. The same folder name, `platform/`, means placement in the runtime and
backend in the Rust crate (D7).

A VM provider only supplies the kernel. Inside the VM a Rust backend still
runs each Sandbox: `namespaces` in [#512][i512]'s route A, `docker` in its
route B. New providers such as E2B or Vercel therefore add a file to
`api/runtime/src/platform/vm/`, not a Rust backend.

## 3. What is implemented

### 3.1 Support matrix

| Host | Placement | Backend | Status | Store opened by | Mount | Isolation |
| --- | --- | --- | --- | --- | --- | --- |
| Linux | local | docker | supported, default | Linux, in a Docker volume (ext4/xfs/btrfs) | FUSE | container |
| Linux | local | native | SDK only, as root; the runtime never selects it on purpose | Linux, at `LAYERFS_STORE_PATH` | FUSE | none |
| Linux | local | namespaces | SDK only: root, cgroup v2, `user.max_user_namespaces=0`, an image archive | Linux | FUSE | namespaces + cgroup |
| macOS | local | docker | supported through the worker container | Linux, in Docker Desktop's VM | FUSE | container |
| macOS | local | native | SDK only, in-process macOS addon | macOS, on apfs/hfs | loopback NFS | none |
| macOS | local | namespaces | refused `Unsupported` | — | — | — |
| Windows | local | docker | supported through the WSL2 worker | Linux, in WSL2 | FUSE | container |
| Windows | local | native, namespaces | not refused; reaches the WSL worker's Linux addon as root; untested | — | — | — |
| any | `vm:*` | namespaces (a host `LAYERFS_SANDBOX_BACKEND=native` overrides it) | implemented; only fake providers have run | Linux, on the VM's ext4 disk | FUSE | VM + namespaces |
| any | `vm:*` | docker | impossible: the VM deployment hard-codes `SANDBOX: "native"` | — | — | — |

Sources: placement [`settings.js:40-48`][settings]; local deployment
[`clients.js:33-43`][clients]; VM deployment
[`vm/index.js:369-386`][vmindex]; backend [`runtime.rs:37-66`][noderuntime]
and [`local.rs:233-266`][nodelocal]; Store filesystems
[`kernel_guard.rs:32-90`][kernelguard].

### 3.2 Where each decision is made

| Decision | Places |
| --- | --- |
| docker or native | [`client.js:73-77`][agentclient] and [`runtime.rs:57-66`][noderuntime], same message |
| native or namespaces | [`runtime.rs:37-52`][noderuntime] only; not checked in JS; ambient on a local placement |
| image needs of namespaces | [`client.js:113-117`][agentclient] and [`runtime.rs:91-114`][noderuntime] |
| FUSE or NFS | [`local.rs:315-335`][nodelocal], `native/provider.rs:93-104`, `namespaces/create.rs:194`, `layerfs-daemon/src/application/filesystem.rs:22-23` |
| Sandbox name | `layerfs-node/src/sandbox.rs:262-272`, [`local.rs:340-350`][nodelocal], `sandbox/sweep.js:23` |
| Store path inside a Docker volume | `platform/linux/index.js:34`, `platform/macos/index.js:133`, `platform/windows/index.js:138,150`, Rust `LOCATOR` |
| whether host Docker applies | `hostDocker`/`hostSandboxes` checks in `status.js`, `volume.js`, `sandbox/list.js`, `sandbox/leftovers.js`, `sandbox/sweep.js` |

## 4. Findings

Defects, each fixed by a step in §8:

- **F1. `layerfs-sandbox` cannot compile on Windows.** `provider/launch.rs:2`
  and the `From<SandboxRequest>` impls in `provider/options.rs:139-178` use
  `crate::backend` with no `cfg`, and `backend` is `#[cfg(unix)]`
  (`lib.rs:5`). `layerfs-node` imports `backend::docker` without a `cfg` too.
  No CI lane builds Windows.
- **F2. `Runtime::Windows` cannot exist on Windows.** It is declared in the
  Unix-only `runtime.rs`, and only `tests/native_platform.rs:38` constructs
  it. In `layerfs-node`, the `cfg(not(any(linux, macos)))` branch in
  `local.rs:258-266` is dead on every target (`mod local` is itself gated to
  Linux and macOS); the ones in `sandbox.rs:154-160` and
  `discovery.rs:710-718` matter only on other Unix systems. [#452][i452] adds a
  Windows Store route, not a Sandbox backend.
- **F3. Store calls crash on a `vm:` home.** `store.create`
  ([`store/calls.js:123`][storecalls]), `start_load` (`store/sync.js:203`) and
  `runInto` call `platform.init(settings)` with no project; the VM's `init`
  reads `project.name` ([`vm/index.js:409-410`][vmindex]) and throws a plain
  `TypeError` with no LayerFS code. `vm.storePath()` also ignores the volume,
  so every Store would map to one guest file, and PostgreSQL Stores on `vm:`
  homes are not refused explicitly.
- **F4. The local placement does not choose the backend.** Its deployment
  ([`clients.js:33-43`][clients]) never sets `LAYERFS_SANDBOX`,
  `_SANDBOX_BACKEND` or `_LOCAL_DIR`, so ambient values pass through; on `vm:`
  a host `LAYERFS_SANDBOX_BACKEND=native` replaces `namespaces`
  ([`vm/index.js:374`][vmindex]).
- **F5. Reopen is asymmetric.** A native record reopens under a namespaces
  deployment (`discovery.rs:584-585` checks only `sandbox == native`); a
  namespaces record under a native deployment is refused (`:680-692`).
- **F6. Local discovery sees only Docker.** On local placements
  `runtime.sandbox.list`, `sweep` and `storeHolders` list containers only, so
  removing a Store can miss native or namespaces holders. `vm:` homes already
  use `project.list_sandboxes` ([`vm/index.js:342-348`][vmindex]).
- **F7. Documentation disagrees with code.**

  | Where | Says | Code |
  | --- | --- | --- |
  | [`api/docs/agent.md`][agentmd]`:150, 263-264` | native is macOS only; Linux keeps Docker only | Linux native exists (`local.rs:241-256`) |
  | `agent.md:369, 373` | a namespaces Sandbox reports `image` as `local`, not a pinned image | it reports the digest (`runtime.rs:76-83`) |
  | `agent.md:152, 362-364` | a record is never reopened as the other kind | see F5 |
  | [`AGENTS.md`][agents] | VM providers "verified"; `project.remove` | only fakes ran; the call is `project.delete` |
  | `api/cli/src/help.js:31-34` | lists the settings | omits seven `LAYERFS_*` settings |
  | `layerfs-api/PROVIDERS.md:5`, `backend/windows/README.md` | four `Runtime` variants | `Namespaces` is missing |
  | [`api/docs/runtime.md`][runtimemd] | anchor `#placement-one-linux-vm-per-home` | the heading is "per Store" |
  | `core/docs/architecture/76-native-mount-session.md`, `nfs-native-lifecycle.md` | links into `core/crates` | the crates are in `engine/crates` |

  AGENTS.md also says direct Store access is Linux only, while
  [`kernel_guard.rs:59-83`][kernelguard] accepts apfs/hfs for macOS native;
  that is D5, not a doc fix.

Structure, fixed by the layouts in §5–§6:

- **F8. `provider/` is not a leaf.** `provider/launch.rs` needs the runtime's
  `Sandbox` and `provider/options.rs` needs Docker's `SandboxRequest`.
  (`types/` already is one, apart from `RuntimeError::Docker` behind
  `cfg(unix)`.)
- **F9. Namespaces' use of native is hidden.** Namespaces is native with
  namespaces added (`LAYERFS_SANDBOX=native`,
  `LAYERFS_SANDBOX_BACKEND=namespaces`): it imports native's `Local`,
  `LocalPaths`, `LocalPhase`, `LocalProcess` and `search`, Linux's `mounts`,
  and five `backend/` helpers (`streams`, `listener`, `identity`, `HelperSpec`,
  `json`). The imports are fine; the folders do not show the relationship.
- **F10. Docker in `@layerfs/runtime` has no home.** Docker argv is built in
  `platform/shared.js` and in six files outside `platform/` (11 calls); `vm`
  supplies a `docker()` that throws, and each caller checks a flag first.
  `project/delete.js:65` is protected only because its container listing is.
- **F11. Small duplicates.** `STOPPED` (`shared.js:78`, `delete.js:52`);
  `/var/lib/docker/volumes` (three places plus the setting default); `df -kP`
  parsing (macOS, Windows, and the three VM providers); reading the worker
  endpoint file (four places); the worker `releaseStore` call (Windows, VM);
  `homeNames` and `removeHome` (linux, macos, vm).

## 5. Target layout: `layerfs-sandbox`

Scope: `engine/crates/layerfs-sandbox/src/` at `81f8758bcf`, counted with
`tools/production_loc.py --files` (blob `686d269d`). Removed files are listed
where they are today and counted in their folder; `—` marks a new file. Tests
live in `tests/` and are not counted. The listing shows D1 option C.

```diff
  src/                                   [8260]
    lib.rs                                   11   (absorbs backend/mod.rs; root re-exports)
    helper_spec.rs                          110   (from backend/; the helper binary's input format)
    runtime.rs                              255   (from backend/; gains LaunchFailure; drops Windows)
-   backend/                               [33]   (removed; partial: the rest is listed where it moves)
-     mod.rs                                 21   (declarations move to lib.rs)
-     identity.rs                            12   (into types/identity.rs as ContainerId::random)
    bin/                                  [434]   (unchanged)
      layerfs-sandbox-helper/             [434]
        main.rs                              45
        exec.rs                             141
        init.rs                              88   (HelperSpec import path)
        mounts.rs                           160   (HelperSpec import path)
+   json/                                 [403]   (streaming JSON for Engine replies and image manifests)
      mod.rs                                231   (from backend/json.rs)
      strings.rs                            172   (from backend/)
+   platform/                            [6488]   (every Sandbox backend)
+     mod.rs                                  —   new (declarations)
      docker/                            [3078]   (from backend/docker/; a container per Sandbox)
        mod.rs                               32
        create.rs                           223   (from container.rs; stop and delete move to sandbox.rs)
        endpoint.rs                         273
+       exec.rs                               —   new (request.rs + provider_exec.rs + inspect.rs)
        failure.rs                           29   (from provider_failure.rs)
-       headers.rs                           29   (into client/body.rs)
-       inspect.rs                           94   (into exec.rs)
        listener.rs                         197
        provider.rs                         118
-       provider_exec.rs                     95   (into exec.rs)
-       request.rs                          131   (into exec.rs)
        sandbox.rs                          115   (from container_types.rs; gains stop, delete, DockerRecord From impls)
        streams.rs                          374
        topology.rs                         189
        upload.rs                           228   (from archive.rs)
+       client/                           [951]   (the Engine HTTP client)
+         mod.rs                              —   new (declarations)
          body.rs                           180   (from docker/; absorbs headers.rs)
          http.rs                           566   (from docker/)
          mux.rs                             10   (from docker/)
          socket.rs                         119   (from docker/)
          version.rs                         76   (from docker/)
      namespaces/                        [1954]   (from backend/namespaces/; Linux; D1)
        mod.rs                               11
        cgroup.rs                            62
        create.rs                           250
        exec.rs                             103
        host.rs                              79
        paths.rs                             35
        provider.rs                         194
        sandbox.rs                          284
        rootfs/                           [936]
          mod.rs                              8
          archive.rs                        289
          cache.rs                          121
          layers.rs                         192
          tar.rs                            304
          types.rs                           22
      native/                            [1456]   (from backend/native/; one daemon process per Sandbox)
        mod.rs                               18   (alias platform becomes os)
        create.rs                           231
        exec.rs                             109
        groups.rs                           114
        listener.rs                          56   (from backend/)
        process.rs                           59
        provider.rs                         171
        sandbox.rs                          271
        search.rs                            13
        streams.rs                           18   (from backend/)
        linux/                            [133]   (from backend/linux/)
          mod.rs                              3
          mounts.rs                          90
          process.rs                         40
        macos/                            [263]   (from backend/macos/)
          mod.rs                              3
          darwin.rs                         238   (the crate's only unsafe)
          mounts.rs                          22
-   provider/                              [34]   (removed; partial: the rest moves to types/)
-     mod.rs                                 10   (into types/mod.rs)
-     launch.rs                              24   (into runtime.rs)
    types/                                [492]   (values every backend shares)
      mod.rs                                 13   (absorbs provider/mod.rs)
      capabilities.rs                        29   (from provider/)
      execution.rs                           32
      failure.rs                             97   (absorbs transfer.rs)
      identity.rs                            62   (gains ContainerId::random)
      image.rs                               25
      options.rs                            171   (from provider/; Docker From impls move out)
      request.rs                             46
-     transfer.rs                            17   (into failure.rs)
```

- **One folder per backend, named as the setting names it.**
  `platform/{docker, native, namespaces}` match the `LAYERFS_SANDBOX` values
  (§7), the `Runtime` variants and the record values. `runtime.rs` stays at the
  root as the one `match` over them; it also takes `LaunchFailure`.
- **The same file names for the same roles.** Each backend has `provider.rs`
  (what `Runtime` holds), `create.rs` (start one Sandbox), `sandbox.rs` (a
  running Sandbox: stop, delete, inspect) and `exec.rs` (one command). Docker
  gets there by renaming `container_types → sandbox` and `container → create`,
  moving its stop and delete into `sandbox.rs`, and joining `request`,
  `provider_exec` and `inspect` into `exec.rs`. `listener.rs` and `streams.rs`
  already match between Docker and native.
- **`docker/client/` is the Engine HTTP client**: `http`, `socket`, `body`
  (with `headers`), `mux` and `version`. `listener.rs` and `streams.rs` still
  read its `Socket`, `Body` and `mux::header`, so its `pub(super)` items become
  `pub(in crate::platform::docker)`.
- **`native/` holds the local process lifecycle and its OS facts.** `linux/`
  and `macos/` become children of the only code that reads them; native's
  `use super::linux as platform` alias becomes `os`, so it no longer shadows
  `crate::platform`. Namespaces keeps importing native's lifecycle as a
  sibling, which is ordinary Rust; the doc comment on `namespaces/mod.rs`
  says so.
- **Shared pieces sit at the root:** `json/` (Docker replies and namespaces'
  image manifests), `helper_spec.rs` (the input format shared by namespaces and
  `bin/layerfs-sandbox-helper`, compiled on every Unix as today) and `types/`.
- **`types/` takes `provider/`.** Capabilities and options move in;
  `ContainerId::random` joins `identity.rs` under `cfg(unix)`; the Docker
  `From` impls move to `docker/sandbox.rs` and `LaunchFailure` to
  `runtime.rs`. `types/` then depends on no backend except through
  `RuntimeError::Docker`.
- **Renames only where a name misleads:** the four Docker files above. `host.rs`,
  `rootfs/types.rs` and `types/identity.rs` keep their names.
- **Windows:** `Runtime::Windows` and `backend/windows/README.md` go (D2);
  `PROVIDERS.md` states that Windows runs the Linux build through WSL2 or a VM.

Build notes, from reading the source (nothing was built):

- `platform/`, `runtime.rs` (with `LaunchFailure`) and `json/` keep
  `#[cfg(unix)]`; they no longer inherit it from `backend`.
- `json/mod.rs` and `strings.rs` import each other through `super::`; both
  paths change.
- `native/` stays gated to Linux and macOS and `namespaces/` to Linux, as
  today; `HelperSpec` stays on every Unix.

### 5.1 Public surface

Modules become private. The crate root re-exports what every caller uses:
`Runtime`, `Sandbox`, `LaunchFailure`, the `types/` items, the four providers,
`Docker` and `HelperSpec`. Items specific to one backend stay in one public
module per backend, re-exported at the root so no external path names
`platform/`: `layerfs_sandbox::docker::…`, `::native::…`, `::namespaces::…`.
That keeps the four `Sandbox` types apart without renaming them; a flat root
would collide on `Sandbox`. The unused `DockerInstance`,
`NativeLinuxInstance` and `NativeMacosInstance` aliases go, and `LocalExit`
becomes `ExecExit`.

Paths that change, as occurrences in other crates, examples and tests
(grouped imports included): `backend::docker::` 22, `backend::native::` 12,
`backend::namespaces::` 5, `backend::HelperSpec` 3, `provider::` 27. The 12
names the root exports today keep their paths.

## 6. Target layout: `@layerfs/runtime`

Scope: `api/runtime/src/` at `81f8758bcf`, same counter; only the files this
change touches are listed.

```diff
  src/                                   [2199]   (partial: files this change touches)
    clients.js                               91   (both deployments name the backend)
    index.js                                145   (picks docker/ or vm/)
    settings.js                             117   (one Sandbox setting; host-OS check per VM provider)
    status.js                                26   (uses platform.docker)
    volume.js                                55   (uses platform.docker.disk)
    platform/                            [1146]
      shared.js                              62   (worker endpoint, homes, refusals, df parsing)
+     docker/                             [295]   (the local placement: Docker on this host, Docker Desktop or WSL2)
+       index.js                              —   new (the Docker group, built on each platform's own docker call)
+       disk.js                               —   new (disk use and volume removal for runtime.volume)
        linux.js                             34   (from linux/index.js)
        macos.js                            109   (from macos/index.js; Docker Desktop worker)
+       sandboxes.js                          —   new (list, inspect, leftovers, remove; STOPPED)
+       stores.js                             —   new (Store volumes, keyed by Store)
        windows/                          [152]   (from windows/)
          index.js                          135   (WSL2 worker; docker through wsl.exe)
          paths.js                           17
      vm/                                 [789]   (vm:<provider>: one VM per project; no Docker group)
        index.js                            312   (Store-keyed hooks refuse; init checks its project)
        cli.js                               30
        apple-container.js                   91
        microsandbox.js                      72
        ssh.js                              284
    project/                              [166]   (partial)
      delete.js                              51   (uses platform.docker.sandboxes)
      init.js                               115   (platform.name branches become capabilities)
    sandbox/                               [94]   (partial)
      leftovers.js                           33   (uses platform.docker.sandboxes)
      list.js                                40   (uses platform.docker.sandboxes)
      sweep.js                               21   (matches labels, not names)
    store/                                [326]   (partial)
      calls.js                              139   (makeVolume moves to docker/stores.js)
      sync.js                               176
      volume.js                              11   (Store-keyed hooks)
    worker/                                [33]   (partial)
      call.js                                33   (one endpoint read and releaseStore call)
```

- **`platform/` keeps meaning placement, and has two children**, one per
  `LAYERFS_PLACEMENT` kind: `docker/` for `local` (Docker on this host, in
  Docker Desktop or in WSL2) and `vm/` for `vm:<provider>`. The three Docker
  placements move into `docker/` beside what they share (D4).
- **One Docker group replaces three flags.** linux, macos and windows expose
  `platform.docker`, built in `docker/index.js` from that platform's own call:
  `run` (the host socket on Linux and macOS, `wsl.exe` on Windows),
  `sandboxes`, `stores`, `disk` and `version`. `vm` has no `platform.docker`.
  `hostDocker`, `hostSandboxes` and the throwing `vm.docker` go. A caller that
  finds no group refuses `Unsupported`; that refusal is new (today
  `runtime.vm` is simply absent on local placements). Helpers always reach
  Docker through `runtime.platform.docker.run`, so tests can still replace it.
- **Store hooks are keyed by Store, not by project.** `createStore` and
  `removeStore` join `store/calls.js` `makeVolume` and `shared.js`
  `makeStore`; the `vm` versions refuse cleanly until a VM home can hold
  Stores (F3).
- **F11's duplicates land once:** `STOPPED` in `docker/sandboxes.js`; the
  volumes directory from `settings.volumes` in `docker/index.js`; `df -kP`
  parsing, `homeNames` and `removeHome` in `shared.js`; the endpoint read and
  `releaseStore` call in `worker/call.js`.
- **Branches on `platform.name` become capabilities** (`initCounts`,
  `uncertainInit`; `project/init.js:110, 112, 141, 143`). `platform.name`
  stays in `init-failures.jsonl` and messages.
- **Unchanged:** `worker/server.js`'s path (hard-coded in macos, windows, vm,
  three test fixtures and a plugin guide) and `vm:ssh`.

## 7. Settings

One Sandbox setting replaces two (D3):

| `LAYERFS_SANDBOX` | Backend | Replaces |
| --- | --- | --- |
| unset or `docker` | docker | `LAYERFS_SANDBOX` unset or `docker` |
| `native` | native | `native` with backend unset or `native` |
| `namespaces` | namespaces | `native` with `LAYERFS_SANDBOX_BACKEND=namespaces` |

The old pair keeps working for one release. Rust
([`layerfs-node/src/runtime.rs`][noderuntime]) is the one validator and maps
the old pair; JS only checks that the fields each value needs are present.
The runtime always names the backend: local sets `docker`, and `vm:` sets
`namespaces` from its settings, not from `process.env` (F4).

### 7.1 Names that do not change

- Record `runtime` values `native`, `namespaces` and `local`
  (`layerfs-node/src/runtime.rs:18-24`), and every `layerfs.*` label.
- Capability names `docker`, `native-linux`, `native-macos`,
  `namespaces-linux`.
- The helper binary `layerfs-sandbox-helper`, `LAYERFS_SANDBOX_HELPER`, the
  spec field names and their asserted errors.
- On-disk names: cgroup `layerfs`, image cache `images`, `daemon.log`,
  `groups`, `LAYERFS_DAEMON_LISTEN`, `/layerfs-store`, `/layerfs-local/...`.
- `settings.platform` values, `LAYERFS_PLACEMENT` values and `platform.name`
  text.
- Rust variants `Runtime::Namespaces`, `SandboxRecord::Namespaces` and
  `ProviderLaunchOptions::Namespaces`.

## 8. Plan

Each step is one commit with its production LOC line. "Gated by" names what
must come first.

| Step | Change | Kind | Gated by |
| --- | --- | --- | --- |
| 1 | Fix the F7 drift that no later step changes (all rows except reopen and settings; not D5) | docs | — |
| 2 | `vm:` homes refuse Store-level calls and PostgreSQL projects before any effect; `vm.init` checks its project (F3) | behaviour | — |
| 3 | Both deployments name the backend (F4) | behaviour | — |
| 4 | Reopen requires the deployment's backend (F5), with a test and the `agent.md` reopen text. Native records reopened under a namespaces deployment, including on `vm:` homes, stop reopening: a compatibility break to state in the commit | behaviour | 3 |
| 5 | One Rust `sandbox_name`, used by `sandbox.rs` and `local.rs` | behaviour | — |
| 6 | The provider alone chooses FUSE or NFS; the daemon keeps its check as an assertion | behaviour | — |
| 7 | Remove `Runtime::Windows` and the dead branches (F2); gate F1 | behaviour | D2, D6 |
| 8 | Host-OS check per VM provider (Apple Container on macOS only); JS refuses native and namespaces on Windows | behaviour | — |
| 9a | `provider/` into `types/`, `LaunchFailure` into `runtime.rs`, Docker `From` impls into Docker (F8) | move | — |
| 9b | `backend/` into `platform/`, `json/` and root files; private modules and the §5.1 public paths; callers, guard and docs updated | move | 9a, D1 |
| 9c | Docker renames and joins, `docker/client/`, the `os` alias, `ExecExit` | move | 9b |
| 10a | `platform/docker/` with the Docker group; Docker placements move in | move | D4 |
| 10b | Store-keyed hooks | behaviour | 2, 10a |
| 10c | F11 duplicates | move | 10a |
| 10d | `platform.name` branches become capabilities | move | — |
| 11 | One Sandbox setting with the mapping; CLI help lists every setting | behaviour | 3, D3 |
| 12 | Local discovery of native and namespaces Sandboxes through `project.list_sandboxes`, reusing the `vm` code (F6); `sweep` matches labels | behaviour | 5, 10a |

Notes for the move steps:

- **9b** updates `tools/check_product_boundary.py:21,27` (the audited
  `src/backend/macos/darwin.rs`), its test
  `tools/test_check_product_boundary.py:127-135` (which also names
  `backend/docker/http.rs`), the `lib.rs` comment, both `PROVIDERS.md` files,
  the source doc-links, and every caller in §5.1. Historical
  `core/docs/issues/**` records keep their old paths.
- **Tests follow AGENTS.md's refactor rule.** Tests that drive removed
  internals (`hostDocker`, the throwing `vm.docker`, `backend::` paths that no
  longer exist) are deleted rather than adapted when adapting is not cheap,
  listed in the commit message and covered on the new code where the behaviour
  survives. Test files are renamed only if D1 picks a new folder name.

Verification follows [`engine/AGENTS.md`][engineagents] (checks and
completion) and [`api/AGENTS.md`][apiagents]; every test command is capped at
120 s.

## 9. Owner decisions

- **D1. Where the namespaces backend lives, and its name.**
  - **C (recommended): `platform/namespaces/`, a sibling of `docker/` and
    `native/`.** The folder matches the setting value, the `Runtime` variant
    and the record value; there is no extra level; no backend sits inside
    another. Cost: namespaces imports native's lifecycle from a sibling.
  - **B (owner direction, 2026-11-01): `platform/vm/`,** with Rust types
    renamed `Vm…` and the record value `namespaces` kept. For it: the backend
    exists for VM placements, and its own `mod.rs` describes "the project's
    VM". Against it: the code needs root, cgroup v2 and blocked user
    namespaces, not a VM; `vm` already names a placement; a VM placement can
    run the `docker` backend (#512 route B); and Rust names would no longer
    match the setting.
  - **A: `platform/native/namespaces/`.** Shows that namespaces is native with
    namespaces added. Against it: a backend sits beside the OS folders `linux/`
    and `macos/`, "native" means no isolation, and paths reach five levels.
- **D2. Windows placeholder.** Remove `Runtime::Windows` and its README
  (recommended), or keep a README at `platform/native/windows/`.
- **D3. Setting name.** `LAYERFS_SANDBOX=docker|native|namespaces`
  (recommended), or a new `LAYERFS_ISOLATION=container|none|namespaces`.
- **D4. Runtime folders.** Nest the three Docker placements under
  `platform/docker/` (recommended, §6), or keep `platform/{linux, macos,
  windows}/` flat beside a helper folder `platform/docker/`.
- **D5. macOS native Store.** `kernel_guard.rs` lets the macOS addon open a
  Store on apfs/hfs (#427); AGENTS.md says direct Store access is Linux only.
  Add a macOS native exception to AGENTS.md, or restrict the guard.
- **D6. Windows build.** Gate the crates so they compile on Windows, with a
  build-only CI lane that reports (CI never gates), or document that
  `layerfs-sandbox` and `layerfs-node` are Unix only.
- **D7. One word, two meanings.** Keep `platform/` in both packages, as
  placement in the runtime and backend in Rust (recommended; each package
  documents its meaning), or name the Rust folder after what it holds.

## 10. Not verified

Reported `NOT_RUN` until done, at the commit that lands the relevant step:

- `api/runtime/test/verify/vm_real.mjs` for Apple Container and Microsandbox
  (#526 ran fake providers only), and any real `vm:ssh` run (no script
  exists).
- `namespaces_userns` and `namespaces_lifecycle` as root at HEAD (one root
  pass on a rehearsal tree only).
- Linux native end to end as root.
- The bare macOS SDK with a Docker deployment, to settle whether
  `kernel_guard` should check the deployment instead of the OS.
- macOS `releaseStore` is a no-op although its worker caches open Stores; a
  volume removal may race a Store the worker still holds (read from code, not
  reproduced).
- Any Windows build (F1, D6).

## 11. Related

- [#512][i512] tested which agent sandboxes can replace the Docker container.
  Route A (no Docker, namespaces inside the platform's VM) passed end to end
  on Microsandbox, Apple Container and a Firecracker guest; Daytona and the
  Lima VM ran route B (nested Docker) only.
- [#515][i515] designed the per-project VM placement; [#526][i526] implemented
  it. #526 is merged into `layerfs_v2_0_0` only (`81f8758bcf`); on GitHub it
  is closed against `main` without a merge. It does not support Daytona, E2B,
  Vercel or Modal yet: shared-kernel runners need a seccomp filter.
- [#452][i452] adds a Windows Store route, not a Sandbox backend.
- [#523][i523], [#524][i524], [#525][i525] and [#427][i427] added native
  Sandboxes on Linux and macOS.

[settings]: ../../../../api/runtime/src/settings.js
[clients]: ../../../../api/runtime/src/clients.js
[vmindex]: ../../../../api/runtime/src/platform/vm/index.js
[storecalls]: ../../../../api/runtime/src/store/calls.js
[agentclient]: ../../../../api/agent/src/client.js
[noderuntime]: ../../../../engine/crates/layerfs-node/src/runtime.rs
[nodelocal]: ../../../../engine/crates/layerfs-node/src/local.rs
[kernelguard]: ../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs
[agentmd]: ../../../../api/docs/agent.md
[runtimemd]: ../../../../api/docs/runtime.md
[agents]: ../../../../AGENTS.md
[engineagents]: ../../../../engine/AGENTS.md#checks-and-completion
[apiagents]: ../../../../api/AGENTS.md
[i427]: https://github.com/Ephemeral-AI-Lab/layerfs/pull/427
[i452]: https://github.com/Ephemeral-AI-Lab/layerfs/pull/452
[i512]: https://github.com/Ephemeral-AI-Lab/layerfs/issues/512
[i515]: https://github.com/Ephemeral-AI-Lab/layerfs/pull/515
[i523]: https://github.com/Ephemeral-AI-Lab/layerfs/pull/523
[i524]: https://github.com/Ephemeral-AI-Lab/layerfs/pull/524
[i525]: https://github.com/Ephemeral-AI-Lab/layerfs/pull/525
[i526]: https://github.com/Ephemeral-AI-Lab/layerfs/pull/526
