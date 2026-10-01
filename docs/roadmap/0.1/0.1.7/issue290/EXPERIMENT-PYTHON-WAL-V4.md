# Python SQLite WAL/synchronization experiment v4

Status: Research; informative and not a product contract.
Owner clarified2026-10-01: use Python for new WAL/fsync experiment. Rust port is
cancelled/unexecuted. Parent530ab63e7572447c90d3117a75daf8a5a101b8d1; tracking#291.

## Frozen12 selections

Five profiles at128 and1024 locators/publication, eight sequential publications:
W-128-delete-full-v4,W-1024-delete-full-v4 (DELETE/FULL,fullfsync0);
W-128-delete-fullfsync-v4,W-1024-delete-fullfsync-v4 (DELETE/FULL,fullfsync1);
W-128-wal-normal-v4,W-1024-wal-normal-v4 (WAL/NORMAL,fullfsync0);
W-128-wal-full-v4,W-1024-wal-full-v4 (WAL/FULL,fullfsync0);
W-128-wal-fullfsync-v4,W-1024-wal-fullfsync-v4 (WAL/FULL,fullfsync1).
W-reader-normal-v4,W-reader-fullfsync-v4 use1024 locators x8 publications with
one actual pinned generation0 reader, PASSIVE checkpoint while pinned, reader
release then final TRUNCATE checkpoint. One producer/SQL writer per selection.

Use the V2 catalog schema/publish function: simulated uploaded pack registration,
locator insertion, Commit row and guarded Branch-generation update. No physical
MinIO upload, canonical construction, daemon, sandbox, product-profile change or
#288 campaign. Different modes are new explicitly registered treatments, not
repeats of the completed V2 arms. All twelve share one provider/source/workload.

## Fixed native provider through Python

Published official SQLite3.51.3, SQLITE_SOURCE_ID
2026-03-13 10:38:09 737ae4a34738ffa0c3ff7f9bb18df914dd1cad163f28fd6b6e114a344fe6d618.
Download official2026/sqlite-amalgamation-3510300.zip, verify sqlite3.c against the
release's SHA3-25632d5424f97e0a7fc5ed2f6335afbb58be4e0298bd7117a34e39d345ff13d859e,
compile that unchanged source to an isolated macOS ARM64 dylib with system clang
-O2 -DSQLITE_THREADSAFE=1 -DSQLITE_ENABLE_COLUMN_METADATA -dynamiclib. Record full
command/compiler/source/binary hashes. No patch/vendor/fork or global installation.
Override this Python process's library search path only. Assert actual Python
sqlite_version/source_id and loaded library path; fail explicitly if unavailable,
never fall back to3.51.2. SQLite3.51.3 contains the official WAL-reset fix.

## Profile and timing boundaries

Each selection creates a fresh database with identical schema; common settings
cache_size=-512,mmap_size0,busy_timeout0,temp_storeFILE,foreign_keysON,page_size4096,
WAL autocheckpoint1000 pages,checkpoint_fullfsync0. Read back effective settings.
Fullfsync1 applies to all syncs on this macOS host; fullfsync0 uses ordinary VFS
sync. No crash/power-loss proof or Linux storage promise follows pragma timing.

Schema, branch0 and profile setup are outside operation timers but inside the
complete child. For WAL, perform/setup-record TRUNCATE before the first timed
publication so the schema's WAL work is not attributed to publication. No warming
sample; cache/native/OS residency remains unknown. Python fixture generation/
bindings, SQL mutation and COMMIT/ROLLBACK follow the existing publisher.

Measure eight publication acknowledgements with mutation and SQL COMMIT event
terms. Then measure final WAL TRUNCATE checkpoint, or an explicit not-applicable
zero checkpoint term for DELETE. Report publication mean, checkpoint time, their
sum and complete external child wall: no WAL writeback work is hidden in setup or
moved outside collection. Record WAL bytes before/after, returned frame/checkpoint
counts, all settings and exact state. Close time outside inner timers recorded.

Reader cases open/configure both real connections before pinning. Reader BEGIN
plus generationSELECT establishes its actual snapshot. Writer commits all8;
reader still sees0, fresh writer sees8. PASSIVE checkpoint must leave incomplete
frames while snapshot is retained. Release reader, final timed TRUNCATE must
complete and shrink WAL to0. No sleep establishes overlap. All reader release/
checkpoint/close work lies in complete performance command and named phase terms.

## Proof, bounds and claims

Independent separate Python verifier reopens through same fixed provider, checks
integrity, exact fixture-generated packs/locators/Commits/Branch generation8,
publication event outcomes, checkpoint success/WAL state and reader snapshot
facts. Remove only owned case database/WAL/shm files; preserve receipts/failures.
One sample per case/profile, fresh append-only output; no median/best-of/retry.
Performance15s, separate proof/cleanup10s. Provider acquisition/build is sealed
setup outside measurement. Historical V1/V2 contexts are not matched speed arms.

All numerical rows INELIGIBLE/performance_claim=false: setup/pages/read-after-write
may warm caches. No physical cache/peak/I/O or power-loss proof. Compare profiles
only at frozen source/provider with exact workload/transaction/count definitions.
A faster NORMAL commit cannot be called equivalent acknowledged durability to
FULL. A WAL size threshold is not a hard cap under pinned readers; report actual
progress/retirement without claiming infinite-read safety. Final report and#291
checkpoint include every row/failed observation, source and per-commit LOC.
