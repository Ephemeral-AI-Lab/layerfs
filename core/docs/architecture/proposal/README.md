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

### Implementation order: pair 3, then pair 1, then pair 2

Owner direction, 2026-09-20: establish the operation-owner/service and transport
foundation first, then build FUSE/Workspace on that endpoint, then add history.
Pair numbers remain responsibility labels. The implementation order is **#181 →
#179 → #180**, after the reviewed C1/C2 contract and a short initial operation
agreement. This supersedes both the former strict 1 → 2 → 3 sequence and the
earlier recommendation to implement all three pairs in parallel.

```text
   REVIEWED C1/C2 + SHORT INITIAL OPERATION CONTRACT
              │
              ▼
   PAIR 3  operation owner + transport       #181
           prove with a test client, no FUSE required
              │
              ▼
   PAIR 1  Workspace accumulator + FUSE      #179
           use the working service endpoint
              │
              ▼
   PAIR 2  history + stage/Commit +          #180
           conditional head publication
```

Pair 3's immediate target is **a Linux Docker `layerfs-daemon` connected to a
host `layerfs-service`**, initially on the current macOS development host. Its
other priority is an architecture portable to future cloud/serverless SQLite:
keep bridge operations and C2 provider assumptions separate. The
[pair 3 scope and acceptance](04-boundary-and-trust.md) requires source-backed
portability analysis and actual Docker/host execution, not cloud implementation.
Record the exact environment, endpoint and trust scope before implementing it.
Pair 1 later adds FUSE and Workspace capabilities to the daemon.
Keep C1/C2 together near storage and keep their object-provider/consumer calls
local. A direct-call test does not qualify the selected cross-process deployment.

**Design input is not parallel implementation.** Before pair 3 starts, obtain
only the contract input it needs from the other pairs: initial logical reads and
updates, stable input and explicit base roots, bounded request/result streams,
authorization scope, and the meaning of saved-root success versus failure or
unknown outcome. Pair 1's full accumulator and pair 2's full history engine are
not prerequisites for this agreement. New-inode operations require allocator
ownership/nonreuse semantics; an existing-file read/edit path can come first.

| Implementation step | Deliverable required before moving on |
| --- | --- |
| **Pair 3 first** | Portable service/bridge/daemon boundaries and a documented future SQLite-provider path; actual Linux Docker daemon to host service using real C1/C2, authorization, bounded transfer and honest failures. A test driver proves read/update/save/new-root readback without FUSE. |
| **Pair 1 second** | Workspace accumulation and overlay reads, resource/flush policy, handles and real Linux FUSE callbacks using the tested endpoint. Prove read → pending edit/read-your-write → bounded submit → saved-root readback. This is not yet logical Commit/history acceptance. |
| **Pair 2 third** | History identities, staging, logical Commit, discard and conditional head publication composed with the working runtime/service. Add the corresponding protocol operations only after their semantics are defined; qualify the complete lifecycle. |

Pair 3 implements the foundation needed by the initial deployment, not every
future deployment or tenancy mechanism. Its early root-save result means C2
finished saving the objects; it does not create a logical Commit, move a branch
head or promise crash durability. Unknown acknowledgement is failure with unknown
outcome, with no automatic resend or guessed rollback. Later history operation
messages extend the same service under pair 2's explicit contract.

Pair 3 supplies tenancy/identity direction before pair 2 freezes history schema.
Pair 2 must also decide history metadata placement and how it composes with C2
save completion; additional history tables or a shared transaction are not
implicit Store capabilities. These early design dependencies do not move history
implementation ahead of the service and Workspace.

Stage 7 remains the C1/C2 architecture/replacement review, not runtime
implementation. Reconcile its findings with the finalized core baseline and
resolve/disposition those affecting the selected integration contract before
using it. Neither this sequence nor historical Stage 6 results establish runtime
qualification. The earlier [sequencing review](../../../../docs/roadmap/0.1/0.1.7/evidence/co-design-sequencing-20260920/README.md)
is retained as research with the later owner direction appended.

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
