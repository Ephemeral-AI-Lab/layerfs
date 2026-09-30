# Expected issue closure after implementation and qualification

> **Status: Research; informative and not a product contract.**
> Read-only GitHub scope audit, 2026-09-30 11:19:12 UTC. Specification source:
> `7edddbdb8e8512627aed0ed42533ef099d802384`. No issue is closed or changed,
> and no implementation/test/benchmark is performed by this audit.

Read the [specification](README.md), [rollout](ROLLOUT.md),
[acceptance](ACCEPTANCE.md) and [latest #276/scaling map](ISSUE276-AND-SCALING.md).
The [17-issue source capture](ISSUE-CLOSURE-SOURCES.json) records current bodies,
titles/states and the latest two comments where retrieved. All captured issues
were OPEN. Body statements tied to older source remain historical; use the
[Phase B ticket audit](../../../issues/286/TICKET-AUDIT-20260930.md) and current
source for implemented status. The current issue title/body governs its scope;
old comments on a repurposed ticket do not merge unrelated campaigns into it.

## 1. Direct implementation closure targets

Five direct scopes match the selected architecture. Closure requires implemented,
merged behavior and the ticket's exact covering proof at the declared profile.
The sixth direct optimization target has a separate numeric performance gate.

| Issue | Architecture removes or supplies | Closure checkpoint and required evidence |
| --- | --- | --- |
| [#248: final file-delta streaming](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248) | Whole extent/descriptor/draft collections; historical-delta replay; long mutation-gate holds; implementation count ceilings | R4 core qualification plus its retained 4,097 public route, focused >65,535 final-run/count-boundary evidence, exact predecessor roots/partitions, G1/G2/pins, all-phase writer progress, resident/cache/spool ownership and known/Unknown custody. R5 lifetime capability may be needed for its composed public workload; it does not enlarge measurement budgets. V2 identity decisions are explicit and v1 compatibility remains independently proved. |
| [#256: namespace/cardinality scaling](https://github.com/Ephemeral-AI-Lab/layerfs/issues/256) | Whole dirty/name/prepared/reconcile populations; total-directory/frame ceilings; repeated namespace work; resource-independent handle/cookie admission | R4/R5 namespace proof plus 129-name, 257-file and 1,025-file public witnesses, >u16 count boundary, >1,024 listing, >128 simultaneous handles when real resources permit, >4,096 wide-rebind graph evidence, sequential Commit bases and bounded resource/work counts. Finite Phase B 1,025 success alone does not close the general ticket. |
| [#249: concurrent Exec/multiple Workspaces](https://github.com/Ephemeral-AI-Lab/layerfs/issues/249) | Singleton mount/control/lifecycle exclusion, global Commit exclusion, fixed whole-command30s lifetime and cumulative command state | R6 after #248 gates: same-W independent concurrent Exec, different mounted Workspaces, Exec/Commit overlap, one pending submission perW, cross-W admitted Commits, shared-Branch conflict, quiet >30s command, exact cancel/disconnect/domain/reaping/close and resource admission. No command-specific route or hidden session/root/handle count. |
| [#219: operator Workspace-count policy](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219) | Missing public operator policy and lower daemon singleton/two-Workspace cap | R6 configuration1/2/3 proves exactly the configured live count, next-create refusal, replacement after exact release, separate sandboxes, retained/attaching/closing counting and no lifetime counter. Generations/Execs do not consume additional Workspace slots. Historical Init experiments stay with their separate research scopes. |
| [#259: FUSE interruption/death/statfs](https://github.com/Ephemeral-AI-Lab/layerfs/issues/259) | Missing interruption/publication race handling, dead mount recovery ownership and zero capacity reporting | R5 real mounted read/write/rename interrupt witnesses, daemon-death observation and bounded exact recovery/reclaim, truthful df/statvfs before/after changes and shared quota/protected capacity, preserved refused-operation errno table. This is mount cleanup, not durable Commit or crash data recovery. |
| [#261: mounted-write optimization](https://github.com/Ephemeral-AI-Lab/layerfs/issues/261) | Costly cumulative ownership/page work in ordinary writes; missing current-path attribution | R2/R4/R5 count/source attribution plus one prospectively registered cache-eligible 4,097-write public performance gate, independent full bytes/heads/cleanup and complete-command budget. Functional success, bounded memory or removal of30s alone cannot close its speed gate. Reuse existing finite Phase B proofs at exact scope. |

These are closure targets, not promised passing numbers. R4's smallest integration
slice is not a substitute for the issue-specific larger/boundary evidence. Reuse
identity-matched unaffected proofs; register only the missing or affected evidence.

## 2. Parent and supporting closure candidates

| Issue | Expected disposition |
| --- | --- |
| [#245: ordinary range-COW/Commit parent](https://github.com/Ephemeral-AI-Lab/layerfs/issues/245) | Parent closure candidate after its declared implementation/qualification scope, file-run and many-file gates, generation/write progress and applicable concurrency rollout are complete. Record the exact supported profile and separately tracked future tiers. Closing #248 alone cannot close the package-scale parent. |
| [#283: phase-local memory observer](https://github.com/Ephemeral-AI-Lab/layerfs/issues/283) | Separate capability-closure candidate if the same-open-FD hypothesis is independently tested on the relevant runtime and a working phase observer or explicitly accepted alternative is proved. Native Linux Server memory containment does not prove the recorded Docker Desktop observation fixed. Its broader matched cache/phase attribution must remain recorded under #276. |
| [#235: benchmark substrate/canary](https://github.com/Ephemeral-AI-Lab/layerfs/issues/235) | Separate evidence-closure candidate if bounded observation completeness plus its exact-identity real canary, loss/truncation/cleanup and declared build/verifier gates pass. Implementing an ObservationSeal or passing another family is insufficient. |

No separate benchmark/capability run is launched by this closure plan. #283 and
#235 can be closed only after their own required work; do not count them as
automatic products of the architecture rewrite.

## 3. Important partial coverage: #276 remains a full ledger

[Issue #276](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276) combines several
old handoffs with newer current risks. The proposed rollout addresses its main
product causes: whole-frontier/reconciliation RAM, G1/G2 predecessor selection,
deep270 repeated validation, path-local directory-move validation, catalog refill
under Save pressure, pinned-read framing, exact retirement and explicit dirty
discard. Record each resolved item with its own current-source proof.

Whole-ticket closure additionally needs a reconciled disposition for every
remaining obligation, including:

- The owner-deferred 10,240 case: current post-credit outcome is unknown. This
  spec does not automatically select it or turn it into PASS.
- Matched cache and true phase-memory numeric qualification, or an explicit
  already-authorized named disposition. A new implementation cannot relabel
  historical INELIGIBLE/NOT_RUN receipts.
- The full deferred #270 path-local move/certification/migration/pin/identity/
  failure-resource proof, rather than only a faster DFS or mounted rename.
- Exact current cleanup, physical quota/refund and retained-owner behavior, and
  any independently required observer/report-completeness evidence.

Do not close #276 by silently removing unfinished rows or splitting them into
new tickets; the owner selected one deferred ledger. Keep it open if a required
cell is deferred, unsupported or unqualified. Old missing-public-pin, unmerged
Phase B or old test-red descriptions require current-source reconciliation, not
implementation of already-delivered work.

## 4. Scopes that remain separate

- [#174](https://github.com/Ephemeral-AI-Lab/layerfs/issues/174): scoped attribution,
  source/simplification/counter audit and uninduced SQL acknowledgment/lock gaps.
  The design covers some boundaries; the complete historical inventory still
  requires a current audit and exact proof/disposition. No automatic closure.
- [#241](https://github.com/Ephemeral-AI-Lab/layerfs/issues/241): its four declared
  ordinary-shell insert-size gates remain distinct. The older projected splice
  branch is integrated historical evidence, while the current generic design
  does not introduce an implicit suffix-free splice. General streaming does not
  erase real bytes copied by ordinary programs.
- [#232](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232),
  [#233](https://github.com/Ephemeral-AI-Lab/layerfs/issues/233) and
  [#230](https://github.com/Ephemeral-AI-Lab/layerfs/issues/230): the full56 edit,
  full21 churn/snapshot and benchmark migration gates need their declared
  complete campaigns. Targeted SC or Phase B family success does not clear them.
- [#69](https://github.com/Ephemeral-AI-Lab/layerfs/issues/69): durability/fsync/
  crash recovery is excluded from this no-sync/no-WAL architecture.
- [#82](https://github.com/Ephemeral-AI-Lab/layerfs/issues/82): SQLite/S3 immutable
  pack placement and remote publication are a separate storage capability.

Broader release, architecture-review, retained-history and benchmark parents in
the Phase B ticket audit remain at their own completion scope. This issue map
is not a blanket closure or release-admission list.
