# Full-corpus construction attribution diagnostic

Status: Research; informative and not a product contract.

Owner2026-10-01 requests attribution of the34.5s full-repository construction
stage. Issue#291; parent dab431abef4c235b992a6225d8a16674ea867911. Diagnostic ID
D-deepseek-construction-attribution-v1: one count-driven instrumented invocation,
not another throughput arm, speed comparison or admission sample. Existing
original receipts remain unchanged. No MinIO upload, source acquisition,
Workspace/daemon work, optimization, dependency or product-source changes.

Reuse the exact closed master deepseek-full-master-v2:103,108 files,
3,475,776,149 file bytes; manifest SHA256
541f55db9814837fd60386e42e56781e80fbdbd6d555d4bb0e3a099e215139af. Original verified
prepared catalog/packs in deepseek-full-import-v1 are the independently proven
reference. Freeze this specification before instrumentation, then commit clean
instrumentation/build identities before collection. Fresh private output and
per-worktree lock; one attempt,180s whole construction diagnostic bound. Failed
or timed-out attempts remain; no deadline increase or nicer-number resampling.

## Instrumentation and accounting

Keep public C1 construct_stream/default128KiB cutoff/CDC8–32KiB and C2 encode_full,
group/pack formats, dedup membership, one producer,256-object/file transaction
cadence and private MEMORY/OFF/cache512KiB/mmap0 unchanged. No new batching,
prepared-statement changes, parallel workers or timing-dependent product choices.
Existing pack-hash expression may be evaluated immediately before its existing
SQL insertion solely to isolate its clock charge. Instrument only the external
example; no product hooks or third-party edits.

Bounded scalar counters collect calls, elapsed nanoseconds and meaningful byte
counts for these existing regions:

- Master catalog copy/setup, source SQL enumeration, file open/read/close.
- C1 construct inclusive and consumer callback inclusive. Derive C1 residual as
  construct minus nested source reads and callbacks. Label it chunking/canonical
  hashing/tree construction/control/clock residue, not pure chunk CPU time.
- C2 FULL encoding by role: elapsed, canonical/raw/stored bytes and verbatim vs
  compressed record counts. Existing public SaveProfile probe_ns is a nested
  diagnostic; subtract it only from FULL total, never add twice. Group build
  by lane includes framing and optional ordinary-lane group compression; public
  API does not expose these separately. Report compressed/raw group counts.
- Pack assembly, per-pack BLAKE3 hashing, filesystem write, placement/control
  residual. Writes include open/write/close, no fsync. OS elapsed is not physical
  disk latency or device bandwidth.
- SQL membership (hit/miss), object insertion, locator statement preparation and
  executions, pack insertion, file-root update, transaction commit/begin and
  final validation; timers include unchanged prepare/bind/step/copy work.

All nested spans are reported inclusive with explicit subtraction. Sum disjoint
measured regions plus nonnegative residual equals their parent; subtraction
underflow is an accounting error, never silently clamped. Final construction
wall includes setup, all source bytes, encoding, SQL, pack files, finalization and
resource close; write the diagnostic receipt afterward. Python command wall and
child user/system CPU times reported separately. Clock/counter overhead remains
in observed regions; no calibrated overhead or baseline speed claim.

Constant three file cohorts:empty,1..131071 bytes,>=131072 bytes; record files,
logical bytes and source-read/C1-inclusive/consumer elapsed per cohort. Consumer
cost belongs to the triggering file; cross-file pack tails do not provide exact
cohort encoded-byte attribution. Optional fixed4096-file interval CSV gives
counts and elapsed deltas (26 intervals including final708 files). It is a
mixed-input progression diagnostic, not a controlled scaling curve. No per-file
or population-sized timing registry. Bounded counters/three cohorts/lane/role
arrays are the only new live instrumentation; CSV streams to disk.

## Output invariance, limits and receipts

Independent expected counts unchanged:210,332 unique objects,45,212 duplicate
emissions,5,836 packs and1,389,188,326 encoded bytes. Exact reference file roots,
object descriptors/locators, pack rows and entries/xattrs must match the already
independently reconstructed original. The only allowed catalog difference is
publication.ready=0 (private diagnostic) versus1 (original retained import).
Separate10s invariant proof: bounded SQL EXCEPT comparisons and exact streaming
pack-byte equality against the verified reference; record any missed bound and
coverage, never claim PASS from counts alone. Original input/output/prefix proofs
reused with their identities. No need to repeat unchanged full reconstruction.

Before effects require12GiB free additional disk, stage packs<=4GiB and catalog
reported separately. Keep existing original import/master untouched; retain the
new diagnostic stage for inspection. No server/process left running. Numerical
cache INELIGIBLE/performance_claim=false; source/OS/cache/physical memory/I/O
unobserved. No cold preparation, pre-touch or speed target. Output:source/build/
reference seals, diagnostic command/log, attribution.json, intervals.csv,
verification command/log/result and original failures if any. Publish safe
counts/hashes only; captured paths/catalog/content stay private.

Analysis may identify the dominant measured region and operation cost/counts.
It must distinguish measured regions from residual inference and explicitly
note limitations of CPU clocks/cache/timer overhead. Recommend the next concrete
change or narrower diagnostic from evidence; do not implement an optimization or
start a new performance arm under this diagnostic's source/contract.
