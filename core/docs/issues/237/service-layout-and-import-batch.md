# #237 Service layout and import batch integration

> **Status: implemented on `codex/issue237-init-research`, not release-admitted.**
> This report describes Service source commit
> `bc944fe6347f640b6d4877f69f2d98b464c4d0ad`. The matched timing table
> below belongs to the earlier isolated prototype at `329325587`; the
> reorganized source has its own one-shot diagnostic row.

## Source organization

The Service entry point now follows the request's path rather than grouping
unrelated work under `operation/`. This is an organization change alongside the
import batch integration; public request and response formats are unchanged.

| Previous location | Current location | Responsibility |
| --- | --- | --- |
| `owner.rs` + `operation/dispatch.rs` | `service.rs` | Authorization, admission, dispatch |
| `native/config.rs` + `native/startup.rs` | `server/config.rs` + `server/run.rs` | Process setup and socket serving |
| `operation/read.rs` + query half of `operation/history.rs` | `read/content.rs` + `read/catalog.rs` | Read-only requests |
| `operation/write.rs` + command half of `operation/history.rs` | `save/content.rs` + `save/catalog.rs` | Mutating requests |
| `operation/filesystem.rs`, `metadata.rs`, role validation | `save/filesystem.rs`, `metadata.rs`, `validation.rs` | Shared filesystem construction and checks |
| `operation/history_bootstrap.rs` | `save/import/namespace.rs` plus shared metadata/validation above | Namespace initialization |
| `operation/import_native.rs` | `save/import/scan.rs` + `save/import/batch/producer.rs` | Source scan, construction, ordered batches to C2 |
| `operation/failure.rs`, history failure map, record conversions | `error.rs`, `records.rs` | Typed failures and wire conversion |
| `input/sequential.rs` | `input.rs` | Bounded request input |

`lib.rs` and each `mod.rs` contain only module declarations or exports.
`ImportBatch` is private to native import. Its sender uses portable Rust
`std::sync::mpsc`; native filesystem scanning still uses Unix metadata APIs.
The service continues to use four existing Init file constructors and one C2
Save/SQLite owner. It does not move pack payloads out of SQLite, change the
4,096-byte DB page, or change the 128-KiB whole-file cutoff.

## ImportBatch mechanism and measured prototype improvement

Each producer collects ordered `Object` and `Done` events. An ordinary batch
is capped at 256 KiB of canonical payload, 512 objects, and 512 completions;
a larger valid object is sent alone. The channel has four batch slots. The
single receiver processes events in order and still calls
`SaveHandoff::accept` once per object. The integrated implementation omits the
prototype's `LFS237` counters and logging, which existed only to diagnose the
experiment. This is a different source identity from the timed prototype.

The [one-shot matched Core pair](slab-handoff-experiment.md) measured the
prototype before this file move:

| 10k / 300 MB observation | Per-message control | Batched prototype | Change |
| --- | ---: | ---: | ---: |
| Public Init | 1,400.623 ms | 1,166.251 ms | −234.373 ms (−16.73%) |
| Public throughput | 214.190 MB/s | 257.235 MB/s | +43.045 MB/s (+20.10%) |
| Channel receives | 34,562 | 1,202 | −33,360 (−96.52%) |
| File loop | 1,194.056 ms | 974.409 ms | −219.647 ms |
| Receiver wait | 449.902 ms | 197.358 ms | −252.544 ms |
| Receiver C2 `accept` calls | 24,562 | 24,562 | unchanged |

The pairs returned the same exact root/object identities, and both full reopened
10k/300-MB readbacks passed. Both source payloads had **0/27,503 resident
pages** at the final preflight; metadata residency remained unqualified. The
control lost daemon telemetry and had a preregistered binary-hash mismatch, so
these times are exploratory raw observations, not an admitted speedup. The new
reorganized and counter-free source has not inherited a timing result from that
prototype. A separate count-only diagnostic on the batch algorithm found 115
C2 COMMITs and two C5 write transactions; those counts were not measured in the
timed candidate. Channel batching has not demonstrated the requested 90% DB
transaction reduction.

## Integrated source: one 10k diagnostic

The clean integrated source at `bc944fe63` received one performance-only
10k/300-MB public Init sample. The [prospective record](evidence/import-batch-integrated/prospective.json)
pins the source, product and harness seals, fixture manifest, output path and
cold-payload procedure. The [build record](evidence/import-batch-integrated/build.json)
pins the actual release Service and daemon binaries. No earlier timed arm was
rerun, and this standalone sample has no new matched per-message control.

| Observation | Integrated ImportBatch source |
| --- | ---: |
| Public Init | **1,110.332 ms** |
| Public throughput, 300 decimal MB | **270.189 MB/s** |
| File loop | **935.124 ms** |
| Source scan | **47.525 ms** |
| Final source payload residency | **0 / 27,503 pages** |
| Recheck-to-timer gap | **1.735 ms** |
| Closed Store + History apparent bytes | **334,249,984 B** |
| Public row | **INCOMPLETE**: Service and daemon telemetry loss |
| In-timer verification | **SKIPPED** (performance-only) |
| Separate full reopened readback | **PASS**: 10,101 paths, 10,000 files, 300,000,000 B |

The [raw receipt](evidence/import-batch-integrated/receipt.json),
[public result](evidence/import-batch-integrated/perf.jsonl),
[cold preflight](evidence/import-batch-integrated/cold-preflight.json),
[final residency check](evidence/import-batch-integrated/cold-recheck.json),
[launch sidecar](evidence/import-batch-integrated/cold-launch.json), and
[separate readback](evidence/import-batch-integrated/readback.json) retain the
evidence. The independent source copy is recorded in
[source-copy.json](evidence/import-batch-integrated/source-copy.json). Store
and History SQLite pages were both 4,096 B and Store
`small_file_threshold_bytes` was 131,072 B in the
[closed DB geometry](evidence/import-batch-integrated/sqlite-geometry.json).
The reopened root was the same `e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`
seen in the prior matched Core pair.

This integrated source's raw time is 55.919 ms below the earlier timed
prototype's 1,166.251 ms, but the source identity, instrumentation and
measurement window changed. That arithmetic is **not** a measured refactor
speedup. The prior matched pair supports only the prototype's exploratory
channel-batching result. Metadata residency is still unqualified and the
integrated public row lost telemetry, so neither row is release admission.

## Current integration checks and remaining work

The reorganized source compiled with `cargo +1.85.1 check --manifest-path
core/Cargo.toml --locked -p layerfs-service`. Warning-denying workspace Clippy,
all locked Core workspace tests and doctests, the product boundary guard (266
production Rust/SQL files), and its six tool tests passed. The enlarged native
import test covers 2,050 tiny files, one 300-KB file and a cross-boundary read.
`cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all` formatted the tree.

The one-shot prototype established a substantial channel-count reduction;
the remaining work is C2 admission/SQLite cost under a single owner. The earlier
same-source v0.1.6 cold-payload comparison was 750.626 ms / 399.667 MB/s. The
historical 518.8 MB/s observation had zero device-read bytes and did not
establish the cold-source contract, so it is not a qualifying target row.
