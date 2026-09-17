# Replacement-core proposals

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Design proposals for the `core/` product. These are **not** descriptions of
shipped behaviour and must not be read as such — the descriptive set is one level
up in [`core/docs/architecture/`](../README.md).

## Why a separate folder

The parent set is source-backed description: every claim is read from code at a
recorded pin. A proposal is a different kind of document — it states what *should*
be built, and mixing the two is wrong because a proposal read as a description
becomes an unverified claim.

So proposals live here, and each one labels its claims:

| Label | Meaning |
| --- | --- |
| **holds today** | read from source; true of the current tree |
| **proposed** | does not exist; a design to be argued with |
| **open — required** | a prerequisite for something else here |
| **deferred** | explicitly out of scope, with the issue that owns it |

## The three co-design pairs

A pair exists where **neither area's interface is complete without the other's
decision** — if one contract stays correct after the other changes, they are not a
pair. Three pass that test; the rest of what has been discussed does not.

```text
╔═ PAIR 1 · PROJECTION & RUNTIME ════════════════════════════════════════════╗
║                                                                            ║
║   ④ FUSE  ◄──────── co-designed ────────►  ① WORKSPACE / RUNTIME           ║
║                                                                            ║
║   the callback set determines       the accumulator's bound determines     ║
║   what the accumulator must hold    where flush points go                  ║
║                                                                            ║
║   COUPLING: change the callback granularity (per-call vs batched) and the  ║
║   accumulator changes. Neither contract is writable alone.                 ║
║   PRODUCES:  the FUSE op set · overlay ceiling · flush policy ·            ║
║              where mutable state lives · one mount or N                    ║
╚════════════════════════════════════════════════════════════════════════════╝

╔═ PAIR 2 · COMMIT & HISTORY ════════════════════════════════════════════════╗
║                                                                            ║
║   01-init-commit  ◄──── co-designed ────►  C5 HISTORY                      ║
║                                                                            ║
║   describes the OPERATIONS            defines the SEMANTICS                ║
║   prepare → commit → publish          what a commit IS · the chain ·       ║
║   → stage → merge                     what a layer IS · discard ·          ║
║                                       scope_allocator                      ║
║                                                                            ║
║   COUPLING: the stage/merge boundary IS the history interface. The doc     ║
║   already specifies the CAS; history specifies what the thing CASed means. ║
║   PRODUCES:  commit/layer identity · parent chain · discard semantics ·    ║
║              inode serial allocation · whether a stage is durable          ║
╚════════════════════════════════════════════════════════════════════════════╝

╔═ PAIR 3 · BOUNDARY & TRUST ════════════════════════════════════════════════╗
║                                                                            ║
║   ② LINK  ◄──────── co-designed ────────►  AUTHORIZATION  ◄──► TENANCY     ║
║                                                                            ║
║   frame format                      per-connection ∣ root→principal ∣      ║
║                                     capability token                       ║
║                                                                            ║
║   COUPLING: the auth choice determines BOTH what the frame carries (a      ║
║   token?) AND what the owner must store (a root→principal map). And the    ║
║   tenant count decides whether the owner needs that state at all.          ║
║   PRODUCES:  frame header · auth mechanism · whether the owner stays       ║
║              workspace-agnostic · one store or many                        ║
╚════════════════════════════════════════════════════════════════════════════╝
```

### Why this order is strict

```text
   PAIR 1  defines the OPERATION SET          #179
              │
              ▼
   PAIR 2  defines what the operations MEAN   #180
              │
              ▼
   PAIR 3  the link CARRIES both, under auth  #181
```

The link carries what pairs 1 and 2 define. Writing history first would invent an
interface the projection cannot use — the mistake of building a layer whose
requirements were guessed.

### One cross-cutting warning

**The tenancy decision inside pair 3 can invalidate pair 2's schema.** Per-tenant
policy rows are not in the four tables core has today. So tenancy must be
**answered directionally before pair 2 freezes a schema**, even though pair 3 is
designed last. Schema changes are the expensive kind.

## Contents

| Document | Pair | State |
| --- | --- | --- |
| [`01-projection-and-runtime.md`](01-projection-and-runtime.md) | 1 | **initialized** — decisions open |
| [`02-init-commit-and-concurrency.md`](02-init-commit-and-concurrency.md) | 2 | written — the operational half |
| [`03-history.md`](03-history.md) | 2 | **initialized** — decisions open |
| [`04-boundary-and-trust.md`](04-boundary-and-trust.md) | 3 | **initialized** — decisions open |

**Not a pair, and deliberately absent:** storage placement. Where the owner's index
and packs live is owner-internal and invisible to every consumer, so it neither
constrains nor is constrained by the three pairs.

## Conventions

- Every proposal records the source pin it was written against and marks which
  parts are read from source.
- **No proposal in this folder is measured.** Claims about behaviour under
  concurrency are structural arguments from the current code, not test results.
- A proposal that is implemented is folded into the descriptive set, and the entry
  here becomes a pointer — it is never silently re-dated.
- Where a premise has been measured and disproven, the reasoning stays visible with
  the disproof recorded beside it. A study that quietly drops a dead premise
  teaches nothing.

## Keeping this current

A change under `crates/*/src` or `crates/*/sql` that invalidates a proposal's
reading of the code updates the proposal in the same commit. A proposal whose
premise no longer holds is **withdrawn explicitly**, with the reason recorded,
rather than left to rot.
