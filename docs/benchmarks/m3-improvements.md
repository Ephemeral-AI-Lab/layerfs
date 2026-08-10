# M3 improvements spec (sequenced)

Validated improvement sequence for the accepted M2 SQLite engine, informed by the
[cas-cdc-cow prototype](https://github.com/agent-infra-foundation/agent-infra-book/tree/main/cloudflare/computer/benchmarks/cas-cdc-cow)
and by the 2026-08-11 three-pass audit recorded in
[`m2-improvements.md`](./m2-improvements.md). Every phase keeps the M2 acceptance
contracts: bounded memory, bounded statements, exact usage accounting, workerd parity,
and the host-neutral core (no node-only imports in `packages/fs/src`).

Measured starting point (mini-bench artifacts, HEAD `93a6a1f`): write 44.2 MiB/s, read
118.1 MiB/s cold / 118.6 warm, small reads 1.17 ms/op, A5 edits 9.4 s for 3 (streamed
fallback), A3/A4 read at 402 transactions and 1,787 statements per 100 MiB. Prototype
reference numbers: 1,000 edits on 16 MiB = 274 ms edit + 124 ms publish (10.1x vs naive,
96% less SQL); 10 B prepend 16.5x; 1.08x write amplification over 32 checkpoints;
explicit conflicts instead of lost updates.

## Sequencing rationale

R7 read-path batching lands first: it is smaller and lower-risk than R1, and it
compounds R1 directly (the local-rebuild window reads become cheaper, which is the only
path toward the sub-10 ms edit envelope). R1 is the safety-critical change
(staging/reconciliation surface); landing it after the read batching means the A5/A6
gates measure the better read path. M3.3 (async hashing) must be designed together with
M3.2's accept loop (currently sync) to avoid a double refactor. M5 hard-depends on
M3.2's publication splice and must not start before it lands.

## M3.1 - R7 read-path batching (verification half complete)

Reads today open a fresh authenticated cursor per 256 KiB pull window (re-descend
root-to-leaf per window) and fetch objects one at a time (1-2 SELECTs per object):
measured 402 read transactions and 1,787 statements per 100 MiB, and warm re-reads are
no faster than cold. The read lease is already pinned for the whole stream
(`readLeaseMs` 300 s), so cursor carry does not conflict with lease semantics.

Work items: batch cache-miss object fetch into
`SELECT size,bytes FROM efs_cas_objects WHERE hash IN (...)` (drop the redundant
per-object `#objectSize` SELECT; sizes are already declared in the manifest); carry the
cursor frames across pull transactions with the admission reservation released on close;
widen the internal pull window from 256 KiB toward the query-batch limit (~2 MiB) while
keeping the 256 KiB `preferredStreamChunkBytes` enqueue contract; per-cell
peak-at-stream- close metric in the mini-bench (housekeeping).

Expected targets (validated; GB/s-class is NOT achievable in M3 because the Node driver
copies each byte 2-3x - that is the M9 mmap/zero-copy profile):

| Metric                        | Today (measured)       | M3.1 target                                            |
| ----------------------------- | ---------------------- | ------------------------------------------------------ |
| Warm sequential read (A4)     | 118.6 MiB/s            | >=250 MiB/s (400-600 achievable)                       |
| Cold sequential read (A3)     | 118.1 MiB/s            | >=250 MiB/s (Node; ~66 MiB/s workerd cap without M3.3) |
| Warm vs cold (A4/A3)          | 1.00x (not measurable) | >=1.2x warm                                            |
| Read transactions per 100 MiB | 402                    | <=55                                                   |
| Read statements per 100 MiB   | 1,787                  | <=250                                                  |
| Small random reads (A6)       | 1.17 ms/op             | <=1.0 ms/op                                            |

Acceptance: A3/A4 MiB/s and statement/transaction gates above on the mini-bench; workerd
parity suite unchanged; no node-only imports. Risk: budget math - a ~2 MiB window with
13 x ~158 KiB objects plus per-row envelopes is within `maxQueryBatchBytes` (2 MiB) only
at the margin; either cap the window at ~1.9 MiB or raise the runtime limit. Scope
estimate: 400-800 LOC, low-medium risk.

## M3.2 - R1 bounded local reconnection in the durable-edit path

Small edits today either path-copy the entire leaf (requires the whole leaf <= 14.2 MiB
and re-chunks all of it) or fall back to the O(file) streamed rebuild (A5: ~4.6 s/edit
on the 100 MiB file). The M1 `local-rebuild.ts` machinery already implements
bounded-window re-chunk with FastCDC reconnection and splice (`reconnectOldOffset`,
`reusedPrefixEntries`, `rebuildManifestLocallyOrStream`), but it is only exercised by
tests, has a hard 16 MiB per-file cap (`MAX_DIAGNOSTIC_CONTENT_BYTES`), and has no
durable staging/persistence arm.

Work items: lift the 16 MiB diagnostic cap (explicit contract change with tests); add an
authenticated full-manifest loader (SQLite -> `DiagnosticBuiltManifest`; ~1 read
transaction for 100 MiB); build `persistLocallyRebuilt` mirroring `persistCandidate`
(staging begin + `protectSourceManifest` + batched object/node puts + reused-subtree
claims from the rebuilt spine + reconciliation + seal); rewire
`prepareDurableEditedContent` to attempt local reconnection before the O(file) fallback
(keep path-copy for equal-length in-leaf edits and streaming as the final fallback); new
admission envelope for the old manifest + affected window (25-35 MiB resident vs the
current `maxManagedResidentBytes/9` 14.2 MiB envelope); per-statement fault-injection
tests.

Expected targets (validated; "sub-10 ms" is NOT achievable - persistence alone is ~9
write transactions x ~4 ms under `synchronous=FULL`):

| Metric                          | Today (measured)              | M3.2 target                                                    |
| ------------------------------- | ----------------------------- | -------------------------------------------------------------- |
| A5: 3 one-byte edits on 100 MiB | 9.4 s (mixed path)            | <1 s total (expect 0.1-0.3 s); never O(file) for in-leaf edits |
| A6: 1,000 scattered edits       | 2 in 8 s (capped, pass=false) | 1,000 in <=20 s, pass=true                                     |
| Per-edit storage growth         | ~3.9 MiB (fallback)           | ~1 chunk + manifest nodes                                      |
| Insert/prepend class            | O(file) fallback              | O(changed window); 10-16x class per prototype                  |

Acceptance: size-changing/append/truncate/prepend/EOF cases byte-identical to the
streamed rebuild; every new persistence statement fault-injects cleanly; workerd parity
unchanged. Risk: the 16 MiB cap is asserted in M1 tests and framed as a diagnostic
invariant - lifting it redefines local-rebuild from diagnostic to production path and
must be an explicit contract change. Scope estimate: 1,200-1,800 LOC plus tests.

## M3.3 - async WebCrypto write hashing (workerd parity)

On workerd every byte is hashed twice today: once outside the transaction (`acceptChunk`
via pure-JS 66 MiB/s) and once inside `putObjectsBatch` (`#verifyDigest`), so a
sync-only seam fix would cap the gain at ~1.4-1.6x. Work items: extend the seam with an
async hash capability used on write paths only (outside read transactions, which must
stay sync); WebCrypto implementation in `packages/sqlite-cloudflare` with ~16-way
concurrency; design the accept pipeline together with M3.2's local-rebuild accept loop
(currently sync); decide the in- transaction double-verify question (trust op-computed
digests vs async verify before insert) - this is the difference between ~1.5x and the
full hashing win on workerd.

Expected targets (validated):

| Metric                          | Today (workerd) | M3.3 target                                                          |
| ------------------------------- | --------------- | -------------------------------------------------------------------- |
| Write-path hashing (outside tx) | 66 MiB/s        | >=300 MiB/s (not ~2 GB/s; WebCrypto call overhead per 158 KiB chunk) |
| 100 MiB streamed write          | baseline        | >=1.5x faster                                                        |
| In-tx verify                    | 66 MiB/s        | unchanged unless the double-verify decision is made                  |

Acceptance: M1 golden vectors and the 11 workerd parity checks byte-identical and
passing; no node-only imports. Risk: async pipeline changes memory/ordering invariants
(pending limit while awaiting, flush ordering, abort/cancel mid-hash). Scope estimate:
400-700 LOC.

## M5 - lazy branch edits (COW pages + patches, publish-time CDC)

Branch `replaceRange` today reads the whole file and prepares a full manifest per edit;
the overlay stack (`OverlayRepository.writePages`/`appendPatch`/`pinHeads`,
`efs_lease_cow_pages`, patches/segments, `maxBranchOverlayBytes` accounting) is built
and tested but unreferenced by the branch manager. Token-based conflicts already exist
and are exact (same-file writers conflict, zero lost updates).

Work items: branch content-state records (base manifest + overlay cursor per
branch/path); edit routing (equal-length <= page-size edits to 8 KiB COW pages - the
effective default `cow_page_bytes` is 8,192, not the prototype's 4 KiB - size- changing
edits to ordered patches, large edits materialize); authenticated branch read
composition (base + head pages + ordered patches); publication via the M3.2 splice with
staging leases/certificates; overlay pinning so GC never prunes mid-edit.

Expected targets (validated against the prototype):

| Metric                                       | Today (measured)                  | M5 target                 |
| -------------------------------------------- | --------------------------------- | ------------------------- |
| Branch-exclusive storage, 100 one-byte edits | ~64 MiB (B4-class full manifests) | >=96% less (<=2 MiB)      |
| 1,000-edit branch loop                       | O(file) per edit                  | <=15 s                    |
| Same-file two-writer publish                 | explicit conflict                 | unchanged (already exact) |
| Publish for page/patch-only branch           | -                                 | never O(file)             |

Acceptance: branch `readFile` equals main byte-for-byte after publish; existing
branch/conflict/replay suite passes unchanged. Risk: publication depends on M3.2's
splice; read composition is new authenticated surface. Scope estimate: 1,000-1,600 LOC
plus tests.

## M3 backlog (hardening, low priority)

- R5 (a): restore the declared-length re-check for intra-batch duplicate leaf edges in
  `reconcileBatch` (strictness regression on forged digest-consistent manifests only;
  legitimate manifests unaffected).
- R5 (b): make the reconciliation batch binding count explicit instead of implicitly
  bounded by leaf size.
- Harness: emit per-cell admission peak at stream close; drop the `null` overhead
  emission for zero-payload cells; record a dirty-tree marker in artifact commit fields.
