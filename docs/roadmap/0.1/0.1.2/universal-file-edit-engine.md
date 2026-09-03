# Universal Workspace regular-file edit engine

> **Status:** Proposed v0.1.2 implementation workstream and GitHub sub-issue.
> It is not a benchmark family. Performance families exercise it through the
> fixed Docker/FUSE product path; focused owner-side checks remain here.
> Tracked by [GitHub issue #14](https://github.com/Ephemeral-AI-Lab/layerfs/issues/14).

## Objective

Replace parallel write, dirty-range, charged-spool, truncate, sparse, capture,
and Commit mutation paths with one environment-independent regular-file range
algebra:

```text
FileEdit {
  node,
  start,
  delete_len,
  replacement,
}

Replacement = Inline(bytes) | Zero(length) | Spool(range)
```

For current bytes `old`, success produces:

```text
old[0..start] || replacement || old[start + delete_len..]
```

This covers overwrite, prepend, append, middle insertion/deletion, unequal
replacement, truncate, sparse growth, and write-past-EOF. It does not represent
rename, unlink, directory mutation, chmod, mtime, ownership, xattrs, or every
POSIX operation.

The optional owner-side public operation should be named
`WorkspaceFileRangeEdit`, not `WorkspaceFileEdit`, so its content/size boundary
is explicit. Ordinary metadata and namespace mutation continue through the
mounted filesystem and Commit them in the same final filesystem root.

## Fixed environment and portability boundary

v0.1.2 timed file-edit evidence uses exactly:

```text
MacBook host
-> host-resident LayerStackStore and public Client
-> Docker Desktop
-> managed Linux container
-> real LayerFS FUSE projection
-> fresh workload process
-> explicit Commit and End
```

There is no timed materialization, alternate-driver, native-Linux, Windows, or
macOS-filesystem matrix. The run identity records host hardware/OS, Docker and
engine versions, image digest, container kernel and limits, FUSE capabilities,
Store placement, cache policy, fixture digest, source seal, runner/schema, and
acknowledgement boundary.

The edit engine itself lives below projection adapters and depends only on
LayerFS inode identity, logical offsets, lengths, immutable byte sources, and
canonical objects. It must not depend on `copy_file_range`, reflink, clone,
`fallocate`, or another OS/driver primitive. One untimed materialization/FUSE
equality group protects semantic portability; v0.1.2 makes performance claims
only for the fixed Docker/FUSE path.

## Smallest implementation shape

```text
Workspace FileData::Edited
└── one balanced implicit piece tree
    ├── Base(root, source_offset, length)
    ├── Inline(owner, source_offset, length)
    ├── Zero(length)
    └── Spool(spool, physical_offset, length)

Commit
└── visit final pieces once
    └── maximal ascending non-Base replacement runs
        └── existing FileMutationBatch
            └── one inode upsert
```

The piece tree is the live edit state; do not maintain a parallel edit log.
Ordinary FUSE operations lower as follows:

| Filesystem operation | Internal edit |
| --- | --- |
| Same-count write | Replace written range with `Spool` |
| Append | Insert `Spool` at EOF |
| Write past EOF | Insert `Zero(gap)`, then `Spool` |
| Truncate smaller | Delete suffix |
| Truncate larger | Insert `Zero` at EOF |

Owner-side range editing supplies bounded `Inline`, `Zero`, or empty
replacement. New-file sequential capture may retain its streaming producer but
must converge on the same canonical representation.

## Immutable spool and failure atomicity

A `Spool` slice becomes immutable before a piece root can reference it. Physical
spool offsets are independent of destination logical offsets:

```text
append bytes to an unreferenced physical slice
-> reserve charge, candidate tree, aliases, and next generation
-> atomically swap the piece root
-> mark replaced slices superseded
```

A failed or short append truncates the unreferenced tail to the validated prior
high-water before returning. Failure to restore it is an explicit cleanup
failure. v0.1.2 charges append-only physical allocation until Commit or Discard;
do not add a free-list or compactor before evidence requires one.

## Commit lowering and canonical identity

Commit walks final pieces once, retains monotonic `Base` ranges, and streams
maximal non-Base runs in ascending final coordinates into the existing
`FileMutationBatch`. Chain Inline, Zero, and Spool readers; do not concatenate a
whole replacement buffer. Superseded bytes never enter canonical construction.

The existing structural splice chunks replacement bytes only, splits the old
extent rope, and rejoins old prefix/replacement/old suffix. Preserve that
identity; add no CDC suffix-resynchronizer or second canonical editor.

For owner-side prepend:

```text
final bytes/digest == ordinary prepend oracle
FileStateRoot       == structural-splice known answer
old payload reads   == 0
```

An ordinary temp-copy prepend keeps its full-stream rewrite root. Different
expressions need byte equality, not cross-expression root equality.

## Complexity

Use:

```text
E   internal single-replacement edits after lowering
Pj  pieces before edit j
P   final normalized pieces
Fj  pieces synchronously split, released, or visited by edit j
B   supplied replacement bytes
Bi  final live Inline bytes
S   physical spool bytes allocated since cleanup
Z   final logical zero bytes
R   maximal replacement runs emitted at Commit
C   base canonical extent count
Q/V returned bytes/pieces visited by a live read
```

Required bounds:

```text
edit j          O(path depth + log Pj + Fj + Bj)
all edits       O(E log E + sum(Fj) + total supplied bytes), when Pj=O(E)
live read       O(log P + V + Q + canonical Base-run traversal)
normalization   O(P)
content Commit  O(P + R log C + final Inline + final Spool + logical Zero
                  + prune + candidate-reachability work)
live edit RAM   O(piece allocation + Bi)
spool disk      Theta(S)
materialize     Theta(projected entries + projected bytes), verification only
```

A ten-byte owner-side prepend has `P=2`, `R=1`, `Bi=10`, and performs
`O(log C + 10)` content work with zero old-payload reads. `Zero(length)` is
constant-size live state, but the current durable format still performs work
proportional to its canonical extent representation.

## Resource and memory gates

- Owner-side Inline replacement at most 1 MiB per call.
- At most 4,096 edits and 8,193 live pieces per file.
- At most 8 MiB live Inline bytes per Workspace.
- At most 2 MiB charged tree/edit allocation, measured from allocation capacity.
- Existing 1 GiB physical spool ceiling, charged append-only until cleanup.
- Explicit result-length, logical-zero, and predicted-zero-extent ceilings.
- Approximately 64 KiB fixed FastCDC storage, plus separately measured existing
  candidate/cache/request/projection buffers.
- No complete-file allocation.
- Owner-side focused checks target at most 112 MiB RSS; every mutation row has a
  128 MiB hard ceiling and zero swap.

## Conformance groups

These are verifier/integration groups owned by the implementation issue, not
timed benchmark-family IDs:

1. Range, piece, extent, EOF, no-op, repeated, descending, overlapping, and
   `UpToDate` normalization boundaries.
2. Real-FUSE/materialization equality and projection-generation refresh.
3. Hard-link aliases, rename/parent rename, replacement rename, and unlink by
   inode, including final-alias reclamation.
4. Invalid range/type/overflow and exact-limit/+1 rejection; read handle,
   writer, callback, and execution Busy behavior; allocation/generation and
   short-spool failure atomicity.
5. Candidate, admission, publication, and projection failure boundaries with an
   exact retry, exactly one Commit, and no duplicate prepend.
6. Discard of mixed `Base+Inline+Zero+Spool` state with no durable or runtime
   residue.
7. Stale-head and POSIX/owner-side composition in both orders.

Verification is never included in a performance distribution. The development
loop runs focused unit/integration tests and selected performance cases; the
complete conformance set runs only in explicit verification/admission mode.

## Focused owner-side performance checks

These checks retain descriptive labels but do not form another benchmark
family:

```text
workspace-range-prepend-head-10b-on-32m
workspace-range-overwrite-middle-4k-on-256k-100
workspace-range-insert-middle-4k-on-256k-100
```

For the prepend check:

```text
range edit + Commit/required refresh target / hard  50 / 75 ms
complete lifecycle target / hard                    80 / 110 ms
replacement CDC bytes                               10
old payload/FUSE/spool transfer                      0
old payload-object IDs lost                         0
```

The first semantically complete and fully instrumented implementation is the
owner-side baseline; do not invent a pre-API latency.

## Files to read

- [v0.1.2 release plan](README.md)
- [Same-count family](same-count-file-edits.md)
- [Count-changing family](count-changing-file-edits.md)
- [Persistent rope edit](../../../../crates/layerfs-content/src/file/rope/edit.rs)
- [Workspace state](../../../../crates/layerfs-workspace/src/cow_tree.rs)
- [Workspace file I/O](../../../../crates/layerfs-workspace/src/file_io.rs)
- [Workspace Commit planning](../../../../crates/layerfs-workspace/src/changes.rs)
- [Workspace lifecycle](../../../../crates/layerfs-workspace/src/lifecycle.rs)
- [Public Client](../../../../crates/layerfs-sdk/src/client.rs)

## Acceptance criteria

- [ ] One piece engine serves ordinary FUSE write/truncate and owner-side range
  editing; no second edit log exists.
- [ ] Commit reuses existing `FileMutationBatch` and structural splice; no OS
  copy primitive, CDC suffix scan, alternate canonical editor, partial
  completion, or byte-copy fallback exists.
- [ ] Reachable spool slices are immutable and short/erroring appends restore
  the exact prior high-water/state.
- [ ] All seven conformance groups pass outside benchmark timing.
- [ ] FUSE is the only v0.1.2 timed projection; untimed materialization equality
  proves the engine is not driver-coupled.
- [ ] The focused owner-side prepend meets its latency, transfer, reuse, memory,
  retry, and cleanup gates.
- [ ] Every retained implementation optimization reruns both affected complete
  performance families.
