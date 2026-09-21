# Native mkdir and the maintained directory overlay

> **Status: implemented and verified in the declared native scope; target v0.1.7, not released.**
> Implementation parent: `85582e1ac2fb75761897115ec9679c59439b1efe`.
> Frozen product input seal: `827e336a7493faed1a9f9121374cd5f7097711dec5d2800bcfaf8ed2d18bde6d`.

The public operation is `Workspace::mkdir(parent, name, mode, umask, deadline)`.
It returns directory attributes and one Local lookup reference. This round covers
native Workspace namespace semantics and their capture/Commit integration. Kernel
FUSE mkdir remains a subsequent projection step. Native namespace mutation while
mounted must refuse explicitly until entry invalidation is supported; the existing
file inode invalidator cannot stand in for namespace coherence.

Creation validates the existing relative-name/path grammar, parent directory and
owner write/search permissions, portable directory mode and permission umask.
An existing name returns `Exists`. One public C5 reservation supplies the fresh
serial in the captured allocation scope. An unknown reservation is not replayed,
and an exposed/consumed serial is not recycled. Namespace publication must be one
exact-state transition: child directory, parent name, parent mtime, dirty records
and local lookup reference become visible together or remain unchanged.

## Private index and generation ownership

The implementation uses the existing COW metadata arena. A directory inode record
owns its current generation's entry-delta tree. Entry values name inode serials;
namespace nesting does not add page-ownership edges to child inode records. The
global dirty index remains shared with file mutations. An acknowledged directory
must survive eviction from the bounded Node cache and work through lookup and
directory enumeration before and after Commit.

An open directory handle pins its selected base, overlay and directory locator.
Later mkdir or Commit changes fresh lookup and newly opened handles, without
retargeting old cookies. Enumeration merges bounded pages in that pinned view and
obtains attributes from the same view. It must not call public live lookup or
acquire lookup references for enumerated names. Retained views and cookie mappings
remain charged and are released by the existing handle lifecycle.

Directory origins distinguish an empty new directory, a canonical base and an
exact captured directory version. First mutation of a directory inherited from
pending G starts an empty D1 delta and retains G through the existing root-owner
parent. Completion substitutes the known canonical base while reusing D1 entry
roots. It must not rebuild every D1 entry tree, demand file-only saved-result rows
for directories, or charge G's bindings to the next Commit. The shared Service
operation from [38](38-prepared-directories.md) constructs the new canonical pages
and patches portable directory metadata while preserving generic attributes.

## Implemented bounds and compiler layout

The 128 dirty-inode, directory-record and changed-name bounds, 32768-byte complete
prepared request, 640 KiB working reservation, 128 KiB separately charged I/O
windows, mutation candidate allowance and completion reservations remain fixed.
File and directory publications must use common generation accounting so either
kind can refuse before acknowledging a generation that cannot be submitted once.
For the current encoder, the fixed C5 request is 195 bytes with no head or 228 with
a head; add 73 per dirty file, 34 per dirty directory (one row and one metadata
record), 10 plus name length per binding, and a five-byte extension when directory
records exist. D1 must account for a head that pending G may create.

A packed inline Cell can hold a 256-byte name key and a 16-byte binding in 272
bytes, with separate key/value lengths. The linked Mach-O arm64 artifact confirms a 288-byte struct (previously 200).
Piece remains 48 bytes. Key, value and combined length
limits are distinct checks. The intended global index has at most three levels
and each generation-local entry tree at most four, derived from the existing
cardinality and minimum page occupancy. Those depth/role bounds are enforced
before descent; the generic eight-level bound is insufficient for the working
reservation after widening cells.

Source review gives conservative principal allocation totals of
371776 bytes for a file update, 279232 for mkdir's global update, and 346336 for
its entry-tree update. Entry and global updates must execute sequentially and
retain only page identities between them. These totals include vector capacities
and an edge vector; fixed scratch and retained owners keep their existing separate accounting.
They are source arithmetic, not a heap/RSS measurement. The final integration
review checked the allocation lifetimes and separate working/I/O charges. Global-to-entry reclaim
depth is at most seven frames, within twelve. Reconciliation keeps the existing
256-row, 26-slot short-key builder and reuses entry roots.

## Registered native selections

The selectors are `semantics`, `successor`, `capacity`, `refusals`,
`reserve_denied` and `reserve_unknown`. They cover nested creation and repeated
explicit Commit; forget/relookup/listing; G creation plus D1 nested creation before
CommitStaged and a subsequent D1 Commit; a fixed mixed file/255-byte-name capacity
case; early refusal; and real reservation denial or lost delivery without replay.
The capacity case must submit all accepted changes in one Commit and report the
exact limiting arithmetic. It cannot reduce the workload after a failed attempt.

Each selection reuses an independent byte copy of the closed canonical input,
creates fresh live C5 authority, and records exact source/binary/caller identities.
Whole commands retain the 60-second hard budget, Docker receives two CPUs, and
construction uses one worker. All six selections passed below. Full DSH upload, kernel
mkdir, wider namespace operations and matched R6/#207 remain unqualified.


## Functional qualification

| Selection | Result | Driver wall, s | Complete command, s | Native cleanup |
| --- | --- | ---: | ---: | --- |
| semantics | PASS | 3.703274917 | 3.803193959 | PASS |
| successor | PASS | 1.123227125 | 1.265267250 | PASS |
| capacity | PASS | 5.527418000 | 5.660325875 | PASS |
| refusals | PASS | 1.343372084 | 1.447400708 | PASS |
| reserve_denied | PASS | 1.066393000 | 1.168223250 | PASS |
| reserve_unknown | PASS | 1.066053875 | 1.192866334 | PASS |

All twelve registered markers passed, including clean close for every selection.
Each selected case ran once. Test and Service exited 0; no forced cleanup or
retained container/volume remained. Every container's actual NanoCpus was
2000000000. The test receipt and early `owners.jsonl` preserve exact owned resources
without keys. These wall times are functional command observations, not performance
or cold-cache results.

The fixed capacity case accepts **108** names of **255 bytes**, plus one existing
file change, for **110 dirty inodes** and **32632 encoded bytes**. Another name
adds **299 bytes** (265 binding, 10 directory row, 24 portable attributes), giving
**32931 > 32768** while still below the 128 dirty-inode limit. It is refused before
Reserve and before publication. All accepted changes are submitted in one explicit
Commit, with observed publication count 0 then 1. Metadata pages increased from
2 to 30 in the caller's before/after observations; that is not a peak or RSS claim.

The unknown-reservation case withholds 86 bytes of actual encrypted terminal data.
The call returns a Service failure with `unknown=true`. Independent reservation
markers show exactly one range was consumed, one attempt was made, and no name or
Branch change was published. The denial case consumes no range and returns a
definite Denied. A subsequent distinct mkdir uses a fresh serial in each case.
No retry of the lost or denied operation is used as recovery.

The permission case uses genuinely UID/GID-1000-owned runtime directories and
configured native ownership. Its process remains UID 0 for the existing mounted
refusal test; it is not proof of a non-root kernel caller. The stable-view case
holds an old directory handle across namespace changes and two Commits, replays
its exact old cookies, and observes the new names through fresh handles. Its exact
one-reference forget/relookup check also detects unintended references from listing.

Locked Rust 1.85.1 final whole-core tests passed host **680 / 0 failed / 3 ignored**
and Linux **678 / 0 failed / 132 ignored**. The six new Linux ignored tests were
then selected explicitly through the native caller above. Host/Linux all-target
Clippy with `-D warnings`, host bins/examples, final fmt, the **251-file** product
boundary guard and six guard self-tests passed. Linux's selected test executable
was explicitly rebuilt after caller-only Clippy corrections. No standalone Linux
bins/examples link was needed for these native test-binary selections.

Original checks remain recorded: first all-target checks reported an unused
`deliver`, removed before full tests; first fmt rejected declaration ordering in
three `mod.rs` files, followed by a new source freeze and final host checks; Linux
Clippy rejected two redundant Copy clones and a range expression in the new test,
followed by a caller-only correction and targeted rebuild. Product source did not
change for that last correction. No functional case failed or was rerun. No
aggregate preflight or CI was run.

## Resource observations and exact inputs

The linked host artifact confirms **Cell 288**, **Piece 48**, **Handle 72** and
**View 40** bytes. Handle was 24 bytes at the preceding committed artifact; the
additional optional inline view is included in the existing table charge. A
handle pins its Node's immutable path locator, valid for the current mkdir-only
namespace mutation scope; later rename work must preserve that locator lifetime.

Compiler sizes for prepared DTOs are InodeChange 80, DirectoryChange 32 and
DirectoryMetadata 24 bytes. Even summing independent maxima gives
`128*80 + 128*32 + 2*128*24 + 128*40 + 32768 = 58368` heap-capacity bytes; the
real shared file/directory and wire limits are tighter. PreparedChanges is 296
bytes inline, HistoryResult 352, StageWire 344, and the relevant failure containers
are bounded fixed records. Final materialization and delivery retain the existing
128 KiB operation scratch charge. Directory lowering's page traversal has its own
640 KiB writer reservation. Namespace operations retain one scratch charge while
releasing the remote admission flag between required RPCs; they add no queue or
second remote lane. Inode allocation during an occupied remote Commit correctly
refuses Busy; this case does not broaden the prior existing-file live-write proof.

These are compiler-layout and source-capacity facts, not a hard heap, process RSS
or cgroup qualification. The DWARF diagnostics record their exact binary hashes.
The initial baseline-symbol command found no linked DWARF in the Mach-O executable;
its subsequent symbol-linking diagnostic retained an ignored option-spelling
warning. Final symbol linking used the supported separate `--num-threads 2`
arguments. Neither diagnostic is a performance sample.

[Functional index](evidence/native-mkdir/functional-index.json.gz),
[check index](evidence/native-mkdir/checks-index.json.gz),
[exact source/binary/caller inputs](evidence/native-mkdir/inputs-01.json.gz),
[layout observations](evidence/native-mkdir/layout/after-01.json.gz),
[independent review](evidence/native-mkdir/source-review.json.gz), and
[archive manifest](evidence/native-mkdir/archive-manifest.json) retain all receipts,
original failed checks, pre-run caller snapshots and reproduction commands.
Uncompressed originals remain under `core/target/pair1-evidence/native-mkdir/`.
Reproduce one selected case with the recorded immutable host binaries and Linux
`mkdir` test executable, `DOCKER_CONTEXT=desktop-linux`, the closed
`large-edit-master-01/result.json`, the recorded runtime image ID, and a fresh
`--output` passed to `core/crates/layerfs-workspace/tests/mkdir_route.py`.

Production LOC is **110540 → 111632 (delta +1092)**: core **45123 → 46215**,
reference **65417 → 65417**. The unchanged counter blob
`b5b9617d08204977176302311e0b2c72a811b420` excludes tests, tooling, docs, examples and
legacy inline tests. [31](31-source-map-and-loc.md) contains every file/folder total;
the exact parent/final-staged comparison is required before committing. No legacy
retirement or counting-scope change is claimed.
