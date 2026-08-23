# H11 retained-control terminal result

Disposition: **`H11_REVISE_EXACT_BLOCKER`**.

## Final audit override: hard Q blocker

The sealed v2 files say analyzer-level `PASS`, but that is not the G5-C disposition. Final source/evidence audit found that the harness's reported `q_high_water=73,033` and literal `q_current=0` exclude benchmark-owned allocations that remain live:

- the 459,443-byte expected-manifest `String` and uncharged 1,001-record `Vec<H11Expected>` (232,232 bytes of record payload before capacity effects);
- current/retained `BTreeSet<ObjectId>` reachability state, including 6,057 retained IDs at N=1,000;
- up to 999 uncharged `u128` history timings and transient formatting vectors/strings;
- the final JSON report `String` itself; and
- reachability's internal `Metrics`, which never contributes to reported `q_max`.

This contradicts the frozen G4 rule that prepared expectations, traversal structures, and report output use checked RAII charges and that no already-allocated vector is adopted into Q. RSS remains below its independent hard ceiling, but RSS cannot waive exact logical Q. The hash-bound [final Q audit](../../../../implementation-detail/phase-4/experiments/g5-foundation-h11/v2/audits/FINAL-Q-AUDIT-v1.json) therefore supersedes v2's H11 PASS label.

## Chronology and protocol repair

H11 used one deterministic 1-MiB retained G4 fixture, 1,001 hash-bound expected revisions, and the balanced schedule `(1,1) (10,1) (100,1) (1000,1) (1000,2) (100,2) (10,2) (1,2)`.

The sealed v1 result at `target/phase4-g5-foundation-h11-20260822-v1/` is preserved as `REVISE`. All eight samples completed, but its analyzer incorrectly compared the genesis-transition first edit at N=1 with non-genesis edits, and demanded exact filesystem allocated-block equality. The raw data showed the true mechanism classes: N=10/100/1,000 first edits all had exactly 150 queries/rows, 168 row-BLOB reads, and 2,177,007 canonical bytes authenticated; only N=1 used the smaller genesis path. V2 prospectively froze N=10 versus N=1,000 for that operation and bounded raw allocated spread plus worst-case slope. It did not change the executable, fixture, schedule, semantic gates, resource ceilings, or 20-second wall.

Sealed v2 complete wall from fail-fast lock acquisition through terminal-verification fsync was **8,551,146,875 ns**, below 20,000,000,000 ns. The structurally similar primary and independent recomputations returned exact normalized agreement and no failures in their selected fields. Those facts remain diagnostic; they do not cure the omitted-Q hard failure.

## Two-sample latency results

| Operation | Control raw ns | N=1,000 raw ns | Control/candidate means ns | Ratio | Delta mean ns | Relative fail | Absolute fail | Material |
|---|---|---|---:|---:|---:|---|---|---|
| Reopen + head, N=1 control | 625,750 / 508,083 | 486,750 / 460,167 | 566,916.5 / 473,458.5 | 0.83515 | -93,458 | no | no | no |
| Head lookup, N=1 control | 11,500 / 10,500 | 10,708 / 10,291 | 11,000 / 10,499.5 | 0.95450 | -500.5 | no | no | no |
| 64-KiB range, N=1 control | 166,125 / 172,791 | 175,208 / 169,833 | 169,458 / 172,520.5 | 1.01807 | 3,062.5 | no | no | no |
| Reconstruction, N=1 control | 2,354,417 / 2,464,750 | 2,409,167 / 2,367,958 | 2,409,583.5 / 2,388,562.5 | 0.99128 | -21,021 | no | no | no |
| Materialization, N=1 control | 3,074,000 / 3,082,042 | 3,061,250 / 2,983,250 | 3,078,021 / 3,022,250 | 0.98188 | -55,771 | no | no | no |
| First edit, N=10 control | 3,370,250 / 3,193,291 | 3,682,834 / 3,432,875 | 3,281,770.5 / 3,557,854.5 | 1.08413 | 276,084 | yes | no | no |

The first-edit relative branch failed, but its two-sample sum delta was only 552,168 ns, below the hard 2,000,000-ns absolute branch. Under the prospectively frozen dual rule it is not product-material. These latency rows are diagnostic because the independent Q gate failed.

## Identity, work, resource, and cleanup

| History N / final revision | Root | Transition | Output digest |
|---|---|---|---|
| 1 / 2 | `02027f397c2f49fb23336cf41ee7734c75abfe687ba9d94c748d2b7d357196f1` | `c6277df1c97589739942f545e9a655f6db95d7d3cc7a5f181d2c2a4fe2b9808e` | `e540f0994075c2dca92a4bd825755a162567ca174382f3981dba0c945a78c77a` |
| 10 / 11 | `0d8bda8478e2246e4532c10fe8e4c89afe43cbf29cb7b89ea896db11abbe0250` | `eb5c1ea0b43fe1a6db16a4427b6eafd49b7847ec4676d70ca84e7237e4112999` | `f57b2057d411ca3daef3266d25a383647e62bae07456b30895587cbc6e6da298` |
| 100 / 101 | `28c0e35c1fbe2d87a18508765d7b3debc822a10eec907347b47f56fbe0a44a8d` | `37c8870856af75d0ebff2327327253448c7cb2bd562fcd627c2a5c01a27161bb` | `45e97487cc22833b9716e6eebb07cd3ea6d5f5c899329b17f4fbfd992c012da2` |
| 1,000 / 1,001 | `d6d54aa587da15a3560b1db7601ba7e851e8924bd69ea5e7b8906bc7147680f7` | `79de8ba0f90a820b2a16be00b924a9bdb12f9775688172324520de54f27f7c22` | `3d48d3f841eac3077cd24907eef7f5a00e257925c26949dacc67dad035f3fc47` |

Every revision's root/transition/file identity is gated internally against the 1,001-row manifest, and both emitted final samples match it. Selected historical transitions and reconstructed output/occurrence bytes are verified inside the source-bound binary. The raw rows emit revision numbers rather than observed historical tuples, so the analyzers cannot independently recompute that historical verification; the hash-bound operation log is custody-only and is not consumed by execution.
- Current live graph was invariant: 58 objects, 1,051,574 canonical bytes, 2,255 mapping bytes.
- Reported Store-level Q peaked at 73,033 and emitted zero, but whole-harness Q is **invalid/unavailable** because of the blocker above.
- Peak whole-child RSS was 14,057,472 bytes, 6,914,048 bytes below the ceiling; maximum individual buffer was 1,048,576 bytes.
- Every child reported one exact post-reopen transaction/COMMIT, no descriptor/permit leak, no seed/temp residue, and removed its SQLite/native work root.
- Fresh final audit finds no benchmark lock and no work root. The v2 runner closed the lock descriptor and later unlinked without inode/token revalidation; its sealed cleanup claim predates release, so robust ownership/release is not proved.

The two terminal analyzers directly gated a defined subset of emitted work fields. The separately hash-bound [emitted-work audit](../../../../implementation-detail/phase-4/experiments/g5-foundation-h11/v2/audits/EMITTED-WORK-AUDIT-v1.json) recomputed every non-timing field inside the six nested operation objects and found exact parity in the proper mechanism classes: range Q 73,033; reconstruction/materialization Q 10,210; first-edit Q 62,825, canonical-new bytes 23,030, objects created/reused 6/0; and zero write/object/transaction counters for read operations. Top-level identity/history/storage/resource fields remain covered by the separate source/evidence audit and original analyzers as documented; this nested-operation audit does not validate complete-child Q.

## Whole-child CPU and RSS

| Scope | Real | User | System | Maximum RSS |
|---|---:|---:|---:|---:|
| All eight rows, summed | 8.35 s | 5.58 s | 2.30 s | 14,057,472-byte campaign maximum |
| N=1,000 sample 1 | 3.68 s | 2.46 s | 1.03 s | 14,057,472 bytes |
| N=1,000 sample 2 | 3.63 s | 2.45 s | 1.00 s | 13,991,936 bytes |

These are whole-child `/usr/bin/time -l` observations, not per-operation CPU attribution.

## Evidence hashes

- V2 terminal: `36a02f356b506cefc2568ef3ab0324ba24e7d503327086a6eb8d972b4c33f712`
- Terminal verification: `d1337e182c7d7ee72b9a9afe38ef080b2d0efd8dbe8a221239df86ccf7602198`
- Payload manifest: `f62d3ae939c3e39450efe265fc3ef960ee5d48d9992f191519e142061103b935`
- Final artifact hashes: `c2e8e857eb74ec5d072d5a6b41f63820acde503409573b0d77704c3483f27180`
- Primary / independent: `2005c48290d49b573226fa3f5e16ef45d3ec14ea4af7fd5207ecac6c852356f8` / `1f5b7f11abd1727b4cf372eecd4509339a774b9373e90a6591b975cf7f196b16`
- Post-terminal nested-operation work audit: `c425fef687e83727120549a9810f9a9b55e6c2f1bf532565e5a213698d939eea`
- Final Q audit: `06b1dfdd76b5fe678b72e2b2c9ab766580c1badd2680feb9e1a468bb7467f333`

H11 v2 is diagnostic, not a qualifying retained-control sentinel. Its expected manifest is prospectively hash-frozen but generated by the same H11/G4 implementation; existing G4 codec goldens remain the independent lower-level authority. The timed `reopen_head` interval contains preflight/open SQLite work outside `Metrics`, and `cache_size=1500` is applied only after `Store::open_measured` returns. Reopen comparisons are balanced history evidence, but their 3-query/3-row/8-BLOB fields are not complete timed SQLite work and the interval is not an exact 1,500-page cold/profile characterization. H11 makes no population distribution, concurrency, GC deletion, controlled-cold, or physical-I/O claim.

## G5-0 qualifying addendum — H11 v9

The original v2 disposition above remains historical. It was repaired through
versioned v3–v9 attempts without deleting or relabeling prior evidence. The
qualifying result is now **`H11_PASS_G5_C_GATE_READY`** at
`target/phase4-g5-foundation-h11-20260823-v9/`; see the complete
[G5-0 report](../../../../implementation-detail/phase-4/experiments/g5-foundation-h11/v9/G5-0-REPORT-v9.md).

V9 charges the manifest, 1,001 expectations, history timings, exact
reachability entries, traversal vectors, operations, tuple output, report
transients, and final output. It borrows process arguments and emits the
terminal Q marker only after every owned capacity has dropped. All eight rows
return whole-harness Q to zero; high-water is `691,675–705,901` bytes. Maximum
RSS is `14,090,240` bytes. The deterministic slopes remain exactly `6` objects,
`23,030` canonical bytes, `2,255` stored mapping bytes, and `24,858.9069`
logical/apparent SQLite bytes per unique revision.

Three fresh independent terminal lanes passed source/Q, performance arithmetic,
and custody. All 50 method rows and 38 final artifacts rehash exactly; lock
inode/token release and terminal cleanup are closed. G5-0 is PASS and G5-1 may
begin. This addendum does not authorize GC, G6, production promotion, or a
general-population history/storage claim.
