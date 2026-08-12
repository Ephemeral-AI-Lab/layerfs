# M3 handoff note

Session-to-session handoff for the next milestone session. Read this before touching
code; the sibling specs [`m3-improvements.md`](./m3-improvements.md) (sequenced plan,
targets, risks) and [`m2-improvements.md`](./m2-improvements.md) (accepted baseline,
audit record) carry the full detail. The benchmark matrix lives in this file's
"Benchmark optimizing targets" section - it is the single source of truth for what M3
must measure.

## 1. Session state

- Repo: `C:\Users\yifan\code\Ephemeral-AI-Lab\ephemeral-ai-fs`, branch
  `agent/draft-spec`, HEAD `f193bc0` (docs only since `93a6a1f`; the engine state that
  all numbers below refer to is `93a6a1f`).
- M2 is DONE and accepted: R3 (host-injected native hashing seam), R5 (statement
  batching), FastCDC copy reduction, the mini-bench matrix, and a refreshed evidence
  record (`d01651b` candidate -> `ae2ecce` evidence; owned-tree digest `649f8c50...`).
  `pnpm validate:m2` passes end-to-end from HEAD.
- A three-pass audit (2026-08-11) returned GO with two R5 hardening nits and harness
  caveats; the re-based M3 targets are validated - the original "sub-10 ms edits" and
  "GB/s-class reads" claims are NOT achievable (see section 6 anti-targets).
- The branch is NOT pushed (71 commits ahead of origin; the push was rejected because
  the OAuth token lacks `workflow` scope and the unpushed range touches
  `.github/workflows/ci.yml`). To push later:
  `gh auth refresh -h github.com -s workflow`, then `gh auth setup-git` (git uses Git
  Credential Manager otherwise), then `git push -u origin agent/draft-spec`. Pushing is
  not a milestone blocker; the evidence record documents the local-only CI cell as a
  deviation.
- How to verify the state: `pnpm validate:m2`; `node tests/performance/mini-bench.mjs`
  (~1 min, writes artifacts); `git status --porcelain` must be empty before
  `pnpm check:evidence`.

## 2. Repository map (file/folder structure)

```text
packages/
  fs/                        @ephemeralai/fs - the host-neutral core (NO node: imports, architecture gate)
    src/cas/                 sha256.ts (pure-JS IncrementalSha256 + HashFunction type), bytes.ts
    src/cdc/                 fastcdc.ts (StreamingFastCdc; #emitChunk returns fresh slices)
    src/cow/                 pages.ts (COW page types)
    src/manifests/           codec.ts (binary node encode/decode + digest verify), builder.ts,
                             cursor.ts (ManifestSequentialCursor - descend per cursor),
                             grouping.ts, binary.ts
    src/operations/          filesystem.ts (public EphemeralFS ops; readStream 256 KiB pulls),
                             streaming-prepare.ts (write pipeline + acceptChunk),
                             durable-edit-prepare.ts (path-copy / streamed fallback),
                             local-rebuild.ts (M1 bounded reconnection - 16 MiB cap, not wired),
                             streamed-rebuild.ts (rebuildManifestLocallyOrStream),
                             manifest-io.ts (readManifestRange), storage-ports.ts (port seams),
                             branch-engine.ts (branch manager; replaceRange = read+write),
                             node-vfs-bridge.ts, full-rebuild.ts, maintenance.ts, patches/
    src/sqlite/              driver.ts (FilesystemSQLiteDriver + SqliteHashFunction),
                             operations-storage.ts (createSqliteOperationsStorage; hashBytes ?? sha256),
                             content-repository.ts (CAS store; readObjectInto = 1-2 SELECTs/object),
                             manifest-cursor.ts (SQLiteAuthenticatedManifestCursor wrapper),
                             staging-repository.ts (leases, reconcileBatch, batched leaf edges),
                             manifest-tree-repository.ts, schema.ts, unit-of-work.ts (budgets)
    src/resources/           limits.ts (defaults; maxQueryBatchBytes 2 MiB, maxQueryBatchSize 256,
                             maxFinalTransactionBytes 16 MiB, readLeaseMs 300 s), safe-integers.ts
    src/cache/               content-cache.ts (64 MiB LRU; admit copies; copyInto = cache-trust)
  sqlite-node/               NodeSQLiteDriver: node:crypto hashBytes, statement cache, WAL backpressure
  sqlite-cloudflare/         CloudflareSQLiteDriver: no native hasher -> pure-JS fallback
  testkit/ node-vfs/ replication/
tests/
  algorithms/                M1 (35) - golden vectors, local/streamed rebuild
  workerd/                   M1 workerd parity (11 checks, pure-JS hashing)
  storage/ node-integration/ maintenance/   M2 (99)
  conformance/               M3 suite (validate:m3 = validate:m2 + tests/conformance)
  performance/               mini-bench.mjs + artifacts{,-baseline,-r3}/ (NOT milestone-owned)
scripts/                     check-architecture/style/docs/exports/evidence, run-test-suite
docs/benchmarks/             m2-improvements.md, m2-minibench.md, m3-improvements.md, THIS FILE
docs/evidence/m0|m1|m2/      correctness.json + exit.md per accepted milestone
```

Ownership for the evidence digest: m2-owned = `packages/fs/src/**`,
`packages/sqlite-node/src/**`, `tests/storage/**`, `tests/node-integration/**`,
`tests/maintenance/**`. Changing any of those mid-M2 forces a digest refresh; in M3 the
same rule applies to whatever `scripts/check-evidence.mjs` will own for m3 (the checker
currently only knows m0/m1/m2 - extending it is part of accepting M3).

## 3. Engine algorithm at a glance (what M3 touches)

- **Write**: `prepareContentStreaming` streams input -> `StreamingFastCdc` chunks ->
  `acceptChunk` hashes via `port.hashBytes` (Node: node:crypto; elsewhere pure-JS) ->
  staging lease (`staging.begin`) -> batched `putEntriesBatch` + `putObjectsBatch` ->
  manifest levels -> root -> `reconcileBatch` until complete -> `seal` -> certificate.
- **Read**: `readStream` pulls 256 KiB windows (`preferredStreamChunkBytes`); per pull
  one read transaction opens a FRESH `SQLiteAuthenticatedManifestCursor` (re-descend
  root->leaf), reads objects one at a time (`readObjectInto`: cache `copyInto` hit ->
  memcpy; miss -> `#objectSize` SELECT + `#withColdObject` SELECT + full-BLOB verify +
  admit). The read lease is pinned for the whole stream (`readLeaseMs` 300 s), and a
  cursor holds no open SQLite statement - both are the preconditions M3.1 relies on.
- **Edit**: `replaceRange` -> `prepareDurableEditedContent`: path-copy when the whole
  leaf <= `floor(maxManagedResidentBytes/9)` = 14.2 MiB (re-chunks the ENTIRE leaf and
  registers sibling subtrees as reused claims); otherwise the O(file) streamed fallback.
  Default leaves are ~16-20 MiB, so 100 MiB-file edits always fall back (A5 measured
  ~4.6 s/edit). `local-rebuild.ts` already implements bounded-window re-chunk + FastCDC
  reconnection + splice but is capped at 16 MiB (`MAX_DIAGNOSTIC_CONTENT_BYTES`,
  local-rebuild.ts:608) and has no durable persistence arm - it is not wired into the
  durable path.
- **Hashing seam**: `OperationsStorage.hashBytes` is SYNC-only. Workerd has no native
  hasher, and every workerd write hashes every byte TWICE (acceptChunk outside the tx
  - `#verifyDigest` inside `putObjectsBatch`) - the in-tx half caps the M3.3 win.
- **Branch overlay**: `OverlayRepository.writePages/appendPatch/pinHeads`,
  `efs_lease_cow_pages`, patches/segments and `maxBranchOverlayBytes` accounting exist
  and are tested, but `BranchHandle.replaceRange` does readFile + writeFile (full
  manifest per edit). Conflicts are token-based and exact. Default `cowPageBytes` is
  16,384 (`operations/filesystem.ts:222`) - the prototype's COW pages were 4 KiB.

## 4. M3.1 - R7 read-path batching (FIRST, ~400-800 LOC, low-medium risk)

Order rationale: smaller and lower-risk than R1, and it compounds R1 (cheaper window
reads for the local rebuild). Touchpoints and algorithm:

1. **Batched object fetch** in `sqlite/content-repository.ts`: on cache miss, fetch
   `SELECT size,bytes FROM efs_cas_objects WHERE hash IN (...)` for the window's
   cache-miss set (32-64 hashes) instead of 1-2 SELECTs per object; validate each row's
   size against the manifest-declared length. The per-object `#objectSize` SELECT
   becomes redundant (sizes are already declared by the manifest entries).
2. **Cursor carry** in `sqlite/manifest-cursor.ts` + `manifests/cursor.ts`: hold the
   `ManifestSequentialCursor` frames across pull transactions (safe: no open SQLite
   statement; the admission reservation releases on `close()`). Handle cache eviction
   between pulls and `close()` during a pull without double-release.
3. **Window widening**: internal pull window 256 KiB -> ~2 MiB (toward
   `maxQueryBatchBytes`), while keeping the 256 KiB enqueue contract of
   `preferredStreamChunkBytes` (enqueue 256 KiB sub-chunks). BUDGET MATH: ~13 objects x
   ~158 KiB + per-row envelope (~2.06 MiB) exceeds `maxQueryBatchBytes` 2 MiB at the
   margin - cap the window at ~1.9 MiB or raise the runtime limit, and respect
   `maxBindings` (workerd 100, node 32,766).
4. **Harness housekeeping first** (tests/performance, not milestone-owned): per-cell
   admission peak emitted at stream close (today the observer fires at `readStream()`
   construction, so read-cell peaks are stale), 3-trial runs, dirty-tree marker in
   artifact `commit` fields.

## 5. M3.2 / M3.3 / M5 pointers (later phases)

- **M3.2 (R1, ~1,200-1,800 LOC)**: lift the 16 MiB `MAX_DIAGNOSTIC_CONTENT_BYTES` cap
  (explicit contract change; asserted in M1 tests); add an authenticated full-manifest
  loader (SQLite -> `DiagnosticBuiltManifest`); build `persistLocallyRebuilt` mirroring
  `persistCandidate` in `durable-edit-prepare.ts` (staging begin +
  `protectSourceManifest` + reused-subtree claims from the rebuilt spine + reconcile +
  seal - the `efs_staging_workspaces` table already anticipates this); rewire
  `prepareDurableEditedContent` to try local reconnection before the O(file) fallback;
  new admission envelope (25-35 MiB resident vs the 14.2 MiB `maxManagedResidentBytes/9`
  envelope today).
- **M3.3 (async hashing, ~400-700 LOC)**: extend the seam with an async write-path
  hasher (WebCrypto in `packages/sqlite-cloudflare`, ~16-way concurrency); design the
  accept pipeline together with M3.2's local-rebuild accept loop (currently sync) to
  avoid a double refactor; DECIDE the in-tx double-verify question first (trust
  op-computed digests vs async verify before insert) - it is the difference between
  ~1.5x and the full workerd hashing win.
- **M5 (lazy branch edits, ~1,000-1,600 LOC)**: branch content-state records (base
  manifest + overlay cursor per branch/path); edit routing (equal-length <= page-size ->
  COW pages, size-changing -> ordered patches, large -> materialize); authenticated
  branch read composition; publication via the M3.2 splice with staging
  leases/certificates; overlay pinning against GC. Hard-depends on M3.2.

## 6. Benchmark optimizing targets (most important)

Run: `node tests/performance/mini-bench.mjs` (full matrix ~55 s, budget 115 s;
`--cell=A1|B1|C1` for subsets). Artifacts: `tests/performance/artifacts/` (one
`efs-benchmark-result-v1` JSON per cell). Today = measured at HEAD `93a6a1f`; targets
are validated (prototype reference: the cas-cdc-cow benchmark measured 1,000 edits in
274+124 ms / 10.1x and prepend 16.5x vs a fixed-chunk baseline).

| Phase   | Item                                 | Today (measured)             | Expected target                  | Diff vs current                   |
| ------- | ------------------------------------ | ---------------------------- | -------------------------------- | --------------------------------- |
| M2 done | R3+R5+copy reduction                 | write 17.9 / read 43.8 MiB/s | write 44.2 / read 118.1 MiB/s    | 2.5x / 2.7x (measured)            |
| M3.1    | Warm sequential read (A4)            | 118.6 MiB/s                  | >=250 MiB/s (400-600 achievable) | >=2.1x (3.4-5.1x achievable)      |
| M3.1    | Cold sequential read (A3)            | 118.1 MiB/s                  | >=250 MiB/s                      | >=2.1x                            |
| M3.1    | Small random reads (A6)              | 1.17 ms/op                   | <=1.0 ms/op                      | >=1.17x faster                    |
| M3.1    | Warm vs cold (A4/A3)                 | 1.00x (indistinguishable)    | >=1.2x warm                      | +20%+ (new capability)            |
| M3.1    | Read transactions per 100 MiB        | 402                          | <=55                             | >=7.3x fewer                      |
| M3.1    | Read statements per 100 MiB          | 1,787                        | <=250                            | >=7.1x fewer                      |
| M3.2    | A5: 3 one-byte edits on 100 MiB      | 9.4 s (O(file))              | <1 s total (expect 0.1-0.3 s)    | >=9.4x (31-94x expected)          |
| M3.2    | A6: 500 scattered edits              | 2 in 8 s, pass=false         | 500 in <=20 s, pass=true         | ~110x per edit (~4.4 s -> ~40 ms) |
| M3.2    | Per-edit storage growth              | ~3.9 MiB                     | ~1 chunk + nodes (~0.2 MiB)      | ~20x less                         |
| M3.3    | Workerd write hashing                | 66 MiB/s (pure-JS)           | >=300 MiB/s                      | >=4.5x                            |
| M3.3    | 100 MiB workerd write                | baseline                     | >=1.5x faster                    | >=1.5x                            |
| M5      | Branch-exclusive storage (100 edits) | ~64 MiB                      | <=2 MiB (>=96% less)             | >=32x less                        |
| M5      | 1,000-edit branch loop               | O(file) per edit             | <=15 s                           | ~10x class (per prototype)        |

Explicitly NOT targets (audit re-based):

| Claimed earlier                  | Reality                         | Diff vs current                                                         |
| -------------------------------- | ------------------------------- | ----------------------------------------------------------------------- |
| Sub-10 ms edits                  | 15-40 ms Node, 40-80 ms workerd | persistence floor: ~9 write txs x ~4 ms (`synchronous=FULL`)            |
| GB/s-class warm reads            | ~400-600 MiB/s Node             | driver copies each byte 2-3x; GB/s is the M9 mmap/zero-copy profile     |
| WebCrypto "native-class" ~2 GB/s | ~300-600 MiB/s                  | per-158 KiB-chunk call overhead + DO crypto pool contention             |
| M3c write win "7.5x"             | ~1.4-1.6x                       | every workerd byte is hashed twice; in-tx `#verifyDigest` stays pure-JS |

Phase gates (mini-bench cells): M3.1 = A3/A4 >=250 MiB/s + warm >=1.2x cold + <=55 read
txs / <=250 stmts per 100 MiB + A6-small-reads <=1.0 ms/op + workerd parity unchanged.
M3.2 = A5 <1 s total, never O(file) for in-leaf edits + A6 500 in <=20 s pass=true +
byte-identical size-change matrix + per-statement fault injection. M3.3 = workerd write
hashing >=300 MiB/s outside txs + 100 MiB write >=1.5x + golden vectors unchanged. M5 =
branch 100-edit growth >=96% less + 1,000-edit <=15 s + publish byte-exact + conflict
suite unchanged.

Harness caveats to keep in mind when reading the anchors: stream-read cells' managed
peak is a construction-time snapshot (fix is the M3.1 housekeeping item); single-trial
runs carry 10-20% variance (quote ~2 significant figures); read cells grow +53.6 KiB
from read-lease bookkeeping; A5's per-edit times mix the fallback (~4.6 s) and path-copy
(230 ms) paths; the "cold" label means cold engine cache (OS/SQLite caches are warm
after the untimed setup writes - the reopen cells A7/B5 measure the truly cold path at
~66-68 MiB/s).

## 7. Evidence refresh procedure (when M3-owned files change)

Follow the exact M2 pattern (`scripts/check-evidence.mjs` validates the accepted
milestone only - m0/m1/m2 today; extending it for m3 is part of accepting M3):

1. `git status --porcelain` must be empty before any evidence check.
2. After all gates pass: commit the work (candidate). Compute the digest:
   `node scripts/check-evidence.mjs --owned-tree-digest m3 <candidate>`.
3. Update `docs/evidence/m3/correctness.json` (commit, digest, refreshed metrics from
   the ACTUAL run) and `exit.md` (candidate, date, checklist, deviations); the evidence
   commit must be directly parented by the candidate.
4. Prettier every file; run `pnpm validate:m3` (fixtures, docs, style, architecture,
   build, exports, m1, workerd, m2, conformance) + `pnpm check:evidence` from HEAD.

## 8. Backlog and ground rules

- M3 backlog: R5 (a) restore the declared-length re-check for intra-batch duplicate leaf
  edges in `reconcileBatch`; R5 (b) make the reconciliation batch binding count
  explicit; harness peak-at-stream-close + dirty-tree marker + drop null overhead
  emission.
- Ground rules: no node-only imports in `packages/fs/src` (architecture gate); hashes
  must stay byte-identical to pure-JS (M1 golden vectors + workerd parity); memory
  admission, statement/elapsed budgets, `efs_usage` exactness, quota ceilings, and WAL
  backpressure semantics are untouched; worktree stays clean before `check:evidence`; no
  commits without explicit instruction.
