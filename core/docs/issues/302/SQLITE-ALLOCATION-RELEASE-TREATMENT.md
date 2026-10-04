# Explicit checkpoint unused-allocation release

Status: Functional/capability/lifecycle qualification passes; competitive gate
unmeasured. Based on269e520f7. Prior current main logical515006464B vs baseline
515342336B but allocation521019392 vs517996544B; current excess6012928 vs
2654208B accounts for3022848B gate overage. Do not relabel physical allocation
as logical bytes. Disposable APFS extent transfer removes16MiB extra allocation
while preserving4096logicalB; same-size truncate does not.

Use existing safe nix0.31.3, already locked, targetmacos/fs feature. No unsafe
product code/third-party edits/new package identities. Early offline lock
regeneration attempted unrelated upgrades; that build was stopped, all original
third-party pins restored, and final lock diff adds only the first-party edge.
Exact package identity-set comparisonPASS. Intermediate logs retained.

Session retains a writable main-file descriptor. An unobstructed explicit
checkpoint transfers only extents beyond actual logical EOF to an exclusively
created same-volume scratch, closes/removes it, and records before/after main
allocation. No data read/copy/truncation. No-op when no extra allocation; readonly
and busy checkpoints do no release. Unsupported capability/volume and definite
filesystem/cleanup refusal are explicit; uncertain transfer/close/metadata
outcomes quarantine, without retry or source deletion. No temporary workers.
Scratch ownership is independent of transfer outcome; it holds no logical Store
payload. Concurrent source-length change returnsBusy instead of a false release
claim. No adversarial path-replacement or whole-importer memory proof is added.

Checkpoint wall includes the complete operation, including failure; whole Init
clock includes it before final close. Harness checks allocation scratch as well
as ordering scratch. WAL/FULL/fullfsync/autocheckpoint/page/cache/worker and
canonical/physical caps remain unchanged. No release or durability waiver.

Public-API allocation regression first failed on16MiB unused physical allocation;
final4 allocation testsPASS: data/length preservation and canonical read/reopen,
readonly pre-SQL refusal, obstructed checkpoint skip, scratch-creation refusal
with intact usable Store.9 publication testsPASS;2 process-crash tests including
SIGKILL after completed checkpoint cleanupPASS; project100/1000 full namespace
oraclePASS (16total). Missing trait-import compile failure corrected, logs kept.
Scoped persistence/project all-target Clippy-Dwarnings, core fmt/boundary438files/
23self-tests and4harness testsPASS. Other unchanged suites not repeated.
Physical power-loss and interruption inside the kernel transfer are not claimed.

Prospective next: one release/locked matched100000-file Init-v2 pair at frozen
identity, complete<=15s, separate sampled proof<=9.5s, sourcecontentcold attestation
inside envelope, product create/import/checkpoint/release/close all timed, exact
root comparison and allocatedDB/WAL/SHM<=matchedreference. Other tiers/history
remain required. Previous failures/timeouts retained; no identity resampling.


## Matched outcomes atb9a31c755

Largest reference complete product5509839083ns, envelope12519750500ns, final
518029312B and proofPASS. Candidate cold9577450416ns after126206resident pages/
99000invalidated files, final0; only5421834500ns product remaining. Watchdog kills
product; comparison unavailable, proofNOT_RUN, complete15012046292nsFAIL.
Partial341667840B is not an allocation verdict; ordering/allocation scratchPASS.
Retain timeout, no unchanged identity rerun. GateINCOMPLETE.

A separately selected10000case at the same frozen product/harness completes.
Reference1561609792/candidate2475484209ns, exact10*current24754842090>
11*reference17177707712: timeFAIL. Exact roots match, sampled independent proof,
cold-content attestation/cleanup/budgetsPASS. Final allocated308314112/305098752B
passes matched no-growth. Product checkpoint/release6048625ns; main allocation
308535296->305065984B, releases3469312B without logical payload truncation/copy.
Performance envelopes2214978666/3414307750ns; separate proof578582167/1343118542ns.
The release is paid inside product time, not an untimed filesystem adjustment.

Current10000work3974statements/2522963executedVM,676transactions,141writecommits,
commit1290462180ns,1408sealed bodies/301574052B. C2bodyreads4911B,zero individual
payload reads;23reservations/115publications. Inclusive spans overlap. No physical
sync-call or cross-window latency-delta claim. This solves this pair's allocation
failure category, not time parity or largest admission. Other100/1000Init and
all3historiesNOT_RUN at this identity; all7 remain required.

Raw issue302-allocation{10000,100000}-{baseline,candidate}-treatment1; comparison
JSON issue302-allocation-comparison-treatment1. Commands runner.py run --case
phase7-sqlite-init-<size>-v2 --arm baseline --baseline-root
target/phase7-baseline/layerfs --out <fresh-owned-output>, then --arm candidate;
read benchmark_agent_report.md before each invocation. Identities, helper/native
resource domains, actual cache numbers and all failures preserved in receipts.

Next product focus is publication/encoding/validation work under existing byte,
row and producer limits, plus actual history driver/proof/budget binding. Do not
extend deadlines or turn logical bytes into a physical allocation verdict.
Goal active. Code commit ProductionLOC137544->137678(+134),reference65417/
core72127->72261,active28083->28217/inactive44044,old191/new7673->7807/rest64263;
exact snapshot method in commit. Evidence-only followup+0. No CI/preflight/push/PR.
