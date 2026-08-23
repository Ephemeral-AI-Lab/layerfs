# G5-2 v3 limitations

- Input preparation uses four exact 250,000-byte modes, targets at most 20
  seconds, must finish all modes in at most 60 seconds, and must leave no more
  than 10,000,000 apparent or
  allocated bytes across the final input root. This is preparation feasibility,
  not product timing evidence.
- This is a 250,000-byte mechanism benchmark only. It makes no larger-file,
  multi-size, or file-size-scaling performance claim.
- Prepared inputs are fsynced and sealed `0444`/`0555`, then reopened and
  rehashed before freeze and immediately before each APFS clone. Only the
  private attempt clone is made writable after its exact clone inventory.

- macOS/APFS only; the per-attempt copy requires clonefile-backed `cp -cR`.
- Prepared fixtures and their cache state are warm-unknown. No cold-I/O claim.
- One process-lifetime private projection, one worker, and one pending slot only.
- No persistent seed, WAL, retry loop, worker pool, cancellation optimization,
  destructive GC, append/truncate specialization, or multi-file projection.
- Count-changing work uses full fallback; only final convergence is observed.
- APFS allocated bytes are observations, not unique physical-byte ownership.
- The 32 MiB RSS cap covers each combined foreground-plus-worker product
  process; the campaign reports and gates the maximum of all five observations
  (three scheduled performance reports and two exact fault-process reports).
- The 1,048,576-byte cap is per individually owned streaming buffer, not
  aggregate RSS.
- Exact-vs-latest semantics, root rotation, zero SQLite writes, terminal cleanup,
  and conservation equations are correctness gates, not performance proxies.
- Every planned product invocation has a durable command and clone disposition;
  stdout, stderr, return status, RSS text, and parsed-receipt disposition are
  fsynced before evaluation. A later failure never erases earlier evidence.
- The protected complete wall closes only after fixture cleanup, one preliminary
  analyzer pair, global-lock release, and its release receipt. Final analyzers
  recompute from the terminal-bound raw bundle after that wall closes.
- V3 preparation accepts only a compact parent/latest/token/edit fixture. It
  rejects per-revision full sources and does not perform `Theta(J * L)` source
  inventory hashing.
