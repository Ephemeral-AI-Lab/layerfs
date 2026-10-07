# Backed file-edit state

Terminology update2026-10-07: the current API/SQL names below use overlay
schema16. Earlier algorithm and proof pins retain their original scope; see
[the naming checkpoint](../issues/307/PRE-S8-TERMINOLOGY-20261007.md).

> Status: source implementation following foundation commit `889836c44`;
> component checks are recorded in the [joint checkpoint](../issues/307/K1-BACKED-STREAM-SUPERVISOR-20261007.md).
> This describes the file editor and its neutral
> state port, not captured normalization, sparse Commit or S10 qualification.

`apply_edits` and additive `apply_edits_backed` dispatch on the same declared
result representation and use one tagged split/concat/half-partition engine.
Existing empty, WholeFile, streamed construction, comparison, predecessor and
canonical mapping formats remain selected by the existing policy. The backed
entry point requires a caller-owned `IndexedEditBacking`; it opens no database,
discovers no provider and performs no operation release.

## Exact reference provenance

Private `EditRef` distinguishes Stored(ObjectId), DraftNode(address-width counter)
and DraftPage(ObjectId). The immutable base and stored decoded children are
Stored. The replacement builder's already-canonical mapping pages are DraftPage;
split/join outputs are DraftNode. Node/page draft record domains remain separate
even if their full binary keys equal each other or a canonical stored identity.
No digest prefix, probability, absence probe or sentinel proves disjointness.

Stored references alone use authenticated object I/O and the bounded PageCache.
A draft uses its exact record domain. Missing draft state requires an explicit
acknowledged resolution or ends with an error; it never becomes a stored read.
Page resolutions retain their original canonical identity. Editable branch
descriptors retain tags until known child resolution, then become canonical IDs.

The public memory `EditObjects` signatures remain, including void `settle`.
Its untagged `NodeSummary` compatibility aliases cannot express arbitrary
Stored-versus-Draft provenance; the old impossibility comment is removed. Normal
memory/backed drivers use private tags directly. The compatibility void method
retains its first error for the next fallible method and stops later work. Normal
drivers propagate release/discard/settle/finish errors immediately.

## One state protocol and bounded raw codec

Memory state retains decoded Nodes and original FinalizedObjects. Backed state
stores all growing draft, parent-count, detached, resolution and emission indexes
through the neutral port. The common engine computes every transition and the
codec lives in Content; SQL performs no mapping algorithm. Scalar counters and
the format-depth traversal frontier remain local. Both profiles share all tree
decisions; there is no failure-driven memory substitute.

The public port carries `EditRecordKey`, `EditRecordExpected`, `EditRecordChange`
and `EditRecordApply`. Keys have opaque u32 domain kinds and all 32 identity bytes.
Apply owns a sorted unique guarded batch; Missing/ExactBytes expectations are
checked before every effect. The complete outer descriptor and nested Vec
capacities fit 65,536 bytes. There is no change-count or total-state limit on the
backed route. Grammar-derived exact vector reservations avoid capacity doubling.
First-key windows return at most 64 keys, with optional exclusion only in the
retained root's exact draft domain, and restart from the first eligible key.

The versioned temporary codec encodes mapping Nodes or finalized mapping Pages
only. Nodes retain exact totals and tagged branch references; they are not
canonical objects before child identities and root context are final. Pages retain
canonical bytes, ordered direct references and all advisory predecessor provenance.
Decode checks versions, tags, counts, totals, identity, role/reference agreement,
counter width, predecessor bounds and trailing bytes. The conservative mapping
record fits below 16 KiB (8,192 canonical bytes, 128 refs and four hints plus header).
Chunks are accepted immediately; WholeFile/FileState objects are not draft records.

Parent counts use checked address-width arithmetic. A bounded local multiplicity
map aggregates repeated children of one at-most-128-entry node. Parent-count and
detached transitions share the same atomic draft operation. Release of a still-
referenced draft preserves it. Branch splits retain the original parent through
both concatenations: a singleton prefix or suffix is a bare summary and supplies
no new counted parent. Taller joins retain their parent through boundary
construction, preserving repeated-child DAG references. Leaf splits and joins
release their superseded drafts before allocating the result; payload slices own
no draft mapping children. Joining equal references releases one original draft
once. Partitions and final canonical bytes retain the existing algorithm.

## One attempted emission and explicit caller custody

The mutable backed chunked editor has a guarded local scope marker. Reusing that
editor scope refuses before its new mutable effects or payload acceptance;
counters cannot reset onto old resolution records. Empty, WholeFile, whole-base
streaming and no-op dispatch intentionally bypass this state provider. The marker
describes local construction only, not SaveFinish or history publication.

Finalization resolves reachable children before parents. Each mapping acceptance
has an Attempted record before the original consumer call and an Accepted record
only after known success. Accepted duplicates reuse acknowledged resolution.
An attempted/unacknowledged record ends construction without resend. Draft
consumption, resolution and reference transitions follow known acceptance in one
guarded job. A failure after consumer acceptance preserves that original outcome
separately from failed bookkeeping; earlier accepted objects are not deleted.
File-state acceptance is attempted once after mappings, with a distinct local
scope marker recording its pending/known-complete state.

An acknowledged resolution retains a versioned 51-byte witness: canonical ID,
exact logical bytes/extents/level and validated non-root fill applicability.
Every subsequent draft reference checks those same requested facts and context
before reusing the acknowledgement. Accepted emission identity alone cannot
validate a changed interior child boundary. This pays an extra 19 retained bytes
and their actual read/write/copy work per resolution; it needs no Stored fallback.

Workspace binds one OperationOwner/file scope to its OverlayOperationRecords service and
retains the first original Completion/unattempted command or Workspace error.
NotApplied is a terminal deciding result, with its original Completion retained;
Content receives the bounded typed body once. Later requests submit no job.
Construction, Save, transport and captured-reader fences govern explicit owner
release. Drop does not guess release, and no marker is a crash-recovery resolver.

## Work and remaining evidence

Memory preserves the former unfinished charge calculation: decoded ExtentNode
and entry sizes plus 128 bytes, or canonical Page length plus 128 bytes. Its
8 MiB-derived refusal remains. Private tags/reference indexes and consumer/Save
ownership are outside that counter, as existing auxiliary indexes were. Backed
`peak_deferred_bytes` is the same unfinished logical charge, not resident backing
bytes. Neither value proves aggregate residency.

The backed route pays raw encode/decode, point/guard/reference/window jobs,
mapping-page re-identification, actual allocation/copy and cleanup work. Memory
does not serialize drafts for storage; finalization now retains draft custody
through acceptance and may clone a bounded decoded node/finalized page. Private
child tags add metadata, and exact repeated-child lifetime transitions can retain
bounded original parents longer. These costs are source changes, not credited
memory or performance improvements. Both profiles retain format-depth frames;
raw-state and service windows do not bound pager/journal/OS cache or Save/transport.

External Content tests compare memory/backed canonical roots, bytes and counters,
exercise original backing/consumer and post-accept failures, malformed drafts,
scope reuse, cutoff/no-op dispatch and repeated-page identities shared with stored
content. Their raw-record fixture is external and supplies no SQL/runtime claim.
Real Owner/Overlay integration belongs to the separate Workspace/Daemon adapter
tests. State above the old deferred limit, dense/sparse >4 GiB, fragmentation,
captured final-state normalization and qualified speed/storage/residency remain
owning acceptance work; this component checkpoint does not close them.

The subsequent [run-aware editor](54-run-aware-localized-file-edits.md) reuses this
engine with Data/Zero/End replacements, one continuous frozen scanner and bounded
positive immutable zero evidence. Its public >4 GiB sparse cases establish
functional compatibility for those selected inputs. They do not establish dense
large-file, complete captured normalization or aggregate resource qualification.
Neutral `ConstructionRecord*`/`IndexedConstructionBacking` names alias this exact
protocol; the [filesystem serial-state route](53-backed-filesystem-serial-state.md)
uses disjoint private kind domains and an explicitly separate caller scope.
