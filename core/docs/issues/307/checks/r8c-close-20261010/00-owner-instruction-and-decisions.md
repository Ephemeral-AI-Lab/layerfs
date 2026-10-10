# R8c/R9c: owner instruction and the decisions taken under it

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Owner instruction, 2026-10-10, after the [R8b](../../R8B-COMPLETION-20261010.md)
and [R9b](../../R9B-COMPLETION-20261010.md) closures and a summary of what
blocked them: **"fix all of them with simplicity and i want to close r8 and r9
fast"**. It supersedes, for the items below only, the earlier limits that kept
them `PENDING OWNER`. Standing rules are unchanged: Disposable/WAL/OFF only,
Durable not run, no third-party change, no retry, no push, 120 s test ceiling,
one attempt per registered selection, every original failure kept.

For each blocker the simplest option that actually removes it was taken. A
**fix** changes product or harness. A **ruling** declares existing behaviour to
be the contract. A **withdrawn scope** is a stage no real mount can produce
without a product hook; it is removed from its row and listed, never reported
as proved. The R8b outcomes stay as written.

| ID | Answers | Kind | Decision | Alternative, if the owner prefers it |
| --- | --- | --- | --- | --- |
| C-1 | OWNER-8 | fix | The Sandbox container denies creation of user namespaces (`unshare` and `clone` with `CLONE_NEWUSER`; `clone3` answers `ENOSYS`). An unprivileged command then cannot create a mount namespace, so no copy of a Workspace mount can exist. Sent as an inline seccomp profile that allows everything else | Vendor Docker's full default profile with those rules added (keeps its other denials; several hundred lines tied to an Engine version); or bind the abort control and abort a detached connection |
| C-2 | OWNER-12, G03 | fix | The request service's existing accounting gains the largest READ size and WRITE length it has received. Real telemetry, reported with the other counters | A system-call fixture around the tests' own threads |
| C-3 | OWNER-12, G04 | withdrawn scope | WRITE header identity (FP-15) and a GETATTR held across a WRITE (FP-16) are withdrawn; the flagged-WRITE counts, handle and RELEASE order, dirty-page Commit and the concurrent append/read proof stay | Build the fixture |
| C-4 | OWNER-10, G01 | fix | Deployed `serial_low_water` is 256 in the runtime and the SDK examples: a quarter of the 1,024 window. The bound below the window (L-3) and the treatment of a non-contention early failure (L-4) stand as built | Any value from 1 to 1,023 |
| C-5 | OWNER-11, G02 | ruling | Debt admission is the `mount:debt` refusal while maintenance is stopped. No headroom number; Status unchanged. The quarantined-engine stage is the proof; a stop on an otherwise usable engine has no hook-free stage and is a withdrawn scope | A configured headroom and an additive Status record |
| C-6 | OWNER-9, G05 | ruling | A command's streams end when the command exits. Bytes a descendant writes afterwards are not delivered and are not an error | Deliver until every holder closes |
| C-7 | G05 | withdrawn scope | A stalled or failing sink, a delayed consumer, a connection cut between frames (FP-6, FP-7, FP-30-Runtime), an attach failure after a successful mount call and a lost runtime result at the actual topology (FP-25-Routes) are not constructible through the runtime protocol | Add such modes to the runtime harness |
| C-8 | OWNER-13, G06 | withdrawn scope | FP-21 Join, Owner, Lane and Registry stages; FP-27 release after a failed demand, concurrent demand, foreign reader, invariant and poisoned-lock classes; FP-31 foreign incarnation; FP-33 short, failed and unknown abort writes, refusal after admission and an independent trace (and FP-23-FS, which depends on them); R5-7 release error and release refused before admission; R4-9 at product-constructor scope | Product hooks, which the product-source rule forbids |
| C-9 | OWNER-14 | ruling | The product runs one receive loop per mount; specification 8.1's "two" is corrected. Selected INIT flags are computed from the offered set by the product's own rule. A `Capacity` refusal on a read fences that mount | Change the product |
| C-10 | OWNER-15 | ruling | R5-6a needs rows of the global Store deleted by an outside process. That is outside every claimed guarantee of the Disposable profile. The ignored reproduction stays as documentation | Offer content and metadata roots of trusted leaf pages to the Save's dependency check |
| C-11 | OWNER-19 | ruling | Workspaces of one daemon are visible to each other's commands (same identity, same mount namespace). The control listener accepts a TCP connect and gives nothing without the authenticated channel. Force is unavailable in the Sandbox topology | Per-Workspace identities; a bound abort control |
| C-12 | OWNER-20 | ruling | L-1 (8 comparator walkers) and L-6 (a still-dirty mapped page is outside the Commit) are confirmed. L-7 is replaced by C-3, C-7 and C-8 | As recorded in the plan |
| C-13 | OWNER-21, G08 | ruling | The count gate is the set of H-1 to H-19 assertions that existing tests make at the final identity, mapped row by row. H-5 and H-13 are recorded baselines by their own text; H-14 belongs to S10. A hypothesis with no assertion is listed as not established. SQL plan correlation is not required for a row with no SQL claim under dispute | New count telemetry |
| C-14 | OWNER-21, G09 | ruling | The resource gate is the proof plan's section 5 as written: each domain reported separately at phase boundaries with its sampling limit. No numerical bound exists and none is added. A domain with no product observation is named as unobserved | Owner-set bounds |
| C-15 | timing | ruling | The 282 timing selections stay `NOT_RUN`. No acceptance threshold exists (OWNER-2), the `P` arm is unauthorized (OWNER-3) and A2 supplies no arm (OWNER-1). R8 closes on function, counts and resources | A timing campaign after those three rulings |
| C-16 | OWNER-16, OWNER-17, R9 | ruling | The recovery catalogue satisfies the receipt condition: every row is recoverable or explicitly marked unrecoverable (59 rows, 7 pins). The recovery commit is an ancestor of local `main`, which is itself unpushed. A generator pinned by blob satisfies ignored-only generation | Locate the 7 pins first |
| C-17 | OWNER-18 | ruling | The content/storage harness port stands as built and is not part of R8 | Review before any registered row |

OWNER-1 to OWNER-7 stay `PENDING OWNER`; none blocks a functional row or the
reference removal.

## Order

1. Product fixes C-1, C-2, C-4 with tests; documents for the rulings.
2. Final suites once at the new identity; release builds; registration v4
   committed before its first invocation.
3. Every registered proof once: full-byte fixture, four preconditions,
   confinement, four holders, streams, lifecycle.
4. Outcomes for all 90 functional rows and 282 timing rows; R8c completion.
5. R9c: if every functional proof passed, remove root `crates/` and
   `core/reference-tests` with their wiring in one isolated commit, recount,
   and run the scoped checks on the resulting tree. Otherwise close unmet.

What C-1 costs: a tool that needs an unprivileged user namespace (bubblewrap,
rootless container tools, some browser sandboxes) fails inside a Workspace
command with `EPERM`. What it replaces: the container no longer runs under
Docker's default filter, whose other denials are capability-gated in the
kernel for the unprivileged command identity; the confinement proof is run
again at the new identity.
