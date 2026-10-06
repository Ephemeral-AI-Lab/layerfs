# S7 engine cost gate audit

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Milestone state: CHECKPOINT, incomplete. S7 remains unchecked.

Starts from reconciled local main `4ecea41983b673d62db90880b777a94565eac985`,
S6 product `983c2ee6d36a4417d8fff2d14db6b141f5386c8c`, tree
`be2744223a450eaa01b9f31c4e3c850bbd141d72`. Tracker #307 agrees that S5/S6
are complete. The [S5/S6 handoff](HANDOFF-S7-S13.md) remains its stopping record.
The current owner selects [source organization](SOURCE-ORGANIZATION-S7-S13.md);
its S6-in-progress date is superseded by these identities, without editing that
unrelated side document or adopting its algorithms/capabilities.

## Implementation and observations

[Operation observations](../../architecture/36-operation-cost-observations.md)
add exact per-original-job SQL/allocation/payload receipts to daemon Completion,
separate foreground/maintenance aggregates, returned BLOB bytes, direct versus
trigger-inclusive changes, physical high-water allocation and freelist/identity
observation counts. Fixed aggregate receipts are credited through result lifetime.
All existing public paths remain; no schema, payload algorithm, SQLite version,
reservation size, resource cap, retry, sync or private namespace index is changed.

| Required dimension | Actual evidence / remaining gate |
| --- | --- |
| Complete owner SQL/queue/request accounting | Original-job receipt includes readiness parking, final attempted service and retained-result credits; native/transport requests remain S8/S9 |
| EXPLAIN + actual VM/row work | Exact payload/capture/maintenance plans and runtime profiles in [Linux overlay](checks/s7-costs/linux-layerfs-overlay.log) and [Workspace](checks/s7-costs/linux-layerfs-workspace.log); source/state checks and accounting triggers stay included |
| Bound/delivered/copy bytes | Statement bound/returned BLOB bytes plus actual cell/composed-window counters; driver/SQLite/transport/kernel copies remain unavailable |
| Allocation/storage/freelist | Entire 268435456-byte daemon reservation, actual high-water, logical pages, committed freelist credit, cookie queries, and precise Linux range calls included; sums of requested range bytes are not newly consumed disk |
| Worst/amortized/cumulative | Indexed point/keyset/cell/staircase/source/reclaim shapes below; complete installed/captured/orphan lifetime proofs retained |
| Page/dirty/journal/index/I/O | Logical page/freelist and conservative growth bounds available; exact per-operation dirty/overflow/index/journal/device-I/O peaks are not observed by the current safe driver API |
| Residency | Fixed API/codec/queue windows source-accounted; actual whole-system phase residency/pager/journal/kernel/host cache gate remains incomplete |
| Sustained service/debt | Live/idle automatic cleanup and logical debt upper bound proven; no sustained numerical arrival/service-rate or high-water acceptance campaign has run |

Linux complete 4096-byte write **plus reply-attempt release**: 17 statement
attempts, 21 SQLite executions, 727 VM steps, 13 changed rows including triggers,
5 direct changes, 128 metadata-root BLOB bytes returned, 4520 declared bound
bytes. Cell input/copy is4096 bytes, one full cell, zero partial cells and zero
codec window initialization. Two pre-BEGIN freelist queries and two range calls
request402653184 bytes in total (256+128 MiB classes); eight identity observations
use16 metadata calls. Subsequent dense read: five statements/105 VM,4160 returned
BLOB bytes (4096 data plus64 root binding/custody), no DML. Snapshot:47 pages,
0 free,192512 logical bytes,268627968 allocated/high-water bytes. No exclusive
physical I/O or resident memory number follows from those logical counters.

Linux real SQLite-full attempted operation records25 attempts/42 executions,
3006 VM,51 total/17 direct changed rows and one explicit rollback. Backed logical
counts and inode absence remain unchanged. Attempt costs are retained rather than
reported as zero because rollback succeeded. Linux owner open has4 attempts/5
executions/245 VM, one freelist cookie/range call, zero payload work and a receipt
which exactly matches its foreground aggregate. [Host count outputs](checks/s7-costs/costs-repaired.log)
retain the distinct SQLite3.51.0 profile, not a pooled platform result.

With database population N, request bytes W, staircase height S, returned keys K
and admitted jobs Q: point work isO(log N); keyset windows areO(log N+K), with
noncovering row seeks separately charged. Write touches<=ceil((W+4095)/4096)
cells; old fragmented byte history does not add cells. Partial stale-cell work
includesO(log S*log N) staircase seeks. M writes accumulate actual bounded
request/cell copies and indexed updates, without rebuilding growing file prefixes.
S6 maintains<=2 live namespace layers and<=3 parked orphan layers, then1; retained
Commit count does not enlarge current read depth. Automatic reclaim has output-sized
cumulative work in bounded weighted turns, not free/disappearing work. Known
deferred Commit collections/walks P3/P6/P7/P13/P14 remain S10 prerequisites.
A bound on these source operations does not replace the missing residency/service
proofs or make a complete integrated operation available.

## Verification and identity

Builds use Rust1.85.1 `--locked --all-targets --no-run` before test execution,
repository ARM64 flags and one construction worker. Host overlay45, Workspace32
and daemon10 pass; the independent SDK checkpoint has9 passes. Linux45/32/10 pass,
with two owned-device cases explicitly ignored. Commands,120-second ceilings,
actual walls and uncontrolled-cache declaration are in [host receipts](checks/s7-costs/host-covering-receipts.json)
and [Linux receipts](checks/s7-costs/linux-receipts.json). No hang occurred.
The longest host package command is22.017095417s; Linux is25.010065750s. These are
functional test commands, not performance samples or proof-budget exceptions.

Changed-scope all-target Clippy `-D warnings`, fmt, boundary568 and26 tool self-tests
pass. Linux retains the existing macOS-provider unused Backend warning. Raw
compiler/fixture failures remain in [failure ledger](checks/s7-costs/FAILURES.md).
[Identity receipt](checks/s7-costs/identity.json) pins source/build/workload/cache
inputs. Source-equivalent S6 physical range/admission and ext4 receipts are reused
at their original identity; the added counters do not relabel them. Native timestamp
FAIL/137 remains, and all cold speed/RSS/sustained-rate selections are NOT_RUN.

Production LOC is recorded in the checkpoint commit and exact first-parent/staged
receipt under core/target/cluster2-307/loc. Counter tools/production_loc.py remains
SHA256 c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb.
Root reference remains65417; all excluded predecessor product source stays counted.
No source retirement or algorithmic shrink is claimed.

## Next ready work

S7 requires actual complete-operation page/journal/IO/residency and sustained
service/debt observations under a prospective admissible cache/workload contract.
The new receipts supply causal attribution for that work. S9's local bound history
checkpoint is independent. S8 continues from the owner-authorized fuser0.18.0 correction and accepted Docker
verification recorded in its separate audit; it is not waiting for a release or
QEMU/custom-kernel qualification.

## Startup-accounting checkpoint (2026-10-06)

Startup SQL is now observed through the same real statement driver: profile
PRAGMAs/readbacks, schema/index/accounting DDL, cache initialization and original
prepare/step failures. `Overlay::create_observed` and daemon `start_observed`
retain separate startup work and exact original success/failure without cleanup
or replay. SQL input bytes and all logical column delivery bytes are explicit;
approximate prepared-statement memory samples are neither cumulative allocation
nor pager/RSS measurements. [Architecture](../../architecture/36-operation-cost-observations.md)
and [new receipts](checks/s7-startup-s9-service/) own this checkpoint.

Host SQLite3.51.0 successful startup:101 attempts/executions,2002 VM steps,
88 rows,1519 logical value bytes,21230 supplied SQL bytes,101 statement-memory
samples summing398496 bytes. Linux bundled SQLite3.53.2:101 attempts/executions,
1918 VM,68 rows,1137 value bytes,21230 SQL bytes,101 samples summing301176 bytes.
Both include one changed initial global-accounting row and9 index-building sorts. These are
finite real schema/profile counts, distinct platform builds rather than pooled
performance. One allocation requests268435456 bytes;3 identity observations
use6 metadata calls, including the separate final receipt observation. Successful
logical188416 bytes sits within full high-water268435456 host/268439552 Linux
reservation. Failed real32-page schema creation retains prior SQL work, original
SQLite-full error and the allocated artifact. Existing-artifact refusal performs
one create_new attempt, zero SQL/open/allocation, and preserves prior bytes.

The host and Docker covering checks retain success/refusal/failure and unchanged
operation EXPLAIN/runtime profiles. Cold speed, phase residency, exact page/journal/
device I/O and sustained service/debt acceptance remain open and NOT_RUN; statement
samples and credit gauges do not close them. S7 stays unchecked. This adds finite
startup attribution without changing schema, profile, quota, reservation or algorithms.

Final-source functional attribution pins selected no-run binaries before and
after execution and compares production hashes, resolving earlier incomplete
execution-hash attribution. These are covering functional tests with120s ceilings,
not cold performance samples or performance proof-budget exceptions. Original
component outputs retain their diagnostic scope, failures and actual walls.

Final frozen functional coverage:173 host checks and92 Docker engine/owner/
Workspace checks pass;2 owned-device cases stay ignored. Native importer adds3
selected Docker passes.39 host and26+3 Docker binaries match their pre/post hashes.
All production source hashes match the frozen identity. Host functional wall
34.902881292s and Docker engine wall11.687125417s remain nonperformance facts.

A follow-up source correction makes startup receipt availability explicit: a
pre-receipt worker failure has unavailable creation work rather than an observed
zero. Service request accounting also includes all pre-credit submission refusals
by original class, separate from dispatched adapter error and credit-window counts.
These corrections preserve original outcomes and bounded fixed counters.

## Native wire cost checkpoint (2026-10-06)

Bridge contract/codec/native and SDK client/runtime/handlers/service now account
actual bounded fragment/body copies, header writes, first-party scratch requests/
capacities/initialization, native API/record/socket/crypto attempts and partial bytes,
shared body/output credits and fair service class ownership. Failed handshakes and
failed native sends preserve attempted work, including copies before failure.
[Architecture40](../../architecture/40-runtime-wire-ownership.md) and
[append-only checks](checks/s9-runtime-wire/) define exact scopes. No changed SQL,
profile, allocation algorithm or counter-version changes the earlier qualified
S6 reservation/freelist/range proofs or S7 EXPLAIN/runtime SQL receipts.

The real host11-operation runtime path observes15 request records,71231 copied
logical body bytes,600 frame-header bytes and72101 transferred encrypted/prefix
bytes. It observes23 reply records,71659 body bytes and72993 wire bytes, including
11 grants. All body/output/service credits return to zero; the service's actual
registry capacity is124416 bytes at that source. Native EOF/partial-call attempts
remain counted separately from positive transfers. A failed handshake retains its
original crypto/I/O work; a quarantined send retains7 framing-copy bytes,40 header
bytes and65519 initialized frame bytes despite zero record I/O. These finite counts
are not whole-system residency, exclusive I/O/copy or a cold/sustained-rate claim.

Host custody coverage has37 bodies: Bridge14, SDK18 owning-provider plus5 wire.
Docker has19 native/portable bodies; global provider bodies are macOS cfg, not
Linux capability. Each source builds with --no-run first; tests have120-second
outer ceilings and Docker also110-second in-container termination plus1-second
kill fence. No hang or unchanged performance resampling occurred. Seven binaries
per platform are hashed before/after. Clippy/fmt/boundary/tool outcomes and all
failures are retained, with exact final source/build/environment identities.

S7 remains CHECKPOINT/unchecked: exact engine page/dirty/journal/device I/O,
whole-system phase residency and sustained numerical service/debt gates remain
open. Full268435456-byte daemon reservation/high-water and backed count-trigger/
freelist/range costs retain earlier identities; S6 diagnostics receive no cold
speed/RSS/rate credit. Native product request frontiers still need S8 integration.

## Remaining-plan reconciliation (2026-10-06)

[The remaining implementation plan](IMPLEMENTATION-PLAN-S7-S9-20261006.md)
reconciles actual source and defines S7 observer/workload registration, complete
cost/page/I/O/phase-residency and numerical service/debt packages. Planning/tooling
commit `5900de7331202e8ead57b4c6c7c77941e8c56bbb`, tree `4a11ab6a242f27218a095bf5d2aeffe1d202df58`, changes no Rust production/schema/profile.
Existing S7 engine evidence keeps its original identity; no new cost/resource
campaign ran and S7 remains incomplete/unchecked. The exact mounted-kernel request
requirement depends on S8; independent engine work does not.

The clarified Project/SQLite guard guidance and 40 passing tooling tests establish
placement, not an acquisition capability. Two failed public review probes for
scratch/source overlap and inherited partial-write byte charge remain diagnostic
[receipts](checks/s7-s9-review-20261006/review.json); they are not S7 qualification.
A first manual-link harness compile failure is retained separately. Source/plan
links and guard checks pass; cold speed/RSS/rate and huge-root work remain unrun.
Separate [S7 tracker receipt](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6010922165) records this status and next-ready work.
Production LOC: core92680 →92680 (+0), reference65417 →65417 (+0),
combined158097 →158097 (+0); exact staged/committed comparison verified.
