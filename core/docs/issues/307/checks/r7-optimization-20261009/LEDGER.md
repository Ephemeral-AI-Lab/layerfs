# R7 optimization ledger

> **Status:** Current planning checklist; no release candidate exists.

Dispatch: owner direction 2026-10-09, local `main`, one checkout. Start commit
`2f8e1ec422a40f4220c40315867c72dd3463ff28`; initial product identity `2b4dc28a6`.
All timing selections are exploratory and admission-ineligible. Global Store is
explicit Disposable/WAL/OFF; Durable is **NOT_RUN — disabled by owner until
explicit reauthorization**. Overlay remains MEMORY/OFF/EXCLUSIVE. No push,
publication, PR, worktree, predecessor retirement or reference retirement.

## Stage state

RESUMED 2026-10-09 by owner instruction, Stage 0 IN_PROGRESS under a new lead (see [Owner resume](#owner-resume--2026-10-09) at the end). The earlier state was STOPPED_BY_OWNER 2026-10-09 at Stage 0; see the [owner-stop handoff](../../HANDOFF-R7-OWNER-STOP-20261009.md). The following preparation description is historical. Three disjoint subagent tracks build prospective registry,
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

## Preparation after 2ac7cc762

LOC confirmation matches its staged tree 85248cd89e2ce23b438413e118102383c2a0c861,
with unchanged combined/core/active/reference totals and delta 0. The resource
observation amendment was committed before its assigned implementation.
An explicit separate observer-cost section 30 preserves section 0; this makes
the complete schema 56+2R records, still below its declared 64+2R cap. It reports
original admitted/completion state and actual JobWork, preserving unavailable
observations and dropping the completed observer credit before owner snapshots.

P build 035 failed because fuser's ForgetOne type is private. The harness now
uses fuser's standard individual-forget fallback, preserving lookup release
without a third-party change. Actual batch kernel requests and unimplemented
callback counts remain UNAVAILABLE, distinct from the callback-indexed array.
Build 037 passed; dependency comparison 038 found 43 registry packages and zero
new/changed version/checksum pins. Format 039 and Linux Clippy 040 passed.
Metadata resolution 034 used a Linux-musl filter solely to exclude an uncached
Redox package; inventory 041 confirms the actual build host is Linux-gnu. Builds
use that actual host and repository ARM flags. The lock pins are unchanged.

Inventory 041 found no executable node at any registered candidate path:
/opt/node/bin/node, /usr/local/bin/node or /usr/bin/node. E07's prerequisite is
unavailable in the pinned environment; no silent workload substitution is made.
Harness owning tests 042 failed to import runner due to a dedented assignment
inside try. The matrix agent fixed that source error; 044 passed all 48 tests.
Host runtime release build 043 passed; format 045 passed. No real mounted timing
or changed Commit survival proof has run. A further reviewer follow-up was
refused by the agent thread limit; preserve that limitation and cross-review.

Independent cross-review found R36-1: dropping the caller's resource Completion
does not guarantee immediate credit release, because the original publisher may
still own its Arc after waking the caller. Source/comments/docs and the external
test now state and bound that transient ownership, with no wait, retry or hidden
zero. Receipt 051 passed the earlier assertion but does not disprove the race.
R36-2: resource observation discarded exact admission/wait/failed-completion
causes. Overlay keeps only quarantine state, so the original error could be lost
after completion release. The telemetry agent is correcting this violation of
the existing 11/36 first-failure plan using the existing bounded diagnostic error
carrier; original control outcome and one reply remain separate. No integration
or blanket failure-coverage claim is made before proof/review.

Full fixture seal 048 passed in 69.678559875 seconds of external setup wall:
103108 regular files, 16868 directories including root, 10070 symlinks,
130046 entries, 3475776149 regular bytes. Every regular byte and supported
metadata matched; both set seals are
39893a14dbaa44cb6f3e657f466cd1d63c87f9de6378bb41da4c23175a3d8eb6.
Raw inventory /tmp/layerfs-r7-full-fixture-seal-20261009.jsonl has SHA-256
90cde1a4647d26ae3ad4971eb91c7bde0f68cda156025520a9fbc21fbc711840.
No alias database was needed, no source/copy write or Git invocation occurred.
This is setup verification under source quiescence, not a native snapshot or
cold-residency claim. Host resource build 050 passed, and reader proof 052 passed
all 10 tests. Runtime Clippy 046 failed three format_collect findings; the lead
replaced per-byte formatting with one hex buffer, and 047 passed.

P release 053 passed, and owned container 054
e9847e604207558c250d7e05585e18662452c18c0a63fba95bbe6dbb1d74db7e
passed lifecycle proof 055: mutation, fresh remount, 64/128/256 long-name
directories, one plain detach per session and exact two-loop joins. Each drain
reported zero held handles; residual lookup associations were reported, then
released by destruction, without a fabricated FORGET claim. This is functional
proof, not a P performance sample. Raw artifacts were copied by 056; 057
explicitly stopped only this owned container. No protected resources changed.

Setup 058 prepared separate empty and 64 MiB big roots. Empty master volume 059
and Init/install 060 succeeded: one root entry, sealed Store 172032 logical and
allocated bytes, setup Init 16658667 ns and install 6495584 ns. Its installed
manifest is /tmp/layerfs-r7-empty-installed-20261009.manifest, volume
layerfs-r7-empty-master-20261009-2ac7cc762, and setup container
b16ac5af97bc4f0767a2360374b5c8bbfcf2d0af1de57dc41a02b8aa698681cd was
explicitly stopped by the harness. Full Init/install are never repeated.
Inspection found output-name collision risk when provision receipts share /tmp;
Events now creates an exclusive receipt-derived artifact directory before any
product effects. Old /tmp artifacts remain untouched; final runtime rebuild and
validation are pending.

R36-2 now retains exact resource admission cause/command, wait cause or original
failed Completion, plus a separate output failure in the existing diagnostic
error field. Failed completion credit stays charged; successful caller disposal
does not claim publisher credit has returned. Independent re-review found both
R36-1/2 corrected, with source-only limits for Await/unexpected response, reader
lock contention/poison and SQL corruption. Host build 062 and Linux build 064
passed; 063 passed two host custody tests; 065 passed three Linux custody tests;
066 passed all four real-binary Application tests, including distinct equal-call
scopes and simultaneous resource/output failure without hiding the original
Status result. Reader proof 067 passed all ten Linux tests.

Host Clippy 068 failed an InitialRecord layout warning reporting Control and
the whole enum as zero bytes despite an inline 208-byte Install variant. The
telemetry agent added an external actual-layout diagnostic before any lint
expectation or allocation change; build 069 is pending. No blind boxing or
third-party/toolchain change is authorized. No product instrumentation commit,
mounted L performance baseline or accepted optimization yet.

Actual-layout diagnostic build 069 passed. Invocation 070 mistakenly reused the
old feature-unified Bridge executable, selected zero tests and establishes no
layout proof despite exit zero. Lead corrected the binary from the exact build
receipt: 071 passed all five tests and printed Request112/Call120/StoreManifest208/
InitialRecord208, variant gap88. The pinned Clippy recursive-layout inference is
therefore a false positive. A narrow expectation on InitialRecord, with the
external layout guard, preserves its inline layout and introduces no allocation.
Host Clippy 073 passed. Linux Clippy 074 found a test-only equivalent comparison
lint; lead changed len+1<=4096 to len<4096, preserving the newline bound. The
scoped rerun 076 is pending. Boundary guard 075 passed 857 production Rust/SQL
files; that scan is no semantic or integrated qualification claim.

Counter extraction 072 passed all 17 owning tests. It retains original arrays,
subtracts only exact resource observer job/Startup work, and never subtracts
memory gauges, lifetime peaks or elapsed spans. Original Status State jobs are
separately charged; a new typed local-presence event will allow a known one-job
count view without inventing its SQL VM cost. Instrumented SDK spans include
acquisition/formatting/transport; an ordinary-control latency is not claimed.
N/P concurrency peers are being implemented under the owner's direction with
independent prepared roots, exact registered children, matched P sessions and
separate terminal joins. New resource/count/peer harness checks and final source,
binary, fixture, dependency and oracle seals remain before the baseline.

Linux Clippy 076 passed after the test-only bound expression correction. Current
harness suite 077 passed all 73 tests; runtime format 078 and host release build
079 passed. Release daemon 080 passed. These builds remain functional Stage0
source, not sealed performance arms; product instrumentation is not committed yet.

A new fresh-context reviewer became available after the temporary thread limit.
It found no semantic defect in the final scoped wire, one-effect/one-reply,
resource first-cause custody, reader snapshots or route-free cleanup/resource
admission. One adjacent Cleanup comment repeated the false immediate-credit
claim; lead corrected it to caller-reference disposal and possible publisher
transient. No behavior, wait, retry, allocation or allowance changed. Review
retains source-only limits for Await/unexpected responses, reader lock poison/
contention and engine corruption. Linux actual-layout runtime proof remains
pending. Review does not establish memory/disk equality, speed or complete Stage0.

The real-daemon lifecycle helper passed import check 081. It uses a separate
already-installed writable clone, new Workspace identities, ordinary unregistered
Bash, independent 9.5s verifiers, actual typed Committed, normal unmount, distinct
read-only Gone observations, fresh-mount byte/metadata/link survival, and explicit
EndSession/owned container stop. Its 60s functional scope is not performance.
The helper retains original attempted IDs and bounds a host-only failure fence;
container stop after failure belongs to the lead. Clone setup 082 is pending.

## Preparation after e19110099

Staged LOC tree d9629b4e10f9a77d16c7ed5ad254259a09c343ff has unchanged
combined184297/core118880/active76006/reference65417 and delta0; committed-tree
confirmation is pending. Only the staging plan and ledger were committed.
All product instrumentation remains unstaged, awaiting final affected checks.

Independent clone 083 copied the empty master without reflinks or sparse copy,
verified byte equality and identical 172032 logical/allocated bytes. Both hashes
were 0c3de5a5f52b54c58802d9b7d2897c3bd7e33d6ba7a402df68632eedadc7c29c.
Real release lifecycle proof 084 passed: unregistered ordinary Bash, 24 regular
payloads plus hardlink/symlink, typed Committed, normal unmount and Gone,
fresh-mount identical semantic oracle, normal unmount/Gone, explicit container
stop. Owned container 287df310723500649b2b1069d54420a451a3f5b44789ca695f07a9a1f177601c
is stopped and retained; the lifecycle clone is mutated and is never reused as
measurement input. The full fixture master is unchanged.

Proof 084 is functional, not a performance sample or cold claim. Its release
seals are runtime28044ca491920b4583cf24a9004b0904c9a4985dc49d588a5b1ccb555928bd21
and daemon3485df1b1a93e54ca36b3b1a0f3d74029035b7c9069b431944ed281046d52d2a.
Daemon logs 085 retained numeric groups. Lead argument-order mistake 086 failed
before correlation; corrected readonly check 087 mapped all10 groups to8
successful SDK events. Artifact retention 088 copied the complete proof and
numeric validation without rerunning any operation. Source/receipt controls
retain actual root exit and observer Exec-ID limits.

Linux layout build089 and proof092 passed all5 tests, with the same actual
112/120/208/208-byte layouts and gap88. Runtime Clippy090 passed. Source review
identified the real fresh-container helper/native staging gap; plan91 commits
its precise correction before harness edits. Matrix agent now implements it;
setup/native manifests must be actual and sealed before baseline predicates.

## Stage 0 instrumentation checkpoint b8d76c0a3

The scoped observation/cleanup/resource implementation is committed locally at
b8d76c0a3. Exact production LOC comparison: 184297 ->185857 (delta+1560), core
118880->120440, active76006->77566; predecessors38878/integration3996/reference65417
unchanged. The pinned counter compared exact e19110099 first-parent and staged
tree f657795b72ab1b095ad2c778cb19da99dff5b7b6; committed confirmation is pending.
This is required instrumentation, not an accepted speed optimization or migration.
No matrix timing or optimization iteration has run; candidate floors remain open.

Ordinary wire golden proofs093/094 passed5tests each. Linux cleanup proof095
passed2tests at64/256rows: one query,47VMsteps,zero full scans with actual
SQLite3.53.2 profile/plan (host028 was55VMsteps). SDK Linux097 passed3tests.
Final format check096 caught the newly added layout test's formatting;098 fixed
only formatting and099 passed. Clippy73/76, boundary75, actual layout71/92,
resource63/65, observedApplication66, readers52/67, cleanup27/28/95 and release
lifecycle84 supply affected scope. Full active suites and final matrix/scaling
remain later R7 obligations; no CI or qualification claim is made.

Committed-tree confirmations are now complete: e19110099 matched
 d9629b4e10f9a77d16c7ed5ad254259a09c343ff at delta0; b8d76c0a3 matched
 f657795b72ab1b095ad2c778cb19da99dff5b7b6 at combined185857/core120440/
 active77566, delta+1560. Product tree6a03560dfecd7e322c5592a3ef0673ff190aef2a.

Big master setup100/101 succeeded once: volume
layerfs-r7-big-master-20261009-b8d76c0a3; sealed
/tmp/layerfs-r7-big-sealed-20261009.sqlite and manifest
/tmp/layerfs-r7-big-installed-20261009.manifest. Two entries, 64MiB zero payload,
root5f36ba23d25f15667438edc437beddb728669e0e66d8450ebf2b8e9486812521;
172032 logical and allocated sealed bytes. HostInit121042250ns,
SDKinstall7062667ns, complete setup1701441792ns. Compression of this zero
payload is an input fact, not representative large-file storage evidence.
Owned setup containerc7977fcc74272540e0e67b79a90ac3f02c979425108745757809d720b8206429
was explicitly stopped by the harness. No performance selection occurred.

Taken under owner direction: recursively assign the completed product-arm
agent a disjoint harness-only prepared-input author; the matrix agent owns
container deployment and runner integration. Lead owns all actual setup,
source/metadata seals, executions and acceptance. No product optimization,
new dependency or allowance is introduced by this setup work.

Tracked preparation102 failed beforeoutput: lead incorrectly equated alltracked
paths with regularfiles. Actual kind classification103 passed:14104trackedpaths,
14090regular+14symlinks; owned-copy-only Git ls-files, nooriginalcheckoutcommand.
Sealed list/tmp/layerfs-r7-tracked-paths-20261009.json SHA
181cfc8309081511265659215075d6fc8731fc2bd59b8708fc7f2834cddfa07c.

Host all-active --all-targets --no-run104 passed263binaries at62261925375ns
buildwall. Targetcore/target/r7-host-proof is worktree-local. First24binaries
105-host-suite-000 through023 eachpassed once under100s wall bound; host-cfg
zero-testbinaries remain explicitlyzero, notLinuxcoverage. Suiteinventory
/tmp/layerfs-r7-host-test-binaries-20261009.json remains exactbuildlisted.
Known notsupplied preconditions will be listed byexacttestname: Q1 closedhuge
rootpreparation and macOS handoff explicitLinuxchildbinary. Current source
DEVELOPMENT_PROFILES useDisposable only; noDurable executed.

Frozen deployment source independentreviewfoundno semanticdefect atits declared
scope. Its anticipated untimed actioncount is2+3A+5N (asset/native roots), with
mandatory actualcontainer fullbyteverification before anymount/warmup/cache.
Retainedfixtureprojection doesnot reread hostpayload; same-size/restoredmtime
rewrite is notcaught there butactualcontainer hash gate rejectsit before sample.
Owningtest106 passed11. Actualstaging remains pending. Reviewfoundmissingactual
sourcebyteinventory/untracked gate inrunner; matrixagent nowimplementsit plus
phaseendpoint/countercompletion. No boolean sourcecleanproof or missing B/C
predicate may becomeeligible. E01 must useactuallifecycle/Goneownershipproof.

Preparedauthor independentreview found noselectedfixture defect but identified
unsupported futurehardlinked-symlink fidelity. Leadadded a pre-output refusal
ofsymlink nlink>1; selected048 haszeroaliases. Preparation107 passed once,
46999462459ns external setupwall. It reads/hashes all3475776149fixturebytes into
disjoint dependencyreplay+fullminus roots. NoStoreexecution, sourcewrite orGit.
Seals108 passed once,38146335541ns setupwall, writing sixexactroot inventories:
/code12entries/11files1908782bytes; /replay95429entries/71894files2169235378bytes;
fullminus35025entries/31215files1349267039bytes; full130046entries/103108files
3475776149bytes; empty1entry; big2entries/67108864bytes. Completepins at
/tmp/layerfs-r7-input-root-seals-20261009.json. Rootsealsetupmemory is itsown
processlifetimepeak only; fullprojection reuses048 byteswith currentmetadata,
mandatory actualcontainerhash remains. No cold claim or measurement sample.

Prospective actualSDKstagingproof uses isolatedemptyStoreclone109/110,
172032logical/allocatedbytes andequal0c3de5...hash. Fixedstagingstop300s per91;
overallsetupstop360s covers distinctstartup/ordinarynonroot9.5s/readonlybacking
snapshot9.5s/EndSession-stop phases, not a longerperformance/proof budget.
Config/plan at/tmp/layerfs-r7-staging-proof-{config,plan}-20261009.json select
/code andfull/native, zeroWorkspaceMounts/cachetreatments/performance samples.

Actualstaging111 FAILED_SETUP atoriginal8thaction verify-1 after53388568750ns
complete setup. /code fullbyte+metadata/aliasverification passed12entries;
/native refuses firstmacOS0755symlink becauseLinuxnative lstatreports0777.
Originalpath native/system/node_modules/@types/node. All10070fixture symlinks
have0755, including28fullminus and14tracked; no silentnormalization orverifier
weakening isauthorized. Nativecopy itself completedonce in50917594375ns and
partialmetadata reconciliation remains inits originalcontainer. NoMount,
cachepredicate, measuredcommand orperformance selection occurred. Exactowned
containerbaf6e1bb841d78d9b118200a27b439c16d1502616d6daf54177255afbb4d63c8
wasretained and112 explicitstopattempt issued bylead. Failedrawinputs/receipts
remain/tmp/layerfs-r7-staging-proof-20261009; no operation replay.

Taken underowner2026-10-09 strictreading: fullN/P exactnativeinput modes are
notrepresentable inthis pinnedLinuxfilesystem. Keep fullN/P selections NOT_RUN
withzero measurementattempts, retain111FAILED_SETUP. FullL Store remains intact
and canmeasured standalone/counts; empty/big controls remainrepresentable. A
separate prospectivelydeclared matchedsymlink-free reducedcut may beprepared
forallL/N/P, alwayslabelledcut withits ownStore/fixture/oracles. Neverpair reduced
controls withfullL orclaim fullE19 tracked/mutationcounts froman alteredcut.
Thisplatformlimit doesnotcloseR7 orreplaceindependentfullL work; allnondependent
work andallowedoptimizations remain required.

## Preparation after522238bd5

Plan113 andstage record committed522238bd50fc75aa1bba17cb279c84f0aa58cafc,
exactstagedtreeb02989cae045f509064d87d4099a343cd5924fc5; ProductionLOC
185857->185857(delta0), core120440/active77566/predecessors38878/
integration3996/reference65417 unchanged. Committedconfirmation ischecked
againstsamepinnedcounter. Producttree remains6a03560dfecd7e322c5592a3ef0673ff190aef2a.
112 explicitstop passed; failed111container stoppedretained. Cutauthor nowowned
byproductarm; nofullStoreororiginalsource changes. Linuxrepresentation preflight
andrunner source/endpoint/counter gates areunder matrixagentownership.

Hostfullscope first36/263 buildlistedbinaries nowpassedonce
(105-host-suite-000through035). complete_installed_roots explicitlyskipped only
huge_native_namespace_is_complete_after_install becauseLAYERFS_Q1_PREPARED is
notsupplied; prepare_huge_native_namespace remainsitsoriginalignoredsetup role.
Threeother native-root tests passedincluding dense500000000bytes. macOS
handoff's explicitchildbinaryprecondition willalso beNOT_RUN byexacttestname.
NoDurableexecution, kernelFUSEclaimsfrommacOS orfullsuitecompletion yet.

A separatecold-source gap wasfoundbeforebaseline: E12/E13 classA readsnative
/replay/F andmaster.json warmed bystaging, whilecurrentLpredicate coversStore/
overlay andN/Ppredicates covernativefixture only. Theircoldreceipt mustenforce
per-fileeviction/mincore foractualReplay regularinputs additionally, withall
componentscold beforeattempt, orselection remainsNOT_RUN. No warmsetupcredit,
Store-substitution, VMdrop orfsync. ClassB/C warmthscopes mustalso beexplicit.

Linuxall-active --all-targets --no-run114 passed263binaries,113480207500ns
externalbuildwall; inventory/tmp/layerfs-r7-linux-test-binaries-20261009.json.
NoLinuxsuitebinary runyet. NoDurable executed.

Cutauthorreview found bufferedomission-output closecouldreplace originalcopy/
sourceerror. Agentcorrected to unbuffered one-write/shortwrite-refusal and
independentclosecustody, preserving priororiginalcauses; independentrereview
found noselected-sourceblocker. Cutsetup115 passed once,93275983083ns external
setupwall. Full119976entries/103108regular/16868dirs, omitted10070symlinks;
minus34997entries/31215regular/3782dirs, omitted28; replay85387entries/71894regular/
13493dirs, omitted10042. Total6994278566regularbytes read/written acrosscuts;
master.json remainsbyte-identical. Zeroaliases; sourceunwritten. Its515899392-byte
setup-processlifetimehighwater isnotdaemon/phase memory evidence. Exactreceipt
/tmp/layerfs-r7-representable-cuts-20261009/cut-preparation-receipt.json.

Leadsealinvocation116 usedguessedwrongreceiptfilename preparation-receipt.json
andFAILED beforeanyseal/output. Corrected117 readsactual cut-preparation-receipt.json
fromauthor source andisrunning, withfreshappend-onlyreceipt. No115 author replay,
failedreceipt relabel orfullsource mutation. Independentoutputseals and actual
representableSDKstaging remain prerequisites.

Oracle source-review beforebaseline: rawGitindexbytes include filesystem-specific
stat-cache fields, so native-vs-L rawindexhash isnotassumedportable. Existingraw
.git oracle scope cannot becomePASS byignoringthe mismatch silently. A prospective
scopedsemanticindex oracle decision/proof remainsneeded; affectedselections stay
unrununtilvalidinputs/oracles. Productarm nowowns onlyneworacle_prepare.py for
non-Git scoped/native-reference expectations andcomparison-performing scripts.
Observe-onlyexit0 cannotbeverificationPASS. Hostoriginal stdoutvalidation must
readactualhostartifacts, neverassumehostpaths visibleinsidecontainer. No timing
oroptimizationacceptance yet.

Independentcutseals117 nowPASS,41106647167ns setupwall; expectedset hashesmatch
allthreeactualroots. Rawinventories/tmp/layerfs-r7-cut-{full,minus,replay}-inventory-
20261009.jsonl andcompletepins/tmp/layerfs-r7-cut-root-seals-20261009.json.
FullcutSHA973d6a02b9402a6a4aba44e1e8e7a43066d67e70449b4c6e5e2f2742123f27f3;
minus460c4910f018f231c051d0ec174a8ae58b48ecd11904acb158690b568aaf5188;
replaye61f2b971af44b6ac851122e537887d6f111fade128d0bf98747fa8e6257c282.
No rewriting116FAILED,115author replay, metadata normalization orfullfixture
replacement. SeparateInit/install andactualSDKcopy proof are next.

## Preparation after0782743f3

Plan119/record committed0782743f371d8fbd8d421609ac294a3ffcb0c932; staged/committed
confirmationsmatch368fb56cd285ce6f815936b1ab4c471a0666ebf4. ProductionLOC
185857->185857(delta0), allsubtotalsunchanged; producttree6a03560d unchanged.
S8P3 permitsnewscopedsemanticindex timingoracle, withnewappend-onlyschema and
pairedverificationidentity; sourceindices/rawactualbytesstillroundtripunchanged.
Independentreview corrected unsafechecksum assumption: gitls-filessuccess alone
isnotchecksumproof; explicitdigest required, debugformatnotcompleteflagdecoder.
Newgit_index_oracle.py/tests nowownedbypriorreviewer asimplementation; independent
crossreview requiredafterfreeze. Actualowned indexDIRC2/14104/1839891 bytes,
validSHA1trailer, TREEextension68699bytes; rawSHA
068de3dd63083a2216a7d5b28259127f5ff60865ee25603ac7c958a02bcc4553.
Regularconfig.worktree235bytes SHA
6cc33b2c2a819a539e48703a4c0daa6588c61b587e451ddc29e40ef13e51e4c6;
itsconfiguredpaths are notfollowed bysourceinspection.

Linux118first8/263binaries passedonce; persistenceacquisition cases arehost-only
cfgandseveralbinariesran0tests, notredundantLinuxcoverage. allocation_filesystem
originalignoredtestrequires explicitownedincompatiblehost-share magic0x6a656a63;
notrun. Host36/263passed, restpending. Allper-binarywall100s; Linuxinner90s.
Freshvolumes120/121/122 createdonlybythisstage forcut-full/cut-minus/original-minus
masters with0782743f3suffix. CutfullInit/install123 isrunningonce atitsnewfixture
identity; fulloriginalInit/install remainsunrepeated.

Cut-full Init/install123 passed once:119976entries,
root0acca6f41f517785d85c2e917b9fa741bc4a2a7815d037dbad9e372be49518c3,
sealed1410854912logical/allocatedbytes; hostInit44880654500ns,
SDKinstall9413153416ns, complete setup56602969709ns. Installedmanifest
/tmp/layerfs-r7-cut-full-installed-20261009.manifest, volume
layerfs-r7-cut-full-master-20261009-0782743f3. Original setupcontainer
3e1f310accc4fc3f7532c62713ee54467e046f8c655c35536707e740c8fb89b3
explicitly stopped byharness. Theseare newreducedinputsetup identities, not
algorithmicstorage reduction orperformancecomparisons withoriginalfull.
Cut-minus Init/install124 isrunningonce; original-minus122volume stillempty.

Cut-minus Init/install124 passed once:34997entries,
roote4c96bea6eda1f814d7ceb1c3a9a6459951a8019bd430a29ce8fa290de714504,
772677632logical/allocatedsealedbytes; hostInit14334723958ns,
SDKinstall5077465208ns, complete setup21570710292ns. Manifest
/tmp/layerfs-r7-cut-minus-installed-20261009.manifest andcut-minusmastervolume
0782743f3suffix; originalsetupcontainer
13543dab0f270181e13ddc423c5811362b0ac0ea00430092755e55801ac83579
explicitlystopped byharness. Originalminus Init/install125 nowrunningonce at
itsdistinct full-L dependency-base identity; itpreserves28originalsymlinks and
neverpairs withsymlink-free nativecontrols. No performance samples.

Original-minus Init/install125 passed once:35025 entries including28 original
symlinks, rootdff2969975a8fd26fbc1e8038c0081a9d66a23080937642c0569b8ced8d73676;
772825088 logical/allocatedsealedbytes; hostInit14746691250ns,
SDKinstall5076763209ns; complete setup21838398459ns. Manifest
/tmp/layerfs-r7-original-minus-installed-20261009.manifest; owned container
b86f1f557ea5352b29d290d43f854ceb5ca77d51adc46b28568e533a698a6be7
explicitlystopped byharness. Thisbase supports fullL dependency scenarios only,
neverpaired with cutnativecontrols. No performance sample.

Expandedharness test126 passed118 tests in6696490375ns/90s. Fresh-contextreview
foundR7-H1 onceclaims notboundtoactualverifiedsource/binaries/user/profile;
R7-H2 failed performance wall includedverifier; R7-H3 streamedP A validator
returnedbeforefreshkernelconnection assertion. Lead tooknecessary frozenfile
fixes aftersubagentfollowups/messages hittemporarythreadlimit. Actualeffective
claim nowusesverifiedsource-set and actualexecution/input/artifactbytes rather
thanfreedisplaylabels; explicitperformanceclock excludesfailedverifier time and
fixes terminalboundary beforecleanup/stop; Passertion precedesrepresentations.
Owningreview-fix127 passed124 tests in6729967875ns/90s. Additionalclaim-error
retention beforeclaimwrite is now source-corrected andawaits nextaffectedcheck.

IndependentGit-indexcrossreview (authorcrossreview, notfreshcontext) found root
leafsymlink resolvedbeforeadmission, validTREE couldincludeintent-to-add, and
validTREE couldomitindex-derivedimmediatechildren. Leadfixedoriginalrootleaf
nofollowadmission, ITAancestor invalidation andvalid-nodechild-completeness.
Three malformedvectors added;127includesactualowningproofs. Sourceprooflimits,
actualpinned-index9.5s observation andLraw-byteCommit/remountremainpending.
No productoptimizationorbenchmarktiming accepted.

## Recipe integration and suite continuation at 0782743f3

Stage 0 remains IN_PROGRESS: no performance sample or accepted optimization.
Lead resumed the host binary inventory at index 36, without repeating indices
0–35. Receipts 128-host-suite-036 through 095 each passed once under an explicit
100-second wall stop. Thus 96/263 host binaries have run; Linux remains 8/263.
Platform-gated zero-test binaries retain their actual output and establish no
additional Linux/FUSE coverage. Compilation identities remain the retained
104/114 builds; harness edits do not change their product source.

The lead corrected the recipe module alias, a local verification-status shadow,
and K terminal deadlines that accidentally charged previous checkpoint verifier
intervals. Every independent verification has a PerformanceClock exclusion closed
in finally. These integration changes await their affected checks.

Fresh-context review of the new recipe integration found three further gaps:
closed expected/helper/script hashes were not bound to actual deployed operands;
workload/environment/input identities were insufficiently checked; and an empty
expected list could skip mandatory tree comparison. The reviewer now owns only
verification.py and its external tests to enforce these checks. Its implementation
will require another independent review. No affected selection has been sampled.

K cumulative count intervals include checkpoint verifier filesystem work. The
lead added original command-end Status endpoints before each Commit and original
after-verifier endpoints within the separate verifier interval. Only command:i
to commit:i can attribute isolated Commit work; cross-checkpoint deltas remain
cumulative and cannot be used as Commit-only ratios. Status/Resources costs use
existing exact count subtraction; no verifier request cost is guessed away.
The current source still needs actual K checkpoint/fresh-mount proof.

Recursive assignments: product_arm extends the original K02/K03 argv and E12/E13
expectations and authors oracle tests; stage0_final_review authors fresh small
code/deployment inputs reusing closed large-cut inventories; stage0_harness_review
implements its concrete loader findings. All work is disjoint, with execution,
records, Git, LOC, source acceptance and measurements retained by the lead.

## Closed-oracle and matched staging checks, receipts 129–138

Host suite now has 116/263 binaries passed once, including 128 indices 096–115.
`host_handoff` skipped only `macos_init_handoff_to_linux_daemon_has_no_host_data_path`
because its explicit Linux child-binary precondition is absent. Linux remains
8/263; no product source change or performance sample has occurred.

129 passed 135 harness tests in 6978138125 ns (90-second stop). 130 passed 26
oracle-author tests in 170052041 ns. New independent IO-custody review then found
known-manifest/payload and JSON reads could lose a prior error through close.
The author and shared oracle now preserve original errors and append independent
close failures; shared directory enumeration forwards walk errors instead of
quietly emitting a partial manifest. Generic stdlib directory-close custody is
not claimed. Source proof does not establish the actual 9.5-second fixture bound
or the cost of unbuffered manifest rows.

131/132 created an independent empty-Store clone in
`layerfs-r7-cut-staging-clone-20261009-0782743f3`; source/destination SHA
0c3de5a5f52b54c58802d9b7d2897c3bd7e33d6ba7a402df68632eedadc7c29c,
172032 logical and allocated bytes. 133 authored fresh small code inputs once in
29509031000 ns, with 13 files / 1973379 bytes and zero large-tree copies or Init.
Code set 4a64a9cea11645154d68616f24d1a3b9662fe137056d5438775254430bfe80af;
code inventory SHA 9bae799eab6dd6dd9fee65c0427baf1476cf494b4232e2f87ae86b23f663b51e.

134 actual SDK staging PASSED_SETUP_ONLY, outer 126742096833 ns and staging
123787646750 ns under its 300-second stop. Fifteen distinct once-only setup
actions verified /code14 entries, /replay85387 entries / 71894 regular files /
2169235378 logical regular bytes / 2379296768 allocated regular bytes, and
/native119976 entries / 103108 regular files / 3475776149 logical regular bytes /
3764051968 allocated regular bytes. All supported metadata and complete payloads
matched the separate cut seals. Ordinary nonroot command had known exit 0 and
zero Exec registrations. EndSession/explicit stop produced KNOWN_STOP for
20586d26778f6ac52f15323a7e6811a5fbf28269e6abcfecea1d8841f853d797.
No mount, cache treatment, reference oracle or performance sample occurred.
Raw runtime/setup outputs remain /tmp/layerfs-r7-cut-staging-proof-20261009.
Later helper source fixes require a fresh small code seal; they do not rewrite
this original successful staging identity.

135 passed 12 strengthened staging-author tests in 238317916 ns. Its first-read
custody and namespace-bijection changes are setup-only; setup resident state is
not a daemon/phase bound or an accepted optimization. 136 independently queried
the pinned image tool: /usr/bin/git SHA
6464b23aabeb8dcb55a67b68c911678041b1b62437eeddf779ac3f201f6a09c9,
known-zero `git version 2.39.5`, no fixture/Store/FUSE access.

The initial Git pin incorrectly treated absent core.untrackedCache as false.
Taken under owner 2026-10-09: retain its actual documented default keep; actual
UNTR extensions still refuse and enabled feature.manyFiles remains unsupported.
Primary pinned sources: [core configuration](https://raw.githubusercontent.com/git/git/v2.39.5/Documentation/config/core.txt)
and [feature defaults](https://raw.githubusercontent.com/git/git/v2.39.5/Documentation/config/feature.txt).
Actual version/binary/default-source receipts remain required; no override or
normalization changes the command.

137 FAILED its new bundle-root alias vector: deployment.owned_input resolved
symlinks before checking ownership, so the lead's attempted correction did not
reject the caller alias. The exact failure is retained. Lead corrected the
shared owned-input admission to reject leaf and nested symlinks before resolve
and added both alias vectors. Fresh review also found missing K02/K03 semantic
node-root/replay operand requirements, changed K checkpoints accepting UpToDate,
and verifier deadlines without a complete controller-span verdict. Lead added
operand gates, original typed Committed validation, pre-send/pre-launch expiry
checks and full verifier interval accounting. Original overdue buffered replies
retain their typed IDs/rows but cannot satisfy a budget. Every measured and warmup
command now checks actual runtime body SHA against its supplied original script.
These post-137 integration edits await affected tests and final review.

138 passed 32 oracle-author/custody tests in 259448417 ns. K reference authoring
has per-body setup stops; it does not claim cumulative K15 performance compliance.
All measurements remain unrun and Stage 0 remains IN_PROGRESS.

## Frozen acceptance and actual Git proof, receipts 139–152

139 passed 154 harness tests (6993094500 ns). 140 closed the Git2.39.5 policy
from six actual tagged primary-source byte streams in 4627716584 ns, with no
fixture/Store access. Policy SHA cef5f89a3bd96643dd2d46b4aa12f18cb529f0b8d6a65974ea36858445f385b0;
review SHA1692bf12cccce2ca502853c68ab80f67e984fe5a4dc8b53c1eed09a97eec974f.
The original version/binary receipt136 and every source byte/hash remain retained.
Actual object format and config presence/default provenance are independently
queried in the mounted proof, not inferred from source-policy labels.

141 passed172 harness/custody tests in6999056208ns;142 passed47 controller/author
tests in333146917ns. Fresh-context agent stage0_frozen_acceptance found R7-FA1:
actual deployed cache and P execution bytes lacked a tie to canonical source and
verified host binary. Lead added canonical cache path and three helper SHA checks,
selected P SHA binding, post-stage checks before warmup/cache, and cached-bytecode
refusal. 143 passed four focused tests in168875416ns. Re-review found a mapping
collision could replace a cache helper digest; lead rejected all three collisions
and added the vector. 147 passed177 tests in7053726292ns. The independent Git
proof now writes canonical E04 bytes verbatim. No benchmark sample exists.

EventProcess partial-constructor ownership is now carried on the original error.
Receipt failure cannot prevent its host-only fence; retention is idempotent, with
one signal and one bounded wait. Original failure, completed raw acknowledgements
and independent output/close/custody failures remain distinct. Git proof reuses
that owner instead of signalling/waiting again. No container Stop, unmount or
filesystem drain is inferred. Selected comparison and deployment payload IO now
preserve the first error through independent close failures. Generic all-IO or
stdlib directory-close custody is not claimed.

144/145 created full-cut independent volume
layerfs-r7-git-fullcut-clone-20261009-0782743f3. Closed master/source and destination
SHAca310bddf95399595942a0ef3a2701ecd8fcec95912f44e4cc821ea30b75f5cd,
1410854912 logical/allocatedbytes; copy/compare complete11622447000ns under15s.
146 refreshed only small code assets at its new helper identity,34330179916ns
setup, zero large-tree copies or Init; code13files1978115bytes,
set156e3faee1a98b9de11b5912564759a836b511715396282d9c11df4f0b5c2b0d,
inventory9cb4e5ba69e1a03fea4c4788e24773edd6447b850ba9e85337cd26a4d7d9d002.
Adapted L config fdd0120174ef929a03e1486e70a34e78e849df8812898464882da8a867a8cae5
references this same complete Store and has zero native materialization roots.

148 FAILED the real functional Git survival proof. Initial mounted index parsed
within801784917ns total independent controller interval (SDK743544500ns),
DIRCv2/14104 entries/1839891rawbytes, actual SHA
068de3dd63083a2216a7d5b28259127f5ff60865ee25603ac7c958a02bcc4553,
verified SHA1 trailer6b8ce99d52a8ac1d67e528c24cbf0450d4a32974. Its9701961-byte
original JSON artifact is retained without a64KiB cap. Supported mtime and
portablectime both1790682026843860200. This establishes initial observation,
not post-Git Commit/fresh-mount survival.

The original E04 command reached its9.5s stop with no complete command event.
Complete failed proof54862541625ns; outer55076629666ns. Exact retained container
3c4e958e32336e29f6dd4171af33fa6ea365c08e8b3558c9c51c7dce66c4a94c;
original Exec084aeeead5dbae7bdf7e0fc98dbdccdd94ee2b787f2aba6d9d2478bcd0e236d0.
Only hostcontroller51258 was fenced (knownexit-9). No command replay, Commit,
unmount, Gone or survival inference occurred. 149 exact-ID read-only inspection
later returned Running=false,ExitCode0,PID90359; no actual exit timestamp was
provided, and Bash exit does not prove descendant/descriptor/drain completion.
150 explicit owned container Stop passed1547871833ns; container retained stopped.
The original failed verdict is unchanged.

Narrow source diagnosis by stage0_final_review: runtime previously published
exec_start/streams/exec_status only after streamEOF and inspection. Last
exec_start_attempt therefore did not isolate the blocking stage. Failed Create
was2.286250ms; failed Start/EOF/inspect/actual-exit times are UNAVAILABLE. Earlier
same-owner Start1.816292ms/1.684833ms and stream230.649542ms/737.900875ms show
those earlier attaches returned before stream completion. Engine source makes
three exchanges(Create/Start/Inspect), headers stop at emptyCRLF, Body::new reads
nothing;8KiBBufReader permits read-ahead without requiring a fill. No transport
defect or measured extra round trip is established.

Lead authorized a scoped harness publication correction: emit each existing
completed inner phase immediately before the next blocking stage. Successful
event order/count and original operations/errors/bodies stay the same; no timeout,
retry, producer or product-source change. Interrupted attempts retain completed
phase evidence at the new harness identity. 151 fmt-check passed;152 release
rebuild is in progress and its new binary/source pins remain required.

Host116/263 and Linux8/263 full-suite binaries remain passed once; remaining
binaries pending. Git timing integration remains pending, now assigned narrowly
to E04/E18 first. Full non-Git native reference authors, actual K/W runner proofs,
final source seals and Stage0 full-matrix baseline remain outstanding. ZERO
accepted optimization iterations; Stage0 IN_PROGRESS, no completion claim.

## Continued Git closure and suite work, receipts 153–155

152 release rebuild PASSED in 1883232333 ns. New runtime binary SHA
a2280440442a486c2c9e29cc12139c6cd3ed0da18b6cd665d66cfbd563a3453c.
153 host runtime Clippy `-D warnings` PASSED in 1025234250 ns. This changes
harness observability only; receipt148 remains failed and is not replayed.

Taken under the owner's direction of 2026-10-09, stricter contract reading:
E18 class C cannot both remain an unrefreshed-index workload and run its identical
status warmup on the same mount. All three E18:C rows remain NOT_RUN, sample0,
in the separate prospective Git variant; original registry and receipts retain
their identities. No reset, refresh, normalization or revised command is used.
154 passed four selection-contract tests in124069916 ns. A/B remain pending.

Fresh-context recursive review of the frozen E04/E18 extension found a missing
standalone workloads.py import asset and a declared expected-tree seal that was
checked before actual queries but not after comparison. Both findings were
verified against source and returned to product_arm for correction, helper
closure and external tests. Lead is adding actual Git policy/query source assets
to fresh staging authoring and integrating variant selection handling. No
performance sample or post-Git raw-index survival verdict exists.

155 host suite index116 indexed_operation_record PASSED once, six tests,
1810820917 ns complete command. Host117/263 and Linux8/263 are now passed once;
remaining binaries are still required. Stage0 remains IN_PROGRESS.

## Git command wait localized; receipts 155–166

155 host indices116–135 all PASSED once; host136/263 binaries complete. The
library harnesses129–135 contain zero tests and establish only successful binary
execution, not coverage of a behavior. 159 Linux indices008–023 all PASSED once;
Linux24/263 complete. Existing build identities are unchanged. Linux13/18
confirm the previously retained Commit/cursor count trends; they are functional
count evidence, not a mounted timing baseline or a new optimization.

156/157 created independent full-cut clone
layerfs-r7-git-phase-clone-20261009-a22804404, source/destination SHA
ca310bddf95399595942a0ef3a2701ecd8fcec95912f44e4cc821ea30b75f5cd,
1410854912 logical/allocated bytes, 11635885875 ns copy/compare under15s.
Clone receipt SHA650dfec00d9d355790bb40717b5716c1d854924b1792d85f876e6eee8d82bd8e.
158 passed12 staging tests in195298417 ns. 160 passed9 Git-query tests in
442653458 ns, including isolated standalone imports and the post-query declared
tree mutation refusal. product_arm corrected both verified fresh-review findings
and froze its Git helper source before authoring; loader integration is ongoing.

161 fresh code assets PASSED_SETUP_ONLY in33915110750 ns,16files2009869 bytes,
setacd546771622716d0dc47bd192a97b7dd03460a9ad0b619e5f0a4ee17b1371c8,
inventory3c1d3dfae161e3addc6c3de5470301ea02d06a1a38e4731f42d2e5352dac6f61.
Git-query source8d63d10df5451b72c118e2a481fc833abeffe87225681074c204301334b7e0b7;
authorb93948f4f56764f1f136406190d68a5910257313ee5b9b5ae2b475b94051c6f7.
It includes the actual qualified policy1790bytes and workloads.py dependency.
No large-tree copy, Init or Store operation occurred in this author.
162 prospectively selected only /code, native[], replay[] for the separate Git
functional proof. The installed full-cut root is unchanged; that proof uses no
replay operand. Confige0c2eb891f58309dc03401b4d3f9206ec68d1658274afb781dd9dc4bcd562920.
Its stdin author body was present in the tool invocation but not captured by the
command wrapper; this is an exact-command reproduction gap, not a sealed sample.

163 FAILED the original9.5-second E04 command at the new runtime identity.
Complete proof12447015000 ns, outer12616819083 ns under explicit120-second test
stop; inner setup/command/verifier/lifecycle stops were not increased. Actual
staging615430916 ns. Initial complete index observation792113125 ns, SDK733707792
ns, same14104entries/1839891bytes/rawSHA068de3dd63083a2216a7d5b28259127f5ff60865ee25603ac7c958a02bcc4553.
New phase evidence: failed command Create2023125 ns and Start1286417 ns completed;
streamEOF and final inspection never completed in the original controller.
Exact container0faa42b0254da3a6b6c7ee96ae9c7d6afa0a66bb3f3971c4fa40b728619c3922;
Exec694dcd3d5e8a9ca4179b1dac2eaa7bfabe00a5bd04d174b9935ef135245a5e19.
Only host56891 fenced once, observedexit-9. No Commit, normalunmount, Gone or
fresh-mount survival occurred; no operation replay or performance sample.

164 exact-ID read-only inspection later found Runningfalse/exit0/PID91968.
165 fetched retained Engine events once for that exact owned container, no
command replay. Correlated Exec start1791506729188284966 and die1791506764375245385
give35186960419 ns (35.186960419s), original exitCode0, same Engine wall clock.
This establishes a real command interval exceeding the proof stop; it does not
attribute that duration to individual FUSE/owner work, establish transport EOF,
prove descendants/descriptors/mappings drained, or qualify the failed proof.
The start/stream distinction is now observed; a Start-blocking hypothesis is
refuted for163. 166 explicit owned Stop PASSED5423883541 ns; container retained
stopped. This is not a normal Unmounted/Gone or descriptor-drain proof.

Remaining work includes complete native references, actual K/W runner proofs,
mutable Git scopes, final harness/source seals, full matrix and optimization
loop. Stage0 IN_PROGRESS; zero accepted optimization iterations.

167 passed28 loader tests in898057083 ns. Lead source review confirms exact
Git variant/helper/source closure, original query stream hashes/values/PIDs,
paired pin/policy and semantic index/tree operands, generated script binding and
comparison-result operand/schema checks. These are mocked/source checks, not
actual reference or mounted verifier qualification.

Recursive controller implementation found an actual setup path incompatibility:
staging retained inventories under /tmp/r7-setup-*, whereas the reference author
admits /tmp/layerfs-r7-* or /code only. Lead changed only the new evidence prefix
to /tmp/layerfs-r7-setup-*. Historical paths/receipts remain unchanged. Native
reference authoring will run its setup helper as root to read the private0700
inventory directory; the existing helper drops every original command/query to
501:20. No private directory permissions are weakened. This corrects the lead's
earlier controller assignment that would have launched the setup helper itself
as501 and failed private inventory admission. The separate measured commands
remain ordinary nonroot unregistered Bash.

168 passed193 integrated harness tests in7622946625 ns;169 passed35 author tests
in266829500 ns. 170 host indices136–199 all PASSED once; host200/263 binaries
complete, Linux24/263. Zero-test platform/library binaries remain explicit in
their raw outputs; they are not substituted for Linux coverage.

Fresh recursive child product_arm/git_loader_review accepted the frozen E04/E18
loader at source scope with no concrete blocker. It identified remaining negative
test coverage gaps, without claiming runtime qualification. The same fresh child
then reviewed the native reference controller and found copied-author/expected
Git operand binding, complete owner close, outer-deadline admission and iterator
close-once defects. All were corrected in assigned harness files and re-reviewed.
The lead's171 check captured10 tests/PASS279176125 ns. Correction: the agent had
claimed a freeze/11 tests but subsequently edited during possible checks and
misreported intermediate counts. Source coordination was stopped explicitly;
the agent acknowledged the error, froze helpers, then completed only authorized
unfreeze work outside checks. 173 final frozen18 tests PASSED402617333 ns.
No171 result is relabelled as18-test coverage. Parent kept its turn active and
used a fresh-context child for recursive review; a root followup attempt had
previously failed with agent thread limit reached, while child spawn succeeded.

172 passed17 deployment tests in246370416 ns. Lead fixed a verified first-cause
gap: stage formerly retained only an error string then raised a new generic
ValueError. It now preserves the exact original exception/type/phase, attaches
the exact stage receipt/output, records independent receipt-output failures,
and closes selected evidence owners once without replacing earlier causes.
The native controller retains that carrier; no failure becomes success.

174 passed10 event-custody tests in175526125 ns. Lead removed buffered raw/stderr
evidence and buffered host protocol pipes, checked one original write's byte
count, refused short writes without resend, and retained the received window
before an evidence-write failure. This closes the concrete possibility of a
later buffer flush resending a failed control/evidence tail. No product code,
producer, timeout, allowance or successful operation/event sequence changed.

175 FAILED an exclusive empty-inventory seal because the proposed output already
exists. It wrote nothing and preserved that prior artifact. Lead is inspecting
and validating its existing closed empty-root inventory for declared reuse;
no repeated seal or overwritten evidence is authorized by this failure.
Mutable Git E10/E11/C12 author/loader extension is assigned separately; E19
related-root prerequisites remain pending. Actual native references and matrix
measurements still have not run. Stage0 IN_PROGRESS, zero accepted optimization
iterations.

176 FAILED before verification: lead selected container-only verify-tree for a
host root; its canonical top-level container-path guard correctly refused the
/tmp source. This was the lead's wrong API choice, not a fixture defect.
177 used the actual host closed-tree validator and PASSED94282875 ns, checking
the existing inventory's full namespace, physical identity and supported
metadata, with no new Init/reseal. Empty inventory SHA
a5695e32c5f1d0521272ea9794f5467c62a433f163aae9bfda2cc1be0bf31a33,
content/metadata set4b195491b29514bc50dd07a339b9ad03b1c05744a9a85f76b3fcc30f30bac981,
one root directory, zero files/bytes. Actual container full verification remains
required. Historical inventory and both failed attempts remain unchanged.

178 Linux indices024–033 PASSED once. Index034 complete_installed_roots FAILED
exit101 in9983603166 ns: three supported mixed/dense/sparse full oracles passed,
the preparation author was ignored, and huge_native_namespace_is_complete_after_install
panicked before fixture construction because LAYERFS_Q1_PREPARED is absent.
Lead mistakenly attached the previously required skip to target complete_root
instead of complete_installed_roots. The exact source and prior host invocation
confirm this is an unavailable external preparation, not a product failure.
This original failure is preserved; the three passing tests are not rerun.
Remaining Linux binaries resume at035. Full-suite completion must name this
precondition gap and retain the failed binary verdict, without calling it PASS.

178 remaining indices035–059 PASSED once. Linux60/263 binaries now executed:
59PASS plus the retained034 failed precondition. Device-capacity tests remain
ignored by their existing exclusive-ext4-loop requirements, named in raw outputs.
No passing binary is rerun at this product identity.

179 passed13 mutable-query tests in857044042 ns. 180 FAILED one new author vector
in196459375 ns: it supplied unregistered A to E10/E11/C12; the author correctly
refused. Only the vector changed to registeredB;181 passed40 tests293196667 ns.
The loader now separately refuses any class outside the canonical case list.
182 FAILED two of208 tests8488358708 ns because synthetic C12 tree rows lacked
device/inode fields required by the unchanged complete hardlink oracle. Only
the synthetic fixture gained coherent device1/inode2/nlink1. 184 passed208 tests
8603177708 ns. 183 passed73 reference-tools tests532242084 ns; its synthetic
fixture dependency/source snapshot is retained as invoked, not relabelled as the
later184 fixture. Neither correction weakens a workload or oracle.

Fresh child review accepted the extended frozen E04/E10/E11/E18/C12 source
closure with no concrete blocker, then separately re-reviewed the membership
gate and synthetic identity correction. It ran no tests. Mutable cases query
actual configuration without an invented fsmonitor override. E10/E11 retain
pre-body context; C12 checks actual emptiness and collects post-body context
after its one canonical init/commit body. E11 binds its actual tracked-path
asset through script/result/deployment closure; lead added corresponding native
controller input and bundle checks. C12:C retains the original registry exclusion.
E19 remains pending exact related-root/282-edit/known-Commit-or-two-root/stdout
evidence; no preparation or fast-path outcome is guessed.

The full core suites remain host200/263, Linux60/263 executed with the explicit
034 gap. Current compiled Rust identities remain unchanged. Harness checkpoint,
actual native references, K/W runner proofs, final source seals and Stage0 full
matrix still pending. No performance sample or accepted optimization iteration.

## Local commit9 — harness checkpoint

e1f6d557bf8654206867f217d189fe93042c610f commits the reviewed harness, tests,
guides and immutable campaign receipts through184. Counted tree
790a93031316f7a0ef02f664ceea81b884fbffd2. Exact staged and committed comparisons
core/target/r7-loc-08-staged.json and r7-loc-08-committed.json agree:
Production LOC185857 ->185857 (delta+0), core120440, active77566, excluded
predecessors38878, excluded integration3996, root reference65417. Pinned counter
c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb;
independent first-parent/staged/committed git archives, product Rust/shipped SQL
with inline/transitive tests, comments/blanks, harness/docs/tools/third-party
excluded. Product tree remains6a03560dfecd7e322c5592a3ef0673ff190aef2a.
No product optimization, migration or retirement is implied. Four protected
untracked handoff/confirmation files remain unstaged. Local main only, no push/PR.

Stage0 remains IN_PROGRESS. Next: seal actual relevant source bytes and build
provenance, author first native reference, exercise actual K/W runner, retain all
matrix outcomes and continue to baseline/optimization. Source review/tests are
not numerical qualification; zero performance samples and accepted iterations.

185 FAILED source admission in318491542 ns before writing a seal: the core
source-specific branch incorrectly included Finder .DS_Store metadata, bypassing
the common exclusion. Four such pre-existing files were reported; no product
source includes them. Lead excluded that exact metadata filename before the
product branch, preserving required shipped inputs of other extensions. No
metadata file was removed or staged. 186 passed7 source-classification tests in
182583125 ns. The original failed seal is retained, not silently replayed or
declared clean. This correction needs a new local harness identity before seals.

## Local commit10 and actual source seals

1b028a24a2ce69aeb18c2b99d51f582d3734a005 commits that source-classification
correction and its owning test/receipts. Staged/committed tree
517986534b4388923c435f824dfdbeb0c465e5d7 matches both exact counter records
core/target/r7-loc-09-{staged,committed}.json. ProductionLOC185857 ->185857
(delta+0), core120440/active77566/predecessors38878/integration3996/reference65417;
same pinned counter/method as commit9. No product/format/migration change.

187 actual corrected L source seal PASSED715878958 ns:994 files,
source setcf07428b908447f470e09f403847322878e6c981c254f4af4f0dea4434fa7198,
artifact12601962458c8b80dda543dcf5bf5fea8a6207098a8cf28d85293756b9cb00db.
188 N seal PASSED229299042 ns:42files,
set6a83226420d3a5ff45c0752ce6aa0b61e13a06dbc4850c0ad49da85cce0bfd45,
artifact30c3c04f17e9fd65b09f8ae57475ddffca5edfb85a48a3adaac5ce09dba24805.
189 P seal PASSED239722542 ns:115files,
set4ae98581691c8ce834682ff05ba445cf8020c02b2457b1635b2ba62fa884ca41,
artifactee3f759ee7cd069be8a775d4357f57b925ce671836a4e3c837013542482ece44.
Each independently enumerated clean tracked membership and read complete bytes;
compiled binary provenance remains a separate requirement.

190/191 fresh native C01 reference volume
layerfs-r7-native-C01-reference-20261009-1b028a24a contains an independent
writable empty Store byte copy, source/destination0c3de5a5f52b54c58802d9b7d2897c3bd7e33d6ba7a402df68632eedadc7c29c,
172032 logical/allocated bytes, copy/compare239906208 ns under15s. Clone receipt
4e44e44c074758fdbc48e941df3a9c864b0d27d53b450f27e884a1a8f0a078f4.
192 fresh code-asset author is in progress; no Init/replay/native root copy or
measurement occurs in that author. Parent/child are read-only auditing cacheA
phase claims while lead prepares the actual reference controller.

192 code-asset setup PASSED34524481125 ns (setup34381371667 ns), 17 entries/16 files/2015827 bytes, inventory bdb32a1af8814ce57db6c5e8d3ef0b1171a66ca12ad59127236230d186236285. No Init, root replay or performance sample. 193 prepared the exact native C01:B controller configuration, SHA68b0bdb25b3380b08e34ad381675b7bc640aff0665917f55baec07bca943fa9e. 194 actual reference PASSED3210051292 ns, controller3060622417 ns: original1000-file body as501:20, complete-tree observer, known EndSession and explicit Stop. Closed bundle70e22efd0cf99404183d702869b39fff7a9a8162b64c90c0171ac4c41c39c9be; all11 witness members/285758 bytes retained. Container d320ea6b5c1fe1e67707799baebfb4f586701a194c008c070e53fc53551698c1 is known stopped. No measured Workspace lifecycle or cache claim.

195 packaging FAILED182122250 ns on chown of copied closed.json. Lead incorrectly attributed it to UID0, changed only its private setup helper and196 FAILED120193667 ns identically. Actual source witnesses all501:0/mode644; group0 is unavailable to the ordinary host owner. Both partial outputs and source witnesses remain unchanged. Taken under the owner direction2026-10-09: packaging code/evidence under host501:20 is permitted setup adaptation, with exact bytes/mode/mtime; it does not alter measured fixture metadata. 197 fresh packaging PASSED188383375 ns, 30 entries/27 files/2301585 bytes, inventory84581bab622e10e94c24b78756799a3db62fd5c001fbb51f2f4d080196b4644b, setd81fc53f3a99654cc7700cd4874d2f0097f4690fd9d9a2d9c90caa982acc426f. Original closed reference SHA unchanged.

Recursive read-only cache audit found a concrete validator gap: main residency status and mandatory Store/overlay presence were not enforced. Parent owns the narrow harness correction and external negative vectors; lead runs checks after source freezes. Backing residency alone before Mount does not establish post-Mount command coldness: Mount warms immutable metadata/pooled reader metadata; startup SQL creates pager/prepared state. Initial CanonicalCache/StorageFetch genuinely start empty, operation-owned body arenas are not cross-operation caches. No measured A cold-command result may receive PASS with unavailable internal state. Existing positive file residency remains INELIGIBLE with zero attempts, not a new cache treatment. Stage0 still IN_PROGRESS; zero performance samples or accepted optimization iterations.

198 host indices200–231 and199 indices232–262 PASSED once. All263 host binaries now ran at the unchanged product source, with named preparation gaps/skips and zero-test platform binaries retained. 200 Linux060–099 and201100–139 PASSED once. Linux140/263 executed:139PASS plus original034 unsupplied-precondition failure. 202 continues140–174; no source product change or passing-test resampling.

202 Linux140–174 PASSED once, including actual mounted Commit/failure/concurrency/cycles/drain/install tests. Linux175/263 executed:174PASS plus retained034 fixture precondition failure. Fresh recursive cache reviewer accepted the narrow guards/runner annotation and the final all-counter/known-stop/zero-backing-page refusal vector at source scope, without a runtime cache claim. Parent froze receipts.py/test_registry.py/newtest_cache_contract.py (17 test methods); lead froze runner.py/test_runner.py/cache README. Child reuse for H05 was refused by tool thread limit; no child turn started, so active parent continues source-only narrowing, explicitly not a fresh independent review.

203 complete frozen harness tests PASSED:227 tests in9.433s, original outer9684673292 ns under100s. This is external harness/source validation, not a cache or measurement result. The cache guard changes no product bytes, residency treatment, allowance or timeout. Next clean checkpoint allows actual build provenance/source seals and first real runner rows; Stage0 remains IN_PROGRESS.

## Local commit11 — cache guard and complete host-suite checkpoint

af92886c735a2ebedcd6477bbb87c8c449e0f96f commits the frozen cache guard/docs/tests and campaign evidence through203. Exact staged/committed tree8f4d9f525d63e4fb35f7f6dda097d9278f8b63d1 matches both core/target/r7-loc-10 records. Production LOC185857 ->185857 (delta+0); core120440/active77566/predecessors38878/integration3996/reference65417; same pinned counter and source classification as commit10. Product tree6a03560dfecd7e322c5592a3ef0673ff190aef2a unchanged. No optimization/migration/retirement or cold qualification claim. Protected untracked files remain unstaged. Next actual source/build seals and runner rows; Stage0 IN_PROGRESS.

204 FAILED84646917 ns before source admission because lead omitted the Python module search path; no seal/operation was attempted. 205 corrected only invocation PYTHONPATH and PASSED669561042 ns:994 L files, source setd27eae914ff6f955981a740b24b44ecb4b2d12dccf20d7b5344f0f1ab54b99e0, artifacted159a63a89386cafe137587b2b8a638ca3c4c5e4baea14d8b84e46c157a080a ataf92886c735a2ebedcd6477bbb87c8c449e0f96f. 206 clean relevant-source locked/offline host release build PASSED469390750 ns, reused incremental build inputs explicitly. 207 corresponding pinned Linux release build is in progress. Previous dirty-source build receipts are retained and are not relabelled as clean.

207 clean pinned Linux release build PASSED49745550250 ns. Recompiled native dependencies and daemon; actual daemon8639784 bytes SHA1ca4d9c29a0860d08243cd7a51e29e5562e5f095aa7354e3e81b848c44d463d5 differs from prior artifact, not relabelled. 208 actual before/after relevant-byte comparison PASSED602561333 ns; locked build provenance artifacta114782463c1c41306721c6c8f6521d000ea576a8dd8e8ede07d7b8d31e53ffa, dependency inputacf6f3059d703004d3dbaec8ffddd453fa18c8027fa2de080fa8011a64d80f5e, harness3f2df751d300a559196ab15f327f35459bbddbcb18d66713fcff54a474ced0a8. Runtimea228044... unchanged; incremental reuse explicit.

209 fresh C01 L volume and210 independent empty Store clone PASSED54941000/211342750 ns; source/destination0c3de5a...172032 logical/allocated bytes. 211 sealed C01:B:L config PASSED737226791 ns including actual preflight, config071c5ec21e15fc02cb157f3ffdc009ba7ab6c321fc6fd454a3b1a6fb3ad63e5a, plan e753732a65a460ffe1cf554e2040ce95b1358f05e9a56fc94cc5f2c706690f03. All12 identities actual; code only, no materialized native root/replay; declared external VM interference unquantified.

212 FIRST ORIGINAL PERFORMANCE ATTEMPT, C01:B:L ataf92886c7, exact1000-file body; attempted1/completed0/sample1, rowFAIL. Wrapper10571476750 ns is setup/control custody, not product time. Measured Mount10174125 ns, command2027328666 ns, transport2021757916 ns nested in command, retained priced elapsed2115828542 ns excluding failed independent verifier1460650750 ns. Original command exit0/unregistered; independent complete C01 tree comparison statusPASS/differences[]/timestampsNOT_CLAIMED. Then host stdout comparison failed first guard 'original host output path required': runtime used relative stdout field because lead supplied relative CLI output; no payload comparison happened. Store213072 logical/217088 allocated bytes, overlay516096 logical/268951552 allocated bytes, daemon lifetimeVmHWM46682112 bytes. These observations do not satisfy speed/storage/memory acceptance; no measured terminal unmount/Gone. ClassB actual objectdemands0. Original custodyRETAINED; host69840 oneSIGKILL/known-9, no filesystem drain inference.

Measured command phase counters AVAILABLE:7000 opcode requests (5000 handoffs/2000inline), raw18003 ownerjobs; Resources observer subtraction leaves18002, actual typedStatusState1Lifecycle yields18001 (Lifecycle8000/Mutation3000/Read3001/Source4000). Readergrants2001; Store objectbatches/IDs0; canonical cache6033hits/0misses/0upstream/0authenticatedbytes. SQL profile/phase values retained; no scalar time adjustment. Warm-to-mount interval includes1129 maintenancejobs/3941rows/7786data bytes and must not be called isolated Mount work. This failed original sample is not an accepted baseline or optimization; original finite phase counts may guide diagnosis.

213 scoped relative-output regression PASSED35 tests/296081042 ns. Lead corrected only runner output normalization to absolute before original runtime/evidence/script owners; strict comparator and original failed fields remain unchanged. Root fresh-agent review spawn was refused by tool thread limit; no reviewer started, limitation retained. 214 explicitly stopped only the failed sample's acknowledged container8d50e8075c1ced7691b82e1dadba99b55fe4f5c7df0be2ddaaa278ac9edcaa16 once, known success5330426125 ns. This external cleanup is not original WorkspaceUnmount/Gone or a repaired212 verdict; no original operation was replayed. Stage0 IN_PROGRESS; one failed performance attempt, zero accepted optimization iterations. New harness identity and fresh independent inputs required before another C01 sample.

## Owner stop — 2026-10-09

The owner explicitly stopped R7 and requested bounded closeout. This supersedes the earlier no-pause/run-to-exit direction. No new sample, optimization, harness extension or broad test campaign began after that instruction. HEAD remainsaf92886c735a2ebedcd6477bbb87c8c449e0f96f, tree8f4d9f525d63e4fb35f7f6dda097d9278f8b63d1; no staged files or closeout commit. The uncommitted one-line stdout-path fix/test/docs and all raw204–214 receipts remain. Stage0 stopped by owner, not achieved or closed for an external blocker. One failed original sample212; zero completed/accepted optimization iterations.

Subagents stopped and returned state. Product_arm's cache/earlier oracle/controller changes are already committed; it owns no live resource/process or uncommitted product patch. Both children completed. Existing git_loader_review independently accepted the absolute-path correction in reused context, explicitly not a fresh-thread review, with no edits/execution. The root fresh-thread request and child H05 reuse attempt were refused by thread limit; no such turns started. Parent's returned H05 source reconstruction/candidates are preserved in CANDIDATES and the handoff, not implemented.

215 bounded read-only custody inventory PASSED5665804208 ns/60s:14 retained campaign-owned containers currently Running=false/Pid0/Exit137;13 retained owned volumes; recorded host PIDs5939/51258/56891/69840 absent in exact-PID ps observation. No original WorkspaceGone/descendant drain inference, cleanup mutation, resource removal or replay. Host sealedStore current stat values are recorded without rereading payloads; original provisioning/seal receipts are unchanged. Protected unrelated resources were not inspected. 216 review-state captures exact Git patch/state and an index of636 original command-wrapper receipts; wrapper PASS is not a benchmark verdict.

[HANDOFF-R7-OWNER-STOP-20261009.md](../../HANDOFF-R7-OWNER-STOP-20261009.md) records HEAD/committed and uncommitted work, checks/failures/skips, complete resource/artifact disposition, stdout first cause/frozen correction, build/source seal limits, prior per-commit LOC and smallest future steps. No closeout commit or new LOC comparison was made; latest exact committed count185857/core120440/active77566/predecessors38878/integration3996/reference65417 remains applicable to unchanged product source. Do not resume until a new owner instruction.

217 bounded documentation verification PASSED604664084 ns/15s:13 local links resolved, exact HEADaf92886c735a2ebedcd6477bbb87c8c449e0f96f unchanged, no staged files, product/Rust-runtime source unchanged. This was closeout documentation verification, not a test/sample/campaign. Final handoff/ledger closing append follows that document-hash snapshot. No long-running command, execution session or subagent work remains active. Handoff written; stop under owner direction.

## Owner resume — 2026-10-09

The owner read the owner-stop handoff and replied, in order, to three offered
steps: "1. commit", "2. yes" (one corrected C01 sample after the commit) and
"3. continue, you are taking over the R7, work iteratively for optimization".
This is the new owner instruction the stop required. It restores the assignment
in the [R7 handoff](../../HANDOFF-R7-OPTIMIZATION-20261009.md) — authority,
the two gates, the forbidden list, measurement rules and exit condition are
unchanged — under a new lead (Claude). The owner is present, so the earlier
"do not ask or pause" reading is not assumed to apply; decisions that are the
owner's are still recorded here.

Closeout commit (commit 12): the one-line absolute-output correction in
`runner.py`, its regression test, the runtime README note, both ledgers, the
candidate list, the owner-stop handoff and receipts 204–217. The four protected
untracked files stay unstaged. No check was repeated for this commit: 213 is
the scoped validation of the exact staged runner and test bytes. Product source
is unchanged, so the production LOC delta is 0.

What is still true after the commit and must not be blurred:

- Sample 212 stays FAIL with its claim and custody; it is not rerun. The next
  C01:B:L sample is a new treatment at a new harness identity with a fresh
  source seal, configuration, output and independent Store clone.
- Zero complete baseline samples and zero kept optimizations exist.
- Linux suite: 174 PASS, 034 FAIL retained, next unrun index 175.


## Commit 12 and the first complete C01:B:L sample

Commit 12 is `82780f72e19d1b5b4a7adfc9a93575241cdb5069`, tree
`264053e03ba847ea0215072ace577b05c3079059`; staged and committed LOC records
`core/target/r7-loc-11-*` agree. Production LOC 185857 -> 185857 (delta +0).

Preparation at that identity, each one attempt, all PASS:

| Receipt | What | Result |
| --- | --- | --- |
| 218 | L source seal | 994 files, set `bb2089f4…`, artifact `63c36c42…` |
| 219 | Build provenance by reuse | 963 compiled inputs byte-identical to the `af92886c7` seal; only `r7/runner.py` differs; runtime `a2280440…` and daemon `1ca4d9c2…` unchanged. No build ran |
| 220, 221 | Fresh volume and independent clone of the empty master | Store `0c3de5a5…`, 172032 bytes |
| 222 | Sealed configuration | config `a6b050ec…`, plan `e753732a…`, preflight PASS |

**223 — C01:B:L at `82780f72e`, one sample, exploratory, not admission-eligible.**
Attempted 1, completed 1. Row status INCOMPLETE with one gap: the second
declared count interval ("measured mount after warm terminal") is UNAVAILABLE
because per-connection opcode counters restart at the new mount. Everything
else is present: verifier PASS (scoped tree oracle, timestamps not claimed),
custody KNOWN_STOP, cleanup Gone, numeric correlation PASS.

| Phase | ns |
| --- | ---: |
| Mount to Ready | 9537958 |
| Command (1000 × `echo $i > f$i`) | 2226041875 |
| Streams (inside command; do not sum) | 2218097000 |
| Unmount to reply | 7247292 |
| Cleanup to Gone | 56433583 |
| Verifier (separate) | 1523316250 |
| Priced total (`complete_command_ns`) / bound | 2325269208 / 15000000000 |

Storage and memory after the phase: Store 213072 logical / 217088 allocated
bytes; overlay 557056 logical / 268992512 allocated (the allocated figure is
the 256 MiB reserved tail plus headroom, not data); daemon VmHWM 30273536
(lifetime high water).

Counted work in the measured command interval, identical to failed sample 212:

| Counter | Total | Per created file |
| --- | ---: | ---: |
| FUSE requests (5000 handed off, 2000 inline) | 7000 | 7 |
| Owner jobs (after the Resources observer credit) | 18002 | 18 |
| — Lifecycle / Source / Read / Mutation | 8001 / 4000 / 3001 / 3000 | 8 / 4 / 3 / 3 |
| Store reader grants | 2001 | 2 |
| Store object demands (class B predicate) | 0 | 0 |
| Overlay statement executions, all families | about 344000 | about 344 |

Ratios: 2.57 owner jobs per FUSE request; 2.2 ms of command wall per created
file. No N or P timing exists yet for this cell, so no gap is stated.

Sample 212 keeps its FAIL. 223 is a new treatment (new harness identity), not
a rerun.

## Owner direction during the resumed run — 2026-10-09

Received mid-run, quoted: "do it in small step, do not batch full thing";
"we want no multiple sampling, one sample is enough because we want fast
iteration"; "420s is too ridiculous" (the wrapper limit of sample 223, copied
from 212). Applied from here on: one candidate per step, one scoped check set,
one timed sample, then report; sample wrappers are bounded at 30 s. Decision
taken under this direction: the first optimization step starts from the one
complete cell (C01:B:L) instead of waiting for the full 222-selection
baseline, N/P timings and the K/W runners. Those stay open and are added
cell by cell.

## Where the C01 command time goes (from 223, no new sample)

Owner class deltas between the measured Mount and the end-of-command Status,
from the original cumulative owner scalars (indices 14–31):

| Class | Jobs | Queue wait ns | Service ns | Wait / job | Service / job |
| --- | ---: | ---: | ---: | ---: | ---: |
| Lifecycle | 8001 | 300116116 | 505001765 | 37.5 µs | 63.1 µs |
| Source | 4000 | 242837518 | 211827441 | 60.7 µs | 53.0 µs |
| Read | 3002 | 143199521 | 204160356 | 47.7 µs | 68.0 µs |
| Mutation | 3000 | 51118766 | 508570072 | 17.0 µs | 169.5 µs |
| Sum | 18003 | 737271921 | 1429559634 | | |

Wait plus service is 2166831555 ns of the 2226041875 ns command: the command
is one serial chain through the single owner thread. Overlay statements in
the interval: 342017 executions, about 342 per created file and 19 per job
(Lease 174005, Workspace 68005, Inode 22004, Begin/Commit/freelist 17001
each, DirectoryEntry 16000, Frontier 8000, Payload 3000). Service time per
statement execution is about 4.2 µs inclusive.

Per-file request sequence (opcode counters): LOOKUP, CREATE, GETATTR, WRITE,
FLUSH ×2 (inline), RELEASE. Owner jobs per handed-off request, from source:
LOOKUP source + observe ×2 (one immutable round) + release; GETATTR source +
observe + release; CREATE source + mutate ×2 (one immutable round) + ticket
release + source release; WRITE source + mutate + ticket release + source
release; RELEASE close.

Refuted hypothesis, kept for the record: the per-transaction physical
reservation (`fallocate` on every admission plus four identity observations)
was suspected as the fixed per-job cost. Measured in the pinned image on the
container filesystem (20000 iterations each, Python ctypes, lock held):
`fstat`+`lstat` 1347 ns, `fallocate` KEEP_SIZE over an allocated 128 MiB
range 1443 ns, 256 MiB 1751 ns. That is about 7 µs of a 55–63 µs job, roughly
126 ms of the 2226 ms command. It is a minor constant, not the cause; no
change was made to the reservation.

## Step 1 — C02a: one owner job for a mutation's post-reply release

Candidate: after its single reply attempt a mutation submitted two Lifecycle
jobs, `ReplyAttempted(ticket)` then `ReleaseBaseSource(source)`. They are
now one job, `Command::Replied`, running
`Overlay::reply_attempted_and_release` in one transaction. Expected count:
CREATE and WRITE each lose one job, 2 of 18 per created file (18002 → about
16002), with their Begin/Commit/freelist and repeated state statements.

Failure scope: both releases are recorded or neither; a failed job leaves the
ticket and the source with the request (previously a failed second job left
the source alone). Read-class requests and a mutation that published nothing
are unchanged.

Gates by source review: no new resident state (one enum variant carrying two
existing `Copy` tokens, Lifecycle charge unchanged); no new table, index,
column or file; the same rows are deleted by the same statements.

Checks, one attempt each, all PASS:

| Receipt | Scope | Result |
| --- | --- | --- |
| `224-step1-attempt1-source.txt` | host, overlay `source` | 5 passed; new test asserts one Begin and one Commit and both-or-neither |
| `224-step1-attempt1-fenced_port.txt` | host, daemon `fenced_port` | 4 passed; disposal jobs after stop 9 → 8, replied mutation admits exactly 1 |
| `224-step1-attempt1-linux-fenced_port.txt` | Linux | 4 passed |
| `224-step1-attempt1-linux-native_mutation.txt` | Linux, real mount | 2 passed |
| `224-step1-attempt1-linux-native_coherence.txt` | Linux, real mount | 6 passed |

Host Clippy `-D warnings` for overlay, fuse and daemon, `fmt --check` and the
boundary guard passed. One Linux build attempt stopped at the stale-source
check (Docker file sharing served an old `tests/source.rs`); the next attempt
built. Not run for this step: the full suites and Linux Clippy.

## Step 1 result — KEPT

Commit 13 is `9244dc8c624f28f9dc1ceafd70e25f149247dc46`, tree
`4b68b76bfdef5c08fd790d39fceba5828ad225e7`; LOC records `r7-loc-12-*` agree.
Production LOC 185857 -> 185918 (delta +61).

Iteration receipts 225–232 at that identity, one attempt each, all PASS: L
source seal, host runtime release build (unchanged binary), Linux daemon
release build (13.2 s incremental), build provenance, fresh volume, independent
clone, configuration, sample. They are produced by
`core/target/r7-iterate-C01.sh <first-number>`, which bounds the sample
wrapper at 30 s.

**232 — C01:B:L at `9244dc8c6`, one sample, exploratory.** Row INCOMPLETE for
the same single harness gap as 223 (second count interval). Verifier PASS,
custody KNOWN_STOP, cleanup Gone.

| Measure | 223 at `82780f72e` | 232 at `9244dc8c6` | Change |
| --- | ---: | ---: | ---: |
| Command ns | 2226041875 | 2004655833 | −221386042 (−9.9 %) |
| Mount ns | 9537958 | 8906958 | |
| Unmount ns | 7247292 | 6980792 | |
| Cleanup to Gone ns | 56433583 | 60244625 | |
| Owner jobs | 18002 | 16002 | −2000 |
| — Lifecycle | 8001 | 6001 | −2000 |
| Statement executions | 342017 | 334017 | −8000 |
| Owner queue wait ns | 737271921 | 533576694 | −203695227 |
| Owner service ns | 1429559634 | 1272420214 | −157139420 |
| FUSE requests | 7000 | 7000 | 0 |
| Store logical / allocated | 213072 / 217088 | 213072 / 217088 | 0 |
| Overlay logical / allocated | 557056 / 268992512 | 557056 / 268992512 | 0 |
| `peak_credited_bytes` | 53643 | 48565 | −5078 |
| `scheduler_bytes` | 25848 | 25848 | 0 |
| Daemon VmHWM | 30273536 | 46686208 | +16412672 |

One sample per identity: the time difference is an observation, not a
repeatability claim. The job and statement counts are exact.

Gates. Disk: equal, logical and allocated, Store and overlay. Memory: the
credited and scheduler byte counters did not rise. VmHWM is higher than in
223 but equal within 4096 bytes to failed sample 212 (46682112), which ran
the same daemon binary as 223. The lifetime high water therefore already
varied between about 30 MiB and 46 MiB with no source change; together with
the source review (no new resident state) the increase is attributed to that
existing variation and not to this change. This attribution rests on two
earlier samples, not on a controlled measurement; what makes VmHWM bimodal is
not diagnosed.

Ratios after step 1: 16 owner jobs per created file, 2.29 per FUSE request,
3.2 per handed-off request; 334 statements per created file.

## Step 2 — C04: attribute-only requests retain no FileRead

Cause, from source: every positive decision of a read-class request retained
an independent FileRead (base source kind 1, `file_read`, lease and
`native_read` rows) and Fuse released it in a separate Lifecycle job after
the reply. LOOKUP and GETATTR reply with attributes only and never use it;
only READ and READLINK serve bytes from it.

Change: `Overlay::observe_native_attributes` runs the same deciding
transaction without the read. Workspace `NativeReadOperation::Lookup` and
`Getattr` use it; a new `Data { serial }` operation (READ, READLINK) keeps the
previous behaviour. `NativeReadValue::read` is now `Option<FileRead>`. Open
and opendir are unchanged.

Expected count on C01: the GETATTR after each CREATE loses its release job,
16 → 15 owner jobs per created file, and its deciding job runs fewer
statements. Engine count test (`233-step2-attempt1-native_lookup.txt`):
statements in the deciding job, getattr 25 → 13; positive lookup 22 without
the read.

Custody: for GETATTR the kernel already holds a reference on the target and
the request's source holds its FileReader lease until release; for LOOKUP the
kernel reference is taken in the deciding transaction. No byte is served from
either reply. Gates by source review: less resident and stored state, not
more (rows that are no longer written); no new table, index, column or file.

Checks, one attempt each, all PASS:

| Receipt (`233-step2-attempt1-…`) | Scope | Result |
| --- | --- | --- |
| `native_lookup.txt` | host, overlay | 8 passed, including the new attribute-only test |
| `native_read_plan.txt`, `native_directory.txt` | host, Workspace | 4 and 3 passed |
| `native_jobs.txt`, `fenced_port.txt`, `filesystem_port.txt`, `cold_failure_scope.txt` | host, daemon | 1, 4, 4 and 2 passed |
| `linux-filesystem_port.txt` | Linux | 4 passed |
| `linux-mounted_parking.txt`, `linux-native_application.txt`, `linux-native_coherence.txt`, `linux-native_mutation.txt` | Linux, real mount | 4, 1, 6 and 2 passed |

Host Clippy `-D warnings` for overlay, workspace, fuse and daemon,
`fmt --check` and the boundary guard passed. Tests that used GETATTR to obtain
bytes now use `Data`; tests that released a lookup's read now assert there is
none. Not run for this step: the full suites and Linux Clippy.

## Step 2 result — KEPT on counts; wall time not confirmed

Commit 15 is `952e0b3bb`, tree `6e07948667d74401b8c51d8cda6a92fdffc44d5d`;
LOC records `r7-loc-14-*` agree. Production LOC 185918 -> 185969 (delta +51).
Iteration receipts 234–241, one attempt each, all PASS.

**241 — C01:B:L at `952e0b3bb`, one sample, exploratory.** Row INCOMPLETE for
the same harness gap. Verifier PASS, custody KNOWN_STOP, cleanup Gone.

| Measure | 232 at `9244dc8c6` | 241 at `952e0b3bb` | Change |
| --- | ---: | ---: | ---: |
| Command ns | 2004655833 | 2202119833 | +197464000 (+9.9 %) |
| Owner jobs | 16002 | 15002 | −1000 |
| — Lifecycle | 6001 | 5001 | −1000 |
| Statement executions | 334017 | 302017 | −32000 |
| — Lease | 174005 | 150005 | −24000 |
| Owner queue wait ns | 533576694 | 503679525 | −29897169 |
| Owner service ns | 1272420214 | 1258286490 | −14133724 |
| — Read class service ns (3002 jobs) | 189971383 | 122174715 | −67796668 |
| — Mutation class service ns (3000 jobs) | 452526206 | 559054418 | +106528212 |
| Command minus owner wait and service | 198658925 | 440153818 | +241494893 |
| Verifier ns (separate process) | 1512690958 | 1624371042 | +111680084 |
| Store logical / allocated | 213072 / 217088 | 213072 / 217088 | 0 |
| Overlay logical / allocated | 557056 / 268992512 | 557056 / 268992512 | 0 |
| `peak_credited_bytes` | 48565 | 53643 | +5078 (223 read 53643) |
| Daemon VmHWM | 46686208 | 46661632 | −24576 |

The counts dropped exactly as predicted. The command wall rose. Diagnosis
from the receipts, as the loop requires before keeping a change whose time
got worse:

- The class the change touches got faster: Read service fell by 67.8 ms for
  the same 3002 jobs.
- Work the change does not touch got slower in this run: the 3000 Mutation
  jobs execute the same statements as in 232 and took 106.5 ms longer
  (+23.5 %); the verifier, a separate process, took 111.7 ms longer (+7.4 %);
  the sample wrapper took 12.3 s against 8.9 s.
- Each receipt also holds the class-B warm-up command, the same body on the
  same daemon just before the measured one. Warm-up / measured command ns:
  223 2145483417 / 2226041875; 232 2000521833 / 2004655833;
  241 2085859541 / 2202119833. Identical work differs by 0.2 % to 5.6 %
  inside one run.

Conclusion: the rise is run-level variation on the shared Docker VM (four
unrelated containers are running; interference is declared and unquantified),
not a cost of the change. It is kept on its exact counts and on the service
time of its own class. What is **not** established: a wall-time gain for
step 2. With one sample per identity, wall differences below roughly 10 %
cannot be told from this variation; job and statement counts and per-class
service time are the usable signals for a step of this size.

New observation for triage: the command wall minus owner wait and service is
59 ms, 199 ms and 440 ms in 223, 232 and 241. That remainder is the path
back from the owner to the request task and the kernel round trip, which no
counter times. Together with a queue wait of 17–61 µs per job on an owner
that is idle between jobs, it points at thread wake-up latency per owner job
(one wake to the owner, one back) as a cost of the same order as the SQL
work. This is a hypothesis with no direct instrument yet; it strengthens the
case for fewer jobs per request and is recorded as candidate D04.

Ratios after step 2: 15 owner jobs per created file, 2.14 per FUSE request,
3.0 per handed-off request; 302 statements per created file.

## Step 3 — C03a: one Workspace state read per native check transaction

Cause, from source: inside one owner transaction the Workspace row was read
repeatedly with identical parameters. `check_native_mount` read it in
`live()` and again in `check_native_attached`; `retain_native_source` read it
a third time; `check_native_source` added the read of `source_state`; and a
deciding observation read it once more, with a second `base_source` point
read, in `source_rows`. No statement between those reads writes the fields
they use.

Change: `check_native_mount` returns the state it read;
`retain_native_source` and `check_native_source` take or return that state;
`source_held` and `source_rows_at` check and build against it. No behaviour
or error order changes: `live()` still refuses a closed Workspace first, then
the mount check, then the source check.

Expected count on C01 per created file: about 8 fewer Workspace statements in
the four source acquisitions, 9 fewer Workspace and 3 fewer Lease statements
in the three observations, and about 6 fewer in the three publishing jobs.
Engine count test (`242-step3-attempt1-native_lookup.txt`): an acquiring
transaction executes 2 Workspace statements (one read, one reader-count
update) and an attribute observation executes 1.

Gates: no state, row or file added; fewer reads only.

Checks, one attempt each, all PASS: all 22 host overlay test binaries
(`242-step3-attempt1-*.txt`); host daemon `native_jobs` 1, `fenced_port` 4,
`filesystem_port` 4, `cold_failure_scope` 2; Linux real-mount
`mounted_install` 5, `mounted_parking` 4, `native_coherence` 6,
`native_custody` 2, `native_mutation` 2. Host Clippy `-D warnings` for the
four crates, `fmt --check` and the boundary guard passed. Not run: the full
suites and Linux Clippy.

## Step 3 result — KEPT

Commit 17 is `87e234a62`, tree `cf9e05faa4e02760868c7b33df673e20eb7ab11e`;
LOC records `r7-loc-16-*` agree. Production LOC 185969 -> 185992 (delta +23).
Iteration receipts 243–250, one attempt each, all PASS.

**250 — C01:B:L at `87e234a62`, one sample, exploratory.** Row INCOMPLETE for
the same harness gap. Verifier PASS, custody KNOWN_STOP, cleanup Gone.

| Measure | 241 at `952e0b3bb` | 250 at `87e234a62` | Change |
| --- | ---: | ---: | ---: |
| Command ns | 2202119833 | 2085655209 | −116464624 (−5.3 %) |
| Owner jobs | 15002 | 15002 | 0 |
| Statement executions | 302017 | 278013 | −24004 |
| — Workspace | 61005 | 40002 | −21003 |
| — Lease | 150005 | 147004 | −3001 |
| Owner queue wait ns | 503679525 | 485224604 | −18454921 |
| Owner service ns | 1258286490 | 1166561334 | −91725156 |
| Command minus owner wait and service | 440153818 | 433869271 | −6284547 |
| Store logical / allocated | 213072 / 217088 | 213072 / 217088 | 0 |
| Overlay logical / allocated | 557056 / 268992512 | 557056 / 268992512 | 0 |
| `peak_credited_bytes` / `scheduler_bytes` | 53643 / 25848 | 53643 / 25848 | 0 |
| Daemon VmHWM | 46661632 | 46637056 | −24576 |

Statement counts fell as predicted and both gates hold. The wall difference
is inside the variation described under step 2 and is not claimed as a gain.

## Summary after three steps (all one-sample, exploratory)

| Identity | Step | Owner jobs | Statements | Owner wait + service ns | Command ns |
| --- | --- | ---: | ---: | ---: | ---: |
| `82780f72e` (223) | start | 18002 | 342017 | 2166831555 | 2226041875 |
| `9244dc8c6` (232) | 1: one post-reply job | 16002 | 334017 | 1805996908 | 2004655833 |
| `952e0b3bb` (241) | 2: no FileRead for attributes | 15002 | 302017 | 1761966015 | 2202119833 |
| `87e234a62` (250) | 3: one state read | 15002 | 278013 | 1651785938 | 2085655209 |

From start to step 3: owner jobs −16.7 %, statements −18.7 %, time inside
the owner (wait plus service) −23.8 %, command wall −6.3 %. The wall follows
the owner time only in part: the command time outside the owner was 59 ms in
223 and 199, 440 and 434 ms afterwards. That remainder has no counter. It is
the open question for the next step (candidate D04): either it is run-level
variation, or the request path outside the owner (receive loop, dispatch
worker, wake-ups, kernel round trip) got slower as the owner got less busy.
An instrument for it is needed before more wall time can be attributed.

Open, unchanged: N and P timings for C01; every other cell of the matrix;
the K and W runners; the harness count-interval gap that keeps each row
INCOMPLETE; Linux suite from index 175; Linux Clippy and the full suites at
the current identity.

## Handoff to the next lead — 2026-10-09

The owner asked for a handoff: the next lead works iteratively, with
aggressive changes, and does not stop until every benchmark cell is faster
than the LayerFS A2 column of issue #306, with storage not worse. The handoff
is [HANDOFF-R7-BEAT-A2-20261009.md](../../HANDOFF-R7-BEAT-A2-20261009.md). It
carries the A2 table, the definition of "beaten", the state above and the
exit condition. No sample, build or test was run for it.

Correction to this lead's own record. Steps 1 to 3 were worked from the R7
handoffs, this ledger and the source. The
[optimization handbook](../../../../../../docs/general/optimization-handbook.md)
and the [optimization guide](../../../../../../docs/general/optimization-guide.md)
were read only while writing the handoff. Against them:

- Within authority: step 1 added a method to an interface between two active
  crates; the original assignment allows that by owner direction.
- Not done: the handbook's scaling check (§7 step 8) and the guide's review
  record (§7) for each of the three steps. Each step has a count test and a
  cause with numbers only. Recorded as owed in the handoff.
- Not affected: no statement, index or schema was changed, so no query plan
  was required; steps 2 and 3 removed executions of unchanged statements.
- Three of the directions this lead listed for closing the gap (custody in
  owner memory, several jobs under one transaction, answers from held state)
  touch guide §4.1 and §4.2. The handoff now says so beside each.

Stage state: IN_PROGRESS, handed to the next lead at product identity
`87e234a62`. C01:B:L command 2,085.7 ms against the A2 target of 183.4 ms.

## Third lead — 2026-10-09: owner decisions delegated, research first

Owner direction (verbatim, 2026-10-09): "Work with subagents for careful
research and analysis on what cause the latency, db/sqlite statements, jobs,
transactions, fuse callback rounds. big o analysis every round, we expect
aggressive improvements, rather than minor changes. for example we could
batch multiple optimization in the same round. and they are expected to be
very very big. for example if a take 100 steps, think about how to cut it
into 10 rather than 80 steps". Later the same day: "you are in a ultra
optimization loop, do the best optimization without breaking our boundaries
of workspace per tool call, unlimited file count, file size, mutation
performed (they should be only bounded to the resource rather than the data
structure limitation). do not ask me question (i am going to sleep now), you
are the owner."

How this lead reads it (the reading is this lead's, not the owner's): design
decisions the handoff marked "owner's, proposal only" are taken here and
recorded with each use. The standing rules of the repository guides are not
lifted: Durable stays disabled, kernel writeback stays off, permission checks
stay, no third-party edit beyond the fuser patches, nothing is pushed, every
test command stays under 120 s. The product boundaries named by the owner are
gates on every change: a Workspace per tool call, and no limit on file count,
file size or mutations other than physical resources.

Harness, commit `40e5d1753`: the declared interval "measured mount after warm
terminal" spans a remount; opcode counters belong to one connection, so the
end connection's own counters are now the interval. Rows can be DIAGNOSTIC.

### Research at `87e234a62` (three read-only analyses, no run)

**Threads and hand-offs.** Per daemon: one owner thread, K = read handles + 2
dispatch workers (6), two receive loops per mount. A handed-off request with
n owner jobs costs 1 + 2n required cross-thread wake-ups: 9 for LOOKUP and
CREATE, 7 for GETATTR and WRITE, 3 for RELEASE; 35 per created file. Every
ready push, completion and receive-unit drop was a `notify_all` on the one
dispatcher condition variable (29 broadcasts per file, each waking up to five
idle workers onto one mutex). Every credit release scanned the 288-slot
admission table with one lock cycle per slot and woke the owner thread, which
found nothing to do (15 per file). The owner ran a maintenance turn every
time its queue emptied, which in a serial chain is after every job.

**Statements.** The receipt's `executions` counts trigger and cascade
sub-programs. Real statements are 233 per created file in 14 transactions
(mutate round 1 runs outside one); the model reproduces every family of
receipt 250 exactly. By purpose: 7 publish state, 15 are kernel custody that
outlives the request, 64 are per-request transient custody (source acquire
and release, decided marks, reply ticket), 72 re-validate state already read
in the same request, 33 are row reads of which about 6 are needed, 42 are
transaction framing. Each transaction also makes four identity observations
(fstat plus lstat) and one `fallocate`. The prepared-statement cache holds 48
statements and one created file's cycle uses 49 distinct texts: an LRU
simulation gives 15 re-prepares per file (no counter observes it). Each
execution pays 12 status calls and a `MEMUSED` walk of the program. All plans
are primary-key or unique point seeks; growth with the number of files is
b-tree depth only. A minimal design reaches 29 statements, 5 jobs and 3
transactions per created file.

**A2.** A2 negotiates the same kernel profile as the product (60 s
lifetimes, 128 KiB windows, background depth 1, no writeback, default
permissions) and its C01 receipt shows the same seven requests per file. Its
advantage is per-request cost: two receive threads answer inline under one
mutex, state is two hash maps and ext4 calls, FORGET, OPENDIR, RELEASEDIR
are free and RELEASE is a map removal. Its whole added cost is 17–38 µs per
request on metadata cells. Requests that can go without changing a lifetime:
FLUSH (answered `ENOSYS` once), OPENDIR and RELEASEDIR, cold-walk LOOKUPs
through READDIRPLUS, WRITE and READ counts through 1 MiB windows, and a
copy's reads and writes through `COPY_FILE_RANGE`.

Big-O at this identity: every count above is O(1) per request; the request
path has no dependence on files in the directory beyond index depth. The gap
to A2 is a constant factor of about eleven, made of four multipliers: jobs
per request (3), statements per job (15), wake-ups per job (2.3) and
per-statement overhead. The plan removes each multiplier rather than
shaving any one.

## Step 4 — D05: uncontended requests are served without a thread hand-off

Cause: C01's command spends 35 required cross-thread wake-ups per created
file because every owner job is handed to the owner thread and back and
every request to a worker; expected 0 from a model in which the thread that
received the request runs each bounded step itself while no one else needs
the connection. Evidence: receipt 250 (owner queue wait 485224604 ns for
15002 jobs on an owner idle between jobs; 433869271 ns of the command
outside the owner), source map above.

Change (commit `b313abdab`): the first step of a request runs on its receive
thread; an owner job is run by the submitting thread when nothing else is
queued and no turn is held; one idle worker is woken per queued step;
observers and the owner thread are woken only when they have something to
do; provider reads are kept off receive loops by `LeaveReceiver`.

Scaling: the change removes a fixed number of wake-ups per job and adds no
loop, statement or state that depends on files, bytes or operations. The
count test at two sizes is owed with the request-path sweep.

Gates by source review: no table, index, column or file; no limit, queue,
buffer or credit enlarged; resident state added is five scalar fields and
one thread-local flag. `scheduler_bytes` rose 25848 → 25872 (the three owner
state fields and the connection pointer).

Checks: receipts `251-batchA-*` (Fuse dispatch 9, fence 4 after the test's
hand-off moved to its own thread; attempt 1 of fence is kept as FAIL),
`252-batchA-host-*` (every daemon binary on host) and `253-batchA-*` (every
daemon binary in the pinned Linux image with real mounts). All pass except
cases whose precondition the suite does not supply:
`complete_installed_roots` (closed preparation), `host_handoff` on host
(Linux binary), `shared_processes` on Linux (named volume). Host Clippy
`-D warnings`, `fmt --check` and the boundary guard passed. Receipt 254 is a
lock-busy non-attempt of the source seal (the iterate script takes the lock
itself).

**262 — C01:B:L at `b313abdab`, one sample, exploratory.** Row DIAGNOSTIC
(first complete row), verifier PASS, custody KNOWN_STOP, cleanup Gone.

| Measure | 250 at `87e234a62` | 262 at `b313abdab` | Change |
| --- | ---: | ---: | ---: |
| Command ns | 2085655209 | 1204594500 | −881060709 (−42.2 %) |
| Mount ns | 6648958 | 6685916 | |
| Unmount ns | 6503958 | 4377000 | |
| Cleanup to Gone ns | 59707500 | 59420916 | |
| Owner jobs | 15002 | 15002 | 0 |
| Statement executions | 278013 | 278013 | 0 |
| Owner queue wait ns | 485224604 | 125972672 | −359251932 |
| — Source (4000 jobs) | 180764553 | 123029082 | |
| — Read, Mutation, Lifecycle (11003 jobs) | 304460051 | 2943590 | |
| Owner service ns | 1166561334 | 910408386 | −256152948 |
| Command minus owner wait and service | 433869271 | 168213442 | −265655829 |
| Store logical / allocated | 213072 / 217088 | 213072 / 217088 | 0 |
| Overlay logical / allocated | 557056 / 268992512 | 557056 / 268992512 | 0 |
| `peak_credited_bytes` / `scheduler_bytes` | 53643 / 25848 | 46581 / 25872 | −7062 / +24 |
| Daemon VmHWM | 46637056 | 32014336 | |

KEPT. The drop is far outside the 5–10 % run-level variation. Jobs and
statements are unchanged, as intended: this step removed hand-offs, not
work. Service time fell 22 % with the same statements, attributed to the
job running on the thread and cache that already hold the request; that
attribution is not measured.

What is left of the waiting: 11003 jobs wait 0.27 µs each; the 4000 Source
jobs still wait 30.8 µs each. A Source job is the first job of a request,
and it arrives on the second receive loop while the previous request's
post-reply release still holds the turn on the first, so it is queued for
the owner thread. Candidate D06 below.

Command against target: 1,204.6 ms against A2's 183.4 ms, 6.6 times slower.

### Step 4 review (commit `804e77f02`)

A fresh-context review of `b313abdab` found no blocker and six defects, all
fixed in `804e77f02` with the false sentences it listed: unbounded hold-back
of maintenance by a submitter that keeps taking its own turn; a panic in a
submitter's turn lost to `Owner::stop`; the connection left open after an
owner-thread panic; a hand-off counted after its first step; dispatcher
shutdown counting retained requests while a first step still ran; a futile
owner wake-up per job after a maintenance error. Receipts 263 (Fuse) and 264
(daemon) on host; the Linux run was taken with step 5 (269, 270). No sample
was taken at this identity.

## Step 5 — E01, E02, E03 and part of E04: framing and constants

Cause: C01's command spends 42 framing statements, 112 stat calls and 14
`fallocate` calls per created file because every owner job, also a read-only
one, opens a write transaction with physical admission; about 15 statement
prepares per file because the statement cache (48) is one below the cycle's
49 texts (LRU simulation, not observed); 13 status calls per execution with
one walk of the prepared program; 12 statements per file to mint six
identities. Evidence: receipt 250 and the statement analysis above.

Change (commit `9306a9073`, written by a subagent to this lead's brief and
reviewed here): a transaction and its admission begin at a job's first
writing statement; Linux establishes the reserved range only when the
tracked tail is short; cache capacity 256 over about 224 fixed texts; six
counters read and reset after execution, `MEMUSED` only for one-use
statements; identities from a counter in the connection, `next_owner`
removed, schema 20.

Owner decision used (delegated, see above): the statement cache capacity
rises from 48 to 256. It is sized to the engine's statement set, which is
fixed by source; it is not a limit that grows with data and is not the fix
for a data-dependent miss. Owner decision used: the per-transaction Linux
`fallocate` was deliberate; it is replaced by the tracked range with the
over-credit analysis in the commit's review (rollback truncation is observed
by the kept trailing observation; the file never shrinks on commit;
`auto_vacuum` is NONE).

Scaling: fixed costs per job, statement and identity removed; nothing added
depends on files, bytes or operations.

Checks: host suites of overlay, workspace, fuse, daemon (265–268); Linux
suites of overlay and daemon (269, 270); failures only the known
precondition-missing cases. Host Clippy, `fmt --check`, guard pass. No test
exists for the cache size: nothing observes a cache hit.

**278 — C01:B:L at `9306a9073`, one sample, exploratory.** Row DIAGNOSTIC,
verifier PASS, custody KNOWN_STOP, cleanup Gone.

| Measure | 262 at `b313abdab` | 278 at `9306a9073` | Change |
| --- | ---: | ---: | ---: |
| Command ns | 1204594500 | 900809042 | −303785458 (−25.2 %) |
| Mount / unmount ns | 6685916 / 4377000 | 7267792 / 5425417 | |
| Owner jobs | 15002 | 15002 | 0 |
| Statement executions | 278013 | 263010 | −15003 |
| — Begin / Commit / Startup | 14000 each | 13000 each | −1000 each |
| — Lease | 147004 | 135004 | −12000 |
| Owner service ns | 910408386 | 574973234 | −335435152 (−36.8 %) |
| Owner queue wait ns | 125972672 | 59433176 | −66539496 |
| Command minus owner wait and service | 168213442 | 266402632 | +98189190 |
| Store logical / allocated | 213072 / 217088 | 213072 / 217088 | 0 |
| Overlay logical / allocated | 557056 / 268992512 | 561152 / 268996608 | +4096 / +4096 |
| `peak_credited_bytes` / `scheduler_bytes` | 46581 / 25872 | 46581 / 25896 | 0 / +24 |
| Daemon VmHWM | 32014336 | 31477760 | |

KEPT. Service fell by more than a third. Only one of the three observe jobs
per file is read-only (the deciding ones mark their source decided), so one
transaction per file went, not three; single-visit requests (C05) remove
the mark.

Two things this sample shows that are not improvements:

- **One more overlay page (+4096 bytes logical and allocated).** At the
  instant the command ended, 64 reclamation targets were still pending (1
  in 262); the snapshot after the unmount is equal in both. Maintenance ran
  1127 turns in the command against 1135. Since step 4 a serial requester
  keeps taking its own turn and the owner thread gets a maintenance turn
  when it wins the race or after eight served jobs, instead of after every
  job. The lag is transient here, but its bound is one step per eight jobs
  under a requester that never idles, which is the bound the owner always
  had under a full queue. Candidate D07 below makes reclamation follow the
  jobs that create it.
- **Time outside the owner rose 168 → 266 ms.** Not explained by a count:
  requests, hand-offs and jobs are equal. The hook that begins a transaction
  subtracts its time from the triggering statement, so statement time moved
  between families, but owner service is measured around the whole job and
  fell. Open observation; the next sample at a new identity will show
  whether it persists.

Command against target: 900.8 ms against A2's 183.4 ms, 4.9 times slower.
