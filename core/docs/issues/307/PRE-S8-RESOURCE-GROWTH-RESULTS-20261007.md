# Focused pre-S8 resource-growth results

> **Status:** All14 registered functional/count/lifecycle selections pass.
> Source-derived working-state bounds are supported by these diagnostics.
> Exact continuous peaks, exclusive physical attribution and numerical acceptance
> remain unqualified. No production correction was required by these findings.

The owner authorized these focused checks after asking which resource gaps were
necessary before S8. This completes that continuation; it does not turn the older
full E3 measurement specification into a PASS. The [prospective plan and owner
inventory](PRE-S8-RESOURCE-GROWTH-20261007.md) defines the workload and scope.

## Original selections and budgets

All receipts below are under [the growth evidence directory](checks/pre-s8-resource-growth-20261007/).
Release source is `0c736c4bf774ababab16dc6627fea3fcbb6ad930`. Host05/Linux07
builds and final inputs16 pin actual executables; registration18–31 pins each
workload/preparation/source/binary/profile.32/47 admit the host/Linux selections.
One run per case/platform, fresh byte-copy clones, natural cache, no speed comparison.

| Case | macOS complete command ns | Linux complete command ns | Run receipts host / Linux | Result |
| --- | ---: | ---: | --- | --- |
| growth-n1000 |800753958|564992875|33 /48|PASS functional/count/cleanup; diagnostic memory only|
| growth-n10000 |352740833|613397958|35 /50|PASS functional/count/cleanup; diagnostic memory only|
| growth-n100000 |475768584|733564208|37 /52|PASS functional/count/cleanup; diagnostic memory only|
| growth-save4m |442088458|625156917|39 /54|PASS functional/count/cleanup; diagnostic memory only|
| growth-save32m |751180375|919490375|41 /56|PASS functional/count/cleanup; diagnostic memory only|
| growth-save256m |3304555166|3410525500|43 /58|PASS functional/count/cleanup; diagnostic memory only|
| growth-repeat64 |6949820042|7003615500|45 /60|PASS functional/count/cleanup; diagnostic memory only|

Each complete command is below the existing15s budget. Independent validation62
checks all original registration/binary/stream hashes, exact phase and operation
inventories, byte-oracle completion, transaction arithmetic and terminal ownership.
Its complete command321862916ns and internal174536458ns are below10s. The full
[machine report63](checks/pre-s8-resource-growth-20261007/63-growth-report.json) retains every point/phase and original identity.

## Namespace growth

| Files | macOS released Rust requested B | Linux released Rust requested B | Logical shared-cache charge B | macOS / Linux terminal Rust requested B |
| ---: | ---: | ---: | ---: | ---: |
|1000|2902152|2887424|32691|10412 /9918|
|10000|2910248|2895807|41055|10416 /9921|
|100000|2921159|2906982|52451|10443 /9924|

A100× namespace increase changes released live Rust ownership by19007B on macOS
and19558B on Linux in these selected operations. The shared cache charge rises
19760B with additional canonical tree paths. This supports the source-derived
indexed/windowed path; it is not a measured claim about every possible namespace.
Source buffers/indexes have fixed caps and active-reader/producer multiplicities,
rather than a container holding the complete namespace. Store/hash/clone file-cache
cost remains separately visible in the raw cgroup observations.

## Streaming Save growth

| Input MiB | macOS / Linux Rust requested B at observed Content handoff maximum | macOS / Linux observed Save-phase RSS B | Publication batches / writes |
| ---: | ---: | ---: | ---: |
|4|24820981 /24806541|44138496 /40222720|2 /3|
|32|25436494 /25421766|52117504 /42012672|11 /12|
|256|26346960 /26332520|61194240 /43196416|87 /88|

Each case verifies every saved byte through the immutable daemon ports using a
131072-byte oracle window. All roots and Save counts match across the two SQLite
builds. The64× byte increase adds approximately1.53MB at the observed Rust handoff
maximum; it does not allocate a256MiB input/output Vec. Maximum pending canonical
bytes are4181283,4192871 and4194106, all within the4194303B product wave bound.
Physical pack bytes4209638,33673625 and269384355 grow with actual content;
217,1742 and13902 finalized objects are accounted. Each Save reserves once and
then acknowledges2/11/87 bounded publication batches, for3/12/88 writes. No
history transition is included in these standalone Save cases.

Read verification fills the shared8MiB cache and the four bounded locator/descriptor
sets. Released Rust memory consequently rises7.19→11.70→15.03MB on macOS
(similar on Linux), while shared-cache charge reaches and remains below8MiB.
Those live read caches are not a leak or zero-cost verification. All are dropped
at Store teardown: terminal Rust requested bytes are10418/10421/10424 on macOS
and9924/9927/9930 on Linux. These are actual observations, not a new numeric ceiling.

## Repeated Commit ownership

Each platform publishes64 distinct Commits, reads the changed byte after every
publication, verifies capture+install family equality and the original2 engine
jobs, and releases all results before recording the next point. Each Commit has
one initial reservation, one publication and one history transaction:192 writes
over64 operations, without retries. Credits, outstanding jobs, queued jobs and
receipt overruns remain0 at every released point. Terminal automatic namespace
reclamation is acknowledged before Owner stop.

| Cycle | macOS released live Rust B | Linux released live Rust B | Logical shared-cache B |
| ---: | ---: | ---: | ---: |
|1|2921450|2906697|52451|
|8|3022191|3007726|118552|
|16|3121807|3107342|194096|
|32|3327959|3313494|345184|
|48|3536935|3522470|496272|
|64|3741463|3726998|647360|

Live Rust ownership rises820013B/820301B across the64 distinct roots. Cache
charge accounts for594909B of that increase; bounded Storage locator/descriptor
and cache-map bookkeeping also fills. Logical cache charge is not exact allocator
attribution, so the remainder is not falsely assigned byte-for-byte. The source
caps these owners independently of prior Commit count; this selection does not
claim an empirical cache plateau because it has not filled every configured cache.
At teardown live Rust ownership falls to10446B/9927B. Cumulative Rust allocation
traffic is about5.16GB, most already freed, demonstrating why cumulative allocation
must not be reported as live memory.

## Process memory and OS backing cache

The large Linux Save has observed cgroup memory343416832B. During Save, the
observed category maxima include45740032B anon and287899648B file cache; fields
overlap and occur at potentially different instants, so they are not summed into
a manufactured total. The substantial file-cache component follows the written
Store/WAL backing. This is explicitly visible; a bounded Rust heap is not presented
as a bound on the entire Docker cgroup or VM. No hard OS-cache ceiling was added.

For the64-Commit Linux case, released RSS changes23597056B→57180160B; macOS
changes43794432B→50888704B. RSS remains54611968B/47202304B after Store/Owner
teardown while live Rust requested bytes are about10KiB. This is consistent with
allocator/native-runtime/page retention rather than retained Rust Store objects;
the exact C-allocation/slack split is UNAVAILABLE, so no stronger attribution is
claimed. The fixed16MiB encode arena and fixed read/index/cache owners explain
why RSS and current logical cache charge cannot be equated. No process swap was
observed in any Linux sample. Whole-process/cgroup limits remain deployment
inputs and future qualification; these diagnostics do not invent an RSS PASS.

The useful conclusion is finite, source-supported working ownership and complete
release in the selected workloads. No unbounded Rust owner or failed release was
found that calls for a product fix. The remaining broader E3 precision gaps remain
explicit: continuous phase maxima, internal copies/visited pages, exclusive physical
I/O and exact runnable-only wait. This closes the requested focused investigation
and supports proceeding with S8 implementation; it does not certify full resource
qualification or guarantee that future workloads cannot expose defects.

## Identity, checks and custody

All executables use locked Rust1.85.1 release, repository ARM64 inputs and
LAYERFS_CONSTRUCTION_WORKERS=1. Disposable Store WAL/OFF; overlay schema16
MEMORY/OFF/EXCLUSIVE; four Store read handles and one selected producer. macOS
uses system SQLite3.51.0; Linux uses bundled3.53.2 in image
`sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
The external System allocator wrapper adds atomic bookkeeping overhead and covers
Rust requested bytes, not C malloc or allocator slack. Samples and handshakes add
observer overhead. All timings remain diagnostic; no warm-cache speed credit.

Host binary SHA256 `e1d96be5f216bf0b359c25e8ab11e1b8a8ab59433638bc7db9510b5c20c999f4`;
Linux `7b7c0f98b168b1375aa740f3b4eb764ee1d10e34acd59ae77d50dabe8f796669`.
Preparation08 is separate from measured operations; exact closed fixture identities
are in12. The new1k/10k preparations remain under `/private/tmp/layerfs-resource-prepared-20261007-n1000`
and sibling `n10000`; the original100k preparation remains at its F11 path.

Tooling02 passes24 tests; final Clippy11, fmt13, boundary14 and46 guard tests15
pass. The original external-adapter Clippy03 failure is retained and corrected.
No functional selection, budget, swap or cleanup failure occurred in this campaign.
All Durable execution remains owner-deferred NOT_RUN. No native>4GiB case, E04,
FUSE/Exec or S10 normalizer was executed. Existing historical verdicts remain unchanged.

Cleanup64 independently checks the successful original child/owner outcomes and
removes only the seven owned host copies/placeholders and seven successful stopped
Linux containers. All text/JSON evidence and sealed preparations remain. No failed
or unknown sample was deleted. The four protected containers, three protected
notes and root reference remain untouched. No push, deployment or new worktree.

Production LOC:165673→165673 (delta0) for tooling commit `0c736c4bf`;
core100256, active57382, reference65417, excluded predecessor36325 and excluded
integration6549 unchanged. Its exact first-parent/staged receipt is17. The following
results-only commit has its own unchanged comparison in
[receipt66](checks/pre-s8-resource-growth-20261007/66-results-loc.json).
[Final document/preservation check65](checks/pre-s8-resource-growth-20261007/65-document-and-preservation-check.txt)
confirms138 existing local targets, unchanged protected-note hashes and production
source, the four protected running containers and no remaining owned diagnostic
container. No production code changed.
