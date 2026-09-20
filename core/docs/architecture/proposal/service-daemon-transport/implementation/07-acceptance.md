# Issue #192 implementation and qualification record

> **Status: INCOMPLETE candidate, not issue closure or a performance qualification.**

The implementation branch is `codex/pair3-foundation`, based on
`10b9d4a6cf9d88267d508cb010cc82950e080d77`. The reviewed proposal snapshot is
published at `21f6af702919c23fafc88890361bef7bfb831140`; its 19 selected-file
hashes and concurrent-change checks are in [spec-custody.json](spec-custody.json).
C1 incorporates only the four committed files in
[optimizer-checkpoint.json](optimizer-checkpoint.json), from
`e54af84653cd1a22b3f26631dbd19eafb895a7b4`. C2 remains at the base revision.
No uncommitted optimizer source, v0.1.6 control, or Stage 6 receipt was imported.

The [runtime architecture](../../../14-service-runtime.md) documents the public
APIs, operation/input/result schemas, configuration, ownership and limitations.
[M0 decisions](06-m0-decisions.md) document the selected native carrier,
security and numeric limits. These are v0.1.7 candidate interfaces; Cargo's
inherited workspace package version remains 0.1.6 and is not a release claim.

## Evidence conventions and reproduction

Every command receipt names its exact command, elapsed complete-command wall
seconds, exit status, target directory and source inventory. A `source.json`
contains SHA-256 values for source/manifests/lockfiles and external tests at that
attempt. Source was uncommitted atop the published specification commit during
qualification; the HEAD field alone is not its implementation identity.
Deployment receipts additionally identify the immutable image, native service
binary, Linux binary, lockfile, real container/network configuration, process
identities, result roots and cleanup. None of these times is a performance claim.

Run each selected command independently from the implementation worktree, with
`CARGO_TARGET_DIR=$PWD/core/target`, `CARGO_BUILD_JOBS=2` and
`LAYERFS_CONSTRUCTION_WORKERS=1`, after acquiring the shared measurement locks.
There is no aggregate gate. Docker test arguments are the image ID recorded in
its receipt, the stated `--selection`/`--telemetry`, and a **fresh** `--output`.
Existing receipts and unsuccessful attempts must never be overwritten.

The first full-workspace command reached a 60 s controller watchdog after
29.35 s of compilation. Its cached retry also reached 60 s in the existing C1
`edit_reference` target, without an assertion failure reported before the stop.
These are incomplete full-workspace proofs, not passing suites. No timeout was
increased and no existing C1 fixture, workload or algorithm was shortened.

Controller `cleanup: PASS` means owned processes, containers and temporary host
fixture files were released. It does **not** by itself prove C2 save abort.
The separate cleanup-fault receipt tests C2 error/ownership behavior.

## Product cases

A PASS below applies only to the named selection and recorded artifact identity.
Older source/binary receipts are retained with their original scope; they are not
silently promoted to later revisions. All Docker cases use actual Linux daemons
and a native macOS service unless a secondary/external-peer scope is named.

| ID | State | Actual evidence and qualification |
|---|---|---|
| V01 | PASS | `qualified-forward-20260920`: authenticated actual daemon/network/service route, observed host peer, image/container identities. |
| V02 | PASS | Route: empty, 131071/131072/131073 cutoff boundaries and 500000-byte multi-frame construction, canonical roots and readback; successful C2 finish before Saved. |
| V03 | PASS | Route: exact readback/zero range; `faults-errors`: out-of-range error; `faults-slow-corrected`: unread partial output receives no accepted terminal success. |
| V04 | PASS | Route: File, Stat, two paged Lists, Readlink; `faults-errors`: path/object absence remain distinct. |
| V05 | PASS | Route: no-op, insert/delete/replace and old root; `faults-additional`: invalid coordinates, overlap, arithmetic/replay caps before body acquisition. |
| V06 | PASS | Route: existing-identity attachment; `faults-additional`: changed-name update, original root, scope/allocation rejection. Malformed-order extremes remain covered by C1's public validation, not a new native campaign. |
| V07 | PASS | Route: exactly two saves plus one tree attachment, two retained root slots, both file identities checked; separate operations without atomic composite semantics. |
| V08 | PASS | `protocol.rs` deterministic one-byte fragmentation/length/count/arithmetic/truncation checks; `qualified-framing`: actual Docker truncated headers/bodies, empty Body, wrong ID and surplus frames refused; `faults-additional`: authenticated external peer illegal state/ID/metadata rejection. No claim of an exhaustive ciphertext fuzzer. |
| V09 | PASS | `faults-errors`: Store/op denial, unsupported profile; `faults-additional`: untrusted key, unknown opcode, external authenticated malformed peer. No fallback/retry. |
| V10 | PASS | `faults-slow-corrected`: requested 250 ms upload/output deadline; `independent-stderr`: 200 successful calls while diagnostic consumer is unread. These are short functional cases, not a maximum-duration stress qualification. |
| V11 | PASS | `faults-errors`: read-only principal refused a declared 64 MiB construct before any body, process exit without draining stdin. |
| V12 | PASS | Incomplete input and deadline receipts plus `faults-lost-response`: encrypted final result withheld, daemon Unknown, successful saved root subsequently readable, no replay. |
| V13 | PASS | `faults-admission`: four real daemon sessions, fifth refused; one active mutation, excess Capacity, Q=0. This does not claim simultaneous C2 writers. |
| V14 | PASS | Route: new native service process and new Docker daemon read old/new roots after ordinary Store reopen; no crash durability claim. |
| V15 | PASS | `faults-cleanup`: externally installed SQLite cleanup trigger, original and cleanup errors retained, further mutation Ownership refusal, old root readable. Unknown outcomes do not trigger guessed deletion. |
| V16 | PASS | Primary forward receipts: no mounts/devices, read-only root, empty Docker diff, log driver none. Local/both explicitly separate. |
| V17 | PASS | Route direct example: same authorized Service handler; pristine independent Store copy, identical canonical file/edit/tree roots and inserted/reused counts. |

## Telemetry cases

| ID | State | Actual evidence and qualification |
|---|---|---|
| T01 | PASS | `final-candidate-tests-20260920`: all existing timer, format, JSON, attachment, panic and compile-fail tests pass on final source. |
| T02 | PASS | External master-off allocator and independent native selection tests passed; selected CPU/RSS bits distinguish disabled from unavailable fields. Normal application configuration precedes the zero-allocation boundary. |
| T03 | PASS | `qualified-off`, `qualified-forward`, `qualified-both` have identical canonical roots and save counts, verified in `mode-parity-20260920.json`; native failure tests preserve original product errors. |
| T04 | PASS | `tests/observations.rs`: cumulative arithmetic, unavailable data, counter/incarnation changes, sampled max and sibling scope. Native CPU-unit regression independently brackets POSIX counters. |
| T05 | PASS | Typed 10000-observation constant-size window, plus native one-slot raw-ring wrap with live window retention. Default-duration OS stress is NOT_RUN. |
| T06 | PASS | External recorder/window tests preserve original errors/panics, release slots, and omit detail at capacity; live windows are not age-evicted. |
| T07 | PASS | `timer_bounds`, external off/owned allocation tests: normalized labels, Vec/String retained capacities, pool custody through report drop and two hosted full recording pools. Caller-created arbitrary clones remain caller-owned. |
| T08 | PASS | Native JSON parse of clipped report within 1024 bytes; bounded identity/outcome retained, valid timing projection rather than byte truncation. |
| T09 | PASS | Native bounded queue overflow/in-flight accounting, rate profile, failed output and fixed loss counters; no retry spool. |
| T10 | PASS | Real local/both route and shared Runtime clones; aggregate queue/rate configuration validation before allocation. Docker collector uses separate output attachments. |
| T11 | PASS | Native producer churn, finite registration refusal, queued ownership retaining closing registrations and release after output shutdown. |
| T12 | PASS | Native count/length limits, active segment included, expiry via external file timestamps, exclusive writer lock, restart and unrelated-file preservation. |
| T13 | PASS | `qualified-enospc` requalifies real HFS+ ENOSPC on current source and preserves Err(42); current macOS/Linux retention tests cover denied path, deletion failure and open readers. Logical file lengths do not bound externally retained physical blocks. |
| T14 | PASS | Native test preserves adjacent benchmark evidence; all acceptance attempts use exclusive fresh paths and keep failures. No operational GC targets benchmark evidence. |
| T15 | PASS | `selected-native-check`: unread native stderr stalls the writer, a failed sink write is counted, original Err(42) survives, and the subprocess exits under its watchdog. Docker unread-collector test is separate. |
| T16 | PASS | `portable-telemetry-graph`: no dependencies with defaults disabled; `portable-bridge-check`: contract builds without native; explicit native macOS tests, Linux daemon/native-test builds and actual Linux retention execution passed. |

## Platform cases

| ID | State | Actual evidence and qualification |
|---|---|---|
| ENV01 | PASS | Actual macOS safe POSIX CPU microseconds converted to ns, libproc RSS/start identity, source/probe duration, process windows. Earlier raw libproc CPU fields are explicitly INELIGIBLE in the appended disposition. |
| ENV02 | PASS | Actual Linux musl process, procfs/sysconf source, container identity and Docker Engine cgroup anon/file/kernel/socket observations where available. No addition of enclosing cgroup and process RSS, no phase-peak claim. |
| ENV03 | PASS | Forward stderr structured envelopes, separate Docker Engine diagnostic attachment, bounded host collector drops; primary container has no telemetry files. |
| ENV04 | PASS | `qualified-both` validates actual owned directory/file permissions; `final-local` separately exercises local mode; Linux native retention image tests rotation/expiry/open readers/failure/cleanup under a 32 MiB container limit. Logical writable-layer size is recorded, phase memory peak unavailable. |
| ENV05 | INCOMPLETE | Two hosted producer pools measured with external allocator: 462540-byte additional high-water for the recorded selection, within 16 MiB combined allowance. Actual connection admission and cgroup observations exist. Full simultaneous maximum producer/collector/encoding/stack/socket envelope has not been empirically qualified. |
| ENV06 | PASS | Actual normal/reopen/deadline/blocked-collector exits plus native stalled-writer subprocess pass. Blocking filesystem calls remain nonpreemptible; delivery is best effort. |

## Known failed attempts and limitations

- Initial network proof found macOS accepted sockets inheriting nonblocking mode;
  the native adapter now normalizes accepted descriptors. The failed receipt remains.
- An early driver supplied `/f` instead of canonical relative `f`; the original
  failed attempt remains and the driver grammar was corrected.
- Raw libproc CPU fields on Apple silicon are Mach ticks, not ns. The failing POSIX
  bracket test, corrected safe getrusage test and appended ineligible disposition
  remain. Original receipts were not rewritten.
- The first slow-upload test wrongly demanded a terminal failure after the output
  deadline. Missing terminal delivery is an unknown caller outcome. The corrected
  250 ms selection preserves this distinction, and the original failure remains.
- The ordinary Docker attachment blocked stdout when the stderr reader stalled.
  The coordinator now uses independent product/diagnostic attach connections. The
  successful 200-call unread-consumer check does not by itself prove native writer
  backpressure; the dedicated subprocess test supplies that separate proof.
- A parallel native test-directory timestamp collision was fixed in the external
  helper with a per-process atomic suffix. Product namespace checks were retained.
- Required sync/durability, FUSE, Workspace, inode allocation, history, cloud and
  the #193 performance driver/campaign remain out of scope. No performance gain,
  reset phase peak or CI status is claimed.

Issue #192 remains open while required matrix/final checks are incomplete. The
branch can be reviewed as an implementation candidate; #193 is not admitted to
performance qualification by this record.

## Resource coordination correction

An early #192 helper incorrectly used flock on private harness O_EXCL markers,
creating empty files and causing refused benchmark launches. The sibling preserved
and cleared its own stale markers. Under both shared global flocks, #192 preserved
and removed the empty primary marker only after inode, size, lsof and process
checks; see `evidence/lock-protocol-correction-20260920.json`. Live markers were
untouched. The helper now uses only TMPDIR and /tmp infrastructure flocks in that
order, deduplicated by resolved path. Private markers belong solely to the harness.

Worktrees and Cargo targets are separate. #192 containers use unique
`layerfs-issue192-*` names, label `io.layerfs.task=issue192`, one CPU, 128 MiB
memory/swap and 16 PIDs (the tiny external retention container selects 32 MiB).
#190 history measurements run natively and have no competing containers. Separate
containers prevent container ownership collisions; they do not isolate shared
macOS CPU/disk measurements. No active benchmark was interrupted or relabelled.

## Final check disposition

- PASS: final workspace examples check; warning-denying workspace/all-targets
  Clippy after the external full-disk range-expression lint correction; workspace
  rustfmt check; product boundary guard (171 production Rust/SQL files); all six
  Python guard self-tests. Exact command receipts are under `evidence/final-*`.
- PASS: `final-candidate-tests` exercises bridge/service/daemon/telemetry, including
  explicit native selection and existing timer compatibility. The dedicated
  ENOSPC test is normally ignored and was run explicitly on its disposable volume;
  the stalled-writer child is an ignored helper invoked by its passing parent.
- PASS: `c1-checkpoint-tests` runs all 17 filesystem-bounds and four parent-lookup
  tests for the exact four imported checkpoint files. Their hashes remain unchanged.
- INCOMPLETE: the exact full-workspace test invocation reached the 60 s watchdog
  in existing C1 reference tests twice; there is no complete full-workspace PASS.
  No third attempt, timeout increase, aggregate gate or CI claim was made.
- INCOMPLETE: ENV05's complete simultaneous maximum resource envelope remains
  unqualified. Component limits, selected heap observations, actual admission
  and short failure cases do not replace that physical proof.

The implementation and its focused functional checks are reviewable. These two
remaining qualification items prevent closing M6/#192 and admitting #193's
performance campaign. Historical failures and diagnostic limitations remain in
this record, not relabelled as successful final checks.

## Production LOC per commit

Counter: `tools/production_loc.py`, SHA-256
`c6e853c2280e2ee96caa6cafff4b221fa9201e1f5bf40d12e8251f70387210fc`.
Count exact first-parent and final staged Rust/SQL snapshots with the same source
classification; exclude reference inline tests, external tests/examples, tooling,
docs, manifests, generated artifacts and Docker deployment configuration. New
bridge/service/daemon runtime source belongs to the core subtotal. External
application adapters are zero; this is not a scope deletion or performance claim.

| Commit | Combined | Reference | Core | External adapters |
|---|---|---|---|---|
| Published spec `21f6af702` | 85533 → 85533 (+0) | 65417 → 65417 | 20116 → 20116 | 0 → 0 |
| Accompanying implementation commit | 85533 → 89766 (+4233) | 65417 → 65417 | 20116 → 24349 | 0 → 0 |

The implementation delta is bridge +1696, service +768, daemon +234, telemetry
763 → 2249 (+1486), and the explicit C1 checkpoint +49. C2 stays at 6841.
[Exact counter report](evidence/production-loc-implementation.json) and the
commit message record method/scope; final committed tree identity is confirmed
against the counted staged tree. Tests/docs are excluded even though their Git
diff is much larger than runtime source growth.
