# Native handle write, append and positional gaps

> **Status: one public operation implemented and natively verified; Pair 1 remains open.**
> Implementation parent: `1343b00accf2277085410c2fd84144b2432f6222`.
> Product input seal: `64e5b72cb535b230074b12db667671740d7727d3632748da9714fb0b1b8c9f1c`.
> R4's writable kernel binding and full Pair 1 remain open.

## Public operation and atomicity

`Workspace::write_file(handle, offset, &OwnedPayload, deadline)` writes through an
existing READY writable Local regular-file handle and returns MutationReceipt.
The borrowed payload must belong to the exact consumer and Workspace incarnation;
it remains usable by the caller after refusal. Accepted bytes count the caller's
input, excluding any synthesized gap. ReadOnly Workspace, read-only/pending/
directory/stale handles, unsupported projection scope, foreign payload, expired
deadline and invalid bounds fail explicitly. Existing limits remain 4 GiB logical
file, 8 MiB normalized Local-plus-Zero input, 256 normalized edits, 1,024 pieces
and 128 dirty inodes. No service operation, dependency or public input type changes.

Native append ignores the supplied offset and selects the current live inode's
EOF after metadata writer admission. Positional overwrite uses
`start=min(offset, EOF)`, `end=min(offset+input_length, EOF)` and preserves the
remaining tail. If offset is beyond EOF, one Zero gap precedes Local input in the
same splice. One fixed two-piece array and the existing splice builder produce
one candidate; there is no public resize/edit pair, intermediate visible extension
or second splice. Content, length, mtime, revision and maintained dirty frontier
publish together under the existing exact root/generation/revision checks.

Handle identity and rights are validated before maintenance, after canonical
attribute refresh (including a failed refresh), under the writer's stamp and at
final publication. A release that wins before publication prevents the write with
BadHandle. A rejected prepared healthy candidate remains under the already
verified reclamation cadence; partial/failed ownership remains charged. No fallible
cleanup follows success. Append admission can return Busy; any explicit subsequent
caller invocation is a distinct operation, never a hidden product retry.

Zero-length input validates access, ownership, handle, deadline and positional
bounds and returns the current generation/revision with accepted_bytes=0. It does
not refresh canonical metadata, run maintenance, acquire custody, allocate a
candidate, extend the file or change timestamp/revision. Native append still
ignores its offset. Existing RangeEdit and SetLen reuse the same two-piece slice
path, with unused zero-length entries skipped by the existing normalization.

## Resources and kernel boundary

The existing two 1,024 × 48-byte piece vectors remain 98,304 bytes. Conservatively
retain all prior 596,448 bytes of transient allowance, then add the complete
96-byte pair, 32-byte expanded FileMutation, 16-byte borrowed slice and 24-byte
copied handle: **596,616 bytes**, leaving **58,744 bytes** inside the existing
655,360-byte reservation. Old scalar/native scratch is not subtracted; it covers
the borrowed payload option and append flag. Actual retained allocations retain
their dynamic charge. No page format, window, FD, worker, backing quota, candidate
reserve or completion fund changes. This arithmetic is not RSS/cgroup evidence.

The FUSE adapter remains read-only. Kernel append has additional file-position
requirements: the kernel chooses its offset from its own inode size and WRITE
reply returns only a count. Native append's ignored offset does not qualify a
mounted append callback. The next projection round must close that origin-specific
offset check, bounded reply/publication coherence and supported kernel mapping
profile, then prove real mounted read/write/append/truncate behavior. No current
test is promoted to that route.

## Verification and remaining scope

The external write caller declares twelve selections through the existing native
service/Linux ext4 driver: positional/tail/gap with repeated Commit and hard links;
append and simultaneous admission; zero/rights/identity/bounds; exact combined
8 MiB envelope; all 256 edits and a refused 257th; quota and actual metadata I/O
failure; release/deadline during real canonical refresh; pending handle refusal;
frozen G/live successor coordinates; and write/append during an observed actual
C2 save. No test hook or alternate product path is introduced.

The real-save case uses the existing read-only SQLite RESERVED-byte observer,
confirms the service PID owns it before and after SIGSTOP, performs D1 writes,
then resumes that same save. Frozen bytes, live gap/tail and the next incremental
Commit are checked through real saved roots. Holding delivery in the separate
successor case is not labelled actual C2-save overlap.

Selections retain fresh output paths, one sample, immutable binaries, independent
closed Store byte copies, a new live C5 fixture producer, one construction worker
and the existing 60-second complete functional hard budget. Frontier declares the
existing 25-second Commit budget; ordinary calls retain ten seconds. The 200-ms
deadline case is an explicit refusal oracle. No performance or cache-state claim
follows from these functional walls.

The first Linux Clippy check failed on an indexed loop in the external frontier
oracle (`needless_range_loop`). The oracle now uses enumerate without changing its
byte selection or assertions. The original log remains FAIL; product source,
budgets and selected cases did not change. Corrected whole-core Linux Clippy
passes and the caller was rebuilt before its first native selection.

Namespace/new-inode
operations, required npm installation, authenticated daemon edit/Commit controls,
failed-state disposition and the matched mounted R6 comparison remain open.


The initial frontier selection exposed a shared native framing prerequisite:
Commit and attributes succeeded, but strict 514-byte readback failed after 257
one-byte data frames. Its original FAIL and diagnostic are retained in
[23 — Native stream fragmentation](23-native-stream-fragmentation.md). That
focused correction was separately committed as 1343b00accf2277085410c2fd84144b2432f6222
before this operation. The archived Workspace implementation and caller were
restored byte-for-byte; the original complete frontier, byte oracle, response
budget and deadlines remain selected on the corrected bridge.


## Actual native results

All twelve write selections pass on the final product seal. Complete walls are
functional budget observations, not latency or throughput evidence.

| Selection | Result | Complete wall seconds |
| --- | --- | --- |
| positional | PASS | 0.950569250 |
| append | PASS | 0.823329833 |
| zero_rights | PASS | 0.753887167 |
| envelope | PASS | 2.234024667 |
| frontier | PASS | 29.976135042 |
| quota | PASS | 0.744758792 |
| metadata_failure | PASS | 0.748782083 |
| released | PASS | 0.798667833 |
| deadline | PASS | 1.022838833 |
| pending | PASS | 0.839981417 |
| successor | PASS | 0.882191875 |
| native_save | PASS | 1.668390292 |

The positional case verifies preserved tails, extension and zero gaps, shared
hard-link identity, changed mtime and a later exact one-byte EditFile over the
acknowledged root. Append checks the live EOF after local resize and concurrent
writers; admitted results never overlap, and any explicitly refused Busy call is
resubmitted as a separate caller operation. No product retry is introduced.

Zero input leaves metadata, backing allocation, revision and service-call count
unchanged while invalid rights/handles/incarnation/deadlines still refuse. The
8 MiB case reads the entire zero gap in bounded windows and verifies the saved
tail; the 257th fragmented write refuses, while 64 overwrites of the maintained
256-edit frontier succeed before the full real Commit/readback. Quota and native
file-size failure preserve visible bytes and borrowed input. Release and deadline
races hold an actual canonical refresh, then prove no publication; a real pending
truncating-open handle rejects both empty and nonempty writes.

Frozen G remains readable while a live write installs a later Zero gap; the next
Commit uses the exact saved G root and insertion coordinates. The actual-save
case separately proves write/append progression while C2 owns the observed SQLite
write transaction, with old and new saved byte oracles. It is not a mounted write.

All 72 earlier native Workspace selections also pass on this source, including
maintenance and every prior mutation/Commit group. Actual Linux read-only mount
and authenticated Status pass as well: **85 current-source PASS**. The index also
retains the four initial candidate PASS selections and one original frontier FAIL
on their earlier source seal, for **90 append-only selections overall**. The
original failure is not promoted. The bridge correction is separately committed
and qualified in 23. All complete commands remain below 60 seconds; the full write
frontier is 29.976135042 seconds and mounted Status is 24.812017958 seconds. No
same-worktree build overlaps a selection; interference observations remain in
receipts and no performance/quiet-host claim follows.

## Checks and reproduction

On the final restored source, locked Rust 1.85.1 host whole-core tests pass 653,
zero failures/three ignored. Linux Workspace tests pass ten ordinary tests with
84 native cases ignored by default; all 84 were then selected through their real
native drivers. Whole-core host/Linux Clippy with all targets and warnings denied,
examples/binaries builds, fmt, the 240-file boundary guard and six self-tests pass.
The original Linux Clippy oracle warning remains in write-checks/linux-clippy-01.log;
its corrected checks and all restored-source checks have separate logs.

Commands use the implementation worktree root and core manifest:

```sh
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --offline
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --offline --all-targets -- -D warnings
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --offline --examples --bins
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Linux uses the existing Rust tool image and read-only registry with this worktree
at `/work` and target `/work/core/target-linux`; its test command selects
`-p layerfs-workspace`, while build/Clippy cover the whole core. All use locked/offline
and one construction worker. Root ARMv8 config SHA remains
3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9.
No CI/preflight, third-party change, fsync or durability work occurs.

Reproduce each case once with fresh output and the immutable identities in
[write-inputs-02.json](evidence/handle-write/write-inputs-02.json):

```sh
python3 core/crates/layerfs-workspace/tests/write_route.py \
  --fixture core/target/pair1-evidence/large-edit-master-01/result.json \
  --binaries <host_binaries-from-inputs> \
  --test-binary <write_binary-from-inputs> \
  --lock-observer <recorded-store-lock.dylib> \
  --case frontier --output <fresh-owned-output>
```

The driver keeps Store/catalog at the host service and exposes only the immutable
caller directory plus owned private volume to Linux. Legacy payload/RangeEdit/
mounted regressions keep their separately declared read-only worktree bind. The
[index](evidence/handle-write/functional-index.json) retains identities, every
status, raw output, actual-save observations, isolation and checked external
cleanup. No resource-performance or writable-mount acceptance follows.


## Exact production source comparison

**Production LOC: 107459 -> 107575 (delta +116)**. Reference remains
65,417 -> 65,417 (0); core is 42,042 -> 42,158 (+116), entirely Workspace
9,133 -> 9,249. Production changes are filesystem/write.rs and overlay/pieces.rs;
external caller/driver and packet/architecture updates are excluded from the
source-size total. There is no reference retirement or relocation.

Both snapshots use counter blob `b5b9617d08204977176302311e0b2c72a811b420` and
`git archive <revision> crates core/crates`, followed by
`python3 tools/production_loc.py --root <archive> --json`. Identical nonblank,
non-comment first-party Rust/runtime-SQL scope excludes inline/external tests,
examples, tools, docs, manifests and generated output. The counted staged tree is
`92b6f5299b0c1810682f82b8760baae84b7ca38d`; final evidence additions do not alter its
production source. See [the exact comparison](evidence/handle-write/production-loc.json).
