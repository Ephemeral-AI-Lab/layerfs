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
