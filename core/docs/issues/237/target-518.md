# #237: what a cold-source 518.8 MB/s 10k Init would require

> **Status:** Research; informative and not a product contract. This is
> arithmetic over retained, single-sample diagnostics, not a new benchmark or
> an attainable-speed claim. SQLite database pages remain 4,096 bytes.

The 10k fixture contains 300,000,000 logical bytes, including its 100 MB
anchor. Matching the historical 578.245 ms caller time would yield 518.8
decimal MB/s. The fastest measured Core research prototype so far, the
[direct C1 build](c1-direct-prototype.md), returned in **1.317538583 s**
(227.697 MB/s). It needs **0.739293583 s less** caller time, a **56.1%**
reduction, to match 578.245 ms. Its measured `history.import_files` child was
**1.162958542 s**; every other caller cost was **0.154580041 s**. If that
other work stayed fixed, file import would have at most **0.423664959 s**,
requiring a **63.6%** reduction in its measured span. This is a conditional
budget, not a prediction that those spans are independent.

The [D12 count diagnostic](c2-detail-diagnostic.md) was made on another
instrumented identity and cannot be subtracted from the C1 candidate as a
matched phase. It does show where the current file path spends work: one
receiver made 24,562 `accept` calls totaling 771.409 ms; its whole Save
charged 227.521 ms to COMMIT, 188.689 ms to SQL, 69.841 ms to FULL encoding,
and 43.306 ms to collision queries. The four file workers and the owner
overlap, so adding or subtracting these figures from a public call would
double-count work. An isolated change to the already rejected 43 ms collision
lookup or the 7 ms prefix allocation cannot close a 739 ms gap.

The historical [v0.1.6 comparison](v016-comparison.md) does not supply a
cold-source 578 ms baseline. That row's source was dirty, its receipt reports
zero process disk reads and no cold cache contract, and its object decomposition
and public route differ from Core's. The two later clean-input reference rows
took **1.020–1.101 s** (272.6–294.0 MB/s) while recording 322–337 MB of
process disk reads. Reusing a prepared fixture is legitimate setup reuse;
letting its resident source pages serve the timed call is not. The #237
research sidecars verified zero resident **payload** pages immediately before
their calls, but did not independently qualify directory/inode metadata cache,
so none is a fully cold admission result.

Any 518.8 MB/s claim needs a newly registered public 10k route with the same
fixture and source cache contract in control and treatment, fresh Stores, a
4 KiB SQLite page, one sample per source identity, and no teardown shifted
outside the public timer. Keep performance-only exploration separate from the
independent readback and #229 sparse-pack proof. The next algorithmic candidate
must address most of the file-import critical path, not only one small C2
counter. The three read-only squad reports are the
[architecture comparison](architecture-v016-v017.md),
[complexity map](complexity-10k.md), and
[SQLite EXPLAIN audit](sqlite-explain.md). They found no missing hot index or
remaining 10k quadratic scan. The old C1 scan blowup is fixed; direct C1
construction is the only separately measured improvement, while roughly
7,700 Store group publications and 80 commits remain on the file path. The
SQLite audit did not measure the timed Save's pager spills, so the reference's
32 MiB cache profile is a diagnostic candidate rather than a claimed
solution. A bounded bulk-admission design may change these costs but must
flush on same-save dependencies and retain exact readback and space
properties. None has yet demonstrated 518.8 MB/s with cold source pages.
