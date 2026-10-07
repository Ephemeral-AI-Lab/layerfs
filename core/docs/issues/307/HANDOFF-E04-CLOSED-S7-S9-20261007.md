# E04 closed; S7/S9 implementation handoff

> Status: E04 functional checkpoint CLOSED on Disposable and Durable.
> Human boundary: “after closing e04 produce the handoff.”
> S7 and S9 remain unchecked. This handoff stops implementation at E04 closure.

The subsequent [owner profile direction](checks/e04-disposal-20261007/63-owner-disposable-development.json)
selects **Disposable-only development verification going forward**. No new
Durable test, proof, diagnostic or performance selection is to be launched.
Outstanding Durable execution is **NOT_RUN — deferred by owner for
Disposable-only development**; it does not block ordinary Disposable development
checkpoints. Preserve Durable product support and every earlier success/failure.
This handoff launches no further profile selection and makes no claim that all
existing mixed-profile test entry points have been changed. Apply the exclusion
in the owning external tests/harness before their next selection, with no
test-only product branch. The completed both-profile E04 evidence remains valid
at its recorded identities; future changed source is not newly Durable-verified.

Work is in the primary checkout `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, on
local `main`. No remote push, release, deployment, new worktree, checkout reset,
other chat's goal change or early legacy retirement was performed. The next
owner must reconcile the actual checkout before editing; this document is a
handoff, not authorization to resume a paused chat or historical campaign.

The [primary validation contract](../303/07-implementation-validation.md),
[S7 audit](S7-EXIT-AUDIT.md), [S9 audit](S9-EXIT-AUDIT.md),
[reconciliation ledger](S7-S9-EXIT-RECONCILIATION-20261007.md) and current human
instructions govern. The [E04 report](E04-DISPOSAL-CONSISTENCY-20261007.md)
contains exact run/build identities, results and limitations. The
[deepest-file implementation plan](S7-S9-IMPLEMENTATION-OWNER-20261007.md)
retains completed checkpoints and tentative next changes; checkpoint 4 has not
been implemented.

## Closed named E04 requirements

| Requirement | Exact proof | Verdict and scope |
| --- | --- | --- |
| Original write window | Disposable original [31](checks/e04-disposal-20261007/31-disposable-original-diagnostic.json), retained validator [33](checks/e04-disposal-20261007/33-disposable-retained-validation.stdout); Durable original [52](checks/e04-disposal-20261007/52-durable-admission-original.json), validator [53](checks/e04-disposal-20261007/53-durable-retained-validation.stdout) | PASS on each profile: exactly 1,000 frozen aligned 4 KiB writes through Workspace::mutate, OwnerClient and authenticated Upstream; original write receipts reconcile to the foreground owner delta |
| Independent final-state oracle | Original oracle and original immutable-root observations in [55](checks/e04-disposal-20261007/55-two-profile-custody.json) | PASS: all 16,777,216 final bytes plus EOF, portable mode/link/time facts, exactly root+dense.bin membership, unchanged original immutable file-content-root bytes, captured Runtime Binding equality |
| Application disposal and local cleanup | Same original [55](checks/e04-disposal-20261007/55-two-profile-custody.json), complete raw run directories below | PASS: actual Close, automatic idle reclamation and Gone; exact final Binding Message/Delivery; explicit host fence, joined workers, zero credits/Save custody; separate consumer fence/join and Owner Stop; both processes exit 0 |
| Native backing and input custody | Original Docker command/CID/image/inspect and independent pre-start/post-exit guest observations retained with both runs | PASS for this diagnostic: true ext4 guest volume, pre-start sealed copied inputs and original bind/probe witnesses; no host/guest inode or allocation equivalence |
| Sampled-peak admission correction | Separate [E3 regression receipts](checks/e3-sampled-peak-20261007/) | Closed refusal gap: a finite sample inventory cannot claim a continuous peak. This supplies no calibrated resource qualification |

The [owner's closure direction](checks/e04-disposal-20261007/56-owner-functional-closure.json)
allows this completed functional scope to close while its eleven broader
observation dimensions remain open. Do not rerun unchanged successful E04 cases
or extend its trace into a general profiling platform merely to keep E04 open.
Binding equality is not a fresh Branch-history query. The exact explicit-fence
idle EOF remains retained; it does not authorize partial/truncated encrypted EOF
or substitute for kernel cancellation evidence.

## Source, build, fixture, cache and raw evidence

The prospective v3 specification was committed at
`d925a28c0cd9f0fd1661df614409df1a61528de4`. The closure commit containing this
document adds external examples, tests, validators and evidence; it changes no
product source. The runs used the exact checked inventories below before that
commit existed. A later commit does not retroactively make a diagnostic a sealed
performance arm.

| Identity | Disposable | Durable |
| --- | --- | --- |
| Source inventory | [24](checks/e04-disposal-20261007/24-corrected-build-inputs.json), 809 files | [44](checks/e04-disposal-20261007/44-host-budget-build-inputs.json), 809 files |
| Matching build witness | [28](checks/e04-disposal-20261007/28-matched-build-witness.json) | [49](checks/e04-disposal-20261007/49-budget-build-witness.json); actual corrected builds 46/47 |
| Host SHA256 | `15a51c573670ed6cd7157ab3e3b2d52548e747cc01b017682011454a516bdd00` | `839539ad47888bd03d6c2adea37c91ff47ec5ce6276f8b93ebb8df3c7dd10021` |
| Linux SHA256 | `7d10a68d36e2f700f3afe60ae564fee5a1671683e20f06805a4d9fd15ec93939` | Same original consumer binary |
| Complete command wall time | 52,921,882,250 ns | 52,693,429,083 ns |
| Product/internal/external stops | 80/88/90 s | 110/118/120 s, prospectively authorized |
| Original guest volume | `layerfs-e04-disposable-fe9ebf338f7743e2-state` | `layerfs-e04-durable-652dfa25a9184db2-state` |
| Database logical/allocated bytes | 4,853,760 / 273,289,216 | 4,853,760 / 273,289,216 |
| Original guest database SHA256 | `e2b86b85f41828f93de5187d169fa3acf751d53a0a530743f8974f4d38a1644a` | `164e4d49c0f31756e774feb909b09e07e8c711e049de379e161187acd2387232` |

Both use locked Rust 1.85.1 debug functional builds and pinned Linux image
`sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
Repository ARM64 inputs remain intact. Ordinary construction uses one producer;
native namespace Init declares its independently supported four-worker profile.
Cache and interference are uncontrolled. These are functional diagnostics,
ineligible for numerical/resource qualification; no speed comparison follows.

Closed masters remain under `core/target/cluster2-307/e04-prepared-20261007/`:

- `base.bin`: 16,777,216 bytes, SHA256
  `a813c22ec9b6429809beafdeed2903207e31f2c68c4ab5795c11a857d26e6f17`.
- `replacements.bin`: 4,096,000 bytes, SHA256
  `c9440baeb12b6f5d6fe142092fe3f862611f8c4ab36788a078922ea764489290`.

Setup clone made independent input copies before each original attempt; no
mutated measured Store/overlay was reused. The actual Project Init/Store/Save/
fork/runtime fixture removes the acquisition source. Source-copy byte counts
are not imported-I/O traffic. Both independent oracles produce final SHA256
`7bea926791fa6462c068c4980c7c2c6f9b12b9d7a2ae50141580db3abd1af617`.

Original raw directories are retained outside Git, relative to the checkout:

- `core/target/cluster2-307/e04-disposal-disposable-20261007/`.
- `core/target/cluster2-307/e04-disposal-durable-admission-20261007/`.
- Earlier failed `e04-native-disposable-20261007/`,
  `e04-disposal-durable-20261007/` and the original paths in
  [failed allocation custody 69](checks/e04-writes-20261007/69-failed-allocation-custody.json).

The retained final binaries are in the Durable raw directory's
`retained-binaries/{host,consumer}`;
[receipt 54](checks/e04-disposal-20261007/54-retained-final-binaries.json)
records their independent copies. The earlier Disposable host executable was
overwritten by the later admission build before it was separately archived.
Its source copies, original binary hash, matching build and executed receipts
remain. The current host executable must not be presented as that prior binary
or as a reusable sealed control. Frozen launcher/probe versions are retained in
[the helper directory](checks/e04-disposal-20261007/helpers/).

## Full exit ledger at handoff

No complete S7 E1–E4 or S9 R1–R4/Q1 exit is closed by this work. S7 and S9 stay
unchecked, qualification stays NOT_EVALUATED, admission false, all 27 E1
proposals NOT_RUN and E1 samples zero. E05 is explicitly NOT_RUN.

| Exit | Implemented/evidenced state | Remaining action and dependency |
| --- | --- | --- |
| S7 E1: prospective registration | Registration machinery and scoped refusal checks exist; E04 now has actual diagnostic route/input/build/disposal proofs | INCOMPLETE: all eligible routes and final identities, cache/observer state, finite schedules, numerical authority or eligible control formula, resource envelopes and feasible Q05 budget before sampling. Tooling PASS is not a sample |
| S7 E2: whole original-operation accounting | Original aggregate JobWork/OwnerWork and E04 narrow write-window sums retained; startup and original failure custody preserved | INCOMPLETE: actual per-job family attribution, all startup/Needs/publication/ReplyAttempted/held result/release/stop/cleanup costs, true receipt/queue capacity, SQL template/bind/EXPLAIN correlation, provider facts/IDs, whole copies, indexed visited rows and eligible debt. S8 owns exact native request/reply/open/lookup portions |
| S7 E3: physical I/O and phase resources | Bounded external sources, stream failure custody, actual guest artifact observations and sampled-peak refusal | INCOMPLETE: actual baseline/interior/final phase coverage, complete ordered inventories, ownership, clocks/error/gaps, calibrated overhead and host/Linux/kernel/cache/dirty/journal/index/overflow domains. Helpers are observation sources, not calibration; runtime capability/permission denials must be evidenced before claiming external gates |
| S7 E4: sustained service/reclamation | Automatic finite E04 idle cleanup shown | INCOMPLETE: registered finite class/Workspace arrivals, numerical service bounds, progress of every runnable class, pressure/headroom and debt production/service/drain during live activity and final idle. Sequential E04 throughput is insufficient; no maintenance pump |
| S9 R1: application supervision | Existing independent socket workers and E04 external explicit fence/join/credit proof | INCOMPLETE: initialized application composition and complete original partial/undelivered/refused result accounting. External example disposal does not activate the actual application |
| S9 R2: contextual admission/topology | Existing scoped DirectoryLeaf serial/root and FilesystemRoot inode/content checks | INCOMPLETE: owning Content qualification of membership, dangling serials, illegal directory aliases, cycles, unreachable/count mismatches, root/context and provenance. Initial qualification is paid; no public trusted boolean, parallel SDK graph engine or hidden repeated-bind whole-root walk. Share with later S10 K2 |
| S9 R3: restart boundaries | Existing session/result primitives and explicit E04 disposal | NOT PROVED at actual connection, consumer-process and host-runtime restart boundaries: original queued cancellation, surviving known attempted outcomes after lost delivery, host-loss terminal unknown and old epoch/capability refusal. Durable objects do not persist session knowledge; no replay/crash resolver |
| S9 R4: host/application wiring | Actual host libraries → authenticated Linux E04 consumer works in the external vehicle | INCOMPLETE: owning remote FinalizedConsumer/Save over Begin/Accept/Objects(Some original SaveToken)/Finish, separate same-Save reader, two interleaved Saves, real API-core/Sandbox boundary replacement before activation. Accept remains canonical-only and lacks advisory predecessor provenance |
| S9 Q1: faithful complete roots | Both-profile 16 MiB E04 source-removed fixture and earlier component evidence | NOT QUALIFIED: ignored/dependency/cache/output data, aliases/raw symlinks, huge namespace, 500,000,000-byte dense native fixture, large sparse files, same-Save/interleaved Saves, conflicts, exact disconnect and resource proofs. Disposable is the sole active development profile; outstanding Durable execution is NOT_RUN — deferred by owner. Actual >4 GiB execution is NOT_RUN — waived by owner; format/offset requirements remain |

The eleven E04 unavailable dimensions remain exactly: private BaseFacts/provider
ID trace, per-original-job statement families, whole-operation copies,
statement-fingerprint/bind correlation, indexed visited rows, exact eligible
debt, queue-only peak, isolated resource/runnable wait, host/Linux/kernel phase
residency, physical I/O and cache enforcement/calibration. SQL spans overlap
service; queue wait includes parking; held-result credits are not queue-only
occupancy. Lifetime high-water values do not replace phase peaks.

S8 must still provide actual request/open/lookup/reply/cache/thread/kernel
ownership and exact send/disposal evidence. The pinned fuser 0.18 public
INTERRUPT capability is a specific dependency gate, not permission to patch
third-party code beyond the authorized signed-timestamp correction or invent
batch-forget support. Independent engine work remains available before that
native residual. Missing instrumentation is not an external gate.

Captured namespace normalization and full capture→Save→Stage→history→known
install/terminal unknown remain S10, except a specifically named shared S9
prerequisite. No S11 cleanup, S12 campaign or S13 reference retirement is done.
The eight restored Init speed failures and eight strict-allocation failures,
original controls, 1.10× arithmetic and 30 s/19 s limits remain. The six history
pairs are a separate route and cannot replace Init evidence.

## Preserved failures and custody

| Original record | Outcome retained; successor does not change it |
| --- | --- |
| E04 writes build15 and Clippy20 | Original failures preserved; corrections were confined to external harness interfaces/formatting |
| E04 writes 50 | Accepted socket was nonblocking; zero original writes |
| E04 writes 60 | Host idle-input timeout after 67 Applied and 66 complete traces; consumer killed with exit 137, no OOM flag. Publication 67's reply/release is unavailable; do not guess success or cleanup |
| E04 writes 68 | ENOSPC on namespace write index 94: 95 attempts/traces, 94 successes, 380 jobs, 378 allocation admissions plus one refusal. Failed namespace job recorded zero changed rows/payload copy. Owner Stop NOT_RUN |
| Native backing 11 | First guard failed: created descriptor reported generic FUSE while reopened descriptor exposed the known incompatible filesystem. Exact probe diagnosis/corrected refusal retained; no blanket FUSE ban |
| Native backing 38/39 | Guest performed 1,000 writes and oracle/Stop passed, but original host CloseFailed and Docker bind-source validation failed. Overall failed/incomplete; original volume/container retained |
| Disposal 13 | Aggregate Python invocation hit its original 60 s wall stop and remains FAILED as a hang. Source/ordered output diagnosis retained in 15/16; 137 completed bodies were not rerun, interrupted body and six unstarted bodies passed separate bounded selections 17/18 |
| Disposal 22 | Clippy comparison-chain failure; same-branch match correction, source-matched build and passing checks retained |
| Disposal 35 | New owner-budget validator regression failed before the correction; authorized positive and unregistered-limit refusal pass in 37/38 |
| Disposal 42 | First Durable selection failed before readiness because host admitted at most 90 s while the newly authorized selection supplied 110 s. Zero writes, consumer NOT_RUN; empty original volume/helper retained. Receipt 43 diagnoses, actual corrected builds 46/47 and fresh selection 52 succeed |
| Disposal 45 | A failed edit-pattern assertion was followed by an unchanged-source build. It is not evidence for the correction; 44/46/47/49 retain the corrected source/build chain |
| E3 continuous-peak negative | Failing regression retained before refusing the unsupported continuous claim; 44 registration tests then pass |

The two successful v3 runs have no terminal unknown operation outcome. They
retain the expected explicit-fence idle EOF and quarantined second-close state
with exact accounting; the validator accepts only that explicit complete case.
Original partial/truncated EOF and CloseFailed cases remain failures.

The human authorized replacement of exactly three overallocated failed
`overlay.sqlite` files with byte-identical copies. This was completed in
[receipt 70](checks/e04-writes-20261007/70-authorized-physical-replacement.json),
with original inode/allocation/hash in 69 and subsequent limitations in
[71](checks/e04-writes-20261007/71-unlinked-original-readonly-access.json) and
[77](checks/e04-writes-20261007/77-retirement-and-binary-availability.json).
Paths and logical bytes were preserved; original physical identities changed.
Path allocation fell by 131,129,397,248 bytes, but VM-held unlinked descriptors
prevented claiming that much real free space. Do not repeat this action or
prune unrelated processes to force reclamation. Receipt 77 also preserves the
external disappearance of old Cargo/binary artifacts; no availability is
inferred from their historical hashes. Input generator receipt 13's logical
64 KiB flush trigger remains explicitly corrected by 14; actual Python capacity
and process peak were not observed.

Both successful consumer containers were removed only after known successful
post-exit observation, as recorded in 55. Their original volumes and pre/post
helper containers remain. Failed run artifacts, Stores, inputs, independent
copies and raw receipts are retained. The unrelated protected containers
`9cf2fe345496`, `ce75ac504df9`, `d2433851ea59`, `d2550144998b` were not interrupted
or pruned. The two protected untracked notes remain untouched and unstaged:
`HANDOFF-S7-S9-RESUME-20261006.md` and `S7-S9-SPEED-TEST-PLAN.md`.

## Verification, local commits and source size

Scoped E04 checks include 11 host and 11 Linux control bodies, 146 distinct
validator bodies with the original aggregate failure preserved, matching locked
host/Linux builds, Clippy and formatting. The final host admission correction
has its own build/Clippy/format receipts 46/48/50. Source-unchanged earlier
Overlay/product-boundary/tool evidence is reused at its original identity.
No successful E04 case or unrelated suite was repeated for this handoff.

| Local commit | Exact production LOC, first parent → committed snapshot | Core / active core after |
| --- | --- | --- |
| `7409c2922b98aefb0adc589234015feeac592102` | 170041 → 170068, delta +27 | 104651 / 61486 |
| `2df73fd79cb82bb6bbc0357ea8bcf4a6d03d1e50` | 170068 → 170068, delta +0 | 104651 / 61486 |
| `7b1b3490b10c6c8d66feb654a909718d4bacd194` | 170068 → 170108, delta +40 | 104691 / 61526 |
| `d925a28c0cd9f0fd1661df614409df1a61528de4` | 170108 → 170108, delta +0 | 104691 / 61526 |
| Closure commit containing this handoff | [Exact staged comparison](checks/e04-disposal-20261007/59-production-loc-comparison.json): 170108 → 170108, delta +0; committed identity is in the post-commit verification below | 104691 / 61526 |

Every comparison uses `tools/production_loc.py`, SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`, with exact
Git snapshot exports, product Rust/runtime SQL including retained/excluded
sources and excluding tests/inline test-only/examples/docs/tools/artifacts.
Reference remains 65,417; excluded predecessors 40,321; excluded integration
2,844; Content 18,613. No relocation, duplication or retirement is called an
algorithmic simplification. Content's #303 baseline remains 13,500 at
`f96d97651be5299f153ccde2bc8d921dd58807ad`: growth +5,113, including real
resident/backed compatibility and separate state/traversal complexity costs.

Post-commit verification is written once to the ignored path
`core/target/cluster2-307/e04-closed-handoff-committed-verification.json`; it
records the actual final commit/tree, counter/source/binary matches and final
working-tree state without creating an endless verification-only commit chain.
The intended final Git state is the committed closure plus only the two protected
untracked notes. The actual final verification, rather than this intention,
governs any later reconciliation.

## Prepared next-owner work, not executed here

Remaining S7 work is organized into three packages: operation accounting,
resources/cache and service acceptance. Routine collection should record
run-level identities once, compact referenced events, fixed aggregates and
required boundaries. Detailed job traces are reserved for a named unresolved
diagnostic. Preserve the approximately 31 MB E04 trace for 4 MB replacement
input; do not grow a generic profiling system around it.

1. **Operation accounting:** checkpoint 4 names exact Daemon observation,
   queue/credit/command/completion files and external tests. Full per-job SQL
   arrays do not fit the existing 64 KiB lifecycle reserve across all 32 slots.
   Implement a real ownership/storage correction and prove actual compiler
   layouts, queue capacity, exceptional receipts and saturated lifecycle
   admission without raising 8 MiB/64 KiB or lowering promised slot counts.
   Preserve original results and publish owner deltas before completion visibility.
2. **Resources/cache:** integrate calibrated phase observers and complete
   ownership/clock/error inventories using existing resource helpers, supported
   native libproc/procfs/cgroup and SQLite observations. Additional supported
   macOS fields are implementation work, not an external blocker. Sample maxima
   are not continuous peaks; no current authority permits replacing aggregate
   phase peaks with cache-size/credits/lifetime-high-water arithmetic. Observe
   actual access/build capabilities before identifying an external gate.
3. **Service acceptance:** prospectively register finite actual arrivals,
   numerical criteria, class/Workspace mix, pressure/headroom and automatic
   live/idle debt drain. Finish independent engine gates before the exact S8
   native residual. No maintenance pump or sequential-throughput substitution.
4. **R2 shared Content prerequisite:** use the existing neutral indexed-record
   port for a paid whole-root qualifier. A tentative indexed inode pass,
   reachable-directory traversal and final reachability/count pass must cover
   dangling serial, alias, cycle, unreachable/count and root/context negatives.
   Host qualification backing/provenance needs an owning Store solution, not
   another database, private file or SDK graph engine. Initial qualification
   alone does not justify changed-root provenance for later K2.
5. **R4 minimal composition:** SDK-owned RemoteSave with a borrowed
   FinalizedConsumer sink and separate borrowed reader using the original Save
   token, over existing Calls and host Sessions. Retain original accepted object/
   reply or first failure, then refuse later calls before transmission; Finish
   once and explicit known terminal release only. Tentative new SDK
   `client/save.rs` and `save_result.rs`, thin Daemon Upstream forwarding and
   external native two-interleaved-Save proof need a prospective deepest-file
   plan. Canonical Accept lacks predecessor provenance; settle that exact
   ownership behavior before implementation. Full S10 remains separate.

All future development selections use Disposable; Durable-specific execution
must be explicitly skipped/deferred in external tests/harness selection. Durable
semantics and historical numerical gates remain intact. Checks remain serialized
by their integration owner, build first,
with prospectively declared independent selections and an explicit wall stop
at most 120 s. The later human budget direction does not rewrite old failures,
frozen numerical limits or one required whole-operation boundary. Preserve one
ordinary construction producer and the single namespaced daemon database.
Content FileBacking/RowSpool and excluded callers still exist; do not infer
absence from active membership or retire them during this handoff.
