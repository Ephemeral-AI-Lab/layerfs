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
