# Real Phase 6 integration probe

> Status: Research; informative and not a product contract.

This isolated Cargo workspace exercises the existing public `WorkspaceApi` with a
separate experimental Linux daemon. Commands enter the real kernel FUSE mount.
SQL owns bindings and final extents, C1 constructs real canonical objects, C2
encodes FULL records and builds packs, MinIO holds the acknowledged pack bodies,
and the host's actual C5 SQLite catalog conditionally advances the Branch.

The current owner-selected scope is
[S1 simplified authority](../../docs/architecture/proposal/phase6-sqlite-minio/implementation/S1-SPEC.md).
The global catalog has only immutable locator tables; C5 owns history and conditional
Branch publication. Trusted daemon construction replaces duplicate service
candidate/file/portable certification and namespace-index installation. Arbitrary
commands use a separate nonroot identity and clean environment; private daemon
backing is inaccessible. Actual provider boundary proof is required before claiming
this experimental profile passes.

The current profile retains512inodes/256handles,owner-bound64serial C5pages,one
Workspace/Branch/producer,serialized capture/install and FULL-only experimental
packing. Indexed changed-only construction and source retirement preserve immediate
immutable bases. See the current
[checklist](../../docs/architecture/proposal/phase6-sqlite-minio/implementation/CHECKLIST.md)
for exactsource/gates. Larger admission/import,remaining syscalls/concurrency,
delta/pooling/cutover,canonical and physical qualification remain open. No shipped
product file is changed or release admitted. Deep270successorExeclatency is deferred.

Build from the repository root (the root ARMv8 flags must be active):

```sh
cargo +1.85.1 test --manifest-path core/benchmark/phase6-live/Cargo.toml --locked
cargo +1.85.1 clippy --manifest-path core/benchmark/phase6-live/Cargo.toml --locked --all-targets -- -D warnings
cargo +1.85.1 build --manifest-path core/benchmark/phase6-live/Cargo.toml --locked --release
cargo +1.85.1 zigbuild --manifest-path core/benchmark/phase6-live/Cargo.toml --locked --release --target aarch64-unknown-linux-musl
```

The daemon image copies the resulting Linux executable over the existing sealed
Linux provider base. Image assembly and source/binary hash capture happen before
the correctness child. Run the collector once with a fresh output directory,
the copied owned MinIO executable, sealed image ID and the host release driver:

```sh
python3 core/benchmark/phase6-live/run.py --output <fresh-owned-path> \
  --minio <local-MinIO-executable> --image sha256:<image> \
  --driver core/benchmark/phase6-live/target/release/phase6-live-probe
```

The collector requires clean published source, creates one isolated macOS MinIO
instance and bucket, writes credentials to a mode-0600 private file, retains raw
logs/failure custody, and terminates only its own provider process. The SDK owns
its container and volume. A successful child explicitly unmounts, drains those
owners and verifies both versions from MinIO after daemon deletion. The full
child has a 15-second bound; proof wall is separately recorded with a 9.5-second
bound. Times are correctness diagnostics with unknown cache state, INELIGIBLE
for speed admission. Do not repeatedly invoke an unchanged source to select a
better number. Failed invocations remain append-only evidence.
