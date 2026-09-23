# #237 v0.1.6 common-source 10k Init arm

**Status: diagnostic, one public run.** The independent source copy had zero resident payload pages at the final pre-call check; directory and inode metadata residency was not qualified. This arm is not a fully cold admission result. The Core arm and SQL plans are reported separately. The historical 578.245 ms / 518.8 MB/s row used different bytes and an uncontrolled source cache, so it is archival context rather than a matched baseline here.

## Identity and method

The product is the unmodified peeled `v0.1.6` release commit `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`; annotated tag object `dbdf0fed6fceba9f72997287eaa7d7ee9ae0fd79`. The isolated benchmark-only branch is `b0730c70a567f5019ba0435d73bfa9f6a04b4004`; its `crates` tree SHA is `dcc4fb6fd01115dcbf91ba02df414e91eb5733be`, exactly the release tree. The release executable SHA-256 is `604bc5c7b3577fdc78acc531353600d28077bb44d668fee90efe28d51e88bffe`. The benchmark wrapper SHA-256 is `5789a47ef189a465aede5b06b9f20c5f4c685fd32dece2bed4f317c07ab2f604`. [Identity](evidence/v016-head2head-a/identity.json) pins the command, seed, nonce, binary, wrapper and shared cold driver.

The shared Core seed-1 `namespace-10000` manifest SHA-256 is `c1d7937c9f90d3585558e6d20b35183a737530d386667d08badc3121a6b3d55e`: 10,000 files in 100 data directories, 300,000,000 logical bytes including the 100-MB anchor. A new manual byte copy served this arm; [copy receipt](evidence/v016-head2head-a/source-copy.json) records 2.573 s of untimed copy preparation. The benchmark created a fresh SQLite Store/client, signaled READY, then the wrapper hashed, invalidated and checked every copied payload file. [Preflight](evidence/v016-head2head-a/cold-preflight.json) and [final recheck](evidence/v016-head2head-a/cold-recheck.json) each found **0/27,503 resident payload pages** across 10,000 files / 300 MB. It sent GO only after the final recheck; the wall-clock recheck-to-timer gap was **0.339 ms**. Metadata cache state remains unqualified. No earlier run's Store or source file was reused for the public call.

The authentic public timer surrounds one `Client::initialize_layerstack(name, LayerStackInitialization::Directory(copy))` call, from just before invocation through return. Store/client setup and external teardown are outside. The fixed v0.1.6 small-content threshold is **128 KiB** (`SMALL_LIMIT=131072`, small values strictly below it); its 2-MiB whole-file-owner maximum is a distinct format bound. Pack payload remained in SQLite `object_packs.data` BLOBs and the DB page size was **4096 bytes**. The run skipped in-timer verification.

## One-run result

| Measurement | v0.1.6 observed | Scope |
| --- | ---: | --- |
| Public Init wall | **750.625833 ms** | One call; [raw stdout](evidence/v016-head2head-a/stdout.txt) |
| 300 MB / public wall | **399.6665 decimal MB/s** | No 400-MB anchor double count |
| Complete wrapper command | 3.863026 s | Includes READY/GO cold preflight and recheck; excludes prior source copy |
| Process CPU | 1,018.043 ms user + 608.434 ms system = **1,626.476 ms** | Four workers overlap |
| Process disk I/O delta | 337,158,144 B read; 6,119,424 B written | Process counters, not a direct page-residency proof |
| RSS | 4,521,984 B at t0; 78,757,888 B at t1; 74,235,904 B incremental peak | Peak status `exact-new-lifetime-high-water` |
| Canonical output | 24,683 inserted objects / 302,156,826 canonical bytes; zero reused | Public operation receipt |
| SQLite Store | 304,553,984 B apparent; 318,783,488 B allocated | Closed original [Store manifest](evidence/v016-head2head-a/store-manifest.json), SHA-256 `79c8b732d6450bea6ceeae989e4721cc0fcac0971bebf4eb25e7721ff10631a2` |
| SQLite pack layout | 1,693 `object_packs` rows / 301,646,854 stored BLOB bytes; 24,683 `objects` rows | Closed Store, `page_size=4096`, 74,354 pages |
| Pack internal used/slack | `NOT_MEASURED` | The v0.1.6 BLOB rows are variable-length; no claimed reserved-capacity comparison |

[Raw performance receipt](evidence/v016-head2head-a/receipt.json) keeps every public metric and the SQLite post-run counts. Its `DIAGNOSTIC` status means payload-cold evidence passed but fully cold metadata admission remains open.

## Micro stages and SQLite work

The [raw aggregate diagnostic and SQL trace](evidence/v016-head2head-a/stderr.txt) provide these per-operation counts. Timers overlap: producer wall sums and send blocking must not be added to public wall.

| Stage/count | Observed | Exact boundary or limitation |
| --- | ---: | --- |
| Directory import (`prepare_import_wall_ns`) | **734.019375 ms** | Inside public call; includes discovery, pipeline, final root/inode construction, admission finish |
| Direct producer/admission pipeline | **709.704333 ms** | Inside directory import; includes four producers, one consumer, join and admission resolve |
| Four producer wall sum / maximum | 2,577.802291 ms / 709.010375 ms | Overlapping workers; one worker handled 172,735,800 B, other three about 42.4 MB each |
| Four producer blocked-send sum | 806.748918 ms | Nested inside worker walls; nonblocked remainder 1,771.053373 ms is **not** pure construction |
| Consumer `recv()` wait | 47.913896 ms | 1,203 slab handoffs plus terminal close; not directly comparable to Core's per-object/per-file receives |
| Pipeline complement of `recv()` wait | 661.790437 ms | Includes admission, thread/loop/join overhead; not pure SQLite time |
| Final root/inode table | 18.960792 ms | Inside directory import, after pipeline |
| Source I/O | 10,000 opens/fstats; 27,951 read calls; 300,000,000 returned bytes | Source-read syscall wall `NOT_MEASURED` |
| Construction | 9,399 small files; 601 in the diagnostic's streaming bucket; 24,933 canonical frames; 1,203 slabs | The streaming bucket includes the 100 zero-length files; per-worker construct-only wall `NOT_MEASURED` |
| Admission | 73 pipeline transactions; 1 publication transaction | Product diagnostic reports 74 SQL batches, including publication |
| SQLite COMMIT wall | 220.649506 ms pipeline + 2.365500 ms publication = **223.015006 ms** | Does not include extra reservation transaction's unmeasured COMMIT wall |
| SQLite BEGIN wall | 0.559208 ms | Product diagnostic's 74 batches |
| Publication phase wall | `NOT_MEASURED` | Only its COMMIT wall is isolated; publication is after directory import but within public call |

The benchmark-only SQLite statement trace counted **75 `BEGIN` and 75 `COMMIT` statements**, one more pair than the admission batches because the initialization path reserves inode serials before its pipeline. It also counted 590 pack INSERT statements, **408 pack UPDATE statements that rewrite an open-pack BLOB**, 591 pack SELECTs, 639 multi-row object INSERTs, 438 object SELECTs, 65 other INSERTs, 44 other SELECTs, four PRAGMAs and three other statements: **2,932 statements total**. The closed Store has 1,693 new pack rows; `1,693 + 408 = 2,101` matches the physical receipt's selected-pack contribution count. The object INSERT statements published 24,683 locator rows. Statement counts are exact for the traced public-call connection; SQL timing spans below the whole transaction are not available in this tag diagnostic (`sql_prepare_ns=0` is an uninstrumented field, not proof of zero work).

## Separate full readback and retained failure

The existing release `repository-init verification` route reopened a byte copy of the closed Store, traversed the namespace and streamed every file against a four-field oracle derived from the same source manifest. The [corrected readback receipt](evidence/v016-head2head-a/readback-corrected/receipt.json) is **PASS: 10,000 files / 300,000,000 bytes**, `verification_ns=3,343,087,666`, complete command 3.361 s. This verification was outside the public timer and did not modify the original Store.

The first separate verifier attempt is retained as [FAIL before content access](evidence/v016-head2head-a/readback/receipt.json): the external setup wrote a newline after the typed layer/root ID, while the release parser requires exact-length hex. The corrected verifier input removed only that newline. There was no second public Init sample. The public performance row remains diagnostic because metadata residency was not proven, despite full semantic readback passing.
