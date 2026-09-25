# #245 scalable ordinary Workspace COW: implementation plan

> **Status:** Current planning checklist; no release candidate exists.

Tracking: [#245](https://github.com/Ephemeral-AI-Lab/layerfs/issues/245), following
[#243](https://github.com/Ephemeral-AI-Lab/layerfs/issues/243) and
[#232](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232). This plan is
based on the #243 ordinary-shell source and retained Phase 2 report at
`7f07b5dadd9bee33f431a243345ce40ed214bb1a`. It proposes work; it
does not authorize a performance claim or alter historical evidence. The
public mutation route remains `WorkspaceApi::exec(command)` →
`/bin/sh -c` → ordinary POSIX/FUSE operations, followed by explicit Commit.
The [architecture](ARCHITECTURE.md) defines the target mechanics,
[Phase 1 verification](VERIFICATION_PHASE1.md) owns the first proof, and
[load-bearing cases](LOAD_BEARING_CASES.md) are deferred exploration.

## Objective and boundaries

Repair the observed package-refresh rename failure, repeated full-piece-list
work, and silent large-shift Exec outcome on the existing generic route. Keep
small positional writes correct. Make the file index and Commit representation
capable of larger workloads by construction, while retaining real file-size,
private-backing, memory, and complete-command budgets. The later many-package
namespace scale is a separate gate: the current generation frontier admits at
most 128 dirty identities and 128 changed names through a 32 KiB metadata
request. A file-index fix alone cannot qualify a large package install.

No shell-text classifier, cooperating edit tool, FUSE range ioctl, new product
entrypoint, extra construction worker, increased deadline, or raised quota is a
solution. The historical ioctl receipts remain opt-in evidence only. A shell
command that shifts a suffix still sends those bytes through FUSE.

## Work packages and stop conditions

| Package | Product and evidence work | Gate before advancing |
| --- | --- | --- |
| A. Localize the recorded failures | Preserve #243 and #232 receipts. Add a labelled mounted diagnostic that records the exact `WorkspaceError` and stage behind root lockfile rename `EIO`; compare root and nested fresh-temp-over-canonical replacement. Diagnose the five-second silent Exec `Unknown` from in-flight callback/progress counts, and the separate ~5.5 s noncommitting cleanup from close state. | Root cause identified from a real mounted route; no failure is reclassified as a capacity observation or a latency PASS. |
| B. Repair shared correctness | Fix the common rename/namespace path at its cause; preserve open handles, replaced-file identity, tombstones, old Commit and private-state failure behavior. Address Exec progress where actual work is occurring, without simply increasing the deadline. Keep any cleanup fix separate from Exec and Commit. | Exact #243 package command exits zero, commits, and passes an independent complete-tree oracle; failed-command/no-Commit still leaves the published head unchanged. |
| C. Make ordinary file writes local | Replace the fixed two-level, absolute-offset piece representation with a dynamic-height, multiway B+ tree-style sequence indexed by subtree byte lengths, or an equivalent proven shift-safe scheme. Pack extents in leaves; update only boundary leaves and copied paths; merge compatible neighbors. Use the existing Base/Local/Zero meaning, payload custody, page ownership and atomic root publication. Cursor reads and traversal must not materialize all pieces. | Focused write/resize/read tests cover overlaps, holes, append, truncate, piece boundaries, old generations and failure rollback. Count diagnostics demonstrate that fixed-size writes no longer rebuild all P pieces/pages. |
| D. Stream file Commit end to end | Coordinate Workspace lowering, Bridge request(s), server save and C1 content construction. A frozen piece cursor emits ordered base-retain and replacement spans through bounded buffers. Remove the 1,024-piece, 256-edit and 8 MiB replay ceilings only with this coherent transport and content change; do not move the same ceiling to the server. Preserve one final Branch-head publication, retained old Commit, definite/unknown outcome handling, and bounded private residency. | Large replacement and fresh streams complete with exact final bytes, no unbounded `Vec<Vec<u8>>`, correct custody and cleanup. The 4 GiB logical file ceiling and actual resource budgets remain explicit. |
| E. Prove Commit generation overlap | Capture the immutable local root with a short metadata boundary, route later writes into the successor, and reconcile a successful Store Commit even if the active revision advances. G2 must see G1 plus later writes while B1 is pending; a second sequential Commit freezes G2 and allows G3 writes. Current reconciliation can return `Busy` after a concurrent write; continuous successful Commit is a required invariant to prove, not an existing PASS. | Mounted test observes pre-capture bytes in B1, post-capture bytes in live G2, then exact B2 and live G3 results. Old heads remain readable; no lost/duplicated mutations, unsupported overlap of two Commit calls, or unexplained success/failure result. |
| F. Freeze and run Phase 1 proof | Use the [Phase 1 plan](VERIFICATION_PHASE1.md) to freeze changed-source identities, exact commands, resource/cache contract, full receipts, and independent verifier before collecting. Reuse historical receipts as diagnostic evidence, not a numeric matched arm. | Every registered cell is reported PASS, FAIL, INELIGIBLE or NOT_RUN; no omission or post-hoc target change. |

Packages A and B can finish before the representation change. C–E need a
reviewed joint format/protocol design before product edits. Where a new on-disk
format, wire contract, algorithm or named bound changes, update the affected
`core/docs/architecture/` document in the same source commit. Keep each
production commit small enough to identify its cause and record production LOC
before, after and signed delta per repository policy.

## Source ownership and invariants

| Concern | Owning source and required invariant |
| --- | --- |
| Public route and mounted callbacks | [SDK Workspace](../../../crates/layerfs-api/sdk/src/workspace.rs), [daemon Exec](../../../crates/layerfs-daemon/src/execution.rs), [FUSE adapter](../../../crates/layerfs-fuse/src/adapter.rs): preserve opaque commands and real POSIX callback order; count exact callbacks and accepted bytes. |
| Namespace correctness | [rename](../../../crates/layerfs-workspace/src/filesystem/rename.rs), [create](../../../crates/layerfs-workspace/src/filesystem/create.rs), [remove](../../../crates/layerfs-workspace/src/filesystem/remove.rs): one visible binding/tombstone result, correct replaced-inode ownership and old history. |
| File COW index | [write](../../../crates/layerfs-workspace/src/filesystem/write.rs), [pieces](../../../crates/layerfs-workspace/src/overlay/pieces.rs), [metadata index](../../../crates/layerfs-workspace/src/backing/metadata_index.rs): checked offsets and lengths, path-local publication, immutable older roots, bounded page and payload references. An absolute-offset path-copy tree alone would rewrite suffix keys for a structural splice. |
| Capture, cleanup and concurrency | [capture](../../../crates/layerfs-workspace/src/overlay/snapshot.rs), [reconciliation](../../../crates/layerfs-workspace/src/commit/reconcile.rs), [page reclaim](../../../crates/layerfs-workspace/src/backing/metadata_reclaim.rs): preserve a frozen root until resolved; do not discard uncertain outcomes or live handles. |
| File and namespace Commit | [lowering](../../../crates/layerfs-workspace/src/commit/lower.rs), [save](../../../crates/layerfs-workspace/src/commit/save.rs), [Bridge request](../../../crates/layerfs-bridge/src/contract/request.rs), [server content save](../../../crates/layerfs-server/src/service/save/content.rs): bounded streaming, canonical content reuse, one History publication. Later namespace batching must address [frontier bounds](../../../crates/layerfs-workspace/src/runtime/state.rs) and Bridge metadata transport rather than silently relaxing validation. |

Do not add unneeded indirection or a second file editor. The existing page
arena and ownership model are the starting point; any replacement must show
why they cannot support the required cursor/path operations.

## Measurement and release discipline

The first performance question is whether a fixed localized WRITE visits and
rewrites work proportional to affected pieces and tree height rather than
all P pieces. Commit may traverse final pieces once and pays for the actual
bytes a shell command wrote; there is no whole-command O(1) insert promise.
Record operation-specific FUSE READ/WRITE/SETATTR/CREATE/RENAME/UNLINK counts and
bytes, piece visits/pages/height, private and Store bytes, CPU/RSS with scope,
Exec/Commit/cleanup substeps, and the exact failure stage.

Follow [benchmark rules](../../../../docs/general/benchmark_rules.md),
[repository rules](../../../../AGENTS.md), [Core rules](../../../AGENTS.md)
and [Core benchmark rules](../../../benchmark/fs-bench-pro/AGENTS.md). Freeze a
new ordinary-shell selection before any new sample; one attempt per case and
arm, one construction worker, fresh append-only outputs, equal enforced cache
state, and a separate full-file/history verifier. The complete command is
normally ≤15 s, with only prospectively declared small exceptions up to 25 s;
verification remains under 10 s. Cold-ineligible rows may guide mechanism
work but cannot support a latency PASS or speedup headline. Do not rerun an
unchanged arm, raise deadlines/workers, warm a timed phase, or relabel the
historical ioctl route.

The later [load-bearing cases](LOAD_BEARING_CASES.md) test namespace, output,
large-file and multi-generation limits under distinct prospective contracts.
They do not expand Phase 1 retroactively or close #232's 56-shape gate.
