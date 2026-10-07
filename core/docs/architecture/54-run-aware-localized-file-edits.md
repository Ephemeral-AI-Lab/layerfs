# Run-aware localized file edits

> Status: source implementation following checkpoint
> `dcdf527584675849e7839ca4118d71ac9aa4b193`; eleven public cases and affected
> host/Linux checks pass in the [component checkpoint](../issues/307/SPARSE-SERIAL-PROGRESS-20261007.md).
> Captured Overlay/Workspace normalization and integrated Commit remain separate.

The additive `EditSource::read_run_at` returns the existing `FileRun::Data`,
`Zero` or `End`. Its default delegates to `read_at`, preserving existing source
implementations. Replacement offsets remain replayable and stable for the whole
operation. Data must be nonempty and fit the supplied byte buffer. Zero is a
nonempty logical span that can exceed the buffer but must fit the replacement's
declared remainder. Checked u64 offsets/sums and early-end refusal enforce exact
declared length. There is no resident run list or artificial total size/run cap.

The private replacement cursor propagates original typed source failures directly.
Premature EOF retains the old construction Io outcome and comparison's
InvalidEdit outcome. Public `ReplacementReader` remains the existing byte adapter;
production construction uses the typed run cursor instead of converting failures
through std::io. Caller-provided run stability is a contract, not an extra source
probe or replay after an error.

## One scanner and canonical builder

Complete `construct_runs`, localized replacement subtrees and WholeFile-to-chunked
construction share `construction/chunk_runs.rs`. It owns one existing Scanner and
ExtentBuilder, uses consume/zeros/finish, and returns a checked MappingBuild. The
frozen CDC profile, mapping codec, streaming lookahead and half partitions are
unchanged. Zero processing preserves the scanner's odd pending-byte alignment and
reuses complete 32 KiB zero chunks/pages through the existing repeated builder.
Every consumer call remains synchronous and one-attempt; failure stops immediately.

A localized middle uses this mapping helper directly. It does not invoke complete
construct_runs or introduce a second cutoff/FileState dispatch. Each replacement
keeps its original independent CDC boundary and enters the same tagged split/join
engine. A WholeFile-to-chunked result consumes a lazy Plan run cursor with one
continuous scanner across retained and replacement segments. Its known length is
checked before FileState acceptance. Small results still append zeros directly
into their final policy-bounded canonical allocation; no huge-hole allocation is
hidden there.

The repeated-chunk predecessor forwarding preserves UnchangedPrefix provenance
for each actual accepted replacement chunk. The advisory hint changes no canonical
identity, partition or dependency. The existing push_repeated_chunk API forwards
with no predecessor, preserving complete-construction behavior.

## Authenticated positive zero evidence

Comparison consumes replacement runs. Data uses the existing bounded base range
read. A Zero run traverses only its requested stored overlap, acquiring actual
authenticated mapping/payload objects on demand. A fixed 64-entry positive witness
memo is separate from PageCache and mutable edit records. It opens no provider,
performs no backing get/apply, and makes no mutation before the editor's existing
scope guard. A known zero no-op can therefore retain its original root without
occupying mutable backing or accepting an object.

Each mapping witness retains exact logical bytes/extents/level and validated
non-root applicability. Every reuse checks those requested facts and context.
Fresh grammar/summary validation precedes PageCache insertion or eviction.
Chunk witnesses retain the authenticated payload length and validate slice bounds.
A chunk is remembered only after inspecting its entire payload as zero. A mapping
subtree is remembered only when the requested overlap covered all its logical
bytes and every referenced byte was proven zero. Partial zero slices never become
whole-payload or whole-subtree certificates. A child-summary/context mismatch,
absence or original provider error ends comparison without construction, fallback
to another source or automatic retry.

The witness memo clears wholesale before admitting a 65th distinct entry. This
is bounded immutable work state, not a file/edit admission limit. Eviction may
repeat real demands. Repeated zero DAGs need only their distinct mapping/payload
evidence; diverse fragmentation pays actual visits and can reread evicted facts.
No universal logarithmic work or whole-base zero certificate is claimed. The
format-depth DFS retains one bounded decoded page per frame. Provider/transport,
Save, consumer, OS and backing residency remain independently owned costs.

## Public evidence and remaining scope

External edit_runs tests compare run/byte canonical roots at odd boundaries and
through representation transitions. They cover >4 GiB explicit zero replacement,
>4 GiB real repeated-page zero no-op, more than 64 distinct payload witnesses,
partial/nonzero slices, shifted repeated summaries, underfilled child context,
exact provider/source/consumer failures, malformed runs, predecessor provenance
and occupied-scope pre-accept refusal. Counts observe actual source run calls,
authenticated object demands and consumer acceptance; they are functional work
assertions, not timing or whole-system residency qualification. The external raw
record fixture supplies no SQL/runtime claim. Exact commands, failures and repairs
are retained by the component checkpoint.

EditCounters retain their existing scope: unfinished draft charge and actual
construction work. Sparse repetition reduces actual accepts; logical zero length
is not renamed as scanned/emitted work. Comparison's provider demands remain
comparison work rather than construction nodes_read. Complete RunConstruction
keeps its logical zero versus processed boundary counters.

The caller must still provide stable captured Data/Zero runs and final normalized
edits over the exact retained generation/root/floor. The existing captured byte
window alone does not implement indexed sparse normalization. This slice supplies
no chronological mutation replay, namespace/full-filesystem update, SaveFinish,
Stage/CommitStaged, local install, release resolver or integrated Commit result.
