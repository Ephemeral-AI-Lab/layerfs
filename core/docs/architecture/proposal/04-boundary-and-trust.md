# Pair 3 — boundary and trust

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
>
> **INITIALIZED — decisions open.** This document records the coupling, what is
> already settled with its evidence, and every open decision. The design itself is
> **not written**. Issue: [#181](https://github.com/Ephemeral-AI-Lab/layerfs/issues/181).

Sibling pairs: [`01-projection-and-runtime.md`](01-projection-and-runtime.md) · [`03-history.md`](03-history.md).
Folder index and conventions: [`README.md`](README.md).

---

## 1. Why this is a co-design pair

```text
   ② LINK  ◄──── co-designed ────►  AUTHORIZATION  ◄────►  TENANCY
   ───────                          ─────────────            ───────
   what the frame carries           who may read root R      how many trust
   (does it carry a token?)         who may write objects    domains share
                                                             one owner
                  │                          │                      │
                  └──────────► what the OWNER MUST STORE ◄─────────┘
```

The authorization choice determines **both** what the frame carries (a capability
token?) **and** what the owner must hold (a `root → principal` map). The tenant
count decides whether the owner needs that state at all.

### The tension to resolve

```text
   A WORKSPACE-AGNOSTIC OWNER
        serves "read root R" and "store these objects"
        holds NO per-workspace state
        ⇒ that is what makes N consumers × M workspaces cheap  (pair 1)

   … CANNOT AUTHORIZE PER WORKSPACE
        it does not know which workspace a root belongs to
        ⇒ per-workspace authorization forces it to hold state it does not
```

**This is the decision that ends the workspace-agnostic property**, and it should be
made deliberately rather than discovered when multi-tenancy appears.

## 2. What it produces

| Output | Feeds |
| --- | --- |
| the frame format and the bounded-operation protocol | implementation |
| the authorization mechanism, and whether the owner stays workspace-agnostic | pair 1 — the owner's state model |
| the tenant model and the storage topology it implies | **pair 2 — its schema** |
| the deployment matrix expressed as composable pieces | implementation |

## 3. Settled — read from source or already decided

| Decision | Why |
| --- | --- |
| **The owner is a role; a daemon is one realization** | embedded · standalone · remote are three deployments of one owner, not three designs |
| **All three via one pluggable link** | embedding is the degenerate case where the link is a function call |
| **Storage placement is owner-internal** | a consumer can never tell which cell it is in — that invisibility is what keeps the matrix small |
| **Six deployment cells reduce to two links** | direct ∣ byte stream (unix socket or TCP+TLS); the address differs, the code does not |
| **The link carries logical operations, not objects** | splitting at the C1/C2 traits would make one message per canonical object — the failure [#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172) already names |
| **Status codes mirror C1's error taxonomy** | `MissingObject` and `ProviderFailure` must stay distinguishable across the wire for the same reason they are distinct in-process |
| **Content-addressing gives the link its integrity** | a received object is verified by re-deriving its `BLAKE3` id, or transitively by the reuse byte-compare — no transport-level MAC is needed |

## 4. Open — every decision this pair must make

- [ ] **Frame format.** Length-prefixed bounded frames; what the header carries
      beyond the operation kind and the root; whether framing is shared with the
      object-batch path or separate.
- [ ] **Authorization mechanism.** Per-connection (all-or-nothing, zero owner
      state) · `root → principal` map (owner state, O(roots)) · capability tokens
      (stateless owner, needs a signer and key lifecycle).
- [ ] **Tenancy.** Single store with many projects — cross-project dedup, the
      product's value, one writer, one policy — versus store-per-tenant, which
      buys isolation and loses dedup. Note that **single-tenant is the simpler
      option**, not the more complex one; multi-tenant adds routing.
- [ ] **The cutoff constraint.** `small_file_threshold_bytes` is **per-store policy
      and changes canonical roots**, and `Store::open` has no migration path. Two
      projects wanting different cutoffs **cannot share a store** for the same
      content. If multi-tenancy is needed for policy rather than isolation, this is
      why — and it is a correctness constraint, not a preference.
- [ ] **Backpressure and reconnection.** A dropped connection mid-save: the rule is
      "unknown acknowledgement is failure, no resend". A fresh flush of the same
      overlay is safe because content-addressing makes it idempotent — whether that
      counts as a resend needs a ruling, not an assumption.
- [ ] **Whether the owner may be embedded in the shipped product**, or whether a
      daemon is mandatory once FUSE exists.

## 5. Boundary

- **No cloud provider promise.** Cloudflare Durable Objects and D1 use WAL and are
  outside the current policy; radish is architectural inspiration, not a template.
  No provider registry, distributed lease service or speculative cloud package.
- **No durability change.** MEMORY journal, `synchronous = OFF`, no `fsync`, no WAL.
  A transport must not imply otherwise.
- **No new protocol without a concrete need.** The operations already exist and are
  bounded; mapping them onto frames is the job.
- **C1 and C2 import nothing from this pair.** No mount handles, daemon ids,
  transport types or tenancy concepts cross into the product core.

## 6. Non-goals

- The projection and accumulator — [`01-projection-and-runtime.md`](01-projection-and-runtime.md), [#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179).
- Commit, stage and history semantics — [`03-history.md`](03-history.md), [#180](https://github.com/Ephemeral-AI-Lab/layerfs/issues/180).
- Measured acceptance — [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171).
