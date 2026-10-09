# Owning Store file-length facts

> **Status:** Current general guide; P5 prerequisite slice, not complete runtime/stat qualification.

The #307 checkpoint after `bfeec632c` adds
[Reader::file_lengths](../../crates/layerfs-storage/src/read/provider.rs) and an
authorized borrowed SDK delivery method. One window accepts the existing 4,096-ID
processing bound, preserves demand order/repeats and rejects missing/non-file
roots. This does not restrict a file, namespace, Save or Workspace total.

[Length lookup](../../crates/layerfs-storage/src/read/length.rs) uses acknowledged
owning Store descriptors for whole-file roots: canonical length minus the owning
format's 23-byte overhead, checked for nonempty data within persisted Store policy.
It fetches no whole-file payload and does not attest current payload integrity.
The trusted Store metadata port supplies the fact; payload reads retain their
separate hash/framing checks and failures. This is not a hash-only untrusted object
admission shortcut, a guessed length or an error-driven fallback.

Chunked/empty file roots authenticate only their small state objects through the
existing read path, then use the owning decoder's logical length. The owning
mapping codec exports its exact 106-byte canonical width; an incompatible declared
descriptor fails before body acquisition. No extent/chunk traversal is required.
Metadata pack acquisition may still read/hash more physical bytes than 106; count
that actual scope rather than claiming zero I/O for chunked files.

The SDK authorizes exact object IDs under its authenticated binding and delivers
lengths after provider locks release. Delivery credits/queues and the daemon's
immutable base/stat integration remain required. BaseView still opens FileView for
regular-file length, so complete P5/S3/S9 acceptance is not claimed from this API.

Catalogue work uses the same actual location SQL as normal reads, now centralized
for query and [Handles::explain_locate](../../crates/layerfs-persistence/src/store/handles.rs).
The read-only diagnostic respects distinct-ID paging on the initialized database.
Runtime SqlWork records/reset actual fullscan/sort/autoindex/reprepare counters
and decoded returned rows alongside existing VM/binding/wall/transaction/BLOB
observations. Returned prefixes of failed statements remain counted. No retry or
Store/overlay profile change is introduced.

Source work is O(U log U + U log N + K log C) metadata set/seek work for K<=4096
demands and U distinct roots, with bounded locator cache C. State decoding and
actual bounded metadata-pack work apply only to those roots. Temporary result/
state-position vectors are window-sized; nothing grows with logical file length.
No timing/cache/throughput or huge-workload claim follows from these source bounds.

Real macOS provider proof uses a fresh Storage handle after a 131,071-byte file is
saved. Two repeated demands perform one distinct catalogue lookup, three SQL
statements/29 VM steps/one returned row, zero fullscan/sort/autoindex/reprepare and
zero BLOB/body/payload reads. Actual EXPLAIN seeks object_location's primary key
then pack's integer primary key. A mixed genuine whole/empty/chunked construction
proof returns ordered lengths with zero payload reads and 926 acquired metadata
pack bytes. These are scoped count diagnostics, not a cold latency qualification;
OS/database cache residency is not asserted. Non-file, missing and oversize windows
fail explicitly. Final logs retain the exact checked source identity.

## S3 completion reconciliation

The completion after `bdc6ed4af` adds effective source-qualified read/ordered-name
composition, actual paired actor install, original unattempted-command custody and
owning SDK stat lengths. See [effective base view](29-effective-base-view.md) and
[S3 exit audit](../issues/307/S3-EXIT-AUDIT.md). Earlier limitations/evidence above
retain their source scope; native/logical runtime transport, mutable byte semantics
and aggregate resource acceptance remain unfinished.

## R7 update, 2026-10-09: a length answered once is remembered

Implemented in [the canonical client](../../crates/layerfs-workspace/src/base/client.rs)
and [its cache](../../crates/layerfs-workspace/src/base/cache.rs). The length of
a regular file is a pure function of its immutable content root, so the
client's `FileLengths` answer, which serves a base inode's attributes
(`BaseView::stat`), is remembered under that content root the first time the
length port gives it and is returned from memory afterwards.

- **Where it lives.** In the daemon's one `CanonicalCache`, in the same
  allowance as the canonical objects (`cache_bytes`), under the same lock and
  in the same least-recently-used order. One answer is charged 264 logical
  bytes (its 8-byte value and the 256-byte bookkeeping allowance every cached
  object is charged); what is stored for it is its 32-byte key, the length
  and its age in one map and the age, kind and key in the other. No new
  allowance and no new limit exists: the cache holds at most
  `cache_bytes / 264` answers when it holds nothing else, and an object and an
  answer evict one another by age alone. `ClientWork::file_lengths` reports
  the number remembered; they are part of `charged_cache_bytes` and not of
  `cached_objects`.
- **Key.** The content root only. No serial, base root, Workspace or mutable
  state is part of it, so every Workspace of a daemon and every base that
  still contains the file shares one answer.
- **Memory-only client.** `CanonicalClient::resident`, which reads inside an
  owner job, has no length port. It answers a length only when it is
  remembered; otherwise the fact is not resident, the owner visit is undecided
  and unchanged, and the request reads it outside the owner
  ([native read custody](73-native-read-custody.md)).
- **Just past the allowance.** An evicted answer, or an allowance too small
  for one answer (below 264 bytes, including the zero some tests use), costs
  one more demand on the length port and, for a native request, the second
  visit and its reader grant. Nothing is refused.
- **Not used by Commit.** Captured-file construction checks a captured base
  file against the length port's own answer every time
  (`CanonicalClient::provided_length`); it neither reads nor fills the
  remembered answers, and its Store length demands are unchanged.
- **Explicit ports.** `BaseView::stat_with_lengths` with another `FileLengths`
  port asks that port every time and remembers nothing.

Counted through the daemon's Fuse port with the real owner and Store
([`read_cost.rs`](../../crates/layerfs-daemon/tests/read_cost.rs)): the first
LOOKUP of a base regular file in a daemon is two owner visits, one Store
reader grant and one length batch; every later LOOKUP of it is one visit and
every later GETATTR one visit, each with no reader grant and no length batch.
With no allowance each one is the first again. The cache-level behaviour is in
[`file_lengths.rs`](../../crates/layerfs-workspace/tests/file_lengths.rs).
These are counts, not timings.
