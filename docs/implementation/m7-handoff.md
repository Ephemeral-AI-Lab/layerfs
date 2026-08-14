# M7 Node VFS handoff

Milestone 7 is accepted with a complete local Node VFS implementation and exact
real-kernel FUSE evidence. `validate:accepted` now selects M7; the candidate, atomic
evidence, and constrained acceptance commits remain separate.

## Supported integration boundary

- `openNodeVfs` opens one portable filesystem and one synchronous bridge that share the
  same persisted format, admission controller, content cache, and runtime limits.
- The Node package receives semantic range, pinned-read, staging, commit, namespace, and
  accounting operations only. It has no SQL, schema, repository, manifest, CAS,
  COW-page, or FUSE dependency.
- Computer owns FUSE flags, kernel handle allocation, mounting, process lifecycle, and
  forwarding. The test host demonstrates that boundary without moving FUSE into a
  production package.

## Read and write behavior

- Read handles retain an inode/root selection through a durable read lease and reuse an
  authenticated bounded manifest cursor. `readIntoSync` fills the caller's exact range
  directly and preserves destination sentinels.
- Writable handles use provider-wide inode coordinators, monotonic admissions, bounded
  dirty records, core-admitted slabs, and direct persisted-range reads. Pending creates,
  aliases, rename, and unlink remain coordinated by inode identity.
- Hidden prefix staging consolidates a bounded group into one core streaming payload,
  transfers ownership explicitly, releases resident capacity, and never advances the
  visible namespace. Flush prepares and atomically commits the complete required inode
  sequence; failures remain open, readable, accounted, and retryable.
- Eligible equal-length overwrites use the core's synchronous local rebuild/path-copy
  route. The 100 MiB one-byte test proves source reads remain below 8 MiB. General
  sequential, sparse, and truncating compositions use a bounded streaming source and
  never allocate a file-sized provider buffer.

## Local verification boundary

`pnpm test:m7:local` builds the workspace and runs the file-backed shared conformance,
format, restart, fault, and resource selection under an executable 600-second deadline.
Coverage includes all 36 three-session commit/close order pairs, garbage collection
under a pinned lease, exact 1/16/64-session pressure, 4/8/16 KiB formats, a 20 MiB
single callback, a 100 MiB COW edit, and every observed SQL position in separate hidden
staging and visible-commit phases.

`pnpm test:m7:fuse` is the mandatory Linux target. It refuses non-Linux hosts, missing
or inaccessible `/dev/fuse`, missing `fusermount`, and an unavailable test dependency.
On a qualifying runner it starts four distinct provider processes across three real
unmount/remount cycles, checks `/proc/self/mountinfo` for each FUSE kernel mount, and
runs the exact 60-second profile: a deterministic 16 MiB persistence fixture, 5,000
mounted one-byte edits, 2,000 namespace operations, 16 readers and 16 writers with 64
operations each, fsync-crash and separate close durability, shell/Git interoperability,
interrupted/resumed/final collection, final digest/namespace/usage verification, and
zero active leases, staging records, or reservations.

The CI label contract is `[self-hosted, linux, x64, fuse]`. The candidate-bound run
completed the exact profile in 24,815 ms, and M8 may now use M7 as its sequential
accepted predecessor.
