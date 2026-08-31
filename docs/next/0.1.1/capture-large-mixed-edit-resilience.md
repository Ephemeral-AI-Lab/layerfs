# Large and mixed-edit capture resilience

> **Status:** Proposal
>
> Target: LayerFS 0.1.1
>
> This document is not part of the LayerFS 0.1.0 contract.

## Question

Can the existing Workspace capture and Commit path retain its current small-edit
latency while remaining bounded for large repositories, large-file replacement,
many fragmented edits, sparse growth, and mixtures of small and medium files?

This proposal is limited to compatibility-preserving internal improvements.
It does not authorize a Store-schema, canonical-format, identity, CDC, SDK, CLI,
or daemon-protocol change.

## 0.1.0 baseline to preserve

The released path already provides:

- online FUSE write capture;
- a bounded Workspace spool;
- persistent file extents and namespace trees;
- missing-only canonical admission;
- explicit pause, quiesce, capture, Commit, rebase, and resume phases;
- fresh-process execution;
- passive per-phase receipts;
- strong small-edit and count-changing-edit benchmark results.

The proposal must preserve those properties and their failure semantics.

## Candidate defects to measure

### Repository-wide planning

Determine whether candidate planning visits nodes unrelated to the changed
frontier. The proof must distinguish changed-frontier work from complete-tree
enumeration rather than infer behavior from total Commit time.

### Fragmented dirty ranges

Measure CPU, allocations, and comparison work as dirty-range count grows. The
accepted implementation must remain linear or near-linear in changed-range
metadata and must not repeatedly scan an expanding prefix.

### Fully replaced files

Determine whether a complete replacement reads or authenticates old payload
objects that cannot affect the result. Avoiding unnecessary reads is valid only
when closure, file length, and final canonical identity remain fully proved.

### Sparse growth and zero-filled regions

Bound work by explicit changed metadata and generated canonical structure. A
large logical zero region must not cause unbounded resident memory or an
unbounded temporary byte vector.

### Mixed file populations

Exercise deterministic mixtures of:

- many tiny edits;
- many new small files;
- medium sequential replacements;
- large localized edits;
- renames and directory changes;
- hard-link and inode metadata changes where supported.

### Long-lived Workspace state

Verify that dirty-range metadata, open-node state, retained output, and proxy
metrics remain bounded across many Commit/rebase cycles.

## Required evidence

The focused matrix should include at least:

| Case | Required observation |
| --- | --- |
| One 10-byte overwrite in a large file | work tracks changed frontier |
| Thousands of disjoint small overwrites | no quadratic range processing |
| Complete large-file replacement | no unnecessary old-payload reads |
| Large sparse extension | bounded resident memory |
| Many small files plus one large file | bounded mixed-workload capture |
| Repeated Commit/rebase cycles | no monotonic Workspace metadata growth |
| Rename and inode metadata changes | correct namespace and inode publication |

For each case record:

- changed files and changed bytes;
- capture, candidate planning, content, namespace, admission, publication, and
  rebase times;
- canonical candidate, inserted, and reused bytes;
- object and transaction batch maxima;
- peak RSS and swaps;
- Store-visible root and reopening proof.

## Patch-release boundary

The following are eligible for 0.1.1 when proved:

- internal frontier indexes;
- more direct use of already captured ranges;
- deletion of redundant old-payload reads;
- bounded data-structure changes;
- additional receipts and tests;
- internal batching that preserves visibility and error behavior.

The following require a separate compatibility decision and normally target
0.2.0:

- canonical byte changes;
- new identity domains;
- Store-schema changes;
- incompatible daemon frames;
- breaking public SDK changes;
- changed Commit visibility semantics.

## Exit condition

The proposal is complete only when the mixed-workload matrix passes with exact
filesystem results, bounded memory, no unreachable canonical objects, and no
regression in the released benchmark rows.
