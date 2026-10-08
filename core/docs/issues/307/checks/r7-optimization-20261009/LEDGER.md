# R7 optimization ledger

> **Status:** Current planning checklist; no release candidate exists.

Dispatch: owner direction 2026-10-09, local `main`, one checkout. Start commit
`2f8e1ec422a40f4220c40315867c72dd3463ff28`; initial product identity `2b4dc28a6`.
All timing selections are exploratory and admission-ineligible. Global Store is
explicit Disposable/WAL/OFF; Durable is **NOT_RUN — disabled by owner until
explicit reauthorization**. Overlay remains MEMORY/OFF/EXCLUSIVE. No push,
publication, PR, worktree, predecessor retirement or reference retirement.

## Stage state

Stage 0 IN_PROGRESS. Three disjoint subagent tracks build prospective registry,
SDK real-binary lifecycle/fixture harness, and passthrough/per-file residency
tools. Lead owns every command receipt, timed sample, decision, record and commit.
Only one Cargo/Docker/test/measurement command holds the existing checkout lock.

| Iteration/checkpoint | Identity | Cells and ratios | Candidates/gates | Next step |
| --- | --- | --- | --- | --- |
| 0, preparation | initial identity above | No sample; ratios unavailable until Stage 0 | No kept product change. Stage 0 diagnostics access gap identified by source, not a measured optimization finding | Build release daemon, finish authentic harness, prepare full fixture once, prove lifecycle, expose mandatory counters before baseline |

## Prospective structural floors

These are direction bounds derived before optimization, not achieved results.
Exact per-opcode/capture-window accounting will refine them before each candidate.

| Ratio | Historical start (not a current timed sample) | Floor/model |
| --- | --- | --- |
| Owner jobs/FUSE request | about 5 | One bounded atomic owner job when overlay state is required; zero when already-held authenticated state suffices. Necessary last-owner release still counted, never hidden |
| Store readers/wholly-local READ | 1 | Zero; local composition must establish no immutable-base demand before reader admission |
| Object demands/cold request | about 1 separately per neighbour | One grouped acquisition per existing bounded admitted plan; canonical dependency/authentication reads accounted separately |
| Owner jobs/captured row | about 31 at 672 rows | Fixed jobs per existing bounded 64-row window plus necessary affected-file construction and publication; no point job per membership question |
| Statements/captured row | about 130 | Indexed useful-row work per bounded window and necessary guarded writes; framing per window/job, not per point |
| Mount-to-Ready/unmount jobs | startup_cost counted | Constant in unrelated root/history size; exact attach/drain/custody jobs remain |
| FUSE requests/syscall | unavailable | At most same-profile P; necessary permission/coherence work retained |
| Wait/service per class | unavailable | No wait behind unrelated work without explicit resource/dependency justification; no numeric latency target invented |

## Decisions

1. Taken under the owner's direction of 2026-10-09: use three concurrent
   disjoint harness implementation tracks and serialize executions through the
   existing checkout lock. Subagents run no timings or Git mutations.
2. Taken under the owner's direction of 2026-10-09: Stage 0 must retain missing
   SDK diagnostics as UNAVAILABLE until an additive legitimate diagnostics path
   is implemented and proven. In-process functional tests cannot replace the
   real-daemon arm. This is required instrumentation, not an accepted speed win.
3. Taken under the owner's direction of 2026-10-09: pinned-image inventory is
   read-only and network-disabled. Missing `node` is a workload prerequisite,
   never permission to silently replace E07 or invoke commands in the fixture.

## Receipts

- [000 environment](000-environment/result.json): pinned image runs, ARM64 Linux
  6.12.76-linuxkit, `/dev/fuse` character device, bash/git available, node absent.
  This is an inventory, not a mounted proof or performance sample.
- [001 fuser provenance](001-fuser-provenance/result.json): authorized patch
  verification passed; no third-party modification.
- `002-linux-release-daemon-build`: initial release build in its own target,
  offline, pinned image and repository ARM flags; pending completion.

## Preservation

The handoff's protected containers, source checkout, temporary installed Store,
untracked handoffs and multi-workspace receipt remain untouched. Removal is
limited to resources created by this stage. Existing raw evidence is immutable.

## Commit accounting

Each commit is compared using pinned `tools/production_loc.py` through
`core/target/rx-count.py`: exact first parent versus staged tree, then committed
tree confirmation. Initial combined 184297; core 118880; active core 76006;
excluded predecessors 38878; excluded integration 3996; root reference 65417.
Harness/docs/test code is excluded. No staged source change may use estimates.

## Preparation checkpoint after ca2e70400

Committed-tree LOC confirmation matches staged tree `7ec678d256a1f1554b0e3f446d2eb92b0986924f`: combined184297/core118880/active76006/reference65417, delta0. Raw `/proc` stdout includes original trailing whitespace; `git diff --check` flagged it and the raw evidence was preserved. No clean whitespace claim is made for that raw file.

Release daemon build002 passed (56892314917ns external build wall; not performance). Full fixture copy003 passed with exact pinned commit and103108files/16867directories/10070symlinks/3475776149bytes. Shape digest attests names/kinds/modes/lengths only, not full content/metadata. Review found symlink times were not copied; metadata reconciliation012 is explicit setup before Init, preserving003.

Runtime build005 passed but selected13 newer transitive registry versions and is unused. Attempt006 showed cargo generate-lockfile regenerates latest compatible even when seeded; lead's first remedy was wrong. Seeding core lock then ordinary offline metadata resolution007 preserves86registry packages with zero new/changed versions, confirmed separately. Final locked release build008 passed (12364171917ns external wall). No timing sample or product change.

Harness registry009 own tests35 passed. The stale lane-bound tests010 are retained. Registry contains38cases/222prospective arm-class selections. Duplicate untracked r7/cache.py was removed by lead before staging after assignment overlap was discovered; canonical residency observer remains r7-cache/residency.py.

Decisions taken under owner direction: class-C identical warmup conflicts E12/E13/C12 remain explicit NOT_RUN; B is L-only; exact E03 wc-only timed stream oracle remains unavailable. P's required native live ownership maps are harness custody, not kept product state. Stage0 diagnostics plan11 uses additive generic observation with no new retained state. Fresh-context agent spawn failed at thread limit; existing agents cross-review separate ownership, with limitation retained.

## Preparation after980c169e6 (product edits not yet committed)

Staged/committed LOC confirmation matches314e9a88de470b398d8e1755e6cd2f12f15ef1fc, combined184297/active76006, delta0.012metadata failed because copiedgroup differed on every visitedentry; originalchecker visiteddirectories twice146913visits.013corrected130045childentries tosourceownership, mode/mtime mismatches0. Rootgroup is also corrected.

FullhostInit015 is knownsuccessful:46933456833ns setup,130046entries, sealedStore1417285632 logicalandallocatedbytes, root2af2a43a455656611bb42ffa809b929ab938b41dcfe83c61093bd41c059a6fb7. SDKinstall knownsuccessful:1417285632bytes in9387567791ns setup; hostSQLite3.51.0/Linux3.53.2.015overall FAILED afterwards: observer attempteduid0 through nonrootSDKexecguard, before send/zeroattempt. This is a harness defect; Init/install are not repeated. Sealed file/tmp/layerfs-r7-full-sealed-20261009.sqlite and private installedmanifest/tmp/layerfs-r7-full-installed-20261009.manifest retained. Mastervolume layerfs-r7-master-20261009-980c169e6; originalcontainer25eae7f3a4c9d4b9041fda7133329dbeb34de45eecfb909f6460535c2faf9a56 observed016, explicitlystopped017 and logsretained018. No mount existed. StoreWAL0/SHM32768; overlaylogical282624/allocated268439552; daemonVmHWM12400KiB is startup lifetime only.

P/cache cross-review fixes in progress: fsync/fsyncdir one-noop outcome, one child-parent inode identity, actual2loop serving/join custody using authorized into_runner (not nonexistent run_owned), mincorevector overlap4096->8192misreported removed, realnonemptyprimitive proof added. No Pnative run yet. SDKrunner review fixes environment/protocol/concurrency/exec IDs/sidecars/observerguard; prospective runner glue in progress.

Telemetry product source pending firstcompile/tests. Lead implements route-free allocated-domain read-only Cleanup in existing Overlay/owner/Service. Typed originalSuccess.native shrinks fatBox toconcreteBox for exact finalwork with no new retainedfact. Formatter max54+R (correction of earlier50), largest55numericvalues, bound4096B/64+R. All sourceclaims unverified runtime.

Fresh-context review now available (temporarythreadlimit resolved); it found cross-channel diagnostic correlation ambiguity and13 originalMountWork fields inaccessible at post-drain Revoke/Close failure. Amendment20 plans caller-scoped transient metadata for correlation before sourcefix. Failed-prefix fields remain explicit UNAVAILABLE with exact originalcustody, no full-failure observationclaim.

## Preparation after 78edb4748

Committed LOC confirmation matches tree 7093045929a324c831bb0ef6e7636c9a9a68ea2b:
combined 184297, core 118880, active 76006, delta 0. Product remains uncommitted.
Build 022 failed missing public docs; 023 then found the lead's incorrect fixture
field; 024 found that the corrected field requires BranchId decoding. Both test
source mistakes were corrected from existing fixture/identity APIs. Build 025
passed. Invocation 026 used an incorrect guessed executable suffix and failed
before test execution; 027 uses the exact compiled path and passed. No failure
receipt is relabelled.

Proofs 027–030 passed: daemon cleanup 1 test, Overlay cleanup 2 tests, Bridge
observations 4 tests, SDK observed facade 3 tests. The cleanup point query used
one statement, 55 VM steps and zero full-scan steps beside both 64 and 256
unrelated namespaces, with actual host profile and EXPLAIN retained. Cache proof
031 passed 12 host tests with two platform skips; 032 passed all 12 on Linux.
No mounted timing or real-binary changed Commit proof has run.

P dependency resolution 033 failed because unfiltered metadata attempted an
uncached Redox-only package under offline mode. Linux-filtered locked metadata
034 passed; release build 035 is pending. This does not authorize a dependency
change. A second fresh-context reviewer spawn again hit the thread limit; the
earlier completed review remains evidence, with follow-up cross-review required.

Source review identified zero delivered per-reader Storage and global Resources
fields. Amendment 36 prospectively assigns their bounded observations, including
the exact charged observer work, before edits. This is Stage 0 instrumentation,
not an accepted optimization. Correction: inventory 000 established that node
was absent from that shell PATH, not absent from the entire pinned image.
