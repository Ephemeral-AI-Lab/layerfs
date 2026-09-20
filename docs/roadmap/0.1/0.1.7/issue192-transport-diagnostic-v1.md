# Issue #192 transport-only diagnostic v1

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

This prospective experiment responds to the owner's 2026-09-20 clarification:
transport should deliver multiple **decimal GB/s of payload**, separate from
C1/C2 construction and storage. It belongs to [#192](https://github.com/Ephemeral-AI-Lab/layerfs/issues/192).
It does not start or qualify [#193](https://github.com/Ephemeral-AI-Lab/layerfs/issues/193)'s
blocked direct-versus-forward product campaign, substitute for O06, or qualify a
complete file operation. Commit this specification before implementing its probe.
The interpretation used for reporting is a minimum of 2,000,000,000 payload bytes
per second; single-stream and two-stream aggregate results remain separate.

## Fixed selections

One performance sample and one separate full-byte verification per row. All rows
start NOT_RUN. There is no best-of, median campaign, warm-up or timing-based retry.
A failed selection remains in append-only evidence. New source or harness changes
need new identities; a changed workload/profile needs a new version.

| ID | Carrier | Direction relative to native host | Streams | Actual bytes per stream |
| --- | --- | --- | --- | --- |
| tdx1-tcp-upload-1 | ordinary TCP control | Linux to host | 1 | 1,073,741,824 |
| tdx1-tcp-download-1 | ordinary TCP control | host to Linux | 1 | 1,073,741,824 |
| tdx1-noise-upload-1 | public native bridge framing/Noise | Linux to host | 1 | 1,073,741,824 |
| tdx1-noise-download-1 | public native bridge framing/Noise | host to Linux | 1 | 1,073,741,824 |
| tdx1-noise-upload-2 | public native bridge framing/Noise | Linux to host | 2 | 1,073,741,824 |
| tdx1-noise-download-2 | public native bridge framing/Noise | host to Linux | 2 | 1,073,741,824 |

Raw TCP is a separate control of this host/Docker path with the same 16 KiB payload
chunk size. It is not a LayerFS product mode or a replacement for authentication.
No rate ratio between TCP and Noise is a product speedup claim. Each case has a
fresh process/container and newly established connection(s); handshakes are setup.
Each Linux stream gets one separate workload container with one CPU, 128 MiB memory
and swap limit, 16 PIDs, read-only root, no capabilities, no mounts and no logging.
Unique names use `layerfs-issue192-*` and label `io.layerfs.task=issue192`.

## Surface and roles

The probe is external example/tool code, with no production hooks or test branch.
The macOS coordinator owns lifecycle, timers, receipts and cleanup. A native host
endpoint and Linux workload endpoint exchange bytes. There is no Store, SQLite,
Workspace, spool, C1 construction, mutation or publication in this experiment.

The Noise rows exercise the actual public `connection::{accept,connect}` and
`Sender::write` / `Receiver::read` APIs with authenticated 16 KiB BODY frames and
terminal control frames. They isolate the bridge transport component; they do
not traverse the headless daemon's stdin/stdout relay or claim service-operation
performance. The real daemon route retains its separate functional evidence.

Use current source after `0819f3f39833d477d9ed6d878a50691c3c046a83`, locked core
Cargo dependencies, Rust 1.85.1, release optimization and two build jobs. For the
selected ARM artifacts use the published dependency configuration
`RUSTFLAGS=--cfg=chacha20_force_neon`; both target triples declare NEON. This does
not patch the dependency, change the Noise suite or create a crypto provider.
Native host and Linux workload binaries plus their immutable image and exact
source/harness/lock/flags must be recorded. No legacy binary fallback is allowed.
Application frame, session and deadline limits remain unchanged.

## Payload and cache contract

Transmit every declared byte. There is no compression, deduplication, sparse data,
file fixture, precomputed result or whole-payload allocation. Generate each 16 KiB
block inside the timed transfer: little-endian u64 word `seed + word_index`, with
wrapping arithmetic, seed `0x1922026092000000 + stream_index`. Blocks are distinct
within a stream. Both carriers use the same recipe and payload boundaries.

The source is generated memory, not file-backed data: OS source-page invalidation
is inapplicable. The generator fills every transmitted block during the timer;
there is no payload pre-touch, acquisition from earlier writes or expected-result
cache. Reuse fixed-size transport buffers only. Declare this as generated-input
cache state, never cold-storage throughput. The measured numerator is actual
received payload bytes, not an implied logical or compressed size.

## Timing and acknowledgement

Use one host monotonic `Instant` for a selection. Setup includes process/container
launch, configuration, authentication and all streams ready. The timer starts
immediately before releasing those streams and includes start control, input
byte generation, all frame/crypto/socket work, backpressure and final receipt
acknowledgements. It ends only after every stream confirms the exact final byte
count. No wall-clock or cross-host Instant subtraction occurs. Two-stream rate
uses total bytes divided by this common interval, never the sum of independent
per-stream rates.

Performance mode performs framing/length/authentication validation and byte
counting, with zero benchmark full-byte comparisons or digests. Verification is
a separate invocation using identical artifacts and selection; it compares every
received word to the declared absolute-index recipe and rejects any mismatch,
truncation, excess bytes or missing terminal. An independently computed small
host oracle checks the recipe, and a deliberate corrupt-input self-check must
fail. Verification results are not throughput samples.

Retain `payload_bytes`, `transfer_ns`, stream count, frame count, handshake count,
setup/cleanup/complete-command wall and the formula
`payload_bytes * 1_000_000_000 / transfer_ns` as
`received_payload_bytes_per_second`. Human output also gives decimal GB/s and
Gbit/s and clearly labels n=1. No throughput extrapolation or exact phase-memory
claim. Process/cgroup/native observations, when collected, keep their own scopes.

## Budgets, gates and evidence

Each complete performance command has a 15-second hard budget; verification has
its own 60-second hard budget. Build/image setup is separate, incremental and
source sealed. No timeout increase, workload reduction, extra construction worker
or buffer-limit relaxation to turn a miss into a pass. Capacity is two stream
workers at most; no unbounded tasks/queues or payload files. Acquire both shared
infrastructure locks in their prescribed order, never private benchmark markers.

A row is usable only with exact byte/terminal counts, successful matching
verification, matching source/binary/harness/config identity, complete output and
cleanup PASS. The 2 GB/s goal is reported separately for each direction and stream
count; reaching it in two streams does not qualify one stream. Unsupported or
missing domains are unavailable, never zero. Record failed/no-go rows as plainly
as passing rows. Every row remains `admission_eligible=false`: this is a scoped
component diagnostic, not release evidence or an admission of #193.

Write fresh receipts under the issue #192 implementation evidence directory,
including raw JSON and reproduction commands. Keep all prior end-to-end failures
and successful smaller functional cases unchanged. Report transport-only results
beside their topology and exclusions; never relabel them as C1/C2 or full-daemon
throughput.
