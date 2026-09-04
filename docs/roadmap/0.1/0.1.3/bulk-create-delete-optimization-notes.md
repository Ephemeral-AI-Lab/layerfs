# Bulk-create and bulk-delete optimization notes

Date: 2026-09-04. Discussion record following three read-only subagent reviews.
These are optimization directions and provisional targets, not implemented
improvements, revised benchmark contracts, or additional Phase 1 pass gates.
Functional repairs remain required by the [completion amendment](failure-repair-amendment.md).

## The 2.7-second initialization reference

The user's reference to "initialize workspace in 2.7 seconds" is the v0.1.1
namespace-v2 **LayerStack initialization from an already prepared directory**:
100,000 files / 500,000,000 bytes in **2.766 seconds**, approximately **180.7 MB/s
and 36,149 files/s**. Opening the subsequent Workspace session took **12.556 ms**.
The older benchmark's **3.857 ms Commit** followed a small edit; it is not a
bulk-create or bulk-delete Commit result.

See the [v0.1.1 architecture/results record](../0.1.1/architecture_shift.md#current-namespace-v2-evidence),
[retained report](../../../../benchmark-results/fs-bench-pro/namespace/issue11-v3-terminal-all4-composite-r003-20260903/report.md),
and [100k raw sample](../../../../benchmark-results/fs-bench-pro/namespace/issue11-v3-terminal-all4-composite-r003-20260903/scenarios/namespace-100000/sample-003/result.json).

The 2.766 seconds excludes source-file creation and metadata normalization;
fixture generation recorded about 14.8 seconds separately. It also excludes
the separate reopen verifier. Initialization used eight host producers and an
eligible empty-Store path. The fixture uses decimal MB, a different file-size
distribution and topology, and an older resource profile. The current workload
uses MiB, real POSIX/FUSE creation, and an existing Store containing a witness
tree. Therefore 2.7 seconds is a strong reference for construction/admission
efficiency, not a measured total for live bulk creation.

## Current evidence and the gap

Corrected source: `fbf32e84662d00993c033515e113437965395494`. The following are
medians of three successful performance samples per case, retained in the
[slot ledger](../../../../benchmark-results/fs-bench-pro/phase1-v013/slots.json).
They were pending complete independent verification/admission at this review.

| Workload | Timed SDK/workload lifecycle |
| --- | ---: |
| Create 20,000 files / 100 MiB | 31.68 s |
| Create 100,000 files / 500 MiB | 188.07 s |
| Delete 20,000 files / 100 MiB | 6.98 s |
| Delete 100,000 files / 500 MiB | 45.50 s |

These totals include Exec through workload completion, Commit, session creation,
visibility query and End. They exclude preparation, outer CLI startup and the
separate benchmark verifier. Ordinary product integrity checks remain included.
The prescribed metadata changes are measured filesystem work, not verification.

At 100k files, creation has approximately **95.26 s Exec**, including **65.04 s
metadata normalization**, and **93.18 s Commit**, including **44.22 s Workspace
refresh**. Deletion has approximately **24.58 s Exec** and **20.89 s Commit**,
including **20.44 s namespace processing**. These independently computed phase
medians need not sum exactly to the lifecycle median.

The historical and current internal construction work is similar in size:

| Work | Historical 100k initialization | Current 100k bulk-create Commit |
| --- | ---: | ---: |
| Candidate objects | ~422,000 | ~412,000 |
| Candidate bytes | ~543 MB | ~567 MB |
| Admission transactions | 131 | 3,234 |
| Largest observed object batch | ~6,664 | 127 |
| Final inode-table/root build versus current namespace phase | 0.133 s | ~10.9 s |

These are scope-aware design comparisons, not a controlled speedup claim. They
show why minutes should not be accepted as an intrinsic CAS/CDC cost.

## Optimization direction

### 1. Compile bulk creation into final immutable structures

Reuse the [v0.1.1 construction/admission approach](../0.1.1/architecture_shift.md#v2-bounded-direct-admission-pipeline):
build each new directory and inode in its final form, compute new inode reference
counts once, intern exact portable-metadata tuples in the existing small cache,
and avoid repeatedly constructing intermediate roots. Reuse canonical content
builders over stable Workspace spool/piece readers after quiescence.

Reuse bounded owned slabs, one admission owner and a carried admission batch
across directory boundaries. The historical implementation used 256 KiB / 512
object slabs, a four-slab queue, and a carried 4 MiB / 8,191-object admission
batch. Reuse must fit the current memory budget; changing batch size alone will
not remove the repeated structural work or expensive refresh.

Adapt the existing safe candidate-admission path for a nonempty Store. Do not
invoke the initializer as a substitute for the measured POSIX workload. Its
empty-Store membership assumptions and failure cleanup are not valid for a
Workspace containing old roots. Preserve deduplication, collision checks,
expected-head validation and atomic Commit publication.

### 2. Build survivors for dense deletion; retain the sparse path

This bulk-delete fixture leaves only a 200-file / 1 MiB witness tree. Construct
the final inode table from that surviving namespace instead of applying 100,000
individual removals. Reuse the existing compact-pair and bulk inode builders,
preserving surviving inode IDs, metadata, content roots and all surviving aliases.

This makes the Commit work depend mainly on survivors for this shape. It does
not bypass the actual POSIX traversal, unlinks or directory removals in Exec.
Select the dense path from real workload/state properties and a bounded plan;
keep incremental processing for a small deletion in a large surviving tree.
Do not hard-code the benchmark's witness or replace the workload with root reset.

Deletion creates a new filesystem root referenced by a new Commit with its
parent-history link. Old immutable roots and their CAS chunks remain intact.
Namespace inode reference counts are not global CAS reclamation counts, and
physical chunk garbage collection is outside this operation.

### 3. Reuse the committed result when refreshing the live Workspace

Avoid resolving every materialized path from the new root after candidate
construction just produced its final inode identities and metadata. Reuse those
results and existing bounded authenticated multi-inode reads where required.
Preserve stable live NodeIds, binding/attribute checks, hard-link identity,
open handles and open-unlinked spool lifetime. Do not eagerly walk untouched
siblings or weaken integrity checks to improve the time.

### 4. Optimize the shared FUSE metadata and traversal paths

The lifecycle targets also require Exec improvements. Investigate redundant
mode-setting transport while retaining POSIX calls and their semantics; actual
mtime changes remain required. Improve repeated directory-binding resolution
inside existing unlink batches, use the existing directory-emptiness check for
rmdir, and reuse bounded page/inode reads in the sparse deletion fallback.
The existing snapshot cache may benefit from admitting hot structural pages
with simple eviction within its current total memory ceiling.

Creation and unlink batching already exist. Improve these shared paths before
adding another API or framework. Metadata visibility, operation ordering and
error reporting must remain correct; merely delaying errors is not a speedup.

## Targets to investigate

| Scope | Architectural objective | More aggressive direction |
| --- | --- | --- |
| 100k-file create Commit | 3–5 s | Approach the initializer's construction efficiency under the actual Workspace constraints |
| 100k-file complete create lifecycle | 10–15 s aggressive objective | Investigate single-digit seconds; attainability is not established |
| 100k-file dense-delete Commit | ≤1 s | 0.25–0.5 s |
| 100k-file complete delete lifecycle | 5–10 s | 3–5 s |

The earlier **20–30-second complete-create figure was an intermediate milestone,
not an expected minimum, final ceiling, or prediction**. The earlier 120-second
create / 30-second delete proposals were incremental milestones and are not
the intended optimization ambition. No numerical goal above is a measured
improvement or a replacement for the existing frozen safety/resource gates.

A fast Commit alone cannot achieve the lifecycle goals: unchanged creation
Exec would still take ~95 seconds, and unchanged deletion Exec ~25 seconds.
The 2.7-second reference motivates a better engine but does not establish the
achievable live POSIX total. Keep the current resource limits and report any
internal producer concurrency explicitly when making comparisons.

## Constraints and eventual validation

Reuse existing infrastructure, 1/10/100/500 tiers, authentic SDK/FUSE routes and
the ≤500 MiB per-file / <1 GiB aggregate workload bounds. Keep old roots readable,
preserve the witness and external aliases, and prove the required canonical
representation as well as visible contents. The bulk inode builder retains an
`InsertNode` tree before emission: an iterator alone does not prove its memory
use fits; account for tree capacity, records, slabs and admission buffers.

When implementation is authorized, start with dense-delete and bulk-create
construction, then live refresh and shared Exec costs. Use one selected
case/seed and one focused regression per changed mechanism. Collect affected
three-seed performance after the candidate is stable, then run required
verification separately. Reuse compatible inputs and unaffected valid evidence;
do not repeatedly rerun passing checks or silently change workload semantics.
