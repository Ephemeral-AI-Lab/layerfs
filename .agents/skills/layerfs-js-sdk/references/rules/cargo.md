# Rust and Cargo in a Workspace

> **Status:** Current general guide. Live Cargo 1.85.1 checks in issue #337
> Phase A used product `db5b1bac9`; at that pin the receipt path was
> `core/target/issue337/phase-a/`. These are diagnostics, not benchmarks.

## Put all mutable Cargo state in the view

Supply Rust/Rustfmt/Clippy and SQLite build prerequisites in the pinned runtime
image. On every Bash call set the cache/output variables again: shell exports
do not persist between calls, and Workspace commands start with HOME=/tmp.

```sh
timeout 110 bash -c 'set -eu
export CARGO_HOME="$PWD/.cargo-home"
export CARGO_TARGET_DIR="$PWD/target"
export CARGO_NET_RETRY=0
export CARGO_BUILD_JOBS=3
unset RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER
cargo fetch --locked'
```

Use the assigned exact toolchain and Cargo.lock. Keep dependency source, archive
cache, compiler output and build-script output inside the mounted root. Do not
use an external Cargo home/target, sccache, host-built application artifacts or
edits to downloaded registry source. Image toolchains are separate from mutable
Cargo state. Cargo documents [Cargo home](https://doc.rust-lang.org/cargo/guide/cargo-home.html)
as its download/source cache and [target output](https://doc.rust-lang.org/cargo/reference/build-cache.html)
as its build cache. The #337 image has no application dependency cache.

## Keep a Workspace when build reuse matters

The ordinary default remains one mount/exec/commit/unmount per call. Cargo
steps may instead keep one Workspace mounted across a declared build sequence,
calling exec then commit for every step and unmounting at the end. This is the
existing [longer-lived Workspace behavior](../methods/workspace.md#workspaces).
Use the returned Workspace for every call; do not invent a fixed mount path.

A fresh mount is a different Workspace, with a different path. The #337 probe observed dependency
compilation after fresh mounts, while the unchanged build on a retained mount
reported no compilation and a source edit rebuilt only project crates. Keep
paths, toolchain, profile, features, flags and environment recorded; cache reuse
is an observation to check, not a guarantee inferred from the presence of target/.
Cargo's dep-info paths are absolute by default; its output layout is internal.
A source edit that the same Workspace must see before Commit may be performed
by a bounded command in that Workspace, as documented for workspace.exec. For
other targeted edits use fs.* and mount a fresh view before reading/building them.

## Split and label the work

Record dependency fetch, package-level first builds, an unchanged build, a source
edit/rebuild and a clean rebuild as distinct steps with their own Commit outcomes.
A clean rebuild removes only generated compilation output and retains the
declared dependency source cache. It is not a cold-network/cache claim.

Build each selected test target first with `cargo test --locked --offline
-p PACKAGE --no-run`, then execute it in another bounded call. Use package/target
commands when the whole workspace would exceed the assigned wall budget. Run
formatting, warning-denying scoped Clippy, applicable documentation tests and
package inspection according to the specification. `cargo package --list
--locked --allow-dirty -p PACKAGE` inspects contents; it does not publish.

Commit each Bash step even after a nonzero exit, and inspect refusals before
choosing another operation. A failed/uncertain Commit is not an UpToDate result;
retain the exact error, logs and Workspace state. Do not replay it blindly.
When replacing a daemon, preserve both log streams before close and keep the
existing Store/history unless its unusability has been established.

## Merge source and generated state deliberately

A Rust project can select only generated `target/` and `.cargo-home/` in its
explicit [LWW rules](lww.md). Never add Cargo.lock, Cargo manifests, source, tests
or application data to those rules. Inspect actual source conflicts, preserve
both changes, and rebuild/verify generated output after the source merge.

## Observed prerequisite checks

The #337 probe fetched 157 locked packages from a Workspace, built all four
packages, compiled tests before running a SQLite WAL/OFF close/reopen test,
passed scoped Clippy/formatting/package listings, completed a clean rebuild,
and clicked a real Chromium page through image-provided Selenium/Chromedriver.
It also executed the retained binary after a fresh mount. This qualifies the
selected workflow only; it is not Depot application acceptance.
