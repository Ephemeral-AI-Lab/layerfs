# Pair 1 — projection and runtime

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
>
> **INITIALIZED — decisions open.** This document records the coupling, what is
> already settled with its evidence, and every open decision. The full mounted
> filesystem design remains open; its proposed transport integration is recorded
> [separately](service-daemon-transport/06-future-fuse-and-cloud.md).
> Issue: [#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179).

Sibling pairs: [`03-history.md`](03-history.md) · [`04-boundary-and-trust.md`](04-boundary-and-trust.md).
Folder index and conventions: [`README.md`](README.md).

**Implementation order (owner direction, 2026-09-20): second, after pair 3's
service/transport foundation, before pair 2's history implementation.** See the
[execution sequence](README.md#implementation-order-pair-3-then-pair-1-then-pair-2).
Supply the initial operation requirements before pair 3 builds its endpoint;
then implement the accumulator and FUSE against that tested endpoint. The early
path saves and reads explicit roots; logical Commit arrives with pair 2.

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
`FilesystemInput` takes sorted, unique final bindings for **each changed name**
in a directory; unchanged names need not be resent. `update_filesystem` rejects
noncanonical ordering. FUSE produces per-call, concurrent, unordered mutations.
**Something must accumulate**, and what it must hold is a function of which
callbacks FUSE delivers.

`FilesystemRead` supplies path-addressed, bounded read operations. That maps
responsibilities, not network requests: Workspace first serves local overlay/cache
hits, and service queries should return the metadata needed together rather than
requiring serialized resolve/stat calls. See the [legacy comparison](service-daemon-transport/05-v0.1.6-comparison.md).

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
| **Logical read APIs exist** | `resolve` · `stat` · `list` (with continuation) · `read_range` · `readlink` · `read_portable` · `read_attribute` · `attribute_keys` are available; their existence does not require one remote call per FUSE callback or per internal API |
| **Writes map 1:N** | `write`/`create`/`unlink`/`rename`/`setattr` accumulate; only a completed batch reaches C1 |
| **Accumulate writes before saving** | Each FUSE write need not start a storage transaction. Base/object reads and a pressure-triggered flush must be accounted for separately; their exact callback behavior is part of this design. |
| **Branch-head publication and storage ownership are separate** | Future history defines conditional head publication; current C2 saves independently obey Store writer ownership. Pair 2 defines the history contract, and pair 1 must qualify read/control progress under actual storage contention. |

## 4. Open — every decision this pair must make

- [ ] **Overlay byte ceiling.** What bounds unflushed bytes per consumer, and what
      happens on breach: refuse, force a flush, or block the writer.
- [ ] **Flush policy.** Size-triggered, time-triggered, or caller-driven — and
      whether a flush blocks the FUSE callback that triggered it.
- [ ] **What each acknowledgement means.** Distinguish accepted overlay bytes,
      completed object save and the later logical Commit/publication result.
      Preserve the no-crash-durability profile; connection loss can leave an
      unknown outcome and must not trigger an automatic resend.
- [ ] **One mount with N workspaces, or N mounts.** One mount routes by path prefix
      and costs one kernel session; N mounts give isolation and cost N. The deciding
      question is whether one workspace may block its neighbours — the same question
      the single-writer discussion raised one level down.
- [ ] **Workspace implementation and limits.** The integration proposal places
      mutable state in a transport-independent Workspace module and makes FUSE a
      thin adapter. Specify its edit lowering, generation/reconciliation and backing
      policy; extract a crate only when a real consumer/build boundary warrants it.
- [ ] **The 4,096-binding ceiling as a product statement.** A directory whose
      effective subtree exceeds it can never be renamed, and the refusal is reported
      as a cycle-check work limit. That is user-visible and must be stated as a
      limitation, not left as an implementation bound.

## 5. Boundary

- **Service first, then projection acceptance.** Pair 3 proves its endpoint with
  a test client before this pair implements FUSE/Workspace. This pair's mounted
  behavior requires actual Linux FUSE evidence. #171's core qualification and
  #172's architecture review do not qualify that integration.
- **No canonical change.** Transport and projection must not alter emitted bytes.
- **No durability claim.** A `write()` acknowledgement must not imply durability
  the store does not provide.
- **Do not collapse `init_namespace`.** It is the documented multi-worker exception
  with a 2.7 s cold Init target; it is not "a commit with a bigger input".

## 6. Non-goals

- Commit and history semantics — [`03-history.md`](03-history.md), [#180](https://github.com/Ephemeral-AI-Lab/layerfs/issues/180).
- Transport framing, authorization, tenancy — [`04-boundary-and-trust.md`](04-boundary-and-trust.md), [#181](https://github.com/Ephemeral-AI-Lab/layerfs/issues/181).
- Performance claims require separate qualification of the implemented runtime;
  [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) covers its core scope only.
