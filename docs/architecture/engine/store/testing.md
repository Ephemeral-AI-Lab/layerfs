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
| CI | [`.github/workflows/store-ext4.yml`](../../../../.github/workflows/store-ext4.yml) |
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

For a run, the harness requires `LAYERFS_TEST_TMPDIR` on a supported
filesystem, refuses anything else with exit 2 before cargo starts, and points
`TMPDIR` (through a short symlink), `CARGO_TARGET_DIR` and `CARGO_HOME` at it.
`--list` prints the same cargo commands and environment without touching the
filesystem, so it needs no `LAYERFS_TEST_TMPDIR`.

```bash
# A cross-crate subset: the first build is the cost, then minutes.
LAYERFS_TEST_TMPDIR=/mnt/ext4/layerfs-store-tests tools/ci/test_store_ext4.sh --quick

# Every Store suite.
LAYERFS_TEST_TMPDIR=/mnt/ext4/layerfs-store-tests tools/ci/test_store_ext4.sh

# Print the cargo commands and the environment, run nothing.
# --list needs no LAYERFS_TEST_TMPDIR: it prints the commands and exits.
tools/ci/test_store_ext4.sh --list
```

`--quick` runs a few targets per suite:

| Suite | Targets | What they exercise |
| --- | --- | --- |
| `layerfs-persistence` | `heads`, `sqlite_publication` | Branch heads and a publication round trip |
| `layerfs-store-service` | `kernel_guard`, `commit_pipeline` | the guard admits a private ext4 directory and refuses tmpfs; a Commit pipeline |
| `layerfs-store` | `create_directory`, `compact_report_status` | `store.create`'s directory ownership and guard refusal; the §11 report wire shape and the `store.status` compaction op |
| `layerfs-api` | `store_family`, `calls_table`, `gc_calls`, `gc_trace` | the Store family and GC call tables, the four GC result shapes and the trace redaction |
| `layerfs-cli` | `gc`, `sqlite_compact_process_kill` | the four GC commands and help; the real `layerfs store compact` process killed at the named failpoints (T-G65) |

The full run is the unfiltered `layerfs-persistence`, `layerfs-store-service`
and `layerfs-store` suites, every `store_*.rs` target found under
`api/crates/layerfs-api/tests/`, plus `calls_table`, `gc_calls`, `gc_trace`,
`calls_scope`, `layerfs-cli`'s `gc` and `sqlite_compact_process_kill`. Other
API targets (machines, sandboxes, Docker) are not Store tests and are not run
here.

## What the harness sets

| Variable | Value | Why |
| --- | --- | --- |
| `LAYERFS_TEST_TMPDIR` | required for a run, validated; `--list` does not check it | the supported filesystem the run may use |
| `TMPDIR` | a short private wrapper under the system temporary directory, whose `tmp` entry links to `$LAYERFS_TEST_TMPDIR/tmp` | `std::env::temp_dir()` in the tests; Unix-domain socket paths are limited to about 108 bytes |
| `CARGO_TARGET_DIR` | `$LAYERFS_TEST_TMPDIR/target` | build output, and `CARGO_TARGET_TMPDIR` for the tests |
| `CARGO_HOME` | `$LAYERFS_TEST_TMPDIR/cargo-home` | the registry and git checkouts stay with the run, off the checkout's overlay |
| `-j` | `min(nproc, 16)` by default | keeps rustc within memory on a many-core host |
| `CARGO_NET_GIT_FETCH_WITH_CLI` | `true` | fetches the pinned `fuser` git dependency with the git CLI, which honours the host's proxy and credentials |
| `GIT_SSL_NO_VERIFY` | `true` | lets that one git fetch complete through TLS inspection; scoped to this process, not a production setting |
| `LAYERFS_TEST_POSTGRES_URL` | passed to the suites as-is | enables the PostgreSQL cases; the engine is always built beside SQLite (#726), so no feature flag is added |

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

### Server and extensions

Use PostgreSQL 17 or newer. The Store schema itself needs 16 or newer for
`STORAGE PLAIN` ([PostgreSQL](postgresql.md)), and the PostgreSQL suites are
qualified on 17.x, so 17 is the supported version for tests.

The full-mode doctor calls `amcheck`'s `verify_heapam` and `bt_index_check` for
every heap and ready B-tree relation, and it never installs an extension itself
([Diagnostics](diagnostics.md)). Create the contrib extensions in the test
database before a full audit:

```sql
CREATE EXTENSION IF NOT EXISTS amcheck;
CREATE EXTENSION IF NOT EXISTS pgstattuple;
```

```bash
psql "$LAYERFS_TEST_POSTGRES_URL" -c 'CREATE EXTENSION IF NOT EXISTS amcheck;'
```

`amcheck` is the required one: without it a full audit reports each relation as
`store.database.relations=Unknown`, because no supported function exists, and
`store.database.relations.summary` is `Unknown` too. That is missing coverage,
not corruption, but the PostgreSQL compaction tests assert that a rebuilt Store
audits cleanly with no `Fail` or `Unknown` observation. On a fresh cluster that
never created the extension, the five `clean()` assertions in
`postgresql_compact.rs` fail with exactly that finding; the
`assert_doctor_clean` assertions in `postgresql_compact_fixtures.rs` depend on
it too. `pgstattuple` is the companion contrib module; the current diagnostic
calls only `amcheck`, but the CI lane creates both so the scratch server
matches the qualified setup.

The push lane is SQLite-only and does not need any of this. When
`LAYERFS_TEST_POSTGRES_URL` is set, the harness asks `psql` whether the
extensions exist and warns with the `CREATE EXTENSION` command when `amcheck`
is missing; an absent `psql` or an unreachable server downgrades that to a
message and never fails the run. `LAYERFS_TEST_EXPECT_AMCHECK` (any value)
makes `postgresql_diagnostic` assert `store.database.relations.summary` is
`Pass`; the `store-ext4` full job sets it after creating the extensions.

### Optional server features and their NOT_RUN cases

A few PostgreSQL targets need server features beyond a plain Store schema.
Each one checks the fixture first and prints `NOT_RUN` with the reason instead
of failing when the feature is absent, so the same harness covers both a
trust-authenticated scratch server and the fully provisioned CI lane:

- `postgresql_phase_attribution` and `postgresql_round_trips` read
  `pg_stat_statements` for their server-side statement deltas. The library must
  be preloaded (`shared_preload_libraries` includes `pg_stat_statements`) and
  the test database must allow `CREATE EXTENSION pg_stat_statements`. Without
  it both print `NOT_RUN: pg_stat_statements is not preloaded (E55000 …)`;
  creating the extension alone is not enough, because the view refuses to run.
- `postgresql_doctor_access`'s wrong-password case needs password
  authentication (scram-sha-256 or md5). With trust or peer authentication the
  server accepts any password, so the case prints `NOT_RUN`; the saved
  authority, schema and unreachable-endpoint transport assertions still run.
- Its TLS case needs `ssl = on` with a self-signed server certificate, so
  `sslmode=require` fails normal certificate validation. With `ssl = off` it
  prints `NOT_RUN`, and the unreachable-endpoint case still checks transport
  distinctness.

### Credentials and TLS

The PostgreSQL test fixtures and the doctor's admin client connect with
`postgres::Client::connect(…, NoTls)`, not through the Store's TLS policy. The
connection string must therefore carry its own password and must not require a
TLS handshake: use `sslmode=disable` on loopback or a private route, and
`password=…` unless the server uses trust authentication. `sslmode=require` or
`verify-full` is fine for the Store sessions, but the test fixtures reject it.
Do not put a password in a job log; pass the whole connection string through
the environment, as the workflow does with a scratch trust-authenticated
service.

### The physical doctor's extra servers

`postgresql_doctor_physical` needs two servers beyond the primary:

| Variable | What it must be |
| --- | --- |
| `LAYERFS_TEST_POSTGRES_MISSING_URL` | a second database (or server) where `amcheck` is **not** installed; the test asserts that the missing extension is `Unknown` and that the doctor did not install it |
| `LAYERFS_TEST_POSTGRES_REPLICA_URL` | a physical standby of the primary, in recovery (`pg_is_in_recovery()` is true), with the test's schema replicated within 5 s |

Create the amcheck-free database on the same scratch server, for example:

```bash
psql "$LAYERFS_TEST_POSTGRES_URL" -c 'CREATE DATABASE layerfs_no_amcheck'
LAYERFS_TEST_POSTGRES_MISSING_URL='host=127.0.0.1 port=5432 user=postgres dbname=layerfs_no_amcheck sslmode=disable'
```

Both variables are optional: with the primary URL set, a test that needs one
and finds it unset prints `NOT_RUN: LAYERFS_TEST_POSTGRES_MISSING_URL is unset`
(or the replica variable) and returns. The `store-ext4` full job has no physical
standby and sets neither, so those extra cases print `NOT_RUN` and the target
stays green. Run the target separately with both for the physical coverage.

Each PostgreSQL test creates and drops its own schema: point the variables at a
scratch server, never at a schema that holds a Store you care about, and make
sure the server is reachable from the process that runs the tests (inside a
Sandbox, `127.0.0.1` is the Sandbox). Without the variable the PostgreSQL test
files print `NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset` and check nothing;
the SQLite Store suites still run and still need the supported filesystem.

### Compaction tests and per-test bindings

The T9 PostgreSQL compaction suite (`postgresql_compact.rs`) gives every test
its own catalog binding, `postgresql-compact-<tag>`. PostgreSQL advisory locks
are server-wide, not schema-scoped, so two tests that shared a binding would
exclude each other while running in parallel even though each owns its schema.
The per-test binding fix is already in place; keep it when adding a case. Those
tests need only `LAYERFS_TEST_POSTGRES_URL`, `amcheck` and the supported
`LAYERFS_TEST_TMPDIR`, not the physical doctor's two extra URLs.

## CI

[`.github/workflows/store-ext4.yml`](../../../../.github/workflows/store-ext4.yml)
runs this harness in the repository's report-only CI. It adds no test
selection: every target it runs is already in the push or nightly lane that
[`tools/ci_tests.py`](../../../../tools/ci_tests.py) chooses; it runs them where
their guard admits a Store. It runs for pull requests into `main` or
`layerfs_v2_0_0`, the 2.0 release base; every other lane runs only for `main`,
so a PR into the 2.0 base carries this Store evidence and no other lane.
Like the other lanes it is not a required check, an
agent does not wait for it, and a green run is not verification of a change
([AGENTS.md](../../../../AGENTS.md) §4).

| Job | Trigger | Command | PostgreSQL |
| --- | --- | --- | --- |
| `store ext4 (--quick)` | push to `main`; pull requests into `main` or `layerfs_v2_0_0` (the 2.0 release base), touching `core/**`, `engine/**`, `control/**`, `api/crates/**` (not `layerfs-node`), `Cargo.*`, `.config/**`, `tools/ci/**`, `.cargo/**`; a markdown-only change does not run it | `tools/ci/test_store_ext4.sh --quick` | off |
| `store ext4 (--full, PostgreSQL 17)` | `schedule` (04:47 UTC) and manual dispatch | `tools/ci/test_store_ext4.sh --full` | a `postgres:17` service container |

Both jobs set `LAYERFS_TEST_TMPDIR=/mnt/layerfs-store-tests`. On the
`ubuntu-24.04` runner `/mnt` is a separate ext4 data disk and `/tmp` is tmpfs;
the prepare step creates the directory, hands it to the runner user (it belongs
to `root` on the image) and asserts the filesystem with `findmnt`, so a runner
image without an ext4 `/mnt` fails in that step instead of as a guard refusal
inside a suite. The harness then sets `TMPDIR`, `CARGO_TARGET_DIR` and
`CARGO_HOME` itself, as it does locally.

The scheduled job sets `LAYERFS_TEST_POSTGRES_URL` to
`host=127.0.0.1 port=5432 user=postgres dbname=postgres sslmode=disable`
against that scratch service container, with trust authentication, so every
suite runs its PostgreSQL cases. Each PostgreSQL test creates and drops its own
schema, nothing outlives the run, and the runner reaches the container's
published port on `127.0.0.1`. The push job leaves the variable unset, so its
PostgreSQL cases print `NOT_RUN` and only the SQLite suites run.

Before the suites, the full job installs `postgresql-client` and creates
`amcheck` and `pgstattuple` in the service container's database, then sets
`LAYERFS_TEST_EXPECT_AMCHECK=1` so `postgresql_diagnostic` asserts the
full-mode relation summary is `Pass`. A server that did not take the extension
fails in that step with its name, instead of as five `Unknown` findings inside
the compaction suite. The job also runs `postgresql_doctor_physical`. It needs
`LAYERFS_TEST_POSTGRES_MISSING_URL` and `LAYERFS_TEST_POSTGRES_REPLICA_URL` for
its extra cases; the full job provides neither, so those cases print `NOT_RUN`
and the target stays green. Run it separately with both for that coverage
(above).

The harness moves `CARGO_HOME` and `CARGO_TARGET_DIR` onto
`LAYERFS_TEST_TMPDIR`, so these jobs do not restore the checkout-scoped
`Swatinem/rust-cache` the other workflows use: it would restore a `target/`
that the harness never reads. They install `mold` as the fast lane's `test` job
does, because they link the same test profile from a cold cache, and carry a
60-minute (`--quick`) or 120-minute (`--full`) job timeout.

## Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| `LAYERFS_TEST_TMPDIR is not set.` | the harness was run without the variable | set it to a directory on a supported filesystem |
| `… is on 'tmpfs', which cannot carry a Store.` | `/tmp` (or another tmpfs/overlay) was chosen | pick ext4, xfs or btrfs on Linux, apfs or hfs on macOS |
| a suite fails with `filesystem 0x… at … does not carry SQLite locks` | `TMPDIR` or `CARGO_TARGET_DIR` escaped the harness (exported by hand) | let the harness set both; do not override them to a scratch directory |
| `NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset` | no PostgreSQL server | expected; set the variable to run those cases |
| `NOT_RUN: pg_stat_statements is not preloaded (E55000 …)` | the measurement targets read server statement counts, and `CREATE EXTENSION` alone does not load the library | add `pg_stat_statements` to `shared_preload_libraries` on the scratch server to measure; otherwise those targets skip and the rest of the suites stay green |
| `store.database.relations=Unknown` and a compaction test fails on `clean` | `amcheck` was never created in the test database | `CREATE EXTENSION IF NOT EXISTS amcheck;`, then re-run; the harness warns when `psql` can see it is missing |
| `NOT_RUN: LAYERFS_TEST_POSTGRES_MISSING_URL is unset` (or `…_REPLICA_URL`) from `postgresql_doctor_physical` | the physical target's extra cases need an amcheck-free database and a physical standby; without them those cases skip | expected in the full sweep; set both (see [the physical doctor's extra servers](#the-physical-doctors-extra-servers)) and run that target separately for the physical coverage |
| `error: TLS support is disabled` or psql prompts for a password | the fixtures use `NoTls` and read the conninfo only | put `password=…` in the connection string and use `sslmode=disable` on a private route |
| `fatal: unable to access 'https://github.com/…/fuser.git'` or a TLS error | the pinned `fuser` git dependency could not be fetched | the harness sets `CARGO_NET_GIT_FETCH_WITH_CLI=true` and `GIT_SSL_NO_VERIFY=true`; check egress to `github.com` |
| `has mode 0755 … fix with chmod 700` | a left-over Store directory with group/other access | remove the named directory; the guard never changes permissions |
| `Updating crates.io index` stays silent for minutes | the sandbox network can stall cargo's multiplexed sparse-index connection | re-run with `CARGO_HTTP_MULTIPLEXING=false`; the download cache resumes where it stopped |
| `path must be shorter than SUN_LEN` | a Unix-domain socket path exceeded the kernel limit, usually because `TMPDIR` was set long by hand | use the harness (it keeps `TMPDIR` short) or pass a short `LAYERFS_TEST_TMPDIR` |
| `Store busy before effect` | another writer held the Store | not an environment failure; rerun the suite |

The repository's test selection and CI lanes are separate: `tools/ci_tests.py`
decides which binaries the push and nightly runs take, and this harness only
gives the Store suites the filesystem they need.
