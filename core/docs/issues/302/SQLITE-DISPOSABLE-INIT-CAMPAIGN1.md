# Supported Disposable Init campaign1

> Status: full four-tier campaign completed;3PASS/1FAIL. Init family not qualified.

Frozen source7b3433afc, unmodified Phase4.5 source7edddbdb8; one prospectively
declared sample per case/arm, baseline then candidate at100/1000/10000/100000.
Candidate uses supported direct-open Disposable MEMORY/OFF, not intervention.
All actual effective setting readbacks match; default page4096/cache-2048,
foreign keys1/temp_store2/mmap0/fullfsync0/checkpoint_fullfsync1, no WAL checkpoint.
Required allocation release/final close remain measured. Baseline native MEMORY/
OFF/page-cache geometry remains original. Every arm uses release/locked binaries,
worktree-local targets, immutable binary archives and prepared source reuse.
Driver binaries are identical across all four rows within each arm: candidate
0af5714e533f82e595efad4d968052032c94c2b732eb11982fb809361b8a59a5,
reference257b0b00d32489571c389e58fa2b13970705a32a507aff478367c6bcbcf5c7ca.
Exact source/harness/compilation/dependency/config/fixture/helper/binary identities
in receipts and declaration; raw append-only outputs under
benchmark-results/fs-bench-pro/issue302-disposable-init-campaign1.

Complete lifecycle comparison in milliseconds; no startup/teardown subtraction:

|Files|Phase4.5 ms|Candidate ms|Candidate/reference|Joint gate|Reference allocated B|Candidate allocated B|
|---:|---:|---:|---:|---|---:|---:|
|100|45.800458|48.748750|1.064372544|PASS|7,372,800|5,214,208|
|1,000|136.441292|138.367042|1.014114129|PASS|23,101,440|20,529,152|
|10,000|1672.956458|1842.982916|1.101632327|FAIL|313,556,992|305,016,832|
|100,000|6046.835208|6081.998333|1.005815129|PASS|518,029,312|514,977,792|

All eight arms complete, source mincore resident_after=0, roots match and separate
sampled namespace verifier PASS. Every path/kind/directory metadata and complete
metadata/content of the declared deterministic sample checked; no full-byte
oracle claim. Scratch cleanup PASS; every15s performance command and9.5s proof
bound passes. Largest command: candidate10000014.213491333s; proof2.556539792s.
Complete CLI wall including build, fixture custody and evidence hashing is
separately disclosed; the performance budget covers cold preconditioning through
child/cleanup as prospectively specified, not build or independent proof.

10000 is an exact FAIL:10*1,842,982,916=18,429,829,160>
11*1,672,956,458=18,402,521,038. Candidate exceeds allowed time by2,730,812.2ns
(2.7308122ms), not a rounding PASS. That row is never resampled unchanged.

Existing receipt breakdown identifies a final-close term at10000:
reference bootstrap6.083250ms/Init1664.169250ms/close2.703458ms;
candidate bootstrap3.831333ms/Init1611.702166ms/checkpoint0.224792ms/
close227.109458ms. Candidate Init itself is52.467084ms faster; complete lifecycle
still fails. At100000 candidate Init5769.680292ms vsreference6039.122375ms,
while close307.590500ms vs0.537375ms. These are observed spans, not proof of
which close subsystem caused the delay. No durability downgrade, moved close,
cache/worker/buffer relaxation or timer subtraction is allowed.

Candidate actual statements/VM steps/transactions/write commits:

|Files|Statements|Actual VM steps|Transactions|Write commits|
|---:|---:|---:|---:|---:|
|100|157|36,866|20|10|
|1,000|299|203,505|31|16|
|10,000|3,981|2,516,074|711|133|
|100,000|9,239|14,751,737|852|436|
These are production counters, not EXPLAIN instruction counts. Nested SQL,
COMMIT, transaction and save spans are not additive.10k COMMIT414.743909ms,
301,569,299 sealed body B/1390 inserts;100k COMMIT837.621618ms,
506,408,509 sealed body B/2230 inserts. No current-round reference SQL/VM/VFS
observer evidence or physical write/sync claim is inferred. A labelled10k
close/write/sync mechanism diagnostic is the next action; ordinary gate numbers
remain unchanged. Cold preconditioning also costs6.995155667s/reference and
8.115410916s/candidate at100000, leaving little command headroom. Namespace
capacity grows with count (candidate100000 entry15,385,064/job25,737,184/
inode8,888,088B); this is disclosed, not a bounded whole-importer memory claim.

All three history cases and Durable measurements NOT_RUN. Full Disposable Init
milestone requires all four PASS at a frozen qualified treatment; current3PASS
is useful evidence but not family completion. Reports/identity checks are retained
without rewriting the historical intervention or previous durable failures.

## Close mechanism observer prerequisite

The first-party diagnostic now delegates original VFS methods and adds actual
xClose count/wall by file class. A public-API interposer records sqlite3_close/
close_v2 and matching OS descriptor close, preserving arguments/results/errno.
It installs the delegated original VFS inside the first open, without overriding
SQLite settings, adding retries or changing product/dependency code. No trace
counter reset occurs. VFS write/sync and underlying runtime identity remain
observable. These calls include nested work and cannot be summed as disjoint
phases or equated with physical device bytes. The observer is a diagnostic,
never a new eligible speed arm or a way to promote the historical10k failure.

A disposable1MiB MEMORY/OFF SQLite capability fixture proves real SQLite/VFS/
OS-close counters, zero VFS sync, no live VFS file/close errors and preserved
original content. Observer compiles with clang-O2-C11-Wall-Wextra-Werror;
source/binary SHA seals and raw output in checks/close-observer-capability1.
No required case was rerun for this capability check. Next run is prospectively
labelled10k cause diagnostic, one count-driven invocation per arm on the exact
archived release vehicles and original cold fixture, no numerical admission.

Compact copies in checks/disposable-init-campaign1 omit large owned databases;
original per-run manifests apply to the original raw run paths, not the compact
subsets. Raw complete evidence remains retained and immutable.
