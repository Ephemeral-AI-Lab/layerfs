# G5-3 v2 limitations

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
- V2 changes only the post-history Store/closed-SQLite lifetime before the
  independent concurrency sentinel. It does not change timers, product receipt,
  workload, population, or any threshold.
- V2 creates one owned sealed input root by exact copy only. It does not
  regenerate the H11 oracle or change any input byte.
- Product source/release and direct focused receipt are settled; both analyzers
  and current-byte self-check PASS. Owned-input preparation, freeze, and zero-row
  forecast remain execution prerequisites; no v2 campaign has run.
- Gate cannot run directly after forecast: an accepted screen plus the compact
  post-screen static closure is mandatory.
