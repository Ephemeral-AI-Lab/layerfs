# Portable file open and atomic truncation

> **Status: one public operation implemented and natively verified; Pair 1 remains open.**
> Exact implementation parent: `a5bdc9f1e7e0fa4815ae356a7c5d783315fac649`.
> Product input seal: `af880d5b99b33fccadeb41ef35f82ab2e8be6880f61756648be497d52c347d31`.
> This closes the shared open/truncation prerequisite, not a writable mount.

## Operation and atomic boundary

`Workspace::open_file(serial, FileOpenOptions, ReferenceScope, deadline)` opens
an existing cached regular inode. FileAccess is ReadOnly, WriteOnly or ReadWrite;
FileOpenOptions contains access, append and truncate. Both types are Copy; default
options are read-only without append/truncate. Legacy `open(serial, scope)`
delegates to these defaults with the ordinary callback deadline. Directory-open
keeps its existing public signature and directory semantics.

ReadOnly, WriteOnly and ReadWrite require owner DAC masks 4, 2 and 6 respectively.
Append/truncate without writable access is InvalidInput; writable access on a
ReadOnly Workspace is ReadOnly. Inactive Projection scope, wrong kind, unretained
serial, deadline and capacity refuse before publication. Append intent is retained
on the handle, but this operation supplies no write API or append-position proof.
Synchronous, creation and platform/mapping flags belong to the later kernel
adapter; no unsupported flag is accepted as a durability promise here.

A truncating open reserves an entry in the **existing 128-slot handle vector**,
allocates a fresh monotonic ID and pins the node before metadata I/O. That entry
is pending, counts toward capacity, and is invisible to all public handle consumers.
Read, attributes, flush and release reject its ID until READY. The shared SetLen(0)
preparation validates the exact pending entry before publication. One state lock
then installs inode length, mtime, revision and dirty frontier, marks the handle
READY and finishes its reservation. Returning the preallocated ID requires no
subsequent allocation, lookup or other fallible step.

A full table therefore cannot truncate and then fail handle allocation. Failed
preparation cancels only the still-pending entry/node pin through a private RAII
owner, without I/O or ID recycling. Any failed backing candidate remains governed
by the existing ownership/quarantine rules. `forget` cannot collect the selected
node while the pending pin exists. If capture overtakes prepared metadata, the
existing exact root/revision/generation validation refuses publication.
Nontruncating opens publish in one short lock, with no metadata I/O.

A READY WriteOnly handle supports attributes, flush and release, but read returns
BadHandle even for zero-length or EOF requests. Flush keeps its selected volatile
meaning and does not Commit. Successful truncation affects existing aliases and
open handles immediately, including truncating an already empty file's timestamp/
revision. Explicit Commit persists that changed state through the existing route.
No public reservation API, second registry, test hook or write stub is added.

## Resource account

On the current 64-bit layout, Handle remains **24 bytes**: options/READY fit the
previous padding. The retained table is **128 × 24 = 3,072 bytes**. Attach verifies
its actual Vec capacity equals 128 and insertion refuses growth. A pending
truncate additionally holds a dynamically charged **56-byte OpenReservation**
until completion or cancellation. WorkspaceStatus.handles counts both pending
and READY slots. Node pins and all failed metadata ownership remain accounted.
Existing transient/disk reservations, windows, FDs and workers do not grow.
These are working-allocation bounds, not measured RSS/cgroup peaks.

## Native route and retained test failures

Eight cases use `open.rs`, the common external native fixture helper and
`open_route.py`. The closed 64 MiB Store is copied by independent bytes; a fresh
live C5 producer initializes fixture names/alias before Workspace attach. The
permission case's public initialization selects mode 0444 for data.bin and leaves
other.bin 0644. It **actually executes as UID/GID 1001**, with its owned common
root matching that identity. Other cases retain root execution. The caller has
only its immutable binary directory and private backing volume; Store/catalog
and service credentials remain at the host service.

All cases retain one construction worker, a fresh output path and a 60-second
complete-command hard limit. Calls use the existing ten-second callback/Commit
budgets. The deadline case explicitly selects 200 ms, holds its real canonical
refresh until that Instant has expired, then releases it and verifies no handle
or truncation published; this is a real deadline observation, not a fake clock or
performance assertion. The pending-handle cases hold only external delivery
before the actual native attribute request; no product branch is injected.

Two initial permission selections remain FAIL. In permissions-01 the test declared
owner 1001 but ran as root with root-owned paths, and existing attach correctly
returned Unsupported. The corrected execution uses actual UID/GID 1001 and its
matching root; the product ownership check was not relaxed. In permissions-02,
all permission assertions completed, but the test attempted clean close with
live lookup references and correctly received Busy. The final caller explicitly
forgets its owned lookup references before clean close, including the other cases
that needed the same lifecycle correction.

The original caller `cf299ae...` and both driver versions are retained with
SHA-verified reconstruction, raw FAIL receipts and checked removal of their owned
containers/volumes. Corrected caller `d2b683a...` owns permissions-03 and the seven
other selections. Product seal, deadlines, resource limits and complete byte/
handle assertions did not change; neither FAIL is promoted or omitted.

## Remaining prerequisites

The next data operation is a handle-based write with READY/writable rights,
atomic append EOF selection and positional Zero-gap handling through the shared
mutation body. Normal operation also needs an explicit synchronous healthy-owner
reclamation cadence before input/candidate admission, so callers do not have to
manually reclaim after each syscall to avoid fixed root/payload limits. That
cadence must skip retained failures rather than silently retrying them.

The writable Linux mapping/coherence profile and actual writable FUSE/daemon
binding remain unimplemented. An unmounted open flag is not a kernel capability
or append/write proof. Namespace/new-inode/metadata/symlink/larger-input work,
explicit failed-state/DiscardStage disposition, full declared npm, remaining R1-C
controls and R6 remain open. No performance, durability, restart continuity or
hard RSS/cgroup claim is made; no issue is closed.


## Actual operation results

All eight declared native cases now pass; two earlier failures remain in the
record. Complete walls below are functional budget observations, not latency or
throughput results.

| Receipt | Result | Complete wall seconds | 04 acceptance scope |
| --- | --- | --- | --- |
| open-permissions-01 | FAIL | 3.453754458 | W-01, W-02, W-12 subsets |
| open-permissions-02 | FAIL | 0.635823625 | W-01, W-02, W-12 subsets |
| open-permissions-03 | PASS | 0.820007750 | W-01, W-02, W-12 subsets |
| open-modes-01 | PASS | 0.771071833 | W-01, W-11, W-12 subsets |
| open-capacity-01 | PASS | 0.892447458 | W-02, W-12, B-01 subsets |
| open-last_slot-01 | PASS | 0.789978208 | W-02, B-01 subsets |
| open-metadata_failure-01 | PASS | 0.752521500 | W-02, B-20 subsets |
| open-forget-01 | PASS | 0.889622667 | W-02, S-15 subsets |
| open-deadline-01 | PASS | 1.028567208 | W-02, W-12 subsets |
| open-successor-01 | PASS | 0.869009583 | S-03, S-18, W-02 subsets |

Permissions-03 proves owner DAC with real UID/GID 1001: mode 0444 permits a read
handle and denies both writable modes, including truncate, without changing the
file; mode 0644 permits the corresponding read/write open. The modes case verifies
legacy read-only access, all supported rights, WriteOnly refusal at zero-length/
EOF reads, flush/release/fstat behavior, invalid option combinations, ReadOnly
Workspace, inactive projection, kind, identity and expired-deadline refusals.

The full-table case proves unchanged bytes, attributes, revision and allocated/
reserved backing at capacity. Releasing one slot permits truncation on open
without any subsequent write; old handles and aliases immediately see zero length,
and real Commit confirms timestamp/empty content. Repeating truncating open on
that empty file advances metadata without another EditFile. Two simultaneous
last-slot callers produce exactly one handle and one Capacity refusal, with no
over-admission or ID reuse.

The pending-refresh case observes the allocated slot, rejects its guessed monotonic
ID through read/attributes/flush/release, and calls `forget` while the pending node
pin exists. After successful publication and Commit, releasing the handle collects
the unreferenced node. The deadline case expires during the real held refresh and
returns no handle, unchanged length/revision and no dirty inode. The native file-
size limit similarly preserves the visible version and releases its pending slot;
it retains the stopped, accounted backing failure instead of claiming cleanup.
A later nontruncating read open remains usable, and the cancelled ID is not recycled.

The successor case truncates by open while G's file delivery is held, then verifies
that G saved its original changed bytes/length while live D1 remains empty and a
later real Commit saves that empty file. This proves a selected capture/continuation
schedule; holding delivery is not S-11 actual C2-save overlap. Prepared metadata
versus capture publication races still require separate native evidence.

## Checks, regressions and reproduction

These exact locked Rust 1.85.1 commands passed from the implementation worktree:

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

Host tests passed 649 with zero failures/three ignored; the boundary guard scanned
240 production Rust/SQL files and its six self-tests passed. Linux used the existing
`layerfs-pair1-rust-tools:c331f3815ef3cfb5c760` image, this worktree at `/work`,
registry read-only, `CARGO_TARGET_DIR=/work/core/target-linux`, and one construction
worker. Workspace `cargo test ... -p layerfs-workspace` passed ten ordinary tests
with 66 native cases ignored by default. Whole-core Linux Clippy with all targets
and warnings denied, and examples/binaries build passed with the same core manifest,
locked/offline flags. The corrected open caller was rebuilt and whole-core Linux
Clippy rerun after the test-only cleanup fix. Changed Python drivers compiled.
Root ARMv8 build-config SHA remains
`3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9`.
No CI or aggregate/retired preflight ran.

All 66 native Workspace cases were selected explicitly on this source: eight open,
nine resize, twelve RangeEdit, ten Stage, twelve CommitStaged, fourteen Composite
and the payload selection with thirteen payload checks. All pass. The actual Linux
read-only mount/authenticated Status regression also passes. The append-only index
therefore retains **69 selections: 67 PASS and two original FAIL**. No product
change was needed after those failures; both were corrected test setup/lifecycle
assumptions. All shared handle/read/directory/metadata and commit routes retain
current-source regression evidence, without a writable-mount claim.

Each complete command remained under 60 seconds. RangeEdit frontier was
28.699981458 seconds, payload 1.060676209 seconds and mounted Status
24.802500916 seconds. These are functional budget checks. Fresh outputs,
per-worktree isolation and process observations are retained; no same-worktree
build overlapped a selection and no quiet-host/performance claim is made. The
Stage-derived drivers mount only the caller archive; older payload/edit/mount
regression drivers retain their existing read-only repository bind. No historical
receipt is rewritten to claim a different source or exposure.

[The evidence index](evidence/portable-open/functional-index.json) links all
selections. Exact inputs, caller/harness versions, raw outputs, checked failed-run
cleanup and check logs are in [evidence/portable-open](evidence/portable-open/).
The runtime image remains `rust:1.85.1-bookworm`, immutable ID
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`.
Closed Store fixtures, private data and executable archives remain local artifacts,
not Git content. One real nonroot permission reproduction after recorded builds is:

```sh
python3 core/crates/layerfs-workspace/tests/open_route.py \
  --fixture core/target/pair1-evidence/large-edit-master-01/result.json \
  --binaries core/target/pair1-evidence/binary-archive/8c066ec93033c1130573091bebde897cc92082f9a38d8df4f554b8665604e2a2/host \
  --test-binary core/target/pair1-evidence/binary-archive/d2b683ab9aee1b217ba3da0545be949c28f178e2fe2657543d282ca1ff040cc3/open-test \
  --case permissions --output core/target/pair1-evidence/open-permissions-reproduction-NEW
```

Use a fresh output directory. The driver declares mode 0444 and actually sets up
and executes this case as 1001:1001. Other cases select their declared identities
and fault mechanisms. Production changes add filesystem/open and update the shared
read/write/directory entry points, runtime state/host and portable types. Reference,
service/history, FUSE callback implementation and dependency inputs are unchanged.


## Commit production LOC

First parent `a5bdc9f1e7e0fa4815ae356a7c5d783315fac649`:
**Production LOC: 107,029 -> 107,222 (delta +193)**. Reference remains
65,417 -> 65,417 (delta 0); core is 41,612 -> 41,805 (delta +193).
Workspace is 8,784 -> 8,977. This adds portable rights and atomic pending-open
publication while moving shared handle insertion from state to its open owner.
No reference retirement or measured simplification is claimed.

Counted staged tree: `1d070ec6c43768ae1c7147598fbdd54420fb9e65`.
Both exact snapshots use `git archive <revision> crates core/crates` and
`python3 tools/production_loc.py --root <archive> --json`, counter Git blob
`b5b9617d08204977176302311e0b2c72a811b420`. The same nonblank/non-comment product
Rust/runtime SQL scope excludes inline/external tests, examples, tools, docs,
manifests and generated artifacts. Only this LOC record/JSON is added afterward;
final product paths are checked identical to the counted tree before commit.
[Exact comparison](evidence/portable-open/production-loc.json).
