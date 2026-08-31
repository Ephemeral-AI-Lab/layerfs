# LayerFS 0.1.0 verification

> **Status:** Release candidate. Final results and source identities remain to
> be filled from the immutable `v0.1.0` release candidate.

This document defines the terminal release checks and the evidence that must be
recorded for LayerFS 0.1.0. Commands run against the exact release source from
the repository root unless stated otherwise.

## Verification record

| Field | Result |
| --- | --- |
| Git tag | **TO BE FILLED AT RELEASE** |
| Git commit | **TO BE FILLED AT RELEASE** |
| Clean source-tree proof | **TO BE FILLED AT RELEASE** |
| Rust toolchain | **TO BE FILLED AT RELEASE** |
| Host and architecture | **TO BE FILLED AT RELEASE** |
| Started at UTC | **TO BE FILLED AT RELEASE** |
| Finished at UTC | **TO BE FILLED AT RELEASE** |
| Overall result | **TO BE FILLED AT RELEASE: PASS or FAIL** |
| Evidence bundle SHA-256 | **TO BE FILLED AT RELEASE** |

No gate may be recorded as passing from an earlier source identity. A failed
gate requires correction and a complete affected verification cycle before
release acceptance.

## 1. Source and dependency identity

Record the following before compilation:

```bash
git status --short
git rev-parse HEAD
git rev-parse 'HEAD^{tree}'
git log -1 --oneline --decorate
cargo metadata --locked --format-version 1
```

Record the lockfile digest with `shasum -a 256 Cargo.lock` on macOS or
`sha256sum Cargo.lock` on Linux.

The release verification tree MUST be clean. The tag and commit MUST match the
release identity in [release-contract.md](release-contract.md), and the locked
dependency graph MUST be retained in the evidence bundle.

## 2. Mandatory workspace gates

```bash
cargo fmt --all -- --check
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
git diff --check
```

All commands MUST exit zero. Test filtering, ignored failures, warning
allowlists introduced only for release verification, and retries that omit a
failed test are not acceptable.

## 3. Store and storage-format proof

Verification MUST demonstrate:

- creation and reopening of one Store through the public API;
- exact schema and standalone SQL inventory matching the
  [storage-format contract](../../docs/versioned/0.1.0/storage-format.md);
- foreign-key, uniqueness, immutable-identity, and scoped-name enforcement;
- authentication failure on corrupted canonical bytes;
- bounded object membership and object-read batches;
- missing-only canonical-object admission and Store-wide deduplication;
- zero canonical-object copies for local Branch Fork;
- immutable Layer and Commit records;
- Branch head/base publication after reachable closure admission; and
- injected transaction failures leaving no visible incomplete head.

Schema inspection and SQL traces MUST be captured as raw evidence, not only as
a prose summary.

## 4. SDK and CLI proof

The public SDK and CLI examples in the versioned references MUST execute
against a fresh Store:

- [SDK reference](../../docs/versioned/0.1.0/sdk.md)
- [CLI reference](../../docs/versioned/0.1.0/cli.md)

The proof MUST cover LayerStack initialization, Branch Fork, bounded queries,
supported diffs, Workspace Create, fresh-process Exec, paged Output, Commit,
stale reconciliation, Add Layer, Monitor snapshot/dedup analysis, and explicit
Workspace End. CLI text and JSON modes MUST describe the same operation result
and use stable typed IDs.

Negative cases MUST include invalid names and IDs, name conflicts, stale
Workspace Commit, invalid cursors, incompatible projection/placement, active
context rebinding, and operations after Workspace End.

## 5. Workspace and projection proof

Materialized and FUSE Workspaces MUST pass the same logical filesystem cases:

- regular files, directories, symlinks, hard links, modes, and modification
  times;
- sparse and zero ranges;
- append, overwrite, prepend, truncate, rename, unlink, and temp-copy-rename;
- count-changing edits at the beginning, middle, and end of large files;
- many small edits mixed with medium and large edits;
- read-after-write and hard-link-alias visibility;
- `fsync`, Commit pause/fence, deferred write errors, and interrupted writes;
- Commit followed by authority readback and fresh Workspace reopening; and
- clean End and explicit discard, with no implicit Commit.

Real-FUSE proof MUST use `/dev/fuse`; a mock adapter is insufficient. Managed
container proof MUST also validate the security and placement rules in the
[container-runtime contract](../../docs/versioned/0.1.0/container-runtime.md),
including loopback daemon publication, no Store/payload bind mount, expected
capabilities, resource limits, mount readiness, and cleanup.

## 6. Bounded-resource and concurrency proof

Verification MUST record peak resident memory and demonstrate bounded behavior
for:

- large high-entropy sequential writes and reads;
- a large canonical-object membership set;
- paged queries and output;
- candidate spill and capture backpressure;
- multiple active Workspaces within the documented limits; and
- Commit or End racing active FUSE callbacks and executions.

No network, full-history enumeration, or unbounded object materialization may
occur while a SQLite write transaction is open. Transaction traces MUST show
that publication is short and visibility-last.

## 7. Benchmark evidence

Performance is an evidence gate, not a substitute for the semantic tests above.
The final report, environment, workload identity, source seal, raw samples,
correctness oracles, storage accounting, and statistical treatment are
recorded in [benchmark-results.md](benchmark-results.md).

The benchmark MUST use public operations, a real FUSE mount where specified,
fresh workload processes, high-entropy fixtures, isolated cases, and identical
acknowledgement boundaries. Setup-only caches may not enter the measured
interval, and a failed correctness oracle invalidates its sample.

## 8. Documentation and artifact audit

Before publication:

1. Check every link in the [versioned documentation index](../../docs/versioned/0.1.0/README.md).
2. Verify the [quickstart](../../docs/versioned/0.1.0/quickstart.md) from an
   empty working directory.
3. Reconcile the [specification](../../docs/versioned/0.1.0/specification.md),
   CLI, SDK, container runtime, storage format, and limitations with the exact
   release source.
4. Build artifacts from the recorded release commit.
5. Verify every file and image digest in [artifacts.md](artifacts.md).
6. Install or extract each distributable in a clean environment and repeat its
   smoke test.
7. Confirm that each binary reports version `0.1.0`.

## 9. Evidence retention

The release evidence bundle MUST contain command transcripts, exit codes,
timestamps, environment inventories, raw test output, SQL traces, source and
artifact checksums, FUSE mount proof, container inspection, benchmark raw data,
and the final acceptance summary.

| Evidence location | Value |
| --- | --- |
| Repository path or immutable URL | **TO BE FILLED AT RELEASE** |
| Bundle filename | **TO BE FILLED AT RELEASE** |
| Bundle SHA-256 | **TO BE FILLED AT RELEASE** |
| Retention owner | **TO BE FILLED AT RELEASE** |
