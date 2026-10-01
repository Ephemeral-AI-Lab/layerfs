# V4c3b bounded span and SQL windows: live result

> Status: Research; informative and not a product contract.
> This dependency is COMPLETE. The full integration objective remains open.

Runtime/source parent [7496b7bed3e97dd8decad6c197e2532081ecc035](https://github.com/Ephemeral-AI-Lab/layerfs/commit/7496b7bed3e97dd8decad6c197e2532081ecc035).
[Prospective specification](V4C3B-SPEC.md), [initial runtime](V4C3B-RUNTIME.md),
[source-evidenced correction](V4C3B-CORRECTION.md), and
[immutable corrected live evidence](v4c3b-corrected-native-cohort/).
Experimental tooling, single Workspace/Branch/producer, current512inode/256handle
profile. No shipped product change, release admission or physical capability claim.

## Delivered behavior and independent checks

Indexed predecessor and bounded extent keysets replace prefix scans on reads and
write boundaries. Actual SQLite VM work for late reads is38steps at both64and4097
spans versus old275/16407. Mutation boundary+delete work is50steps at both scales
versus old boundary270/16402. Exact split/source offsets, holes, overwrite, truncate/
regrow and orphan bytes pass external actual SQL/files/C1 checks. These count-driven
diagnostics are algorithm evidence, not a speed or physical-memory admission.

Whole-operation SQL transactions become point/64row cleanup/adoption transactions;
unsealed candidate preparation and explicit installation barriers prevent partial
metadata from being accepted as a certified current root. Tail truncate first
accepts logical size then retires in64row windows; failure preserves accepted readable
bytes and quarantines further installation. Known install only retires sources;
Unknown pending ownership remains. No automatic retry, guessed adoption or durability
expansion. SQLite MEMORYjournal, synchronousOFF,2MiBcache, mmap0 remain declared.

One new-source six-step live cohort uses public WorkspaceApi.exec, real kernel
FUSE, daemon SQLite, C1/C2, actual MinIO immutable uploads/ACK/locators and C5
conditional publication. The closed original full128fixture/first3commands are
reused, followed by64generic overwrites/find, physical zero-local-source/all128names
observation and clean UpToDate, then64ordinary sparse dd writes4096bytes apart.
Sparse length258049; literal expected bytes are X at each write and zero elsewhere,
independently calculated SHA256f4cd34cb0357f6aa871f69c67fde0424f046aa9a772ac8a6dba684a099af619d,
mode420decimal (0644octal). No command recognizer or mutation hook.

Separate readonly verifier checks every file SHA256/path/kind/mode/size and exact
head/parent chain at all six roots. Six publication operations create five Commits;
clean fifth state retains fourth root/head. Final138files/1373337payload bytes.
Semantic/history/cleanup PASS. Independent canonical root vectors and physical
memory/cache/SQLite journal containment NOT_RUN. Known-reply roots are seals,
not independent canonical expected pins.

## Observed timings and work

|Step|Exec ms|Commit ms|Semantic/history proof|
|---|---:|---:|---|
|seed|44.778667|124.058458|PASS|
|target|146.951833|668.963208|PASS|
|localized-successor|3.794667|48.544458|PASS|
|enumeration-churn|99.251500|42.564333|PASS|
|clean-retirement|81.661500|17.964000|PASS|
|sparse-span-read|84.787417|59.615500|PASS|

External performance child8.082603166985791s (<15); separate proof4.900773041008506s
(<9.5). Internal nested lifecycle7284.712709ms and proof4876.950875ms are not added
to the external walls. Cache state unknown: all timings INELIGIBLE for speed
admission. One affected-source cohort only; no favorable-sample selection.

Final cumulative span work438predecessor/438range queries,305rows/16143VMsteps.
Mutation627boundary/281delete queries,0deleted rows/13135VMsteps,63truncate batches/
63tail rows. Installation151inodes/140extent batches/218extents,12name batches/
146names,151prepared rows/2edit batches; peak64rows. Actual source retirement281files
in203batches, max64; pending=false. Directory15pages/519decoded rows, max64rows,
1308payload and3292owned-capacity bytes. Decoded rows include bounded kernel reply/
peek rereads, not unique names. Sparse step adds64extents and64source retirements.
Actual script+construction counters are not isolated physical-I/O measurements.

## Failed predecessor and verification scope

Initial runtimefb16b6c907700361f680ee0f7ed4fb5b281ccddb performance child7.963867792s
completed all6operations and cleanup, but separate proof failed after4.187174042s
on sixth-state mode: expected0644 was parsed as decimal644, actual420. First five
roots passed. Original FAIL and raw evidence remain unchanged in
[v4c3b-initial-failed-cohort](v4c3b-initial-failed-cohort/). Independent oracle mode
correction and demonstrated write-prefix/truncate fixes produced this new source;
this is not a repeat of an unchanged passing speed arm.

Initial24external SQL/files/C1checks passed; after mutation changes17covering checks
passed and7unchanged namespace checks were reused by scope. Host locked all-target
Clippy, fmt, native release and Linux musl release passed; Python preparation compile
passed. External useless-vec lint was corrected; failed output retained. No unchanged
passing suite replay. Full unchanged Core suites/examples/boundary and Linux Clippy
unrun for this tool-only checkpoint. No CI or retired aggregate preflight.

## Identities, ownership and reproduction

Owned branch codex/phase6-metadata-experiments in
/Users/yifanxu/.codex/worktrees/phase6-sqlite-minio-design/layerfs.
Native macOS MinIO/global SQLite/C5 and Linux Docker VM daemon/FUSE; provider
process drained, test data retained. Private provider credentials/logs excluded.
Image sha256:f1a2fee8c6f1e6ed6edac9715ba624a64a45c0cd91d5cc4175be05f246a70fbb;
host4b02c44d626c2efadb31bf239e1d9082ad5b500700b056f2138028e56a9c69cb;
Linux8540ca9a03a49fba200b2d9e1c177fbbbda7ab6ae5d87bd40780094d261433df.
Case8c702ca7d4af806e5725ceb3d60b8e168bde2ab8bb835d9e70d17022b690e136;
proof plan4bdd8d12659cdb8a795e33c539d757d6e54c1e78cb2ef7d52e6b3a0239c6d19f.
Identity receipt pins dependency lock, root ARMv8 flags, harness, scenario and MinIO.

Recorded invocation (fresh output required for reproduction):

```
python3 core/benchmark/phase6-live/run.py \
 --output benchmark-results/phase6-integration/v4c3b-stream128-corrected \
 --minio /Users/yifanxu/.codex/worktrees/issue290-storage-probes/layerfs/benchmark-results/storage-probes/deepseek-full-import-v1/minio-provider/minio \
 --image sha256:f1a2fee8c6f1e6ed6edac9715ba624a64a45c0cd91d5cc4175be05f246a70fbb \
 --driver benchmark-results/phase6-integration/binary-archive/4b02c44d626c2efadb31bf239e1d9082ad5b500700b056f2138028e56a9c69cb/phase6-live-probe \
 --case-file benchmark-results/phase6-integration/v4c3b-stream128-corrected.case \
 --purpose 'V4c3b corrected bounded mutation/span/SQL work and independent decimal oracle mode; original full128plus sparse6root source-covering diagnostic; failed predecessor retained'
```

All spec/runtime/correction comparisons use actual first-parent/final staged trees:
reference65417->65417(+0), Core70279->70279(+0), combined135696->135696(+0).
Method tools/production_loc.py SHA256c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb,
same counter over Git archives, including shipped SQL/excluding tools/tests/docs.
This result commit gets its own counted staged/committed comparison and issue link.

Next V4c3c: paged inode reservations with exact owner/profile admission, then
inherited namespace import/mount. Fixed population512, handles256, changed/edit/
verifier limits still apply; merely raising constants does not establish streaming.
Remaining generic syscall, Unknown/provider/canonical/resource gates, genuine public
SDK Init, all seven owning families and complete DeepSeek commands/locality remain
open/NOT_RUN. Full objective active; #288 and issue closure untouched.
