# Workspace load-bearing scenarios

> **Status: Research; informative and not a product contract.**
> Workload catalog, 2026-09-30. Current implementation baseline inspected:
> `7edddbdb8e8512627aed0ed42533ef099d802384`. Scenario IDs below guide architecture
> and evidence selection; they are not registered benchmarks, acceptance numbers
> or claims that every workload is supported.

## Purpose and authority

LayerFS Workspace must be designed for frequent mutations, large files, many
tiny files, large edits and many small edits. The complete path includes live
reads/edits, capture, canonical construction, publication, local installation,
selected views and checked retirement. Bounded upload buffers alone do not
establish this workload envelope.

This catalog combines the owner discussion and the
[bounded-memory architecture proposal](core/docs/architecture/proposal/bounded-memory-commit-20260930.md)
with the pinned
[#245 LOAD_BEARING_CASES source](https://github.com/Ephemeral-AI-Lab/layerfs/blob/d63379b955d1af4656cbfe23ebefc05ebf36ef8c/core/docs/issues/245/LOAD_BEARING_CASES.md).
The [local historical document](core/docs/issues/245/LOAD_BEARING_CASES.md)
is retained unchanged. Its workload shapes remain useful; its implementation
limits belong to its original source. Current component contracts, repository
rules, frozen case specifications and source-pinned receipts retain authority.

Agents working on Workspace backing, file/namespace operations, Commit, resource
admission, SDK/daemon integration or scalability should identify the applicable
SC IDs in their plan and report. State which behavior is current, a design target,
verified at a specific scope, deferred or unknown. This catalog does not direct
agents to implement features or launch a benchmark campaign automatically.

## Workload map

| Owner workload | Main scenarios | Architectural pressure |
| --- | --- | --- |
| Fast-changing data / frequent mutations | SC-01, SC-06 | Local revision cost, actual write progress during Commit, bounded maintenance |
| Large files | SC-02, SC-04 | Range addressing, unchanged-content reuse, payload windows and size arithmetic |
| Many tiny files | SC-03, SC-05 | Identity/name/owner state, listing width, topology and per-file overhead |
| Large edits | SC-02, SC-04 | Streamed input and affected-range planning, truncate and replacement semantics |
| Many small edits | SC-01, SC-02 | Surviving fragmentation, final-state aggregation and ownership retirement |
| Combined high load | SC-06, SC-07, SC-08 | Generation lineage, pin/quota custody and aggregate byte admission |

All sizes/counts below are illustrative candidates. A larger tier is a future
profile where current bounds exclude it. Each named variant is registered and
reported separately if promoted; one passing variant does not cover its siblings.

## Common product route and expected properties

Ordinary-shell cases use public `WorkspaceApi::exec(command)` in the real mount,
through the daemon's ordinary shell. Examples use image-local deterministic
fixtures under `/fixtures/stress/` and destinations relative to the mount. Those
paths describe fixtures to prepare and freeze; this document creates none.
Use normal tools such as cp, mv, rm, mkdir, dd, cat or an image-pinned script.
No LayerFS edit tool, hidden ioctl, fixture-selected product path, package-network
dependency or candidate-derived expected result substitutes for that route.

Mutating success cases call Commit after a confirmed successful Exec. A failed
Exec retains its accepted partial private state without implicitly committing
or discarding it. Record the observed head separately; an independent authorized
publication retains its own outcome and installation custody. Read/output-only
controls do not need a Commit.
Native component proofs can cover isolated invariants, but must retain their
route label and cannot stand in for SDK/POSIX coverage. Concurrent G1/G2 cases
freeze an actual overlap route instead of inferring overlap from sleeps.

Design targets shared by the scenarios:

- Live data is a selected paged view of canonical Base or exact parent-file
  versions, Packed/Payload and Zero intervals, plus inode/name/tombstone facts.
  Readers see accepted revisions, not an
  incompletely prepared range or tree. Stable identity survives rename and links.
- Resident live-edit and Commit plans use admitted windows/cursors/caches. Whole
  dirty/extent/name/result/graph/retirement populations stay in charged paged
  backing. Application heap and mappings have a separate deployment budget.
- Each admitted Commit owns its working set from a global byte pool. Idle
  Workspaces do not reserve full Commit buffers; their base/reference and required
  completion/custody state is still admitted. Server/C1/C2 memory is separately
  composed, including codec/engine/wave/index overlap and physical cache scope.
- Capture preserves G1 while later G2 changes retain their own root. Before final
  Branch publication, preparation facts and protected installation resources are
  READY. Known-result installation performs a small selection change rather than
  building a complete post-publication patch. Pins retain immutable contexts.
- Storage sharing preserves unchanged content. Actual changes, proof/replay
  state, old selected versions and failed owners still consume quota. Credit is
  returned only after identity/block-checked release; buffered spool growth is
  not excused as bounded memory. Unknown delivery authorizes no replay/deletion.
- Sparse-update work, total latency, disk overhead and bounded RAM are separate
  properties. Canonical root/partition equivalence is required where identities
  are promised. A short install gate does not make total Commit constant-time.

## Architecture coverage and remaining gates

The concrete replacement
[implementation specification](core/docs/architecture/proposal/bounded-workspace-implementation-20260930/README.md)
maps these scenarios to selected modules, ownership, complexity and later proof.
Its [latest #276 and scaling map](core/docs/architecture/proposal/bounded-workspace-implementation-20260930/ISSUE276-AND-SCALING.md)
forbids moving quadratic work to disk unchanged. New private/canonical/runtime
profiles have explicit migration and independent oracles; they do not promote
the existing finite family proofs to those capabilities.

The proposed complete design supplies a mechanism for every scenario. Streaming
Commit buffers alone does not complete that coverage. This is a design map, not
implementation evidence or a PASS table.

| Scenario | Proposed architectural coverage | Remaining gate |
| --- | --- | --- |
| SC-01 | Bounded live-edit plans, final-state cursors, paged owner state and safe retirement | Hot/generic-path throughput, fragmented mutation admission and scope-specific proof |
| SC-02 | Range sources, sparse canonical reuse, streamed wide edits/shrink and selected views | Larger logical/write/wire profiles, exact offset arithmetic and canonical partitions |
| SC-03 | Paged identities/bindings/cookies/results and external exact validation | Wide/deep namespace interfaces, declared counts and per-file IO/proof cost |
| SC-04 | Payload windows, bounded construction and ownership cleanup | Actual full-command bytes/cache profile, larger shapes and one-call versus multi-call semantics |
| SC-05 | Stable component identities, ordered namespace streams and exact graph/refund proofs | Canonical compatibility and global validation cost; incremental parent proof is a separate optimization |
| SC-06 | Immutable G1, independent G2, versioned sources, terminal exceptions and small install | Private resolver/format specification, exact old/new/pin proofs and real SDK overlap route |
| SC-07 | Protected READY resources and explicit known/unknown/installed/cleanup custody | Public recovery/discard policy, read framing and external failure/cleanup qualification |
| SC-08 | Exclusive working sets plus aggregate byte admission | Per-Workspace pending permit, daemon/control/transport concurrency, command leases and scheduling/isolation proof |

SC-01 through SC-06 are direct workload targets of bounded live backing plus
streamed construction/generation completion, with their listed limits and proof
gates. SC-07 combines the core custody model with adjacent lifecycle/API fixes.
SC-08 is part of the joint #249/#219 ownership/resource design now, with concurrent
enablement/qualification following #248's memory/identity/custody gates.
Supporting a logical shape does not establish a latency target, an unlimited
size profile, constant whole-host memory or every filesystem/application semantic.

## SC-01 — Rapid mutations and surviving fragmentation

**Shape.** Keep repeated overwrite, separated overwrite and append as distinct
variants. Use deterministic patches in an inherited file and alternate first,
middle and last ranges. Candidate scales can distinguish thousands of writes,
larger surviving runs and future million-record targets. Historical WRITE count
and final interval count are recorded independently. The owner-deferred 10240
selection is not activated by this catalog.

**Required behavior.** Latest live bytes are exact after accepted calls. Repeated
overwrites replace older live intervals where semantics allow; disjoint changes
remain distinct. Commit processes final surviving state without a resident
descriptor/result array. Old canonical and selected versions retain their bytes.
Successive Commits name the correct predecessor and do not need cumulative old
delta replay. Backpressure is bounded and keeps the registered deadline policy.

**Observe.** Actual callbacks/bytes, final extents, affected-range plan/map/vector
capacity for each live mutation, index visits/reseeks, payload packing/ownership,
allocated and reserved bytes, cleanup work, simultaneous
capacity and writer wait. Fixed buffers alone do not cover permanent registries.

## SC-02 — Large inherited file: sparse, dense and truncating edits

**Shape.** Start with the historical 500 MiB sparse-patch candidate or a proposed
3 GiB inherited file. Register a 4 KiB middle overwrite, a larger bounded region,
and a replacement exceeding 8 MiB in total as separate variants. Include first/
middle/last boundary positions. Separately shrink a highly fragmented file,
extend it with a hole, and mutate while holding an old view. Single files such
as 16 GiB model weights belong to a future larger-size profile.

**Required behavior.** A sparse overwrite addresses required ranges and mapping
boundaries without automatically reading/copying/spooling the whole unchanged
file. A dense edit pays its actual bytes. A shrink or wide fragmented overwrite
uses bounded affected-range/update/retirement planning before publication; moving
only Commit arrays to disk would leave this case unsolved. Independent full-file
and old-view oracles check EOF, offsets, size, zero ranges and metadata.

**Observe.** Logical edit bytes, Base/boundary reads, callbacks, affected versus
untouched extents/pages, temporary payload/object bytes and full memory/cache
scope. Native one-WRITE limits and an application's multi-callback edit differ.
Live Zero gaps need no payload bytes, but current SaveFile v2 still transmits
Zero replacement bytes; an implicit-zero transfer is a separate design.

## SC-03 — Package forest and many tiny files

**Shape.** Preserve a roughly 160-package refresh and a concentrated 160-file
package with a 192 MiB asset. Use create/refresh/remove plus lockfile or manifest
replacement. Isolate one wide directory, a distributed tree and deep component
access; they exercise different paths. Proposed 10,000/100,000/1,000,000-file
tiers are architecture pressures, not existing many-file Commit proofs. Include
payloads around 32 B, 128 B and 1 KiB to expose metadata and packing boundaries.

**Required behavior.** Every identity, binding, type, mode and byte is present or
absent as declared; no skipped packages or stale temporary names. Lookup, listing,
cookie/reference custody, capture, binding decode, saved results and validation
are bounded in resident bytes. Deep access uses real components and observes
platform pathname limits. Preserve aliases and correct directory parent rules.

**Observe.** Dirty identities and changed names separately; listing pages/cookies,
held references, inode reservations/round trips, graph visits, private files/
packs, logical versus allocated bytes, and old-tree reuse. Live packing currently
covers payloads <=128 B; larger small files use owned payloads. Dense metadata
and bounded memory do not imply free per-file IO or cheap full topology proof.

## SC-04 — Bulk file flow: fresh, append, replacement and logs

**Shape.** Use deterministic 256 MiB input for a new file and an existing-file
append, separately. Include a 64 MiB file replaced through a temporary file, a
larger payload candidate, a sustained Workspace log append and shell-driven
prepend/insert. Shell concatenation into a temporary file followed by mv sends
the complete new file through FUSE; it is not an in-place insertion primitive.

**Required behavior.** Acquisition, live publication, Commit construction and
ownership release use bounded windows with no file-sized heap or cache spool.
New-file and inherited-file routes each receive exact bytes, length, mode and
old-head proof. A large operation delivered as many FUSE callbacks does not
inherit a one-shot native atomicity promise. Replacement preserves namespace and
selected open-handle semantics. Cleanup includes the actual abandoned owners.

**Observe.** Real read/write callbacks and bytes, data passes, private disk high
water, codec/Store work, end-to-end/Exec/Commit/cleanup walls and page-cache scope.
The architecture cannot erase full-file IO an ordinary command actually performs.
Workspace file logging is distinct from bounded stdout/stderr capture in SC-07.

## SC-05 — Namespace moves, replacement and recursive cleanup

**Shape.** Move a file and a whole directory across parents; replace an existing
destination via a temporary file; recursively remove an inherited package and
copy a replacement tree. Include a subtree of unrelated descendants, aliases,
opaque symlinks and an open-unlinked file. Keep file and directory moves separate.

**Required behavior.** Exact old-name absence and new descendants, identity,
modes, link counts, target bytes and held-file contents are preserved. Reject a
cycle or invalid parent topology before publication. An unrelated subtree remains
byte-identical and reuses roots where the canonical contract permits. Local
rename should update component/parent facts without rewriting resident descendant
paths. Complete Commit topology proof may still require broad graph work.

**Observe.** Exact syscall/error, parent/ancestor work, canonical alias/cycle
visits, subtree/page reuse, listing consistency, deletion plans and checked
payload/index retirement. Differentiate local rename cost from final graph
validation; one fast rename does not qualify a million-entry namespace Commit.

## SC-06 — G1/G2 overlap, successive Commits and selected views

**Shape.** Capture G1, then accept deterministic G2 content and namespace changes
during each actual Commit phase. Include insert-like content produced by ordinary
tools, overwrite/shrink/holes, repeated/backward source ranges where the supported
route permits them, rename/unlink/alias changes and selected orphan versions.
Publish G1 and then Commit G2. Register explicit barriers and route identities;
independent SDK overlap is a future capability where control serialization blocks
it today. A lower-level proof retains its narrower label.

**Required behavior.** G1 includes only its captured changes; G2 changes survive
installation and appear in the successor. File source coordinates select the
correct version, including when a source span no longer exists in the successor.
Expected Branch head and ordinary file predecessor follow the known successful
lineage. Old pins retain original immutable resolver contexts and bytes. Orphan/
multi-origin exceptions cannot follow latest-by-serial state or grow recursive
resolver chains across every Commit. Exact canonical identities need their proof.

**Observe.** Source root/version and descriptor bytes per Commit, actual writer
progress/wait at each phase, installation gate work, result/exception tables,
resolver depth, known outcome, installed revision and separate cleanup custody.
All mandatory completion RAM/disk is admitted before Branch send; G2 cannot
consume protected progress resources.

## SC-07 — Admission, failed commands, output and exact custody

**Shape.** Select independently: occupied private quota; held old pins plus later
changes; memory/response admission; definite pre-publication refusal; uncertain
delivery; known publication followed by local IO/refusal; release/unlink failure;
and an Exec that mutates then exits 7 without Commit. Include view-read boundary
sizes and malformed/forged/stale authority where a sanctioned route supports the
proof. Distinct failures remain distinct registrations.

Use a separate no-file-write control producing deterministic 16 KiB stdout and
stderr each. Compare it with SC-04's multi-MiB Workspace log file and with daemon
diagnostic capture. Their limits/owners are independent; today ExecResult retains
up to 8 KiB per stream with truncation flags. Pipe draining must complete without
turning discarded output into an unbounded queue.

**Required behavior.** Admission/known/unknown/installed/cleanup results are
explicit. A failed shell does not implicitly advance the head or discard accepted
private state. Unmount detaches; forced container deletion is not clean Workspace
closure. Recovery/discard is an explicit supported decision. Unknown owners and
selected old bytes stay charged; no canonical replay or guessed refund occurs.
SDK advertised read sizes require their original full-size proof, not a smaller
successful request. Output-only control preserves the Branch head.

**Observe.** Allocated+reserved+uncertain quota, selecting roots, canonical call
count, final installed revision, retained archives and checked close/release.
The historical exit-7 cleanup FAIL and current owner-deferred 10240 outcome retain
their identities/statuses. External fault induction must not add test-only
production hooks or fake a successful operation.

## SC-08 — Multiple Workspaces and aggregate resource admission

**Shape.** Start with distinct 1/2/3-Workspace profiles. Mix a bulk writer, tiny-file
churn and sparse edits, with both idle and active owners. Target overlapping Exec
and one Commit per Workspace after the public daemon/control route supports it.
Vary active Commit slots separately from attached Workspace count and cache size.
Include quiet long-running Exec, explicit cancel/disconnect and descendant/pipe
cleanup as separately registered functional lifetime proofs. #249 removes the
elapsed whole-shell 30-second limit; syscall/Commit and benchmark bounds remain
distinct. A command can straddle capture and is not a filesystem transaction.

**Required behavior.** Independent identities, output, roots and errors remain
isolated. Working sets have exclusive leases funded by a global byte ceiling;
unavailable capacity gets declared bounded admission or an upfront refusal.
Idle base/reference and protected completion state are counted. Large streams
cannot silently starve control/metadata/cleanup. Per-Workspace admission does
not establish a host RSS or cgroup bound on its own.
Retained local failures do not supply a daemon-wide Commit mutex. Actual shared
Store/allocator quarantine and same-Branch conflict retain their proper scope.
Strict sibling filesystem and child RSS/PID/CPU isolation requires a supported
runtime profile; cwd, output caps and process groups alone do not establish it.
Detached descendants retain defined execution custody.

**Observe.** Simultaneous live allocation capacities, Server codec/engine/shared
indexes, physical cache, command process groups/output, blocked/admitted slots,
fairness and cleanup. Preserve the registered construction-worker policy; no
extra worker or helper lane is added to make a gate pass. Current host frozen
and content-call serialization are implementation facts, not a concurrency PASS.

## Coverage of the pinned #245 cases

| Historical case | Retained scenario coverage |
| --- | --- |
| Many packages | SC-03 distributed refresh |
| One large package | SC-03 concentrated names/asset; SC-04 payload flow |
| Move and replace | SC-05 file/directory moves and temporary replacement |
| Remove and copy | SC-05 deletion, recursive acquisition and custody |
| Tiny edit in large existing file | SC-02 sparse range and untouched-page work |
| Large existing-file edit | SC-02 dense replacement; SC-01 surviving intervals |
| Large fresh file and append | SC-04 distinct new/existing routes |
| Shell-driven prepend/insert | SC-04 complete command IO and replacement |
| Printed logs versus a Workspace log file | SC-07 output control; SC-04 file append |

## Current limits and evidence boundaries

Re-audit these at each changed implementation identity; do not copy dated limits
forward as architecture laws.

The proposed architecture's
[constraint inventory](core/docs/architecture/proposal/bounded-memory-commit-20260930.md#constraints-removed-re-scoped-and-retained)
distinguishes population-sized resident scratch and daemon-wide logical gates
from resource admission and separately reviewed file/stream/count profiles.
Planned removal is not current implementation or qualification evidence.

| Current baseline observation | Interpretation |
| --- | --- |
| Logical single file 4 GiB; SaveFile body 8 GiB | Separate logical and wire ceilings; larger-file scenarios need a future profile |
| One native WRITE/replacement 8 MiB; FUSE write/read request buffer 128 KiB | An application's large operation can span calls; one-call and multi-call semantics differ |
| Prepared stream 256 MiB; default Server declaration count 65,536 | Encoded size and shape admission differ from total existing namespace size |
| Default Workspace-host accounting Budget 8 MiB | Shared accounting; not per-Workspace RSS or Server memory |
| Active index maximum level 7; 128 handles; 32 view leases | Distinct structural/owner bounds; pin count does not bound selected bytes |
| TinyPack cutoff 128 B; current Local/Zero wire replacement bytes | Packing threshold and zero transfer are separate concerns |
| NODE_LIMIT 256 and MAX_AFFECTED 128 | Growth/scan units, not total node or live-edit population ceilings |

The pinned source's old 1,024-piece/256-edit/128-dirty/128-name/32 KiB request and
1,024-cookie statements are historical. Its 16 MiB Workspace/512 MiB container
configuration is also historical. Do not restore those values or silently widen
current limits to satisfy a proposed scenario. Finite existing proofs and Init
counts do not establish arbitrary mutation, listing, memory or concurrency scope.
See the [seven-family checkpoint](core/docs/issues/286/SEVEN-FAMILY-CHECKPOINT-20260930.md)
and its source-bound reuse map; current 10240 remains UNKNOWN/OWNER-DEFERRED/SKIPPED.

Model/database files and S3-style object trees are motivating shapes. This
catalog does not qualify database crash durability, locking/mmap or transaction
snapshots, or provide an S3 API. Those need their own capability contracts;
the current no-sync/no-WAL profile remains unchanged.

## Promoting a scenario to evidence

Freeze each selected variant's manifest, exact shell/script bytes, tool/image
identity, sizes/counts/seed, expected complete tree and old/pinned results,
mode/UTC-mtime/alias/symlink/absence facts, source/build/compilation identities,
cache/resource/worker policy, timers, numerical limits and proof scope before it
runs. Expected content/roots come from an independent oracle. A candidate trace
cannot create its own expected result.

Prepare pristine inputs once through existing fixture/`project_api.init`
mechanics and reuse independent validated byte copies outside post-initialization
timers. A clone does not establish cold cache. Fresh initialization/output cases
retain their required setup mode; do not add another Init runner or benchmark
framework. Run only affected prospectively selected cases. Reuse qualifying
unchanged evidence explicitly, especially Family 2 retained-history proofs.

The oracle covers the complete declared tree and bytes/identities for new, old
and selected views with checked product cleanup or exact intentional retention.
If required proof cannot fit its registered budget, retain the incomplete/unrun
verdict; do not substitute a sampled claim, enlarge a timeout or shrink the case.
Passing correctness alone proves no throughput, constant memory or cache claim.

Report complete command, Exec, Commit and cleanup separately from verification;
record actual callback counts/bytes, final extents, directory/graph/index work,
physical allocated/reserved/refunded space, simultaneous capacities, process/
engine/file-cache scope and unavailable fields. Status.write is an aggregate,
not a count of FUSE WRITE callbacks. A lifetime peak is not a phase peak. Numeric
cache eligibility and functional status remain separate.

Preserve FAIL/INCOMPLETE/INELIGIBLE/NOT_RUN and original registrations. These
scenarios extend existing #232/#243/#286 scopes, not replace failed cases. Follow
[benchmark rules](docs/general/benchmark_rules.md),
[benchmark report tables](benchmark_agent_report.md) and the applicable
[Core harness rules](core/benchmark/fs-bench-pro/AGENTS.md).
Keep the accepted history storage tolerance and reject lopsided storage-for-speed
trades. Proposed tiers and engineering targets do not rewrite historical receipts.

## Current-source pointers

- [Live file operations](core/crates/layerfs-workspace/src/filesystem/active_file.rs),
  [affected extents](core/crates/layerfs-workspace/src/backing/active/extents.rs),
  [payload/read publication](core/crates/layerfs-workspace/src/backing/active/generation.rs),
  [live index](core/crates/layerfs-workspace/src/backing/active/index.rs).
- [Commit preparation](core/crates/layerfs-workspace/src/commit/active.rs),
  [reconciliation](core/crates/layerfs-workspace/src/commit/active_reconcile.rs),
  [lifetime/compaction](core/crates/layerfs-workspace/src/backing/active/lifetime.rs),
  [Host admission](core/crates/layerfs-workspace/src/runtime/host.rs).
- [Wire limits](core/crates/layerfs-bridge/src/contract/request.rs),
  [prepared input](core/crates/layerfs-bridge/src/contract/prepared_stream.rs),
  [Server declaration admission](core/crates/layerfs-server/src/service/save/prepared.rs),
  [Exec output](core/crates/layerfs-bridge/src/contract/execution.rs).
