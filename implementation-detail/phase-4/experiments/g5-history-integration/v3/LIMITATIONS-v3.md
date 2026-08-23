# G5-3 v3 limitations

- This is one continuous 1 MiB retained-history mechanism, not a file-size or
  multi-process scaling claim.
- Exact 4 KiB range and separately observed same-size edit evidence occurs only
  at the four gate checkpoints (two in screen), not after every fill edit.
- The 10 MiB concurrency case is one 2-reader/1-writer COMMIT sentinel.
- Read-only reachability and storage slopes are reused from sealed H11 v9; the
  slopes remain diagnostic rather than general population claims.
- Exact/latest semantic distinction and shutdown/restart remain reused G5-2 v3
  authority; G5-3 checks only checkpoint conservation in the long-lived child.
- Random edits, branch DAGs, backup/restore, GC, and additional lifecycle
  research are not claimed.
- V1 is an honest screen REVISE caused only by RSS 22,102,016 bytes exceeding
  the 20,971,520-byte cap; its product/Q/cleanup evidence remains preserved.
- V2 is terminal NO-GO after its compatibility-repaired direct process reached
  RSS 21,643,264 bytes against the unchanged 20,971,520-byte cap.
- V3 changes only the SQLite cache profile for the three simultaneously live
  concurrency connections. All three are exactly 1,280 pages; page size is
  4,096; aggregate ceiling is 15,728,640 bytes; aggregate reduction is
  8,847,360 bytes; connections high/terminal are 3/0. No timer, workload,
  population, or threshold changes.
- The initial writer-only v3 diagnostic is preserved but is not controlling.
- V3 reuses the sealed v2 input root without copying or regeneration. Its own
  adoption manifest must rehash modes, files, row count, v2 custody, and the
  current executable.
- Product source/release and controlling direct receipt are settled; focused
  adoption, both analyzers, and the cache mutation self-check PASS. Input-reuse
  adoption, freeze, and zero-row forecast remain prerequisites; no v3 campaign
  has run.
- Gate cannot run directly after forecast: an accepted screen plus the compact
  post-screen static closure is mandatory.
