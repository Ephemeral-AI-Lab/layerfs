# Phase 4.5 follow-up: path-local C1 directory-move validation

> **Status:** Research; informative and not a product contract.

Source inspected: `c51b5c982f186c7ab078def81cea0401f3f6d5f2` on
`codex/issue264-phase45` (2026-09-27). This is a proposed follow-up to
[#264](https://github.com/Ephemeral-AI-Lab/layerfs/issues/264), whose mounted
implementation and focused proof are recorded in the
[Phase 4.5 checkpoint](PHASE4_5_IMPLEMENTATION.md). No C1 format change,
path-local Commit result, latency result, or release admission is claimed here.

## Remaining operation and proposed boundary

An ordinary inherited directory move now edits a constant number of private
Workspace bindings without visiting its descendants. Its subsequent explicit
Commit has a different cost. For a stored directory or symlink rebound by a
prepared update, C1's [`check_parent_aliases`](../../../crates/layerfs-content/src/filesystem/validate.rs)
walks the canonical base to discover an old binding, while
`check_effective_cycles` examines the moved effective subtree. The current
[filesystem root](../../../crates/layerfs-content/src/filesystem/root.rs)
authenticates an inode table but has no unique-parent index. These checks
protect real single-parent and acyclic-tree invariants; removing either scan
without another proof is incorrect.

Let `B` be base bindings, `N` effective bindings in the moved subtree, `K`
changed bindings in the final prepared batch, `D` distinct candidate ancestor
links reached from changed non-file edges, `h` canonical index height, and
`M`/`S` directory/symlink identities. For a pure stored-directory move, the
current validation examines at least `Ω(B + N)` bindings; indexed page work
also depends on `h`. This is separate from Phase 4.5's mounted rename bound.

| Proposed structure | Purpose and validation rule |
| --- | --- |
| Versioned canonical root with an authenticated serial-keyed owner index for directories and symlinks | Give each live non-file serial its one `(parent serial, component)` owner; the root directory has none. Regular files retain legal multiple hard-link names. Publish the index and inode table in one canonical root. |
| Final-state candidate batch | Apply removals and additions to a candidate forward binding view and owner-index view before validation. Reject two final owners, an orphaned required inode, or disagreement between the two views. Check deleted/replaced parents under the same final-state rules. |
| Changed-edge cycle proof | Follow final candidate parent links from each changed non-file edge, with visited/color or memoized state. A cycle newly introduced into a valid base must contain a changed edge. Authenticate each indexed lookup; reject an invalid or untrusted base rather than assuming its index is correct. |

For a valid indexed base and a pure move, the target is `O((K + D)h)` indexed
validation work, `O(Kh)` new canonical index pages, and `O(K + D)` transient
candidate state. The persistent owner index costs
`O(M + S + Σ owner-component bytes)` alongside the existing canonical tree.
Actual deletion of a subtree may still need work proportional to the released
subtree. These are asymptotic targets, not an elapsed-time estimate: extra index
I/O can make small trees slower.

The existing v1 root is an exact 116-byte grammar and its namespace profile is
pinned by History. A v2 index cannot be attached silently to a v1 root. The
implementation design must specify canonical bytes, references, profile and
History compatibility, old-head reads, and a deterministic choice between new
v2 stacks and an explicit, charged v1 migration. Any migration first validates
the old tree and costs at least one full traversal; an unindexed v1 head keeps
the old scan until migration. A v2 root has a different canonical identity even
when its visible namespace matches v1. No CDC or C2 payload-delta change is
needed for this namespace validation proposal.

## Evidence required before a complexity claim

[`FilesystemResult.counters`](../../../crates/layerfs-content/src/filesystem/update.rs)
already carries C1 validation totals. The Service currently drops them, and
the Workspace's successful Commit report does not retain the submission's
`saved_files` count. First carry these existing facts to a bounded diagnostic:

- Report directory and inode page visits, entries examined, and read waves
  separately for alias and cycle validation; reconcile site counts with the
  existing C1 totals. Label them as C1 update visits, not physical disk reads.
- Report `saved_files` before the successful submission is retired. A pure
  directory move should save zero file payloads; a mixed temp-file replacement
  does not have that expectation.
- Key one immutable diagnostic record to the source/build, base and candidate
  roots, Workspace incarnation, branch/generation, and stage token. Missing,
  duplicate, or mismatched records make a comparison `INCOMPLETE`.
- Compare a small and large base, and small and large inherited moved subtrees,
  with the same final changed bindings. Report C1 visits, changed canonical
  pages/bytes, `saved_files`, memory and cleanup for both. A count diagnostic is
  not a cold-cache speed sample. Any latency claim needs a prospectively
  registered public `WorkspaceApi::exec` plus explicit Commit selection under
  the [benchmark rules](../../../../docs/general/benchmark_rules.md).

## Correctness and documentation gates

Focused tests must cover a pure inherited directory move; nested move and
move-back; changed edges that form a cycle; same-batch removal and rebind;
duplicate directory/symlink owners; a file with multiple hard links; deletion
and replacement; old-head readability; and atomic refusal before publication.
One public Exec followed by one explicit Commit must verify new and old trees
with an independent identity-based oracle. Reuse Phase 4.5's frozen-generation
and known/unknown outcome custody checks. Do not substitute a small-tree
functional pass for the C1 visit comparison.

When source changes land, update the source-pinned
[filesystem architecture](../../architecture/04-filesystem.md),
[limits](../../architecture/06-limits.md), and
[counter semantics](../../architecture/10-counters.md) in the same commit as
their changed format, algorithm or bound, per
[`core/AGENTS.md`](../../../AGENTS.md). Record the v1/v2 compatibility decision,
raw count receipts, exact checks and gaps in a dated implementation checkpoint;
update the versioned manual and release evidence only if a release is proposed.
The [documentation policy](../../../../docs/general/documentation-policy.md)
requires status, source and working links.

Direct whole-path Inspect, source import and their pathname limits are
independent paths. [#256](https://github.com/Ephemeral-AI-Lab/layerfs/issues/256)
owns broad namespace cardinality and live-handle/resource-ceiling work;
[#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248) owns final
file-delta scaling. The private page level/slot and local directory-count
encodings require their own format audit. This follow-up does not claim those
limits are removed.
