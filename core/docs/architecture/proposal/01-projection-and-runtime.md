# Pair 1 — projection and runtime

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
>
> **INITIALIZED — decisions open.** This document records the coupling, what is
> already settled with its evidence, and every open decision. The design itself is
> **not written**. Issue: [#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179).

Sibling pairs: [`03-history.md`](03-history.md) · [`04-boundary-and-trust.md`](04-boundary-and-trust.md).
Folder index and conventions: [`README.md`](README.md).

---

## 1. Why this is a co-design pair

```text
   FUSE's callback set   ──determines──►  what the ACCUMULATOR must hold
   the accumulator bound ──determines──►  where FLUSH points go
   flush points          ──determines──►  when update_filesystem runs
```

Change the callback granularity — per-call versus batched — and the accumulator
changes with it. Neither contract is writable alone.

**The coupling is concrete, not stylistic.** C1's write API is batch-shaped:
`FilesystemInput` takes sorted, unique, **complete** final bindings per changed
directory, and `update_filesystem` refuses anything else with
`NonCanonicalOrdering`. FUSE produces per-call, concurrent, unordered mutations.
**Something must accumulate**, and what it must hold is a function of which
callbacks FUSE delivers.

The read half does not have this problem — `FilesystemRead` is already
path-addressed, bounded and batch-capable, so reads map close to 1:1.

## 2. What it produces

| Output | Feeds |
| --- | --- |
| the FUSE operation set actually served, each mapped to a C1 call | pair 3 — the link carries these operations |
| the overlay's byte ceiling and flush policy | pair 2 — a commit's size distribution |
| where mutable state lives, and what the workspace entity owns | pair 2 — what a commit stages |
| one mount with N workspaces, or N mounts | operations |
| what a caller is told when `commit` returns, given nothing is durable | pair 2 — the acknowledgement contract |

## 3. Settled — read from source or already decided

| Decision | Why |
| --- | --- |
| **Workspace state lives consumer-side** | the owner must scale to N consumers × M workspaces without per-workspace memory; `FilesystemRead` is stateless given a root |
| **The owner stays workspace-agnostic** | it serves "read root R" and "store these objects"; `InodeScope` is a 32-byte value inside a root, not session state |
| **Reads map ~1:1** | `resolve` · `stat` · `list` (with continuation) · `read_range` · `readlink` · `read_portable` · `read_attribute` · `attribute_keys` all exist and are path-addressed |
| **Writes map 1:N** | `write`/`create`/`unlink`/`rename`/`setattr` accumulate; only a completed batch reaches C1 |
| **DB traffic happens at four events** | create · commit (many) · merge · end — never on the FUSE write path |
| **The per-branch serializer is the merge, not the committer** | `workspace_stages` is one row per workspace; only the merge CAS serializes ([`02`](02-init-commit-and-concurrency.md) §9) |

## 4. Open — every decision this pair must make

- [ ] **Overlay byte ceiling.** What bounds unflushed bytes per consumer, and what
      happens on breach: refuse, force a flush, or block the writer.
- [ ] **Flush policy.** Size-triggered, time-triggered, or caller-driven — and
      whether a flush blocks the FUSE callback that triggered it.
- [ ] **What `commit` returns.** Nothing is durable: MEMORY journal,
      `synchronous = OFF`, no `fsync` anywhere. The acknowledgement must not read
      as committed, and must survive connection loss.
- [ ] **One mount with N workspaces, or N mounts.** One mount routes by path prefix
      and costs one kernel session; N mounts give isolation and cost N. The deciding
      question is whether one workspace may block its neighbours — the same question
      the single-writer discussion raised one level down.
- [ ] **Where the accumulator lives.** Inside the FUSE implementation, or in a
      separate runtime layer that a second projection (materialization) could share.
- [ ] **The 4,096-binding ceiling as a product statement.** A directory whose
      effective subtree exceeds it can never be renamed, and the refusal is reported
      as a cycle-check work limit. That is user-visible and must be stated as a
      limitation, not left as an implementation bound.

## 5. Boundary

- **Design first, measure second.** Nothing here is measurable until a mount exists.
  [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) owns measured
  acceptance; [#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172) owns
  the implementation.
- **No canonical change.** Transport and projection must not alter emitted bytes.
- **No durability claim.** A `write()` acknowledgement must not imply durability
  the store does not provide.
- **Do not collapse `init_namespace`.** It is the documented multi-worker exception
  with a 2.7 s cold Init target; it is not "a commit with a bigger input".

## 6. Non-goals

- Commit and history semantics — [`03-history.md`](03-history.md), [#180](https://github.com/Ephemeral-AI-Lab/layerfs/issues/180).
- Transport framing, authorization, tenancy — [`04-boundary-and-trust.md`](04-boundary-and-trust.md), [#181](https://github.com/Ephemeral-AI-Lab/layerfs/issues/181).
- Any measured performance claim — [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171).
