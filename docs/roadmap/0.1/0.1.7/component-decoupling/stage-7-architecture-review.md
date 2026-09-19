# Stage 7 — cluster 1 + cluster 2 architecture review

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Owner direction, 2026-09-20: replace the former Stage 7 runtime implementation
assignment with a review of cluster 1 (C1, content) and cluster 2 (C2, storage).
Tracking: [#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172), under
[#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165), after the completed
[Stage 6 qualification](../evidence/stage-6-round4c-20260919T000000Z/README.md).
This changes the current assignment; earlier handoffs and receipts retain their
historical meaning. The [first source audit](../evidence/stage-7-architecture-audit-20260919T192306Z/README.md)
records LOC, simplifications, public APIs, configuration and flexibility findings
against a frozen working snapshot while Stage 6 continues. Revision-substitution
proof and finding disposition remain open; this is not Stage 7 acceptance.

## Priority and outcome

**Architecture flexibility, pluggability and independent evolution come first.**
Determine whether callers can rely on explicit contracts while C1 and C2 change
their internal algorithms independently. Separate compilation or two crate names
alone do not establish loose coupling. Review the three canonical-content and
four physical-storage responsibilities in the
[cluster overview](cluster-1-2-components.md), their internal boundaries, the
C1/C2 boundary, and the boundary exposed to future integration owners.

The primary question is: can integration develop against one qualified C1/C2
revision while another worktree optimizes C1/C2, then adopt the optimized revision
through dependency selection and rebuild without rewriting integration logic?
Apply the question to C1 alone, C2 alone, and a compatible pair; a required
lockstep change is a finding to explain, not evidence of independent replacement.

Use the existing interfaces first. More traits, crates, factories or a plugin
registry are not acceptance criteria. Identify the smallest correction for each
demonstrated coupling. This assignment establishes the review and its evidence;
it does not preselect an implementation framework or authorize a broad refactor.

## Review in priority order

| Priority | Review | Required evidence |
| --- | --- | --- |
| 1 | Independent replacement and caller stability | Source-linked dependency and call graph; public versus internal API inventory; exact caller edits needed to substitute C1, C2 or both |
| 2 | Complete boundary contracts | Inputs, outputs, ownership/lifetimes, ordering/finality, authentication, absence versus failure, visibility/publication, cancellation/abort and unknown outcome; explicit policy and capability constraints |
| 3 | Hidden coupling within and between clusters | Concrete storage/SQL/pack types crossing outward, internal-module access, shared mutable state, initialization order, algorithm-specific assumptions, caller-owned predecessor hints and leaked resource/worker assumptions |
| 4 | Compatibility across revisions | Source/API, canonical bytes/IDs/profile, persisted schema/physical encoding and error/behavior compatibility assessed separately; supported combinations and explicit refusals |
| 5 | Preserved operational properties | Batching, localized work, bounded streaming/backpressure, memory/copy ownership and independent/integrated telemetry through the same production bodies |

Begin with `AuthenticatedObjects`, `FinalizedConsumer`, `FinalizedObject`, public
construction/edit/filesystem/read APIs, and C2's `Store`, `StoreProvider`,
`SaveOperation`, `SaveHandoff` and policy types. Their existence is a starting
point, not a passing verdict. Trace real callers through construction, edits,
save/finish, close/reopen and reads. Include optional predecessor information:
an algorithm change must not quietly require callers to reconstruct private C1/C2
state or invent history knowledge inside the core.

Keep C1 independent of concrete storage and runtime types. C2 may depend on C1's
documented canonical contracts; review whether it also depends on algorithm
details that should remain private. Workspace, FUSE, history, transport,
authorization and tenancy retain their own owners outside C1/C2. Distinguish
mechanical source compatibility from behavioral and persisted-data compatibility.
An unchanged function signature is insufficient if the required call sequence,
resource usage or interpretation of success changed.

## Concrete acceptance scenario: parallel integration and optimization

The owner's example uses [#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179)
(projection/runtime) as a consumer. It is representative, not the only supported
caller and not an instruction to implement that pair in this review.

1. Pin a qualified baseline C1/C2 revision, toolchain, dependency lock and supported
   profile. Select a small external consumer using public APIs, representing the
   construction/edit/save/read behavior that integration needs. Reuse an existing
   example or external test where it covers the scenario; name coverage gaps.
2. Keep that consumer's source unchanged. Select a real, contract-preserving
   algorithm revision developed independently, with an exact revision and diff.
   Exercise baseline, C1-only substitution, C2-only substitution and the combined
   substitution where compatible. Record unsupported combinations and their causes;
   recompiling the same implementation or swapping a mock proves no algorithm
   replaceability. If no suitable revision exists, mark the proof `NOT_RUN` and
   describe the missing evidence rather than inventing an optimization.
3. Adopt the candidate through ordinary source/dependency selection and rebuild.
   Record every required manifest, lockfile, caller, configuration and data change.
   Contract-preserving replacements should need no integration logic edits or
   private API access. Do not patch third-party dependencies or use legacy fallback.
4. Check canonical outputs, logical readback, error taxonomy, ownership and
   publication behavior under the same supported profile. Verify candidate reads
   of baseline-created data and baseline reads of candidate-created data where
   compatibility is promised; explicitly classify any format or profile change.
5. Report bounds, batching and telemetry behavior for the same consumer, and the
   relevant external compatibility checks. Timing or resource improvement claims
   require the repository's separate measurement contract and matched identities.

“Switch and swap” means selecting compatible component revisions and rebuilding.
It does not imply a stable Rust binary ABI, dynamic loading, live replacement of
an active save, arbitrary mixing of incompatible schemas, or a transparent Store
migration. A canonical/profile/schema change is an explicit compatibility decision,
not a successful drop-in algorithm substitution.

## Deliverables and acceptance

- [ ] Pin the exact reviewed source and dirty-tree diff, if any; distinguish
      committed qualification evidence from later working-tree changes.
- [ ] Publish the boundary/dependency inventory for both clusters and their seven
      responsibilities, identifying the contracts an integration may depend on.
- [ ] Rank flexibility/pluggability findings first, each with source/caller
      evidence, practical impact, the smallest remedy and the check that would
      demonstrate closure. Separate actual coupling from speculative extensibility.
- [ ] Record the substitution matrix and runnable reproduction commands for the
      parallel-worktree scenario. Classify every cell as proven, incompatible or
      unproven; record necessary caller changes instead of hiding them in glue.
- [ ] State API, canonical/profile, persisted-format and operational compatibility
      separately. Carry existing bounds and failure semantics through replacement.
- [ ] Give separate verdicts for C1 replaceability, C2 replaceability, their
      composition, and readiness for subsequent integration design. List every
      unresolved blocker with its owner and required evidence. Unrun proof is not
      accepted merely because a crate compiles or the Stage 6 suite passed.
- [ ] Close only when the review and required substitution proof support those
      verdicts and blocking findings are resolved or explicitly dispositioned by
      the owner. Preserve any limitation in the final verdict.

## Integration follows through the co-design pairs

| Pair | Owns subsequent design |
| --- | --- |
| [#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179) | Projection and runtime: FUSE and the Workspace accumulator |
| [#180](https://github.com/Ephemeral-AI-Lab/layerfs/issues/180) | Commit and history: stage/merge boundary, identities and timeline |
| [#181](https://github.com/Ephemeral-AI-Lab/layerfs/issues/181) | Boundary and trust: transport, authorization and tenancy |

Owner implementation order, 2026-09-20: **pair 3 (#181) service/transport → pair 1
(#179) Workspace/FUSE → pair 2 (#180) history/Commit**. The
[execution contract](../../../../../core/docs/architecture/proposal/README.md#implementation-order-pair-3-then-pair-1-then-pair-2)
requires only the short initial operation agreement before pair 3, not parallel
implementation. These pairs consume the reviewed core contract; the separate
parallel-worktree core-optimization scenario above remains a replacement proof.
Stage 7 does not implement their
runtime, create placeholder adapters, retire the reference tree, or grant release
admission. Stage 6's closed core qualification and the separate retained-history
claim keep their existing scope and evidence.
