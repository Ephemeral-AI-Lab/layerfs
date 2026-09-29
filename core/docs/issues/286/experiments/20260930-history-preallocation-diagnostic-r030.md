# #286 r030: allocation-order causal diagnostic

> **Status: complete count-driven diagnostic, never a gate sample.** The
> original level9 stride1 gate still **FAILS** by139,264 B at r021. The watcher
> in this round is outside the product and its physical preallocation is not a
> qualifying C2 write path, despite the smaller observed file allocation.

The [committed diagnostic script](20260930-history-preallocation-diagnostic-r030.py)
ran at clean source `62ed81588`, with the exact level9 release backend SHA-256
`8f52b440238a548d117716759adc28794b580404b7421172ba85ceaf1bf1b661`,
the frozen v3 stride1 corpus/root pins,157 states and one construction worker.
It polled the **same original Store** every25 ms and, when physical blocks
lagged `ceil((st_size+2MiB)/1MiB)`, asked Darwin `F_PREALLOCATE` from physical
EOF for at most4MiB. Each call and returned byte count is retained. The
complete child exited0 in **166.738 s** under the unchanged170 s diagnostic
bound; the separate independent verifier exited0 in **20.733 s** under30 s.
The cache state is uncontrolled and this time is not a speed result.

| Observation | r030 diagnostic | Original official r021 |
| --- | ---: | ---: |
| C2 apparent bytes | 75,712,512 | 75,712,512 |
| C2 allocated bytes (`st_blocks*512`) | **78,643,200** | **83,890,176** |
| C5 allocated bytes | 196,608 | 196,608 |
| Compound allocated bytes | **78,839,808**, diagnostic only | **84,086,784**, FAIL vs strict `<83,947,520` |
| C2/C5 content SHA-256 | both **identical** to r021 | original pins |

There were **73** successful `F_PREALLOCATE` calls, requesting and reporting
78,610,432 B in total, with no probe error or timeout. The final C2 allocation
is exactly75 MiB and is **5,107,712 B below** the compound limit after adding
C5, but that arithmetic is **not** a gate PASS: it came from an external
watcher whose cost is outside the product's named save children. Both final
database files are byte-identical to the original r021 C2/C5 (SHA-256
`ef1fba79a76add4c781c9bbcf1c5219a9548d3cb03b4483a8d84ded9059ca781`
and `8dcf12b01322afcaf9e1a758b05abbe68e1d3216c0a37eda9033090f9af93833`).
The independent verifier checked the frozen roots, whole state trees and
selected content without failure. This establishes that **allocation order**,
not a byte change or copied Store, explains the16-MiB physical step on this
host. It does not prove that an integrated producer will preserve the time,
memory, concurrency or failure contracts.

[SHA-indexed original files, all73 call records, exact command, timing,
trace and verifier outputs](20260930-history-preallocation-diagnostic-r030/evidence-index.json)
are append-only. The product-source next step is to identify a safe
platform-gated preallocation boundary inside C2 save, paid within its measured
phase, with no logical-size change, extra worker, file-sized spool or unreported
fallback. The current audited-unsafe rule permits only `encoding/codec.rs`; any
new filesystem FFI boundary needs an explicit contract/guard amendment and
must fail clearly on unsupported platforms. A committed product treatment and
fresh original-owner official stride10/3/1 receipts would be required before
family 2 could pass. Families 3–7 remain NOT_RUN; #285 stays draft.
