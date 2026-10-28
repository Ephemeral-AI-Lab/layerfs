# Parallel worktree development

> **Status:** Current general guide.

[tools/worktree.py](../../tools/worktree.py) assigns reusable build resources to
one checkout and writable Docker E2E state to one run. It does not schedule other
agents, alter product locking, run frozen benchmarks, or provide an aggregate
pre-push gate. Follow [repository instructions](../../AGENTS.md); the
[cleanup policy](cleanup.md) covers releasing its caches.

## Register a worktree

Use Python 3.9+, Git, Docker, `lsof`, and the installed Rust 1.85.1 toolchain for
macOS builds. Linux builds run in a prepared Rust image; the E2E client needs a
prepared Node runtime image. Images must already exist locally. Registration
resolves their immutable image IDs and never pulls or retags them. The sandbox
image is the image selected by the owning product test, with its required runtime
and FUSE capabilities; this tool does not manufacture or qualify it.

From the checkout containing the launcher:

```sh
python3 tools/worktree.py register --worktree /absolute/path/to/worktree-a --owner issue-owner --build-image existing-rust-image --runtime-image existing-node-image --sandbox-image existing-sandbox-image
```

The command prints an absolute `resources.json` path. Subsequent commands use
that path through `--resources`. The registration ID combines the checkout's
canonical path hash and readable name. A branch rename does not change it, and
two checkouts with the same basename receive different IDs. Existing identical
registrations are reused; conflicting image/owner inputs are refused.

The default state root is `~/.cache/layerfs-worktrees`; `register --root` selects
another location outside the checkout. This state contains resource registration,
leases, preparation/build logs, artifact bundles, and E2E receipts. It is protected
by exported cleanup scopes. A launcher from one checkout can operate another
registered checkout without copying scripts into it or merging branches first.

| Resource | Owner and placement |
| --- | --- |
| Source | Registered checkout; mounted at `/work` read-only during Linux builds |
| macOS Cargo target | That checkout's `target` |
| macOS Cargo home | Private directory in its registration state |
| Linux Cargo target | Labelled private Docker volume at `/work/target` |
| Linux Cargo home | Separate labelled private Docker volume at `/cargo-home` |
| Toolchain/images | Pinned toolchain and shared immutable installed images |
| E2E artifacts | Unique sealed bundle containing daemon, addon, SDK runtime/operator/test files, the `layerfs` command (`api/cli/src`, `api/cli/test`), hashes and source identity |
| E2E writable state | Unique per-run Store volume and SDK home, container prefix, trace, and output receipt |

## Prepare dependencies and optional target seeds

Preparation is explicit and network-enabled for dependency fetching. Ordinary
builds are offline and locked. An optional closed Cargo home seeds `registry` and
`git` only; credentials, installed tools and global configuration are not copied.
The destination must not already contain those seed directories. `lsof` and
container inspection must establish that the seed is idle. Never seed from a
cache another process is changing.

```sh
python3 tools/worktree.py prepare --resources /absolute/path/resources.json --platform macos --dependency-seed /absolute/path/to/closed-cargo-home
python3 tools/worktree.py prepare --resources /absolute/path/resources.json --platform linux --dependency-seed /absolute/path/to/closed-cargo-home
```

Omit `--dependency-seed` for a cold fetch or an explicit fetch after dependency
changes. Builds do not silently fetch missing dependencies, refresh a failed
operation, or fall back to somebody else's writable cache. Private Cargo homes
also keep writable package-cache metadata separate between worktrees.

Optionally seed a new target profile from another registered, quiescent worktree:

```sh
python3 tools/worktree.py seed --resources /absolute/path/a/resources.json --source-resources /absolute/path/seed/resources.json --platform linux --profile debug
```

The selected profile and target metadata are copied, not the entire historical
target tree. macOS uses an APFS clone copy; Linux uses an independent writable
volume copy. The destination profile must not exist. Image identities, lockfile
and repository Cargo configuration must match. Both worktree leases are acquired
without waiting; live Linux consumers or open macOS seed files block copying.
Cargo decides which copied artifacts remain reusable. Seeding is not a cold-cache
claim or a guarantee of zero recompilation. Keep source inputs quiescent for the
whole seed operation, including builds launched outside this tool.

## Scoped builds and tests

Publish a paired daemon/addon plus matching SDK files:

```sh
python3 tools/worktree.py build --resources /absolute/path/resources.json --platform linux --profile debug --jobs 2
```

The command prints the sealed bundle path. Select `--platform macos` for host
artifacts. macOS Store execution remains unsupported; Store E2E runs on Linux.
The Linux target mount stays inside `/work`, which also satisfies the existing
[native build script](../../api/scripts/build-native.mjs)'s path restriction.
The standalone `build:native` script remains available; the isolated launcher
publishes the daemon and addon together rather than overwriting a live addon.

Builds start at the checkout root with `--manifest-path Cargo.toml`, preserving
the root ARM64 Cargo profile. They reject inherited Rust flag/target/compiler
overrides that would bypass it. Source files, the lockfile, root configuration,
SDK runtime, operator and test files, and the `layerfs` command's source and
tests, are fingerprinted; a changed source identity
during build prevents publication. The SDK bundle is copied independently of the
live checkout and all artifact hashes are checked before an E2E run. Failed builds
remain unsealed. Do not run a partially published directory.

Run other selected Cargo checks through the same resources:

```sh
python3 tools/worktree.py cargo --resources /absolute/path/resources.json --platform linux -- test -p layerfs-storage --no-run
python3 tools/worktree.py cargo --resources /absolute/path/resources.json --platform linux --timeout 90 -- test -p layerfs-storage --test selected_test
python3 tools/worktree.py cargo --resources /absolute/path/resources.json --platform macos -- clippy -p layerfs-storage --all-targets -- -D warnings
```

The test example's `selected_test` must name an existing package test target.
An executing `cargo test` first compiles with `--no-run`; compilation is logged
separately. Test execution accepts an explicit timeout of at most 100 seconds,
leaving cancellation time under the repository's 120-second ceiling. Host timeout
cancels the invocation's process group; Linux timeout kills its acknowledged
container. Failed/interrupted build containers remain stopped for diagnosis, with
their IDs in the operation log directory. Release those exact containers after
diagnosis before asking cleanup to remove their referenced volumes.

One nonblocking lease protects a worktree/platform's writable build caches.
Another invocation against that cache fails immediately; A and B have different
leases and run concurrently. E2E runs do not acquire a build lease. Choose each
agent's `--jobs` according to shared CPU/memory capacity; private directories do
not eliminate resource interference. Iteration stays package-scoped; the launcher
refuses workspace aggregates, target/config overrides and `fs-bench-pro`.

## Run pinned Linux E2E artifacts

```sh
python3 tools/worktree.py run --resources /absolute/path/resources.json --bundle /absolute/path/to/sealed-linux-bundle --timeout 90 -- node api/agent/test/verify/direct_locking_real.mjs inner
```

Select the owning test's Linux entrypoint. Tests that bootstrap their own outer
Docker setup or override deployment settings need their inner entrypoint; the
launcher does not rewrite old campaigns. Only the pinned SDK/test tree is mounted,
not the live checkout. Generic correctness scripts can be placed in that tree
before building. The launcher accepts Node commands, not an implicit Bash/Cargo
build inside a test. From `api`, `npm run build:isolated -- ...` and
`npm run test:isolated -- ...` forward to the same launcher.

Each run receives its own named Store volume, private SDK home, unique container
prefix, trace and retained output directory. The SDK process opens the same local
Linux volume its daemons use. The native SDK selects Disposable explicitly;
Durable remains disabled. Several daemons created by one test intentionally share
that run's Store. Other runs have different volumes. No fixed host port is chosen
by this launcher; SDK daemon endpoint allocation keeps its existing behavior.
Docker Desktop uses the Linux engine socket and `host.docker.internal`; native
Linux uses host networking for the SDK's loopback-published daemon endpoints.

The command validates bundle ownership, image identities, file hashes and complete
namespace. The bundle is mounted read-only at the same absolute path in the client
container, so Docker API callers can locate the pinned daemon. It records full
acknowledged client IDs; teardown inspects and validates each exact child ID under
the run's UUID prefix before acting. It never stops another owner by process name
or issue prefix. This prefix is reserved for this run alone.

On success, the launcher stops remaining owned containers, removes the exact IDs
and disposable Store volume, and retains receipts/traces. On failure or timeout,
it stops owned execution and retains stopped containers and writable state for
diagnosis. Container inspection/teardown shares a 120-second run deadline; any
failure or uncertain cancellation is recorded and never called PASS. The launcher
does not automatically retry a failed test. These are correctness runs; performance
qualification still needs declared cache state and interference control.

After diagnosis, explicitly release a retained run:

```sh
python3 tools/worktree.py teardown --resources /absolute/path/resources.json --run-receipt /absolute/path/to/run/receipt.json --release-failed --reason "diagnosis complete; state released"
```

This writes a separate disposal record and preserves the original failed receipt.
Inspect any partial/uncertain teardown before selecting another operation.

## Export cleanup releases

At assignment handoff, export an exact cache release scope:

```sh
python3 tools/worktree.py cleanup-scope --resources /absolute/path/resources.json --release-caches --reason "assignment handed off; caches released" --out /absolute/path/new-scope.json
python3 tools/cleanup.py plan --scope /absolute/path/new-scope.json --out /absolute/path/new-cleanup-pass --reclaim-gb 300
```

Without `--release-caches`, the scope selects nothing. Exported scopes protect the
registration state, all pinned bundles/records and retained failed-run volumes.
Generated macOS compiler children and the owned Linux cache volumes are selected
only with the explicit handoff release. The cleanup script independently rechecks
active processes and all container consumers before any deletion. Ordinary cleanup
can omit `--reclaim-gb` when no low-space episode is active. The launcher does
not remove checkouts or frozen evidence, and never runs a global Docker prune.


## Existing registrations after the layer split

After rebasing across #463, close build/test owners in your selected worktree.
For a registration made before the move, retain its original `resources.json`
and update only that registration's `macos_target` from the selected checkout's
`core/target` to its `target` directory. The Linux volume names and Cargo homes
remain private and unchanged; the launcher now mounts the target at `/work/target`.
Run `prepare` again for each platform you use. Registration verifies exact inputs
and does not silently rewrite an older record; `prepare` reads the selected
record, so its target path must be updated first. Never scan or migrate another
worktree's records. Keep an active cache warm and release the old cache through
the cleanup procedure at handoff; retain records, sealed bundles and failed-run
evidence.
