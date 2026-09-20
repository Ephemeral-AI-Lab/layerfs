# Pair 2, semantic half — history

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
>
> **INITIALIZED — decisions open.** This document records the coupling, what is
> already settled with its evidence, and every open decision. The design itself is
> **not written**. Issue: [#180](https://github.com/Ephemeral-AI-Lab/layerfs/issues/180).
>
> **Implementation review status (2026-09-21):** C5 history exists in
> `core/crates/layerfs-history/` through the service, but independent review at
> `92e56635ae4559d175fe3cd455f36f9fe6b5b498` found it **not ready**.
> The [remediation specification](commit-history/remediation-20260921.md) records
> the fixes and acceptance gates. The governing
> [implementation specification](https://github.com/Ephemeral-AI-Lab/layerfs/blob/c85cf6b69b3809d860caaad764a09e86a54ece9a/core/docs/architecture/proposal/commit-history/implementation.md)
> and [pre-publication audit](https://github.com/Ephemeral-AI-Lab/layerfs/blob/c85cf6b69b3809d860caaad764a09e86a54ece9a/core/docs/architecture/proposal/commit-history/review-20260921.md)
> are pinned on unmerged PR #211. [Paper 16](../16-history.md) describes the
> implementation; it does not override that specification or waive review findings.

Operational half: [`02-init-commit-and-concurrency.md`](02-init-commit-and-concurrency.md) — the five phases, every DB operation, and the concurrency cases.
Folder index and conventions: [`README.md`](README.md).

**Implementation order (owner direction, 2026-09-20): third, after pair 3's
service/transport and pair 1's Workspace/FUSE integration.** See the
[execution sequence](README.md#implementation-order-pair-3-then-pair-1-then-pair-2).
Provide the minimal identity/allocation and acknowledgement definitions needed
by earlier steps as design input; full history implementation follows them.
Their saved-root result is not a logical Commit. This pair adds staging, Commit
and conditional publication through the established service, after tenancy and
history metadata/transaction placement are explicit.

---

## 1. Why this is a co-design pair

```text
   02-init-commit-and-concurrency            THIS DOCUMENT
   ─────────────────────────────             ─────────────
   describes the OPERATIONS                  defines the SEMANTICS
    ① prepare → ② commit point                what a commit IS
    → ③ publish → ④ stage → ⑤ merge          the parent chain
                                              what a layer IS
                                              discard
                                              scope_allocator
```

**The stage/merge boundary IS the history interface.** The operational document
already specifies the CAS precisely — `UPDATE layer_stacks SET head_layer_id = ?2
WHERE layer_stack_id = ?1 AND head_layer_id = ?3` — and cannot say what the thing
being CASed *means*. Change the meaning of a stage, durable versus advisory, and
the stage step changes.

**Empirically supported.** The reference keeps both in **one crate**,
`layerfs-layerstack-store` — 18,376 production lines, its largest — holding the
CAS, packs and SQLite *and* `commits` / `branches` / `layers` / `layer_stacks` /
`workspace_stages`. History was never operationally separable from the store. Core
has C2 without it, which is exactly why the operational document has a hole at ④/⑤.

## 2. What it produces

| Output | Feeds |
| --- | --- |
| commit and layer identity, and what `parent` implies | `02` §3 phase ⑤ |
| the stage: durable or advisory, and what `discard` does | `02` §3 phase ④ |
| inode serial allocation — **C1 refuses to own this** | `02` §3, and zero-copy fork |
| the schema: core has **four** tables, the reference has **eight** plus the allocator | `02` §4 |

## 3. Settled — holds today

| Fact | Evidence |
| --- | --- |
| **Objects immutable, packs append-only** | there is **no `UPDATE objects`**; the only two UPDATEs in the storage layer are the pack append and the watermark |
| **The merge is O(1)** | 1 SELECT · 1 INSERT whose `root_id` IS the commit's `root_id` reused verbatim · 1 CAS. Zero objects created, read or copied |
| **A stale merge is refused, not queued** | applying a commit built against `L0` after the head moved to `L1` promotes a tree lacking `L1`'s changes; a queue that applies in arrival order silently **reverts** |
| **The queue belongs in the runtime** | its unit of work is *rebase then merge*, and rebasing needs the caller's edit stream, which the store does not hold |
| **Commits to different branches never contend** | every branch-level structure is a different row; the CAS target is per-branch |
| **Divergent edits cannot conflict** | different bytes → different ids → different rows; confirmed by the absence of any in-place modification |

## 4. Open — every decision this pair must make

- [ ] **How a workspace and a branch relate.** `workspace_stages` holds a
  `branch_id`, so a workspace targets one branch. May several workspaces stage to
  the same branch, and if so what does the merge CAS mean for the second one?
- [ ] Is a commit **durable on write**? Whether the chain is linear, and whether a
  commit is immutable once written. What does `parent_commit_id` guarantee?
- [ ] **Commit versus layer.** Why both exist, how a layer relates to a commit, and
  what the layer-stack ordering promises.
- [ ] **Is a stage durable on write?** A `workspace_stages` row is an ordinary row.
  Whether it survives a restart, and whether a discarded workspace deletes it,
  changes the commit step.
- [ ] **Discard semantics.** Delete the row only, or account for the objects the
  workspace produced? There is **no GC**, so "discard" currently means
  "unreferenced bytes accumulate".
- [ ] **`scope_allocator`.** Inode serials are allocated by the caller and C1 only
  validates range. Who allocates, is an allocation durable, does an exposed serial
  survive a discarded workspace, and what makes serials unique across a store
  holding many projects? **Zero-copy fork depends on this.**
- [ ] **The schema.** Any of the six missing tables is a schema version bump, with
  the open/validate consequences the existing `user_version` history sets out.
- [ ] **Whether this is a port or a redesign.** If the reference's model is being
  carried over, this document becomes descriptive rather than a proposal — a much
  smaller job. **Settle this before writing §5.**

## 5. Boundary

- **No diff or conflict resolution.** Deferred to
  [#164](https://github.com/Ephemeral-AI-Lab/layerfs/issues/164) by owner decision
  (2026-09-16). Until it lands, a rebase resolves overlapping edits **silently** — a
  known semantic gap to document, not a bug to fix here.
- **No GC.** Reclamation is out of scope; this document states the accumulation, it
  does not solve it.
- **`init_namespace` keeps its multi-worker exception** and its 2.7 s cold Init
  target. It creates history rather than staging into it.
- **Concurrency prerequisites are separate.** Insert-or-reuse, private packs and
  per-save publication are registered under
  [#177](https://github.com/Ephemeral-AI-Lab/layerfs/issues/177) and
  [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178), not here.

## 6. Non-goals

- The projection and accumulator — [`01-projection-and-runtime.md`](01-projection-and-runtime.md), [#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179).
- Transport, authorization, tenancy — [`04-boundary-and-trust.md`](04-boundary-and-trust.md), [#181](https://github.com/Ephemeral-AI-Lab/layerfs/issues/181).
