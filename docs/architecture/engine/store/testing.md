# Testing the Store

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The Store suites create real SQLite Stores, and a Store opens only on a local
filesystem that carries its locks. This page is the environment contract, and
the harness that enforces it before cargo spends a build.

| | |
| --- | --- |
| Harness | [`tools/ci/test_store_ext4.sh`](../../../../tools/ci/test_store_ext4.sh) |
| Suites | `layerfs-persistence`, `layerfs-store-service`, `layerfs-store`, and the `store_*` targets of `layerfs-api` |
| Guard | [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs) |
| Related | [Store](README.md), [SQLite](sqlite.md), [PostgreSQL](postgresql.md), [Diagnostics](diagnostics.md) |

## Why the tests need ext4

`kernel_guard::check` runs before a Store file is opened. It admits direct
opening only when the Store's own directory is a real directory (not a
symlink), owned by the current effective user, with mode `0700`, and the
filesystem holding it is `ext4`, `xfs` or `btrfs` on Linux, or `apfs` or `hfs`
on macOS. Anything else is `OpenRefusal::Unsupported` with the reason
`filesystem 0x… at … does not carry SQLite locks; direct Store access needs
ext4, xfs or btrfs`
([`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs)).

The allowlist is not arbitrary. Every opener of one Store must share one kernel
and a filesystem with working advisory locks: SQLite's WAL locks and the writer
admission lane's `flock` on the Store's directory
([SQLite](sqlite.md)). `tmpfs`, `overlay`, FUSE mounts and network filesystems
do not coordinate those locks between the processes a suite starts, so a Store
there is refused rather than run without coordination.

The suites keep their Stores in two directories, and both inherit the
filesystem of the test environment:

| Directory | In the tests | Usual host |
| --- | --- | --- |
| `TMPDIR` | `std::env::temp_dir()` | `/tmp`, which is tmpfs in most containers |
| `$CARGO_TARGET_DIR/tmp` | `env!("CARGO_TARGET_TMPDIR")` | the checkout's `target/`, an overlay in a container |

On a host whose `/tmp` is tmpfs, or in a container whose checkout is an
overlay, the guard refuses the Store and suites such as
`layerfs-store-service --test kernel_guard` fail at their "a private directory
on a supported filesystem passes" assertion. That is an environment failure,
not a product failure. Tests that must prove the refusal on purpose use
`/dev/shm` (tmpfs), so pointing `TMPDIR` at ext4 does not weaken them.

## Run the suites

The harness requires `LAYERFS_TEST_TMPDIR` on a supported filesystem, refuses
anything else with exit 2 before cargo starts, and points `TMPDIR` (through a
short symlink), `CARGO_TARGET_DIR` and `CARGO_HOME` at it.

```bash
# A cross-crate subset: the first build is the cost, then minutes.
LAYERFS_TEST_TMPDIR=/mnt/ext4/layerfs-store-tests tools/ci/test_store_ext4.sh --quick

# Every Store suite.
LAYERFS_TEST_TMPDIR=/mnt/ext4/layerfs-store-tests tools/ci/test_store_ext4.sh

# Print the cargo commands and the environment, run nothing.
tools/ci/test_store_ext4.sh --list
```

`--quick` runs a few targets per suite:

| Suite | Targets | What they exercise |
| --- | --- | --- |
| `layerfs-persistence` | `heads`, `sqlite_publication` | Branch heads and a publication round trip |
| `layerfs-store-service` | `kernel_guard`, `commit_pipeline` | the guard admits a private ext4 directory and refuses tmpfs; a Commit pipeline |
| `layerfs-store` | `create_directory` | `store.create`'s directory ownership and its guard refusal |
| `layerfs-api` | `store_family` | the Store family's call table |

The full run is the unfiltered `layerfs-persistence`, `layerfs-store-service`
and `layerfs-store` suites, plus every `store_*.rs` target found under
`api/crates/layerfs-api/tests/`. Other API targets (machines, sandboxes,
Docker) are not Store tests and are not run here.

## What the harness sets

| Variable | Value | Why |
| --- | --- | --- |
| `LAYERFS_TEST_TMPDIR` | required, validated | the supported filesystem the run may use |
| `TMPDIR` | a short private wrapper under the system temporary directory, whose `tmp` entry links to `$LAYERFS_TEST_TMPDIR/tmp` | `std::env::temp_dir()` in the tests; Unix-domain socket paths are limited to about 108 bytes |
| `CARGO_TARGET_DIR` | `$LAYERFS_TEST_TMPDIR/target` | build output, and `CARGO_TARGET_TMPDIR` for the tests |
| `CARGO_HOME` | `$LAYERFS_TEST_TMPDIR/cargo-home` | the registry and git checkouts stay with the run, off the checkout's overlay |
| `-j` | `min(nproc, 16)` by default | keeps rustc within memory on a many-core host |
| `CARGO_NET_GIT_FETCH_WITH_CLI` | `true` | fetches the pinned `fuser` git dependency with the git CLI, which honours the host's proxy and credentials |
| `GIT_SSL_NO_VERIFY` | `true` | lets that one git fetch complete through TLS inspection; scoped to this process, not a production setting |
| `--features postgresql` | added to every suite when `LAYERFS_TEST_POSTGRES_URL` is set | the PostgreSQL test files are `#![cfg(feature = "postgresql")]` |

The `TMPDIR` wrapper exists because a Sandbox run directory is long: a test that
binds a Unix-domain socket under `TMPDIR` otherwise fails with `path must be
shorter than SUN_LEN`. The wrapper is a `mktemp -d` directory under `/tmp`
(override with `LAYERFS_TEST_SYSTEM_TMP`), removed on exit, and holds only the
symlink; `statfs` follows it, so the Stores are still on the supported
filesystem.

Overrides: `LAYERFS_TEST_JOBS`, `LAYERFS_TEST_CARGO_HOME`,
`LAYERFS_TEST_CARGO_TARGET_DIR` and `LAYERFS_TEST_SYSTEM_TMP`. Arguments after
`--` are appended to every `cargo test`, for example
`tools/ci/test_store_ext4.sh --quick -- --nocapture`.

## Find a supported directory

On Linux, ask the mount table:

```bash
findmnt -n -o FSTYPE --target /path/to/candidate   # ext4, xfs or btrfs
```

Inside a Docker Sandbox (the `namespaces` runtime), the sandbox run directory
is ext4 while `/tmp` is tmpfs and the checkout is an overlay: `$HOME` and
`/layerfs/data/machine/native/run/<sandbox>` report ext4, so a directory such
as `/layerfs/data/machine/native/run/<sandbox>/tmp/layerfs-store-tests` is a
valid `LAYERFS_TEST_TMPDIR`. Never use `/tmp`.

On macOS the corresponding local filesystems are `apfs` and `hfs`; the
harness reads the mount table there too.

The build and the Stores live under `LAYERFS_TEST_TMPDIR`, so allow several
gigabytes on that mount.

## PostgreSQL

`LAYERFS_TEST_POSTGRES_URL` is a libpq connection string (despite the name),
not a URL:

```bash
LAYERFS_TEST_POSTGRES_URL='host=127.0.0.1 port=55433 user=postgres password=… dbname=postgres sslmode=disable' \
  LAYERFS_TEST_TMPDIR=/mnt/ext4/layerfs-store-tests tools/ci/test_store_ext4.sh
```

Use PostgreSQL 17 or newer. The Store schema itself needs 16 or newer for
`STORAGE PLAIN` ([PostgreSQL](postgresql.md)), and the PostgreSQL suites are
qualified on 17.x, so 17 is the supported version for tests.

Each PostgreSQL test creates and drops its own schema: point the variable at a
scratch server, never at a schema that holds a Store you care about, and make
sure the server is reachable from the process that runs the tests (inside a
Sandbox, `127.0.0.1` is the Sandbox). Without the variable the PostgreSQL test
files print `NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset` and check nothing;
the SQLite Store suites still run and still need the supported filesystem.

## Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| `LAYERFS_TEST_TMPDIR is not set.` | the harness was run without the variable | set it to a directory on a supported filesystem |
| `… is on 'tmpfs', which cannot carry a Store.` | `/tmp` (or another tmpfs/overlay) was chosen | pick ext4, xfs or btrfs on Linux, apfs or hfs on macOS |
| a suite fails with `filesystem 0x… at … does not carry SQLite locks` | `TMPDIR` or `CARGO_TARGET_DIR` escaped the harness (exported by hand) | let the harness set both; do not override them to a scratch directory |
| `NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset` | no PostgreSQL server | expected; set the variable to run those cases |
| `fatal: unable to access 'https://github.com/…/fuser.git'` or a TLS error | the pinned `fuser` git dependency could not be fetched | the harness sets `CARGO_NET_GIT_FETCH_WITH_CLI=true` and `GIT_SSL_NO_VERIFY=true`; check egress to `github.com` |
| `has mode 0755 … fix with chmod 700` | a left-over Store directory with group/other access | remove the named directory; the guard never changes permissions |
| `Updating crates.io index` stays silent for minutes | the sandbox network can stall cargo's multiplexed sparse-index connection | re-run with `CARGO_HTTP_MULTIPLEXING=false`; the download cache resumes where it stopped |
| `path must be shorter than SUN_LEN` | a Unix-domain socket path exceeded the kernel limit, usually because `TMPDIR` was set long by hand | use the harness (it keeps `TMPDIR` short) or pass a short `LAYERFS_TEST_TMPDIR` |
| `Store busy before effect` | another writer held the Store | not an environment failure; rerun the suite |

The repository's test selection and CI lanes are separate: `tools/ci_tests.py`
decides which binaries the push and nightly runs take, and this harness only
gives the Store suites the filesystem they need.
