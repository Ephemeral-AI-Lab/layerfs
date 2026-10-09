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
