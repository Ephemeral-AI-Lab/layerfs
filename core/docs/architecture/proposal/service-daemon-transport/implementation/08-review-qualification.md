# Issue #192 review and short-test qualification

> Status: historical review receipts before the optimization revision; not current qualification.

All PASS statements below apply only to their named historical source inventories.
The schema-7 multi-writer revision, ordinary TCP and release artifacts require
new evidence. [Optimization decisions](09-optimization-decisions.md) supersede
the earlier bounds. No image or binary below may be selected as a fallback.

This follow-up starts at implementation candidate `5f637f17c17d134deb6b567dfcbd290b112457d8`
on `codex/pair3-foundation`. The published specification remains
`21f6af702919c23fafc88890361bef7bfb831140`; C1's four-file checkpoint remains
`e54af84653cd1a22b3f26631dbd19eafb895a7b4`, and C2's product source remains
`10b9d4a6cf9d88267d508cb010cc82950e080d77`. No sibling task communication, private
measurement-marker access, third-party modification, or optimizer source import
was used for this follow-up. Commands acquire both global infrastructure flocks.

## Findings and fixes

- C1 `edit_reference` repeatedly rediscovered deterministic fixture lengths by
  constructing candidate files. The test now reads the lengths from its existing
  sealed oracle, retaining exact bytes, edit tuples, roots, partitions and surviving
  identities. The old 55.33 s execution becomes 3.48 s in the focused receipt.
- C1 `edit_bounds` now uses exact lengths captured from its original search;
  every extent-count check remains. Immutable prepared Store data can be reused
  between edits, with an independent result Store for each case. The original
  18.39 s target becomes 5.32 s. No product algorithm or C1 oracle was changed.
- C2 `codec_frames` allocated a one-MiB decoder arena for every byte cut.
  Reusing the public decoder retains every truncation and typed-error assertion,
  and adds a valid-frame check after all errors. All seven tests take 0.04 s in
  the focused receipt; this is test setup reuse, not a product speed claim.
- C2 `metadata_window` streams its unchanged 131,200 distinct values through four
  saves rather than 1,312. First, pre-boundary, crossing and final leaves have
  separate save boundaries. Exact catalogue groups/ordinals, eviction, cross-save
  reuse and reopened readback assertions remain. This reduced setup version took
  14.21 s before the test-only codec compilation change.
- A diagnostic sampled the remaining window cost in Zstandard compression. A
  structured-ID fixture experiment was slower and stopped at 44.98 s; that
  receipt stays FAIL and its data change was reverted. The final fixture has its
  original values. The codec-only test profile reduced that target to 4.85 s, but the
  complete suite still exceeded its cap, including substantial executable-start
  overhead outside the test-harness timers. The final workspace uses test
  `opt-level=1`, with `zstd-sys` at 3. C2 compression/transaction policy, dev/release
  profiles, Rust test debug assertions, overflow checks, dependencies and workload
  cardinality stay unchanged. Test timings across these compilation identities are explicitly
  different; none are Stage 6 measurements or performance qualification.
- Telemetry final-handle destruction used a racy `Arc::strong_count` check.
  `Output` now owns one `Arc<Owner>` whose destructor stops the worker exactly
  once; the worker holds only shared queue state. An external concurrent-drop
  test observes release of the real exclusive output namespace. Submission also
  rechecks closure while holding the queue lock, preventing late records from
  being stranded after a closed/empty writer exits.
- CLI `Result` termination and ordinary `eprintln!` diagnostics could block exit
  behind unread stderr. Native ordinary diagnostics now attempt one <=512-byte
  nonblocking write. Both executables return explicit exit codes. The regression
  establishes a real host service/daemon connection, fills its stderr pipe, then
  supplies invalid input: error cleanup exits in 0.001 s. This qualifies running
  process error cleanup, not a one-second process-launch guarantee. Earlier
  launch-budget failures and the later diagnostic PASS are retained separately.

## Resource proof scope

`owned_allocations.rs` uses an external allocation observer and public APIs. It
holds a producer queue, the full eight-record pool (256 nodes, 32 depth and
128-byte escaped labels), 32 active process windows, a maximum eight-MiB collector
queue and 16 closing producer registrations. Eight encoder workers synchronize so
encoded owners overlap retained recordings and queues. Two hosted components
share one recorder and sampler. The collector adds no second sampler.

The native proof separately records allocator-owned bytes, process RSS, virtual
mapping size, observed threads and explicit stack requests. They are different
accounting domains and are never added as exclusive operation memory. It checks
both count and allocation-capacity queue limits, refusal at the next recording,
window and producer, release of closing registrations, post-shutdown loss and
bounded cleanup. Full queues use touched spare Vec capacity while preserving
valid encoded envelope lengths. No hidden product hooks or changed limits exist.

The real four-daemon `envelope` deployment complements the co-hosted ownership
proof with native process/container identities, Q=0 refusal, distinct results,
actual service sockets/thread observations, Docker cgroup domains and configured
stack requests; socket option readings do not bound kernel physical memory. It is not a throughput or reset phase-peak measurement.

## Evidence handling

All `evidence/review-*` command attempts are retained with their exact command,
source inventory, status and complete wall time. Timeout/diagnostic attempts
are not promoted to PASS. Full-suite retries occurred only after new test setup
fixes; the timeout was not increased. Product telemetry/resource assertions use
original frozen bounds. No benchmark receipt, workload or performance claim was
modified. The final disposition and source/binary/image inventory follow below.

## Completed workspace and platform checks

The final test-profile complete command is `review-short-workspace-cached-20260920`:
**512 passed, 0 failed, 3 ignored, 15.050893 seconds**. The two ignored subprocess
helpers are invoked by their passing parents. The ignored real-full-disk case ran
explicitly in `review-enospc-command-20260920` on a guarded disposable 32 MiB HFS+
volume; its ENOSPC result preserved `Err(42)` and the volume was detached.

Final test-harness execution observations include C1 reference about 0.21 s,
edit bounds about 0.50 s, and metadata window about 1.97 s. These are routine
unit-test turnaround observations using compiled artifacts. The first invocation
of the freshly rebuilt suite exceeded 60 s while completed target timers totaled
6.09 s; the remaining elapsed time was outside those target timers. That FAIL is
retained. No cold-start or product-throughput conclusion follows from the cached
suite PASS. The initial test-profile build used a 60 s interrupted slice and a
38.50 s incremental continuation, separately recorded from test execution.

The native feature test explicitly passed on macOS (`review-native-final`),
including all eight native cases, the maximum ownership child and stalled-output
child. The current maximum observed owned heap was 11,112,366 bytes on macOS and
11,091,214 bytes on Linux, under the 25,165,824-byte combined allowance. The
Linux maximum child completed in 1.04 s in a separately declared 64 MiB / 16 PID
instrumentation container. MacOS sampled RSS was 25,231,360 bytes; Linux sampled
RSS was 13,537,280 bytes. These RSS observations are separate from owned heap,
thread stack requests and virtual mappings, and are not exact phase peaks.

The real maximum-connection deployment (`review-envelope-deployment`) passed in
6.67 s: four authenticated Linux daemons, one held service mutation, fifth-session
refusal, A+1 immediate refusal, distinct per-daemon results, seven observed service
threads and four sockets (each socket has two descriptor handles). Actual `vmmap`
reported 16.7 MiB of Stack mappings with 400 KiB resident at the sample. Container
cgroup observations and configured socket/stack limits are in the receipt.
No shared volume, mount, payload file or telemetry file was used in forward mode.

Six Linux native lifetime/monitor/retention cases passed in 0.12 s. The broader
scratch-image attempt retained one FAIL because `python3` is absent from scratch;
its JSON-parser-dependent test is explicitly omitted from the scoped rerun.
The same test passes on macOS, and actual Linux forwarded/local reports are parsed
independently on the host in the forward/both route. No product fallback or Python
installation was added to the daemon image. Linux failed-sink ownership/cleanup
also passes in the maximum-envelope child.

The last recorded daemon image for that historical selection is
`sha256:bab1522c12fd2b5f2dc75b21102f08190ea473faa3f15cb2baf3143afc8d7b0a`,
with Linux binary SHA-256
`669060a924842560c4afb56a99ad5bb35cba8bfa47c12eae00b9c000b8752399`.
The host service and this image use the unchanged unoptimized dev compilation
profile; the test-only optimization does not qualify production performance.
