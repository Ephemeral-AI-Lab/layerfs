# Phase 6 integration implementation log

> Status: Dated planning checkpoint; not release evidence or a product contract.

Append-only checkpoints. Current action/gates are maintained in CHECKLIST.md.

## 2026-10-02 — full autonomous objective received; V4a selected

Parent `1a59e129f8a3d73f49efe9691b53ba4b377be612`, clean owned worktree/branch
confirmed; active goal read from app state. Previous discussion/source inspection
is evidence progress, not implementation completion. Current V3 path has actual
full-provider two-head proof, but 512-inode/full-reconstruction/full-audit scope
cannot prove the user's larger/local-update objective. Those gaps remain required.

Confirmed source cause candidates: per-locator Noise/TCP setup, unary packs/PUTs,
repeated exact-CAS lookup, full metadata reconstruction and whole candidate audit.
Existing C1 already has EditSequence/apply_edits and PreparedRows/filesystem COW;
shipping daemon already retains authenticated control/service sessions. Public SDK
ProjectApi currently binds concrete Server, so genuine MinIO-backed Init must be
integrated before Family2 can qualify; direct prototype genesis is not its proof.

Own prospective V4a specification/checklist/log. No performance collection or
product enablement in this checkpoint. Next: fixed-size transport instrumentation,
one labelled diagnostic, then one session lifetime treatment preserving custody.
All five owner groups and all seven families are still incomplete. No #288 update,
release claim, merge, rollback or other-owner mutation is authorized.

## 2026-10-02 — V4a transport counters prepared

Parent `46c3dc7cf0f206731ff63a23a1207f093e60db6b`. Added fixed five-action
actual request counts/times and actual connection attempt/authentication time,
shared across Remote clones and emitted cumulatively after each known Commit.
Collector retains a purpose label and the exact parsed statistics beside raw logs.
Native per-call connection algorithm, P6META3, storage/construction/oracles and
limits are unchanged. Owning host Clippy/fmt and locked Darwin/Linux ARMv8 release
builds pass. Existing engine-only tests are unaffected and reused; real full-path
diagnostic remains NOT_RUN at this source checkpoint.

Next: one fresh sealed count-driven diagnostic on V3's exact two-head workload,
then inspect actual connection and request costs before the single session change.

## 2026-10-02 — V4a cause diagnostic observed

Source `198c25296c7e447348422f98a1a11bf0d3732ccc`; one declared count-driven
real-provider diagnostic, fresh `v4a-connection-diagnostic`, child exit 0 in
7.410180 s. Bytes/mode/parent/head and cleanup PASS. No plain-arm resample.
Overwrite Commit 331.744250 ms: 37 actual metadata connections, 213.298414 ms
connect/authentication (5.764822 ms per connection), calls 29 lookup + 7 register
+ 1 publish. First cumulative snapshot includes bootstrap; its 38 connections
and 218.887334 ms are not relabeled as a pure Commit phase. All raw times remain
cache INELIGIBLE; physical/canonical and larger workload gates remain open.

Connection setup/accept polling is a demonstrated major cost. Next: one shared
authenticated session, monotonic exact reply IDs, known idle rotation before new
submission, quarantine on failure, bounded physical connection ownership. Keep
all construction/storage/validation algorithms unchanged for this treatment.

## 2026-10-02 — V4a persistent session source prepared

Parent `22f7d5d05170a8f654c5862a09a879e45f3abe0a`. P6META4 shares one native
connection/request counter per Remote owner; server consumes exact increasing
request IDs on that authenticated connection. Capacity/action checks precede
connection side effects. Known idle age >2 seconds rotates before a new
submission; any uncertain/failed operation quarantines and closes the session,
with no retry/resend. Native five-second bounds and all packing/construction/
validation/MinIO algorithms remain unchanged. Stats remain fixed five-cell state.

Four external real TCP/Noise tests PASS: two requests/one connection, wrong reply
ID quarantine/no resend, over-capacity refusal before connection, and real-clock
known idle rotation with increasing IDs. These peer fixtures prove transport
contracts only. Host locked Clippy/fmt and locked Darwin/Linux ARMv8 release builds
PASS. Existing engine tests unchanged/reused. Full-provider treatment NOT_RUN at
this source checkpoint; next is the single sealed create/overwrite invocation.
All full-goal workload/locality/resource gates remain required and incomplete.

## 2026-10-02 — V4a full-provider treatment passed

Source `776f0f879e52e401d03f4a69301d8c076f1b2a59`; one treatment, full child
9.209285 s, proof19.785250 ms, byte/mode/head/parent/cleanup PASS. Both Commits
use one metadata connection; overwrite has zero connects and unchanged
29lookup+7register+1publish calls. Actual Commit observations82.707000/67.960125 ms;
cache INELIGIBLE, not a qualified speedup. Native deadlines/packing/construction/
validation are unchanged. V4a transport gate complete, broader goal incomplete.

Next: freeze V4b packed locators (exact group/record), bounded private pending
canonical state, batching/read windows and real ACK/collision custody; then
V4c SQL changed catalogs/immediate-parent edits/incremental namespace certificates.
All three named live shapes, all seven families and complete DeepSeek/locality
proof remain required. Genuine public SDK MinIO Init binding remains a prerequisite.

## 2026-10-02 — V4b interfaces selected prospectively

Parent `88e9a37b43506157f559daec53ec141ef0b50415`. Read actual C2 limits and
public directory/inode streaming builders. Freeze P6META5 packed locator fields,
128-ID batches, positive authority pack IDs, exact page/EOF order, existing C2
pack/group/pending limits and private-pending/finish/ACK/collision custody in
V4B-SPEC.md. No guessed pack ordinals or limit enlargement; no collection or
implementation completion at this prospective checkpoint.

Next: replace unary physical object writer/reader with bounded pending/packing/
batch modules and owning placement/identity tests, then one real-provider treatment.
The existing C1 public directory/inode iterators are available for later paged
import, but locality/namespace certification still requires V4c and its own proof.
Goal active with all larger groups preserved; report major checkpoints to #293.

## 2026-10-02 — V4b packing/batch source prepared

Parent `a4770e33a5792e0a8bc2dbb3d3a44f1c3ec78e35`. Implement P6META5 exact
packed locators and 128-ID ordered lookup/registration batches. Global SQLite
assigns stable positive pack IDs, keeps full pack digests/group/record ordinals,
and authenticates complete registration pages before short atomic transactions.
Canonical payload remains in MinIO. Consumer owns one private bounded pending
window, exact same-window/persisted CAS checks, explicit finish before genesis/
READY, lane packs and shared wave decoder/group scratch. Known locator reuse
avoids a second lookup; byte/count admission happens before payload reads.

Four physical/codec/pending boundary tests PASS after correcting a demonstrated
FAIL: the first 300-native-record test used legacy256 instead of the actual
tight-lane16 group limit. Retain `v4b-packing-final.stdout` failure; corrected
`v4b-packing-corrected.stdout` passes. Use actual C2 append_fits with running
assembled size, avoiding quadratic repeated group summation. No C2 limit changed.
Four real-wire session tests, host locked Clippy/fmt and locked Darwin/Linux
ARMv8 release builds PASS. Existing engine tests unaffected/reused. Full-provider
treatment NOT_RUN at this checkpoint. No physical memory/whole-goal completion.

Next: single frozen actual SDK/FUSE/C1/C2/MinIO/C5 two-head treatment and observe
pack/request/window counts plus exact retained byte/history/cleanup proof. V4c
indexed dirty rows, immediate-base edits/incremental namespace certification and
all larger workload/family/DeepSeek/locality gates remain required.

V4b source additionally records fixed actual MinIO PUT/GET call counts, requested
PUT body bytes and received GET body bytes, separately in daemon and authority
processes. These are request/body observations, not physical disk/cache counters.
The sealed build includes this instrumentation; no payload/register algorithm or
limits changed. Full-provider treatment is still NOT_RUN before source freeze.

## 2026-10-02 — V4b full-provider packed treatment passed

Source `fc8a9f8a4f1c8b2cecff155c93cd099bb748c652`; one prospective treatment,
child6.275378 s, proof20.155667 ms, bytes/mode/retained-version/head/parent/cleanup
PASS. Three known uploaded packs per Commit, private window peak18objects/5674
canonical bytes. Overwrite calls: one batch lookup, three batch registrations,
one publication (five total); one metadata connection across both Commits.
Observed Commit44.687959/41.091458 ms remains cache INELIGIBLE. Actual daemon PUT
count3 per Commit; overwrite GET4 body2593bytes. Authority GET26 between completed
publications still includes whole candidate validation; locality is NOT proved.
Raw failed tight-group boundary and corrected pass are retained with checks.

V4b's declared small-path packing/locator/window gate is complete. Physical
simultaneous resource, shipping pooling/delta, large import and all locality
gates remain incomplete. Next: V4c immediate-base SQL overlay/changed catalogs,
local file edits, paged namespace construction and incremental certification;
then generic namespace/package syscall scope and all owner workload groups.

## 2026-10-02 — V4c responsibility/dependency split

Parent `0e3fee4ba365d7ddb75fb58c54c1d2e0f0af5621`; clean source confirmed.
V4c1 replaces daemon full-population/full-file construction with indexed dirty
rows and immediate-base C1 edits/COW. V4c2 owns actual certified incremental
authority publication, paged import and inherited mount, including removal of
the total512 population limit through representation/admission rather than
raising it. Current authority audit and C1 subtree cycle scans remain explicit
locality/scaling gaps. V4c1 cannot be reported as whole-path locality completion.

Freeze V4C1-SPEC.md before implementation/collection. Next: SQL dirty/new/name
indexes, visible immutable base ranges, replayable current-final edit spool and
prepared rows; preserve all real ACK/validation/C5/known installation behavior.
Goal remains active with all three shapes/seven families/full DeepSeek groups.

## V4c1 indexed construction runtime checkpoint (PARTIAL, provider run next)

Parent `05d4b568ccd8c00a2314b23ed65004b8710a96d2`. Owned experimental runtime:
engine/construction/daemon/lib, new edits/prepared modules, external construction
checks. Replace live-id vectors and base-less rebuilds with partial dirty index,
indexed live-name membership, changed-name tombstones, operation-owned SQL typed
values and keyset/ordinal replay. Existing C1 update_filesystem and apply_edits
perform actual construction against the selected immediate base. Metadata remains
portable C1, storage remains actual C2/MinIO ACK before existing authority audit
and conditional C5 publication. Installation adopts only prepared rows under the
existing mutation lock and clears their extents/name changes; no full inode UPDATE.

Maintain in-memory total-inode count and SQL per-directory child counts in mutation
transactions instead of repeated population counts; parent mtime changes with name
mutations. Same total512/serial512/handle256 limits remain. Explicit operation
limits are512 changed inodes,512 changed names per directory and512 indexed file
edits; refusal retains accepted mutable data, and no larger supported profile is
claimed. Current edit spool shares the owned SQLite connection/cache and is rebuilt
from final extents, not historical writes. Source retirement is still unqualified;
unreferenced immutable source files are retained. V4c2 must provide admission and
retirement/import/certification before the unbounded-population claim.

External tests: six pass (three construction, three engine). Literal independent
byte expectations prove overwrite/truncate/regrow and old-root preservation,
changed-name tombstones/unrelated inode identity, plus1MiB immediate edit and
large-to-small/small-to-large crossing. The large edit serves under4KiB local
replacement bytes, not the unchanged megabyte; full C1 provider reads and physical
containment remain separate unproved observations. This fixture is in-memory
canonical objects plus actual SQLite/source files, not a real-provider speed row.

Commands (own worktree, Cargo+1.85.1, locked): test --test construction --test engine;
clippy --all-targets -- -D warnings; native release build; aarch64-musl release
zigbuild. All pass. Initial compile errors (missing filesystem reexports, enum
name and usize ToSql) were corrected; initial all-target Clippy rejected needless
non-Drop test drops, corrected without suppressions. Passing tests reran only after
new child-count/read-counter source changes and a new counted large-edit assertion.
Prior passing unchanged transport/packing tests are reused by scope; full unchanged
Core suites/examples and Linux Clippy not run. Product source remains unchanged.

This is PARTIAL until the declared real-provider two-head run passes. Full-candidate
host audit, C1 effective-cycle repeated walks, finite population quota, inherited
mount/import, rename/link/symlink and physical resource qualification remain gates.
All named cases, seven families and complete DeepSeek import/generic-locality work
remain open. Next: seal runtime/build identities, run one affected real-provider
4KiB create/overwrite treatment within15s child/9.5s separate proof and publish
exact source/counts/outcomes; cache unknown remains INELIGIBLE.

## V4c1 real-provider gate COMPLETE; V4c/full goal remain incomplete

Parent `1ca095a0e84167a7657445726067ca6f77169618`. One affected frozen treatment,
real public WorkspaceApi/FUSE/SQLite/C1/C2/MinIO/C5 path, exit0 child6.869795792s,
proof20.525584ms. Create Exec17.426792ms/Commit74.806042ms; overwrite Exec17.600875ms/
Commit57.480875ms. Semantic/heads/parent/cleanup PASS, canonical/physical NOT_RUN,
cache INELIGIBLE. Overwrite prepares1inode/0directories/0names,6final byte spans,
7local source bytes; C1 reference touched/scanned1. No speed improvement claimed:
previous packed small-case times were lower.11locator lookups remain in overwrite;
host26GET interval still full-candidate validation. Full raw result and analysis
committed in RESULTS-V4C1.md and v4c1-indexed-treatment/. No unchanged-arm rerun.

V4c1 marked complete only for its declared dependency. Existing finite512 profile,
full audit, cycle work, retirement, canonical/physical/Unknown, inherited import
and all larger groups remain. Next: V4c2 certified incremental authority design,
starting with exact role/graph facts at admitted registration and a namespace
transition proof against a certified base; publish the concrete interface before
replacing any validation. Root content authentication alone cannot certify names.

## V4c2a prospective file-graph authority freeze

Parent `57e3f88d1fe0bae101280aeb23375b1f35d1dc89`. Freeze V4C2A-SPEC.md before
implementation/collection: derive typed immutable file summaries and child
constraints from authenticated canonical registration in the existing global SQL
connection; indexed bounded-depth certificate checks replace only repeated full
file reads. Namespace/portable validation stays enabled; no namespace/canonical/
physical/profile qualification claimed. One affected provider4KiB gate next,
existing15/9.5s bounds/cacheINELIGIBLE. Larger groups remain incomplete.
