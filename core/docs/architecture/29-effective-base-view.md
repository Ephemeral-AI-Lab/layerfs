# Effective immutable-base and overlay read composition

> **Status:** Implemented S3 completion after `bdc6ed4af`; S4 namespace mutation, S5 byte composition and S6 kernel/orphan/resource ownership are separate unfinished milestones.

S5 update after `f5558fc22`: `SourceView::read` composes the local layered
window and fetches its single covering inherited canonical range. `stat` takes
the mutable logical EOF; writes do not demand inherited payload. Shrink/regrow
uses the lower-layer cutoff rather than a caller-supplied base cutoff. See
[payload streams](31-payload-streams.md); raw cell access does not establish the
effective view. S6 ownership and S8 output/kernel custody remain open.

Workspace retains a checked complete immutable root, with exact ObjectId caching,
public content child/inode/list/readlink and FileView range plans. No repeated open
imports, scans, copies or restores that base. A source-issued SourceView retains
its selected BaseView. The source lease prevents install while provider I/O is
pending; every short overlay read validates that exact source. A retained immutable
BaseRead keeps its own file root/EOF after the source ends and later install occurs.
Current global content has no GC; this is not a distributed retention lease.

SourceView stat/lookup applies local complete inode values and dentry replacements/
whiteouts over the canonical base. Logical base PathNotFound is distinguished from
provider MissingObject/failure. A whiteout stays absent. Base demand runs outside
SQL; final point selection applies later local metadata/names over the unchanged
base. Aliases resolve the same serial and current attributes. These are portable
read facts: namespace_refs is not POSIX directory nlink. Permission checks, atomic
compound namespace effects and lookup/open/orphan references belong to S4/S6.

Directory merge uses one bounded canonical page and one owner NameWindow containing
active/captured generation pages. Each SQL cursor constrains namespace, generation,
parent and binary resume key through dentry_capture. Active overrides captured,
which overrides base. One processing window visits at most64 distinct keys,
including deleted entries; an empty visible page can return a progress cursor.
No prefix rescan or resident directory mirror is used. Readdir under mutation has
weak current-view semantics; native stable cookies/handle custody remain S4/S8.
The public canonical directory reader now accepts a raw resume boundary independent
of stored-name grammar. Canonical stored names remain UTF-8, exclude backslash,
and are <=255 bytes; no encoded object/ID changed. P12 must resolve faithful raw
Unix-name/import requirements. This cursor extension is not a format substitution.

The daemon implements OverlayRead with real typed short jobs. The Workspace does
canonical work on its consumer thread, so a blocked provider leaves the SQL owner
and other namespaces runnable. InstallPrepared carries the already checked root
into that owner; its readiness fence waits for earlier sources, then pairs the
existing engine install with Workspace binding publication. No network/canonical
I/O runs under the binding lock. Stopped/admission/readiness-refused unattempted
jobs retain original commands; attempted install errors retain original error and
PreparedBase. Workspace service errors retain their exact completion/error chain.
This updates the earlier Stopped-only queued-cancellation contract explicitly.

BaseView stat now obtains regular-file length through an explicit owning FileLengths
port. Missing capability fails; it never opens a whole payload to guess length.
CanonicalClient can retain a shared thread-safe port; stat_with_lengths accepts a
borrowed port. SDK Sessions::length_port adapts its authenticated binding and
initialized demand Storage, preserving runtime/storage/authority errors. It creates
no new provider/session handle and remains compatible with stack-borrowed Saves.
Whole facts use trusted acknowledged Store descriptors, not payload-integrity
attestation. Chunked/empty facts use the existing small authenticated file state.
The logical network metadata protocol remains S9 integration work.

Production edges are daemon -> Workspace for actor install/read composition and
SDK -> Workspace for the consumer stat metadata port. Guard coverage includes
those actual replacement edges while rejecting reverse production imports, domain
engine dependencies and server/legacy wiring. There are no third-party graph/pin/
checksum/source changes. Workspace's daemon dependency exists only in external tests.

## Work and evidence

Point reads pay indexed O(log N) local seeks and actual canonical visited paths.
A directory window fetches at most64 base names plus64 per local generation and
merges at most64 keys: fixed transient/output windows, O(visited keys + page/path
work), with bounded lookahead charged each call. Across an unchanged directory,
resume keys advance and deletion work contributes to progress. Source checks and
parent reads are additional fixed statements, not hidden in a name-only total.
Pager/journal/kernel/provider queues and caller-retained output credit/residency
still require S7/S8 acceptance; this is not an aggregate RSS or throughput claim.

Actual EXPLAIN seeks dentry_capture(ns,gen,parent,name>cursor). Name-family VM
stays806 macOS/804 Linux beside128/1024/4096 unrelated names, with64 returned and
zero fullscan/sort/autoindex/reprepare. The final Linux complete owner window pays
9 statements/921 VM including source/parent checks. Existing point/capture/install
plans are reused at unchanged SQL scope. The 507-key merge visits507 keys, emits377
and progresses through an empty deletion page. This is count-driven correctness/
work evidence, not a cache-cold latency or large-workload qualification.

Actual provider-blocking and actor-install tests prove continued read/mutation/
unrelated progress, finite source fences, new selected root, preserved later metadata
and old immutable bytes. The macOS SDK/real Store stat test serves131071 bytes of
logical length with zero owning payload/pack reads, authorized by an actual native
handshake binding, then preserves RuntimeError::Denied without fallback. Required
checks/source/build pins are recorded in [S3 exit audit](../issues/307/S3-EXIT-AUDIT.md).

## R7 update, 2026-10-09: derived child-directory counts

`ViewStat` gains `subdirs`, the number of a directory's bindings that are
directories; the reply reports two more as the POSIX link count
([native mutation and kernel coherence](77-native-mutation-coherence.md)).
`namespace_refs` is still not a link count. A local row carries the value in
its `subdirs` column ([namespace operations](30-namespace-operations.md)).

The canonical format stores nothing of the kind: a directory row is a name and
a serial, and a directory's reference count is 1. For a base directory the
count is therefore **derived** in
[`links.rs`](../../crates/layerfs-workspace/src/base/links.rs):

- An empty directory (root-page count 0) is 0 with no read.
- Otherwise the directory is listed with the existing listing API in windows
  of at most 256 rows and 64 KiB, and each window's serials are resolved with
  one batch inode demand (`FilesystemRead::lookup_inodes`). Nothing held
  grows with the directory; the listed total is checked against the root-page
  count. Cost: one root-to-leaf descent per window plus the inode-table pages
  of the listed serials, O(entries) once.
- A serial's kind never changes, so the result is a pure function of the
  directory's `content_root`. It is remembered under that key in a
  least-recently-used table owned by the shared `CanonicalCache`, beside the
  object cache and under the same lock: `DIRECTORY_COUNT_CAPACITY` = 16,384
  entries for the whole daemon, 88 logical bytes each (1,441,792 bytes at
  capacity; accounting of the stored values, not a measured allocation).
  Eviction loses an answer and nothing else. `ClientWork` reports
  `directory_counts` and the cumulative `directory_count_scans`.

The derivation is provider work. It runs where base facts are read outside
the owner: `SourceView::stat`/`lookup`, `SourceView::supply` and
`VisitFacts::supply`. `BaseView::base_inode` seeds a first local row's
`subdirs` from it as it seeds `entries`. A memory-only client never derives;
see [native read custody](73-native-read-custody.md).

After an install the directories a Commit changed have new content roots, so
each is listed once more on its next stat. That recomputation is the accepted
cost of keeping the canonical format unchanged; no second mechanism carries
counts across an install. Not measured here: latency of the first stat of a
very large base directory, and the hit rate of the table on trees with more
than 16,384 non-empty directories.

## S4 update

Lookup and listing now read the directory's local row first: a directory
created above the installed floor makes no base demand, and a removed directory
is absent. `readlink` serves a locally created symlink from its creation cell
and an inherited one from the base object. Enumeration semantics under mutation
are declared in [namespace operations](30-namespace-operations.md), which also
supersedes this document's statement that atomic namespace effects are pending.
