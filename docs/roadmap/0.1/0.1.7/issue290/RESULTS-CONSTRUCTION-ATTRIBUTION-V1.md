# Construction attribution: full deepseek-harness corpus

Status: Research; informative and not a product contract.

One count-driven diagnostic completed with exact output invariance. Source open/
read/close consumed35.90% of this instrumented invocation, C2 FULL encoding28.89%,
C1 residual13.60% and measured Store/catalog SQL14.21%. Indexed CAS membership
was1.106156s across255,544 demands. The evidence supports investigating source
file overhead and payload encoding first; a wholesale SQLite replacement is not
justified by this cost breakdown.

This is diagnostic attribution, not a new throughput arm. Existing34.5s original
construction/4.6s upload receipts and the original10s proof timeout remain
unchanged. No optimization, additional producer, upload, acquisition or product
source change occurred. No comparison of instrumented wall to original wall is
used as a speed result. Cache INELIGIBLE/performance_claim=false.

## Identity and proof

[Prospective contract](CONSTRUCTION-ATTRIBUTION-DIAGNOSTIC-V1.md) was committed
at231a5b3a27756eb4c15f89a0774b4a6bdc621149 before instrumentation. Instrumentation
and runner source f4d178f4f54c2c77ec1186c9d836825bf6eb12d8, published before the
single admitted invocation. Owned branch codex/issue290-storage-probes and
worktree /Users/yifanxu/.codex/worktrees/issue290-storage-probes/layerfs.
[Identity](evidence-construction-attribution-v1/identity.json) records actual
release binary, repository ARMv8 config, tool, closed master, original catalog,
original failed proof and exhaustive remainder proof seals.

Reused master includes all103,108 files,3,475,776,149 regular-file bytes,
16,868 directories,10,070 symlinks and130,046 xattrs. Manifest SHA256
541f55db9814837fd60386e42e56781e80fbdbd6d555d4bb0e3a099e215139af.
Original independently verified reference import catalog SHA256
8514f24e74c1718fd1839a60428fa894316cc6c449772c500521bbe2968c150b.
No source repository edits. Same C1 default128KiB cutoff/CDC8–32KiB and C2 FULL
codec/group/pack policies, single producer,256-object/file transaction cadence,
MEMORY/OFF/cache512KiB/mmap0. Actual Rust-linked SQLite3.51.0; no WAL writes.

Complete diagnostic command34.372307s <=180s; measured constructor scope
34.326521s. Separate invariant proof5.287048s <=10s, PASS. Every entries/xattrs/
objects/packs row compared bidirectionally, and every packed body compared
byte-for-byte against the independently reconstructed original reference.
Only allowed catalog difference: diagnostic publication.ready=0 versus original1.
Counts unchanged:210,332 unique objects,45,212 duplicates,5,836 packs,
1,389,188,326 pack bytes and103,108 file roots. This reuses the previous independent
original-byte reconstruction proof through exact output equivalence; no second
full reconstruction or speed sample ran.

## Disjoint accounting

| Region | Elapsed | Share of measured constructor wall |
|---|---:|---:|
|SQL|4.878804s|14.21%|
|c1_residual|4.668990s|13.60%|
|c2_FULL|9.918552s|28.89%|
|c2_control_residual|0.102654s|0.30%|
|group_build|0.096632s|0.28%|
|pack_assembly|0.053799s|0.16%|
|pack_hash_write|2.121259s|6.18%|
|setup_copy_resource_close_outer|0.054693s|0.16%|
|source_open_read_close|12.324800s|35.90%|
|source_sql_enumeration|0.106336s|0.31%|

All rows sum exactly to34,326,520,833ns. Source enumeration is a separate0.106336s
SQL read of the master; including it with Store/catalog SQL gives4.985140s/14.52%.
C1 construct inclusive27.607012s contains7.577034s source reads and15.360988s
consumer callbacks. The4.668990s C1 residual includes chunking, canonical identity
hashing, mapping/tree construction, buffers, control and clock overhead. It is not
an isolated chunker CPU measurement. Global C2 child regions include work in
callbacks plus final sealing and per-file catalog updates, which explains why
those child sums cannot all be subtracted from callbacks alone. The published
[analysis](evidence-construction-attribution-v1/analysis.json) records the explicit
parent/subtraction arithmetic and positive control residual.

Source calls:103,108 opens4.666622s,317,143 reads7.577034s/3,475,776,149 bytes,
103,108 closes0.081145s. These are kernel-call elapsed with unknown page cache,
not physical device bandwidth or pure wait time. File-open measurement includes
building its path. Catalog fs::copy returned75,501,568 logical bytes in0.183ms;
copy/clone implementation and physical data movement are unobserved, so it
cannot be used as storage throughput. Input-master construction remains the
original independent byte-copy acquisition; no source-data pretouch occurred.
Identity hashing read master/reference metadata files before the diagnostic;
metadata cache residency therefore is not a cold claim.

Pack assembly0.053799s; pack BLAKE3 hashing0.963185s; filesystem writes1.157480s.
Hash/write enclosing region2.121259s includes a0.594ms nested clock/plumbing
residue. Writes include file creation/write/close and no fsync. Group build
0.096632s includes optional ordinary-lane compression:117 ordinary compressed
groups,28,972 raw native groups,3,348 raw whole-file groups. Exact group bytes and
all physical locators matched the original.

C2 FULL encoding9.918552s: chunks7.406787s/119,353 unique records;
whole files2.510600s/87,696 records; mapping/file-state roles1.165ms combined.
The existing public probe diagnostic1.438016s is nested inside FULL encoding;
other FULL work8.480536s includes compression, canonical payload extraction,
framing and allocation. Neither number isolates Zstandard alone.23,920 chunk and
4,002 whole-file records were verbatim; others in those payload roles used codec
frames. Probe cost alone does not show whether removing it improves total work:
its purpose is to avoid full compression of incompressible payloads.

Whole single Rust child CPU:18.379409s user +9.066252s system =27.445661s;
CPU/complete-command wall0.7985. No per-region CPU attribution, device wait or
physical-memory conclusion follows. Timer/counter overhead is included and not
calibrated away; mixed-input4096-file intervals are recorded in
[CSV](evidence-construction-attribution-v1/stage/intervals.csv), not a scaling curve.

## SQL calls and file cohorts

| SQL region | Calls | Elapsed | Average per call |
|---|---:|---:|---:|
|sql_membership|255,544|1.106156s|4.329µs|
|sql_object_insert|210,332|0.664600s|3.160µs|
|sql_locator_prepare|5,836|0.001458s|0.250µs|
|sql_locator_update|210,332|0.629196s|2.991µs|
|sql_pack_insert|5,836|0.028838s|4.941µs|
|sql_file_root_update|103,108|1.462983s|14.189µs|
|sql_transaction|1,224|0.947684s|774.252µs|
|sql_validation|3|0.037890s|12629.889µs|

Membership hits45,212/0.169498s, misses210,332/0.936659s. One indexed demand per
emitted object; no whole-population scan. Per-file root UPDATE103,108 calls1.462983s
uses Connection.execute, which prepares the statement each time; object/locator
paths use cached statements. Different tables/operations have different work, so
the14.189µs versus~3µs averages are not proof that statement caching removes that
difference. Transactions are821 object-cadence +402 file-cadence +1 final batch;
a COMMIT/BEGIN batch is two SQL statements, not one transaction instruction.

| Cohort | Files | Original bytes | Read elapsed | C1 inclusive | Consumer inclusive |
|---|---:|---:|---:|---:|---:|
|empty|274|0|0.000225s|0.003286s|0.001824s|
|small|101,494|523,127,919|6.426659s|11.422978s|4.272668s|
|large|1,340|2,952,648,230|1.150150s|16.180747s|11.086495s|

Small files are98.43% of files and15.05% of bytes, but account for6.426659s of the
7.577034s read calls. Large files contribute84.95% of bytes and1.150150s reads.
C1 residual is0.723651s for small versus3.944102s for large files. Encoding-role
split locates most FULL work in large-file chunks. Open/close times were recorded
globally, not separately by cohort; cache/input ordering is uncontrolled and
this is not a controlled per-size speed comparison. Consumer costs belong to the
triggering file; cross-file pack tails prevent exact encoded-byte allocation by
cohort. Empty274 files have zero logical bytes and exact output equality.

## Evidence-based next actions

1. For source costs, inspect actual open/read syscall and page-fault behavior on
   the small-file path. The measured fixed per-file costs are substantial;
   batching SQL alone cannot remove them. Preserve real source acquisition work
   inside the measured scope; do not prepack/warm input to hide the cost.
2. For CPU costs, narrow the C2 FULL region into payload probe, full codec and
   framing/context work using existing public evidence or an external profiler.
   Payload encoding9.92s is meaningful; generic group machinery0.097s is small.
   Preserve the accepted codec profile/storage ratio and canonical output.
3. A low-scope SQL experiment can reuse the file-root UPDATE prepared statement.
   Its measured total1.463s bounds its direct savings budget in this invocation;
   no savings number or speed PASS is predicted. Membership itself is only1.106s,
   and all measured Store/catalog SQL4.879s, so these are smaller opportunities
   than source+payload work. Do not remove validation or change resource quotas.
4. Initialization concurrency could address serial CPU and file waits if a
   separately authorized, bounded profile permits it. This diagnostic makes no
   producer/worker increase and gives no parallel speed forecast; normal Commit/
   capture construction remains one producer under repository rules.

No optimization implementation or new comparison arm is included in this
checkpoint. The next intervention must freeze its prospective treatment/profile,
retain a matched oracle and compare one admitted sample per arm where required.
No #288 campaign, benchmark speed admission, SDK Init/Commit/Exec, mounted
bootstrap, history/canonical namespace, cloud fencing or physical-resource proof.

## Checks and custody

Locked release example build, warning-denying scoped Clippy and full-workspace
fmt check passed. Product boundary272files passed,7 guard selftests passed;
external four-file fixture validated nested accounting, counts, exact catalog and
pack output equality. After adding hit/miss subregions, the narrow covering
fixture checked the new partitions and unchanged counts. Source is product-only;
all new counters live in the external example. No inline product tests/hooks,
new dependency, third-party patch, fsync or profile tuning. Full Core suites were
not rerun for external tooling-only changes; owning previous product tests remain
their original-source evidence. No Linux/physical containment/CI/preflight run.
[Check logs](evidence-construction-attribution-v1/checks/selftest-v1.json),
[final subregion check](evidence-construction-attribution-v1/checks/selftest-v2.json),
[verification](evidence-construction-attribution-v1/verification.json) and
[SHA256 manifest](evidence-construction-attribution-v1/MANIFEST.json).

Private diagnostic stage retained at
benchmark-results/storage-probes/construction-attribution-v1/stage in the owned
worktree. Original master/import unchanged; no provider started, no server left
running. Before effects12GiB additional available disk was required; stage pack
bytes1,389,188,326 <4GiB, catalog retained privately. Physical allocation/cache/
engine memory remain unavailable. Public evidence has counts/hashes, not source
paths/content/catalog or credentials.

Per-commit production LOC for231a5b3a2 andf4d178f4f is unchanged:
combined117,426 ->117,426 (+0); reference65,417 ->65,417 (+0);
Core52,009 ->52,009 (+0). Same tools/production_loc.py SHA256
c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb,
Git archive first-parent/final staged snapshots, committed tree match; shipped SQL
included, tests/examples/tools/docs excluded. This report-only checkpoint likewise
changes no production source; its exact comparison is recorded in the commit.
