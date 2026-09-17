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
| **open — required** | a prerequisite for something else in the proposal |
| **deferred** | explicitly out of scope, with the issue that owns it |

## Co-design pairs

Three pairs, in design order. A pair exists where **neither area's interface is
complete without the other's decision** — if one contract stays correct after the
other changes, they are not a pair.

```text
   PAIR 1 · PROJECTION & RUNTIME                            #179
     ④ FUSE  ◄── co-designed ──►  ① WORKSPACE / RUNTIME
        the callback set determines what the accumulator holds;
        the accumulator's bound determines where flush points go
                                              │
                                              │ defines the OPERATION SET
                                              ▼
   PAIR 2 · COMMIT & HISTORY                                #180
     02-init-commit  ◄── co-designed ──►  C5 HISTORY
        the operations are specified; the SEMANTICS are not.
        The stage/merge boundary IS the history interface.
                                              │
                                              │ defines what the ops MEAN
                                              ▼
   PAIR 3 · BOUNDARY & TRUST                                #181
     ② LINK  ◄── co-designed ──►  AUTHORIZATION  ◄──►  TENANCY
        the auth choice determines the frame format AND what the owner stores;
        the tenant count decides whether that state exists at all
```

**Why this order is strict:** the link carries what pairs 1 and 2 define. Writing
history first would invent an interface the projection cannot use — the mistake of
building a layer whose requirements were guessed.

**One cross-cutting warning:** the tenancy decision inside pair 3 can invalidate
pair 2's schema, so it must be **answered directionally before pair 2 freezes a
schema**, even though pair 3 is designed last.

## Contents

| Document | Pair | State |
| --- | --- | --- |
| `01-projection-and-runtime.md` | 1 | **not yet written** |
| [`02-init-commit-and-concurrency.md`](02-init-commit-and-concurrency.md) | 2 | written — the operational half: five phases, every DB operation by phase, shared-versus-private state, three concurrency cases with diagrams, the three prerequisite races, why a stale merge is rejected rather than queued, and the races-versus-conflicts boundary |
| `03-history.md` | 2 | **not yet written** — the semantic half: commit/layer/stage identity, the parent chain, discard, `scope_allocator`, and the schema |
| `04-boundary-and-trust.md` | 3 | **not yet written** |

**Not a pair, and deliberately absent from this folder:** storage placement. Where
the owner's index and packs live is owner-internal and invisible to every consumer,
so it neither constrains nor is constrained by the three pairs.

## Conventions

- Every proposal records the source pin it was written against and marks which
  parts are read from source.
- **No proposal in this folder is measured.** Claims about behaviour under
  concurrency are structural arguments from the current code, not test results.
- A proposal that is implemented is folded into the descriptive set, and the entry
  here becomes a pointer — it is never silently re-dated.
- Where a proposal's premise has been measured and disproven, the reasoning stays
  visible with the disproof recorded beside it. A study that quietly drops a dead
  premise teaches nothing.

## Keeping this current

A change under `crates/*/src` or `crates/*/sql` that invalidates a proposal's
reading of the code updates the proposal in the same commit. A proposal whose
premise no longer holds is **withdrawn explicitly**, with the reason recorded,
rather than left to rot.
