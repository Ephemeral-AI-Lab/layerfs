# Accepted checkpoint: 100 / 1,000 / 10,000 native Init

One performance-only sample per registered case on 2026-09-23. The source
commit was `a5b0a7326a10775cf763e62466d55e0a19980e8a`; the Core product tree
remained the accepted `970854f2c` tree, with no later experimental product
change. The harness reused exact sealed release binaries. Each run had its own
SQLite Store and History catalog, source byte copy, output path, and public
daemon request. The 100 and 1,000 fixture masters were prepared once outside
the timer; the 10,000 master was reused. The source copies were independent
byte copies, not APFS clones. Full verification was `SKIPPED` in all rows.

| Files / input | Public Init | Full command | Input rate | Store + History, apparent | Store + History, allocated | CPU user + system | Service RSS sample | Daemon RSS sample | Row |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 100 / 5 MB | **37.720 ms** | 102.863 ms | 132.557 MB/s | 7,008,256 B | 7,008,256 B | 25.811 + 29.315 ms | 3.883 MB | 1.933 MB | `INCOMPLETE`: Service telemetry loss |
| 1,000 / 20 MB | **114.494 ms** | 205.979 ms | 174.681 MB/s | 23,724,032 B | 23,724,032 B | 83.655 + 100.423 ms | 50.299 MB | 2.212 MB | `DIAGNOSTIC` |
| 10,000 / 300 MB | **1,419.181 ms** | 1,862.189 ms | 211.389 MB/s | 334,000,128 B | 342,360,064 B | 981.526 + 907.667 ms | 64.750 MB | 2.212 MB | `DIAGNOSTIC` |

**Scopes.** Public Init is the caller timer around one request and confirmed
root. The full command includes Service/daemon startup, public Init, cleanup,
and the final nonfaulting source-residency recheck. CPU is child process user
and system time for the **Service and daemon lifecycle**, not CPU isolated to
the public timer. RSS values are telemetry's sampled maximum for each process
during its HistoryCommand root; neither root covers its operation boundaries,
so these are **observed samples, not true process or phase peaks**. The
100-file root has only one sample per process and incomplete Service telemetry.
Store/History sizes are the closed SQLite files, measured after the command;
allocated size uses `st_blocks * 512`. All six databases report SQLite
`page_size=4096`, and no WAL or SHM files remained.

**Cache and proof.** The preflight and last check before each public timer
found **0 resident source payload pages**: 0/390, 0/2,045, and 0/27,503,
respectively. Each last check finished about 1 ms before its timer. Directory
and inode metadata residency was not qualified. The underlying runner still
records `cache_contract=source-cache-uncontrolled-v1` and
`admission_eligible=false`; these rows are research diagnostics, not cold
namespace admission. No warm payload from a previous run was used. The
100-file public operation returned a root, but the telemetry loss keeps its
row `INCOMPLETE`. The 1,000 and 10,000 rows had telemetry and cleanup `PASS`.
No full reopened readback was run for these three new rows.

The new 10,000-file observation is **1.419 s / 211.389 MB/s**. It does not
replace the previously accepted one-shot **1.110 s / 270.189 MB/s** observation
at the same product tree. The new public time is 27.8% longer. Both runs had
zero resident source payload pages, while metadata cache state and host load
were not controlled enough to attribute the difference to a code change.

## Why the new 10,000-file time is higher

This is **not a product-code regression**: the two rows have the same Core
product seal (`445f3c63…`), harness seal (`6d9a3e2e…`), five binary SHA-256
hashes, 300-MB fixture manifest, fixed stack/scope seed, and resulting root
(`e7850f75…`). Their last payload checks were both 0/27,503 pages. The new
Store holds the same 24,683 objects in the same three Saves, with **one fewer**
262,144-byte pack (1,263 versus 1,264); it did not persist more objects. The
[read-only SQLite count comparison](evidence/accepted-scale-20260923/10k-store-comparison.json)
retains the exact queries, per-Save results and database hashes.

| Retained measurement | Earlier 1.110-s row | New 1.419-s row | New minus earlier |
| --- | ---: | ---: | ---: |
| Public Init | 1,110.332 ms | 1,419.181 ms | **+308.849 ms** |
| Service HistoryCommand root | 1,106.726 ms | 1,418.561 ms | +311.835 ms |
| Named `history.import_files` child | 935.124 ms | 1,010.320 ms | +75.196 ms |
| All other named Service children | 86.288 ms | 84.346 ms | −1.941 ms |
| Service root minus all named children | 85.315 ms | 323.895 ms | **+238.580 ms** |
| Service + daemon lifecycle user CPU | 970.112 ms | 981.526 ms | +11.414 ms |
| Service + daemon lifecycle system CPU | 768.225 ms | 907.667 ms | **+139.442 ms** |

The Service's unnamed interval includes the `build_namespace` path around
`build_filesystem`, after `history.begin_tree_save` and before
`history.finish_tree_save`; those child timers time only begin and finish, not
the intervening build. The receipt cannot apportion the extra 238.580 ms
within that interval. The higher system CPU and `import_files` time are
consistent with a different host/filesystem scheduling or cache window, but
the retained measurements do **not** prove which one caused it. The benchmark
did not qualify inode/directory metadata residency or capture phase-specific
I/O counters. A specific cause would require a prospectively labelled
count/timing diagnostic of that unnamed interval and its Store calls, rather
than another 10k sample selected for a faster result.

The complete-command number moved the other way: 2,905.933 ms earlier versus
1,862.189 ms now. Time outside the public timer was 1,795.602 ms earlier
versus 443.008 ms now, so command time cannot explain the public-operation
change. The [earlier receipt](evidence/import-batch-integrated/receipt.json)
and [new receipt](evidence/accepted-scale-20260923/10000/daemon-host/init_namespace/namespace-10000/receipt.json)
retain the original counters.

The archived metadata and raw telemetry are in
[`evidence/accepted-scale-20260923`](evidence/accepted-scale-20260923).
The [100 receipt](evidence/accepted-scale-20260923/100/daemon-host/init_namespace/namespace-100-compact-v3/receipt.json),
[1,000 receipt](evidence/accepted-scale-20260923/1000/daemon-host/init_namespace/namespace-1000-compact-v3/receipt.json),
and [10,000 receipt](evidence/accepted-scale-20260923/10000/daemon-host/init_namespace/namespace-10000/receipt.json)
retain each status, identity, timer, CPU scope, and telemetry result. The
archive omits the 365 MB of SQLite files, source copies, and large file lists;
the untouched full outputs remain under
`benchmark-results/fs-bench-pro/issue237-accepted-scale-*-20260923-01/`.

Reproduction command, once per case with a **new** output path:

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py \
  --case namespace-100-compact-v3 \
  --independent-source-copy --fixed-operation-identity \
  --out "$PWD/benchmark-results/fs-bench-pro/NEW-OUTPUT-PATH"
```

Substitute `namespace-1000-compact-v3` or `namespace-10000` for the other
registered cases. This command is for reproduction at a new identity/window,
not a way to replace any of the receipts above.
