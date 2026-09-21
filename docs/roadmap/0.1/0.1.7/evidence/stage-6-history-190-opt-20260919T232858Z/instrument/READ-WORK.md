# Read-work observation (#190)

The harness wraps each filesystem StoreProvider and the separate content predecessor
StoreProvider with an opt-in observer under `LAYERFS_HISTORY_PHASES=1`. It forwards
exactly the original ordinary/scoped provider method and returns its result unchanged.
No product source or storage error mapping is copied. No object IDs or returned bytes
are retained by the observer. No per-read telemetry nodes are added.

Per state, `filesystem.provider` and `content.predecessor_provider` report independent
`read_waves`, `requested_objects`, `returned_objects`, `returned_canonical_bytes`,
`failed_waves` and `read_elapsed_ns`. Elapsed time covers the delegated method only;
counter aggregation is outside that duration but inside the state's measured phase.
It overlaps the filesystem/content enclosing phase and must not be added to it.
Empty demands and failed calls count as waves. Failed results have no returned bytes.
The constant-size counters do not deduplicate demands: repeated acquisitions count.

Existing filesystem StoreProvider `connection_opens` and `group_decodes` remain.
Pack reads, compressed bytes and physical disk reads are **not measured**: those are
not exposed by the existing AuthenticatedObjects call. StoreProvider's alternate
`read_wave` exposes pack counters but would require reproducing a private error
conversion, so this observer deliberately preserves the original dispatch instead.
Returned canonical bytes must never be labeled pack/disk bytes. A successful full
state publishes these counters after its timing scope ends; a failing state does
not currently retain its partial observer counters.

The external `tests/history_read_work.rs` check exercises result/demand preservation,
ordinary vs scoped forwarding, MissingObject/ProviderFailure/IdentityMismatch errors,
exact deterministic counters and disabled observation. No wall-clock threshold.
Execution and actual measurements are coordinated by the campaign root; this note
contains no timing result and does not claim a check ran.
