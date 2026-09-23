# #237: 10k threshold-probe reserve research

> Research only, 2026-09-23. Source-derived counts and a prospective diagnostic;
> no product edit, public Init sample, allocator measurement, or speed claim.
> Scope is the frozen `namespace-10000` native-directory route. The SQLite
> database page size stays 4 KiB; it has no relation to the proposed probe size.

## Exact work implied by the source

The [fixture plan](../../../benchmark/fs-bench-pro/families/init_namespace.py#L28-L59)
declares 10,000 files and determines their sizes. The [native import](../../../crates/layerfs-service/src/operation/import_native.rs#L143-L229)
calls `construct_stream` once for each successful file job. That function makes
one unconditional `Vec::try_reserve_exact(cutoff)` call **before reading the
file** ([constructor](../../../crates/layerfs-content/src/file/content.rs#L279-L315));
the default cutoff is 131,072 B ([policy](../../../crates/layerfs-content/src/policy.rs#L13-L18)).
Thus the successful frozen 10k route has exactly **10,000 explicit prefix-reserve
calls**, requesting **1,310,720,000 B cumulatively**. This is the sum of
requests, not live memory, committed pages, device I/O, or allocator time.
There are four file workers, so at most four of these probe allocations are live
at a time: **524,288 B of requested prefix capacity**, before allocator rounding
and other live buffers.

`plan(CASES["namespace-10000"])` produces:

| Class | Files | Exact length | Prefix bytes read per file |
| --- | ---: | ---: | ---: |
| Empty | 100 | 0 B | 0 B |
| Tiny A | 1,074 | 325 B | 325 B |
| Tiny B | 6,825 | 326 B | 326 B |
| Small | 1,500 | 20,782 B | 20,782 B |
| Medium | 500 | 332,506 B | 131,072 B |
| Anchor | 1 | 100,000,000 B | 131,072 B |

The probe therefore reads **99,414,072 B** over all files. The 100 empty and
7,899 tiny files account for **7,999** of the 10,000 full-cutoff reserve calls,
or **1,048,444,928 B** of nominal capacity requests, while filling only
**2,574,000 B**. A hypothetical 4 KiB initial reserve would request
**40,960,000 B** initially across the same 10,000 calls, but the 1,500 small
and 501 chunked files would then need growth. Those growth counts, capacities,
copy costs and elapsed time are **unmeasured**. `try_reserve_exact` is a request;
the allocator may round, and an untouched large allocation need not fault or
dirty all its pages. The 1.27 GB difference in initial requests is not a
projected time or RSS saving.

The existing `source.take(cutoff).read_to_end(&mut prefix)` bounds **length**
and source read requests. A smaller initial capacity does not itself prove the
same maximum **capacity**: `read_to_end` can grow a full `Vec` beyond the bytes
ultimately read. It can also shift allocation failure from the current explicit
pre-read `try_reserve_exact` to growth during the read. Both must be checked
before treating a one-line reserve change as product-safe. Buffer capacity does
not enter the canonical representation: below cutoff the same bytes go to
`construct_bytes_in`; at cutoff the same prefix is chained to the remaining
source ([dispatch](../../../crates/layerfs-content/src/file/content.rs#L297-L309)).
That supports a byte-identity hypothesis, which still needs a check.

## One diagnostic after the C1 worker's timed window

Run a standalone **allocation microdiagnostic**, outside the public benchmark
timer and after the concurrent C1 experiment finishes. Use the same locked Rust
toolchain and implement only the constructor's exact `Vec::new` → initial
`try_reserve_exact` → `source.take(cutoff).read_to_end` sequence. Feed a
nonallocating synthetic `Read` source the exact six size rows above, one file
at a time, with all accepted cutoffs (128, 256, 512 and 1,024 KiB). Compare
initial capacities 0, 512,
4,096 and the cutoff, each prospectively declared and executed once. Record by
class: explicit and implicit allocator calls/reallocations, total requested
allocation bytes, final and maximum `Vec::capacity`, source read calls/bytes,
and elapsed allocation/read time. Check that the bytes and class dispatch agree
and that the source never supplies more than `min(length, cutoff)`. An allocator
wrapper can count calls, but its results describe this synthetic reader and this
host allocator, not cold-file Init throughput. Retain every result, including a
slower or over-capacity arm.

If the full public D11 count-driven worker diagnostic shows prefix allocation
is material **and** the microdiagnostic shows a smaller start does not breach
the accepted-cutoff memory/error bounds, prototype the smallest shared
`construct_stream` change: replace the reserve argument with
`cutoff.min(4_096) as usize`, leaving the `source.take(cutoff)` limit untouched.
If growth exceeds the existing capacity ceiling or changes bounded allocation
failure behavior, discard that one-line treatment; a capped fallible read loop
would be a separate candidate justified only by measured allocation time.
The existing external streaming test already checks
known versus unknown routes for root and object order
([test](../../../crates/layerfs-content/tests/streaming.rs#L139-L159)); extend
that one test with the exact threshold boundaries at every accepted cutoff
and the 10k size classes, including error and peak-capacity checks in the
external diagnostic rather than a test-only production hook.
Then take one registered cold-source public 10k control/treatment pair, with
unchanged route, four workers, Store format and 4 KiB SQLite page size. Compare
public time, CPU/RSS, exact roots and object bytes, Store geometry, the #229
sparse-pack lane and full reopened readback. A microdiagnostic win alone is not
a product speedup; no warm source page may credit either public timed arm.

If D11 instead attributes the file span mainly to C2 accept/seal or source I/O,
stop here. Extra allocations and copies on 2,001 larger files could make the
small start slower, despite the large reduction in nominal initial requests.
