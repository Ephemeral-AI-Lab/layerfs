# Phase 4 Rollback to SQLite and Core/Engine Optimization Implementation Plan

- Status: implementation pending
- Date: 2026-08-17
- Controlling specification:
  `spec.md`

## 1. Outcome

This plan returns the active codebase to two honest storage lanes:

- Memory for semantic reference and shared-core ceiling measurements;
- SQLite for durable production behavior and Phase 4 qualification.

It first deletes the two rejected pack implementations. It then freezes and
implements the missing Phase 3 durable mapping, builds a real create/edit
benchmark, and optimizes only costs shown by that benchmark.

The plan does not create a third backend. A third backend requires a later
specification and measured evidence that SQLite-specific work, rather than the
shared core, is the dominant remaining limit.

## 2. Execution rules

- Work only in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` on
  `codex/empty-worktree`.
- Do not modify `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`.
- Preserve unrelated worktree changes.
- Use `apply_patch` for manual edits and deletions.
- Do not commit unless explicitly requested.
- Use one Cargo writer/process at a time.
- Run the smallest exact tests first; expand only at stable checkpoints.
- Do not change frozen canonical bytes, CDC outputs, or SQLite format without
  separate explicit authority.
- Do not retain dead compatibility wrappers, feature flags, or disabled
  modules for the rejected pack implementations.
- Do not start an optimization before its direct counter exists.
- Run one optimization experiment at a time. Delete a rejected experiment
  before starting the next.
- A microbenchmark is diagnostic only; the unchanged full-workload row owns
  performance decisions.

## 3. Current checkpoint and protected state

The rollback begins from:

- branch: `codex/empty-worktree`;
- checked-out Phase 4B first implementation checkpoint: `f737958`;
- documentation/finding checkpoint available for reference: `6d28b0b`;
- current related dirty paths:
  `crates/layerfs-engine/src/append_only.rs` and
  `crates/layerfs-engine/src/bin/phase4_fair_benchmark.rs`.

Before deletion, record:

```sh
git status --short
git branch --show-current
git rev-parse HEAD
git diff --stat
git diff --check
```

The current append-only changes and untracked proxy benchmark belong to the
rejected Phase 4B direction. Capture their final source fingerprints and any
missing diagnostic result in the historical finding document before deleting
them. Do not preserve their code through a dormant feature.

## 4. Work-package dependency order

```text
WP0 authority/evidence snapshot
  -> WP1 delete append-only carrier
  -> WP2 delete PackedInMemoryCas
  -> WP3 post-deletion correctness baseline
  -> WP4 freeze durable Phase 3 mapping
  -> WP5 implement shared mapping
  -> WP6 Memory semantic engine
  -> WP7 SQLite mapping integration
  -> WP8 create/edit benchmark
  -> WP9 measured baseline
  -> WP10 authentication/closure optimization
  -> WP11 SQLite statement/transaction optimization
  -> WP12 remaining measured core optimization
  -> WP13 backend compatibility audit
  -> WP14 final campaign and decision
```

WP10 through WP12 are conditional. Stop when the durable SQLite lane reaches
the target with acceptable correctness and resource results. Do not optimize a
phase that counters show is immaterial.

## 5. WP0 — Authority and evidence snapshot

### Work

1. Confirm the branch, HEAD, and complete dirty status.
2. Classify every dirty path as Phase 4B work or unrelated user work.
3. Reconcile the latest proxy campaign into
   `../storage/append-only/first-implementation-findings.md` as non-promotion evidence if the
   result is not already recorded.
4. Label `../storage/append-only/spec.md` and
   `../storage/append-only/acceptance-ledger.md` as superseded/rejected for active work while
   preserving their contents and measured values.
5. Record the current source fingerprints used by the final proxy run.

### Required facts to preserve

- exact 100 MiB source fingerprints;
- five measured rows per lane;
- append-only median 31.596630 MiB/s;
- conservative SQLite-control median 35.290276 MiB/s;
- append-only 11.69% slower wall time;
- both rows were Phase 3 semantic proxies and did not authorize target or
  promotion claims;
- current append-only layout and correctness limitations.

### Exit condition

The rejected implementation can be deleted without losing the reason it was
rejected or mislabeling the diagnostic as a full logical benchmark.

## 6. WP1 — Delete the Phase 4B append-only carrier

### Production changes

1. Remove `crates/layerfs-engine/src/append_only.rs`.
2. Remove its module declaration, exports, constructors, profiles, counters,
   observations, and carrier-only error vocabulary from
   `crates/layerfs-engine/src/lib.rs` and other active modules.
3. Remove append-only-only benchmark binaries, including the untracked fair
   proxy binary after its evidence is recorded.
4. Remove append-only-only tests, fixtures, and test hooks.
5. Remove `fs2` or any other dependency only if repository-wide search proves
   it has no remaining caller.
6. Remove Cargo target declarations that exist only for Phase 4B.
7. Do not alter the SQLite schema, profile, SQL, or data files in this package.

### Search gate

Run repository-wide searches before and after editing:

```sh
rg -n "append_only|AppendOnly|Phase4B|phase4b|carrier marker|store\.log" \
  crates Cargo.toml
rg -n "\bfs2\b|FileExt" crates --glob '*.rs' --glob 'Cargo.toml'
```

Remaining hits must be historical documentation, generic English that does not
compile a carrier, or an independently justified non-carrier caller.

### Focused verification

```sh
cargo test -p layerfs-engine --offline --lib
cargo check -p layerfs-engine --offline --all-targets
git diff --check
```

The test run must discover and execute a nonzero number of SQLite tests.

### Exit condition

- no append-only production target remains;
- no dormant feature can restore it;
- no carrier-only dependency remains;
- SQLite tests pass without schema or profile changes.

## 7. WP2 — Delete `PackedInMemoryCas`

### Production changes

1. Remove `PackedInMemoryCas` from the core CAS module.
2. Remove `ChunkLocation` and any packed locator/index structures that have no
   non-packed caller.
3. Remove packed-only content and COW entry points, including
   `full_replace_packed_cas`, when search proves they are unused elsewhere.
4. Remove packed-only tests and benchmark modes.
5. Preserve `InMemoryCas`, ordinary logical-file construction, standard COW
   behavior, and existing Phase 1/2/3 golden identities.
6. Preserve the Phase 2 packed benchmark report and label its implementation
   rejected by the current specification.

### Search gate

```sh
rg -n -S "PackedInMemoryCas|ChunkLocation|full_replace_packed|packed_cas|phase2-opt2|packed" \
  tools crates Cargo.toml Cargo.lock \
  --glob '*.rs' --glob 'Cargo.toml' --glob 'Cargo.lock'
```

There must be no active Rust hit after deletion.

### Focused verification

```sh
cargo test -p layerfs-core --offline
cargo check -p layerfs-core --offline --all-targets
git diff --check
```

### Exit condition

- only the ordinary in-memory CAS remains;
- frozen Phase 1/2/3 results are unchanged;
- the core package passes without a packed feature or compatibility shim.

## 8. WP3 — Post-deletion correctness baseline

### Work

Run the complete existing core and engine owner suites on one unchanged source
fingerprint:

```sh
cargo metadata --offline --no-deps
cargo test -p layerfs-core --offline
cargo test -p layerfs-engine --offline
cargo check -p layerfs-core --offline --all-targets
cargo check -p layerfs-engine --offline --all-targets
cargo fmt --all -- --check
git diff --check
```

If workspace-level architecture or custody checks exist, run the affected ones
once at this checkpoint. Do not run unrelated long E2E walls.

### Record

- exact Git/source fingerprint;
- test targets and nonzero test counts;
- failures and their first typed causes;
- removed source/dependency counts;
- any historical documentation hits intentionally retained.

### Exit condition

The repository has a clean, warning-free Memory/SQLite baseline before any
new persistence format or performance work begins.

## 9. WP4 — Freeze the durable Phase 3 mapping

### Deliverable

Create the canonical mapping specification:

`../mapping/logical-persistence.md`

It must freeze exact bytes rather than describe only a conceptual graph.
The separately approved
`../storage/sqlite/visible-head.md` is the only schema exception
needed to persist that mapping; it does not expand WP4 into a migration system.

### Design inventory

Trace and list every persisted field and caller for:

- `Object`, `ObjectId`, and canonical encode/decode;
- raw `ChunkId`, canonical Bytes-object ID, and chunk length;
- `LogicalFile` and ordered chunk references;
- `TreeNode`, node kind, names, children, and metadata;
- `RootHandle` and parent relationship;
- all Phase 3 `Delta` variants and embedded state; and
- engine `RootRecord` and `DeltaRecord`.

### Decisions the mapping must freeze

1. Domain/version bytes.
2. Integer endianness and width.
3. File and directory node envelopes.
4. Exact metadata fields and canonical order.
5. Ordered file references containing:
   raw chunk ID, raw length, and canonical Bytes-object ID.
6. Directory entry encoding and name constraints.
7. Manifest paging and maximum references per object, if required for bounded
   memory.
8. Stable root identity derivation.
9. Exact delta encoding for every supported operation.
10. Strong-edge traversal order and cycle/depth/reference limits.
11. Typed errors for truncation, trailing bytes, duplicates, invalid order,
    unknown version, malformed IDs, size overflow, and resource overflow.
12. Compatibility rule for future versions.

Prefer composition from the already frozen Phase 1 `Object::Bytes` and
`Object::Directory` encodings. A new `ObjectKind` requires a documented proof
that those two cannot represent the required mapping safely and boundedly.

### Golden-vector set

At minimum freeze:

- empty file;
- one-chunk file;
- multi-chunk file with distinct raw and canonical object IDs;
- empty directory;
- nested directory with deterministic ordering;
- all persisted metadata boundary values;
- root with and without parent;
- one example of every delta operation;
- maximum valid reference page;
- truncated, trailing, reordered, duplicate, and unknown-version failures.

For successful cases record exact encoded bytes, BLAKE3 object ID, root/delta
identity, and reconstructed logical value.

Pre-promotion vectors are measurement fixtures, not compatibility authority.
Regenerate the independent authoritative set after one profile is promoted.

### Exit condition

Split WP4 into two explicit states:

- **WP4-C — candidate specification:** complete when the candidate grammar,
  bounds, semantics, failure rules, and measurement gate have passed review.
  This authorizes only WP4-M below; it grants no compatibility authority.
- **WP4-M — policy measurement:** complete from CP-0006's 27/27 compact PASS.
  K64/F64 is policy-selected and DIR256K is the unmeasured fallback; neither
  is compatibility-promoted.
- **WP4-P — compatibility-profile promotion:** complete only after the A/B
  input is frozen, every losing
  alternative and selector is deleted, independent final goldens are
  regenerated and fingerprinted, and the final read-only audit passes.

No public or compatibility-bearing production codec begins before WP4-P.

## 9A. WP4-M — completed compact profile measurement lane

### Narrow authority

CP-0006 completed the prospective compact K64/F64 path: 1/10/100-MiB writes,
100-MiB same/`+1` edits, and three roundtrips. DIR256K is carried forward by
the declared unmeasured fallback. The prior multi-profile 100/512 campaign is
historical custody-lost NO-GO evidence and is not the current authority.

The selector is private to tests and the benchmark. Do not add a public feature
flag, provider abstraction, format-negotiation surface, or permanent
multi-profile production API. Isolated benchmark databases may contain these
candidate identities; no candidate has compatibility authority. Each candidate
database and receipt uses a private domain-separated profile ID and the
candidate-only schema authority in
`../storage/sqlite/visible-head.md`.

### Dependency order

1. WP4-C candidate specification.
2. WP4-M compact shared codec/SQLite measurement path — complete at CP-0006.
3. WP4-P policy-input promotion, loser/selector deletion, final independent goldens,
   fingerprint, and read-only audit.
4. WP5 frozen-format exit rerun and finalization of WP5 and later production
   work against the single promoted profile.

Every CP-0006 row says `qualification=false`, `promotion=false`,
`purpose=fixed_radix_acceptance`, and `milestone=WP4-M-FIXED-RADIX`.
It cannot support a product, compatibility, or 200/300-MiB/s claim.

### Exit condition

Exit satisfied by CP-0006. WP4-P is eligible. It must delete losing constants,
branches, fixtures, and the private selector before generating final goldens
or rerunning WP5's exit checks.

## 10. WP5 — Implement the shared mapping

WP4-M may contribute the minimum shared codec code needed for measurement, but
that provisional code is not a completed WP5 deliverable. WP5 finalizes only
the surviving single profile after WP4-P.

### Production changes

1. Put encoding and decoding in the existing semantic owner wherever possible:
   object framing in the object module, file references in content, tree
   semantics in COW/tree, and delta semantics in delta.
2. Add a cross-domain codec module only if the frozen format genuinely cannot
   live in those owners. Do not introduce a provider abstraction.
3. Use checked arithmetic for all lengths, offsets, counts, and allocations.
4. Decode with explicit maximums and exact EOF.
5. Stream or page reference metadata when the frozen bound cannot fit the
   admitted in-process memory budget.
6. Remove reliance on provisional tree identity at the durable boundary.
7. Allow authenticated reconstruction through a narrow object-read semantic
   port rather than concrete `InMemoryCas` authority.

### Direct tests

- every golden success vector;
- every frozen malformed vector;
- encode-decode-encode byte identity;
- deterministic IDs across repeated runs;
- fragmented versus contiguous input equivalence;
- bounded decode allocation and checked-overflow boundaries;
- reconstruction and cross-chunk range reads without a concrete in-memory CAS.

### Verification

```sh
cargo test -p layerfs-core --offline <exact_mapping_test> -- --exact --nocapture
cargo test -p layerfs-core --offline
cargo check -p layerfs-core --offline --all-targets
cargo fmt --all -- --check
git diff --check
```

### Exit condition

The core can encode, authenticate, decode, and reconstruct a real Phase 3 file,
tree, root, and delta using only the promoted mapping and bounded object reads.
Rerun this check after WP4-P and verify that no losing profile or selector
remains.

## 11. WP6 — Add the Memory semantic engine lane

### Production changes

Add the smallest `MemoryEngine` needed to exercise the same persisted mapping
and capture semantics as SQLite:

- immutable canonical-object put/reuse through `InMemoryCas`;
- bounded root and delta storage;
- one-writer capture ownership;
- atomic in-process publication of root and delta;
- authenticated visible-root and delta loads;
- exact object/range reads; and
- explicit observations marking durability and process reopen
  `NotApplicable`.

Do not add an engine factory or public trait solely for two implementations.
Keep the existing SQLite `Engine` name unless a rename is independently needed;
the benchmark may call lane-specific functions directly.

### Parity tests

Run the same semantic vector against Memory and SQLite and assert identical:

- object IDs and creation/reuse outcomes;
- root ID and parent;
- delta bytes/identity;
- closure member sequence;
- reconstructed source bytes; and
- exact range results.

Durability, on-disk allocation, and process reopen are deliberately not equal
and must remain explicitly labeled.

### Exit condition

Memory is a trustworthy core ceiling, not a shortcut workload or fake durable
engine.

## 12. WP7 — Integrate the mapping with SQLite

The private WP4-M SQLite path may exist earlier only for profile measurement.
This production integration starts after WP4-P and exposes only the promoted
format.

### Production changes

1. Persist the frozen logical objects using existing immutable object storage.
2. Replace opaque benchmark delta payloads with the frozen Phase 3 delta codec.
3. Publish the complete root/delta graph in one existing capture transaction.
4. Validate full strong-edge closure before publication.
5. Authenticate the visible root, delta, and required closure after genuine
   close/reopen.
6. Implement only the single version-2 schema and exact version-1 handling
   authorized by `../storage/sqlite/visible-head.md`; do not build a
   migration framework or silently translate non-empty provisional stores.
7. Preserve existing journal/durability settings and typed error mapping.

### Correctness tests

- new file, unchanged recapture, small edit, large edit, and full replacement;
- immutable reuse with tampered incumbent bytes/metadata;
- missing child, wrong kind, malformed manifest, and invalid delta;
- commit failure and reopen after last successful root;
- parent conflict and stale-capture behavior;
- cross-chunk range reads and full streamed reconstruction;
- Memory/SQLite parity for all logical identities.

### Exit condition

SQLite owns a real full Phase 3 persistence path. Benchmark-only directory-of-
chunks proxies are no longer needed or allowed in qualifying results.

## 13. WP8 — Build the create/edit benchmark

### Target

Add one release benchmark binary, provisionally:

`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs`

It should be one binary with two explicit lanes, not a benchmark framework.
Before WP4-P, it may also expose the private WP4-M profile-selection selector;
that selector and its losing profiles are deleted at promotion.

### Fixture preparation

Provide a separate deterministic preparation command that writes the 1, 10,
and 100 MiB fixtures, validates the retained 512-MiB profile-selection fixture,
and writes a checked manifest. Preparation is outside every engine timer.

The manifest contains:

- generator version/seed;
- source length and raw fingerprint;
- logical fingerprint;
- CDC profile and ordered chunk-sequence fingerprint;
- canonical object IDs;
- expected root and delta IDs for each edit; and
- exact range probes, including a cross-boundary probe.

### Scenarios

Implement the matrix from the controlling specification:

- new file;
- unchanged recapture;
- one-byte middle replacement;
- 4 KiB middle replacement;
- 1 MiB middle replacement where applicable;
- full replacement;
- retained prepend/append/truncate/EOF/scattered regression rows.

Before WP4-P, the private profile-selection campaign also runs the forced
+1-reference early/middle edit and the section 12.7 wide-directory
create/replace/insert rows for every candidate ceiling.

### Output

Emit one JSON object per run. Human-readable summaries are generated from the
JSONL, not mixed into the timed engine path.

Every row includes:

- source and mapping fingerprints;
- engine/lane and scenario;
- timer-boundary version;
- exact output/result gates;
- additive phase timers;
- work, SQL, memory, storage, and cache observations;
- durability/reopen applicability;
- qualification booleans; and
- failure as an exact typed cause, not a partial success row.

Pre-promotion profile-selection rows always set `qualification=false` and
`purpose=profile_selection`. Only the post-promotion single-profile binary may
emit normal qualifying rows.

### Required benchmark self-gates

Reject a row unless:

- source, CDC, canonical identities, root, and delta equal the manifest;
- creation/reuse counts equal the scenario contract;
- the complete closure is visited and authenticated;
- exactly one SQLite transaction commits;
- reopened root and delta match publication;
- reconstructed bytes equal the expected source;
- all exact range probes pass;
- all bounded-memory maxima remain within their declared limits; and
- no residue, retry, hidden worker, or extra publication occurs.

### Exit condition

The binary cannot accidentally print a qualifying result for a proxy graph,
missing closure, missing durability, failed reopen, or unequal logical output.

## 14. WP9 — Establish the unoptimized fair baseline

### Campaign

Build once, then run one warmup and five measured iterations per required row
without compiler contention. Keep source generation outside timing.

CP-0006 completed the WP4-M compact profile evidence and made WP4-P eligible;
the historical 100/512-MiB A/B is not rerun. After WP4-P, delete the
provisional selector, rerun WP5's exit check, and rebuild the single-profile
binary before establishing the ordinary fair baseline below.

Example shape; finalize flags with the binary:

```sh
cargo build -p layerfs-engine --offline --release \
  --bin phase4_create_edit_benchmark

target/release/phase4_create_edit_benchmark \
  --engine memory \
  --manifest <fixture-manifest> \
  --scenario <one-scenario> \
  --iterations 1 \
  --jsonl <one-memory-row>

target/release/phase4_create_edit_benchmark \
  --engine sqlite \
  --manifest <fixture-manifest> \
  --scenario <one-scenario> \
  --iterations 1 \
  --jsonl <one-sqlite-row>
```

An outer campaign driver runs warmups separately, alternates row order, and
collects five isolated measured processes per engine/scenario. A developer
`--scenario all` mode may exist for diagnostics, but its campaign-wide RSS is
not promotion evidence.

Use `/usr/bin/time -l` for external RSS/host observations on macOS, while
reporting that its boundary includes process setup. Do not claim cold APFS
state unless an approved external procedure actually establishes it. Otherwise
report cache state `Unavailable` or `warm_or_unknown` with exact conditioning.

### Baseline report

For each row report median, min, max, spread, throughput or edit latency,
created/reused work, authenticated bytes, SQL counts, commit/reopen time, RSS,
and allocated store bytes.

Rank costs into:

1. shared core;
2. SQLite API/query work;
3. durability/reopen;
4. required versus duplicate authentication/closure; and
5. unavailable host effects.

### Decision

- If SQLite reaches 200 MiB/s, continue only with the largest justified step
  toward 300 MiB/s.
- If the Memory ceiling is below or near 200 MiB/s, do not add a database;
  optimize shared core.
- If Memory is fast and SQLite-specific counters dominate, proceed to WP11
  after checking WP10 for duplicate boundary work.

## 15. WP10 — Optimize authentication and closure

This is the first conditional optimization because prior diagnostics showed
multiple full object-authentication and closure passes.

### Measurement first

Add inclusive counters for every authentication caller and closure boundary:

- caller/boundary name;
- object/reference visits;
- unique versus occurrence visits where boundedly observable;
- canonical bytes read, checksum bytes, hash bytes, decode bytes;
- cache/receipt lookup and hit/miss;
- root, manifest, chunk, and delta time; and
- lock wait/hold time.

### Smallest experiments, in order

1. Fuse two traversals only when they operate on the exact same immutable
   snapshot and require the same validation result.
2. Pass a bounded authenticated receipt from an immediately preceding operation
   instead of looking up and authenticating the same locator again.
3. Reuse already authenticated unchanged COW members within one capture.
4. Keep reopen authentication independent unless a persisted authenticated
   generation receipt proves the exact reopened identity.

An operation-local verified-work receipt must include the immutable store and
validation authority, integrity epoch, mapping profile, generation,
authenticated root and transition, object ID, locator or row identity, and
exact byte range. Count/byte bounds and deterministic eviction must be
explicit. It is distinct from the locator-free snapshot-closure receipt. Do
not cache every object in the source.

### Rejection rules

Reject an experiment if it:

- trusts an index/key without payload authentication;
- weakens public immutable-reuse tamper detection;
- uses an unbounded visited map or object-byte cache;
- changes closure membership/order;
- moves work outside the timer; or
- fails to improve the full durable median.

### Exit condition

Record before/after full rows and direct counters. Keep only measured wins.

## 16. WP11 — Optimize SQLite statements and transaction work

### Audit before editing

For each scenario, record:

- transaction count;
- statement preparations and executions by SQL text/operation;
- object existence probes;
- inserts attempted/created/conflicted;
- incumbent value reads and authentication;
- root/delta reads/writes;
- rows examined/changed;
- query plans and index use;
- SQLite busy/locked events; and
- commit/sync time.

Confirm whether the current path already uses one transaction and prepared
statements. Do not call an operation “unbatched” without this evidence.

### Smallest experiments, in order

1. Reuse one prepared statement per repeated operation within the capture.
2. Remove redundant existence probes when an insert/conflict outcome already
   supplies the same decision, while still authenticating an incumbent before
   reuse.
3. Use bounded ID batches for existence or incumbent reads only if the observed
   per-object API crossings dominate.
4. Execute bounded insert groups inside the existing single transaction; do not
   create a source-sized SQL statement.
5. Write root and delta once after all objects and closure have qualified.
6. Add or change an index only when `EXPLAIN QUERY PLAN` and timings identify an
   exact missing access path.

### Non-negotiable behavior

- one capture transaction;
- one durable commit;
- immutable no-overwrite semantics;
- exact incumbent authentication;
- no WAL-mode switch;
- no pool, worker, async runtime, or hidden retry;
- bounded parameters and allocations; and
- unchanged SQLite format unless a migration is separately approved.

### Exit condition

Keep only changes that reduce direct SQL counters and improve the qualifying
full durable row without regressing edit latency, memory, or physical bytes.

## 17. WP12 — Optimize remaining measured shared-core work

Run these only in descending measured cost.

### Canonical encode/hash/copy path

Possible narrow changes:

- write canonical framing and payload through bounded reusable buffers;
- hash the exact canonical stream during its single construction where the
  object contract permits;
- avoid cloning a canonical object solely to pass it to the engine;
- compare authenticated incumbents without a second full decoded allocation;
- separate raw chunk hash from canonical object hash without conflating IDs.

Required counters: allocations, copied bytes, canonical bytes produced, raw
hash bytes, canonical hash bytes, and passes per object.

### COW edit locality

Possible narrow changes:

- retain authenticated unchanged chunk references;
- rebuild only the changed file manifest pages and tree spine;
- avoid re-encoding unrelated siblings;
- use bounded prefix/suffix rejoin validation required by the frozen CDC/COW
  contract.

Required counters: source bytes rescanned, chunks revisited/created/reused,
manifest pages rebuilt, tree nodes rebuilt, and closure bytes revisited.

### CDC mechanics

Only after the preceding work:

- increase or reuse bounded input buffers;
- remove per-byte/per-chunk allocations;
- make boundary scanning contiguous where possible;
- preserve the exact frozen chunk-boundary and ID sequence.

Required counter: unchanged CDC sequence fingerprint plus lower CDC time,
allocations, or source passes.

### Exit condition

Every retained change reduces a measured dominant counter and the relevant full
row. Do not keep speculative helpers for a later optimization.

## 18. WP13 — Storage-backend compatibility audit

### Deliverable

Create:

`PHASE_4_STORAGE_BACKEND_COMPATIBILITY_AUDIT.md`

### Method

Create a table mapping each required LayerFS semantic operation to:

- the Memory implementation;
- the SQLite implementation;
- requirements a future local KV engine would have to meet; and
- requirements a future remote database would have to meet.

Audit at least:

- immutable conditional object publication;
- batch existence/authentication/read;
- exact range reads;
- capture ownership;
- root/delta atomic publication;
- generation/snapshot identity;
- reopen semantics;
- durability acknowledgment;
- conflict and retry ownership;
- typed errors;
- bounded memory/request sizes; and
- instrumentation availability.

### Remote-specific analysis

Model request counts for the 1/10/100 scenarios under:

- one RPC per object;
- bounded batch RPCs;
- server-side transaction/conditional publication; and
- local authenticated metadata caching.

Do not implement a remote adapter. The audit must identify which current APIs
accidentally assume cheap local calls or expose SQLite-specific details.

### Third-backend gate

Recommend one named backend only if:

- SQLite misses 200 MiB/s after accepted shared/SQL optimizations;
- SQLite-specific cost is dominant and directly measured;
- the backend can satisfy atomic publication, immutable authentication, exact
  range reads, bounded memory, and typed errors; and
- the expected gain is large enough to justify integration and qualification.

Otherwise conclude that Memory and SQLite remain sufficient.

## 19. WP14 — Final campaign and decision

### Stable-source verification

On one unchanged source fingerprint:

```sh
cargo metadata --offline --no-deps
cargo test -p layerfs-core --offline
cargo test -p layerfs-engine --offline
cargo check -p layerfs-core --offline --all-targets
cargo check -p layerfs-engine --offline --all-targets
cargo fmt --all -- --check
git diff --check
```

Also run affected architecture/custody checks and warnings-denied/clippy gates
required by the controlling repository documents.

### Performance campaign

Rebuild the release benchmark once from the verified fingerprint. Run the full
Memory/SQLite 1/10/100 scenario matrix with one warmup and five measured
iterations per required row. Preserve raw JSONL and external host observations.

### Final report

Record:

- removed implementation paths and dependencies;
- exact mapping version and golden-vector fingerprint;
- exact source/fixture fingerprints;
- exact Git/source fingerprint and commands;
- test targets and nonzero outcomes;
- timer boundaries and qualification gates;
- medians, ranges, spreads, and units;
- the complete improvement-metrics table;
- Memory ceiling versus SQLite durable results;
- RSS/logical-memory/store allocated bytes and cache state;
- whether 200 MiB/s was reached;
- whether 300 MiB/s was reached;
- small/large edit latency and work amplification;
- retained and rejected optimizations;
- backend compatibility findings; and
- remaining materialization/projection work owned by the later phase.

### Final decision

Select exactly one outcome from the controlling specification:

1. retain SQLite after reaching at least 200 MiB/s;
2. retain SQLite and continue shared-core optimization because the remaining
   limit is engine-agnostic; or
3. authorize a new specification for one named third backend because a measured
   SQLite-specific limit dominates.

Do not claim that Memory satisfies the durable target. Do not call logical
reconstruction native materialization. Do not leave an experimental third
engine or rejected optimization in production code after the decision.

## 20. Implementation ledger

Update this table as work proceeds. A row is complete only when its exit
condition and exact verification evidence are recorded.

| Work package | Status | Source fingerprint | Evidence |
| --- | --- | --- | --- |
| WP0 authority/evidence snapshot | complete | starting HEAD `e760a122d128dc242e9364483a7259b360dacf87`; deleted-source SHA-256 set in `deletion-record.md` | Finding reconciled with five-row proxy evidence; rejected/superseded notices added; deletion record created. |
| WP1 delete append-only carrier | complete | final active source hashes recorded in `deletion-record.md` | Carrier module and two binaries deleted; exports/errors/dependencies/lockfile entries removed; active searches and SQLite checks pass. |
| WP2 delete `PackedInMemoryCas` | complete | final core CAS/content/eval source hashes recorded in `deletion-record.md` | Packed implementation, entry points, helpers, tests, and workspace eval benchmark modes deleted; ordinary `InMemoryCas`/COW paths pass core checks. |
| WP3 post-deletion baseline | complete | implementation commit `f595046e150e60dda6e3f06d915bbc283e20e952`; final active source hashes recorded in `deletion-record.md` | Metadata, workspace tests, all-target/all-feature checks, format, and diff gates pass; WP4-C is complete and WP4-P remains pending. |
| WP4-C candidate mapping specification | complete | candidate record SHA-256 `3e94b054e6bf0eb198f6b04287d8a6cb209fb2925450b6c6bc6a69c84ab63e06`; narrow schema-authority record SHA-256 `cfddcc291cfff40ffcfd19e8e93ba2a4e51b3b16c412d137ece5463acc7625df` | Scalable checked-u64 candidate, receipt trust boundary, 100-GiB analytical equations, rejection reconciliation, and executable selection dependency are recorded in `../mapping/logical-persistence.md`; `../storage/sqlite/visible-head.md` separately authorizes only the complete-head schema transition. Neither grants candidate compatibility authority. |
| WP4-M provisional profile measurement lane | complete | CP-0006 raw `b3596ff61b1314bad66f38675bc8acecccaa57d6a8686e30a0e224e91c8f72e1` | 27/27 compact PASS; Python/Ruby agreement; one transaction/COMMIT and terminal Q zero in every row; K64/F64 policy-selected; DIR256K fallback; `promotion=false`. |
| WP4-P compatibility-profile promotion | pending / eligible | — | Await loser/selector deletion, K64/F64 + DIR256K selected-only goldens, specification/vector fingerprint, and final audit. |
| WP5 implement/finalize shared mapping | pending | — | Provisional shared code may be measured only under WP4-M; rerun the frozen-format exit after WP4-P with one surviving profile. |
| WP6 Memory semantic engine | pending | — | — |
| WP7 SQLite mapping integration | pending | — | — |
| WP8 create/edit benchmark | pending | — | — |
| WP9 fair baseline | pending | — | — |
| WP10 authentication/closure optimization | conditional | — | — |
| WP11 SQLite batching | conditional | — | — |
| WP12 remaining shared-core optimization | conditional | — | — |
| WP13 backend compatibility audit | pending | — | — |
| WP14 final campaign/decision | pending | — | — |
