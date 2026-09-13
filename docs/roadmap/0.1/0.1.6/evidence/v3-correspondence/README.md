# V3 metadata correspondence component

Status: component PASS, four tests against `attempt-03-source.json`. Shared
FrozenFile/content-builder integration, public C1/C2 acceptance and final-candidate
capacity/benchmark evidence remain open. No benchmark or #122 scenario ran here.

`correspondence.rs` implements explicit 24-byte logical `OriginId`s, one monotonic
allocator per Workspace, normalized 49-byte descriptors, and two roots in the
existing shared disk Index: canonical offsets and nonzero `(origin,start)`
intervals. It rejects duplicate/overlapping coordinates, gaps, empty descriptors,
overflows and excess descriptor counts. Physical relocation may preserve lineage;
fresh equal-content occurrences cannot impersonate prior occurrences.

The first matching pass writes monotonic nonzero anchors to an unlinked journal.
The second uses virtual start/EOF anchors and matches zeros only inside each old/new
gap, skipping deleted nonzero spans. Zero runs never advance the first-pass cursor.
The resulting owned iterator emits increasing disjoint predecessor Base spans and
snapshot-relative Replacement spans, preserving explicit zero replacements. It
validates full new coverage, predecessor order and both EOF bounds. The component
has no payload reader or canonical encoder and never rewrites live state.

Metadata input/output is streamed. Each journal has one 16 KiB stdlib buffer;
64-record index batches plus fixed records keep the declared component buffer bound
below 64 KiB, excluding the separately budgeted Index page machinery and input
producer. Combined logical journal bytes have an explicit caller-provided limit.
Actual simultaneously allocated anchor/plan bytes and retained-plan physical bytes
are recorded using `fstat`; host aggregate physical reservation remains a runtime
integration responsibility. There is no claim that logical bytes equal allocation.

Each 64-record Index scan resumes at an exclusive last key, so the current Index
API introduces a boundary seek per batch. `origin_seeks`, `offset_seeks`, descriptor
visits, build-index operations, anchors, fragments, backward bytes, reused and
replacement bytes are counted explicitly. This is not a claim of a retained
page-stack cursor's ideal one-seek complexity. Canonical unchanged data is represented
by predecessor coordinates; it is never processed byte-by-byte by this component.

## Checks

- Exact reviewed `A/X/Y` insertion example; shifted partial origin overlap; fresh
  lineage despite assumed equal content; relocation preserving lineage; reordered
  backward spans becoming replacement input.
- Adversarial leading-zero insertion and deletion before a **1 PiB logical zero
  run**. The latter yields one anchor and one merged Base fragment covering the run
  and following byte, **49 logical journal bytes and zero payload bytes read**.
  This is symbolic metadata stress, not a supported public file-capacity claim.
- Multiple zero-gap fragments, fresh nonzero gaps, empty input and truncation.
- Duplicate origin intervals, range/coordinate failures, descriptor/journal limits,
  old-input preservation, unlinked temporary ownership and nonwrapping origin ids.
- 180 fragmented origin intervals across index batches: 180 exact spans, no
  replacement payload, bounded overlap visits and descriptor traversals.

## Preserved attempts

1. The first shared compilation was stopped by an ambiguous integer type on
   `old_end.saturating_sub`; the exact `u64` type repaired it. Raw failure remains
   in `../phase2-components/index-uncertain-header-attempt01.log`. No test ran there.
2. `attempt-02-tests.log`: four PASS. Retained but invalidated as complete component
   evidence after the shared Index header/refcount recovery implementation changed.
3. `attempt-03-tests.log`: four PASS after that Index fix was independently proved;
   also covers final allocated-journal-byte counters. Only these four affected tests
   reran. Other successful Store/component families were not rerun for this change.

## Integration contract

Allocate fresh origins for new byte occurrences, preserve identity/coordinates on
split/trim/physical substitution, and never reuse an origin coordinate. Build the
current description from captured range metadata, bind its canonical root only to
the exact result from the existing shared builder, and replace the published inode's
latest description only after authoritative publication. Retain earlier roots only
while an active attempt needs them. Feed Plan spans into the existing FrozenFile /
physical-predecessor cursor machinery; keep original captured provenance for the
next description rather than rewriting it to transient C1 offsets.
