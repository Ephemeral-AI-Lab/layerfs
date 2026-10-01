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
