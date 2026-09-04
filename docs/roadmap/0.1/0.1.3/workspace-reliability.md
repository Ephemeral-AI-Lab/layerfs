# Workspace reliability

> **Status:** v0.1.3 proof plan; no implementation, measurement, or passing claim.
> **Family ID:** `workspace_reliability`. No timed cases; **12 proof recipes,
> 28 independently identified subcases**. Fault subcases are not multiplied by
> 1/10/100/500. [Shared testing rules](testing-rules.md) own common fixtures,
> source/custody, evidence, size limits, and admission.

## Contract and boundaries

Establish whole-Workspace failure containment, exact live-process publication,
orderly persistence, session behavior, and integrity detection. Every failed
operation must preserve the state promised for that operation; a failed
multi-operation tool is not an implicit transaction rolling back earlier
successful filesystem calls.

The released [storage contract](../../../versioned/0.1.0/storage-format.md)
uses `journal_mode=MEMORY` and `synchronous=OFF`. Its acknowledgement is readable
in the live local Store process; it explicitly excludes process-crash, OS-crash,
power-loss, and recovery durability. [v0.1.2 limitations](../../../../release-notes/0.1.2/README.md)
retain that boundary. A successful fsync, Commit, orderly reconnect, or daemon
disconnect proof here does not establish crash-safe durable storage. Any stronger
claim requires an explicit storage/acknowledgement decision and separate recovery
qualification; do not change the storage profile under this benchmark task.

Use one LayerStack and one Branch. Multiple processes may operate within its
one writable Workspace. Repeated publication in that same session is allowed
where named below; Branch fan-out, competing publication, Add, and conflict
history remain separate scope. Store-owner process termination is not one of
these fault injections.

## Fixture, independent oracle, and lanes

Start from one moderate 32 MiB prepared tree with hundreds or thousands of
paths, including untouched sentinels, regular files, directories, aliases,
relative symlinks, and exact metadata. Cap additional live working content at
16 MiB, counting alias path lengths and symlink targets; every subcase must
preflight a conservative **48 MiB aggregate maximum**, with no individual file
over 1 MiB. This is below the common 500 MiB file / strictly-under-1 GiB total
limits. Publication-batch recipes can use the full declared 16 MiB dirty
allowance, distributed into distinct contents, to cross production spill/batch
thresholds. If that does not cross a required threshold, qualification fails:
do not invent a larger fixture or assume size alone proves a boundary.

The repeated-publication subcase retains genesis plus three states, at most
`4 * 48 MiB = 192 MiB` of represented snapshot content. Other subcases do not
grow Commit history beyond the states explicitly named below. This keeps the
represented-history bound below 1 GiB as well as the live-tree bound.

Count temporary coexisting replacements, sparse logical lengths, hidden files,
and aliases. Never fill the host disk to trigger `NoSpace`; use a qualified
test-only lower resource limit or existing legitimate fault seam. Store/spool,
fixture-cache, logs, and oracle artifact storage have separate reported bounds.
Stream reference bytes; no extra expected tree is written into the workload.

An independent native-filesystem/model oracle derives expected paths, types,
bytes, lengths, normalized metadata, exact symlink targets, and hard-link
equivalence classes from the initial manifest and declared operation schedule.
Compare the **entire tree**, including unchanged sentinels, at each named
checkpoint and after orderly reconnect. A candidate's own root or success
receipt alone is not an independent oracle. For permitted concurrent outcomes,
freeze the allowed outcome set and synchronization protocol before execution.

Each subcase uses a fresh independent writable input unless its sequence
explicitly retains a session. Reuse code and qualified input custody, not a
mutated prior subcase's Store. Retain subcase-specific results even when recipes
share setup code. No fault injection or extra oracle probes enter performance
collection in another family.

**Short lane** targets a selected subcase in roughly 1–5 seconds after qualified
cached preparation, provisionally; its fixture verification and actual admitted
hard wall remain explicit. **Extended lane** contains heavier fault boundaries,
500 public Exec sessions, and the 600-second sustained run. Do not claim all
proofs or a complete family finish in a few seconds.

## Exact recipe and subcase inventory

Recipe rows group shared purpose and setup; the following subcase IDs are the
actual independently reported proof members.

| Recipe ID | Subcase count | Purpose |
| --- | ---: | --- |
| `workspace-admission-validation` | 5 | Invalid edits/mutations, same-owner lease, and busy admission |
| `workspace-publication-failure-retry` | 3 | Candidate, early-admission, and final-publication failures |
| `workspace-published-presentation-failure` | 1 | Publication succeeds before presentation recovery is needed |
| `workspace-dirty-lifecycle` | 2 | Dirty End/Discard and fully restored net-zero changes |
| `workspace-write-sync-failure` | 2 | Failed spool write and deferred bounded resource error |
| `workspace-runtime-termination` | 2 | Explicit workload cancellation and daemon-route disconnect |
| `workspace-descendant-integrity` | 2 | Corrupt and missing referenced non-root content |
| `workspace-concurrent-tools` | 2 | Parallel independent work and shared-path contention |
| `workspace-link-handle-semantics` | 3 | Hard-link aliases, symlinks, and open-file namespace lifetime |
| `workspace-metadata` | 3 | Independently attributed chmod, mtime, and xattr cohorts |
| `workspace-exec-session-reuse` | 1 | 500 sequential public Exec sessions |
| `workspace-session-continuity` | 2 | Repeated publication and actual-duration endurance |
| **Total** | **28** | **12 recipes; zero timed distributions** |

### Admission and validation: five subcases

| Subcase ID | Lane | Sequence and independent required outcome |
| --- | --- | --- |
| `workspace-invalid-sdk-edit-proof` | Short | With unrelated paths already dirty, reject one invalid range through the public SDK. Exact full live state, dirty state, head, and prior successful edits remain unchanged; valid work can still publish. Reuse existing boundary permutations instead of duplicating all invalid ranges here. |
| `workspace-invalid-namespace-proof` | Short | Attempt a directory move into its own descendant and a file/directory replacement mismatch in one prepared tree. Each rejection leaves the exact pre-call tree and prior dirty work intact. Freeze supported syscall errors; do not accept arbitrary failure. |
| `workspace-lease-lifecycle-proof` | Short | Two Clients sharing the same Store owner compete to create a writable Workspace for this one Branch. The second is Busy while the first lives; End then Client-drop paths release the lease and reusable mount placement. Reuse the existing lease test with whole-tree sentinels, rather than add a second lease implementation. |
| `workspace-open-writer-busy-proof` | Short | Hold a real-FUSE writable descriptor after known writes. Commit must return the supported Busy outcome without changing head or dirty contents. Close it, then publish the exact complete tree. |
| `workspace-live-execution-busy-proof` | Short | Hold managed execution behind an explicit barrier after a known write prefix. Commit is Busy and preserves dirty work; finish execution and publish. Use barriers, never timing races. |

### Publication and presentation: four subcases

| Subcase ID | Lane | Sequence and independent required outcome |
| --- | --- | --- |
| `workspace-candidate-failure-retry-proof` | Short | Dirty multiple path/types; fail candidate construction. The old head/snapshot and live dirty tree remain exact. Retry creates exactly one Commit; another attempt is `UpToDate`. |
| `workspace-admission-batch-failure-retry-proof` | Extended | Prove a production candidate-spill/batch boundary and at least one successfully admitted early transaction, then fail a later admission batch. No visible head/Commit points to incomplete content; old snapshot remains exact, live edits remain, and retry publishes the whole intended tree once. Previously admitted unreachable objects may remain. |
| `workspace-final-publication-failure-retry-proof` | Extended | After candidate preparation/admission, fail the final visibility transaction at its qualified publication point. Branch/head and visible Commit count remain unchanged, without partial-tree exposure. Retry publishes once and a further attempt is `UpToDate`. |
| `workspace-published-presentation-failure-proof` | Short | Fail real-FUSE resume/refresh after successful ordinary publication. Require `Created` with `presentation_failed`, the exact published tree, and one Commit. Explicit presentation recovery restores access; retry is `UpToDate` without duplicate publication. |

For all three prepublication failures, compare head/base, Commit count, the old
snapshot, and the live dirty manifest separately. Do not assert unchanged object
count after early admission: unreachable admitted objects are allowed by the
[publication contract](../../../versioned/0.1.0/specification.md). Record actual
fault reachability, transaction boundaries, and retries. A fault that never
fires is a failed qualification, not a successful proof.

### Dirty lifecycle: two subcases

| Subcase ID | Lane | Sequence and independent required outcome |
| --- | --- | --- |
| `workspace-dirty-end-discard-proof` | Short | Publish tree A, continue writing B within the same Workspace, and require Clean End to reject dirty B without losing it. Discard End removes B and all resources; reconnect exposes exact A. |
| `workspace-dirty-net-zero-proof` | Short | Perform real writes, append/truncate, and temporary namespace changes, observing each intermediate state. Restore bytes and every affected file/directory metadata field to the original manifest. Commit is `UpToDate`, with no new Commit or publication write; reopen equals the original tree. |

Net-zero means full portable state equality, not just equal regular-file bytes.
It does not duplicate the untouched clean-Commit timing curve.

### Write/sync and runtime termination: four subcases

| Subcase ID | Lane | Sequence and independent required outcome |
| --- | --- | --- |
| `workspace-short-spool-write-proof` | Short | Inject a short spool append after other paths have successful edits. Verify the failed operation's prescribed rollback, spool accounting, earlier successful data, and all untouched paths. Surface the error through the exercised API and recover/discard explicitly. |
| `workspace-deferred-nospace-proof` | Short | Inject bounded `NoSpace` through the ordinary proxy write path. Observe the error at the declared next barrier/fsync/pause, rather than treating a queued write acknowledgement as durable success. Preserve prescribed state; never report successful Commit of the intended full output after a rejected write. |
| `workspace-workload-cancel-proof` | Short | Stop a managed process group via the public cancellation API after its known write prefix reaches a barrier. Terminate owned children, finish the output receipt, preserve that completed dirty prefix, and prove no automatic publication. Explicitly Discard and reopen the old tree. |
| `workspace-dirty-runtime-disconnect-proof` | Short | Keep the Store owner alive but disconnect the managed daemon/execution route after a known dirty prefix. Require infrastructure error, termination of owned work, no accidental Commit, Discard cleanup, exact old snapshot, and a reusable lease/mount path. |

Cancellation is a user-requested workload outcome; transport disconnect is an
infrastructure error. Preserve distinct receipts and expected handling. Neither
promises rollback of earlier successful calls or survival of a Store crash.

### Referenced content integrity: two subcases

| Subcase ID | Lane | Sequence and independent required outcome |
| --- | --- | --- |
| `workspace-corrupt-descendant-proof` | Extended | After orderly close, corrupt one known referenced non-root payload object in a disposable Store copy. Read through the public Workspace path: fail closed with the supported integrity/I/O outcome, never altered bytes or fabricated zeros. |
| `workspace-missing-descendant-proof` | Extended | In another disposable copy, remove that referenced payload object. Store opening or traversal must reject the integrity failure at its documented detection boundary; never return fabricated success. Record which boundary detected it. |

Identify the object through a qualified reference transcript, not by randomly
damaging bytes and accepting any crash. The untouched input Store remains valid.
These extend existing root-corruption checks to authenticated descendants.

### Concurrent tools: two subcases

| Subcase ID | Lane | Sequence and independent required outcome |
| --- | --- | --- |
| `workspace-parallel-read-write-proof` | Short | Four workers mutate disjoint small subtrees while readers consume immutable inputs; deterministic barriers hand a closed output to a different reader/mover. Join every worker before one Commit. The entire tree equals the independent scheduled result and no callbacks, handles, or processes leak. |
| `workspace-shared-path-contention-proof` | Short | Two writers first contend with exclusive creation: exactly one succeeds and the other gets the documented already-exists error. Then publish bounded complete generations by unique-temp-file sync/close/rename while readers repeatedly open that shared name. Each single-open read is one complete allowed generation. End with a deterministic final generation, join, Commit, and verify the whole tree. |

Do not require atomicity of arbitrary overlapping writes, invent a filesystem
lock promise, or prescribe which concurrent writer wins. The shared-path
protocol has an explicit allowed outcome set. File readers must not reopen
between chunks and then mislabel a cross-generation read as a filesystem bug.

### Links and open handles: three subcases

| Subcase ID | Lane | Sequence and independent required outcome |
| --- | --- | --- |
| `workspace-hardlink-alias-proof` | Short | Create aliases across directories, write through one, observe identical bytes/inode equivalence through the others, move their parent, and unlink one alias while another survives. Replace one remaining name by a new inode and verify the surviving old alias still names its own bytes. Commit/reopen preserves exact equivalence classes and link counts, without payload copying solely for alias creation. |
| `workspace-symlink-semantics-proof` | Short | Cover relative-target resolution after the declared moves, a dangling target, and a two-link cycle in independent fixture cells. `readlink` retains exact target bytes; lookup yields expected bytes or the frozen supported missing/loop error. Commit/reopen preserves targets and full tree state. |
| `workspace-open-rename-unlink-proof` | Short | Hold a descriptor while its file is renamed, then unlinked; lookup reflects the new/absent names while the open descriptor still accesses the correct inode and prescribed bytes. Also replace a named target while an old descriptor remains open. Close all writable/read handles before Commit; removed inodes are not resurrected and final path topology is exact. |

These absorb the former `link-inode-topology.md` plan. The four link-creation
timing rows are retired, not retained as controls. Preserve canonical logical
inode equivalence; host numeric inode values need not be identical across mounts.

### Metadata: three separately reported cohorts

| Subcase ID | Lane | Sequence and independent required outcome |
| --- | --- | --- |
| `workspace-metadata-chmod-proof` | Short | Change prepared files, an alias group, and a directory to declared supported modes, including 0640 for the target file. Verify exact portable modes, alias coherence, unrelated metadata, and reopen. Retain supported permission/access checks; do not claim unimplemented ownership/ACL semantics. |
| `workspace-metadata-mtime-proof` | Short | Set a file and directory mtime to 1700000013.123456789. Verify exact seconds/nanoseconds, alias observations where applicable, unrelated metadata, and reopen. |
| `workspace-metadata-xattr-proof` | Short | Set `user.layerfs-v013=mixed-proof`. Require exact round-trip if supported, or the precise documented unsupported result with no mutation. Freeze the capability/errno contract before admission; arbitrary failure does not pass. |

Share fixture/oracle code across these cohorts while retaining all three
outcomes. Do not add separate latency families or a benchmark-only xattr API.

### Reuse, repeated publication, and sustained activity: three subcases

| Subcase ID | Lane | Sequence and independent required outcome |
| --- | --- | --- |
| `workspace-exec-500-proof` | Extended | Run exactly 500 sequential public Exec calls in one Workspace. Each reads input, replaces one fixed small result, emits bounded output, completes its receipt, and releases its reader before the next call. Final Commit/reopen verifies the final result and full tree. |
| `workspace-repeat-publication-proof` | Short | In the same Workspace publish three distinct small multi-path states in order. Verify every checkpoint through its pinned Commit, continue writing after each publication, and finish with `UpToDate`. Head/parent order, exact earlier states, current tree, and bounded rebase resources must agree. |
| `workspace-sustained-600s-proof` | Extended | Exercise the same Workspace continuously for at least 600 seconds with a fixed bounded working set, cycling read/write/atomic replacement/rename/temporary cleanup and deterministic inter-worker handoffs. No intermediate Commit. At the deadline finish the current cycle, join workers, publish once, and verify the receipt-derived final tree and unchanged background. |

The 500-Exec proof measures lifecycle count, not elapsed-time endurance. The
600-second proof measures actual duration: record monotonic start/end,
completed cycles, operations, successful bytes, peak/current resources, and
cleanup. No idle sleeps or a stalled process count as active coverage. Freeze
a nonzero progress/stall threshold and the extended hard wall during baseline
qualification. Reuse a small file set, replace rather than append unbounded
logs, and retain a bounded external operation digest/receipt; timestamps and
actual cycle count are evidence, not a fabricated fixed count.

Leave one bounded final result containing the completed-cycle identity and
derived bytes so the final Commit publishes an independently checkable change;
do not accidentally finish with only transient operations and an unchanged tree.

The sustained proof deliberately has one final Commit. The brief three-state
proof independently checks repeated publication and earlier snapshots. Neither
turns 500 fast epochs into a duration claim or introduces unbounded history
growth. Candidate/root and physical Store growth remain measured separately.

## Existing code to reuse and missing integration

The sources below establish existing coverage and intended seams, not claims
that these new whole-Workspace subcases already exist.

| Existing source | Reuse and extension |
| --- | --- |
| [Workspace file-edit tests](../../../../crates/layerfs-workspace/tests/file_edit.rs), groups 3–7 | Reuse alias, invalid-edit, rollback/retry, discard, and composition checks. Extend the complete dirty manifest and multi-batch boundary instead of cloning every one-file permutation. |
| [SDK lease tests](../../../../crates/layerfs-sdk/tests/leases.rs) | Reuse same-owner exclusion and End/Client-drop release; qualify with the shared whole-tree sentinel fixture. |
| [Lifecycle implementation](../../../../crates/layerfs-workspace/src/lifecycle.rs) and [worker admission](../../../../crates/layerfs-workspace/src/worker.rs) | Commit Busy, Clean End refusal, publication status, presentation recovery, writer/Exec quiescence, and continued-session rebase. |
| [Reconciliation tests](../../../../crates/layerfs-workspace/tests/reconciliation.rs), `published_commit_reports_projection_failure_and_recovers_without_recommit` | Existing publication/presentation proof is on the materialized reconciliation route; extend the ordinary real-FUSE route without requiring Branch fan-out. |
| [File I/O implementation/tests](../../../../crates/layerfs-workspace/src/file_io.rs), `short_spool_append_restores_high_water_and_piece_root` | Reuse short-append rollback and lowered policy seam; qualify real Workspace error propagation. |
| [FUSE proxy tests](../../../../crates/layerfs-fuse/tests/proxy.rs), `deferred_mutation_errors_surface_at_the_next_synchronization_point` | Reuse deferred-error expectations; current fixture is a proxy mock, so it alone does not establish end-to-end publication behavior. |
| [Live Docker tests](../../../../crates/layerfs-sdk/tests/live_docker.rs) | Reuse post-attach failure, disconnect, mount/process/spool/output cleanup, and lease reuse. Add nonempty prior/dirty state and extended reuse. |
| [SDK client](../../../../crates/layerfs-sdk/src/client.rs) and [execution](../../../../crates/layerfs-workspace/src/execution.rs) | Use public Exec/output/stop methods, real receipts, and owned-process termination. |
| [Namespace implementation](../../../../crates/layerfs-workspace/src/cow_tree.rs) and [live FUSE tests](../../../../crates/layerfs-sdk/tests/live_fuse.rs) | Reuse alias, pin/unpin, rename/unlink, coherent file and cache tests. Add declared whole-Workspace handle/parallel sequences. |
| [Store tests](../../../../crates/layerfs-layerstack-store/tests/v4.rs), `visible_missing_and_same_length_corrupt_objects_are_integrity_errors` | Extend root integrity coverage to referenced payload descendants; keep existing CAS and schema tests without new scale variants. |
| [Store fault injection](../../../../crates/layerfs-layerstack-store/src/schema.rs) | Existing statement failures are debug/test seams. Qualify the actual later-batch and visibility points; report any missing seam as implementation work rather than assuming the fault was exercised. |

## Acceptance and implementation boundaries

- All 12 recipes expand to exactly the 28 IDs above: **22 short-lane and six
  extended-lane subcases**. No fault-by-size Cartesian multiplication.
- Every subcase has a frozen state/error oracle, input and source identity,
  reached-boundary evidence, transient size bound, and admitted runtime budget.
- Verification covers complete live/committed/reopened trees as appropriate,
  earlier successfully acknowledged calls, allowed failures, and cleanup.
- A missing injection seam or unsupported semantic contract is an explicit
  qualification gap, not a passing test or permission to bypass integrity.
- Store crash/power-loss survival remains unproven and outside this contract.
