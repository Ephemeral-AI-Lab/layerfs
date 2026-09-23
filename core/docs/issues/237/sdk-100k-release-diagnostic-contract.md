# #237 one-shot Core SDK 100k release diagnostic

> Frozen before the first Core SDK 100k **release** call. The owner explicitly
> requests release binaries for this follow-up. This selection is separate
> from the #236 debug benchmark and the earlier unregistered debug diagnostic;
> neither receipt or selection changes.

## Identity and operation

The sole `core/benchmark/fs-bench-pro/runner.py` accepts an explicit
`--diagnostic-100k-release` mode with receipt schema
`core-fs-bench-pro-sdk-init-100k-release-diagnostic-v1` and
`benchmark_registration=UNREGISTERED_DIAGNOSTIC`. Build the existing SDK
`benchmark_init` driver and independent `verify_namespace` example from
`core/target/release/examples/` with `cargo +1.85.1 build --manifest-path
core/Cargo.toml --locked --release -p layerfs-sdk -p layerfs-service --example
benchmark_init --example verify_namespace`. No extra optimization override,
dependency patch, alternate operation, daemon, or FUSE route is allowed.

Use one public `Client::init_project` call timed by the driver's existing Rust
`Instant` boundary. `Host::create`, process launch, setup and full reopened
verification stay outside the operation timer. A separate external timer
records the complete driver process wall. The product retains four Init
producers, one C2 save owner, the 128-KiB whole-file cutoff, SQLite BLOB packs,
and 4,096-byte SQLite pages.

## Exact input and cache state

Reuse the sealed seed-1 `core-sdk-init-fixture-v2` / `host-direct-sdk-v2`
`namespace-100000` master, with manifest SHA-256
`23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`.
The case has 100,000 files, 1,000 data directories plus root, 500,000,000
total logical bytes including two 100,000,000-byte anchors, and class counts
`(2, 1000, 78998, 15000, 5000)`. Create a fresh independent writable byte
copy; validate all manifest paths, metadata, sizes, content SHA-256 and file
inode independence. Invalidate source payload pages after those reads and
require zero resident pages across the entire input on preflight and the last
nonfaulting check before launching the driver. Record page size/count and the
check-to-launch gap. Refuse the call if any of this fails. Directory/inode
metadata residency remains unqualified, so the row is never fully cold
namespace admission evidence.

## One attempt, bounds and evidence

Run exactly one release-profile attempt at a clean committed source identity
and a fresh result path. The **610 s outer driver watchdog** retains the
product's independent 600 s Service deadline or failure plus teardown; the
**600 s separate verifier watchdog** allows the full 101,001-path/500-MB
oracle to report its outcome. These are safety limits, not relaxed PASS gates.
Report whether the complete command meets #236's 15 s condition and the
general 25 s exploratory exception ceiling, and whether verification meets
#236's 5 s and the general under-10-s expectation. A miss stays a miss even if
the diagnostic finishes. Do not widen a bound or repeat an unchanged arm.

Retain the exact source/tree, product/harness/Cargo/config seals, `--release`
build log/profile, target and archived binary SHA-256 values, fixture/copy and
cold receipts, driver stdout/stderr/exit/result, operation and complete-command
nanoseconds, call count, cleanup/timeout, competing work, per-child lifecycle
user/system CPU and normalized peak RSS. A confirmed root receives one full
independent reopened verifier for all 101,001 paths, 1,001 directories,
100,000 files, portable metadata, 500,000,000 bytes, SHA-256 and root. Record
verifier wall/exit/output separately. After close, record Store and History
apparent `st_size`, allocated `st_blocks * 512`, `PRAGMA page_size` and
`page_count`, Store pack-row count and `sum(length(data))`, plus retained
scratch. Hash every retained raw result file in an append-only manifest.

Report the one raw SDK time and `500,000,000 / seconds` decimal MB/s as an
unregistered **release diagnostic** with `admission_eligible=false`. Do not
compare it to the 23.972-s debug row as a matched speed ratio, or claim a
v0.1.6 regression before a reference release run on these exact bytes.
