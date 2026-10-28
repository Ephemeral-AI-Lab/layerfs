# Agent rules for core

> **Status:** Current general guide.

Read [repository instructions](../AGENTS.md) first. Core contains the four
algorithm and contract crates: Content, History, Storage and Telemetry. The
[root workspace manifest](../Cargo.toml) decides membership.

## Scope and dependencies

Core depends only on core, including build and dev dependencies and test source
includes. It has no rusqlite dependency. The
[boundary guard](../tools/check_product_boundary.py) checks every crate against
one layer map and exact production-edge allowlist; unknown crates fail closed.
Engine composition belongs under [engine/](../engine/AGENTS.md), and public SDK
composition belongs under [api/](../api/AGENTS.md). Neither layer is a fallback
implementation for core. The retired v0.1.6 reference remains absent; its recovery
tag and historical evidence are described in the root guide.

Start with the [cluster-one handbook](../docs/general/cluster_one_handbook.md)
and [CAS/CDC/delta guide](../docs/general/cas_cdc_deltaencoding_handbook.md).
Preserve canonical compatibility, public ports, source pins and limitations.
For Workspace integration also read the
[current design index](docs/issues/303/README.md) and the relevant primary
contracts. Research and old Stage handoffs do not select new algorithms or
reopen a closed campaign.

## Construction and completion boundaries

Content owns canonical filesystem/file construction. Storage owns encoding,
packing and object read/write ports. History owns publication and mutable
history contracts. Telemetry owns observations and timing. Project adapts native
inputs; Workspace adapts an immutable base plus captured Overlay changes. They
reuse the same Content and Storage pipeline. See the engine guide's
[shared construction boundaries](../engine/AGENTS.md#shared-construction-and-completion-boundaries).
Do not add a second encoder, constructor or source include to supply integration.

Storage keeps its narrowly audited FFI module `src/encoding/codec.rs` under
`deny(unsafe_code)`; Content and Telemetry forbid unsafe code. Preserve the
existing unsafe boundaries and extend guard coverage when required shipped
formats are introduced. No third-party source or dependency changes are implied.

## Checks and completion

All product layers follow the
[production size and responsibility rules](../engine/AGENTS.md#production-file-size-and-responsibility)
and [verification rules](../engine/AGENTS.md#checks-and-completion). Build and
test through the selected worktree's registered private resources from the root
manifest. Build tests first with `--no-run`, then execute each selection under an
explicit wall timeout of at most 120 seconds. Preserve repository ARM64 flags,
locked dependencies, exact failure custody, the Disposable-only execution policy
and per-commit production LOC comparisons. Migrations retain cheap tests of
surviving behavior and delete tests that require retired scaffolding, with the
root guide's required behavior coverage and deletion record.

For docs-only changes verify local links/anchors, status, claims and whitespace;
Rust/runtime checks do not apply. CI reports after a push and never gates it.
```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --all-targets
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 -B core/tools/check_product_boundary.py
python3 -B -m unittest discover -s core/tools -p 'test_*.py'
```

Every test invocation obeys the repository's
[2-minute test ceiling](../AGENTS.md#4-code-build-and-docs): run it under an
explicit timeout of at most 120 s, per package when the whole suite cannot fit,
and treat reaching the ceiling as a hang to diagnose, not a run to wait for.

Migrations and refactors follow the root
[delete-rather-than-nurse rule](../AGENTS.md#4-code-build-and-docs): delete a
test that is tricky to adapt, keep or move only cheap tests of surviving
behaviour, and re-cover any still-valid fix or contract on the new code.
`benchmark/fs-bench-pro` is frozen and left broken (owner, 2026-10-18):
do not build, run, check or repair it.

For docs-only changes, check links/anchors, status/claim accuracy and whitespace;
report why Rust/runtime checks are inapplicable. For tooling/harness changes run
the owning scoped tests. At final implementation identity run required covering
checks once; diagnose a failure from its output/source before a justified repair
and rerun. Do not repeat unchanged passing checks or run an unrelated legacy sweep.

The guard checks source text, line caps and common prohibited constructs,
including markers embedded in source comments/strings. It is not a Rust semantic
or runtime proof. Review ordinary macros, helpers, dependency/target declarations
and generated inputs; extend scanners for new shipped formats. An excluded
FUSE/daemon package is not exercised by passing active-workspace tests.

There is no CI or aggregate pre-push gate; `tools/preflight.sh` is retired.
Do not disable discovery or recreate an equivalent wrapper. Report exact checks,
failures and gaps. A scan or proposal alone does not establish implementation.

### Build flags are part of the check

aarch64 binaries retain the repository ARMv8 AEAD build inputs:
`--cfg aes_armv8 --cfg polyval_armv8 --cfg chacha20_force_neon
-C target-feature=+aes,+sha2`.
The [root Cargo config](../.cargo/config.toml) is discovered from the working
directory, not by a manifest path into core. Builds outside the repository or
with explicit RUSTFLAGS must repeat that profile. Record flags/config in build
identities; preserve the transport's required-profile refusal when integrating
it. Documentation builds do not prove the binary profile.

Measurement work additionally follows the
[agent measurement workflow](../docs/general/agent-measurement-policy.md) and
[owning core harness guide](../benchmark/fs-bench-pro/AGENTS.md). Frozen identities,
cache states and budgets are evidence contracts, not product runtime limits.

### Fuser provenance and authorized-patch checks

Additional owner authorization 2026-10-14: "you are allowed to modify fuser with
patch". The [receive-buffer decision](docs/issues/321/FUSER-RECEIVE-BUFFER-DECISION-20261014.md)
records the scope. Receive loops read into the `max_write + 4096` bytes
negotiated at INIT instead of fuser's 16 MiB + 4 KiB maximum. The delta is in
`read_buf.rs`, `session.rs` and `session/lifecycle.rs`, applied after the
lifecycle patch. `receive-buffer.patch` and `receive-buffer-provenance.json`
pin it, and the focused check verifies it. The earlier records stay unchanged.

Additional owner authorization2026-10-08: the [scoped receive-loop lifecycle
extension](docs/issues/307/FUSER-LIFECYCLE-DECISION-20261008.md) may extend the
pinned fuser0.18.0 package beyond time.rs to establish loop entry/exit, retained
startup/join ownership and the proposed explicit lifecycle API. This narrowly
supersedes the timestamp-only file restriction below. Preserve the original
timestamp provenance unchanged; record and verify the lifecycle delta separately.
No other dependency change or general unsafe exception is authorized. Permission
does not establish implementation or native Ready/drain qualification.

Latest owner direction2026-10-06: "use fuser 0.18.0 from crates io and apply patch".
This supersedes the earlier registry-only/no-patch ruling for this one correction.
Keep the dependency version exactly0.18.0; use the checked-in copy of its official
crates.io archive with only `src/time.rs` changed by the recorded upstream timestamp
fix and regression tests. Both owning workspace roots use the exact path/version
`[patch.crates-io]` entry. Do not edit shared Cargo registry packages, adopt a Git
dependency, or extend this exception to unrelated source/dependency changes.

The original archive checksum, all85 published file hashes, exact diff and patched
time.rs hash are pinned in `patches/fuser-0.18.0/provenance.json` and checked by the
focused guard. A patched Cargo.lock entry has no registry checksum; the explicit
provenance record authenticates its modified bytes. See
[the authorized patch record](docs/issues/307/FUSER-REGISTRY-PATCH-20261006.md).
Native qualification and the complete S8 contract remain separate requirements.

Subsequent owner direction2026-10-06: "docker verification is enough". Existing
native Docker proofs are accepted for verifying this fuser correction. Retain
Linux's fractional signed-minimum endpoint outcome as a platform limitation;
do not require QEMU or a custom-kernel campaign to verify the dependency fix.
This acceptance does not complete S8's product implementation or turn that
individual native FAIL into PASS. Continue the remaining S7–S9 work using Docker.

Before a native fuser build, run the focused provenance check:

```sh
python3 -B core/tools/check_fuser_integrity.py
```

For an installed registry source, also supply its archive, package directory and
checksum taken from its original registry lock or the pinned base-archive record
with `--archive`, `--package-directory` and `--locked-checksum`. This reads and
verifies package files without modifying
them. Run scoped self-tests when changing the check. It is not an aggregate
pre-push/CI wrapper or a timestamp-capability proof. Current evidence and the
historically rejected unmodified Git candidate are in
[the fuser verification record](docs/issues/307/FUSER-OFFICIAL-CANDIDATE-20261006.md).

### Authorized Persistence seal file control

Owner authorization 2026-10-07: "Approve the narrowly audited seal wrapper".
The system macOS SQLite enables persistent WAL and otherwise leaves empty
sidecars after checked close. Persistence uses `deny(unsafe_code)` with one
allowed module, `src/backend/sqlite/file_control.rs`, compiled on macOS only.
It sets and reads back `SQLITE_FCNTL_PERSIST_WAL` under sole seal ownership.
No other Persistence module may contain unsafe Rust; the guard and its tests
enforce the exact path. This does not authorize third-party patches, dependency
changes, manual sidecar deletion or other FFI. See the
[decision and diagnostic](docs/issues/307/SEAL-PERSIST-WAL-DECISION-20261007.md).

### Audited Windows FFI modules

Owner direction 2026-10-25 (route 2, Windows-native Store; root AGENTS.md)
adds audited Windows FFI beside the macOS seal wrapper. The guard
(`tools/check_product_boundary.py`) and its tests enforce the exact paths:

- Persistence keeps `deny(unsafe_code)` with two allowed modules:
  `src/backend/sqlite/file_control.rs` (the macOS seal wrapper, unchanged)
  and `src/backend/sqlite/windows_lane.rs` (the named kernel mutex writer
  lane and `GetVolumeInformationW` volume filesystem).
- layerfs-project moves forbid→deny with one allowed module,
  `src/import/windows_identity.rs` (`GetFileInformationByHandle`: volume
  serial, file index, creation time).
- layerfs-overlay moves forbid→deny with one allowed module,
  `src/database/windows_identity.rs` (handle/path identity,
  `GetFileInformationByHandleEx` allocation, `GetDiskFreeSpaceExW`).
- Storage's `src/encoding/codec.rs` authorization is unchanged.

Acquisition rows are keyed by dev+inode. On Windows the identity MUST be
the real volume serial and file index; a zeroed placeholder collides every
entry onto one key and Init refuses with "the entry changed while Init
read it".

`#[cfg(unix)]` code is invisible to a Windows compile: a unix-only edit
that does not parse can pass on Windows and still break Linux. Code touched
for this route is cross-checked with `cargo check --locked` in the pinned
Linux container (`rust:1.85.1-bookworm`) before any "it builds" claim.

