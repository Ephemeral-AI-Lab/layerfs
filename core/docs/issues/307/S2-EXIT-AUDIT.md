# S2 exit audit — generations and fair service

> **Status:** Completed milestone audit; S2 generation/service implementation and required covering evidence pass. Completion source is the local S2 milestone commit following `7019801f9`.

The [S2 row](../303/07-implementation-validation.md#3-slices) owns this audit.
S1 is complete at `7019801f91d3a7ba9c16ef381aa4a57474feaae5`. This closure
adds frozen capture revision, exact ticket generation, route engine affinity and
bounded custody observations to the existing generation/owner implementation.
It does not complete S0 or S3–S13.

| S2 criterion | Implementation and actual evidence |
| --- | --- |
| Incarnation routing | Every routed SQL operation validates the minted route's engine identity and namespace/incarnation. Two actual engines with identical local namespace/incarnation keys reject cross-owner tokens. AUTOINCREMENT and exact incarnation reject a reclaimed route after replacement. Authenticated remote daemon headers remain S8/S9, not this process-local capability. |
| Short shared-owner jobs | One owner thread accepts typed bounded commands; no caller closure, construction, network or whole Exec is admitted as a DB job. Startup/readback and queue lock separation are source-verified and exercised. |
| Active/captured membership and fixed EOF | Capture seals existing indexed generation rows, advances only active metadata and records the exact frozen local revision. Captured inode values/sizes, binary name cursor domain and cells do not change under later active publication. Keysets terminate at that sealed domain; existing 128/1024/4096-growth and193-name proofs pass at schema5. |
| No bulk capture copy | Capture updates one Workspace row; no INSERT/SELECT membership/payload copy, BLOB reads or mutation enumeration. Public profiles prove Payload counters unchanged; source establishes the bounded capture transition. |
| Publication/reply-attempt frontier | Mutation commits final local state plus its revision/generation ticket. Readiness waits for earlier ticket release before the one capture attempt; later same-namespace mutations wait behind the finite queue fence. Lost replies/completions cannot erase publication; ticket observations never release or resend it. |
| Install/retire | Exact capture generation/revision/base identity is validated. Known install advances base/floor, clears that capture and enqueues physical retirement in fixed work.24-cycle proof retains later inode/cell/reply state and excludes retired metadata from the live lookup range. Actual install EXPLAIN/VM and runtime counters are retained. |
| Bounded queue/results and lifecycle progress | Namespace/class rotation, configured outstanding slots/bytes and independent lifecycle reserve include queued/executing/caller-held results. Ordinary saturation still permits original ticket releases and unrelated state reads. Dropped receivers release resident result credits while backed publication/capture custody remains. Stop cancels only unattempted jobs. |
| Failure retains exact state | Changed capture revision/base, wrong route/generation and duplicate release/install are refused without altering retained state. Actual SQLite FULL during later active publication preserves prior captured identity/bytes, active state and other Workspace state. Closed captures retain immutable metadata/name/cell access until explicit known fenced release/install. Unknown SQL outcome quarantines and preserves the original nested cause/artifact; no guessed rollback/replay/cleanup. Actual device/Commit uncertainty campaigns remain later acceptance. |

## Covering source/build checks

Final closure product source passes:

- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --all-targets`:565 passed.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings`:PASS.
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check`:PASS.
- `python3 -B core/tools/check_product_boundary.py`:493 production Rust/SQL files, PASS.
- `python3 -B -m unittest discover -s core/tools -p 'test_*.py'`:25 OK.
- Actual Linux ARM64 locked overlay/daemon all-target tests:21 passed.

Raw [core](checks/s2-frontier/core-test.log), [Linux](checks/s2-frontier/linux-test.log),
[Clippy](checks/s2-frontier/clippy.log) and [first frontier diagnostic](checks/s2-frontier/frontier-profile.log)
logs are append-only. The first count diagnostic precedes route affinity; its SQL
queries/templates are unchanged. Final covering checks include the affinity source
and additional cross-owner proof; no second diagnostic sample was taken.

Rust1.85.1 and root ARM64 Cargo flags are unchanged. Linux uses image
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`,
with checkout-local `core/target/cluster2-linux` and
`core/target/cluster2-linux-cargo`; macOS uses `core/target`. Source/lock/config
hashes are retained in [identity receipt](checks/s2-frontier/identity.json).
Native kernel/global Store/runtime receipts retain their own earlier source and
platform scope. No native FUSE pass or release performance campaign is claimed.

## SQL and derived work

Retained capture uses `workspace` INTEGER PRIMARY KEY; pending tickets use
`request` PRIMARY KEY `(ns,revision)` with a terminating keyset and64-row window.
The actual production templates generate EXPLAIN. Correlated count diagnostics
beside128/1024/4096 unrelated rows stay at15/397 VM steps on macOS and14/396 on
Linux for capture/ticket-page queries, returning1/64 rows with zero fullscan,
sort, autoindex or reprepare. Ticket observation also pays one route-state point
query; the quoted ticket-family count is not a fabricated whole-operation total.

Let N be actual shared index population, K the returned keys (K<=64), W admitted
namespace lanes and J admitted jobs per lane. A custody observation is indexed
O(log N + K) work with bounded row/reply residency. Capture/install touch fixed
metadata/index records, paying real B-tree/page work. Dispatch examines at most
W lanes and5 classes; a finite fence may inspect J configured outstanding jobs.
These dimensions are explicit admission windows independent of total filesystem
entries/bytes/history. Credits include captured name/cell/ticket reply windows
and owned cursor/name capacities. Rotation guarantees service turns, not a
latency/device-throughput/process-residency bound.

There is one retained capture plus active generation; a failed operation does not
allocate another generation chain. A refused/uncertain capture remains in flight
until exact resolution, rather than allowing unbounded speculative captures.
Installed physical rows/reclaim debt can still grow until S6 supplies live-owner
eligibility/deletion; S2 does not qualify that lifetime resource behavior. The
24-cycle proof establishes logical installed-floor exclusion, not cleanup throughput.

## Remaining gates and next dependency

S2's explicit exits above are complete. S6 must supply bounded normal success/
failure resolution, independent orphan/reader custody, live-generation cleanup,
physical reservations/headroom and sustained composition. S5 still owns effective
byte validity/inheritance, cutoff truncate/regrow and sparse behavior. S7 owns
aggregate pager/journal/copy/thread/residency/resource acceptance. No withdrawn
extent/fold/pin algorithm is adopted by this primitive layer.

The fuser0.18.0 timestamp blocker remains scoped to S0 native capability/S8/S12;
no third-party exception was authorized. The next completion target is S3's
actual immutable/base-overlay view contract, followed by S4 namespace semantics.
Its content reads and engine interfaces exist; effective merge/retained-root
integration must be implemented and proved. S0 algorithm/pressure work remains
an explicit prerequisite for the owning S5/S6 slices, not an unrelated feature lane.
