# Co-design sequencing and core readiness review

> **Status:** Research; informative and not a product contract.

Read-only review of frozen issue bodies `/tmp/layerfs-design-order.l4IJqs/{179,180,181,172,165}.json`, current Stage 7 assignment/index, the first Stage 7 audit reports, and `/tmp/stage7-public-apis.md`. No issue/repo changes, builds, tests, measurements, or new source proof. The source findings apply to the captured snapshot, not automatically to the Stage 6 agent's current result.

## Recommendation

**Adjust the plan from a sequence of completed tickets to a sequence of shared decisions. Start all three design discussions now, in parallel with Stage 6 finalization and Stage 7 review. Freeze only the boundaries each implementation slice actually needs.** Pair numbers should not imply implementation order, and neither complete Stage 7 acceptance nor complete #181 implementation is a prerequisite for asking/answering #179's design questions.

The smallest useful order is:

1. Pin the baseline and designate the core integration surface; all three pairs jointly decide owner/process placement, the first deployment/trust scope, operation granularity, published versus unfinished-save visibility, and acknowledgement meanings.
2. Finish #179's accumulator/flush contract, #180's identities/publication/allocator contract, and #181's framing/auth/backpressure contract in parallel, exchanging their named dependencies.
3. Implement the smallest real end-to-end slice against the pinned C1/C2 pair. A direct in-process slice can validate batching, visibility and publication without waiting for a socket implementation. FUSE and the byte-stream link can then proceed in parallel once the shared operation envelope is frozen. If a daemon is required for the very first supported deployment, a minimal real link becomes a dependency of that deployment, not a reason to design the full transport before the operation contract.
4. Adopt compatible optimized core revisions through the Stage 7 unchanged-consumer matrix, then qualify each claimed deployment on its final exact source and format identities. Stage 6 performance proof does not admit a runtime.

This order does not silently choose embedded shipping, daemon shipping, single tenancy, or the location of C1. Those are short joint decisions before implementation, not assumed defaults disguised as a plan.

## Dependency DAG, not a blanket serial gate

```text
Stage 6 final source/evidence -----> pinned qualified core baseline
Stage 7 source findings ---------> allowed facade + constraints
                                            |
#179 + #180 + #181 joint decisions ----------+
       |                 |                 |
#179 accumulator    #180 publication    #181 boundary/trust
       +-----------------+-----------------+
                         |
       minimal real operation slice on pinned core
                /                         \
        FUSE implementation         required stream link/auth
                \                         /
                  supported deployments
                         |
    exact-source integration qualification + Stage 7 proof/disposition
```

Stage 7 replacement experiments may run independently after an exact baseline and actual compatible candidate are available. They need not hold up design work. Resource-sensitive execution waits for the measurement lock; read-only design does not.

## Gates by milestone

| Topic | Before discussion | Before interface freeze | Before first real implementation/use | Before final qualification/adoption claim |
|---|---|---|---|---|
| Stable C1/C2 pin | Existing frozen audit is sufficient, label its gaps | Name exact candidate core commit/tree, lock, profile/schema and relevant changes since audit | Use an immutable baseline, not the moving Stage 6 checkout; integrate later changes deliberately | Qualify final chosen identities and record post-baseline changes; historic PASS covers only its original pin |
| Allowed public surface | Audit already identifies it | Designate logical C1 and Store facade versus advanced canonical/physical internals; no SQL/pack dependency in adapters | Dependencies match designated facade; preserve original C2 failure via SaveHandoff | Unchanged consumer must stay inside that surface; violations become actual coupling findings |
| Malformed capacities and resource arithmetic | Discuss now; source findings do not require a reproducer to discuss | Decide derived-only capacities versus supported adjustable fields, provider wave bound, true scratch ceiling | Fix/check reachable public-boundary validation before any adapter can pass configurable/untrusted capacity values; an initial controlled slice may only use Store-derived policy/capacities and fixed validated resources, with this limitation explicit | External malformed/extreme input checks and required core verification must establish typed refusal before effects; no broad boundedness claim with this gap open |
| Unfinished-save C1 reads | #179 must decide whether required | Choose accumulator model and visibility guarantee | Published-base + one C1 update + finish works with current bridges. A second C1 operation using an uncommitted prior root requires a real bridge before that path is built/shipped | Prove the selected path, cancellation and publication behavior; do not invent mid-operation commits to hide a missing bridge |
| One C1 crate identity | Discuss without build | Document ordinary source/dependency selection | Caller and C2 resolve the same C1 instance | Exact baseline/C1-only/C2-only/pair assembly and locked dependency changes recorded |
| Schema churn / canonical profile | Establish that API stability differs from data compatibility | Name supported schema/profile; decide whether a change is algorithm-only or a format change | Never open a persisted fixture with an incompatible core by weakening checks; fresh disposable Store can support an explicitly new format | Cross-version reopening in both directions where compatibility is promised; report schema-changing mixtures INCOMPATIBLE, absent proof NOT_RUN |
| Unchanged-consumer substitution proof | Not needed to begin any pair discussion | Freeze the intended contract and proof consumer; practical findings may amend it | Baseline integration may start before every candidate swap is measured/tested; do not claim replacement readiness | Required for Stage 7 acceptance and adoption claims; gaps stay open or get explicit owner disposition |
| Provider error detail and timing gaps | Bring error classes and missing cause detail into shared design | Freeze externally meaningful errors, success/unknown-outcome rules, and coarse timing ownership | Preserve absence vs failure and storage unknown-outcome semantics on the first real path; decide original-cause diagnostics | Verify telemetry/error behavior on the same bodies and all claimed deployments |
| Security/trust scope | Start jointly now | Principal/root permissions, tenant scope, and process/channel assumptions before wire freeze | Authorization must exist before crossing an untrusted boundary. A local direct diagnostic slice can explicitly have a smaller trust scope | Verify promised authentication/authorization and tenant isolation; canonical hashes alone prove neither |

These gates intentionally distinguish fixing a public-core bug from gating every harmless design task on its fix. Controlled use of defaults is not closure of malformed-capacity validation.

## What can proceed in parallel

- Stage 6 finalization; Stage 7 source audit and compatibility planning; all three co-design discussions.
- #179 callback mapping, read-after-write overlay semantics, memory/backpressure accounting and mount topology.
- #180 logical commit/layer/stage identities, allocator uniqueness, discard semantics and conditional publication design.
- #181 deployment/trust model and bounded operation envelope once it receives #179's operations and #180's completion meanings.
- After shared contract freeze: direct core integration, FUSE callback plumbing and stream-link implementation, to the extent their files/owners are independent. End-to-end wiring still waits for the corresponding real pieces.

Must wait: history schema freeze for tenant/identity scope; a wire-format freeze for operation shape and acknowledgement/error meaning; uncommitted-root consumers for a supported same-save bridge if required; adopting an optimized core as a drop-in replacement for compatible-data/unchanged-consumer evidence; release claims for exact-source qualification.

## Critique of alternative orders

**“Finish Stage 6, then all Stage 7, then #181, then #180, then #179” is too serial.** Stage 7's conditional findings need #179 requirements to determine whether they matter. #181 cannot freeze logical operations before #179 defines accumulator/flush behavior or #180 defines publication. Delaying all design leaves known questions unanswered and may force speculative bridges or protocols. Freeze a safe baseline for implementation; do not wait for unrelated optimization or every deployment cell.

**“Transport first because everything else uses the link” freezes the wrong dependency.** Framing mechanics alone do not decide whether a message is a read, bounded commit batch, final root publication, or an unknown-outcome response. Sending one frame per canonical object is not a requirement of using traits; batching is a protocol decision. Conversely, putting arbitrary logical operations on a link does not automatically keep it bounded. #181's current operation-level direction is sensible, but requires an explicit operation envelope and process-placement decision, not a generic RPC framework.

**“FUSE first and add trust/history later” is also risky if called production-ready.** FUSE success/read-after-write/flush behavior commits to visibility, failure and publication semantics. A prototype can use a narrowly declared scope; actual shipped callbacks need the cross-pair decisions they expose. No amount of content-addressed dedup makes a lost acknowledgement a safe implicit retry of a history operation.

## Exact amendments to existing issues (proposed, not applied)

### Shared text for #165, #172, #179, #180, #181

Replace language that reads as “complete Stage 7 before subsequent integration design” with:

> Stage 7 review and the three co-design discussions may proceed concurrently against an explicitly pinned core snapshot. Stage 7 findings constrain interface freeze and implementation admission according to the affected boundary. The pairs jointly settle shared decisions before dependent code; no complete pair is a blanket prerequisite for another. Final integration qualification and Stage 7 replacement acceptance retain their separate proof obligations.

### #172

Add a disposition table with columns: finding, required consumer behavior, owner, minimum remedy, gate (discussion / contract freeze / implementation / final proof), evidence. Mark the same-save bridge conditional on #179; classify schema mixtures independently from source/API compatibility; retain NOT_RUN in all unexecuted substitution cells. Before freezing the next baseline, reconcile the audited snapshot with Stage 6's final source instead of relabeling the old audit. Keep malformed-capacity findings open until source fixes and external checks prove closure. No new ticket required.

### #179

Add three acceptance bullets:

- State where C1 executes relative to the consumer accumulator and owner/link, and how the complete batch crosses any process boundary.
- State whether uncommitted reads are served by the overlay against a published base, or by chained C1 roots inside an unfinished save; the latter requires the explicit bridge/proof from #172.
- Separate byte-acceptance, C2 save publication, stage creation and branch merge acknowledgements, jointly with #180/#181; define read-after-write and failed-flush retention without implying durability.

Qualify “DB traffic happens at four events” as the history/control DB rule if that is the intended meaning. Current C1 reads backed by C2 require storage lookup traffic, so unqualified “DB traffic” conflicts with the existing read path. This needs a vocabulary clarification, not a new component.

### #180

Make tenancy/identity scope from #181 a prerequisite only for history schema freeze, not for beginning history design. Update stale “core has four tables” to the pinned baseline's actual schema (five tables in the audited schema-6 snapshot), and describe history storage as a separately owned boundary: current C2 rejects unexpected tables, so casually inserting history tables in its SQLite file is not an implementation detail.

Qualify branch non-contention and merge complexity statements by layer. Different branch CAS rows avoid logical same-head contention, but the audited Store has one exclusive save owner and SQLite write serialization; row separation does not prove parallel physical commits. Reuse the #177/#178 disposition instead of reopening their entire parked register. Clarify that content object immutability does not make divergent history commands conflict-free.

### #181

Split acceptance into (a) shared boundary/trust decisions needed for contract freeze and (b) deployment-specific link implementation/proof. Do not make completion of all remote/daemon cells block a supported direct slice. Frame definitions consume the frozen logical operation envelope from #179/#180. Explicitly state where C1 runs; keep C1/C2 unaware of transport types.

Replace “content-addressing ... no transport-level MAC is needed” with a narrower statement: canonical digests authenticate retrieved bytes against an authorized expected root; caller/server authentication, authorization and channel integrity are separate requirements owned here. Do not infer the security of an operation stream from object hashes. Retain the question of retry after lost acknowledgement as unresolved until the command-level semantics are designed; fresh reflush is not automatically safe because object insertion deduplicates.

## Completion boundary

This review recommends a sequencing change and concrete amendments; it does not enact owner decisions, close #172, approve arbitrary core mixes, or certify new runtime behavior. Existing Stage 6 closure remains historical evidence at its exact scope; the active finalization's resulting pin must be inspected before it becomes the integration baseline.
