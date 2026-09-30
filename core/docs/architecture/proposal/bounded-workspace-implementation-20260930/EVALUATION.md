# Verification and evaluation after the architecture rollout

> **Status: Research; informative and not a product contract.**
> Proposed evaluation plan, 2026-09-30. Baseline:
> `7edddbdb8e8512627aed0ed42533ef099d802384`. Family8/9 and all new variants
> below are proposed selections, not registered cases, implemented runners,
> performed measurements or passing results.

Read the [specification](README.md), [rollout](ROLLOUT.md),
[acceptance](ACCEPTANCE.md), [issue closure gates](ISSUE-CLOSURE.md),
[scenarios](../../../../../scenarios.md) and
[seven-family checkpoint](../../../issues/286/SEVEN-FAMILY-CHECKPOINT-20260930.md).
Use the [report template](../../../../../benchmark_agent_report.md),
[benchmark rules](../../../../../docs/general/benchmark_rules.md) and
[Core harness instructions](../../../../benchmark/fs-bench-pro/AGENTS.md).

The evaluation has three parts: the existing seven-family regression coverage,
two proposed concurrency families, and cross-cutting resource/complexity/failure
proofs. Implementing a streaming API or observing two launched processes is not
the corresponding acceptance result.

## 1. Evidence classes and execution prerequisites

- Correctness proofs cover independent complete declared state, old/new/pinned
  views, exact accepted operations, generation/head identity and cleanup or
  intentional retained custody. Component controls keep their route label.
- Count/resource diagnostics explain work, simultaneous allocations, backing
  and ownership. Within-run operation counts are not repeated performance arms.
- Performance selections measure a prospectively frozen real public operation,
  with cache/topology/resource/source identity, complete command and separate
  verification. Numeric eligibility is independent from functional success.

Before implementing a new/changed benchmark or collecting a sample, commit its
roadmap case specification under the relevant issue. #249 owns concurrent command
delivery; #249/#219 own multiple-Workspace delivery; #248/#256/#276 own the relevant
streaming/scaling variants. Freeze IDs, cardinality, counts/bytes, schedule,
entrypoint, timers, limits, independent oracle and membership first. This research
file is not that frozen admission contract and does not create or update tickets.

Use the existing harness/runner and public SDK routes; add no second benchmark
framework or candidate-specific mutation route. Pristine masters/builds/images
are reused by exact identity. Samples use independent writable clones where
required. No warmed setup reads, unchanged-arm resampling or hidden per-operation
status/digest/RPC is introduced. Keep one producer per admitted construction
operation and the unchanged Store capacity; concurrent operations do not authorize
extra helper construction workers.

The seven-family benchmark topology remains the declared host SDK/coordinator/
Server/SQLite with Linux daemon/FUSE/workloads. Do not move SQLite into Docker
or silently change the hosting contract to obtain a memory or speed result.
The proposed native-Linux Server cgroup capability has a separate product
capability/resource proof; it cannot replace or be pooled with those benchmark
arms. Whole physical-memory qualification remains unavailable where the actual
topology cannot enforce/observe the required domain. Preserve INELIGIBLE or
INCOMPLETE rather than inventing exclusive attribution.

## 2. Existing seven families: coverage and affected-case selection

All seven stay visible in the final coverage report. Each row records either a
new exact-source proof or explicit unchanged-mechanism evidence reuse. A broad
source seal alone neither invalidates every proof nor proves every path unchanged.
The rewrite changes much of the Workspace composition, so Families3-7 should be
expected to need affected final-source evidence; decide at the actual source.

| Family | Preserve and verify | Architecture-specific additions or selection |
| --- | --- | --- |
| 1: init_namespace | ProjectApi::init release-only SDK timer;100/1,000/10,000/100,000 rows remain visible with their actual registration/oracle scope; exact namespace/selected content and cleanup | Reuse when Init/profile/C1/C2 mechanisms are unchanged. If initial namespace grammar/certification/profile or its construction changes, select affected tiers with new profile IDs/independent v2 roots. Keep the lite verifier's selected-content scope explicit; do not call it full payload coverage. |
| 2: history_retention | All three strides10/3/1 and17/53/157 states; independent roots/O3/old/new trees/bytes; genuine C2+C5 allocation and the accepted up-to10% history profile | Reuse unchanged v1 canonical/codec/packing/history proofs. Shared C2 index/engine/Save changes require the smallest affected checkpoint, then only further affected rows. Do not automatically rerun all strides or retune compression. V2 identity/migration evidence is separately registered, not compared to v1 expected IDs. SDK/daemon time stays N/A. |
| 3: workspace_write | Nine append/dispersed/repeated cells at100/512/4,097; real SDK Exec/FUSE, known Commit, old/new byte oracle and cleanup | Add partially overlapping/nested writes, wide overwrite/shrink of a fragmented file, no-op restoration and byte-size variants where affected. Count accepted writes separately from surviving intervals and final transmitted replacement bytes. |
| 4: workspace_commit | Clean/dirty/capture/pin/known/Unknown/capacity/installation and cleanup scopes; historical10,240 remains deferred | Add successive-generation parent locality, real writes during each Commit phase, candidate crossing capture, G2 changes immediately before install, slow consumer and result terminal loss. Validate protected READY resources and exact current-G2 selection. |
| 5: workspace_namespace | Wide/deep trees, moves/replacement, aliases/orphans/listing and refusal/refund; existing deep270 and byte/path identity witnesses | Add resource-admitted >128 simultaneous handles, >1,024 listing on one handle, next u16/count boundaries, >4,096 wide rebind and path-local namespace v2 proof. Count completed graph visits/ancestors/new pages; full initial certification and actual deletion closure remain charged work. |
| 6: workspace_mutations | Mixed ordinary syscalls, permission/refusal, retained reads and exact exit7 accepted-state semantics | Add FUSE interrupt/publication races, daemon-death recovery, truthful statfs and explicit discard. Cancellation/nonzero exit preserves accepted changes; unmount does not mean rollback. Unknown publication forbids discard/replay. |
| 7: workspace_shell_package | Ordinary install/update/remove recipes,128/129/257/1,025-file and G1/G2 retained witnesses; complete tree/metadata/bytes/old-head proof | Add package refresh while another command modifies a different file, many-file repeated Commit lineage, long names/large encoded result pages and inode refill under two occupied C2 Save slots. Use ordinary commands; do not specialize product routing. |

Existing functional family PASS and command-budget coverage remain finite scoped
proofs. Historical numeric cache INELIGIBLE rows are not a speed baseline. The
historical dirty-discard FAIL remains FAIL; new explicit-discard cases have their
own IDs. Do not activate the deferred10,240 selection or every SC tier by default.
The >65,535 final-run and other boundary obligations can use focused external
correctness/count proofs; they are not automatically full performance campaigns.

## 3. Proposed Family8: concurrent commands in one Workspace

The public surface is multiple independent ordinary WorkspaceApi::exec calls,
composed with exact owned execution/cancel operations where required. One shell
forking two children does not prove multiple SDK control calls are admitted.
Standard finite cases use the normal15s complete command budget unless a small
prospectively declared25s exception is justified. Verification remains separate
and under10s. Counts, files, payloads and numeric thresholds are frozen in the
subsequent case specification.

| Proposed witness | What actually overlaps | Independent correctness/progress requirement |
| --- | --- | --- |
| F8-A: distinct-file writers/readers | Two SDK commands update different files while a reader selects versions | Both commands are active and accept work before either completes; exact bytes/names, own output and Commit result; no whole-command registry/lifecycle exclusion |
| F8-B: same-file overlapping writes | Two ordinary positional writers target partially overlapping ranges | Controlled accepted-publication order gives exact last-accepted-write bytes; no lost retained regions/torn accepted syscall; conflicting inode operations serialize and unrelated files progress |
| F8-C: Exec with Commit | A command writes while capture/lowering/transfer/construction/install occurs | G1 contains exactly pre-capture accepted writes; G2 keeps later writes; next Commit includes them against its immediate predecessor; old/pinned heads remain exact |
| F8-D: second same-W Commit | A second caller submits while the first owns its pending slot | Precise Busy/no second publication, with first operation and ongoing filesystem writes intact; no automatic replay |
| F8-E: cancellation/disconnect | One command is cancelled or loses its owned channel while another continues | Exact command domain/FD/output cleanup; accepted writes survive; sibling command/channel remains valid; queued/callback publication respects the declared cancellation boundary |
| F8-F: descendants/output/liveness | Leader exits with setsid/double-fork descendants or a pipe holder; noisy and silent variants | Domain empty, reaping, pipe termination and terminal delivery are separate; no premature terminal or stranded descendant;8KiB/stream captures and at most one unsent Alive remain bounded |
| F8-G: active count versus lifetime count | Declared concurrent levels2 and4 when admitted, then many sequential short commands | Capacity follows actual resources; no hidden fixed command limit; completed commands release registry/PID/timer/channel state and admission never scans all old commands |
| F8-H: external mounted-path writer | Authorized process launched by the external supervisor writes the same visible mount | Same supported create/write/truncate/rename/unlink semantics without an Exec command lease, then exact public Commit; separate route label and explicit external process ownership |

F8-B's reference is built from the frozen write operations and deterministic
schedule, not from the candidate's output. For an uncontrolled race, do not assume
start order equals publication order; prove legal atomic outcomes or use a
controlled fixture schedule. Do not create a product command parser, private
test hook or production status poll per write to manufacture that order.

F8-C uses deterministic barriers from real external providers/kernel/process
coordination. A fixed sleep, two launched PIDs or two submitted RPCs is not overlap
evidence. Fault-held variants are correctness diagnostics and do not contaminate
the ordinary performance selection. Record accepted publications before/after
capture and full old/live/saved states at exact identities.

The removal of30s has an additional dedicated correctness witness: one healthy
quiet command actually exceeds30s without termination while another short
command/status/cancel operation can progress. Freeze a finite functional harness
bound before running it. It is not a15/25s performance selection, does not enlarge
any existing performance gate, and does not replace Commit/callback/liveness bounds.

## 4. Proposed Family9: multiple Workspaces sharing one daemon/Store

Use one sandbox/daemon, configured Workspace count, exact IDs/incarnations and
independent mounts. Separate Branches are used for independent Commit cases;
a deliberately shared Branch is a distinct conflict case. Sharing a Store does
not grant a Workspace access to another mount/backing or cancellation handle.

| Proposed witness | Configuration | Independent requirement |
| --- | --- | --- |
| F9-A: live count/admission | Operator values1/2/3, each a separate configured variant | Exactly the configured number of simultaneous Workspaces, refusal of next create without partial visibility, replacement only after exact cleanup, independent sandbox policy and no lower singleton/two cap |
| F9-B: independent execution | Two and three Workspaces perform ordinary file/namespace commands | Simultaneous real mounted progress, exact isolated data/output/handles, cross-actor denial and no long daemon-global lock |
| F9-C: independent Commits | Two dirty Workspaces, separate Branches, existing C2 capacity | Both own independent submissions and make progress as resource admission allows; exact per-W roots/parents/results; one producer per operation and no added workers/permits |
| F9-D: shared-Branch conflict | Two captured Workspaces share one expected head | At most one matching CAS successor; loser gets explicit HeadMoved with exact staged/save/cleanup custody, no hidden merge/rebase/resend |
| F9-E: bulk versus small/control work | W-A bulk Commit; W-B small filesystem/control operation | Protected control/cancel/catalog progress between bounded work quanta; no wait for A's whole Commit due to an unrelated global lock. Real exhausted bulk capacity may still refuse a bulk operation. |
| F9-F: closing/failure isolation | Close/cancel/exhaust W-A while W-B runs | No cross-W kill, premature owner refund or sibling channel loss; stale A incarnation cannot affect replacement; retained owners continue consuming their live count/quota |
| F9-G: catalog progress under bulk Save | Two occupied C2 Save slots plus a third W consuming its admitted serial range | Refill goes through authenticated C5 catalog admission before either complete bulk Save finishes, using the same quotas; actual exposed serials stay consumed |
| F9-H: resource/pin pressure | Admitted active Commits/commands plus selected views and per-W child/private quota variants | Global/per-W/Server/child resource accounting composes without double counting; protected install/control remains usable; precise refusal/retention when capacity is exhausted; pins keep exact bytes until release |

F9-G is a narrow held-I/O correctness/count probe, not a changed benchmark Save
algorithm. Prepare permitted preconditions outside its diagnostic interval and
consume the actual selected identity range; do not shrink allocator ranges to
force a refill. F9-F/H use precise per-W quota/child failures for isolation claims;
shared allocator/Store integrity failures may legitimately quarantine their
broader declared domain and must report that scope.

## 5. Cross-cutting streaming and complexity proof matrix

Vary population and bytes independently; do not infer bounded memory from a
large payload with one extent, or infer fragmentation scaling from many WRITEs
that overwrite one range. Each selected size/count is a different declared
workload variant, not repeated sampling of an arm.

| Axis | Compare | Required observation |
| --- | --- | --- |
| Write count versus final state | Repeated same range, nested/partial overlaps, separated ranges | Accepted syscall bytes/counts, surviving intervals, replacement/descriptor bytes and C1/page work. Overwritten unowned state retires; no WRITE-history replay. |
| Successive generations | Same bounded new changes over increasingly fragmented saved predecessors | First touch inherits one opaque parent span; lowering does not enumerate all historical predecessor extents; current-generation delta work stays local and next base is exact |
| Dirty identities and directory width | Many files across directories versus one wide directory | Paged lowering/results/graph state; resident windows remain admitted; monotone cursor progress and honest ordering/ancestor costs |
| Payload bytes | Sparse changes to a large inherited file versus fresh/dense replacement | Actual bytes/unchanged boundary reads, replay high-water and physical file-cache residency; fresh/copy pays full bytes; no file-size-proportional resident spool |
| Fragmented wide mutation | Replace/truncate a range containing many surviving intervals | Bounded split/join boundary planning, short global publisher, precise inode conflict lease; actual removed-owner traversal/relocation stays measured |
| Active operations versus idle Workspaces | Idle W count, admitted active command/Commit counts, deliberate admission exhaustion | Idle W does not reserve full Commit buffers; active windows compose inside the real global budget; additional concurrency may refuse rather than overallocate |
| Pins and cleanup | No pins, retained selected views, final release, known and Unknown failure | Correct old bytes/location context; actual allocated/reserved/retired credits; cleanup backlog is charged and cannot disappear from wall or disk accounting |
| Completed command history | Many sequential commands with fixed active concurrency | No growing retained command/PID/timer/output population; no repeated scan of all earlier commands |

The target bound is the selected simultaneous allocation/physical-domain envelope,
not an invented exactly flat RSS line. Structural height, active owners and caches
consume real admitted bytes. Observe SDK/host Server process scope, SQLite engine,
daemon, child processes, anon/file/shmem/kernel/socket/dirty/writeback, private
payload/pages, replay/scratch and retained pins separately where supported.
Unavailable exclusive/peak observations remain unavailable. Verify phase peak
reset/read through the same owned descriptor when applicable; lifetime peak is
not substituted. A cgroup hard ceiling/OOM alone does not prove healthy progress.

Disk proof includes peak temporary metadata/replay/payload and final C2+C5 bytes.
Physical SQLite freelist/high-water can persist after logical cleanup; do not
claim disk refund from row deletion alone. Pins/Unknown owners can retain actual
data intentionally. No approximately5% storage saving at approximately50% speed
loss is accepted; the existing up-to10% history allocation tolerance does not
grant an unbounded scratch allowance.

## 6. Canonical, migration, wire and lifecycle gates

Run meaningful external contract tests at the coherent frozen source before
dependent enablement. The proposed concurrency families cannot replace:

- Independent v1 compatibility and v2 FileState/edit-policy/namespace golden
  roots/partitions, parent certification, cycle/alias/final-batch negative cases.
- C2 schema11 exact locator migration, unchanged payload packs/save IDs/ordinals,
  proven quiescence, bounded transactions, explicit known/Unknown/cleanup phases,
  and new v2 Stack/scope import with paged serial remap.
- Result sets larger than32KiB, exact EOF/trailer/seal and withheld/partial terminal;
  pinned reads16KiB/31KiB/32KiB/65,535/65,536/128KiB and maximal long-name listing;
  partial authenticated I/O, identity/incarnation and cross-channel result refusal.
- READY/known publication versus lost response, small current-G2 install, separate
  installed-but-cleanup-failed outcome, exact same-selector local completion,
  and no automatic resend/adoption/refund.
- Real FUSE interrupted read/write/rename publication outcomes, dead-mount recovery,
  statfs/permission/refusal, runtime actor/bootstrap/domain and accepted exit7
  writes with separately authorized explicit discard.

Tests stay outside product src and use production/public boundaries or real
external providers. No fault branch or fake allocator/clock is added to product.
Complete the owning locked Core checks, examples, boundary guard/self-tests at
final handoff and report exact commands/platform gaps. No CI/preflight wrapper.

## 7. Reports and evaluation order

Extend the existing report layout with one scenario row and separate per-command/
per-Workspace tables. The following cells are templates, not evidence:

| Case/profile | W / active Exec / active Commit | Complete command / bound | SDK Exec and Commit | Actual overlap/progress | Global/domain memory | Temp/final allocated | Correctness/custody/cleanup | Numeric eligibility |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| <exact selection> | <counts> | <ns / bound> | <per-owner rows> | <events/work counts> | <measured scopes or UNAVAILABLE> | <bytes by owner> | <verdicts> | <status/reason> |

| W/incarnation / command | Admission/start/terminal events | Exec wall | Accepted operations/bytes | Output/exit/cancel | Cleanup/retained owner |
| --- | --- | --- | --- | --- | --- |
| <identity> | <caller monotonic timestamps> | <ns> | <counts/bytes> | <bounded result> | <status/identity> |

| Workspace / Commit | Captured G1 / installed head / live G2 | Commit wall | Writer wait / capture-install hold | Server/transport/FUSE attribution | Old/new/pin/cleanup proof |
| --- | --- | --- | --- | --- | --- |
| <identity> | <exact identities> | <ns> | <observations or UNAVAILABLE> | <complete scopes/counts or UNAVAILABLE> | <independent scope/status> |

Report makespan and per-owner latency; do not sum overlapping Exec/Commit/Server
spans into an end-to-end total. Throughput is explicitly actual completed work
divided by its declared wall interval, with written bytes, final changed bytes
and Commit transfer bytes separate. Do not infer a2x gain from two active callers.
Small/control responsiveness and maximum observed publisher/writer wait are
separate metrics. Attribution/completeness requires all named producers; a missing
Service record is incomplete evidence even when bytes were saved correctly.

Suggested sequence at actual implementation identities:

1. Freeze contracts/oracles and prove the smallest complete R4 path with exact
   failure/resource custody. Validate real provider capabilities early.
2. Select affected Families3-7 mechanism witnesses and cross-cutting count/resource
   diagnostics while the implementation is coherent. Reuse unchanged Family1/2
   proof; any affected Init/history checkpoint is explicitly justified.
3. Qualify R5 lifetime/FUSE/domain/wire boundaries, then the functional Family8/9
   overlap, isolation, admission and failure witnesses. #249 enablement waits #248.
4. At final cutover source, complete the seven-family run/reuse matrix and missing
   issue boundary proofs. Take only prospectively selected performance rows once,
   with separate exact-identity verification and complete raw evidence.
5. Publish the admitted profile and all FAIL/INCOMPLETE/INELIGIBLE/NOT_RUN/deferred
   cells. Missing cache/physical capability stays a limitation; later larger tiers
   and full56/21-case campaigns are distinct selections, not automatic reruns.

As the first compact concurrent performance selection, freeze four ordinary
mechanisms: F8-A disjoint writers, F8-C Exec/Commit overlap, F9-C independent
Commits, and F9-E bulk versus small/control work. Every other new witness still
needs its correctness/resource proof; it need not acquire an artificial speed
target or become a long timed benchmark. Add further performance cases only when
they answer a declared mechanism question. The long quiet command is always a
separate lifetime correctness proof.
