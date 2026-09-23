# #237: v0.1.6 versus Core on the same 10k source

> **Status:** One-shot research comparison, not a release-admitted speed pair.
> Both products imported independent byte copies of the **same** 10,000-file,
> 300,000,000-B source. Both used SQLite BLOB packs and 4,096-B database pages.
> Source payload residency was zero at both public calls; directory/inode
> metadata residency was unqualified. The Core performance receipt is
> `INCOMPLETE` because a daemon telemetry event was lost. Neither arm was
> rerun. See the [preregistered protocol](v016-v017-common-source-prereg.md),
> [v0.1.6 arm](v016-head2head-arm.md), [Core arm](v017-head2head-core-arm.md),
> and [SQLite EXPLAIN companion](v016-core-sqlite-head2head.md).

## Public and pipeline comparison

The exact v0.1.6 release product is peeled tag commit `44cf74848`; the
current root `crates/*` production tree is byte-identical. Core is the
integrated C1 direct-builder/C2 bounded-group source at `7f2124ba0`, with
temporary reporting-only Service diagnostics. Both native small-content
cutoffs are 128 KiB. The same fixture manifest SHA-256 is
`c1d7937c9f90d3585558e6d20b35183a737530d386667d08badc3121a6b3d55e`.
Each arm had a fresh Store and a distinct writable source copy; the reference
used a benchmark-only READY/GO cold barrier after Store/client setup, and
Core used the H3 driver immediately before its daemon request.

| Measurement | v0.1.6 release | Current Core | How to read it |
| --- | ---: | ---: | --- |
| Public operation | **750.625833 ms** | **1,380.218125 ms** | Raw Core caller is **629.592292 ms / 1.839×** longer. SDK Init and daemon Import are distinct public surfaces. |
| Throughput, 300,000,000 B / public wall | **399.667 MB/s** | **217.357 MB/s** | Decimal bytes; 100-MB anchor is inside the 300 MB. Neither is a fully cold metadata result. |
| Performance status | `DIAGNOSTIC` | `INCOMPLETE` | Core lost one daemon telemetry event; both public calls returned a root and cleanup completed. |
| Source payload at final pre-call check | **0/27,503 resident pages** | **0/27,503 resident pages** | Recheck-to-timer gap **0.339 / 6.820 ms**; metadata cache unqualified. |
| Source bytes and read calls | 300 MB / **27,951** | 300 MB / **29,952** | Core counted 1,129.335 ms of read syscall wall **summed across four workers**; old read-call wall `NOT_MEASURED`. |
| Device-read evidence | **337,158,144 B** process disk reads | `NOT_MEASURED` | Core's 300-MB logical read count is not a device-I/O counter. |
| Import pipeline / file loop | **709.704 ms** | **1,169.934 ms** | Closest broad child spans, **460.230 ms** apart, but old includes join/resolve and Core's scope ends after all per-file Done messages. Not a disjoint attribution. |
| Producer wall sum / blocked-send sum | **2,577.802 / 806.749 ms** | **4,420.195 / 2,593.985 ms** | Four overlapping workers. Subtracting nested send time gives **1,771.053 / 1,826.210 ms** nonblocked remainder, not pure construction. |
| Handoff / receive calls | **1,203 bounded slabs** | **34,562** Object/Done messages | Core sends 24,562 objects and 10,000 completions separately: about **28.7×** as many receive events. |
| Receiver `recv`/`recv_timeout` time | **47.914 ms** | **445.971 ms** | Different queue-event and terminal boundaries. Their 398-ms difference is **not** a measured saving from batching. |
| Final namespace work | **18.961 ms** final root/inode table | **45.340 ms** whole Core namespace stage | Core includes prerequisite/tree Saves; not isomorphic. |
| In-public file Save connection release | `NOT_MEASURED` | **72.854 ms** | Old external Store/client teardown is after its public timer; old internal guard release was not isolated. |
| Process CPU | **1,626.476 ms** user+system inside old call | **1,797.839 ms** sampled Service+daemon windows | Core window coverage differs and telemetry is incomplete; no CPU speed ratio is claimed. |
| Separate full reopened readback | **PASS**, 10,000 files / 300 MB | **PASS**, 10,000 files / 300 MB | Both outside performance. The first old verifier attempt failed on external ID-text newline before content access and is retained. |

The old `prepare_import` child was 734.019 ms and contains its 709.704-ms
pipeline and 18.961-ms final table. Core's Service stage ledger places
45.427 ms in scan, 1,169.934 ms in the file loop, 72.854 ms in file-Save
connection drop, 45.340 ms in namespace work and sub-millisecond catalog
reserve/publication calls. Those Core top-level children explain 1,340.547
ms of its sampled 1,340.913-ms Service command. The daemon public caller is
39.306 ms longer than that Service span, but different clocks/boundaries and
telemetry loss prevent assigning the remainder to transport. Cross-version
children cannot be summed into the 629.592-ms public gap as if they had the
same start/end events.

## SQLite work and physical output

All three closed databases (old single Store, Core content Store and separate
Core History catalog) reported **4,096-B pages**. The [read-only EXPLAIN
probe](v016-core-sqlite-head2head.md) used SQLite 3.51.0, the same host
library linked by both release binaries. Its plans compile hot SQL without
executing writes. The following statement counts come from the timed
diagnostics, **not** from EXPLAIN.

| SQLite or output measure | v0.1.6 release | Current Core | Interpretation |
| --- | ---: | ---: | --- |
| Successful write COMMITs | **75** exact traced BEGIN/COMMIT pairs | **105** exact C2 Save commits + **2** source-derived History writes = **107** | Old count covers its combined Store; Core spans file/prerequisite/tree Saves and separate C5 DB. |
| Measured COMMIT wall | **223.015 ms** for 74 diagnostic admission/publication commits; reserve unpriced | **226.570 ms** across 105 C2 Save commits; C5 COMMIT-only wall unmeasured | The measured buckets differ by about **3.6 ms**, far below the public gap. Count reduction alone does not explain it. |
| Object rows stored | **24,683** | **24,683** | Same count on the same source, though canonical identities/formats may differ. |
| Object-row INSERT calls | **639** exact trace | **1,414** exact SaveOutcome calls | Core makes **2.21×** as many locator INSERT calls. Old sorts and pages locators; Core inserts per bounded placement. |
| Pack row creation | **590** multi-pack INSERT statements for **1,693** rows | **1,262** one-row pack creates | Both packs are SQLite BLOBs. |
| Pack append/update calls | **408** whole-BLOB `UPDATE`s | **1,237** incremental BLOB appends | Core writes changed spans, which is better per append; old has fewer calls but rewrites its bounded open BLOB. |
| Object lookup calls | **438** SELECT statements; fresh-ID bitmap skipped 24,683 absent IDs | At least **24,683** per-new-row collision lookups plus wave presence queries | Core file Save collision queries cost **43.921 ms**. This is real extra work but not the whole gap. |
| Pack BLOB bytes | **301,646,854 B** variable-length BLOBs | **330,825,728 B** reserved capacity; **305,969,540 B** declared used | Core reserves **24,856,188 B** spare in its packs. Encodings differ; old BLOB length is not directly Core's used-byte metric. |
| Total apparent DB footprint | **304,553,984 B** single DB | **333,737,984 B** content Store + History | Core is **29,184,000 B / 9.58%** larger. |
| Total allocated DB footprint | **318,783,488 B** | **335,695,872 B** | Host `st_blocks` accounting; not measured device writes. |

`EXPLAIN QUERY PLAN` shows primary-key seeks for object and pack reads in
both versions and indexed C5 stack/name queries. There is no missing hot
content index or large hot-table scan. A representative 128-row locator
INSERT compiles to **829** VDBE opcodes in the old schema and **2,617** in
Core, including 128 scalar TEMP-scope subqueries; these are compiled shapes,
**not** runtime cost ratios. The Core lookup also checks Save visibility.
Core's actual timed-owner pager spill count was `NOT_MEASURED` in this arm;
the zero-spill D13 observation belongs to a different Core identity.

## Complexity and next test

The [complexity map](complexity-10k.md) found a historical C1 repeated-prefix
scan; that mechanism was removed before this pair. Both current imports still
must do at least `Ω(source bytes + files + produced objects)` work. Indexed
content lookups are logarithmic in Store rows, while producer construction,
packing and persisted bytes are linear in input. The old and Core code both
use four Init constructors and the same 128-KiB small-content cutoff. The
most concrete new multiplicative difference is **handoff granularity**:
roughly 1,203 bounded slabs versus 34,562 individual object/completion
messages for the same source, with similar aggregate nonblocked producer wall
but substantially different blocked-send/receiver timing scopes. That is a
call/synchronization-count hypothesis, not a proven 398-ms time saving.

The next **Core-only** experiment should prospectively replace the
per-object/per-file handoff with a bounded producer-slab channel while keeping
per-object `SaveHandoff::accept`, SQLite BLOB packs, 4-KiB pages, 128-KiB
cutoff, four Init workers and the same public timer. Use a byte/object/
completion bound, four queue slots, and an explicit singleton path for an
object above the ordinary slab byte cap; account for worst-case queued and
producer-held memory. Count actual slab sends, blocked send and receiver
wait separately, C2 admission/SQL/COMMIT work, Store bytes and full readback
on one matched cold-payload pair. The detailed source-backed candidate shape
is in the [Core arm report](v017-head2head-core-arm.md#prospective-next-10k-experiment-bounded-producer-slabs).

Bulk locator/pack publication is the second candidate: Core made more object
INSERT and pack-create calls and used 29 MB more DB space, while old used
multi-row pages and variable-length pack BLOBs. Keep pack payload in SQLite
and Core's incremental changed-span append; a direct copy of the old full-BLOB
rewrite would be a worse byte algorithm. The isolated 80→7-COMMIT treatment
already slowed Core, and the common-source measured COMMIT buckets are nearly
equal, so reducing transaction count alone is not the next speed experiment.

The historical **578.245-ms / 518.8-MB/s** v0.1.6 observation read zero
device bytes and had no cold-source contract. This new reference run read
337,158,144 process-reported disk bytes and took **750.626 ms**. Recovering
the current Core-to-reference **629.592-ms** raw gap would reach about
**399.7 MB/s**, not establish 518.8 MB/s under a cold-source contract.
