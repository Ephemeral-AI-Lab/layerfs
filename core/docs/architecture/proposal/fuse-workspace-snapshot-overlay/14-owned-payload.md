# R0-W / R3a: immutable private payload inputs

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Implemented from `d555c8bef2cd00f60f26720cb4b95a226917df17`, 2026-09-21.
> The owned-input operation is verified on Linux aarch64; visible file edits remain R3b work.

R3a closes the inputs needed for one operation: receive immutable bytes into
private backing and retain their exact ownership. It does not publish a file
edit. R3b's maintained disk metadata/piece index must exist before a write can
change the visible inode. This avoids building an interim resident namespace or
exposing private index/capture internals just to test the backing.

## Public operation and lifetime

```text
Workspace::own_payload(length, &mut existing Source, deadline) -> OwnedPayload
OwnedPayload::clone / len / is_empty
OwnedPayload::reader(checked half-open range) -> PayloadReader
PayloadReader implements the existing bridge Source
Workspace::backing_status() -> bounded aggregate consumer observation
Workspace::reclaim_payloads(deadline) -> checked cleanup result
Workspace::close_clean_until(deadline)
```

`OwnedPayload` and its reader are opaque production input types. A later
RangeEdit/lowering operation consumes the same immutable bytes. Success means
the declared input was fully accepted and exact EOF checked. No inode, timestamp,
base root, stage or Branch changes. A range reader pins the original complete
payload, including unused ranges; this round does not claim range-granular
reclamation. Cloning pins ownership without copying payload or enumerating files.

The existing no-argument `close_clean` delegates to a ten-second local deadline;
daemon shutdown calls `close_clean_until` with its existing shared shutdown
deadline. The read-only daemon startup retains `disk_budget_bytes=None`, while
still rejecting malformed explicitly supplied disk configuration. R3a is a local
Workspace/Source capability; it does not silently turn on a remote input or edit
control operation.

## Selected resource profile

| Resource | Selected bound |
| --- | --- |
| Logical payload length | Existing bridge MAX_FILE, 4 GiB |
| Logical segment data / immutable header | 1 MiB / 4 KiB |
| Direct-I/O alignment / maximum I/O | 4 KiB / 128 KiB |
| Acquisition | One per consumer, immediate excess refusal |
| Range readers | Two per consumer, each retains its window until dropped |
| Cleanup | One caller-driven cleanup operation with its own window |
| Windows | Four aligned 128 KiB allocations: 512 KiB retained; 640 KiB during sequential initialization |
| Payload records | Demand-grown, at most 4,096 and also constrained by the shared RAM budget |
| Backing descriptors | At most 16: common parent + at most 11 Workspace directories + acquisition + two readers + cleanup |
| RAM | Existing 8,388,608-byte aggregate accounted Workspace default; no RSS/cgroup guarantee |
| Disk quota | Explicit positive `WorkspaceConfig.disk_budget_bytes`; no numeric product default |
| Runtime metadata/index pages | Zero in R3a |
| Additional disk progress reserve | Zero for immutable input read/reclaim; R3b must reserve its own actual publication pages |

Fixed host/directory bookkeeping and each record/capacity are charged in addition
to the windows. The final profile reserves 16 KiB for the execution host and
16 KiB per backing directory, including bounded path/descriptor scratch. Incoming
root paths and retained mount/backing paths discard unused caller/growth capacity;
a short logical path cannot retain an arbitrarily oversized allocation. Replacement registry allocation is charged alongside retained
old capacity until the old allocation is released. An idle allocated window or
unused retained registry capacity remains charged. The cleanup window is separate
so two retained reader windows and an acquisition do not consume cleanup's scratch.
No extra worker, save lane, wait queue, cache growth or compactor is introduced.
The 4,096-input ceiling is a declared compatibility limit, not npm qualification.

### Routine private-ownership work counts (2026-09-26, #248/#256 phase 1)

Source pin: `f74dbe77da12fa533587be8a578375bce3f19373` plus the first
post-Phase-1 ownership slice committed with this note. Earlier sections of this
page keep their own pins.

`Workspace::backing_status()` reports two saturating lifetime counts of the
private ownership registry in addition to its byte and slot aggregates:

| Field | Meaning |
| --- | --- |
| `routine_scans` | Ownership records visited by consumer-wide *routine* reclamation selection (`OwnershipHost::maintain`, which every accepted mutation runs before it admits its own input). |
| `lookup_scans` | Ownership records visited by an id or custody lookup, an incarnation census, or the deliberate scoped `reclaim_payloads` pass. |

They are product telemetry like the projection counters: an operator can see how
much routine private work one accepted mutation pays, and a count never gates an
operation. They are not timers, RSS, Store reads or a performance claim, and a
lifetime total is never a per-phase number.

The counts exist because the quantity that must not grow is work, not wall time:
with `N` earlier retained acquisitions the routine selection walked the whole
registry before the next input was admitted, so `N` accepted writes paid
`N(N+1)/2` record examinations. The measured baseline for 1,024 retained
one-byte inputs through the public `Workspace::own_payload` route is 523,776
routine examinations and zero by-id examinations
(`OWNERSHIP_TRACE routine_per_write` rising 0.0, 4.0, 35.5, 159.5, 383.5, 767.5
at 1, 8, 64, 256, 512 and 1,024 accepted writes). This section records the
instrumented baseline only; the indexed registry and work-driven reclamation
that make `routine_scans` independent of `N` are their own later change.

## Disk equation and format

Let `A=H=4096`, `S=1048576`, and `L` be the admitted logical input length:

```text
N = ceil(L / S)                         // zero for empty input
D(L) = H*N + round_up(L, A)
segment i logical bytes = min(S, L-i*S)
segment i file bytes = H + round_up(logical bytes, A)
```

Reserve all `D(L)` before creating a segment or reading source bytes. Native
`fallocate` allocates a complete segment, and `st_blocks*512` observes its actual
regular-file allocation. Transfer reservation to that allocation, without double
counting. One byte costs 8 KiB; 64 MiB costs 64.25 MiB; 128 MiB costs 128.5 MiB
and therefore refuses a test quota of 128 MiB before consuming input.

The quota is owned private regular-file allocation plus outstanding reservations.
It includes ready, retained, partial, dead and failed-cleanup files. It does not
include global filesystem metadata/journals or another process's allocation, and
is not a kernel project quota or a guarantee of available device space. Real
ENOSPC can occur inside the local quota. Unexpected allocation larger than the
profile is retained at its observed amount and stops admission. An unobservable
allocation retains its worst known reservation, stops admission and reports
`accounting_complete=false`; it is never clamped or presented as a hard physical
quota pass. No runtime metadata/index file is silently excluded: none exists in
this operation, and R3b must extend the equation before adding those files.

Segment names derive from a non-reused monotonic payload ID and segment index
under `private-backing/<workspace-id>`. No vector of every segment is retained.
The 4 KiB header contains `LFSWPLD1`, version 1, header/alignment/data/window
sizes, Workspace incarnation, payload ID, whole length, index and segment logical
length. Bytes 80–4095 are zero. These fields validate an already retained owner;
they are not a restart/adoption format. Old or colliding files are never adopted.
Padding is explicitly initialized. Base references and logical zero spans require
no local payload allocation in this round; their future piece/index representation
is still R3b work.

## Linux capability and failure ownership

The initial native profile requires private owned directories, canonical paths,
the Linux `0xEF53` filesystem family with a 4 KiB block profile, native preallocation
and 4 KiB-aligned `O_DIRECT`. Runtime evidence must identify its actual filesystem;
the magic alone cannot distinguish ext2/ext3/ext4. Other hosts/filesystems fail
explicitly. Published nix 0.31.3 is already in the workspace and supplies these
native calls. No new package, unsafe product block, patched dependency or buffered
fallback is introduced. Short direct reads/writes are explicit failures; a short
backing read never becomes a zero-filled acknowledged range.

Directory ownership is retained in the existing attach registry before creating
its path. Acquisition similarly records a prospective segment before exclusive
creation, including an ambiguous create failure. A partial file has its known
device/inode identity and observed allocation; a matching header alone cannot
adopt a file whose creation was never established. Source failure, short input,
extra input, cancellation and deadline all return a typed retained failure with
phase, identity, completed bytes, created segments, allocation/reservation,
cleanup disposition and accounting completeness. Known uncreated reservations
can be released; created or uncertain resources remain owned.

Dropping a token or reader does no filesystem traversal. The registry retains
ownership until explicit cleanup finds no external token/reader references.
Cleanup checks computed segment names outside registry/Workspace state locks,
validates the retained header or known partial-file identity, closes its last
managed file descriptor, unlinks the exact owned file and then refunds its charge.
Known absence can establish release; an ambiguous existing collider cannot.
The reader closes its file before releasing its record pin. Failed cleanup retains
the remaining position and charge. A later explicit cleanup request is distinct
from an automatic retry. Close refuses retained inputs and unresolved cleanup.

## Verification and actual scope

The owning external test is `layerfs-workspace/tests/payload.rs`; its Linux case
is explicitly ignored in ordinary source-only runs and selected by
`tests/payload_route.py`. The driver uses a fresh named ext4 volume for ordinary
input and a separate small ext4 loop filesystem for real ENOSPC. It exercises the
public Workspace/Source API with a fixture read-only delivery capability, not a
private source include or alternate backing algorithm. Large input is generated
incrementally, with no whole-input fixture allocation.

The actual oracle covers cross-segment unaligned reads, exact EOF, source
failures, reader pinning/refusal, quota including headers/padding, read/header
failure, explicit failed-cleanup repair, simultaneous acquisition/two readers/
cleanup, a 64 MiB input under the 8 MiB account, actual payload-page residency and
native disk-full retention. `payload_residency.py` uses PROT_NONE mappings and
mincore without reading or touching the input pages. Its result concerns private
payload inodes; it cannot establish an RSS/cgroup/global-filesystem bound.

The owned-input API passes its actual functional proof. This does not complete
B-09/B-10/B-17/B-19/B-20 as writable-filesystem rows, nor B-25 as a full resource
qualification: the index, namespace mutation, capture/save and process/kernel
memory domains required by those rows are still separate. No mounted write,
snapshot, Commit, npm or R6 result is claimed. Remaining
R0-W decisions include R3b page/index encoding and publication reserve, root pins
and reclamation, R3c one-retained-G admission, R3d larger shared input/finality,
and R4 writable FUSE/mmap/coherence enforcement.

The implementation owns `backing/directory.rs` (directory/FD lifetime),
`backing/segments.rs` (format/direct I/O), `backing/payload.rs` (input admission
and ownership), `backing/reader.rs` (bounded Source), and `backing/reclaim.rs`
(checked release), with necessary private exports and runtime/type/lifecycle
changes. Daemon shutdown passes its existing absolute deadline; FUSE maps the
new typed local I/O causes. There is no writable FUSE callback in this round.

The final external test also induces a real short native write. After a segment
is preallocated and its header written, its external Source temporarily lowers
only the test process's file-size soft limit. The next pwrite transfers 65,536
bytes of a 131,072-byte request. Production reports WriteZero, publishes no input
token and retains the full 1,052,672-byte segment until checked cleanup. The
oracle verifies the actual written prefix and unwritten zero remainder, and
restores the initial limit. This uses the documented [Linux write behavior](https://man7.org/linux/man-pages/man2/write.2.html)
and [prlimit](https://man7.org/linux/man-pages/man1/prlimit.1.html); no product fault
hook or simulated I/O implementation is involved.

The 64 MiB input allocates exactly **67,371,008 bytes** in 64 segments. Its
16,448 private-inode pages have **zero resident pages** both immediately after
acquisition and after a complete Source read. Before the final path-capacity correction, the largest Workspace accounting
observation at the generator's Source callbacks was **1,936,757 bytes**, within
8,388,608; the final receipt below has its own updated observation. The sampled process FD count during that acquisition is 7 versus a
baseline of 6. These are scoped observations, not an all-phase allocator peak,
RSS/cgroup maximum, global filesystem-cache bound or throughput result.

Real ENOSPC occurs after 39 complete MiB of input and creation of a 40th segment.
Native allocation of that last segment is partial: the retained total is
**41,906,176 allocated bytes**, with zero remaining reservations after known
uncreated segments are released. The original StorageFull cause is retained.
Cleanup validates complete headers and the partial file's separately retained
identity, then releases the files and quota.

A colliding file with a valid copied header and valid private permissions is
refused and preserved. Its header supplies no authority. After the test owner
removes that unowned file, a new explicit cleanup establishes absence and restores
admission. Stale directories and tmpfs also refuse without adopting/falling back.
The earlier proof used a permissions-invalid collider; it remains historical
evidence, while the final oracle strengthens this ownership check.

Locked Rust 1.85.1 host tests cover the whole core workspace and explicit
non-Linux backing refusal. Linux's nine readable Workspace tests pass, and the
Linux payload case runs through the external driver instead of being silently
counted in a source-only run. Host examples/binaries and all-target Clippy pass;
Linux daemon build and Workspace/FUSE/daemon Clippy pass. The first Clippy
attempt retained a manual-clamp lint; the equivalent standard clamp corrected
it. Formatting, the boundary guard and its six self-tests pass. The initial
functional proof, added short-write/read-residency proof, and final strengthened
ownership oracle retain separate receipts in the evidence directory.

Reproduce after the locked Linux test build (the driver accepts the actual
`cargo test --no-run` executable, preferably copied into its SHA256 archive):

```sh
LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/crates/layerfs-workspace/tests/payload_route.py \
  --test-binary "$PWD/core/target/pair1-evidence/binary-archive/<SHA256>/payload-test" \
  --output "$PWD/core/target/pair1-evidence/NEW-PAYLOAD-OUTPUT"
```

The runtime image is `rust:1.85.1-bookworm`, identified by image ID in the receipt.
The driver uses privileged access only inside its owned disposable Docker runtime
to mount the separate ENOSPC filesystem. Failed runs retain their named container
and volume for inspection; successful runs verify unmount/removal. Each complete
functional command retains the unchanged 60-second hard budget.

## Final source and results

[Final owned-input receipt](evidence/r3a-payload-20260921/payload-04/result.json):
13 registered checks PASS, complete functional command **1.202449999997043 s**.
The final Source-callback accounting observation is **1,961,333 bytes**, including
the corrected path/descriptor allowance. The budget remains 8,388,608 bytes.
The [actual mounted Status refresh](evidence/r3a-payload-20260921/status-04/result.json)
also passes, in **28.421281916991575 s**, including handle lifetime, held-read
progress, authorization and checked shutdown. Its read-only idle account is
1,395,796 bytes. It does not qualify mounted payload mutation.

Both final routes have product-input seal
`1a0d6a9edc497f38532b43a2fcad4e52e785e09e34fb72c48a09e8e0cae508c7`.
The final Linux test executable is retained in the immutable local archive
`core/target/pair1-evidence/binary-archive/8658627977eabd7e5218f62d3a6c564afb109a7513622d1a883d61b22e8fd79b/payload-test`.
The receipt pins that binary, its test source, driver, oracle, image and kernel.
Earlier narrower proofs and their original numbers remain in the same evidence
directory; they are not promoted to the strengthened oracle or final accounting.

Whole-core tests passed before the final clamp/path-capacity changes; the final
source was then covered by focused Workspace tests, whole-core Clippy and
examples/binaries, Linux compilation/Clippy, and the actual final payload/Status
routes. Formatting, boundary and self-tests passed. No CI or aggregate preflight
ran. R3b next implements the maintained index and one explicit unmounted local
RangeEdit; R3c/R3d, writable FUSE, the remaining controls, npm and R6 remain open.

## Commit production LOC

First-parent comparison against `d555c8bef2cd00f60f26720cb4b95a226917df17`:
**Production LOC: 99,637 -> 101,202 (delta +1,565)**. Reference remains
65,417 -> 65,417 (delta 0); core is 34,220 -> 35,785 (delta +1,565).
This adds bounded private-input ownership and Linux backing; it is not reference
retirement or a performance comparison.

The counter is `tools/production_loc.py`, Git blob
`b5b9617d08204977176302311e0b2c72a811b420`. Both exact snapshots were archived
with `git archive <revision> crates core/crates`, then counted using
`python3 tools/production_loc.py --root <archive-directory> --json`. The scope
is first-party Rust implementation and runtime SQL; comments, blanks, legacy
inline tests, external tests, tooling, manifests, locks and docs are excluded.
The measured staged tree is `1735462f8708e619ced2468ef7d0c7699d847a4e`; adding
this record changes documentation only. The committed tree is checked against
the final staged tree, and its product-input seal matches the final receipts.
