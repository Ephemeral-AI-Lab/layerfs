# Ordinary-workload execution contract

This premeasurement supplement freezes implementation choices for #23–#29.
The seven canonical family specifications retain their membership, semantics,
size bounds and gates: 68 new cases and 204 initial performance slots. This
document introduces no additional cases. [Testing rules](testing-rules.md)
govern shared fixtures and seeds; [execution-contract.md](execution-contract.md)
owns the pinned product, resource profile, numerical deadlines and custody.
The committed revision containing these documents is the contract identity.

## Common deterministic operations

All paths are relative to the Workspace root. Bytewise order means unsigned
UTF-8 byte order. `frame`, shard bytes, shared paths and metadata are exactly
those in testing-rules.md. For additional files define
`bytes(D,path,L)` as the first L bytes of `fixture_block` with initial state
`LE64(SHA256(frame(seed)||frame(D)||frame(path))[0:8])`. Generate consecutive
1 MiB blocks, with only the final block shortened; do not restart the state
between blocks. This fixes the generator's partial-word behavior.
`rank(D)` is the shared ranked permutation of indices 0..499. Schedule ordinal
is its position, distinct from the ranked index used in a pathname.

Creation uses exclusive `create_new`, write-only descriptors and `write_all`;
in-place replacement uses write-only existing descriptors without truncation.
Payload buffers are at most 1 MiB. Offset writes/reads repeat short successful
transfers until the requested length is complete; unexpected EOF or zero-byte
write is an error. Interrupted syscalls may retry without restarting operations;
all actual requests/bytes and retries are counted. Other errors terminate the
workload with operation ordinal, path, error and completed-prefix receipts.
No ordinary-case error is an accepted supported-error outcome. Preserve failed
state for separately attributed diagnosis, then recover with Discard if clean
End cannot succeed; do not label recovery as passing product cleanup.

POSIX-created regular files use 0640; directories use 0750. At the end of a
mutating POSIX workload, set changed surviving files' mtimes and changed
directories' mtimes to 1700000000.0. Set directory times deepest-first.
The changed directory set is newly created directories plus the direct parents
of every create/unlink/link/rename; a recursive operation includes every parent
it actually changes. Do not normalize unaffected ancestors: changing a child's
contents does not change its parent's directory mtime. Removed paths are
excluded; moved paths use their final names. Normalize each inode once when
hard links alias it. Set symlink mtime without following its target. Count this
work and include it in workload wall, with separate normalization attribution.
Prepared fixture metadata is set after children, outside performance.

Each mutating POSIX workload then opens the root directory, calls `sync_all`
once and closes it. This is the existing FUSE fsyncdir/all-dirty barrier.
Record its wall separately within the workload wall; propagate all errors.
Required payload/editor temporary-file syncs remain additional measured calls.
Read-only, clean-Commit and owner SDK curves add no normalization or sync.
All workload handles close and managed execution exits before Commit.

All ordinary curves use one public Create, one managed Exec, one Commit and
one End. Exceptions are clean Commit (zero Exec) and distributed SDK edits
(zero Exec, N singular public edits). No shell mutation, extra readiness Exec,
per-operation process, visibility polling or verification call is allowed.
Normal runtime setup/attach calls retain their existing separately reported
scope. Freeze their observed topology under the common execution contract.
Inner workload wall encloses named subphases; it is not added to those same
subphases again in lifecycle balance. Full-tree hashes and transcripts belong
only to verification, including Git output hashes and random-read digests.

## Payload and tiny files

Payload pathname is `payload.bin`, with released flat-file bytes. Create opens
once, writes all selected bytes, normalizes file metadata, calls file sync_all,
and closes. The common root sync follows; normalization does not repeat.
Random read opens once read-only, performs the canonical offset schedule with
4 KiB `read_at` requests, and closes. Cheap completion counts are measured;
the separate verifier checks each returned byte against the flat-file oracle.

Tiny small-operation backgrounds use the shared 500-shard tree at root.
Prepare `tiny/` and `tiny/p0` through `tiny/p9` for all three curves.
For each ordinal k and index i=rank("tiny-file-churn")[k], the target is
`tiny/p{k mod 10}/f{i:03}.dat`, with the canonical ten-length cycle indexed by
k, and bytes("tiny-file-churn",path,L). Stat/unlink prepare all 500 targets;
create prepares none. Execute only the first N, in rank order. A create opens,
writes and closes once; stat uses symlink_metadata only; unlink uses remove_file.

Bulk mutation targets are one shared N-shard profile below `bulk/`; the fixed
untouched witness is a one-shard profile below `witness/`. Their bytes use the
same shared profile and seed with relative paths inside each profile, so the
witness's identical first shard is deliberate, declared reuse. Bulk create
starts without `bulk/`; it creates the full profile, including `dest/`, through
ordinary mkdir/write/close. Bulk delete starts with the full profile and removes
all files then all directories including `bulk/`. Traverse directories in
bytewise order; deletion is postorder. Shared profile directory prefixes are
created once in root-to-leaf bytewise order. No per-file sync is added.

## Directory construction and scans

Construction prepares empty `new-directories/` outside timing alongside the
fixed 500-shard tree. Ordinal k selects i=rank("directory-construction")[k]
and depth from the canonical cycle. Create
`new-directories/c{i:03}/d001/.../d{depth-1:03}`; depth one creates only c{i:03}.
Each chain is independent, so chain mkdir counts are 1/55/550/2750. Metadata
normalization and final root sync are separately counted.

Scans use the shared N-shard tree at root. Visit root once, then recursively
list each directory through ordinary directory handles to EOF, sort its child
names bytewise, and visit each child once. Metadata scan lstat's root and every
entry and opens no regular file. Content scan performs the same enumeration
and metadata classification, then opens each regular file once, streams with
one 1 MiB buffer until EOF, and closes it. Directory entry/page counters refer
to actual FUSE observations, not fabricated one-page assumptions. Verification
records exact traversal order, per-file EOF/length/digest and page membership.

## Git workflow

Shared background lives below `background/` with 32 shards. Prepare `tracked/`
and `added/`. Rank domain is `git-tool-workflow`. The canonical ten-slot cycle
chooses kind by ordinal k; target index i gives `tracked/modify-{i:03}.dat`,
`tracked/delete-{i:03}.dat` or `added/add-{i:03}.dat`. Initial tracked targets
include all modify/delete slots; additions start absent. Initial/add bytes are
bytes("git-tool-workflow",path,2500). Modify replaces bytes 1024..1034 by their
old value XOR 0x5a. Editor-save ordinal m and temporary naming follow the family
specification. Exactly six Git processes run the canonical commands after apply.

Pin the actual runtime Git executable digest and `git --version` in custody
before samples; the same binary builds the independent native reference.
Use branch `main`, object format sha1, author and committer `LayerFS Benchmark`
and `benchmark@layerfs.invalid`; both dates are `1700000000 +0000` for genesis
and `1700000001 +0000` for the new commit. Genesis message is
`layerfs v0.1.3 tool genesis`. Retain canonical workflow commit message.
Set GIT_CONFIG_NOSYSTEM=1, GIT_CONFIG_GLOBAL=/dev/null, GIT_TERMINAL_PROMPT=0,
GIT_OPTIONAL_LOCKS=0, LC_ALL=C, TZ=UTC. Use an empty HOME outside the Workspace.
Configuration: core.autocrlf=false, core.filemode=true, core.symlinks=true,
core.logAllRefUpdates=false, core.hooksPath=/dev/null, commit.gpgSign=false,
tag.gpgSign=false, gc.auto=0, maintenance.auto=false, credential.helper=,
status.showUntrackedFiles=all, diff.renames=false, index.version=2,
core.untrackedCache=false, core.fsmonitor=false, protocol.allow=never.
No remotes or network calls.
Template directory is empty; do not inherit host Git templates or configuration.

Use umask 0027 for Git and retain its native file modes in the independent
reference; Git-owned files follow those modes rather than overriding them with
the ordinary payload mode. Set deterministic mtimes on all prepared repository
files/directories. Apply
normalization to changed source paths before Git status. After Git completes,
normalize only source parents and repository files/directories actually created
or modified by this workflow; derive that set from the reference operation
manifest, not a timed whole-tree census. This is a separately counted workload
subphase before root sync. The Git index's stat cache may describe earlier
source timestamps; do not edit index bytes or refresh it after custody capture.
All repository files must remain within the canonical 256 MiB cap. Qualify
conservative allocation against an independently generated native repository:
bound each loose object by zlib compressBound(uncompressed object length),
bound each index entry by its actual pathname plus fixed SHA1/v2 header/padding
fields, reserve both old/new index and lock versions and 1 MiB for refs/config.
Reject qualification if this bound exceeds the cap; compression is not required.

Native reference semantics fix first porcelain, tracked diff, staged tree,
commit parent/tree/message/identity/time and required objects. Before LayerFS
Commit in verification, seal all repository paths/bytes/metadata; after remount
compare that custody before executing Git. Then require fsck --strict and clean
status. Native index stat/inode fields are not independent equality oracles.

## Subtree mutation and Workspace locality

Namespace background paths are `background/d{i div 1000:03}/f{i:06}.dat`,
i=0..99999, length 2500. Each affected tree uses
`source/tree-{a,b}/s{s:03}/f{j:03}.dat`, s=0..N-1,j=0..199, length 1024.
Bytes use domain `namespace-mutation` and full relative pathname. Prepare
`source/` and `destination/`. Rename A once to `destination/moved-a`, without
enumerating it in performance. Remove B in bytewise postorder. No moved-file
metadata rewrite: rename preserves those inodes. Normalize surviving affected
parents `source/` and `destination/`; their ancestors remain untouched.

Locality uses canonical root shared profiles. Fixed move uses the exact
`regular/s000/f064.dat` to `dest/moved.dat` endpoints and one rename.
Distributed SDK schedule selects shard s=rank("workspace-distributed-sdk")[k],
j=128+(s mod 64), offset zero, delete length 4096. Replacement is the original
first 4096 bytes XOR 0x5a, constructed from input generator outside edit timers.
This guarantees changed bytes and distinct eligible files without a FUSE read.
The SDK preserves the input metadata; no extra metadata mutation is substituted.
Dense order is rank("workspace-dense-rewrite"). For each selected shard visit
j=0..199, rewrite every byte with bytes("workspace-dense-rewrite",path,L),
using existing-file writes at offset zero without truncate or per-file sync.
Prepare replacement blocks outside measured file-write calls, but generation
inside the composite workload remains included in its wall. No cached output
Store is allowed. Verify every rewrite differs from its input during fixture
qualification; a deterministic identity collision is a qualification failure.

## Complete dependent episodes

Place the 64-shard background under `background/`; prepare `cells/` and
`finished/`. For i=0..499, prepare `cells/e{i:03}/source.bin`, `edit.bin`,
`replacement.bin`, each 8192 bytes from domain `agent-episodes` and full initial
path, and hard link `alias.bin` to edit.bin. Select rank("agent-episodes").
All operations for a selected cell run serially in the canonical seven stages.

Read source S completely. Overwrite edit offset 2048 with 4096 bytes
`S[t] XOR 0x5a`, t=0..4095. Read the complete alias as observation A. Append
16 bytes `S[t] XOR 0xa5`, t=0..15, read that suffix through alias as B, then
truncate edit to 8192. Rename the cell to `finished/e{i:03}` and read edit as C.
Temporary `.replacement.tmp` contains `S[t] XOR C[t] XOR 0x3c` for t=0..8191;
sync, close and rename it over replacement.bin, then read replacement as D.
Create relative symlink `replacement-link` targeting `replacement.bin`, read
through it as E. Create scratch.bin containing S[0:4096], read as F, remove it.
Write output.bin with byte t equal to
`S[t] XOR A[t] XOR B[t mod 16] XOR C[t] XOR D[t] XOR E[t] XOR F[t mod 4096]`.
These are bounded data-dependent computations, not benchmark verification
hashes. Do not substitute expected bytes for any named observation.

Normalize changed surviving edit/alias inode, replacement, output, symlink and
cell directory, plus cells/finished parent directories. Unchanged source inode
is preserved. Root sync occurs once after all N episodes, with no per-episode
Exec or Commit; the required temporary-file sync occurs in every episode.
Verification additionally checks each intermediate observation and scratch/
append lifetime, exact final output and full background/unselected cells.

## Independent verification and failure custody

Build expected manifests from deterministic input records and the explicit
transformations above before operating on the product. Use a separate pure
oracle transformation path; do not derive expectations from workload receipts.
Manifest fields are path, type, length, mode, mtime, streamed content digest,
symlink target and hard-link equivalence class. Build an independent native
expected tree only when required for qualified canonical-root construction;
its disk/time is verification or preparation scope, never workload scope.
The existing canonical tree encoder may encode independent manifest data;
record that dependency and do not accept the product's output root as expected.

Verify all actual paths after fresh Store reconnect and real-FUSE remount,
including untouched paths. Independently compare expected Branch transition,
Commit outcome and canonical root; failed publication must not become Created.
Checks of cancelling operations and transient lengths occur at the specified
verification-only barriers. Failures retain exact errors, completed prefixes,
observed state and source identities. An unexpected capability failure remains
a failed product result after harness investigation, never an empty passing
manifest or fabricated route/resource zero.
