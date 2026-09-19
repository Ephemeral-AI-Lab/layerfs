# Co-design sequencing review — 2026-09-20

> **Status:** Research; informative and not a product contract.

Owner-requested parallel review of whether the three co-design pairs need a
different order. This document proposes a sequence and concrete issue amendments;
it does not adopt deployment choices or change GitHub issue scope/state. Stage 6
finalization was left undisturbed. No product changes, builds, tests or benchmarks
were performed.

## Conclusion

**Adjust the execution sequence, keep the three issues and their numbers.**
Pair numbers identify responsibilities. Their interfaces require shared decisions,
so completing #179, then #180, then #181 is too serial. Completing a full daemon
first has the inverse problem: its operations and completion meanings are not yet
defined. Use a short shared contract milestone followed by parallel work and
specific schema/protocol/implementation gates.

The proposal [index](../../../../../../core/docs/architecture/proposal/README.md)
currently calls pair 1 → pair 2 → pair 3 strict, then requires pair 3 tenancy input
before pair 2 schema freeze. [#181](https://github.com/Ephemeral-AI-Lab/layerfs/issues/181)
already carries that narrower dependency. This is a real planning contradiction,
not an inferred need for new tickets.

## Recommended sequence

| Milestone | Owners | Work and exit condition |
| --- | --- | --- |
| 1. Design discovery and core review | #172, #179, #180, #181; Stage 6 continues independently | Discuss requirements now against explicit source pins. Reconcile the Stage 7 audit with final Stage 6 changes before selecting the implementation baseline. No design discussion is represented as accepted replacement proof. |
| 2. Shared initial contract | All three pairs, consuming #172 findings | Select the first deployment/trust scope, C1/C2 placement, logical operation shapes, bounded input/output, overlay/flush visibility, allocator ownership, and distinct saved/staged/published/unknown outcomes. Name the supported core facade and schema/profile. |
| 3. Parallel implementation after affected contracts are accepted | #179 runtime/FUSE, #181 operation owner plus one required transport, #180 history | Implement against the shared contract in separately owned files. Each track waits only for decisions/capabilities it actually consumes. History schema waits for tenancy/identity and metadata-publication decisions; protocol framing waits for operation and acknowledgement semantics. |
| 4. First actual integrated slice | #179 + #181, using minimum #180 semantics as needed | Real supported FUSE path: read a supplied existing file, retain an edit in the overlay, read the pending result, submit a bounded update, finish C2 and read the explicit resulting filesystem root. Use the real process boundary if that deployment has one. This proves a root save/read path, not a logical Commit. |
| 5. History and complete operation integration | #180 + #179 + #181 | Add agreed inode allocation/create behavior, stage/Commit and conditional head publication, discard and defined disconnect/cleanup outcomes. No automatic rebase/retry or added crash durability. |
| 6. Final qualification and replacement proof | Integration owners + #172 | Qualify actual claimed deployments at exact final identities. Run required unchanged-consumer C1-only/C2-only/combined replacement and promised cross-version data checks. Prior Stage 6 PASS does not admit a runtime or a changed artifact. |

Design discovery can overlap Stage 6 finalization and Stage 7. This is a proposed
clarification of the current broad "review before design" wording: core-dependent
implementation still requires disposition of the findings affecting its slice.
Stage 7 final acceptance retains all required substitution proof. A controlled
baseline slice does not close malformed-capacity or resource-validation findings;
unsafe configurable inputs must not become reachable pending their fixes/checks.

## Decide these together before freezing interfaces

1. **First deployment:** all components in Linux, or Linux FUSE/Workspace with an
   owner on macOS, or another explicitly selected supported arrangement. Keep
   C1/C2 together initially. A Linux environment alone does not require a daemon.
   The host/Linux split is a candidate, not an owner-selected requirement.
2. **Trust and tenancy direction:** principal/authorized root or Store scope,
   tenant-to-Store routing, identity scope and whether cross-project reuse is
   allowed. This precedes history schema freeze; it does not require a general
   token service or every tenancy feature.
3. **Logical operation shape:** stable input generation, changed-name/final inode
   batches, byte stream bounds, read/result shape, pressure policy and whether
   unpublished results must become inputs to another C1 operation. Keep canonical
   provider/consumer handoff local between C1/C2.
4. **Completion meanings:** overlay acceptance, object save completion, logical
   stage/Commit acceptance and conditional branch publication are distinct.
   Unknown acknowledgement remains failure with unknown outcome; no implicit
   resend, reflush, guessed rollback or polling.
5. **History placement and publication:** current C2 rejects unexpected tables
   and exposes no high-level history transaction hook. Decide whether history
   owns separate persistence referencing saved roots or needs an explicitly
   supported shared schema/transaction boundary. Do not bypass the core facade
   through public low-level SQL helpers.

Starting with an existing regular file permits the first edit/readback slice
without inventing fresh-inode allocation. Creation and scope reuse still require
the allocator ownership/nonreuse contract from #180. A process-local prototype
may retain its root handle in memory; restart discovery is a separate history
contract, not a free property of saving objects.

## How to reflect the order in existing issues

Keep #179, #180 and #181 as the responsibility trackers; no renumbering or blanket
"blocked by whole sibling issue" links are needed. Proposed shared wording:

> Pair numbers name responsibilities, not an implementation sequence. Design
> discovery may proceed concurrently with Stage 7 against explicit source pins.
> Before dependent implementation, jointly settle the first deployment/trust
> scope, bounded logical operations, visibility and completion meanings, and
> disposition affected Stage 7 findings. Track dependencies at the contract or
> schema/protocol milestone, not whole-issue closure. Final integration and
> Stage 7 replacement acceptance retain separate evidence requirements.

| Location | Proposed amendment |
| --- | --- |
| Proposal index and #165 | Replace the strict pair chain with the milestone table. Show shared initial decisions and parallel implementation, followed by integrated qualification. |
| #172 | Classify each finding by the milestone it blocks: discussion, contract freeze, reachable implementation, final proof. Keep conditional same-save capability separate from mandatory fixes and replacement evidence. |
| #179 | Jointly define operations/pressure/visibility with #181. Consume #180 allocation and completion semantics. Make the first FUSE slice's supported behavior explicit. |
| #180 | Gate schema freeze on #181 tenancy direction and metadata/publication placement. Supply allocation and acknowledgement meanings early; do not wait for a complete FUSE implementation. |
| #181 | Separate early deployment/trust/operation decisions from the implementation and qualification of one concrete link. Consume #179 operations and #180 completion meanings before framing freezes. |
| Active local pair proposals | Remove stale routing of runtime implementation to #172 and runtime acceptance to #171; align with the amended issue scopes. Historical receipts remain unchanged. |

Full suggested text and detailed dependencies are in the three specialist
reports below. These amendments have not been applied by this review.

## Wording to correct before implementation

- The current rebase/backoff queue sketch conflicts with owner direction:
  stale captured heads are refused and operations are single-attempt. Do not
  implement that older sketch as a dependency of #179/#180.
- Object content-addressing does not establish whole-operation replay safety,
  peer authorization or request integrity. Keep those contracts in #181; no
  automatic repeat of an unknown-outcome operation.
- Different branch CAS targets do not eliminate shared Store/SQLite writer
  contention. First integration need not add concurrent writers; any promised
  concurrency requires its own real substrate and proof.
- "No DB traffic on the write path" must distinguish metadata publication,
  base/object reads and the still-open pressure-triggered flush policy.
- **Correction to the earlier explanation and inherited #179 wording:** C1
  requires the final binding of each **changed name**, sorted and unique, not a
  resend of every name in a changed directory. See
  [the source contract](../../../../../../core/crates/layerfs-content/src/filesystem/input.rs).
  This directly affects accumulator size and wire payload design.

## Evidence

- [Runtime and transport review](runtime-transport-review.md).
- [History, trust and acknowledgement review](history-trust-review.md).
- [Core readiness and independent sequencing critique](core-readiness-review.md).
- Captured issue bodies: [#179](issue-179.json), [#180](issue-180.json),
  [#181](issue-181.json), [#172](issue-172.json), [#165](issue-165.json).
- [Source manifest](source-manifest.json), baseline HEAD
  `66bce8378b5e9ecb1135b1636f8ee2ffac46ccf5` plus explicitly identified current
  planning/audit documents. This is read-only design evidence, not a sealed
  implementation or measurement claim.

Validation: local report links resolve and report whitespace/JSON are valid.
No source verification, platform qualification, issue mutation, commit or push
was performed.

## Subsequent owner direction — 2026-09-20

The owner clarified that the question was whether to implement pair 3 first,
then directed a documentation update. The current implementation sequence is
**pair 3 (#181) service/transport → pair 1 (#179) Workspace/FUSE → pair 2 (#180)
history/Commit**. Only a short shared operation/identity/acknowledgement agreement
precedes pair 3; it does not require parallel implementation. Pair 3 is tested
with a real client before FUSE is implemented. The governing current sequence is
the [proposal index](../../../../../../core/docs/architecture/proposal/README.md#implementation-order-pair-3-then-pair-1-then-pair-2).

This supersedes the parallel implementation recommendation above. The original
research and specialist reports remain unchanged as the record of that proposal;
their identified contract dependencies still inform the short initial agreement.
