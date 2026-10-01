# Python WAL and synchronization results v4

Status: Dated planning checkpoint; not release evidence or a product contract.
Owner clarified Python WAL/fsync comparison, not Rust port. All12 selections ran
once at b4669b00dd17c94c59de8d1fcb90a7bf53383dd7 under the
[prospective specification](EXPERIMENT-PYTHON-WAL-V4.md). All commands and exact
reopened proofs/cleanup passed; cache INELIGIBLE/performance_claim=false remains.

| Locators per publication | Profile | Mean acknowledgement ms | Final checkpoint ms | Eight publications + checkpoint ms |
| --- | --- | ---: | ---: | ---: |
| 128 | delete-full | 0.681 | 0.000 | 5.450 |
| 128 | delete-fullfsync | 14.101 | 0.000 | 112.807 |
| 128 | wal-normal | 0.286 | 0.548 | 2.835 |
| 128 | wal-full | 0.335 | 0.427 | 3.110 |
| 128 | wal-fullfsync | 5.460 | 4.881 | 48.559 |
| 1024 | delete-full | 1.884 | 0.000 | 15.069 |
| 1024 | delete-fullfsync | 15.764 | 0.000 | 126.109 |
| 1024 | wal-normal | 1.485 | 0.976 | 12.857 |
| 1024 | wal-full | 1.546 | 0.812 | 13.177 |
| 1024 | wal-fullfsync | 6.725 | 6.066 | 59.864 |

WAL/FULL ordinary sync averaged0.335/1.546ms at128/1024 locators. WAL/FULL with
macOSfullfsync averaged5.460/6.725ms versus matched DELETE/FULL/fullfsync14.101/
15.764ms. WAL/NORMAL averaged0.286/1.485ms, with required checkpoint work included
separately. NORMAL has a different acknowledged durability contract; these timings
cannot justify treating it as FULL. No repeated arms or target/worker increase.

Both pinned-reader cases held an actual generation0 snapshot while the writer
published generation8. PASSIVE checkpoint returned[0,138,0]:138 WAL frames,
zero checkpointed while pinned. Reader release then final TRUNCATE completed,
WAL length0. NORMAL case: eight ACKs12.063ms, pinned checkpoint0.054ms,
release0.060ms, final checkpoint1.137ms. FULL/fullfsync:53.560/0.058/0.050/6.254ms.
An autocheckpoint threshold is not a hard WAL-size cap under retained readers.

Fixed provider: official SQLite3.51.3, unmodified amalgamation hash verified
against the [official release](https://www.sqlite.org/releaselog/3_51_3.html).
System clang builds an isolated dylib; only each child gets DYLD_LIBRARY_PATH.
Python version/source_id and dladdr loaded path/hash are asserted. No global
installation, vendoring, patch, Core dependency change or provider fallback.
Orchestrator inventory says3.51.2, but timed/proof children actually load3.51.3;
provider and each result record make this distinction explicit. This release
includes the official WAL-reset fix. Compile options differ from the historical
Homebrew provider, so prior V2 receipts are not matched speed arms.

All profiles share schema, eight publications, cache512KiB,mmap0,busy_timeout0,
temp_storeFILE,foreign_keysON,4KiB pages and WAL-autocheckpoint1000. Native/OS
residency unknown; metadata read-after-write and setup may warm pages. Checkpoint
and reader retirement are paid inside the complete command. No power-loss,
replication, physical peak/cache/I/O or LayerFS Commit/Exec admission is claimed.

Proof checks integrity and exact packs/locators/Commit parents/Branch generation;
reader snapshot and final checkpoint facts passed. There was no MinIO upload in
this group. MinIO server remained stopped. Python syntax,12-case membership and
actual provider qualification passed; product source unchanged, no Cargo/CI/
retired preflight rerun. Source acquisition/build are separate setup receipts.

[Raw receipts and hashes](evidence-python-wal-v4/MANIFEST.json),
[JSON](evidence-python-wal-v4/summary.json), [CSV](evidence-python-wal-v4/summary.csv).
Exact collector: `python3 tools/storage_probes/run_wal.py --output benchmark-results/storage-probes/collection-wal-v4`.
All performance/proof children exit0 within15s/10s, respectively. No cases omitted.

Unexecuted Rust specification29d8e7375 was archived by530ab63e7 on owner correction;
no Rust SQLite driver or samples exist. WAL specificationacb4e1cd3 and harness
b4669b00d, like both draft/archive commits, each compare production117426->117426
(+0), reference65417->65417(+0), Core52009->52009(+0). Same counter SHA256
c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb over first-parent/
final staged Git archives; committed snapshots confirmed. This owning report
checkpoint's exact comparison is in its commit message/#291 update.
