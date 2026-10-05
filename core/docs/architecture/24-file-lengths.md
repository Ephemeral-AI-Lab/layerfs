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
