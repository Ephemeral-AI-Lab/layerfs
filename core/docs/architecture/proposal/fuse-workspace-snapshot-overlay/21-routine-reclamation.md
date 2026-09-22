# Routine healthy-owner reclamation before admission

> **Status: implemented and natively verified; Pair 1 remains open.**
> Exact implementation parent: `1f9cceb73ba0ede11c86120b73b2015f900d1dd8`.
> Product input seal: `e50c36e824564e3eb4375c638b43ac642fb4020fa15b3274bbea1abf06d4b0ab`.
> This is a bounded write prerequisite, with no new public operation or writable-mount claim.

## Selected cadence and ownership

Ordinary input acquisition, the shared existing-file mutation body (RangeEdit,
SetLen and truncating open), and Stage/composite Commit preparation perform one
synchronous consumer-wide healthy-owner pass before reserving the next candidate.
Cheap input/identity/deadline checks precede the pass. Metadata roots retire first;
payload records newly freed from their custody can then retire in the same pass.
The caller's existing absolute deadline applies. The pass adds no worker, queue,
retry, compaction, implicit Commit, disk spill, or public configuration.

Only a registry-owned metadata root with no other Arc pin is eligible. Its arena
must be complete and unblocked; it must have no pending create/slot/edge work,
temporary pages, uninstalled custody, cleanup progress or earlier cleanup failure.
Sealed unpublished healthy candidates are eligible after their final pin ends.
Metadata cleanup records failure even when its first ownership read fails before
a DFS cursor exists. Ordinary calls never retry that root. Existing explicit
reclaim and clean-close routes retain their deliberate checked cleanup semantics.

An eligible payload likewise has only its registry owner, no metadata custody,
READY and complete state, no partial acquisition, failure, admission block,
reservation or prior cleanup progress, and exact completed-byte/segment counts.
Live payload tokens, readers, live roots, frozen G and retained replies remain
authoritative owners. Failed/partial owners remain charged. Releasing another
Workspace's healthy garbage cannot clear any arena's quarantine or incomplete
accounting flags.

Selection takes a temporary Arc under the registry lock and releases selection
locks before I/O. With no eligible owner, the pass avoids taking a writer or
cleanup window. Metadata cleanup uses the existing writer and cleanup window;
after these drop, payload cleanup uses that same window. No Workspace state or
registry lock spans backing I/O. No cleanup occurs under the short capture cut.

The former broad metadata sweep after known Commit installation is removed.
The next submission's pre-capture pass releases healthy retired roots, including
between repeated clean Commits. Ordinary Commit therefore cannot resume an older
failed cleanup after publishing its own result. `CommitPhase::Cleanup` is removed
from this proposal API; earlier source-pinned receipts keep their original phases.
Reconciliation, exact remote-outcome preservation and submission ownership retain
their established behavior.

The marker fits existing padding in the selected 64-bit RootState layout: field
arithmetic changes from 284 to 285 bytes, both rounded to 288. Actual
`size_of::<RootOwner>()` remains dynamically charged on each target. No additional
heap collection, descriptor table, window, FD or worker is allocated. Existing
32-root, 4,096-payload and 65,536-slot ceilings remain; this is allocation arithmetic,
not a measured RSS or cgroup result.

## Verification scope

The new external `maintenance.rs` selections use the existing native-service
driver and immutable Linux caller. They declare 4,097 one-byte payload acquisitions,
96 existing-file mutations followed by 40 clean Commits, cleanup across Workspaces
with a pinned reader, and 64 D1 mutations while G is retained. Fault selections
damage actual payload/ownership headers, verify refusal before consuming new
Source input, restore only the externally damaged bytes, and require failed
owners to remain until explicit cleanup. A partial Source remains accounted while
healthy work continues. This is no restart/recovery claim.

The earlier payload-window proof keeps its empty payload alive until the active
Source callback, then drops it immediately before explicit cleanup. Its exact
one-owner release assertion and both occupied reader windows remain intact; the
new pre-admission cadence must not turn that proof into a zero-work cleanup.

Fixtures remain independent byte copies of the closed Store with a fresh live C5
catalog and public fixture initialization. Every selection uses a fresh output,
one sample, the existing ten-second operation deadlines and 60-second complete
functional budget. Counters/walls are functional observations, not a hard RSS,
cgroup, throughput or R6 qualification.

## Next operation

Handle-based write still needs READY/writable validation, atomic append EOF
selection and positional Zero-gap splicing in the shared mutation path. Kernel
writes additionally require their bounded reply/coherence binding and actual
Linux mounted proofs. Namespace/new-inode operations, declared npm installation,
authenticated edit/Commit controls and the matched R6 comparison remain open.

## Results and source-size account

All six declared maintenance selections pass on the product seal above:

| Selection | Result | Complete functional wall seconds |
| --- | --- | --- |
| payload_churn | PASS | 4.165576041 |
| mutation_churn | PASS | 2.420022250 |
| cross_workspace | PASS | 0.823580000 |
| frozen | PASS | 1.125118875 |
| payload_failure | PASS | 0.764947916 |
| metadata_failure | PASS | 0.791997041 |

Every payload acquisition in the 4,097-operation loop observes exactly one record
and 8,192 allocated bytes. Every one of the 96 mutations observes at most three
roots and two payload records, with exact visible bytes. Forty subsequent clean
Commits each remain below eight roots. The frozen case retains the old reply and
G while 64 D1 mutations stay below eight roots, then verifies both successive
real Commits. These assertions are selection-specific resource evidence; no
latency, global memory, syscall-write or mounted qualification follows.

Exact locked Rust 1.85.1 host checks passed:

```sh
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --offline
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --offline --examples --bins
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --offline --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Host tests pass 649 with zero failures and three ignored. The boundary guard
scans 240 production Rust/SQL files and its six self-tests pass. Linux Workspace
tests pass ten ordinary tests; 72 native cases are ignored by default and selected
separately through their actual drivers. Whole-core Linux Clippy with all targets
and denied warnings, plus examples/binaries build, also pass. Linux builds reuse
the existing `layerfs-pair1-rust-tools:c331f3815ef3cfb5c760` image with this worktree
at `/work`, a read-only registry and `/work/core/target-linux`. Root ARMv8 config
SHA remains `3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9`.
No preflight, CI, dependency patch, durability operation or performance run occurs.

Reproduction uses the immutable identities in
[maintenance-inputs-01.json](evidence/routine-reclamation/maintenance-inputs-01.json):

```sh
python3 core/crates/layerfs-workspace/tests/maintenance_route.py \
  --fixture core/target/pair1-evidence/large-edit-master-01/result.json \
  --binaries <host_binaries-from-inputs> \
  --test-binary <maintenance_binary-from-inputs> \
  --case payload_churn --output <fresh-owned-output>
```

Use each declared case once, with a fresh output. The driver records per-worktree
isolation, observed concurrent work, source/product/caller/harness/image identities,
actual Linux/ext4 route and checked cleanup. Its runtime exposes only the immutable
caller directory and private volume. Legacy payload/RangeEdit/readable-mount
regression drivers retain their separately documented read-only worktree bind;
their old exposure is not relabelled as the narrower new driver.


All 66 earlier native Workspace selections also pass on this source: 12 RangeEdit,
10 Stage, 12 CommitStaged, 14 composite Commit, nine resize, eight open and the
payload selection. Actual Linux read-only mount/authenticated Status also passes.
The append-only [functional index](evidence/routine-reclamation/functional-index.json)
contains **73 PASS, zero FAIL** for this round; historical failures remain in their
original round receipts. The longest complete functional command is RangeEdit
frontier at 29.052515625 seconds; mounted Status is 24.778817250 seconds and payload
is 1.065640958 seconds. All remain below the declared 60-second hard budget. No
same-worktree build overlaps a selection. No performance/cache or quiet-host claim
is made.

Changed production files are backing/metadata.rs, metadata_reclaim.rs, payload.rs,
reclaim.rs, commit/completion.rs, commit_types.rs, filesystem/write.rs and
 overlay/snapshot.rs under layerfs-workspace. External tests add maintenance.rs
and maintenance_route.py and update payload.rs/stage_route.py. This packet and
architecture/14-service-runtime.md describe the same change.

Exact first-parent/staged-tree production source comparison:
**Production LOC: 107222 -> 107378 (delta +156)**. Reference remains
65,417 -> 65,417 (0); core is 41,805 -> 41,961 (+156), entirely Workspace
8,977 -> 9,133. This adds required bounded retirement; it is not reference removal,
relocation or a performance simplification claim.

Both snapshots use counter blob `b5b9617d08204977176302311e0b2c72a811b420` and
`git archive <revision> crates core/crates`, followed by
`python3 tools/production_loc.py --root <archive> --json`. Identical nonblank,
non-comment first-party Rust/runtime-SQL scope excludes inline/external tests,
examples, tools, docs, manifests and generated output. The counted staged tree is
`ccf6ba1562e502ef0efb47a16ed6ce4ffd1e6ea9`; final evidence additions do not alter its
production source. See [the exact comparison](evidence/routine-reclamation/production-loc.json).
