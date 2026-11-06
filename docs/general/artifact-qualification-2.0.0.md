# LayerFS 2.0.0 artifact qualification (Linux x64)

> **Status:** Current general guide; a dated evidence record, not a release receipt. Nothing here was published.

What was built and installed outside a source checkout on one host, what ran
from the installed files, and what did not run. Issue #39 (parent #30). Raw
logs were kept outside the repository; commands are given so each row can be
repeated. Anything not listed as RAN was not run.

## Source and tools

| Item | Value |
| --- | --- |
| Source | `ce9cd4144` = baseline `d4304741` (`layerfs_v2_0_0`) plus four open PRs (scope, cli, python, node); local integration branch, never pushed |
| Build script and package changes after the builds | `35bec0c97` (`build-native.mjs --features`), `254feec2d` (`prepack`); neither changes Rust code, so the three Rust artifacts are those of this source |
| Host | Ubuntu 26.04.1 under WSL2, Linux 6.6.87.2-microsoft-standard-WSL2, x86_64, glibc 2.43 |
| Toolchain | rustc/cargo 1.85.1 (`4eb161250`), Node v24.22.2, npm 11.19.1, Python 3.14.4, maturin 1.16.0 (`pyproject.toml` asks `>=1.9.6,<2.0`) |
| Profile | `--release` for every artifact below (`CARGO_BUILD_JOBS=3`, `--locked --offline`); build times: addon 2m18s, command 49s, wheel 26s (warm shared target) |
| PostgreSQL builds | `--features postgresql`, own target dir; OpenSSL 3.0.15 headers and link stubs taken from the `rust:1.85.1-bookworm` image (the host has no `libssl-dev`), `patchelf` 0.19.3 for the wheel |

## Artifacts

| Artifact | sha256 | Bytes | Cargo features |
| --- | --- | --- | --- |
| Node tarball `layerfs-2.0.0.tgz` | `0511a7c19712f738e78a5dee7d4c131a6840a3571d8374c656e9fe4c4450d5e8` | 6,434,756 | default |
| addon inside it, `layerfs.node` | `14b812066276e74d63456a899fe45bfdfe65f7b0f62a1775f886345bc35054e4` | 15,695,296 | `layerfs-api/machine` |
| Python wheel `layerfs-2.0.0-cp310-abi3-manylinux_2_39_x86_64.whl` | `cfcc8f8a4419a16ef674157358b03bf6a0850a675ec3e5c5bc73129685d55527` | 6,505,248 | `machine` (default) |
| `layerfs` command | `b2face68ab5c9619c39be23f640331d2cef5e654cf2be8d01d39e768b40268bb` | 15,773,832 | `machine` (default) |
| Node tarball, PostgreSQL build | `1732e36b30575ca5445fe4dadfa81e8fbf07796a66a4b26e87c9f24c2e65e3ed` | 7,281,333 | `+postgresql` |
| addon inside it | `7ae927b2b4aecedae0e1c691cd979c073c0922b5412b08a5e03ebe35cb5ce97b` | 17,815,056 | `+postgresql` |
| Python wheel, PostgreSQL build (same name) | `a7cdbe22afa9097b17a037a246566c46f515c7f35591fb369dd983671171391a` | 10,576,475 | `+postgresql` |
| `layerfs` command, PostgreSQL build | `895289a7e79beae88dd456f8bb3e61610a7b42b7ea796c6c1f0e1ce80ac084d5` | 17,901,504 | `+postgresql` |

Contents. Tarball (8 files): `LICENSE`, `layerfs.node`, `package.json`,
`src/{errors,index,native,values}.js`, `src/index.d.ts`. Wheel: `layerfs/`
(`__init__.py`, `_client.py`, `_errors.py`, `_values.py`, `aio.py`, a `.pyi`
for each, `py.typed`, `_native.abi3.so` sha256 `db836ad5…06e4`) and
`dist-info` (`METADATA`, `WHEEL`, `RECORD`, `licenses/LICENSE`, a CycloneDX
SBOM). The PostgreSQL wheel adds `layerfs.libs/` with `libssl`, `libcrypto`
and `libzstd` copied from the build host.

Native dependencies (`ldd`): the default addon and command need only
`libgcc_s`, `libm`, `libc`. The PostgreSQL addon and command also need
`libssl.so.3`, `libcrypto.so.3`, `libz.so.1`, `libzstd.so.1` from the system.
`cargo tree` shows that both `postgresql` and `tls` pull `native-tls` and
`openssl-sys`. Every artifact needs `GLIBC_2.39` (`objdump -T`), so none runs
on a host older than that (for example Ubuntu 22.04 or Debian 12); not
checked on such a host.

## Support statement

Derived from the manifests and from what ran.

| Artifact | Declared | Run here |
| --- | --- | --- |
| Node `layerfs` | `engines.node >=22`; `build-native.mjs` builds on macOS or Linux only, any CPU, no `CARGO_BUILD_TARGET`; one addon per package, no per-platform prebuilds | linux/x64, Node 24.22.2 |
| Python `layerfs` | `requires-python >=3.10`, abi3 (one wheel for every CPython 3.10+); wheel tag follows the build host's glibc | cp310-abi3 wheel, Python 3.14.4, linux/x64 |
| `layerfs` command | `publish = false`, no install script; Cargo 1.85.1 | linux/x64 |
| Platforms a Sandbox can use (`connect/SUPPORT.json`, Linux host) | `native` (namespaces and FUSE, root), `docker`, `microsandbox`, `ssh`; `apple-container`, `lima`, `firecracker` are refused "runs on macOS hosts only" | none: no Sandbox was created (needs the daemon and the sealed Linux bundle) |

Client versus execution route. The three artifacts are clients: they run the
call in the calling process (sqlite on `native`, PostgreSQL over the network)
or send it to a machine's worker. The execution routes are separate and need
the sealed Linux bundle (`tools/linux_bundle.py`), which is in no artifact; its
absence is reported as `Unsupported` with the missing path (seen on
`machine.connect`).

## Runs from the installed artifacts

Each in a fresh directory on ext4, `NODE_PATH`, `PYTHONPATH`, `LAYERFS_NATIVE`
and `LAYERFS_HOME` unset, a private mode-0700 home, Disposable profile.

| Case | Result |
| --- | --- |
| `npm init -y && npm install <tgz>` in an empty project | RAN, exit 0, 1 package; the only `layerfs.node` on disk is `node_modules/layerfs/layerfs.node` |
| Node: `store.create`, `project.init`, `branch.fork`, `file.write`, `file.read`, `file.list`, `commit.list`, `doctor`, `close` | RAN, all OK (read back `"hello\n"`, list `README.md, docs`, 1 commit); table has 61 calls |
| Node: refusal `file.read` on a missing Branch | RAN, `LayerFSError` code `NoSuchBranch` |
| Node, F4: addon deleted, copies planted at `<project>/layerfs.node`, `node_modules/crates/layerfs-node/layerfs.node` and `<dir>/crates/layerfs-node/layerfs.node` | RAN: `import "layerfs"` throws "the LayerFS native addon is missing: <path> (the package's own; this is linux/x64, Node v24.22.2); build it with …"; no fallback. `LAYERFS_NATIVE=<file>` loads; a junk file gives "could not be loaded … file too short" |
| Node types and licence | RAN: `src/index.d.ts` present (18,348 bytes); `package.json` `license` MIT, `LICENSE` identical to the repository's |
| `python3 -m venv` and `pip install --no-index <wheel>` | RAN, exit 0; `pip show -f`: `License-Expression: MIT`, `layerfs/py.typed` and every `.pyi` installed |
| Python: the same eight calls, refusal, `doctor`, `close` | RAN, all OK; the refusal is `LayerFSError` `NoSuchBranch` |
| Python types | RAN: `mypy --strict` on a sample using `layerfs.Result` and `file.write(..., message=)` against the installed stubs: "Success: no issues found" |
| Command copied alone to an empty directory | RAN: `layerfs --version` `layerfs 2.0.0`; `store create`, `project init`, `branch fork`, `file write` (stdin), `file read --text`, `file list --text`, `commit list`, `doctor` all exit 0; the missing-Branch refusal exits 125 with `{"code","message","detail"}` |
| Same checks with the PostgreSQL tarball, wheel and command | see below |
| Python import without the native module | RAN: `import layerfs` with `_native` hidden fails with "cannot import name '_native' from partially initialized module 'layerfs' (most likely due to a circular import)", not a message that names the missing module or the build (a finding; no fix made) |

## PostgreSQL and TLS

| Case | Result |
| --- | --- |
| Default tarball, wheel and command pointed at a PostgreSQL Store (`store.connect`, URL, `password: env:…`) | RAN against a reachable server: all three refuse before any connection with `Unsupported`, "this LayerFS build has no PostgreSQL engine; build it with --features postgresql"; Node and Python raise `LayerFSError`, the command exits 125. No crash, nothing created. `detail` is `null` |
| Real server | RAN: a throwaway `postgres:17-alpine` container (server 17.11) on `127.0.0.1:55555`, a separate database per client, removed afterwards (`--rm`; not in `docker ps -a`) |
| PostgreSQL command | RAN: `store connect` (kind `postgresql`), `project init`, `branch fork`, `file write` (stdin), `file read --text` returned `x`, `store status` `reachable: true`; all exit 0 |
| PostgreSQL addon (installed tarball) | RAN: `store.connect`, `project.init`, `branch.fork`, `file.write`, `file.read` (`"x\n"`), `commit.list` (1). Writing straight to `main` is refused `ReadOnly` ("main changes only through publish"), as designed |
| PostgreSQL wheel (installed in a venv) | RAN: the same chain, read back `x`, 1 commit |
| `tls` feature (https buckets) | NOT RUN: not built, no bucket. `build-native.mjs --features tls` is accepted by the script but unbuilt |
| Opt-in path | `feat` commit `35bec0c97`: `build-native.mjs [--debug] [--features postgresql,tls]`; with no flag the build is unchanged. Wrong or missing feature names exit 1 with the usage line (five cases run). The wheel needs no change (`maturin build --features`); the command uses `cargo --features` |

Owner decision: whether published artifacts include PostgreSQL. Including it
adds an OpenSSL runtime dependency to the addon and the command (a bundled
copy in the wheel), about 2 MB to the addon and the command and 4 MB to the
wheel; the default stays feature-light.

## Documented setup commands

Run from the repository root of the pinned commit (a clean checkout, warm
Cargo target), `PATH` with the venv's `maturin`.

| Command (source) | Result |
| --- | --- |
| `npm --prefix api run build:native` (api/README, quickstart) | OK, exit 0 |
| `… build:native -- --debug` with `LAYERFS_NATIVE_OUTPUT=<file>` | OK, exit 0, 128 s cold |
| `cd api/python && maturin build --locked` (README, sdk, quickstart) | OK, exit 0 (debug wheel; docs now say `--release` for the wheel to install) |
| `cargo +1.85.1 build -p layerfs-cli` (quickstart); `… build --locked -p layerfs-cli` (node.md); `… build -p layerfs-python -p layerfs-cli` (python.md) | OK, exit 0 |
| `cp target/debug/lib_native.so api/python/src/layerfs/_native.abi3.so` (python README) | OK; `liblayerfs_python.so` is not the file name |
| `npm --prefix api run gen:types`; `python3 api/python/tools/gen_stubs.py --check` | OK, no diff |
| `node --test api/node/test/calls.mjs` | 3 pass |
| `npm --prefix api/node test` | 23 pass, 0 fail, 1 skipped (`events.mjs` exec output: `LAYERFS_DAEMON` unset, NOT RUN) |
| `python3 api/python/test/test_calls.py`; `python3 -m unittest discover -s api/python/test` | 7 tests OK (1 skipped); 39 tests OK (1 skipped, daemon) |
| `cargo +1.85.1 test --locked -p layerfs-cli --test doctor` under `alarm 100` | FAILED: exit 142 while compiling (cold test build 75 s plus the dependency build). Built first with `--no-run`, it passes: 2 passed. Fixed in the docs |
| `layerfs help`, `layerfs --version` | OK |

Missing before this work and added to the docs: any statement of how to pack
and install the three artifacts, that `npm pack` needs the addon, the opt-in
PostgreSQL builds. Fixed: `engine/crates/layerfs-python` path (sdk.md), stale
"Linux wheels NOT_RUN" claims. Fixed in code: `npm pack` without
`layerfs.node` produced a 7-file tarball, exit 0 (npm omits a listed file that
is missing); a `prepack` check now stops it (`254feec2d`).

Licence: workspace `Cargo.toml` MIT; `api/node/package.json` MIT, `LICENSE`
in the tarball identical to the repository's; wheel `License-Expression: MIT`,
`dist-info/licenses/LICENSE` identical. No Apache-2.0 string remains in the
checked docs.

## Machines and `firecracker` / `ssh` (F13)

`connect/SUPPORT.json` lists `firecracker` and `ssh` for macOS, Linux and
Windows hosts. A search of `support.rs`, `lib.rs` and `layerfs-control/src/machine` for a
switch that disables either finds none. Through
every installed client (Node, Python, command) on this Linux host:

- `machine.create fc1 firecracker`: `InvalidInput` "a firecracker machine
  names its lima --nested host"; with `--host nolima --nested`: `InvalidInput`
  "only a lima machine is nested". Both are argument checks: the platform is
  accepted as a value, so it is selectable at the API level.
- `machine.create l1 lima`: `Unsupported`, "lima runs on macOS hosts only".
- `machine.connect box1 127.0.0.1 --user root` (ssh): `Unsupported`, "no sealed
  Linux bundle at …/linux-x86_64"; it reaches the ssh adapter and stops on the
  missing bundle, not on a disabled platform.

A real firecracker or ssh machine was not created (no Lima, no bundle, no
host). Issue #7's "disabled as selectable" is not enforced at this source.

## NOT RUN

macOS (native, arm64, loopback NFS), Lima, Apple Container, Microsandbox,
Firecracker, Windows, Linux aarch64, Docker machine route and every Sandbox,
Workspace and daemon-backed call (`LAYERFS_DAEMON`, no sealed bundle), other
Python versions (3.10 to 3.13) and Node versions (22, 20), hosts with glibc
below 2.39, the `tls` build and any https bucket, `project.export` (#53 not
landed), MCP (shelved), CI (unchanged). Each Linux artifact was run on one
host only.

## Gaps and owner decisions

1. PostgreSQL/TLS in published artifacts (above); the refusal text says "build
   it with --features postgresql", which an npm or pip user cannot act on.
2. glibc floor: artifacts built here need 2.39. Release builds should come
   from the oldest supported host (for example the Bookworm image the bundle
   build already uses); the wheel tag would follow.
3. No publication runbook for npm or PyPI, no per-platform prebuilds or
   optional dependencies, no macOS or aarch64 artifacts; `release-policy.md`
   says nothing about them. Publishing is a separate release action.
4. CI excludes `layerfs-node` and the JS and Python packages (owner decision
   2026-10-23) and builds no artifact; nothing guards the pack and install
   steps above.
5. `firecracker` and `ssh` stay in `SUPPORT.json` and are accepted by
   `machine.create` and `machine.connect` (above).
6. Python missing-native import error is unclear (above); the Node loader
   message says "in the LayerFS repository", which does not help an
   installed-package user.
