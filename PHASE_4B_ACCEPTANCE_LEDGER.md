# Phase 4B exploratory acceptance ledger

Status: exploratory candidate; **not qualified and not promoted**.
Rollback status: **rejected and superseded for active implementation** by
[`PHASE_4_ROLLING_BACK_TO_PREVIOUS_OPTIMIZATION_SPEC.md`](PHASE_4_ROLLING_BACK_TO_PREVIOUS_OPTIMIZATION_SPEC.md).
This ledger remains only as historical qualification evidence.

| Requirement | Evidence | Status |
|---|---|---|
| One carrier-wide writer authority | `append_only_writer_authority_is_process_wide`; `fs2` advisory lock; second open returns `CarrierBusy` | PASS |
| Separate bounded read-only readers | No read-only open API; exclusive lock applies to every public open | **NOT QUALIFIED / DEFERRED (P4B-C1)** |
| Canonical object identity and semantic validation | `append_only_round_trip_reuses_and_recovers`; object hash/validation counters | PASS |
| Authenticated reuse and unequal occupants | `append_only_authenticates_unequal_index_occupants_and_recursive_closure` | PASS |
| Full recursive authenticated closure | Same closure test; commit/reopen validates directory descendants | PASS |
| Fixed 256-bucket disk index and bounded cache | `append_only_index_cache_is_bounded_and_observable`; 32-page cache | PASS |
| Marker format/profile and capture evidence | marker tamper test; format ID, marker digest, capture-range digest | PASS |
| Marker visible-end bounds | invalid references and object/index lookup bounds in tamper tests | PASS |
| Publication ordering | code order: index root, delta, root, marker, flush, one sync | PASS |
| One marker and one sync per successful capture | `append_only_publication_has_one_marker_one_sync_and_exact_residue`; benchmark counters | PASS |
| Exact residue equation | same test asserts `residue = carrier_bytes - visible_end`; complete unmarked residue is retained | PASS |
| Typed append-stage failures | object/index/index-root/delta/root fault points; poisoned failed handles | PASS |
| Typed recovery causes | committed-history tamper fails closed; later malformed/checksum/predecessor tail retains old marker, classifies residue, and poisons the reopened writer handle | PASS |
| Torn-tail recovery | `append_only_torn_tail_does_not_publish` | PASS |
| Exact ranges | beginning, middle, end, empty, reversed, and out-of-bounds ranges | PASS |
| Phase 2 CDC persistence | frozen FastCDC 8/16/32 KiB profile and exact chunk lengths | PASS |
| Phase 3 COW/delta reopen | parented second root and both deltas reload after reopen | PASS |
| No public rollback API | `rg "pub fn rollback"` has no match; Drop is the private abandonment seam | PASS |
| Source-sized staging absent | benchmark uses file input and one scan; no `collect::<Vec<_>>()`; 262,157-byte declared bound | PASS |
| Stage timing separation | benchmark prints source/CDC/callback/encode/hash/put/commit/reopen timings; commit-only publication and sync are isolated, while incremental digest CPU is a direct overlapping counter | PASS, single run |
| Direct carrier/index/sync/lock counters | benchmark prints direct logical counters; actual syscalls/RSS unavailable | PASS, bounded evidence |
| 100 MiB diagnostic | release benchmark report includes hash-only and incremental-digest diagnostics; 5,363 chunks, 1.667840766 s wall for the latter | PASS, carrier diagnostic only |
| Full logical workload fairness | Current benchmark admits CDC objects but commits an empty-directory root and fixed delta; its generated LCG source also differs from the historical SQLite fixture | **NOT COMPARABLE / NO-GO** |
| Phase 4A A/B decision | no same-source Phase 4A baseline and no three-iteration median/spread | **NO-GO / OPEN** |

## Exact verification commands

```text
cargo fmt --all
cargo fmt --all -- --check
cargo test -p layerfs-engine append_only -- --nocapture
cargo check -p layerfs-engine --all-targets
cargo test --workspace
cargo check --workspace --all-targets
cargo run -q -p layerfs-engine --bin phase4b_benchmark -- /tmp/layerfs-phase4b-final-1m.log 1
cargo run --release -q -p layerfs-engine --bin phase4b_benchmark -- /tmp/layerfs-phase4b-final-100m.log 100
jq -e -c . PHASE_4B_BENCHMARK_REPORT.jsonl >/dev/null
```

Observed results:

- focused append-only tests: 12 passed, 0 failed;
- engine all-target check: passed;
- workspace tests: 68 passed, 0 failed; workspace all-target check: passed;
- formatting check and JSONL parse: passed;
- 1 MiB streamed diagnostic: completed with one marker, one successful sync,
  zero residue, and `peak_in_flight_bytes=262157`;
- 100 MiB streamed diagnostic: completed with 5,363 chunks, 10,732 frames,
  106,327,544 carrier bytes, one marker, one successful sync, zero residue,
  and `wall_ns=1678407666` after the engine-only identity-hash and incremental
  live-digest changes. This is scanner/admission-only: the committed root is
  an empty directory plus a fixed benchmark delta, not a source-referencing
  logical root.
- the 100 MiB row is exactly 104,857,600 input bytes versus 106,327,544
  carrier bytes: 1,469,944 bytes overhead, carrier/input ratio
  `1.014018478394` (1.401847839% overhead). It was a single release-profile
  run from a generated file source, with warm-or-unknown OS cache state; it is
  a diagnostic only, not a speed comparison;
- CPU time, RSS/PSS, syscall counts, medians/spread, cold APFS behavior, and
  Phase 4A comparison: unavailable/not run, as explicitly reported by the
  benchmark.
- The final run proves the bounded identity path directly:
  `canonical_bytes_streamed=104927319`,
  `engine_object_hash_bytes=104927319`,
  `harness_identity_hash_bytes=0`, and `identity_hash_passes=one_engine_pass`.
  The old explicit-ID API remains for the identity-mismatch trust-boundary
  tests.
- The live marker digest is incremental over the exact appended capture bytes;
  reopen still independently rereads and authenticates the committed range.
  The run reports 91,380,523 ns of digest-update work, 8,122,875 ns for
  commit-only publication, and 7,959,541 ns for the single sync. These are
  not additive stage timings because live digest updates overlap ingestion;
  the report labels them separately.
- The 5,363 ingest lookups still caused 55,240 index-page reads and 55,240
  cache misses with the bounded 32-page cache. Reopen reread 427,887,475
  carrier bytes for a 106,327,544-byte carrier (4.024239x). Those read costs
  remain unresolved and prevent a speed claim.
- The current source is a generated LCG file, not the historical SQLite
  experiment's xorshift fixture. A fair A/B must use one pre-generated source
  file and fingerprint for both engines, exclude source generation from both
  timed boundaries, publish the complete source-referencing root/member graph,
  and perform equivalent full logical reopen verification.
