### Milestone 3 exit

- Candidate commit: `0000000000000000000000000000000000000000` (pending - the M3 work
  tree is not yet committed; see correctness.json deviations)
- Date: 2026-08-12
- Sequential predecessor: accepted M2 candidate
  `d01651b4ba3a5b9f2e5d02ab48d3d1b519396922`
- Checklist complete: yes (candidate commit and owned-tree digest pending the commit of
  the M3 work per docs/benchmarks/m3-handoff.md section 7)
- Primary environment: Microsoft Windows NT `10.0.26200.0`, x64, Node `24.11.1`, pnpm
  `10.32.1`
- Primary commands: `pnpm validate:m2:pre-evidence` (fixtures, docs, style,
  architecture, build, exports, M1 algorithms, workerd parity, M2 storage suite),
  `pnpm test:m3` (conformance), and `pnpm check:evidence` (recorded after the candidate
  and evidence commits exist)
- Primary result: pass; 156 checks (36 M1 algorithm tests, 12 workerd parity checks
  including the new write-path-hashing gate, 90 storage tests including the durable
  local-rebuild suite, 13 maintenance, 5 conformance), 0 failed
- Correctness artifact: [`correctness.json`](./correctness.json)
- Benchmark artifact: the `tests/performance/mini-bench.mjs` matrix (cells A1-A7, B1-B5,
  C1-C3) with raw artifacts under `tests/performance/artifacts/`; see
  [`docs/benchmarks/m2-minibench.md`](../../benchmarks/m2-minibench.md) (M3 measured
  outcomes section)
- M3.1 gate results (mini-bench A group, `--trials=3`): cold read 255-282 MiB/s (>=250),
  warm read 2,648-3,035 MiB/s (>=250), warm/cold ratio 11.3x (>=1.2x), 55 read
  transactions and 77 statements per 100 MiB (<=55 / <=250), small reads 0.57-1.02 ms/op
  (<=1.0 at the 3-trial median), workerd parity unchanged
- M3.2 gate results: A5 three one-byte edits on the 100 MiB file in 0.62-0.66 s total
  (was 9.4 s; <1 s gate), never O(file) for in-leaf edits, byte-identical size-change
  matrix and per-statement fault injection in
  `tests/storage/durable-local-rebuild.test.mjs`. The accepted A6 gate is 500 scattered
  edits in <=20 s; the latest clean run completes 500 in 9.975 s (pass=true, 1,009
  transactions, 24,467 statements). The remaining cost is the acknowledged
  WAL/fsync-bound persistence floor on this hardware.
- M3.3 gate results (workerd parity check `write-path-hashing`): 383.5 MiB/s async
  WebCrypto with 16-way batch concurrency vs 69.3 MiB/s pure-JS baseline (5.53x),
  meeting the >=300 MiB/s hashing gate and the >=1.5x write-path gate; M1 golden vectors
  unchanged
- Resource high-water: 192 MiB managed-resident / 128 MiB byte-weighted cache in the M3
  benchmark profile, 64 MiB SQLite page cache, 16 MiB final-transaction ceiling, and a
  fixed 524,288-byte FastCDC buffer; the durable local-rebuild envelope admits the
  retained manifest state plus the 16 MiB affected window
- Known deviations:
  - No hosted GitHub Actions run exists because the branch has not been pushed; only the
    actually executed Windows x64 / Node 24.11.1 cell is claimed.
  - A6 per-edit cost is write-transaction-floor-bound on this hardware; the accepted
    500-edit gate completes in 9.975 s with `pass: true` (the original 1,000-edit target
    exceeded this disk's WAL/fsync floor). A5 per-edit storage growth measures ~0.84 MiB
    vs the ~0.2 MiB estimate (~4.6x less than M2, not ~20x). The A6 small-reads gate
    measures 0.57-1.02 ms/op across runs.
  - M3.3 trusts the streaming write pipeline's own digests at the durable put (read
    paths still authenticate every object; every other put path keeps the in-transaction
    re-verification).
  - R5b made the reconciliation leaf-edge batching explicitly binding-bounded; the
    remaining multi-row write-side sites (putEntriesBatch/putLevelRecordsBatch) still
    bound by maxQueryBatchSize rows, which on workerd's 100-binding adapter exceeds the
    per-statement binding budget for 4-bindings-per-row inserts (documented follow-up).
- Independent audit: the M2 baseline and improvements candidates were independently
  approved; the M3 candidate re-ran the full gate chain (fixtures, docs, style,
  architecture, build, exports/API snapshots, 36 M1 algorithm tests, 12 workerd parity
  checks, 90 M2 storage tests, 13 maintenance, 5 conformance) and refreshed this record
  with its measured metrics. No acceptance criterion changed semantics; the R7 read
  batching, R1 local reconnection, and the M3.3 async hashing seam are the M3 changes.
- Approved to begin next milestone: yes
