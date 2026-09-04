# Deduplication and reliability execution freeze

This addendum freezes the remaining execution choices for Stage 1 issues
#30–#34. The canonical family documents retain their complete membership,
required observations, gates and operation surfaces. The common
[execution contract](execution-contract.md) owns numeric resource budgets,
phase deadlines, source custody, environment and terminal collection rules.
This document does not assert that a fixture is qualified, a fault seam exists,
or an outcome passes. Those are independently evidenced implementation gates.
No product optimization, storage-format change or weakening of a gate is
authorized. Commit this specification before collecting its measurements.

## Deterministic deduplication inputs

Use the three seed labels from [testing rules](testing-rules.md). All integers
below are unsigned little-endian u64. `frame(s)` is its UTF-8 byte length as
u64 followed by the UTF-8 bytes. Define

```text
H(family, profile, seed, ordinal, role) = SHA256(
  frame("dedup-input-v1") || frame(family) || frame(profile) || frame(seed)
  || ordinal_le_u64 || frame(role))
state = little_endian_u64(H[0..8])
```

Feed `state` into the existing `sdk_edit_common::fixture_block` stream;
continue the same state across buffer boundaries. The stream starts at byte
zero for every independently written file. Use a reusable buffer of at most
1 MiB. Never hash each generated word/block, clone a file, or use a product
root to generate input. Hash framing costs occur in setup except for the
explicitly measured compiled filesystem workload's byte regeneration.

Regular files have mode 0640, directories 0750, and mtime
1700000000.000000000. Set directory times after all their children. Paths are
relative UTF-8 strings and sorted by raw UTF-8 bytes in manifests. Include
root-directory metadata. A fixture receipt records every regular pathname's
distinct host device/inode, link count one, length, bytes written, allocation,
SHA-256 and mode/mtime; shared bytes do not authorize shared source inodes.
Only the declared prefix is prepared. Static manifests and chunk transcripts
may share qualified cache entries by complete generator/input/oracle identity.

For dedup fresh-import families, one `Client::initialize_layerstack` with
`LayerStackInitialization::Directory` starts with a fresh output Store. Its
acknowledgement is the import timer end. Every input byte is read and scanned.
Neither a prepared output Store nor a copied completed import is an input.
No post-import mutation, Commit or FUSE read occurs in performance mode.

### Cross-file imports

Family `dedup_cross_file` uses paths `files/f{i:04}.dat`, length 1,048,576.
All profiles use the common base-stream domain profile `base`, role `bytes`.
Base ordinal zero is the common anchor. Unique file i uses base i; identical
file i uses base zero. For mixed profile write `q = i / 4`, `r = i % 4` and
use base ordinal `3*q + max(r-1, 0)`. Thus the first block is 0,0,1,2 and
the next is 3,3,4,5. Prefixes 10/100/500 contain 7/75/375 distinct bases.
The N=1 sample is only `dedup-cross-file-anchor-1`; three profiles reference it.

Qualify every distinct base's internal chunk uniqueness and pairwise chunk-set
disjointness before admission; retain a qualification failure rather than
searching for a replacement seed. The exact-copy payload denominator is
1 MiB, independent of N. Retain 10 IDs and 30 performance slots.

### CDC imports

Family `dedup_cdc_locality` profile names are `overwrite`, `insert`, `delete`,
`common-body`, `scattered`. Reference path is `reference.dat`; variant paths
are `variants/v{i:04}.dat`. Generate reference B with ordinal zero and role
`reference`; a profile has its own reference, constant across all four tiers.
For each variant i, let

```text
offset = 65536 + u64(H(profile, i, "offset")[0..8]) % 917505
mask_byte(j) = 1 + stream(profile, i, "mask")[j] % 255
```

Here abbreviated H/stream arguments retain this family's full framed seed
domain. The offset lies in [65536,983040]; replacement/deletion remains
at least 61,440 bytes before EOF and the chosen offset itself is at least
64 KiB from either end. Overwrite XORs 64 bytes beginning at offset. Insert
inserts 4096 bytes from role `insertion`; delete removes 4096 bytes at offset.
Common-body concatenates 131072 bytes of role `prefix`, B[131072..917504],
and 131072 bytes of role `suffix`. Scattered changes byte
`4096*b + (u64(H(profile,i,"position")[0..8]) % 4096)` for each b in
0..255, XORing `mask_byte(b)`. Distinct variants use distinct framed i;
qualification rejects repeated complete variant digests and equality with B.
It does not silently change masks or offsets after a miss.

For the three localized profiles seal complete independent CDC transcripts,
including suffix resynchronization in both coordinate spaces. For common-body
report the intersection restricted to the declared middle. For scattered,
report actual shared IDs rather than a guessed zero-sharing result. Before
candidate optimization, freeze percentage/window targets from the retained
untouched baseline and qualified oracle; no imported historical 85/90/95%
threshold applies. Stage 1's exact correctness gate is equality with the sealed
transcript, not an invented minimum percentage.

The boundary recipe is one proof with three named seed cohorts, lengths
0,1,8191,8192,16384,32768,32769, paths
`boundary/s{seed_index}/n{length}/{a,b,changed}.dat`. Omit `changed` only
at length zero. Each exact pair regenerates the same role `boundary-bytes`
stream with ordinal equal to length; changed XORs byte length/2 with 1.
There are 20 files per seed and 60 files total, 42 exact-pair files and
18 changed files. Check all canonical/FUSE bytes and transcripts, empty-input
absence of chunks, nonzero chunks <=32768, length sums, >=2 chunks at
32769, and loss of the sole original chunk for changed inputs <=8192.
Existing fragmented-read CDC unit evidence remains separately attributable.

### Workspace additions

Family `dedup_workspace_reuse` uses `base/b{i:04}.dat` for i=0..127 and
`added/a{i:04}.dat` for additions. Base stream domain is profile `base`,
role `bytes`, ordinal i, each exactly 1 MiB. `added/` exists empty at genesis.
Exact file i regenerates base i%128. Local file i regenerates that base then
XORs 64 bytes at the CDC offset formula above using profile `local`, ordinal
i and role `mask`. Unique file i uses profile `unique`, role `bytes`.
Qualification checks the entire 128-file base chunk set, addition uniqueness,
and exact local intersections. Never read base bytes to create an addition.

Create one real-FUSE Workspace and use one public Exec running the existing
compiled workload helper. Each file is created exclusively, streamed with
ordinary writes, chmod/mtime normalized, `sync_all` called, then closed.
After all files, normalize `added/` and root metadata and fsync the opened
directories; close all handles before process exit. Record sync separately
inside workload wall. Observe the complete Exec receipt, call public Commit
exactly once (require Created), and End Clean. Report lifecycle phases
separately. The 128 MiB base is preparation, but every added byte is measured
work. The largest final logical tree is 628 MiB. A verifier reconnects to the
exact Commit in a separate run and checks the entire base and additions.

### Retained history

Family `dedup_branch_history` uses the exact `workspace-shards-v1` shard zero
and its existing framing/paths. For distributed/hotset rank eligible ordinals
by `SHA256(frame(seed)||frame("dedup-history-"+profile)||ordinal_le_u64)`,
breaking ties by ordinal. Select k modulo 200 or 8 from that ranking. The
64-byte replacement begins at file_length/2-32 and is generated from this
family's profile, ordinal k, role `replacement`. Qualify inequality against
the current range without altering the seed. Recurring B uses profile
`recurring`, ordinal zero, role `B`, exactly 49152 bytes; A is genesis file
192. Even k installs B, odd k restores A through one Inline SDK replacement.
Metadata uses file zero with even k=0600 and odd k=0640. Unrelated file j
at step k uses profile `unrelated`, ordinal `200*k+j`, role `bytes`.

The unrelated workload uses one Exec per step, no per-file subprocesses.
Write each existing file completely, normalize mode/mtime, sync and close;
normalize/fsync directories deepest-first and root last, then exit. Metadata
uses one Exec performing chmod, preserving all other metadata, syncing and
closing the file before exit. SDK profiles perform no FUSE payload writes.
Each step then calls `commit_workspace_session_with_status` exactly once,
requires Created, and records Commit/parent/root/head before the next step.
N counts successful Created acknowledgements, never attempts or UpToDate.

At N=500 there are 500 edits/Commits for SDK profiles, or 500 Execs/Commits
for filesystem profiles. Unrelated rewrites 100000 files and 500 MiB.
Each tier starts from a fresh independent pristine genesis and executes its
entire prefix. No prepared history replaces measured work. History has one
Branch and one Workspace; verifier-only forked Branches are allocated after
storage measurement and excluded from all history denominators.

## Independent content and storage oracle

The expected byte generator and schedule never consume actual result roots,
transcripts or timing. A verifier derives expected bytes and expected extents
before candidate work, then authenticates actual canonical objects and traverses
each regular file's rope. Reuse existing canonical decoding, `CoreReader`,
`filesystem::resolve` and `rope::visit_extents`; do not infer payload from an
untyped Store-wide object search. Canonical-format helpers may be shared;
the expected state transition must not call the product edit/Commit algorithm.

Fresh file writes/imports use an independently driven frozen FastCDC reference
scanner with an oracle implementation/source digest distinct from the product
execution path. Its profile constants and gear table are the released format
constants. Qualify it against existing frozen-vector and fragmentation checks;
compare complete offset/length/ObjectId transcripts, not only whole-file hashes.
Canonical payload encoding and ObjectId authentication obey the released format.

For distributed/hotset history use a flat extent-vector reference model:
clip the previous expected extent sequence at start and start+64, retaining
original ObjectId, full decoded chunk length and correctly advanced source
offset for slices; insert the independently chunked 64-byte replacement;
append the clipped suffix; coalesce only adjacent extents with the same ID
and contiguous source coordinates. Repeat this transformation per step.
Recurring full replacement replaces the complete vector with independent
CDC(B) or CDC(A). Metadata leaves the vector unchanged. Unrelated full writes
replace every file's vector. Do not demand fresh whole-file CDC segmentation
for partial SDK edits. Independently check bytes, coverage, no gaps/overlaps,
and exact normalized expected extents. Actual tree shape/root authentication
is checked separately from this representation-independent vector.

For every file and history step retain payload occurrences, logical offset,
referenced length, source offset, full chunk length and ObjectId. Distinguish
P (preexisting), U (reachable), I=U-P, the current set, and retained-history
union. Sum distinct full payload lengths for physical retained payload cost;
do not confuse partial referenced bytes with full stored chunk lengths.
Classify metadata/namespace through typed graph roles. Record ordered-vector
and unique-set digests and exact roots. All retained Commit IDs and parents
must be enumerated through public paginated Query and matched to receipts;
verify every historical state canonically and through one public fork/FUSE
Workspace at a time. End every such Workspace before creating the next.

Storage receipts include SQLite file length, allocated blocks, page size,
page count, freelist/live pages, canonical bytes by role, candidate/inserted/
reused counts and bytes, signed deltas, spool/temp census and final durable
inventory. Store census and transcript work belong only in setup/verifier
scopes. Performance retains existing intrinsic scan/admission/phase counters;
any required unavailable observation is a qualification gap, not zero.

## Reliability input and common checkpoint protocol

`workspace-reliability-v1` uses seed-1 and a fresh independent writable clone
for each proof. Create 1000 independently written files
`sentinels/f{i:04}.dat`, each 32768 bytes using the framed generator above,
family `workspace_reliability`, profile `fixture`, ordinal i, role `bytes`.
Create `links/alias.dat` as a hard link to sentinel zero. Create symlinks
`links/relative -> ../sentinels/f0001.dat`, `links/dangling -> absent`,
`links/cycle-a -> cycle-b`, `links/cycle-b -> cycle-a`. Let S be the sum
of these four exact UTF-8 target lengths. Create `sentinels/balance.dat`
with `33554432-1000*32768-32768-S` independently generated bytes, ordinal
1000. This makes initial conservative logical content exactly 32 MiB,
counting the alias path and symlink targets. Use standard modes/mtimes above;
symlink mode is 0777 with exact readlink target and normalized no-follow mtime,
without chmod following the link. Create empty `work/`,
`work/a/`, `work/b/`, `work/dir/`, `work/dir/child/` and `dest/`.

The fixture manifest includes all directories, regular bytes/lengths/modes/
mtimes, symlink targets and canonical hard-link equivalence classes. It derives
from the recipe independently of candidate output. Normalize metadata after
setup; qualify import preserves the declared graph. Unsupported fixture
semantics fail qualification instead of silently dropping links. Subcases
may add at most 16 MiB including transient files, aliases and open-unlinked
inodes; preflight <=48 MiB aggregate and <=1 MiB each file. The three-state
publication proof separately bounds four represented states at <=192 MiB.

Define `D(tag,n)` as n generator bytes using profile equal to the exact
subcase ID, ordinal zero, role tag. Ordinary probe writes use 4096 bytes,
except explicitly sized writes below. Every checkpoint compares the entire
live tree against the native-filesystem/model oracle, including unchanged
sentinels and portable metadata. Normalization is an explicit scheduled
operation, never a verifier repair. Capture intermediate state before later
normalization when the recipe requires observing a real change. After failure
compare prior acknowledged work, dirty tree, branch/head/base, visible Commit
count and old pinned snapshot independently. Every injection receipt records
armed point, actual hit count, phase/transaction ordinal, prescribed error,
rollback and explicit retry/discard. Hit count must equal one.

## Reliability subcase schedules and exact outcomes

The following 28 IDs are the members; headings/recipes add no members. Each
uses the fixture and common checkpoints, with all required public operations
and whole-tree reopen/cleanup from the canonical specification.

| Subcase suffix (prefix `workspace-`) | Frozen additional sequence and outcome |
| --- | --- |
| `invalid-sdk-edit-proof` | Write work/a/prior.dat=D(prior,4096) by completed Exec, then singular SDK edit sentinel 1 with start=32769, delete=1, Inline [1]. Require SdkError::Workspace(WorkspaceError::Storage(StoreError::InvalidInput("file range"))) with unchanged state; publish prior work once. |
| `invalid-namespace-proof` | With prior.dat dirty, rename work/dir into work/dir/child/nested, then replace directory work/b with prior.dat. Require EINVAL for each from current storage-port mapping; preserve complete pre-call state. Publish prior work. |
| `lease-lifecycle-proof` | Same Store owner, two Clients; second writable Create is WorkspaceBusy. End first; recreate at same placement. Drop owning Client; second Client can reuse lease/placement. Check sentinels after both release paths. |
| `open-writer-busy-proof` | Managed helper opens/writes work/a/writer.dat then finishes its managed execution only after arranging a qualified independently held real-FUSE writable descriptor. Attempt Commit with that descriptor pinned: Busy. Close descriptor, normalize scheduled metadata and publish exact bytes. Descriptor lifetime and execution lifetime are independently observed; a live Exec cannot stand in for this proof. |
| `live-execution-busy-proof` | One Exec writes/closes prefix.dat then signals an external barrier and remains active. Commit is Busy. Release barrier, join receipt, normalize metadata and publish once. |
| `candidate-failure-retry-proof` | Prepare dirty 4096-byte files under work/a and work/b, plus a relative symlink and moved directory. Arm existing candidate-construction seam, Commit fails once. Retry Created, third attempt UpToDate. |
| `admission-batch-failure-retry-proof` | Independently create 15 distinct 1 MiB files plus two distinct 64 KiB files under work/a and work/b, normalize/sync/close. Require candidate spill >0 at production 8 MiB threshold, one committed early admission transaction, and failure on first insertion of the second early admission batch. Preserve old snapshot and dirty tree; retry Created then UpToDate. |
| `final-publication-failure-retry-proof` | Same large dirty schedule. Fail immediately before ADVANCE_BRANCH after INSERT_COMMIT in final visibility transaction. Roll back that transaction; no visible new Commit/head. Retry Created then UpToDate. |
| `published-presentation-failure-proof` | Dirty work/a/published.dat. Fail real-FUSE resume after ordinary publication. Require Created plus presentation_failed and exactly one new Commit. Call public recover_workspace_presentation; full tree accessible; retry UpToDate. |
| `dirty-end-discard-proof` | Write/publish A at work/a/result.dat; overwrite with B. End Clean raises WorkspaceDirty and preserves B. End Discard releases resources. Reopen exact A. |
| `dirty-net-zero-proof` | In one Exec overwrite sentinel 2 first 64 bytes, append 64 bytes then truncate, create/move/remove work/a/temp.dat, observing each transition. Restore bytes plus every modified file/directory mode/mtime to original; require UpToDate, zero publication write, exact reopen. |
| `short-spool-write-proof` | After successful prior.dat, inject half-write then I/O error on a 4096-byte write to another file. Require EIO through ordinary API/barrier, exact prior contents and failed-call piece/high-water rollback; discard explicitly. |
| `deferred-nospace-proof` | After successful prior.dat, lower the test-only spool limit to current successful high-water. Queue a distinct 4096-byte proxy write; require deferred ENOSPC at next fsync/barrier, not fabricated full-output success. Compare prescribed rollback/prior data; discard. |
| `workload-cancel-proof` | One public Exec starts one owned child; write/close prefix.dat and signal external barrier. Public stop_workspace_execution terminates group and child, finalizes bounded output receipt, preserves completed prefix and performs no Commit. Discard/reopen old state. |
| `dirty-runtime-disconnect-proof` | Same barrier/prefix/child schedule, disconnect managed daemon route with Store owner alive. Require infrastructure-loss result, owned work termination, no Commit, Discard and same lease/placement reuse, exact old snapshot. |
| `corrupt-descendant-proof` | From qualified sentinel-3 transcript choose first payload ObjectId, orderly close and copy Store, flip last byte of its encoded canonical payload without changing stored ID/length. Public read must surface integrity/EIO, never altered bytes/zeros. |
| `missing-descendant-proof` | Independently copy same pristine Store and remove the same referenced payload row after orderly close. Public Store open or traversal must fail integrity at the recorded boundary; arbitrary process crash is not a proof. |
| `parallel-read-write-proof` | Four workers use work/w0..w3, each 16 cycles: read immutable sentinel(worker), write/sync/close cycle.dat=D(worker-cycle,4096), signal reader/mover worker (w+1)%4, which reads then moves it to final.dat. Barrier each cycle, join all, normalize metadata, one Commit. |
| `shared-path-contention-proof` | Two workers race O_CREAT|O_EXCL on work/claim.dat: exactly one success and one EEXIST. Then each publishes 16 complete 4096-byte generations via unique temp, sync/close/rename to shared.dat; reader checks one open per full read against 32 allowed byte strings. Join, install deterministic D(final,4096), normalize, Commit. |
| `hardlink-alias-proof` | Link sentinel0 to work/a/alias, write its first 64 bytes through alias, observe links/alias and sentinel0 same inode/bytes. Move work/a to dest/a, unlink links/alias, replace dest/a/alias via new temp inode; sentinel0 retains changed old inode bytes. Check exact link classes/counts and payload noncopying for alias creation. |
| `symlink-semantics-proof` | Read links/relative, move links/ to work/links/ (relative target becomes missing), move back (resolves). Check dangling ENOENT, cycle ELOOP, exact readlink target bytes; publish one declared 1-byte marker so reopen checks committed topology. |
| `open-rename-unlink-proof` | Hold read/write descriptor to work/a/held.dat, rename to moved.dat, unlink while open, read/write descriptor, observe pathname ENOENT. Separately hold old read descriptor while replacing named target by temp rename; old descriptor returns old bytes, fresh open new bytes. Close all, normalize, Commit without resurrecting removed inode. |
| `metadata-chmod-proof` | Set sentinel0/its alias to 0600 then 0640, sentinel1 to 0600, work/a to 0700; verify modes, alias coherence and unrelated metadata, normalize timestamps only, Commit/reopen. |
| `metadata-mtime-proof` | Set sentinel0 and work/a to 1700000013.123456789; observe alias, exact nanoseconds and unchanged other metadata; Commit/reopen. |
| `metadata-xattr-proof` | setxattr sentinel0 user.layerfs-v013=mixed-proof. Frozen Linux FUSE unsupported path: callback ENOSYS translated by kernel to userspace EOPNOTSUPP; require that exact outcome and no mutation. Record callback and syscall separately. A different platform/outcome is a qualification failure, not an arbitrary-error pass. |
| `exec-500-proof` | Exactly 500 sequential public Execs, each reads sentinel1, truncates/replaces work/result.dat with D(exec-k,4096), syncs/closes and emits only decimal k plus newline. Finish receipt/release reader before next Exec. Normalize, final Commit and whole-tree reopen. |
| `repeat-publication-proof` | Three successive completed Execs install D(stage-k,4096) at work/a/one and work/b/two, normalize/sync/close; Created after each on same Workspace. Verify all three pinned states, parents/head, continue writing after each; final UpToDate. |
| `sustained-600s-proof` | Two workers alternate via explicit barriers over work/active0 and active1. Each full cycle reads sentinel1, writes/syncs/closes 4096-byte temp, renames to active path, hands off to peer for read, creates/removes 64-byte scratch, and writes bounded 64-byte cycle identity result. Repeat actively >=600 monotonic seconds, finish current cycle, join, normalize, one Created Commit. |

The sustained proof records progress at least once per completed cycle and
fails its progress gate if no cycle completes in any continuous 30-second
interval. Require at least one completed cycle and >=600 seconds between
first active operation and final cycle completion; no idle sleeps qualify.
Retain a rolling external digest plus bounded last-cycle receipt, not unbounded
logs. The common execution contract supplies its finite extended hard wall.
The six extended members are admission-batch, final-publication, both descendant
integrity proofs, exec-500 and sustained-600s; all other 22 are short-lane.

## Qualified instrumentation boundaries

Existing source establishes seams, not their availability from the benchmark:
candidate failure and projection refresh/resume are debug thread-local,
crate-private hooks; short spool append is cfg(test); Store statement failure
is debug thread-local; resource policy is internal. Extend only verifier-only
instrumentation needed to arm these exact points on their owning thread and
return hit receipts. Do not change release behavior, thresholds, batching,
storage acknowledgement or fault-free algorithm. A hook set on the harness
thread while mutation occurs on another thread is not qualified.

Production candidate spill is 8 MiB; object admission is bounded by 127
objects / 4 MiB-1 bytes. Arm later-batch failure by observed transaction
ordinal, not guessed statement count. The source locations are
`objects.rs::insert_object_batch` and
`workspace.rs::commit_candidate`'s final INSERT_COMMIT/ADVANCE_BRANCH
sequence. Existing statement failure can implement the exact failure once
qualification binds ordinal to that execution; require actual earlier commit
and spill evidence. If 15 MiB+128 KiB does not reach the prescribed production
boundary, retain qualification failure and investigate; do not enlarge the
fixture past the 16 MiB dirty allowance or lower production thresholds.

The existing `ResourcePolicy` limit maps `workspace spool limit` to
PortError::NoSpace and FUSE ENOSPC. `rename type` and `rename descendant`
map to PortError::Invalid/EINVAL. The FUSE adapter maps generic I/O to EIO,
name existence to EEXIST and missing paths to ENOENT. Symlink loop detection
is Linux pathname traversal's ELOOP. Xattrs have no LayerFS callback override;
the locked fuser default is ENOSYS. Record and verify the actual Linux
translation before accepting its supported-error proof.

No missing seam, unreached fault, incomplete output/cleanup observation or
unimplemented case counts as executed coverage. Genuine product failures
remain failures under the terminal collection rules; they may be documented
without changing the pinned product or inventing passing verification.
