# G4 Round 1 — core architecture research report

- Lane: **core architecture (CAS + CDC + COW + canonical identity + mapping + storage + native projection/VFS)**
- Status: **ROUND-1 COMPLETE / READ-ONLY / ZERO DISPOSABLE EXPERIMENTS**
- Finality: **FINAL — cross-review/monitor corrections incorporated for C1/C5 boundary, G2 timers, post-restart seed authority, C4 edit-authority/resource limits, and the C3 SQLite-first storage ladder**
- Starting checkpoint: branch `codex/empty-worktree`, commit `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`
- Report date: 2026-08-22
- Scope boundary: this report changes no product source, retained/sealed evidence, shared ledger, or benchmark state. The pre-existing untracked handoff remained present and byte-identical at SHA-256 `8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00` (`implementation-detail/phase-4/experiments/g4-materialization-acceptance/round-1-research-handoff.md:1-430`).

## Executive disposition

The shortest architecture that can plausibly satisfy the whole lifecycle is **not a new canonical format**. Canonical-v2 already is an authenticated, history-independent extent map: the root commits file mode, total raw length, reference count, level, cumulative child extents, and child object IDs; leaves commit ordered `(raw_length, object_id)` pairs (`crates/layerfs-core/src/canonical_v2.rs:15-31`, `:72-166`, `:208-255`). Every Object ID commits the complete canonical object bytes under a domain-separated BLAKE3 hash (`crates/layerfs-core/src/identity/digest.rs:6-25`, `:39-65`), and validation checks the expected identity before decoding (`crates/layerfs-core/src/object/codec.rs:153-165`). Canonical-v2 already removed the prior raw-content identity lane and is the accepted path; **do not re-propose raw-ID removal** (`implementation-detail/phase-4/baseline/canonical-v2-baseline-v1.md:90-120`).

The viable system split is:

```text
authoritative logical plane
  expected namespace/file root
    -> canonical-v2 mapping DAG
      -> immutable canonical chunk objects

rebuildable physical value plane (ordered research ladder)
  first: one-file SQLite authority
    -> bounded immutable extent BLOB rows + catalog/head/receipts
  second only if needed: external immutable segment locations
    -> subordinate to SQLite's committed catalog/head, never a second truth
  every consumed canonical object authenticated against ObjectId

rebuildable native projection plane (candidate)
  bounded content-keyed verified seed cache
    -> reflink/clone to private temp when supported
      -> patch selected authenticated ranges
      -> fsync + rename + directory fsync
    -> complete authenticated stream fallback everywhere else

VFS/SDK projection (candidate)
  lazy range reads directly from the same canonical-v2 DAG/value plane
```

The highest-confidence Round-2 candidates are therefore:

1. **Prove and measure closure/proof-product deletion or fusion inside the accepted one-traversal Canonical-v2 verified stream.** The current reconstruction already traverses the mapping/chunks once while updating several evidence products. C1 keeps expected-root and per-object authentication and changes only product-redundant closure/occurrence bookkeeping. This is format-preserving and can attack an observed `88.483 ms` closure-family ceiling, but it is an authority-contract change, not a free benchmark edit or a newly invented single-pass route.
2. **Promote the G3 mechanism into a bounded, service-custodied, content-keyed native seed cache design**, never an ordinary same-UID mutable-file cache. G3's `3.414166 ms` one-byte screen proves APFS clone/patch materialization potential only for an operation-local, read-only, unlinked seed; it does not prove persistent/cross-process seed authority or canonical mapping/edit authority after reopen (`implementation-detail/phase-4/baseline/g3-incremental-materialization-baseline-v1.md:20-40`, `:48-65`, `:117-130`).
3. **Define direct VFS streaming on the current mapping**, so open/setup can be small and reads pay only for selected authenticated extents. This is a shared product improvement; it does not make the first sequential read of all 100 MiB sublinear.
4. **Retain an ordered, post-G4 storage research ladder rather than a G4 do-now item.** First lower-bound bounded immutable authenticated extent BLOBs inside the same SQLite file/transaction authority. Only if that is insufficient should research consider external immutable segments with SQLite catalog/head authority. External bytes without a committed SQLite locator are orphans, never a second durable truth.

Additional Merkle/outboard layers, a new CDC profile, prolly-tree mapping, per-chunk reflink assembly, Git-style packing, and foreground compression are deferred or rejected below. They do not have a measured current bottleneck large enough to justify their compatibility and failure surface.

## 1. Custody and method

### 1.1 Starting state

Read-only preflight observed:

- `HEAD=5c342f0ae24ecc69f2bfc03da1c05d1074fe956a` on `codex/empty-worktree`;
- tracked diff stream SHA-256 was the empty-stream hash `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`;
- the only starting untracked path was `implementation-detail/phase-4/experiments/g4-materialization-acceptance/`, containing the protected Round-1 handoff;
- no active G4/G5/Phase-4/cargo benchmark process and no benchmark lock was observed;
- no G4 acceptance, G5, production integration, commit, chmod, reset, clean, or sibling-repository action was performed.

Environment freeze (read-only observation): UTC `2026-08-22T06:23:08Z`; monotonic `1146842102652833`; macOS `26.4.1` build `25E253`; Darwin `25.4.0`; `aarch64-apple-darwin`; Apple M3 Max; 14 logical/physical CPUs; 38,654,705,664 bytes RAM; SQLite CLI `3.51.0`; Rust/Cargo `1.96.0`; APFS Data volume on Apple SSD AP1024Z / Apple Fabric, capacity 994,662,584,320 bytes and free 428,441,878,528 bytes. These observations are descriptive, not a cold-cache or stable-media claim. G3 itself makes the same limitation explicit (`implementation-detail/phase-4/baseline/g3-incremental-materialization-baseline-v1.md:117-123`).

### 1.2 Research sequence and evidence rule

Local production code, benchmark-private code, governing Phase-3/4 documents, G0/G1/G2/G3 retained evidence, v11/v12 repair records, canonical-v2 evidence, old carrier/packing/compression evidence, and Cargo topology were inspected before external research. External claims below use direct primary implementation/document/paper links. Historical absolute medians are ceilings and sizing inputs only; a promotion claim still requires a fresh adjacent position-balanced A/B under the benchmark lock (`research/phase-4/foundations/benchmark-and-evidence.md:1-92`; `implementation-detail/phase-4/experiments/g4-materialization-acceptance/round-1-research-handoff.md:105-171`).

No disposable experiment was run. Static code and sealed measurements already determine the architectural facts in this lane, while any new wall-time claim would be timing-sensitive and require lead-coordinated benchmark-lock custody.

## 2. Actual current system trace

### 2.1 Canonical identity and mapping

Canonical-v2 is a Merkle-style extent DAG, even though it is not named an “extent map” in code:

- Each leaf reference is exactly 36 bytes: a `u32` raw length followed by the 32-byte canonical chunk Object ID (`crates/layerfs-core/src/canonical_v2.rs:15-22`, `:72-139`).
- Leaf validation enforces non-empty/fixed canonical partitioning and a maximum raw chunk size (`crates/layerfs-core/src/canonical_v2.rs:141-166`).
- Branch records commit level and ordered child descriptors; child descriptors include cumulative logical end and child Object ID (`crates/layerfs-core/src/canonical_v2.rs:168-205`; imported descriptor contract at `:9-13`).
- The root commits mode, total length, occurrence count, level, child count, cumulative extents, and child IDs (`crates/layerfs-core/src/canonical_v2.rs:208-255`).
- An expected root plus authentication of each reached canonical object therefore determines the exact ordered output byte string. A physical segment ID, SQLite rowid, path, inode, pack offset, or cache locator must never enter this identity.

One latent compatibility issue is real: `selected_mapping_profile_id()` hashes the mapping capacities `[64, 64, 262144, 8388608]`, but not the CDC minimum/target/maximum, normalization shift, seed, gear table, or chunker version (`crates/layerfs-core/src/canonical_v2.rs:24-31`). The current CDC contract is separately hard-coded as 8/16/32 KiB, normalization shift 2, seed 0 (`crates/layerfs-core/src/cdc/mod.rs:11-20`) with a bounded 32-KiB streaming buffer (`:31-56`). Existing roots remain unambiguous because they commit the resulting ordered chunk IDs. Recreating/editing equivalent content across implementations, however, relies on the globally frozen source contract rather than a CDC-profile ID embedded in mapping identity. **Any chunk-profile change must get a new explicit versioned content/profile contract; it must not silently reuse canonical-v2's current profile ID.**

### 2.2 Storage and authentication today

The public engine is a single mutexed SQLite connection (`crates/layerfs-engine/src/lib.rs:243-249`) configured to rollback journal `DELETE`, `synchronous=FULL`, `temp_store=FILE`, and `mmap_size=0` (`:683-715`). The authoritative object table stores `object_id`, kind, canonical length, and complete canonical bytes as a SQLite BLOB (`:717-747`).

There are two distinct read implementations:

1. **Public engine API.** `read_object_range` authenticates the complete BLOB and then reopens it for the selected range (`crates/layerfs-engine/src/lib.rs:912-965`). `authenticate_blob` opens and reads the complete BLOB once to hash it, then opens and reads it again to parse/validate it (`:968-1017`). `load_object` first resolves length, calls the range path for the whole canonical value, then constructs another `ObjectRecord`, which hashes/decodes the returned owned bytes again (`:377-381`). Thus a public range can make two full passes plus the selected read, and a full object load can add another owned validation pass. Existing-object insertion also SELECTs the entire incumbent into a `Vec`, compares bytes, and validates it (`:850-909`). This is a shared implementation inefficiency, but it is not the private G4 benchmark's path and has no accepted G4 wall attribution.
2. **Current Phase-4 benchmark store.** It also stores complete BLOBs (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:2364-2401`), but its borrowed paths validate one borrowed row (`:3040-3062`) and batch up to 64 ordered leaf references per SQL statement (`:3064-3148`). The 100-MiB warm reconstruction still performs about 170 SQL queries, returns/authenticates 5,371 objects, reads 5,284 chunk BLOBs, borrows 104,926,292 chunk bytes, and authenticates 105,122,401 canonical bytes (`implementation-detail/phase-4/test-checkpoint-report/cp-0010-dirty-72ed9fee8e6a-fastcdc-v2-phase4-grind.md:201-215`).

### 2.3 Reconstruction, ranges, and redundant evidence products

The retained benchmark reconstruction:

- authenticates namespace root, file root, mapping nodes, and every chunk;
- validates mapping shape, level, cumulative extents, occurrence count, and raw lengths;
- hashes reconstructed raw bytes into a source/output fingerprint;
- hashes the ordered `(raw_length, object_id)` sequence again; and
- hashes role, ID, length, and the complete canonical bytes again into a closure commitment (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:2150-2169`, `:8238-8368`, `:9058-9177`).

The expected Canonical-v2 root already commits that ordered mapping, and each validated Object ID already commits its full canonical bytes. Therefore the closure and occurrence commitments do not appear to add a new cryptographic binding **once the expected root is itself authoritative and every reached object is authenticated**. This is a proof obligation, not authorization to delete checks: current receipts, comparison outputs, error precedence, and evidence vocabulary may rely on those products. The G3 `stream_root` is useful existing evidence because it walks/authenticates the canonical references, emits bytes, and computes only the reconstructed output digest—without the separate benchmark closure/occurrence hashers (`crates/layerfs-engine/src/bin/phase4_g3_materialization.rs:1087-1117`). Round 2 should formalize the equivalence and then measure one change at a time.

Range reads already route only through intersecting cumulative extents and authenticate complete selected chunks before returning slices (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:8370-8537`). A second per-file Bao/outboard tree is therefore not needed for current 8–32-KiB chunks. Bao is a useful reference for verified streaming/slices when the payload itself is coarse-grained, but its combined/outboard encodings add another tree and proof representation ([Bao specification](https://github.com/oconnor663/bao/blob/master/docs/spec.md)).

### 2.4 COW, deltas, and history independence

Directory nodes are immutable `Arc` values, but each changed ancestor clones its containing `BTreeMap` (`crates/layerfs-core/src/cow/tree.rs:146-203`). A mutation rebuilds the changed ancestor path and then computes `Delta::between` (`crates/layerfs-core/src/cow/mutate.rs:53-66`); directory diff constructs the union of names in a `BTreeSet` and recursively descends changed identities (`crates/layerfs-core/src/delta/mod.rs:109-185`). File mapping uses deterministic fixed-ordinal grouping. It is history-independent but count changes can repack a suffix: measured 500-MiB early/middle `+1` rows were 27.140916/15.102042 ms same-open and still under 50 ms, while first-after-reopen remained about 1.23–1.26 s (`implementation-detail/phase-4/algorithm/complexity-analysis.md:2322-2391`). This does not justify a prolly-tree migration for G4; it identifies a future threshold if near-size-independent multi-GiB edits become an explicit SLA.

Noms documents the relevant trade: content-determined prolly-tree boundaries preserve history independence and local structural sharing for ordered collections ([Noms technical overview](https://github.com/attic-labs/noms/blob/master/doc/intro.md)). It is a valid future model, not a free replacement: new node grammar, boundary algorithm, adversarial-size bounds, migration, proof vectors, and all mapping roots would change.

### 2.5 Native materialization and projection today

G3-v13 creates a verified seed, syncs it, reopens it read-only/no-follow, rehashes it, unlinks the name, and retains the descriptor (`crates/layerfs-engine/src/bin/phase4_g3_materialization.rs:1303-1383`). Its permit binds store/profile/epoch/head/transition, parent and target roots, native destination/seed identities, open/mutation/publication serials, the exact selected range, and a canonical range commitment (`:274-424`, `:1632-1735`). A qualified operation uses `fclonefileat` (`:440-484`), patches authenticated target range bytes, syncs data and metadata, atomically renames, syncs the directory, and reconciles ambiguous outcomes; invalid or unsupported qualification uses a complete authenticated stream fallback (`:1954-2250`).

The retained row is impressive but narrow: one-byte patch of 100 MiB was `3.414166 ms`, with 22,551 canonical bytes authenticated, one clone, and one patched byte (`implementation-detail/phase-4/baseline/g3-incremental-materialization-baseline-v1.md:48-58`). The seed is only operation-local, read-only, unlinked, and same-open. It persists no cross-process authority and explicitly makes no malicious-same-UID, VFS, SDK, migration, or product claim (`:117-130`). v11's four defects and v12's five evidence-protocol defects show why native fast paths must keep exact Q, cleanup, error precedence, and locality proof contracts (`implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-V11-POST-SEAL-REAUDIT-DISPOSITION-v1.md:35-67`; `implementation-detail/phase-4/experiments/g3-incremental-materialization/v12/V12-PREEXEC-REVISE.md:8-38`).

There is no production VFS/projection implementation today: `layerfs-vfs` and `layerfs-sdk` contain only component constants (`crates/layerfs-vfs/src/lib.rs:1-5`; `crates/layerfs-sdk/src/lib.rs:1-5`). `layerfs-os` is an environment-observation boundary, not a materializer (`crates/layerfs-os/src/lib.rs:1-94`). The engine manifest exposes only the private Phase-4 benchmark binary and disables auto-bins (`crates/layerfs-engine/Cargo.toml:1-22`). Any persistent cache or VFS claim is therefore new product architecture.

## 3. Measured constraints and honest ceilings

| Constraint | Retained observation | Architectural consequence |
|---|---:|---|
| 100-MiB durable full create | 308.884052 ms / 323.746076 MiB/s | Any shared storage candidate must stay within 324.328255 ms at the 5% guard in a fresh adjacent trial. |
| Writer RSS / SQLite cache max | 12.48 MiB / 8.35 MiB | Keep streaming memory bounded; do not restore a full-file or 80-MiB SQLite cache. |
| Warm complete reconstruction | 338.775916 ms | Current shared read path misses the <300-ms desired threshold. |
| Fresh-process reconstruction | 366.356667 ms, OS cache warm-or-unknown | Not a physically cold result; do not relabel it. |
| 1-MiB authenticated range | 2.279209 ms / 438.748706 MiB/s | Current extent/chunk granularity is already effective for ranges. |
| Reopen/head | 2.088334 ms | Opening the catalog is not the problem. |
| First edit after reopen | 154.019083 ms | Canonical mapping/edit authority work remains. Native seed custody can accelerate a later read/clone/materialization step but does not eliminate or prove this edit path. |
| G3 one-byte native patch | 3.414166 ms, once-only mechanism screen | A protected content seed can satisfy <50 ms; persistent authority is unproven. |
| 100-MiB logical full-create work | 5,284 refs; 5,372 objects; 105,122,466 canonical bytes; 196,174 mapping bytes; 5,381 SQL calls | Cardinality and row crossing matter; data volume remains approximately the source size. |
| Accepted physical store endpoint | 109,199,392 apparent/logical; 117,510,144 allocated bytes | New steady-state storage must be compared with apparent **and** allocated bytes. |

Sources: `implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md:14-27`; `implementation-detail/phase-4/baseline/sqlite-writer-memory-cache-spill-2000-baseline-v1.md:45-76`, `:94-129`; `implementation-detail/phase-4/test-checkpoint-report/cp-0010-dirty-72ed9fee8e6a-fastcdc-v2-phase4-grind.md:139-179`.

G2-v5 decomposed every timed one-pass 100-MiB reconstruction row into **disjoint** direct components: canonical authentication, closure commitment, source/output fingerprint, SQLite BLOB acquisition, occurrence commitment, topology validation, secondary decode, plus a raw residual. Within each individual row, the direct component timers plus that row's residual exactly equal that row's parent. The published `94.817 ms`, `88.483 ms`, `87.890 ms`, and `59.404 ms` values are component-wise medians across measured rows; because the median cell for each component can come from a different row, those median cells must not be added to manufacture a synthetic median parent (`implementation-detail/phase-4/experiments/g2-materialization-decomposition/G2-REVISE-REPORT-v1.md:149-176`; `implementation-detail/phase-4/2026-08-21-phase-4-full-grind.md:142-160`). Each component median remains a candidate-specific ceiling. For example, even an impossible zero-cost replacement for the entire `59.404 ms` acquisition component would only move the historical `338.776` parent reference to `279.372 ms`; it can plausibly clear 300 ms but cannot explain a trusted <50-ms warm path. The latter requires a different authority boundary such as a protected native seed or lazy VFS open.

## 4. Target end-state invariants

1. **One canonical truth.** Object IDs and Canonical-v2 roots remain the only logical content authority. Segment locations, SQLite rowids, native paths, inodes, cache recency, and clone lineage are rebuildable physical metadata.
2. **No unauthenticated bytes.** Every canonical frame consumed from a value plane is bounded, decoded, and authenticated against the expected Object ID before its raw payload influences output. A cached full-file seed is usable without full rehash only if its custody/receipt authority excludes mutation in the relevant threat model.
3. **No per-version full-file duplication.** CAS storage grows by new canonical objects, not revision count. Native cache keys use content/file-root identity rather than revision/head identity and are capacity-bounded.
4. **Fallback is complete and exact.** Unsupported clone, cross-volume destination, invalid cache receipt, changed object count, count-changing edit, corruption, or lost authority falls back to complete authenticated streaming; it never trusts the mutable destination.
5. **Crash-safe old-or-new publication.** Payload durability precedes catalog/head publication; native temp data and metadata durability precede rename; directory sync follows rename; ambiguous outcomes are reconciled without masking the original error.
6. **Bounded memory.** Reconstruction/VFS keeps at most a bounded segment read window, one maximum canonical object, mapping path/batch metadata, and bounded output. No full 100-MiB buffer.
7. **History independence.** Equal logical content has the same canonical graph independent of ingest/edit history. GC and compaction may change physical layout without changing any canonical ID.
8. **Cross-platform semantic equivalence.** APFS clone, Linux reflink, and Windows ReFS block clone are optional accelerators. The portable contract is authenticated stream + durable atomic replacement.

The immutable/content-addressed split is well established: Venti uses hashes as write-once block identifiers and coalesces duplicates ([USENIX Venti paper page](https://www.usenix.org/conference/fast-02/venti-new-approach-archival-data-storage)); Nix explicitly treats store objects as immutable and requires their closure to exist, while allowing deterministic lazy loading ([Nix store object specification](https://releases.nixos.org/nix/nix-2.31.0/manual/store/store-object.html)). Those systems support the invariant, not the performance projection.

### 4.1 Shared G4 engine boundary versus later VFS/SDK exposure

G4 should expose one narrow **shared engine primitive**, conceptually `materialize_verified_file(expected_root, canonical_path, sink) -> VerifiedFileSummary`, rather than keep the verified stream inside the benchmark. The exact Rust surface should follow repository conventions, but its contract must:

- pin one catalog/head generation and resolve the file root from the caller's exact expected namespace root/path;
- authenticate every mapping/chunk object and validate the complete Canonical-v2 extent/summary contract before or as its corresponding bytes reach the sink;
- never expose an “authenticated” flag for bytes that have not passed Object-ID and canonical grammar checks;
- return a typed summary binding expected namespace/file root, profile, length, reference count, output digest if requested, and the pinned generation;
- preserve partial-write error provenance. A sink that received a verified prefix after a later missing/corrupt object has **not** earned publication authority; the caller must discard its private temp;
- contain no destination pathname trust, clone qualification, rename, fsync, or directory-sync policy. A separate OS/native publisher consumes only a successful verified summary and owns the old-or-new protocol; and
- support a no-output/hash-only sink for reconstruction/scrub measurement without introducing benchmark-only validation behavior.

This shared primitive is a valid G4 production-shaped boundary even though `layerfs-vfs` and `layerfs-sdk` remain stubs (`crates/layerfs-vfs/src/lib.rs:1-5`; `crates/layerfs-sdk/src/lib.rs:1-5`). Public SDK/VFS exposure, page-cache policy, native publication API, and cross-platform adapters remain later integration. G3's existing `stream_root(..., output: &mut impl Write, ...)` demonstrates the implementation shape but is benchmark-private and returns no reusable engine type (`crates/layerfs-engine/src/bin/phase4_g3_materialization.rs:1087-1117`).

## 5. Candidate matrix

Each candidate below fills the shared-ledger row fields explicitly. Predicted gains are hypotheses or upper bounds, never promotion claims.

### C1 — Canonical-v2 verified-stream contract; remove/fuse redundant evidence products

- **Mechanism:** Keep the accepted reconstruction's existing single mapping/chunk traversal, authoritative expected namespace/file root, per-object authentication, Canonical-v2 shape/extents/length checks, and byte emission. Specify exactly which closure, ordered-occurrence, and reconstructed-source proof products are derivable/redundant; delete one product per experiment or fuse retained product updates inside that same traversal. Preserve an optional end-to-end raw output digest when the caller explicitly needs it. This candidate removes/fuses proof work; it does not introduce a second fast route or reduce “multiple traversals” that the accepted benchmark does not have.
- **Target paths:** A shared engine `materialize_verified_file(..., sink)` boundary used by reconstruction, first/fallback materialization, full sequential warm reads, and scrub where semantics permit. Native publication and public VFS/SDK wrappers remain separate. No change to range traversal, CDC, COW, canonical encoding, Object IDs, or SQLite layout.
- **Complexity:** Remains `Theta(S + N + M)` for `S` raw bytes, `N` chunk occurrences, and `M` mapping nodes; the accepted path already performs one traversal and one Object-ID authentication per reached object. C1 reduces constant proof-hash/update work, not traversal count or asymptotic work.
- **Measured ceiling:** G2 closure direct-component median `88.483 ms`; source/output fingerprint direct-component median `87.890 ms`; occurrence commitment median `0.409 ms`. Components are disjoint within each row and reconcile with that row's parent through its residual. Component-wise medians may originate in different rows and therefore cannot be added into a synthetic median parent (`implementation-detail/phase-4/experiments/g2-materialization-decomposition/G2-REVISE-REPORT-v1.md:149-176`).
- **Predicted gain:** Plausible `20–88 ms` on 100-MiB reconstruction if the closure product is formally removable. The `88.483-ms` direct-component median is an upper-bound sizing reference, not an additive stack with the other component medians; an illustrative full-ceiling subtraction from the historical `338.776-ms` parent gives `250.293 ms`, but only an adjacent A/B can establish the realized wall change and any residual movement. No <50-ms claim.
- **CPU:** Lower BLAKE3/update/length-framing work; per-object canonical authentication remains mandatory. Raw output digest is retained only when required by the API/receipt.
- **Memory/Q:** Same bounded buffers and mapping frames; possibly fewer hasher states. Terminal Q must remain zero and G3-v11's reconciliation buffer lesson still applies.
- **Storage:** No format or steady-state storage change.
- **Authority:** Exact expected root plus per-object identity authentication is the authority. A proof must show the removed product adds no binding, and receipt/error semantics must be versioned if their field set changes.
- **Durability:** No SQLite/native ordering change. Materialization publication retains data sync, metadata sync, rename, directory sync, and reconciliation.
- **Identity/format:** Canonical-v2/Object IDs unchanged. Receipt/evidence schema may require a new version; no reuse of old receipt bytes under new semantics.
- **Cross-operation effect:** Establishes one shared verified-stream implementation for reconstruction and fallback first materialization and may reduce scrub. It prevents benchmark/product validation drift. It does not help create directly, ranges materially, persistent warm cache hits, or reopen/head.
- **Experiment:** Under benchmark lock, adjacent balanced A/B on 1/10/100 MiB: baseline versus one removed closure product only. Require identical roots, object IDs, output bytes/mode, selected errors, authority decisions, SQL/object/auth counts, publication state, Q, RSS, storage, and create/edit/range/reopen guards. Include corruption/missing-object/wrong-role/length/cycle cases and independent proof review. Falsifier: <5% wall gain, changed error precedence, or any loss of negative-case detection.
- **Evidence:** Canonical-v2 commitments (`crates/layerfs-core/src/canonical_v2.rs:72-255`); current extra products (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:2150-2169`, `:8238-8368`, `:9058-9177`); G3's smaller verified stream (`crates/layerfs-engine/src/bin/phase4_g3_materialization.rs:1087-1117`); G2-v5 hashes in Appendix A.
- **Disposition:** **PROMOTE TO ROUND 2, rank 1.** Shared improvement if and only if the authority proof closes; reject any benchmark-only bypass.

### C2 — One-pass public object authentication/read API

- **Mechanism:** Replace the public engine's hash pass + grammar pass + selected-read pass with a single bounded authenticated decoder/tee for full loads; for ranges, authenticate once while retaining only selected bytes. Avoid `load_object` revalidating an already authenticated owned buffer. Existing-object comparison should authenticate/hash the incumbent in one pass without a full duplicate `Vec` where possible.
- **Target paths:** Public `Engine::load_object`, `read_object_range`, reused `put_object`; future SDK/VFS consumers. It does not alter the private Phase-4 benchmark store.
- **Complexity:** Full load remains `Theta(L)` but falls from multiple full passes to one; range under the current all-object public API remains `Theta(L)` authentication plus `Theta(R)` retained bytes unless the caller has an independently authenticated chunk/extent root.
- **Measured ceiling:** No accepted G4 wall attribution. Static implementation count is exact: two complete BLOB passes in `authenticate_blob`, then the selected read; `load_object` adds owned object construction (`crates/layerfs-engine/src/lib.rs:377-381`, `:912-1017`).
- **Predicted gain:** Up to roughly one-to-two canonical-object passes for this API; negligible for small mapping objects and potentially material for large BLOB callers. Zero defensible projection for the current 338.776-ms benchmark until the path is benchmarked.
- **CPU:** Fewer hashes/decodes/copies; same cryptographic coverage.
- **Memory/Q:** Full load still owns the requested canonical value; range owns only requested bytes plus bounded decoder buffer. Reused-put can avoid a second full incumbent allocation.
- **Storage:** None.
- **Authority:** Identical expected Object ID and canonical grammar; expose a typed “authenticated object/range” result so callers cannot accidentally treat unauthenticated bytes as verified.
- **Durability:** None; read-only change. Put conflict behavior and immutable-conflict precedence must remain exact.
- **Identity/format:** None.
- **Cross-operation effect:** Shared SDK/VFS/read improvement; no direct Phase-4 create or G3 fast-path benefit.
- **Experiment:** Unit corruption matrix plus an isolated public-API microbenchmark over canonical objects spanning tiny mapping nodes and 8/16/32-KiB chunks; compare byte-read/hash/decode/copy counters before wall. Falsifier: any weakened malformed-object or immutable-conflict behavior.
- **Evidence:** `crates/layerfs-engine/src/lib.rs:850-909`, `:912-1017`; SQLite's incremental BLOB API supports small subsection reads but ties a handle to a particular row and expires it when the row changes ([SQLite `sqlite3_blob_open`](https://www.sqlite.org/c3ref/blob_open.html), [SQLite `sqlite3_blob_read`](https://sqlite.org/c3ref/blob_read.html)).
- **Disposition:** **PROMOTE AS SHARED CODE-QUALITY FOLLOW-UP, not a G4 acceptance candidate by itself.**

### C3 — Ordered storage ladder: SQLite-resident extent BLOBs, then optional external segments

- **Mechanism:** **Stage A first:** keep the accepted single SQLite file as the only durable authority, but group canonical objects into bounded immutable authenticated extent BLOB rows. A SQLite catalog maps `(object_id, kind, canonical_len)` to `(extent_rowid, offset, frame_len)`; roots, receipts, extent bytes, locations, and visible head publish in the same SQLite transaction/profile. Batch-resolve leaf IDs, use incremental BLOB reads for selected bounded spans, and authenticate every extracted canonical object against its expected Object ID. **Stage B only if Stage A is insufficient:** place the same immutable framed extents in external segment files while SQLite retains the sole committed catalog/head authority. An external frame without a committed SQLite locator is an orphan, never an independently discoverable or second durable truth.
- **Target paths:** Post-G4 research for full create insertion, reconstruction, fallback materialization, warm reads, ranges, reopen, and storage maintenance. COW and canonical mapping stay unchanged. Neither Stage A nor Stage B is a G4 do-now path.
- **Complexity:** Both stages keep `Theta(S)` canonical reads/authentication. Stage A makes `O(M+B)` catalog batches and `O(K)` bounded SQLite BLOB span reads, with `K <= N`; new extent rows write `Theta(new canonical bytes)`. Stage B replaces BLOB span reads with coalesced `pread` spans and adds segment generation/lease maintenance. Compaction/GC is `Theta(live bytes selected)` and is charged to the foreground request if synchronous or to separately measured maintenance work if asynchronous.
- **Measured ceiling:** SQLite BLOB acquisition direct-component median `59.404 ms`; current warm reconstruction crosses about 170 queries and 5,371 object rows (`implementation-detail/phase-4/test-checkpoint-report/cp-0010-dirty-72ed9fee8e6a-fastcdc-v2-phase4-grind.md:201-215`). Historical custom external carrier is a negative control: 55,240 index reads for 5,363 lookups (~10.3/lookup) and ~4.02x reopen reread (`implementation-detail/phase-4/storage/append-only/first-implementation-findings.md:161-190`).
- **Predicted gain:** Stage A has at most the `59.404-ms` acquisition component as a sizing ceiling; realized wall, residual movement, and create cost are unknown. Stage B has no independent gain entitlement beyond an observed Stage-A shortfall. The illustrative zero-cost acquisition subtraction gives 279.372 ms, not a prediction. No positive create, <300-ms, or <50-ms claim exists until an adjacent experiment.
- **CPU:** Stage A may reduce per-row SQLite VM/B-tree crossings but adds extent-frame parsing and copied-slice authentication. Stage B may reduce SQLite payload handling but adds system-call/frame/checksum/GC work. Object-ID and grammar validation remain. Compaction, repair, migration, and recovery CPU are maintenance or foreground charges, never free background work.
- **Memory/Q:** Bound each extent BLOB and read window prospectively (for example, a tested 256-KiB–1-MiB class, not a committed format value), plus one maximum canonical object and mapping/batch metadata; target <20 MiB RSS/Q. Builders, migration, compaction, repair, and recovery buffers count in their owning foreground/maintenance row, with concurrency caps and terminal Q zero.
- **Storage:** Stage A stays in one SQLite database and adds extent framing, possible padding/slack, and locator rows; it must beat the current 109,199,392 apparent / 117,510,144 allocated endpoints within the 5% guard. Stage B adds segment headers/footer and may temporarily retain SQLite/old/new segment generations plus orphan tail during migration/GC. Report steady-state and temporary apparent/allocated high-water separately. History grows by new canonical objects, not full revision copies.
- **Authority:** Stage A's SQLite transaction is the sole durable authority; an extent locator is untrusted until expected Object ID, kind, length, bounds, and canonical grammar validate. In Stage B, SQLite remains the sole committed locator/head authority; segment scanning must not resurrect uncommitted frames into truth. Duplicate physical locations require deterministic catalog policy and authentication.
- **Durability:** Stage A uses the current one-file `FULL`+`DELETE` SQLite transaction boundary for extent bytes, locations, receipts, and head—this is why it must be tested first. Stage B is allowed only after Stage A is insufficient and then requires append complete frames -> sync segment -> SQLite catalog/head transaction -> reconcile ambiguous commit; unsynced or uncommitted frames cannot be referenced. SQLite's rollback-journal protocol depends on journal sync, database sync, and commit-by-journal deletion ([SQLite atomic commit](https://www.sqlite.org/atomiccommit.html)); `synchronous=FULL` semantics remain explicit ([SQLite synchronous pragma](https://www.sqlite.org/pragma.html#pragma_synchronous)).
- **Identity/format:** New **physical storage/schema profile**, not a canonical-object version. Stage A migration builds extent rows/catalog under a new SQLite generation and atomically flips the format marker after authentication. Stage B, if ever justified, is a later physical generation. Preserve the old store as rollback source until verification; no destructive in-place reinterpretation.
- **Cross-operation effect:** Stage A could improve reconstruction/fallback/ranges by reducing object-row crossings while preserving one-file authority; create may regress from extent assembly. Stage B is only a response to measured Stage-A insufficiency. Neither changes canonical edit authority, trusted native-cache authority, or the current G4 materialization candidate.
- **Experiment:** Not G4 do-now. If opened later, first build a disposable **SQLite-resident** bounded extent-BLOB lower bound over sealed 1/10/100-MiB snapshots; compare identical traversal/authentication/output using exact extent/object/query/BLOB-read/bytes/CPU/RSS/Q/apparent/allocated counters and current one-file durability. Falsifiers include <10% reconstruction gain, >5% create/storage/RSS regression, or range harm. Only if Stage A is insufficient may a separately preregistered Stage-B external-segment lower bound run; it must prove SQLite-only committed truth and all two-file crash/GC states.
- **Evidence:** Current schema/profile (`crates/layerfs-engine/src/lib.rs:683-747`; benchmark `:2364-2401`); SQLite incremental BLOB API in C2; G2 ceiling; rejected carrier evidence; local SQLite architecture report (`research/phase-4/storage/sqlite/durability-and-layout.md:198-321`). WiscKey shows that external key/value separation imports GC and crash-consistency work ([WiscKey, FAST'16](https://www.usenix.org/conference/fast16/technical-sessions/presentation/lu)); Git's replaceable object-to-pack indexes are only a later Stage-B analogy ([Git multi-pack-index design](https://git-scm.com/docs/multi-pack-index)). Neither source proves LayerFS performance.
- **Disposition:** **DEFER FROM G4 DO-NOW. Retain as an ordered post-G4 research ladder: Stage A SQLite-resident extents first; Stage B external segments only on measured insufficiency and never as a second durable truth.**

### C4 — Bounded content-keyed verified native seed cache

- **Mechanism:** After a complete authenticated reconstruction, durably publish an immutable cache file keyed by `(storage/profile version, file_root)` rather than namespace revision/head. A privileged daemon, private mount/namespace, or equivalent custody owns mutation; clients receive read-only descriptors plus signed/MACed cache receipts bound to root, length, generation, native identity, and integrity epoch. Materialization clones/reflinks the seed to a private temp, authenticates only proven changed extents, patches, syncs, and publishes; otherwise complete fallback.
- **Target paths:** Warm/trusted native read/clone, repeated checkout, and the payload-copy portion of incremental materialization **after** a separate canonical edit/mapping authority has already produced and proved the target root and changed range. First-ever materialization populates the cache; pure logical reconstruction can bypass it. C4 does not target canonical edit construction, mapping authority establishment, or the first-edit-after-reopen authorization path.
- **Complexity:** Cache hit is `O(1)` metadata/authority plus clone metadata and `O(changed authenticated bytes)` patch; fallback remains `Theta(S+N+M)`. Eviction is `O(log E)` or amortized `O(1)` per entry with a simple SQLite LRU/clock catalog. No per-revision scan.
- **Measured ceiling:** G3 one-byte 100-MiB materialization operation `3.414166 ms`; no-op 10-MiB materialization `0.993791 ms`; these are single APFS mechanism rows, not medians/cold I/O (`implementation-detail/phase-4/baseline/g3-incremental-materialization-baseline-v1.md:42-65`). First edit after reopen remains a separate `154.019-ms` canonical authority lifecycle guard and is not C4's measured or predicted saving (`implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md:24-27`).
- **Predicted gain:** A correctly custodied 100-MiB cache hit plausibly meets <50 ms for qualified no-op or small same-size **native materialization** on supported same-volume clone filesystems, after target-root/range authority is supplied independently. It makes no prediction for the ~154-ms first edit after reopen; native seed authority does not prove canonical mapping/edit authority. First-ever materialization remains near the complete stream/write cost and may increase due to cache admission unless admission and destination share a safely cloned durable seed.
- **CPU:** A qualified live-descriptor hit avoids full seed rehash and pays bounded receipt/native validation plus independently authorized changed-extents work. A miss/restart performs complete authentication or rebuild. Eviction, repair, quarantine validation, full revalidation, and rebuild consume explicitly attributed foreground CPU/I/O when on the request path or separately measured maintenance CPU/I/O when scheduled asynchronously; neither category is free background work.
- **Memory/Q:** G3 demonstrates bounded range buffers; retain <20 MiB per admitted operation. Cache catalog metadata is small, but concurrent admission, full revalidation/rebuild, repair, and eviction/lease-drain operations own their buffers and RSS/Q in the foreground or maintenance row. Bound their concurrency, record high-water and terminal Q, and never hide a full-file allocation in maintenance.
- **Storage:** Hard cap by **allocated bytes**, not apparent length. Illustrative `C=2 GiB`: ten distinct 100-MiB roots consume about 1,000 MiB apparent before filesystem sharing; 100 or 1,000 roots plateau at 2 GiB after eviction. Without a cap they would consume ~1/10/100 GiB respectively and are unacceptable. Unchanged content across revisions reuses one `file_root` entry. Admission temp files, quarantined corrupt entries, old/new files during repair, clone COW allocation, and eviction lag can transiently exceed steady state; reserve space and report apparent/allocated high-water plus post-maintenance endpoints.
- **Authority:** C4 authority covers only the exact cached seed bytes and their use as a native read/clone source. It does not construct or prove a new canonical mapping, edit, target root, or changed-range relation; those inputs must come from the canonical engine's separate authenticated edit/mapping contract. Ordinary user-editable cache files are not trusted. G3 proved why: exact current-byte authority otherwise requires full reread without exclusive mutation service/persisted authority/gap-free journal (`implementation-detail/phase-4/baseline/g3-incremental-materialization-baseline-v1.md:20-31`). **No primitive in the current repository retains exact seed-byte authority across clients or a broker/service restart.** Root, receipt, pathname, inode, mode, timestamps, and a reopened read-only descriptor do not prove the current bytes. Therefore a current-product restart/cache miss must fully reauthenticate canonical bytes or discard and rebuild the seed. A future no-rehash restart hit is conditional on a separately proven protection-domain primitive—such as a distinct-UID service-private immutable/read-only namespace or OS-verified/sealed file—in which malicious same-UID clients cannot mutate the stored bytes. Until that exists, persistent <50-ms authority is unproven. On every admission, authenticate complete canonical reconstruction and output digest; on same-lifetime use, validate receipt/generation/descriptor identity/RO state. On restart, corruption, or authority mismatch, quarantine/remove and rebuild.
- **Durability:** Cache is derived and may be discarded after a crash. Admission still uses temp -> data sync -> metadata -> sync -> rename -> directory sync; catalog record follows durable publication or is reconciled on reopen. User destination follows the G3 old-or-new protocol. Never let cache-catalog success substitute for destination durability.
- **Identity/format:** No canonical change. New cache receipt/catalog schema, independently versioned. `(profile,file_root)` avoids per-version duplication; mode changes can conservatively use distinct roots.
- **Cross-operation effect:** Large potential win for repeated native materialization and the clone/copy portion of same-size incremental materialization once canonical target/range authority exists independently. It does **not** accelerate or authorize canonical edit construction and does not claim to reduce first-edit-after-reopen wall. It costs cache and maintenance resources and offers little for server-side logical reconstruction/ranges unless VFS consumes cached native bytes under the same proven seed authority.
- **Experiment:** First write a threat/authority matrix and portable cache contract. Then, under G4 custody, adjacent A/B at 1/10/100 MiB for cache hit/no-op/1-byte/1-MiB materialization, count-change fallback, invalid receipt, external mutation, cross-volume, unsupported clone, lost-ack, restart revalidation/rebuild, repair/quarantine, and capacity eviction. Hold canonical target-root/range inputs identical and outside the candidate; preserve the 154-ms first-edit-after-reopen guard without crediting C4. Attribute foreground and maintenance CPU/RSS/Q/I/O/apparent/allocated high-water separately. Falsifiers: any ordinary mutable path treated as trusted, >50-ms qualified 100-MiB materialization median, fallback mismatch, unbounded cache/maintenance, hidden resource work, or create/edit guard >5% regression.
- **Evidence:** G3 source/rows/repair history above. Apple `fclonefileat` is an optional same-filesystem copy-on-write primitive ([XNU `clonefile(2)` manual](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/clonefile.2)); Linux `FICLONE/FICLONERANGE` shares extents only within the same filesystem and may reject unsupported/alignment cases ([Linux `ioctl_ficlonerange(2)`](https://man7.org/linux/man-pages/man2/ioctl_ficlonerange.2.html)); Windows ReFS block clone is same-volume and has alignment/size/platform restrictions ([Microsoft ReFS block cloning](https://learn.microsoft.com/en-us/windows-server/storage/refs/block-cloning)).
- **Disposition:** **PROMOTE TO ROUND 2, rank 2, for native materialization authority/resource design before timing; explicitly exclude canonical first-edit authority.**

### C5 — Direct VFS streaming over Canonical-v2/value plane

- **Mechanism:** Build VFS handles later on the shared G4 verified-stream/range engine boundary. Immutable handles pin an expected root/generation. `open` authenticates the small namespace/file-root path; `read(offset,len)` routes through cumulative extents, batch-resolves locations, authenticates complete selected mapping/chunk objects, and returns only selected bytes. Sequential read uses read-ahead/coalescing; `mmap` is initially unsupported or uses a page cache with explicit verified-page state. Writes go through COW/new roots, never mutate canonical payloads.
- **Target paths:** Virtual projection, warm reads, ranges, reopen; it can avoid eagerly materializing a native copy when consumers only read a fraction.
- **Complexity:** Open `O(tree depth)`; range `O(log_f N + selected chunks + R)`; full sequential read `Theta(S+N+M)`. Ten thousand files still imply `Theta(J)` metadata/open work for `J` files; no magic namespace constant time.
- **Measured ceiling:** Existing authenticated returned 1-MiB range is `2.279209 ms` at 100-MiB root; reopen/head `2.088334 ms` (`implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md:25-27`). VFS itself is unimplemented (`crates/layerfs-vfs/src/lib.rs:1-5`).
- **Predicted gain:** Open/setup can plausibly remain <50 ms; partial workloads avoid `S-R` reads and all destination write/fsync. A consumer that reads all 100 MiB still pays complete canonical acquisition/authentication and has no <300-ms proof from C5; C1 is the current candidate, while any C3 layout result is deferred and evidence-conditional.
- **CPU:** Proportional to selected canonical objects; read-ahead reduces syscall/query overhead but not hashing.
- **Memory/Q:** Verified page/read-ahead cache bounded by policy (e.g. 1–8 MiB per active stream plus global cap), one mapping path, and selected output. Backpressure required.
- **Storage:** No native duplicate by default; bounded page cache only. Optional native seed cache remains C4, not hidden VFS state.
- **Authority:** Handle pins expected root and catalog/segment generation. Each page/chunk has an authenticated state tied to Object ID. Invalidation changes head for new opens but never mutates bytes visible to a pinned immutable handle.
- **Durability:** Read-only projection has no publication. Writable overlay commits canonical objects then head under the normal transaction protocol; dirty page cache cannot masquerade as committed canonical data.
- **Identity/format:** Current Canonical-v2 suffices. VFS API/handle/receipt schema is new; no object format.
- **Cross-operation effect:** Strong for range-heavy and partial-read workloads; avoids materialization storage. Does not replace durable checkout when native paths are required and does not accelerate create alone.
- **Experiment:** Userspace projection prototype over the existing store, with deterministic 4-KiB random, 1-MiB range, full sequential, 10k-file metadata, reopen, corruption, and concurrent-head-change traces. Compare object/query/read bytes and wall; no kernel integration initially. Falsifier: range regression >5%, unbounded page cache, stale-head byte mixing, or all-file path slower without a compensating partial-read use case.
- **Evidence:** Current range router (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:8370-8537`); Linux documents FUSE as a maintainable userspace alternative before adding a new in-kernel filesystem ([Linux new-filesystem guidance](https://www.kernel.org/doc/html/latest/filesystems/adding-new-filesystems.html)).
- **Disposition:** **PROMOTE TO ROUND 2, rank 4, as a shared projection design/prototype; outside G4 native acceptance unless the user-visible operation is explicitly virtual read.**

### C6 — New large extent/macrosegment object with Merkle/outboard proofs

- **Mechanism:** Group many current chunks into larger immutable segment objects and attach a versioned Merkle/outboard index so ranges authenticate selected sub-extents without hashing the entire macrosegment.
- **Target paths:** Sequential reconstruction, ranges, segment value plane, remote transfer.
- **Complexity:** Full `Theta(S)`; range `Theta(R + log S/proof_granularity)`; updates rewrite changed proof paths and any boundary-affected macrosegments.
- **Measured ceiling:** No local evidence that mapping metadata is limiting: canonical-v2 mapping is only 196,174 bytes for 100 MiB and 1-MiB ranges already take 2.279 ms (`implementation-detail/phase-4/baseline/canonical-v2-baseline-v1.md:103-132`).
- **Predicted gain:** Unknown; may reduce object/catalog cardinality but adds proof bytes/decoding and larger failure/rewrite units. It risks worsening small ranges, CDC reuse, and edits.
- **CPU:** Fewer object lookups but extra proof hashing. Full stream may hash similar total bytes.
- **Memory/Q:** At least one proof path and bounded segment window; must not buffer a full macrosegment.
- **Storage:** New outboard/index bytes; for a binary tree, internal-hash overhead is approximately one hash per proof leaf, so 32-byte hashes at 64-KiB leaves are ~0.049% before framing, but exact design/canonical overhead is unmeasured. If current 16-KiB average chunks remain proof leaves, it duplicates existing Object IDs.
- **Authority:** Expected macrosegment root plus verified proof. Must define malleability, length binding, duplicate leaf handling, and canonical proof encoding; never trust segment offset alone.
- **Durability:** New immutable payload/index publication and GC pairing; both must be durable before catalog/head reference.
- **Identity/format:** New canonical object/profile and migration unless purely physical/outboard. A purely physical outboard cache must be rebuildable and never authoritative.
- **Cross-operation effect:** Potential sequential/read-cardinality win; likely edit/range/compatibility cost. Does not solve trusted native cache.
- **Experiment:** Only after a later C3A SQLite-extent result shows object cardinality remains dominant. Simulate 64/256-KiB/1-MiB proof leaves on exact retained chunk sequences; measure proof bytes, selected canonical bytes, changed bytes, object reads, range amplification, and GC. Falsifier: <10% acquisition reduction, >5% range/edit/storage regression, or duplicated trust tree.
- **Evidence:** Current authenticated extent map above; Bao provides a primary verified-stream/slice encoding reference, not a reason to duplicate the tree ([Bao spec](https://github.com/oconnor663/bao/blob/master/docs/spec.md)).
- **Disposition:** **DEFER. Current Canonical-v2 chunk IDs already provide finer authenticated granularity.**

### C7 — History-independent prolly-tree file mapping

- **Mechanism:** Replace fixed-ordinal mapping pages with content-determined boundaries over canonical `(raw_length, object_id)` entries at every mapping level, preserving deterministic history-independent shape while localizing count-changing inserts/deletes.
- **Target paths:** Count-changing edits, mapping history, diff/sync; neutral or slightly worse full create/read.
- **Complexity:** Expected changed mapping `O(log N + local boundary resynchronization)` rather than worst-case suffix `Theta(N-p)`; worst-case/adversarial bounds require explicit min/max page rules.
- **Measured ceiling:** Current 500-MiB early/middle `+1` same-open rows are 27.140916/15.102042 ms and below 50 ms; current product authority explicitly accepts suffix-linear behavior (`implementation-detail/phase-4/algorithm/complexity-analysis.md:2322-2391`).
- **Predicted gain:** Large only at multi-GiB count changes or a near-size-independent SLA; little/no user-visible G4 benefit at current 1/10/100-MiB matrix.
- **CPU:** New rolling/entry hash and boundary calculation; lower suffix serialization for count changes.
- **Memory/Q:** Bounded page builders and mapping spine; comparable if min/max pages are enforced.
- **Storage:** Less rewritten mapping history on count changes; baseline mapping is already only ~0.187% of 105,122,466 canonical bytes at 100 MiB, so steady-state absolute savings are small for full create.
- **Authority:** Canonical boundary algorithm, entry encoding, min/max, hash, seed, and adversarial behavior must be frozen and test-vector verified.
- **Durability:** Same CAS/catalog publication; migration temporarily retains both mapping graphs.
- **Identity/format:** New mapping version/profile and all file roots change. Read-only compatibility and explicit migration required.
- **Cross-operation effect:** Helps count-changing edits/history/diff; risks create, full read, range, proof, and interoperability.
- **Experiment:** Deterministic simulator over retained exact chunk sequences and 10/100/1,000 edits (early/middle/late insert/delete), with canonical bytes rewritten, node count, depth, range path, adversarial page-size distribution. Only benchmark if simulator crosses an agreed SLA threshold.
- **Evidence:** Local fixed-radix model (`research/phase-4/core/cow/mapping-and-deltas.md:1-183`); Noms primary design describes history-independent content-determined prolly trees and expected localized mutation ([Noms technical overview](https://github.com/attic-labs/noms/blob/master/doc/intro.md)).
- **Disposition:** **DEFER until a stricter multi-GiB/count-change SLA is adopted.**

### C8 — New larger CDC content profile

- **Mechanism:** Increase target/max chunk sizes to reduce object count and catalog crossings, with a new frozen CDC/content profile and new mapping/profile binding.
- **Target paths:** Full create, full reconstruction, value-log catalog. Risks ranges, small edits, dedup, and G3 changed-range reads.
- **Complexity:** Still `Theta(S)` scanning; object count roughly inversely proportional to average chunk size. Edit/range amplification grows with chunk size.
- **Measured ceiling:** 5,284 chunks and a `59.404-ms` gross acquisition family at 100 MiB; ranges already 2.279 ms. FastCDC exact hot loop is already retained at the current 8/16/32-KiB profile (`crates/layerfs-core/src/cdc/mod.rs:11-75`; `implementation-detail/phase-4/baseline/fastcdc-contiguous-region-kernel-v2-baseline-v1.md:55-114`).
- **Predicted gain:** Potentially fewer catalog/object operations, but no safe wall projection without a corpus. It cannot remove `Theta(S)` hashing/writing and may consume the range/edit budget.
- **CPU:** Similar byte scan; fewer per-object hashes/encodes/SQL binds, larger hash calls.
- **Memory/Q:** Maximum chunk buffer grows with profile; must stay under the accepted memory envelope.
- **Storage:** Less mapping/framing; worse changed-chunk and dedup amplification. Current mapping/framing savings ceiling is small relative to ~105 MiB payload.
- **Authority:** Exact CDC min/target/max/normalization/seed/gear/version must be profile-bound. Current `selected_mapping_profile_id` omits these inputs, so silent replacement is forbidden.
- **Durability:** Same publication, but migration/dual-profile storage can duplicate many objects until GC.
- **Identity/format:** New content/chunk profile and new roots. Existing roots remain readable; no rechunk-on-read. Migration only by explicit reconstruction/reingest.
- **Cross-operation effect:** Possible full create/read win; likely range/edit/dedup regression; native cache keys must include profile.
- **Experiment:** Corpus simulator first: exact representative file classes, 8/16/32 control versus 32/64/128 and 64/256/1024 KiB candidates; record chunk count/distribution, duplicate reuse, one-byte/insert locality, range amplification, canonical/mapping bytes, and CPU. Benchmark one predeclared profile only if simulated net benefit is material and no guard regresses.
- **Evidence:** Current code and G0 retained evidence; local CDC report (`research/phase-4/core/cdc/locality-and-algorithms.md:1-205`).
- **Disposition:** **DEFER; require a new versioned profile and representative corpus.**

### C9 — Per-chunk loose files plus reflink assembly

- **Mechanism:** Store each canonical chunk payload in a separate immutable native file and assemble destinations by cloning/reflinking each extent into place, falling back to copy.
- **Target paths:** Native materialization, ranges, CAS reads.
- **Complexity:** `Theta(N)` file opens/stats/clone-range calls and directory entries; for current 100 MiB, at least 5,284 payload extents plus mapping. Cross-volume/unsupported paths become `Theta(S)` copies.
- **Measured ceiling:** G3 proves one whole-file clone, not thousands of chunk clones. Current reconstruction already has 5,284 chunk occurrences; the deleted carrier showed how locator/index cardinality can dominate (`implementation-detail/phase-4/storage/append-only/first-implementation-findings.md:161-190`).
- **Predicted gain:** Unlikely to beat one verified full-file seed clone; may beat byte copy on a narrow same-filesystem case but risks metadata/extent fragmentation and inode explosion.
- **CPU:** Lower byte copy on successful reflink; high syscall/path/metadata CPU.
- **Memory/Q:** Bounded, but descriptor pressure must be batched.
- **Storage:** One inode/file per unique chunk, directory/index overhead, fragmented extents; physical sharing is filesystem-specific and cannot be assumed in quota/accounting.
- **Authority:** Every loose file still needs protected custody or Object-ID authentication; path/name alone is not authority.
- **Durability:** Thousands of independently created names and directory updates complicate crash cleanup/GC; catalog must never reference incomplete files.
- **Identity/format:** Physical-only if canonical bytes unchanged; new store layout/migration nonetheless.
- **Cross-operation effect:** Might help selected ranges; likely hurts create/reopen/GC/10k-file workloads. Does not solve persistent whole-file authority.
- **Experiment:** Only as a disposable syscall/object-count lower-bound against C4 whole-file clone and, if storage research is later opened, C3A SQLite extents or conditional C3B segment streaming at 1/10/100 MiB, with allocated storage and fragmentation. Falsifier: more than 2x syscalls, >5% create/storage regression, or any advantage absent on fallback platforms.
- **Evidence:** Current 5,284-chunk cardinality (`implementation-detail/phase-4/test-checkpoint-report/cp-0010-dirty-72ed9fee8e6a-fastcdc-v2-phase4-grind.md:139-155`); platform clone restrictions in C4.
- **Disposition:** **REJECT as the primary architecture; retain only as a narrow disposable comparison if C4/C3 need a lower bound.**

### C10 — Foreground compression, delta packs, or Git-like pack-first storage

- **Mechanism:** Compress/delta canonical objects into packs and decode on read; maintain pack indexes/repack/GC.
- **Target paths:** Storage footprint, perhaps sequential reads; risks create, range, edit, CPU, recovery.
- **Complexity:** Foreground encode/decode `Theta(S)`; delta chains add dependency depth and random-read amplification; repack `Theta(selected pack bytes)`.
- **Measured ceiling:** Adaptive zstd-1 saved only 4.1453% of finalized DB bytes and spent ~147.8 ms encoding; only 563/5,284 chunks became smaller. Git delta experiment made the pack 844 bytes larger (`research/phase-4/storage/compression-and-packing.md:168-239`, `:297-325`, `:409-426`).
- **Predicted gain:** Negative on retained fixture and create guard; storage saving below the desired material threshold.
- **CPU:** Material foreground regression; delta decode can multiply warm/range CPU.
- **Memory/Q:** Codec windows/dictionaries/chains add state; must remain bounded but provide no proven benefit.
- **Storage:** ~4.1453% ideal saving here, before index/GC complexity; below a sensible >=20% reopen threshold.
- **Authority:** Decode then authenticate canonical bytes/Object ID. Pack checksum/index is not canonical authority.
- **Durability:** New pack/index/repack/GC crash protocols and space amplification during rewrite.
- **Identity/format:** Physical-only if decoded canonical bytes unchanged, but new storage profile and migration.
- **Cross-operation effect:** Potential space saving; likely create, range, reopen, repair, and GC harm.
- **Experiment:** None in Round 2. Reopen only with a representative corpus predicting >=20% post-dedup net saving and a codec lower bound inside create/read budgets.
- **Evidence:** Sealed local compression/packing study above; Git's MIDX/pack design is useful for index/GC concepts, not evidence that packing wins this workload ([Git multi-pack-index](https://git-scm.com/docs/multi-pack-index), [Git pack format](https://git-scm.com/docs/pack-format)).
- **Disposition:** **REJECT for current workload; evidence-conditional reopen only.**

## 6. Operation-by-operation architecture

| Operation | Authoritative work | Preferred physical route | Honest complexity and target |
|---|---|---|---|
| Reconstruction, no native output | Pin expected root; authenticate canonical mapping/chunks; validate lengths/extents; stream bytes to caller/hash only if requested | C1 over current SQLite BLOB path; C3A/C3B are deferred storage research | `Theta(S+N+M)`. C1 may clear <300 ms; no <50-ms claim without trusted derived bytes. |
| First-ever native materialization | Same as reconstruction plus exact native write, data/metadata sync, rename, directory sync | Current stream; later C3A SQLite extent BLOBs, and only if insufficient C3B external segments, while admitting C4 seed when policy allows | `Theta(S+N+M)` and stable publication. Must preserve create/fallback guards. C3A/C3B are not G4 do-now. |
| Warm/trusted repeated materialization | Validate cache authority bound to exact file root and descriptor; no full payload authentication if custody is valid | C4 one whole-file clone, optional authenticated range patch | `O(clone metadata + changed bytes)`, plausibly <50 ms on supported same-volume clone FS; full fallback otherwise. |
| Incremental same-size materialization | Canonical engine first constructs/proves target root and exact parent/target changed-range relation; C4 then validates only seed-byte custody and consumes that independent authority | C4/G3-style clone + patch for payload materialization only | `O(changed authenticated extents + patch + sync/publication)` after separate canonical edit/mapping work. G3 is a materialization mechanism proof, not edit-authority proof. |
| Count-changing edit/materialization | Canonical COW produces new root; seed qualification only if a complete, safe extent-remap proof exists | Initially complete authenticated fallback | Current G3 deliberately falls back (`crates/layerfs-engine/src/bin/phase4_g3_materialization.rs:1996-2081`). Avoid speculative partial shifting. |
| Same-open edit | CDC changed region + canonical COW + immutable new objects + head publication | Current SQLite object-row path; any C3A/C3B layout remains deferred and must preserve the same authority | Preserve existing ~4.6–7 ms 100-MiB guards and exact identity. |
| First edit after reopen | Reestablish exact canonical mapping/edit authority; do not infer it from head lookup or a native seed receipt | Current canonical authority path. C4 may accelerate a later materialization of the already-authorized target, not the edit itself | Current 154.019 ms gap is not reopen/head (2.088 ms) and remains unchanged/unproven by C4. |
| Range read | Authenticate root/path and complete selected chunks; return selected slices | Current mapping over C3/C5 | `O(log_f N + selected objects + R)`; protect 2.279-ms 1-MiB row. |
| Reopen/head | SQLite format/profile/integrity/head/receipt validation | SQLite catalog remains authoritative | Already ~2.088 ms; do not spend format complexity here. |
| Scrub/repair | Full reachability walk and canonical authentication; rebuild physical indexes/cache | Current SQLite scan; later C3A extent scan or conditional C3B segment scan; remove/rebuild C4 entries | `Theta(live canonical bytes)`. Charge synchronous work to foreground or asynchronous work to explicit maintenance CPU/RSS/Q/I/O/storage rows; never call it free background work or substitute it for foreground correctness. |

## 7. Resource and history model

### 7.1 Canonical/value-plane storage

Let:

- `V0 = 105,122,466` bytes be accepted initial canonical bytes at 100 MiB;
- `D0 = 109,199,360` logical SQLite DB bytes and `A0 = 117,510,144` allocated store bytes;
- `u_i` be new unique canonical bytes introduced by revision `i` (new chunks plus mapping/root/delta objects, after CAS reuse);
- `o_i` be new unique object count;
- `e` be Stage-A SQLite extent framing, locator, and unused-tail/slack overhead;
- `h` be Stage-B external segment-frame overhead per unique object;
- `t` be Stage-B unreachable crash/orphan tail not yet reclaimed;
- `C` be native-cache allocated-byte cap.

Then the ordered storage ladder should obey:

```text
sqlite_extent_logical(n) = V0 + sum(i=2..n, u_i) + e
external_segment(n)      = V0 + sum(i=2..n, u_i) + h * sum(i=1..n, o_i) + segment_footers + t
catalog(n)               = O(total unique live/retained objects + roots + receipts)
native_cache(n)          <= C allocated bytes at steady state, with separately reported maintenance high-water
```

Stage A must be modeled and measured first inside the accepted SQLite file. Stage B's formula is relevant only if Stage A is insufficient; it does not authorize a second truth. In both cases CAS/reference history, not revision labels, determines payload growth.

It must **not** obey `n * source_size` unless every revision really has distinct content and the native cache policy deliberately retains every projection. CAS/reference history, not revision labels, determines value-plane payload growth.

| Revisions | C3A/C3B canonical value plane | Unbounded native projections (rejected) | Bounded example `C=2 GiB` |
|---:|---|---:|---:|
| 10 | `V0 + Σ₂¹⁰ u_i + e` for C3A; add external frame/footer/tail only for conditional C3B | up to ~1,000 MiB for ten distinct 100-MiB roots | ~1,000 MiB maximum before sharing/accounting details |
| 100 | `V0 + Σ₂¹⁰⁰ u_i + e` for C3A; conditional C3B uses its separate formula | up to ~10,000 MiB | <=2 GiB steady state; eviction/rebuild work and high-water measured separately |
| 1,000 | `V0 + Σ₂¹⁰⁰⁰ u_i + e` for C3A; conditional C3B uses its separate formula | up to ~100,000 MiB | <=2 GiB steady state; eviction/rebuild work and high-water measured separately |

For one-byte same-size edits, `u_i` should be one localized CDC region plus a mapping spine and namespace/transition metadata, but exact bytes are data-dependent and must be measured; this report does not invent a fixed per-edit number. For current fixed-ordinal count changes, mapping history can be suffix-linear: 100-MiB early/middle examples rewrite 365,495/185,915 canonical mapping bytes, and the exact model scales with the affected suffix (`implementation-detail/phase-4/algorithm/complexity-analysis.md:2268-2306`). The value plane still reuses unchanged chunk objects.

### 7.2 Garbage collection, compaction, and cache maintenance

C3A stays under SQLite's one-file transactional authority. Deleting dead locator/extent rows may move pages to the SQLite freelist without reducing allocated file bytes; any `VACUUM`, rebuild, or incremental reclamation is explicit maintenance with measured CPU, RSS/Q, logical/apparent/allocated temporary high-water, lock time, and post-maintenance endpoint. If it blocks a request, all of that work is foreground. Stage A must be evaluated before the following conditional Stage-B design.

A safe conditional C3B collector is generational:

1. In a consistent SQLite read transaction, compute live Object IDs from retained roots/snapshots/receipts and pin the source segment generation.
2. Copy and authenticate selected live frames into new segments; write footer/manifest; sync all new segment files and their directory.
3. In one SQLite write transaction, install the new generation's locations and mark old segments retired; commit under the normal `FULL` policy.
4. New readers pin the new generation. Existing readers retain leases/descriptors on the old generation.
5. Only after all old-generation leases drain, unlink retired segments and sync the segment directory.
6. A crash at any point yields either unreferenced new segments, an old committed generation, or a new committed generation with old segments still present. Recovery never guesses from file mtime.

Trigger compaction from measured live/allocated ratio and orphan-tail threshold, not wall-clock age alone. Keep at least one safe rollback generation until verification. Charge all copy/authentication/sync/catalog-switch/lease-drain/unlink work to a foreground or explicit maintenance resource row. Git's incremental MIDX/repack/expire design shows why a replaceable object-to-pack index and deferred expiration are useful ([Git MIDX design](https://git-scm.com/docs/multi-pack-index)); LayerFS still needs its own SQLite/segment crash proof.

C4 cache eviction is logically simpler because the cache is derived, but it is not resource-free: mark entry evicting, prevent new leases, wait for descriptor leases, unlink, sync directory, and delete/reconcile the catalog entry. Repair/revalidation/rebuild may read and hash `Theta(S)`, create a second full apparent file, allocate new extents, and hold buffers/descriptors. A crash may leak a rebuildable file or catalog row but must not affect canonical head correctness. Foreground-triggered work is charged to that request; asynchronous work receives explicit maintenance wall/CPU/RSS/Q/I/O/apparent/allocated counters. Capacity accounting uses allocated bytes (`st_blocks`/platform equivalent), includes admission, quarantine, old/new repair files, clone amplification, and eviction lag, and exposes temporary high-water plus post-cleanup values.

### 7.3 First-materialization temporary storage

Portable stream fallback needs one private temp with apparent size `S`, then rename to the final destination; before rename, old destination plus new temp can coexist. A C4 clone temp may have apparent size `S` but initially small additional allocated bytes due to shared extents; patching causes filesystem-granularity COW. Physical sharing must not be assumed in quotas. APFS/Linux/Windows clone primitives are same-filesystem/volume accelerators with platform restrictions, so cross-volume destinations always use the complete stream fallback (primary platform references in C4).

## 8. Crash ordering, concurrency, and security model

### 8.1 C3 storage-ladder publication state machines

C3A publishes extent BLOB bytes, locators, roots/receipts, and head in one SQLite transaction under the accepted one-file `FULL`+`DELETE` authority. A failed/uncommitted transaction exposes none of the new extent generation; an ambiguous outcome is reconciled by reopening the same database. This simpler authority boundary is the required first lower bound.

Only a later, separately justified C3B introduces the following two-file state machine. SQLite remains the only committed catalog/head truth throughout:

| Crash point | Durable/recoverable interpretation |
|---|---|
| Before a complete frame/footer | Ignore/truncate only beyond the last independently validated durable watermark; never catalog it. |
| Complete frame written, before segment sync | No SQLite reference may exist. Treat as orphan/unknown tail on recovery. |
| Segment synced, before catalog transaction | Durable orphan. It may be reclaimed or, only through a later normal authenticated SQLite write transaction, admitted as a location; scanning alone never promotes it into truth. |
| Catalog rows written, transaction not committed | SQLite rollback restores old catalog/head; segment frames remain orphaned. |
| SQLite commit return | Catalog/head may reference only previously synced segment ranges. This is the success boundary. |
| Commit or directory-sync acknowledgement lost | Reopen SQLite, validate format/head/receipt/locations, and report old-or-new. Do not truncate segments based on an exception alone. |
| During GC copy | Old generation remains authoritative; new unreferenced segments are removable after validation. |
| After GC catalog switch, before old unlink | Both physical generations exist; new catalog wins, old waits for reader leases. |

SQLite's documented atomic-commit sequence relies on filesystem/VFS assumptions and distinguishes application commit semantics from actual device stable-media completion ([SQLite atomic commit](https://www.sqlite.org/atomiccommit.html)). This report therefore claims dispatch/ordering, not power-loss proof beyond the platform's fsync contract—the same evidence boundary retained by G3 (`implementation-detail/phase-4/baseline/g3-incremental-materialization-baseline-v1.md:117-123`).

### 8.2 Concurrency

- **Writers:** C3A uses the existing SQLite write transaction/fence for extent bytes through head commit. Conditional C3B requires one process/cross-process writer fence spanning segment append through SQLite head commit; SQLite's own write serialization is necessary but insufficient if a segment writer can append without the same ownership token.
- **Readers:** C3A pins `(head/root, SQLite extent/catalog generation)`; conditional C3B additionally pins segment generation and uses immutable `pread` spans. Neither may observe a mixture of location generations for one operation.
- **Compactor:** acts as a writer for location publication; copies/authenticates pinned immutable inputs, then atomically switches catalog generation. Its wall/CPU/RSS/Q/I/O/storage is foreground or explicit maintenance evidence.
- **Native cache:** per-key admission lock prevents duplicate builders; read-only descriptor leases survive eviction intent; mutable destination never becomes seed authority.
- **Head changes:** immutable old roots remain readable if retained; VFS handles do not silently follow a moving head. New opens may choose the new head.
- **Backpressure:** bound simultaneous segment windows, VFS page cache, cache admissions, and fallback materializations; queue instead of scaling Q with request count without limit.

Current public engine calls are serialized through one `Mutex<Connection>` (`crates/layerfs-engine/src/lib.rs:243-249`). A new architecture must separately test multiple processes, reader/GC overlap, same-root concurrent admissions, and lost writer ownership; single-process success is not concurrency evidence.

### 8.3 Security and corruption

- Validate every frame length, offset arithmetic, segment bounds, object kind, canonical length, and Object ID before decode/use. Enforce maximum canonical/mapping depths and counts from current core limits.
- Frame CRC/checksum is torn-write/corruption diagnostics only; cryptographic authority is the expected Object ID/root.
- Open store/cache/segment directories and files descriptor-relative with no-follow semantics; reject symlink/wrong-kind paths. Preserve G3's typed preflight and cleanup behavior (`crates/layerfs-engine/src/bin/phase4_g3_materialization.rs:1954-2007`, `:2142-2237`).
- Physical catalog locators may be malicious or stale; failed authentication is corruption, not a request to return bytes from another duplicate location silently. Repair may select another authenticated location and record the event.
- Bound proof/node sizes against algorithmic DoS. A new prolly/CDC boundary must include adversarial min/max page/chunk limits.
- BLAKE3 collision resistance is assumed by the existing format; no new truncated physical hash should become authoritative.
- A same-UID attacker who can write cache files or replace their namespace defeats path/inode/mtime hints. Either provide stronger custody (service-owned namespace, read-only mount/sealed descriptor lifecycle) or rehash completely and lose the fast hit.

The repository currently supplies only the second option after restart: full revalidation/rebuild. G3's unlinked descriptor cannot be reopened after broker death, and none of the existing OS/VFS/SDK crates implements a verified-file, fs-verity-like, separate-UID cache service, or replayable mutation journal (`crates/layerfs-os/src/lib.rs:1-94`; `crates/layerfs-vfs/src/lib.rs:1-5`; `crates/layerfs-sdk/src/lib.rs:1-5`). Consequently this report's C4 timing projection applies only after a future authority primitive is proved or within the exact live descriptor lifetime; it is not a current cross-client/restart claim.

## 9. Compatibility and migration

| Change | Canonical compatibility | Store migration | Rollback |
|---|---|---|---|
| C1 verified-stream contract | Canonical-v2 unchanged | Receipt/evidence schema may change; dual reader if persisted | Re-enable old verification path; canonical bytes unchanged |
| C2 public one-pass API | Unchanged | None | Code rollback |
| C3 storage ladder | Unchanged canonical bytes/IDs; C3A new SQLite physical profile, C3B later only if justified | First copy+authenticate objects into bounded SQLite extent rows and atomically flip format marker; only later may a separate generation use external segments subordinate to SQLite | Preserve the old SQLite object-row store until post-flip verification; C3B never resurrects uncommitted external frames; no destructive in-place conversion |
| C4 native cache | Unchanged | Build lazily; cache schema only | Delete/rebuild cache |
| C5 VFS | Unchanged | API/handle version only | Disable VFS, use native materialization |
| C6 macrosegment canonical objects | New profile/root unless outboard-only | Full dual-format/read migration | Retain old graph; expensive |
| C7 prolly mapping | New mapping version and roots | Explicit re-encode/re-root | Retain v2 reader/objects |
| C8 CDC profile | New content/profile and roots | Explicit rechunk/re-root; never on read | Retain current profile |
| C9 loose reflink CAS | Physical profile only | Copy/authenticate objects | Keep old store until verified |
| C10 pack/compression | Physical profile if decoded canonical bytes exact | Repack with dual indexes | Retain old packs/generation |

Canonical/profile negotiation belongs at store open, not inferred from payload coincidence. Existing v1/v2 read-only migration/profile rejection already has retained evidence (`implementation-detail/phase-4/baseline/canonical-v2-baseline-v1.md:141-145`). C3 should follow the same explicit marker discipline.

## 10. Shared improvements versus benchmark tricks

**Shared/product-shaped:** C1 only after the root/per-object authority proof and public receipt/error schema are specified; C2 public API pass reduction; the deferred C3A one-file SQLite extent lower bound before any C3B external storage; C4 service-custodied bounded cache for native read/clone only, with separate canonical edit authority and portable fallback; C5 direct VFS ranges; common counters for object/location/frame/clone/fallback/maintenance work.

**Benchmark-only and therefore rejected:** skipping closure/fingerprint because the fixture is known; trusting the mutable destination because a previous row just wrote it; retaining an unlinked seed beyond its proven lifecycle without a product authority service; disabling fsync/dirsync; using an in-memory full-file cache outside Q/storage; prewarming OS cache and calling it cold; deleting/omitting validation only in the measured arm; stacking C1+C3+C4 in one experiment; comparing against historical absolute medians rather than adjacent A/B.

G3 itself cleanly labels its seed mechanism benchmark-private and same-open (`implementation-detail/phase-4/baseline/g3-incremental-materialization-baseline-v1.md:14-18`, `:117-137`). That boundary must survive Round 2.

## 11. Decisive experiments designed; execution ledger

### 11.1 Experiment A — verified-stream authority A/B (recommended first)

**Hypothesis:** With an authoritative expected Canonical-v2 root and complete per-object authentication/shape validation, removing the separate closure commitment preserves every corruption/missing/wrong-role/cycle/length outcome and reduces 100-MiB reconstruction materially without changing SQL/object/read bytes.

**Design:** One change only. Adjacent, position-balanced A/B at 1/10/100 MiB under the benchmark lock. Predeclare exact source/method/executable/fixtures, rows, error corpus, timers, counters, Q, RSS/storage, cleanup, no-rerun rule, and independent recomputation. Keep output fingerprint initially so closure and fingerprint ceilings are not stacked.

**Promotion gate:** byte/mode/root/transition/error equivalence; terminal Q zero; create/edit/range/reopen within protected guards; >=10% reconstruction wall win in both positions; identical canonical auth/object/SQL/read bytes except the removed hasher work. **Falsifier:** any negative-case loss, authority ambiguity, or <5% gain.

### 11.2 Post-G4 storage experiment B — SQLite-resident extent-BLOB lower bound

**Hypothesis:** Bounded immutable authenticated extent BLOB rows inside the accepted single SQLite database can reduce object-row acquisition crossings without introducing a second durable authority and without >5% create/storage/RSS regression.

**Design:** This is explicitly **not G4 do-now**. If opened later, create a disposable derived SQLite database from exact sealed fixtures, with canonical bytes unchanged in bounded immutable extent BLOB rows and catalog/head/receipts under the same one-file transaction authority. Compare the current borrowed object-row BLOB path with incremental bounded extent-BLOB reads; keep identical root traversal, Object-ID authentication, closure/fingerprint contract, and output. Count catalog queries/rows, extent BLOB opens/reads/requested/returned bytes, span count, auth bytes/objects, wall/CPU/RSS/Q, one-file transaction work, and apparent/allocated steady-state plus temporary high-water. Only if this Stage-A result is insufficient may a separate preregistration compare external segments, including all two-file crash/GC costs.

**Promotion gate:** exact semantics; >=10% reconstruction win and <300-ms median in a fresh adjacent post-G4 campaign; create/storage/RSS <=5% regression; one SQLite committed truth; bounded foreground/maintenance compaction and repair. **Falsifier:** row-count reduction without wall benefit, create/range/storage/RSS guard failure, or any locator/extent authority ambiguity. These gates authorize more research, not current G4 integration.

### 11.3 This lane's actual execution ledger

```text
lane: core-architecture
disposable_experiments_executed: 0
hypothesis: design-only; Experiment A is the recommended G4-shaped proof-product test, while storage Experiment B is explicitly post-G4/deferred
exact_commands: None
utc_start: Unavailable — no experiment started
monotonic_start: Unavailable — no experiment started
utc_end: Unavailable — no experiment started
monotonic_end: Unavailable — no experiment started
wall: Unavailable — no experiment started
namespace: No /tmp/layerfs-g4-r1-core-* namespace created
inputs: local tracked source/docs plus sealed retained evidence, read-only
output_custody: this report only
raw_results: None
cleanup: Not applicable; no transient experiment files created
transient_bytes: 0
retained_experiment_bytes: 0
resource_model: static/code/evidence analysis; no timing, CPU, RSS, physical-I/O, or cache claim
unsupported_observations: cold-cache state, host physical I/O, stable-media completion, C3/C4 performance, multi-process behavior
reason_not_run: decisive next measurements are timing-sensitive and require benchmark-lock coordination; static facts already falsify immediate format/pack/profile implementation
```

## 12. Ranked recommendations

1. **C1: formalize then measure the Canonical-v2 verified-stream contract as a narrow shared engine `materialize_verified_file(..., sink)` primitive.** This is the smallest shared change with a measured ceiling large enough to clear the <300-ms reconstruction target and prevents benchmark/product validation drift. Keep expected-root and per-object authentication; remove one derivable product at a time. Keep native publication and public VFS/SDK exposure separate. Do not call it raw-ID removal or benchmark cleanup.
2. **C4: specify a bounded, service-custodied native seed cache for repeated read/clone/materialization only.** Reuse G3's exact qualification/publication/fallback lessons. Require a separate canonical engine proof for target mapping/root/changed range; do not credit C4 against the 154-ms first edit after reopen. Measure foreground and maintenance eviction/repair/revalidation/rebuild CPU/RSS/Q/I/O/storage. A mutable same-UID path is never sufficient authority.
3. **C5: prototype direct VFS range/sequential reads on current Canonical-v2.** Treat lazy open and avoided output writes as the win; do not claim a full sequential read is sublinear.
4. **C2: repair the public engine's multi-pass object API as an independent shared follow-up.** It is statically clear but not a substitute for G4 evidence.
5. **C3: defer the storage ladder from G4 do-now.** If post-G4 evidence still needs a layout candidate, lower-bound bounded authenticated extent BLOBs inside the same SQLite file/transaction first. Consider external immutable segments only if Stage A is insufficient; SQLite's committed catalog/head remains the only truth, and all compaction/GC/repair resources are charged.
6. **Keep Canonical-v2, current 8/16/32-KiB CDC, fixed-radix mapping, G1 `cache_spill=2000`, and G3 fallback/publication as protected baselines.** Any candidate must show exact semantic parity and fresh guard results.
7. **Defer C6/C7/C8.** Add a macrosegment Merkle layer only if later C3A evidence proves residual object cardinality dominates; adopt prolly mapping only for a stricter multi-GiB edit SLA; change CDC only under a new explicit content profile after a representative corpus study.
8. **Reject C9/C10 for the current workload.** Thousands of reflink extents trade byte copy for metadata explosion; compression/delta packs already failed the local economics.

Net recommendation: **keep logical identity stable, move physical bytes only behind replaceable authenticated locators, and spend new complexity only where a sealed ceiling exists.** The likely end-state is Canonical-v2 + closure/proof-product simplification inside its existing one-traversal verified stream + a one-file SQLite authenticated-extent value/control plane if later proved necessary + bounded protected native read/clone cache with separately proved edit authority + lazy VFS. External segments remain a conditional second storage stage, subordinate to SQLite and never a second truth.

## Appendix A — local source/evidence custody

The following are the material local inputs used for conclusions. Line anchors identify the relevant inspected region; SHA-256 identifies the complete file. Generated binaries were not copied. Sealed target evidence was read in place.

### A.1 Cargo topology and production/benchmark source

| Local file and line anchor | SHA-256 |
|---|---|
| `Cargo.toml:1-19` | `dbcb7eeb7672bdd5e8bb8ece8d238879e867b6f7f343ddfed50e20f807760621` |
| `Cargo.lock:1` | `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8` |
| `crates/layerfs-core/Cargo.toml:1` | `7104453012be05e2e9c9baa870dfba01c1a8ca321ac9b628649926437032849c` |
| `crates/layerfs-core/src/lib.rs:1` | `ad1a0191dfe2ecafeae35f1f8d68b49ea3b1cd3cb36ce5226278f90cf3e0305b` |
| `crates/layerfs-core/src/limits.rs:1` | `2ca5b3e8957331011f328fe87315c6fd43c6162c4da7ddee2960b571b30ea34f` |
| `crates/layerfs-core/src/identity/digest.rs:1-75` | `8d22dbf8216da6cb2d88c3e067d41724d6dddaa0007a65cf5cbc5b9923151ce7` |
| `crates/layerfs-core/src/identity/ids.rs:1` | `4e6fe13f99abc20d0395c8e95de937614070f7d7bf7e3027d52259990927f54c` |
| `crates/layerfs-core/src/object/model.rs:1` | `fe6cb9e79d3d9aa16cc82896015d3a0765fb542be5a333a2f5d74f47e42801ae` |
| `crates/layerfs-core/src/object/codec.rs:130-180` | `513596fffcd7dca5f63fd0d86a9df6376e6794ee350c137eb6d786bba2c74659` |
| `crates/layerfs-core/src/canonical_v2.rs:1-270` | `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc` |
| `crates/layerfs-core/src/cas/mod.rs:1` | `53a4effd5ccafedb649ad9c151e6ee7115958f5b9b4e5128f8c835518d3dd319` |
| `crates/layerfs-core/src/content/mod.rs:1` | `0969881a415f8bd4f4e1574170f8ee869b15145b215fad2c9a86dc0102ad6c9e` |
| `crates/layerfs-core/src/content/persistence.rs:1` | `5b7831aa493e84aa77db274c1ac87db70b709a406e8241d7a665c6cefcf287fa` |
| `crates/layerfs-core/src/cow/tree.rs:145-215` | `de3171a54ac9eb4c16be834d51e0b1636009529316e04703a67def3a335e48c7` |
| `crates/layerfs-core/src/cow/mutate.rs:45-75` | `59c22e102f235831e7ff5c12f119553c084044831199d015aaa53f57f88767fa` |
| `crates/layerfs-core/src/cow/persistence.rs:1` | `e2a25b67f7ee17a78a33aa0318bfcbcf020a5162b6670df8743941d282d65d56` |
| `crates/layerfs-core/src/delta/mod.rs:95-190` | `c417e08dc2b6ecb39dc8371ccc5517780f948924425d33921b1036f725c46b1e` |
| `crates/layerfs-core/src/delta/codec.rs:1` | `e601dfcc561188d58d6cbb41d4ad0b606501995bce04e366afb601a7ba0f5c61` |
| `crates/layerfs-core/src/validation.rs:1` | `f42eb13125cc19ecfc3e4567d35926b2871cd65b46d9f0af985c5a1782f02a5e` |
| `crates/layerfs-core/src/cdc/mod.rs:1-75` | `bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6` |
| `crates/layerfs-core/src/cdc/gear.rs:1` | `beb8637ea160f5b61401c0dec2b632927c81be0b491b443142973dc23108edb5` |
| `crates/layerfs-engine/Cargo.toml:1-22` | `35fd9c667575fdb3dd6ae720c4c43e6c654a9fd47da8b5dadc9f7672bd04498d` |
| `crates/layerfs-engine/src/lib.rs:243-260,377-387,683-747,850-1017` | `9475d9d32d2e59cdf7b8a5f9cc3e35ecf3c58e47152fcfbf96c7a8b896eeaadb` |
| `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:2150-2189,2364-2420,3005-3148,8238-8537,9058-9177` | `c78738ab213c7438544abdf2a37131652813873e30077469d578624f86ce3cdb` |
| `crates/layerfs-engine/src/bin/phase4_g3_materialization.rs:274-484,1087-1165,1303-1383,1632-1735,1954-2250` | `f9ffe7058761c60e7d81c5da18ed3d7a9afdb5344f41b9a97dcb8c2b8a51f032` |
| `crates/layerfs-os/Cargo.toml:1` | `ee7387a8858d3900792b424c77153a291983885a361a2c3e12128c5aa7cea21d` |
| `crates/layerfs-os/src/lib.rs:1-115` | `13866474b3b8387e06d9c501c533c3067100eb573654ed2b0912292847d94996` |
| `crates/layerfs-vfs/Cargo.toml:1` | `e6868b66f840e56c3614e7da13e6ea099b2b4a9de15e15c0d1d4d42708ffd27d` |
| `crates/layerfs-vfs/src/lib.rs:1-5` | `20de55cdbe636b2219d7eaa60bc703b126bb18b77f17d35c137ba0228ee75849` |
| `crates/layerfs-sdk/Cargo.toml:1` | `e3c94ac5a46873b7a3d3b91e123bf6950f8ba589ff333ea0b5928e153f818fdd` |
| `crates/layerfs-sdk/src/lib.rs:1-5` | `7bdcac0987a591841ce31d17134e040eef651335abc550ffec1b3d1971c01210` |

### A.2 Governing and retained local documents

| Local file and line anchor | SHA-256 |
|---|---|
| `implementation-detail/evaluation.md:1` | `067f4107b886a504511475f0977b269016d233b6186a0de70b1a5681460c46c3` |
| `implementation-detail/phase-3.md:1` | `c27b6cb030aac3edaf4ed949498139c01a9ec94738f3f3c7b8d7d2041d356443` |
| `implementation-detail/phase-4/2026-08-21-phase-4-full-grind.md:1-344` | `03ca46e7772c63a9f39eaa50275edd82a0e5ece50fc1c0aff00b4a21bd8db304` |
| `implementation-detail/phase-4/README.md:1-73` | `a5dc635898e53939e34e135471bffc22d6361babeb7d90a48e38678f4a67c830` |
| `implementation-detail/phase-4/algorithm/spec.md:1` | `67202cac261e401e103fe74143f7346fda3f2250ec6ede7fcf3e54016dc74fbf` |
| `implementation-detail/phase-4/algorithm/tests-and-benchmarks.md:1` | `a8e65a188e4f5904c347f01d9bd65022c057c2348cf4d0350d8089f32a6e5fdf` |
| `implementation-detail/phase-4/algorithm/complexity-analysis.md:68-124,469-589,1255-1271,2322-2416` | `c6a44fda3286b2e7e38b905f0336757563aec815068a23745011f0ec9b1c550b` |
| `implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md:1-42` | `0cafb37d4d44659d226dae51d8ae7243612e628b4b3f943c540992393668d1de` |
| `implementation-detail/phase-4/baseline/canonical-v2-baseline-v1.md:90-145` | `ea5b8f5a44991b726f7161ed10ad70eb4a98f4b4f507111346cf3f79633bdeed` |
| `implementation-detail/phase-4/baseline/g3-incremental-materialization-baseline-v1.md:1-137` | `b94a638bc94be43f25d7e9b30248d93dcfc35d7170f6f85673389706f5695056` |
| `implementation-detail/phase-4/baseline/fastcdc-contiguous-region-kernel-v2-baseline-v1.md:55-114` | `affd582d67f083a93755dd4d9ba41df1d80a79f94ad2f50fb02d34e36244a408` |
| `implementation-detail/phase-4/baseline/fastcdc-contiguous-region-kernel-v2-baseline-v1-manifest.tsv:1` | `f64a484c7966d17f7e1af2ebc8a91c58248605e28d29c9d0d750ded93f951e38` |
| `implementation-detail/phase-4/baseline/sqlite-writer-memory-cache-spill-2000-baseline-v1.md:45-129` | `75c1baff105c0de5557c4e7201ece4884d43dad6304e4e19c6a5fd26f9e812b6` |
| `implementation-detail/phase-4/baseline/sqlite-writer-memory-cache-spill-2000-baseline-v1-manifest.tsv:1` | `1e93b6ffb06051cdfef6958b799dcaaecb97349e3c04bbc23403041ec2ace473` |
| `implementation-detail/phase-4/test-checkpoint-report/cp-0010-dirty-72ed9fee8e6a-fastcdc-v2-phase4-grind.md:130-220` | `9917ac7b1f13eef61afd15cbb0e7f2a1cf457502cc8e0ecec30b90bcc35c2964` |
| `implementation-detail/phase-4/storage/append-only/decision.md:1-108` | `ea5ef914ffb84ce3155cf24dd28ed0800eedeac1839e74adc78d838700526633` |
| `implementation-detail/phase-4/storage/append-only/first-implementation-findings.md:161-220` | `4655028f67a35d27862c4cfab5f3d434dd5bbdb60e46161f61b30f4467c69988` |
| `implementation-detail/phase-4/storage/append-only/spec.md:1` | `8688cbcffc651ecc9326cf9bfc96cdd7eb4d4d8bc20a966edac3fe4cc979d652` |
| `implementation-detail/phase-4/storage/append-only/acceptance-ledger.md:1-85` | `84ee2b81d11d1cc8640383213d01c72450ba3916a904a535fc181a8f69860557` |
| `implementation-detail/phase-4/storage/sqlite/spec.md:1` | `256856fb1c0e0376abb56a83b229a71347ab5e0bd129f814c1c03dc0b4770bc9` |
| `implementation-detail/phase-4/storage/sqlite/implementation-plan.md:1` | `143ca5336169e8f7387a7e9075cdb11ac557eb0c8a5b067aeb690e1ba421effb` |
| `implementation-detail/phase-4/storage/sqlite/visible-head.md:1` | `8340011e0d9fe41834856a8e418c018a2911f25cd5a34e3788f0b58e87265c53` |
| `implementation-detail/phase-4/mapping/logical-persistence.md:1` | `a69569a36b76f2b5763991f11227d4e193dddbaeec9a828f7a0c922df672179a` |
| `implementation-detail/phase-4/mapping/research-handoff.md:1` | `9d2929b4228da4ca140d8a879631b35e848ee7b860f6a35ef28a3c1e91639c58` |
| `implementation-detail/phase-4/wp4m/f-series/f3/report.md:700-1010` | `11473b0aa941758bbb5a6311b10731be0efe24ca7f2074259dff32c41a3ea716` |
| `implementation-detail/phase-4/rollback/deletion-record.md:1-78` | `587c07cdfc86005282557b42a06d16c39b853d69fa301ce7ce14a918c6c796b7` |
| `implementation-detail/phase-4/experiments/g2-materialization-decomposition/G2-REVISE-REPORT-v1.md:47-270` | `a85419e73f6aefa701028b2192cf49682ef14e403d6d914239b719f077e12cce` |
| `implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-V11-POST-SEAL-REAUDIT-DISPOSITION-v1.md:1-67` | `8226aacee217a58436b2c8405d953ee18882e5ad400662f1004368a91a26dae5` |
| `implementation-detail/phase-4/experiments/g3-incremental-materialization/v12/V12-PREEXEC-REVISE.md:1-53` | `13d7bd160b730285ba4457fcabc0107c8064ed6c63bdf9a1cfc84e275596e2c8` |
| `implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v13.md:1` | `70a8fedfa97a03ea56031cb06b033593d1595b7558c986ee625deab40ea33fee` |
| `implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/COUNTER-DICTIONARY-v13.md:1` | `8809034ee8fff0013eb622799a9c676e14c8a102ec5557172f121d7a0434fe58` |
| `implementation-detail/phase-4/experiments/g4-materialization-acceptance/round-1-research-handoff.md:1-430` | `8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00` |

### A.3 Research inputs

| Local file and line anchor | SHA-256 |
|---|---|
| `research/phase-4/foundations/benchmark-and-evidence.md:1-92` | `62d385cd7a7245429326e7a9f6f6ba053c30fcbdf322b7fa0cabd10bfe9007a2` |
| `research/phase-4/foundations/hypothesis-ledger.md:1-40` | `1d4b3bb83f9dbb43d66e10702b946cb8f8dddc39c6c1faae00187ea4e4b6c2f9` |
| `research/phase-4/foundations/invariant-matrix.md:1` | `c9a25b681fb5f15555adec5e356651fae06ce3cc8b075ebd617b7840a524c285` |
| `research/phase-4/assurance/verification-security-resources.md:1` | `03f07d8337f346a411ed6138753dd8dc73781d191d8fdd9a35e0d8fc46341461` |
| `research/phase-4/decision-map.md:1-220` | `8ddb236ff7d3cfa03257c9006d8b6f219b151f7433a331b4f2b9ea900c0c30fb` |
| `research/phase-4/handoffs/hot-cold-materialization.md:1-380` | `3cb890cc34cf3667944482294a41bad4120e8bd3e7c86ebfdd09385b26b22429` |
| `research/phase-4/core/canonical/identity-and-hashing.md:1-180` | `ce947becfe9105a5df58888314ead2491f17ff1ca5842cd78f45302ab18efdb6` |
| `research/phase-4/core/canonical/v2-single-identity.md:1-230` | `0857d7633bfa8f8d7831087be4cea30479a9092553f9e08058528be593ac3cd7` |
| `research/phase-4/core/canonical/canonical-v2-exploration-findings.md:1` | `8b9b1fa13e56aed1b754da6b4b1dfe38d740199a0bded3b652fb3130ce824cd9` |
| `research/phase-4/core/canonical/h05-terminal-findings.md:1` | `261ca204466438d69b0d2dfd96cb517c86145abff6440381cfcb749c9935f2bf` |
| `research/phase-4/core/pipeline/full-create-pipeline.md:1` | `daabf94a31a5613e1cf78fbaef1d46f3d8395fb3bc94c2fdbba6fdaf02a4be8d` |
| `research/phase-4/core/cas/authenticated-reuse.md:1` | `49c20e7404248f5dcc461271f3f829d0e2a97469c1b6fb97a0ef4c071630a6dd` |
| `research/phase-4/core/cdc/locality-and-algorithms.md:1-205` | `6e3935dae62b735c015f8feef09ddae49829f525bc6ce6a7e92e806f2cd13ba5` |
| `research/phase-4/core/cow/mapping-and-deltas.md:1-183` | `b48facb78eb05cd5d11b330e990a6fcc11b88d595dbe34e9d5f4d9ed207ee2ca` |
| `research/phase-4/storage/compression-and-packing.md:1-503` | `d5160bc38e9fb24601ec936e1ec46a0a0c81d06ff6f803f26534ca67c16d2815` |
| `research/phase-4/storage/sqlite/durability-and-layout.md:1-374` | `12053708d794fa9737b3c388d1ae74887e4267b0b1334d3b654430c9ea1b3a3e` |

### A.4 Sealed/retained measured evidence read in place

| Local artifact and line anchor | SHA-256 |
|---|---|
| `target/phase4-fastcdc-contiguous-region-kernel-20260821-v2/static-v1/STATIC-CLOSURE-v1.json:1` | `be39b66aaf844314a53d149a003a4537b76139769e2c2f69c319bab7e473ba18` |
| `target/phase4-fastcdc-contiguous-region-kernel-20260821-v2/audit-v1/INDEPENDENT-RECOMPUTATION-v1.json:1` | `cfd5fa4faddd9d575926ca5c9ad565ec6aadd631682a8f3970a6201d280fbdf3` |
| `target/phase4-fastcdc-contiguous-region-kernel-20260821-v2-independent-rerun-v1/results-v1/PAYLOAD-MANIFEST-v1.tsv:1` | `9d6953bcdc3d8b476452b0e3646a04151d7ebc3a345dcb5cb7ccdfa9a481b713` |
| `target/phase4-fastcdc-contiguous-region-kernel-20260821-v2-independent-rerun-v1/results-v1/TERMINAL-v1.json:1` | `58eec75af7449df4fd32726488e4f7186a0073fb2296390c6da97a713727e5a4` |
| `target/phase4-fastcdc-contiguous-region-kernel-20260821-v2-independent-rerun-v1/results-v1/TERMINAL-VERIFICATION-v1.txt:1` | `0120f98a5cbee02ac90d8e3a92dc939e530331bf579237960297bd92afb42b26` |
| `target/phase4-g1-writer-memory-cache-spill-20260821-v1/results-v1/rows-v1/G1-RAW-v1.jsonl:1` | `3b4ca568ac3fbf3dd32fc1fb74f2bd3b14bad5aa3800e964cf47cbd847a58520` |
| `target/phase4-g1-writer-memory-cache-spill-20260821-v1/results-v1/INDEPENDENT-RECOMPUTATION-v1.json:1` | `ddc289b7b612857204f288c16c7404b14fe362727af41398359a7c59ef3e1f9f` |
| `target/phase4-g1-writer-memory-cache-spill-20260821-v1/results-v1/PAYLOAD-MANIFEST-v1.tsv:1` | `f02664ea4d82a73126584ed6197b4cea5bc3a21fc08a1562488a7c253dac2a3c` |
| `target/phase4-g1-writer-memory-cache-spill-20260821-v1/results-v1/TERMINAL-v1.json:1` | `54692f9a8d4445bb7c6e17738b0bbb781c8554aad8111d881aa3826d35fc2f07` |
| `target/phase4-g1-writer-memory-cache-spill-20260821-v1/results-v1/TERMINAL-VERIFICATION-v1.txt:1` | `0c89f9913b09ffe1259419b532e70e8d124244e0a942d6f8db20d4cdaeca2b85` |
| `target/phase4-g1-writer-memory-static-20260821-v1/STATIC-CLOSURE-v1.json:1` | `8c512b39a04481174fb4e9729d5385284d63e9fd5eb10b8a56f144b400d47566` |
| `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/rows-v5/G2-V5-RAW.jsonl:1` | `c64a4f7b4d1a831fd7406251f0de2ab44cfbf390d07188d55298fdbbfefb0eeb` |
| `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/G2-V5-ANALYSIS.json:1` | `432f903ecebe3afc6370e422c559e346f71abd71ba16f328d35e169e28732803` |
| `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/G2-V5-INDEPENDENT-RECOMPUTATION.json:1` | `86ab101df69f82ec548d8baa223ea4a6fde13646660969f6478a4e73fe08df5e` |
| `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/PAYLOAD-MANIFEST-v5.tsv:1` | `12f74b88188c1a22babe129c4b1d5d0e1889ba55d2cf0046ae55af6803709399` |
| `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/TERMINAL-v5.json:1` | `09a5948a2c6a31c55811d50459c24cf72c4d2e3ff61ea5773754bf5c6c1a60a2` |
| `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/TERMINAL-VERIFICATION-v5.txt:1` | `41447453a34b1933850e6e090a2bc59628d58f7d585e7c394e937cfe03250af0` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/rows-v13/G3-V13-RAW.jsonl:1` | `3d2b40da82f612441cf1af88ee89f2d8c79b139c75818d6c7e2a5488cbad956c` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/G3-PRIMARY-ANALYSIS-v13.json:1` | `b28003f59dcf3fbfa6a585762d70cdc0beae0b4c81ec51904327d388452820d7` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/G3-INDEPENDENT-RECOMPUTATION-v13.json:1` | `2f137bb1116d1637656d1c89777dcb9e1291e04899f6710a000e5a6933419ace` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/PAYLOAD-MANIFEST-v13.tsv:1` | `1581f8f4b890237c6c04f17b79baf445067461767146c916b2d4df80c3030a49` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/TERMINAL-v13.json:1` | `1230187c702455eb3cf15aaa7d02197ebc5f60b196d08c072e524a87107a828e` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/TERMINAL-VERIFICATION-v13.txt:1` | `a9d06860828f14304b7f6fc1ef35146577e7ba770bacc4d4c428250d60169dd6` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/STATIC-CLOSURE-v13.json:1` | `cbefce3c9ad384105acbf2c81e0a0d4304c8c7eb118d16d874ad6913de9e3531` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/ENVIRONMENT-v13.json:1` | `c381064a91e1c58fade232c329346c032e2839f04332f3bf119c795a1237e11f` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/SOURCE-CUSTODY-v13.json:1` | `348b6409a8d45a74d5a80808a95611ea8d79f67d882292b549a84fbf464c004c` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/OPERAND-CUSTODY-v13.json:1` | `58b652948950ed27e7ceb57c5b156705932e44e9d89724c63e8687f84b782d58` |

Fixture custody used for sizing/reference only: 1 MiB `4a3acf60f044bbae8ed0d0a8aa8fabd8b4cee74216dbccc36255b9c6fbe50a2a`; 10 MiB `0c7a66930ae0d1d69fcc0b59942278eeb3a3fd92a8912e3e30963f288a8f430e`; 100 MiB `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` (retained canonical-v2 fixture set referenced by `implementation-detail/phase-4/baseline/canonical-v2-baseline-v1.md:103-120`).

## Appendix B — external primary sources and applicability

- [SQLite incremental BLOB API](https://www.sqlite.org/c3ref/blob_open.html) and [BLOB read](https://sqlite.org/c3ref/blob_read.html): subsection access/handle lifecycle; used only to interpret current public API possibilities.
- [SQLite atomic commit](https://www.sqlite.org/atomiccommit.html) and [synchronous pragma](https://www.sqlite.org/pragma.html#pragma_synchronous): rollback-journal ordering and durability configuration; used first for C3A's one-file transaction authority and, only conditionally, for C3B's SQLite side of the two-file protocol.
- [WiscKey, FAST'16](https://www.usenix.org/conference/fast16/technical-sessions/presentation/lu): key/value separation and its GC/crash trade; analogy, not performance transfer.
- [Venti, FAST'02](https://www.usenix.org/conference/fast-02/venti-new-approach-archival-data-storage): immutable content-addressed block storage/dedup; invariant precedent.
- [Git multi-pack-index design](https://git-scm.com/docs/multi-pack-index) and [pack format](https://git-scm.com/docs/pack-format): replaceable object-to-pack locations, incremental index layers, repack/expire; GC/index precedent, not approval of compression/deltas.
- [Bao specification](https://github.com/oconnor663/bao/blob/master/docs/spec.md): authenticated slices/outboard tree; used to show what a new proof layer would entail.
- [Noms technical overview](https://github.com/attic-labs/noms/blob/master/doc/intro.md): history-independent prolly trees; future mapping reference.
- [Nix store-object specification](https://releases.nixos.org/nix/nix-2.31.0/manual/store/store-object.html) and [content addressing](https://releases.nixos.org/nix/nix-2.24.2/manual/store/store-object/content-address.html): immutable store/closure and deterministic lazy content precedent.
- [Apple XNU `clonefile(2)`](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/clonefile.2), [Linux reflink ioctl](https://man7.org/linux/man-pages/man2/ioctl_ficlonerange.2.html), and [Microsoft ReFS block clone](https://learn.microsoft.com/en-us/windows-server/storage/refs/block-cloning): platform-specific clone capabilities/restrictions and required portable fallback.
- [Linux new-filesystem guidance](https://www.kernel.org/doc/html/latest/filesystems/adding-new-filesystems.html): userspace projection before an in-kernel filesystem where appropriate.
