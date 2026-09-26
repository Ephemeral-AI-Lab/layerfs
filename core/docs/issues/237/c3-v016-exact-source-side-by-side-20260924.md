# #237: exact-source v0.1.6 and promoted C3 Init, by stage

> **Status: Research; informative and not a product contract.** This is a
> side-by-side explanation of two different public Init routes on the same
> seed-1 SHAKE source. It is not a paired speed result or #236 admission.

## Custody and operation boundary

`origin/main` was fetched on 2026-09-24 and resolved to merge commit
`d455ad42cdc59de7224259722f1f3153c3ec3ab6` (PR #239). This
worktree started clean at that commit. The promoted placement change is
`92492722a`; `c3a4766dc` corrected an external test's SQL type. The
original C3 **research** receipt belongs to source
`a3628330863398f0cd02e3c789558fd6b2559722`, product seal
`1751be36a619ad141d3d97aa666379d7971ec1433c877f3e0a740ef1637c7bf0`,
harness seal `96a04c70338d34116c1c66ac04617bb77df368fbcae4e0c1ca29ac04f48e90a3`
and release driver SHA-256
`4181b4d290915d8441becf8d5efc30c9a75d25c333f02e7e8b466651d0e5069f`.
It is **not** a merged-commit performance sample.

The old arm used peeled v0.1.6 tag
`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`, product `crates`
tree `dcc4fb6fd01115dcbf91ba02df414e91eb5733be`, and release
driver SHA-256
`641f710b76599d27437556490c9acd9a4636a9f7dfe6ae33dd1dc9ca211ddca0`.
Its one public `Client::initialize_layerstack(...Directory(copy))` call
enters the [reference Store importer](../../../../crates/layerfs-layerstack-store/src/layerstack.rs).
The C3 driver times one `Client::init_project` call, through
[SDK](../../../crates/layerfs-api/sdk/src/client.rs),
[Service request](../../../crates/layerfs-service/src/project.rs),
[file import](../../../crates/layerfs-service/src/save/import/scan.rs),
[namespace save](../../../crates/layerfs-service/src/save/import/namespace.rs),
and separate History publication. Both drivers create/open their Store before
the public timer. The old single SQLite Store contains content and history;
Core uses a content Store plus History database. Their canonical formats,
admission cadence, and timer boundaries differ.

Both one-shot arms used manifest SHA-256
`23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`:
100,000 files, 1,001 directories, 500,000,000 logical bytes. Each had
an independent source byte copy, a fresh Store, and **0/126,206 resident
payload pages** immediately before the call. Directory/inode metadata
residency was not qualified. Both separate release-binary full reopened
oracles passed every path, portable metadata value, file size and SHA-256.
The [old receipt](evidence/c3-v016-side-by-side-20260924/v016-receipt.json),
[C3 research receipt](evidence/c3-v016-side-by-side-20260924/c3-research-receipt.json)
and [oracle receipts](evidence/c3-v016-side-by-side-20260924/v016-verification.json)
[(C3)](evidence/c3-v016-side-by-side-20260924/c3-research-verification.json)
retain the exact scope and status. The old public call was
**3.779070375 s**, complete child **19.119348541 s** (>15 s), and verifier
**21.895595708 s** (>10 s). Research C3 was **5.013439250 s**, complete
driver **6.397864750 s**, and verifier **8.644194375 s**. The old
complete child includes its cold preflight; C3's preflight was outside
its driver command. Neither raw public time supports a version speedup
or regression claim; cache metadata is unqualified, and the old budget
misses remain misses.

## Separate merged-C3 count diagnostic

The C3 research driver had no stage trace. A single distinct, temporary
count instrument was frozen against merge commit `d455ad42c`, using
[this source diff](evidence/c3-v016-side-by-side-20260924/c3-stage-instrument.diff.gz)
(SHA-256 of decompressed diff `6eea53dfa6d110fc922a576338b89b3e0f604b0814f2f629ecfff0f8a7d71844`)
and [this benchmark-only runner](evidence/c3-v016-side-by-side-20260924/c3-stage-runner.py)
(SHA-256 `24a129481d64a1a143652ea772f11590bf4beb14925842366d843aef5cde259c`).
The run pinned source tree `f66651353a1ce34e4dccf3d73911773f56ba61dd`,
instrumented product seal `42796cc9788ad658cc549124419754eabbd021c6206db5cd936423338b01e394`,
harness seal `92054f746134a409391b7f2d7e241093fd9db6dcd7a8dfc7e1c0f267adbe2c54`,
release driver SHA-256
`b38680813ff61e5d469bad17fceefd408673aabc2ed13164bdca7070deec43f7`,
the same manifest, a fresh independent copy, and a fresh Store. The
[source-copy](evidence/c3-v016-side-by-side-20260924/c3-stage-source-copy.json)
and [launch preflight](evidence/c3-v016-side-by-side-20260924/c3-stage-cold-launch.json)
passed; the separate full reopened
[oracle](evidence/c3-v016-side-by-side-20260924/c3-stage-verification.json)
passed 101,001 paths and 500,000,000 bytes. The runner's retained hash
manifest verified `PASS`. Its public **5.200490584 s** and complete
**6.564908500 s** are *diagnostic observations*, never another C3
performance sample. The instrumented source was restored after the run.

The first output path stopped before any SDK call with a Rust visibility
error in the temporary counter re-export. Its
[failed build receipt](evidence/c3-v016-side-by-side-20260924/c3-stage-build-failed-01.json)
and [compiler log](evidence/c3-v016-side-by-side-20260924/c3-stage-build-failed-01.log)
remain retained. The correction used a fresh `-02` output path. Full raw
diagnostic output, including closed databases, remains at
`benchmark-results/fs-bench-pro/issue237-c3-stage-20260924-02/` in this
isolated worktree; the one-shot old and research C3 raw Stores remain
at the paths named by the handoff. The small evidence files here are
byte-for-byte copies, indexed by
[SHA256SUMS.json](evidence/c3-v016-side-by-side-20260924/SHA256SUMS.json).

## Work by stage

The old numbers below are from its original one-shot diagnostic trace;
the C3 counts and stages are from the distinct instrumented release run.
All durations are observations under their own source and cache scope.
Nested timers and sums across concurrent workers are **not** added to
the public wall.

| Stage or count | Released v0.1.6 | Merged C3 diagnostic | Mapping limit |
| --- | ---: | ---: | --- |
| Source inventory | 1,001 `read_dir`, 2,001 symlink metadata calls; scan-only wall unmeasured | scan **506.622250 ms**, 100,000 file jobs; Service scans and stats each entry | Old scan is inside preparation/pipeline; Core has a separately timed scan. |
| File opens and reads | 100,000 opens and fstats; 205,102 reads, **500,000,000 B** | 100,000 opens; 215,104 counted reads, **500,000,000 B** | Read call sizes and metadata calls differ. |
| Construction and handoff | four workers; direct pipeline **3.301259584 s**; 2,140 slabs, 114,474 canonical frames, 246.138957 ms summed send blocking | four workers; file/handoff span **3.829728167 s**; 2,137 batches, 111,354 object messages; 15.299396578 s summed worker wall and 5.316410275 s summed send blocking | Worker sums overlap each other and the owner. Object grammar differs. |
| Receiving owner | 2.023528518 s consumer idle inside old pipeline | 879.874809 ms receiver wait + 2.943121590 s owner accept, leaving 6.731768 ms of file-loop work | The C3 wait/accept intervals are disjoint on its receiver thread; old idle uses a different pipeline. |
| File Save finish | Included in old preparation; no matching standalone span | 140.863833 ms `finish_call_ns` after file loop | Core has an explicit Save finalization. |
| Pack placement | 2,690 final pack rows; isolated placement wall unmeasured | 19.647563 ms Save `place_ns`, 2,295 pack creations and 1,985 appends | Core's 149.895687-ms `write_pack_total_ns` is a nested diagnostic total, not an additional stage. |
| Root / namespace construction | 452.704542 ms final root/inode table, after direct pipeline but before final admission finish | 719.056500 ms for prerequisites and filesystem tree Save together | Different canonical tree and Save boundaries. |
| History publication | In old public call; separate wall unmeasured | 0.275084 ms catalog `initialize_layerstack` in diagnostic | Core uses a separate History SQLite database. |
| Enclosing importer | 3.760955417 s old `prepare_import_wall_ns`; its pipeline + final-root spans leave 6.991291 ms for other work | scan + files + file finish + namespace + publication = 5.196545834 s; public call leaves 3.944750 ms | Old and Core public boundaries are different. |
| Process setup / teardown | Driver setup 6.389125 ms and teardown 84.114291 ms; complete child has 15.340278166 s outside the public timer, including cold preflight | Diagnostic driver has 1.364417916 s outside its public timer; Host/Store setup and teardown were not split | The complete-command scopes differ. |

The [old trace](evidence/c3-v016-side-by-side-20260924/v016-stderr.txt)
and [C3 trace](evidence/c3-v016-side-by-side-20260924/c3-stage-stderr.txt)
are the sources for this table. The old pipeline and final-root spans
are sequential in its importer, but the old root span excludes
`admission.finish`; the 6.991291-ms remainder contains that work and
other framing. C3's `owner_accept_ns` is nested in the file span, and
its SQL timings are nested in Save work.

### Placement and SQLite

The old trace reports 133 admission transactions and **135 traced
`BEGIN` / 135 `COMMIT` SQL statements** across the wider call; 1,698
object-insert, 1,137 pack-insert, 406 pack-append and 1,139 pack-select
statement executions, **6,960 traced statements** total. Old
`sql_begin_ns` is 0.969412 ms and `sql_commit_ns` is 425.675751 ms;
the latter includes a 413.391125-ms, 130-commit pipeline subtotal
and later final/publication commits. The old statement trace counts
executions, not created pack rows.

The C3 diagnostic's three `SaveOutcome`s report **112,424 inserted
objects**, **2,295 pack creations**, **1,985 in-place pack appends**,
**2,833 batched object-insert statements**, and **427 acknowledged
Save transactions**. The disjoint Save profile buckets total
**460.075864 ms SQL** and **679.365114 ms transaction cadence**
(BEGIN/COMMIT/ROLLBACK), nested in the file and namespace spans.
Core's counter does not count every SQL statement, and it excludes
History publication. Core total SQL statement count and separate
COMMIT-only wall are therefore **unmeasured**. The two SQL traces
cannot be divided to infer per-statement speed. C3's exact payload
pack closure and retained pooled open pack are visible in the closed
Store geometry below; the diagnostic's placement counters describe a
different, instrumented identity from the research Store. Its closed
Store has 2,295 packs and 519,884,800 B apparent length, versus the
research Store's 2,297 and 519,917,568 B; the diagnostic is not a
replacement space sample either.

## Closed Store comparison

Read-only SQLite URI `mode=ro&immutable=1`, `PRAGMA page_size`,
`page_count`, `freelist_count`, `dbstat`, pack BLOB lengths and filesystem
`st_size`/`st_blocks*512` reproduced the original
[old geometry](evidence/c3-v016-side-by-side-20260924/v016-geometry.json)
and [C3 research geometry](evidence/c3-v016-side-by-side-20260924/c3-research-geometry.json).
The Store files were closed and were not modified. Core totals include
[History](evidence/c3-v016-side-by-side-20260924/c3-research-history-geometry.json).

| Closed-file metric | v0.1.6 single Store | C3 content Store + History | Difference |
| --- | ---: | ---: | ---: |
| 4-KiB pages | 125,703 | 126,933 + 21 = 126,954 | +1,251 |
| Freelist pages | 20 | 0 + 0 | −20 |
| `object_packs` B-tree bytes (`dbstat`) | 508,952,576 | 512,823,296 | +3,870,720 |
| `objects` B-tree bytes (`dbstat`) | 5,726,208 | 6,082,560 | +356,352 |
| Object rows | 112,424 | 112,424 | 0 |
| Pack rows | 2,690 | 2,297 | −393 |
| Pack BLOB capacity | 507,025,429 B | 511,331,101 B | +4,305,672 B |
| Declared post-used pack tail | No separate reservation in old framing | 142,988 B, all 16 pooled v12 packs | Different grammars |
| Apparent `st_size` | 514,879,488 B | 519,917,568 + 86,016 = **520,003,584 B** | **+5,124,096 B** |
| Allocated `st_blocks*512` | 520,110,080 B | 523,313,152 + 86,016 = **523,399,168 B** | **+3,289,088 B** |
| Allocated minus apparent | 5,230,592 B | 3,395,584 B | −1,835,008 B |

The old pack versions are v1/v2/v4/v6 with 9/1,116/1,501/64
rows and 103,924 groups total. The old variable-length BLOBs have
no separate post-used tail in their framing. C3 versions v9/v12/v15/v17
have 18/16/944/1,319 rows and 76/2,001/5,180/6,103 groups.
Its 2,281 payload packs are exact length; 16 pooled packs hold
4,194,304 B capacity, 4,051,316 B declared used and the 142,988-B
tail. Parsing the fixed directories read-only gives **5,182,500 B of
vacant payload directory slots** plus 33,520 B pooled vacancy.
These slots sit **inside** declared used length, so adding them to
post-used tail would double-count. v9/v15/v17 readers derive fixed
body offsets from that reservation; shrinking it requires a format
change. The Core Store also has content-signature and Save indexes,
while the old single Store holds its History tables; named table pages
are not all like-for-like. The +5.124-MB file-length difference and
−1.835-MB filesystem-allocation slack difference explain the
+3.289-MB allocated difference exactly. Neither row proves the
remaining gap is all pack-directory overhead.

## CPU, memory, and gates

The old public-call window consumed **3.125596042 s user +
4.220182875 s system CPU** and ended at a new lifetime high-water
RSS of **92,405,760 B** (87,932,928 B above t0). Its connection
cache used **34,604,032 B** at t1, with 555,584 B overflow; the
explicit importer buffer peak was 5,216,394 B. Research C3 recorded
**3.857321 s user + 5.560948 s system CPU** and **154,189,824 B**
peak RSS by `wait4` over the entire driver lifecycle, including Store
setup/teardown. The instrumented C3 diagnostic's lifecycle was
3.936676 s user + 5.731301 s system, 156,925,952 B peak RSS.
The raw research RSS difference from the old t1 high-water is
61,784,064 B, but the observation scopes differ. Core live SQLite
page-cache usage, construction-buffer peak, and host page-cache growth
were **not** separately measured. CPU/RSS attribution to a particular
stage is **unresolved**; no like-for-like resource delta is claimed.
The 0-resident-payload checks describe the start of each call, not
host cache residency during it.

**Later memory follow-up, 2026-09-24:** the separately frozen
[matched t1 resource diagnostic](c3-v016-exact-source-memory-result-20260924.md)
uses the old driver's own `getrusage` and `proc_pid_rusage` probe in a
new merged-C3 release driver. At public-call return, C3 reached
154,763,264 B peak RSS, **62,357,504 B above** the retained old t1
peak. It also records Core's live Save-cache point readings and source
inventory reservations. This later diagnostic does not relabel either
one-shot performance row.

Performance admission remains **INELIGIBLE** for both release routes
because source metadata cache state is unqualified, and these APIs
are different. The registered #236 **debug** `namespace-100000` row
remains **NOT_RUN**; none of these release rows substitutes for it.
The separate [#229 sparse-history gate](https://github.com/Ephemeral-AI-Lab/layerfs/issues/229)
is **open / NOT_RUN for C3 promotion**. A dense 100k Init does not prove
sparse history compactness. Retained `history-stride10` complete
commands took 38–40 s against 15/25-s limits, prior matched stride1
runs stopped at `Integrity("dependency encoded work")`, and its old
verifier sampled paths. Later C5/C6 17-Save sparse probes showed a
mechanism benefit, but both dense candidates missed their frozen
allocated-space rule; C6's dense total was 525,819,904 B, 1,420,736 B
above its limit. Those research changes are outside this C3 promotion.
