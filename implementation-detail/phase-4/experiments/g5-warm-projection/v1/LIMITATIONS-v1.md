# G5-2 v1 limitations

- macOS/APFS only; the per-attempt copy requires clonefile-backed `cp -cR`.
- Prepared fixtures and their cache state are warm-unknown. No cold-I/O claim.
- One process-lifetime private projection, one worker, and one pending slot only.
- No persistent seed, WAL, retry loop, worker pool, cancellation optimization,
  destructive GC, append/truncate specialization, or multi-file projection.
- Count-changing work uses full fallback; only final convergence is observed.
- APFS allocated bytes are observations, not unique physical-byte ownership.
- The 32 MiB RSS cap covers each combined foreground-plus-worker product
  process; the campaign reports and gates the maximum of its three observations.
- The 1 MiB cap is per individually owned streaming buffer, not aggregate RSS.
- Exact-vs-latest semantics, root rotation, zero SQLite writes, terminal cleanup,
  and conservation equations are correctness gates, not performance proxies.
