# Issue 261: independent scaling audit of three mounted 100-write cases

> Read-only source audits by the primary agent and two independent subagents,
> reconciled against the append-only source-`7361312e6` receipts in
> [`THREE-PATTERN-DIAGNOSIS.md`](THREE-PATTERN-DIAGNOSIS.md). No extra sample was
> taken for this audit. Every raw latency remains cache-`INELIGIBLE`.

## Observed work

| Case | Exec / Commit ms (diagnostic only) | Ledger 4 KiB reads / writes | Final extents / changed runs | Live metadata pages at 25/50/75/100 |
| --- | ---: | ---: | ---: | --- |
| True EOF append | 433.382 / 34.674 | 2,941 / 1,470 | 101 / 1 | 4 / 4 / 4 / 4 |
| Dispersed 10 MiB edit | 518.925 / 75.974 | 3,696 / 1,929 | 201 / 100 | 4 / 4 / 18 / 37 |
| Repeated one-place edit | 467.413 / 23.324 | 3,451 / 1,489 | 3 / 1 | 4 / 4 / 4 / 4 |

These are 18,067,456 / 23,040,000 / 20,234,240 bytes through the 4 KiB
ledger I/O API for 100 supplied bytes, **not measured device traffic**. Every
case created 100 private one-byte segments: 819,200 cumulative payload write
bytes, two direct 4 KiB writes and 8,192 allocated bytes per segment. The
repeated case reclaimed old versions and retained two segments at its final
snapshot; it still paid for all 100 acquisitions. Its final Commit used one
changed run and one replacement byte, so the remaining repeated-edit cost is
in Exec/publication and cleanup, not historical-write replay by Commit.

## Complexity and round trips

Let `N` be successful mounted WRITE callbacks, `E` final file extents, `H`
extent-index height, `F` page fanout and `R` final changed runs. The public
sequential writer necessarily incurs `N` kernel WRITE request/reply pairs,
`N` immutable acquisitions and `N` atomic Workspace root publications.
Acquisition also does one file create, allocation, and close per byte. The
current source performs one preflight `symlink_metadata` before its atomic
`O_EXCL|O_NOFOLLOW` create; its latency is not isolated. Normal routine
payload reclaim drains named released candidates, `O(released candidates)`;
the observed record visits were 99 / 99 / 197, with zero lookup scans. The
old full-registry-per-write `O(N²)` cause from #252 is absent on this route.
The deliberate scoped close still walks retained records `O(N)`.

The extent leaf holds 124 records and a branch 248 child references
([format](../../../crates/layerfs-workspace/src/backing/binary_plus_tree/extent/splice.rs)).
Append and repeated fit one leaf (`H=0`); dispersed needs a branch (`H>=1`,
strongly source-derived as one level for this case, though no root-level
counter is exported). The splice copies touched pages and ancestors, sharing
unmodified children. New-page publication and old-root cleanup enumerate
every Local/child edge on each copied page; contiguous owner refs are batched
into 62-record ledger pages. The formal bounded page-local work is
`O(N*H*F)` plus touched leaves, but `F` matters: as a branch's child list
grows, every copied branch revisits more child edges. The dispersed late
increase in ledger I/O and 37 live metadata pages is measured; leaf
fragmentation or growing branch edges are source-supported explanations, not
identified page-kind/occupancy counts. The current status lacks exact extent
page visits, writes, occupancy and root height.

The SDK driver makes one Mount, one Exec and one Commit call, then Status and
Unmount for cleanup ([driver](../../../crates/layerfs-api/sdk/examples/benchmark_shell.rs)).
One daemon Exec starts one shell and one writer with one fd. Each receipt has
100 actual FUSE WRITE callbacks, one LOOKUP, three GETATTR, one OPEN and zero
READ; writable OPEN requests direct I/O ([adapter](../../../crates/layerfs-fuse/src/adapter.rs)).
Each successful write invokes inode invalidation before reply. FLUSH/RELEASE,
notification acknowledgements and exact control-frame counts are not exported.
Four upstream Workspace Service calls fit one Exec Inspect and Commit's
SaveFile, portable metadata and History operations; there is no per-write
SDK/control or host Service RPC. The daemon's one-second revision poll cannot
explain these subsecond Execs.

Commit uses a frozen final-extent cursor, `O(E)` descriptor/replacement
traversal, then applies `R` final edits to the C1 chunked file. The 100
dispersed runs made 2,028 logical C1 `load_node` calls and 49.176 ms host
SaveFile; append made 4 and 19.131 ms, repeated 17 and 5.002 ms. `load_node`
counts draft and stored nodes, so these are **not** Store reads/round trips.
Exact Store reads/writes remain unexported. `FileInput::record` uses seek+read
per edit-record lookup; it is a later Commit candidate. The #248 8,194-byte
file takes the C1 whole-file path and cannot inherit the 10 MiB C1 timing.

## Fix order and limits

1. **Already fixed:** publication reuses its authenticated ledger page. The
   original separated 100-write v2→v3 counts fell by exactly 239 ledger reads,
   matching the source-derived new-page count; timing was cache-ineligible.
2. **Selected here:** old-root cleanup reads an authenticated owner then
   `load_raw` immediately reads the same record again. Reuse the owner within
   that cleanup step, while revalidating ledger path identity, page identity
   and checksum. The eligible page count is not exported, so no exact savings
   are predicted ([prospective treatment](../../../../docs/roadmap/0.1/0.1.7/issue261-ownership-io-treatment.md)).
3. **Private payload:** one page per tiny payload could reduce one-byte
   allocation to 4,096 bytes and payload writes to one per callback, but is a
   new physical format. #261 bars speculative packed-page formats; keep it a
   separate proposal requiring an explicit scope/custody ruling. It would not
   remove per-write file creates, FUSE callbacks or root publication.
4. **B+ and C1:** export extent page-kind/occupancy/visit/write and C1
   stored-versus-draft load counts first. Only then consider sibling rebalance
   for dispersed edits or one sorted bulk C1 edit walk for many final runs.
   Both must preserve old-root references and exact Commit roots.
5. **Smaller conditional fixes:** removing the preflight path lookup needs a
   disappearing-collider race ruling and failure oracle. The per-write resident
   node scan is `O(live nodes)` and open-handle search `O(open handles)`, but
   both are constant-size for this one-file/one-fd workload. Optional diagnostic
   `backing_status()` scans retained records; it is not enabled in the 4,097
   gate image. The earlier 512-write `EBUSY` return site is unproven, so do not
   change FUSE permit semantics from that receipt alone.

The separate 4,097 gate still needs one frozen-source public attempt. Ideal
packing of its 8,194 separated extents requires at least 67 leaves and one
branch, but the dispersed 100-write metadata growth prevents a credible speed
extrapolation. The 30-second product Exec timer belongs to #249; benchmark
budgets stay separate. Current source has no historical 4,096-edit cap on the
file stream. A gate must report Exec, Commit, complete wall, actual callbacks,
resource/cleanup outcome and the independent old/new head oracle, including
every FAIL/INELIGIBLE row.
