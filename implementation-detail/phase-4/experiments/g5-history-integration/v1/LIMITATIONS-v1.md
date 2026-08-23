# G5-3 v1 limitations

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
- Product source, executable, CLI, and one durable screen-shaped focused receipt
  are settled and hash-bound. Input adoption, freeze, and zero-row forecast are
  still absent execution prerequisites; no campaign has run.
- Inputs are immutable external operands adopted by exact hash/stat; G5-3 does
  not regenerate the H11 oracle or create a new input root.
- Gate cannot run directly after forecast: an accepted screen plus the compact
  post-screen static closure is mandatory.
