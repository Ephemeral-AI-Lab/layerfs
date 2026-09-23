# #237: known-length native file construction, 10k experiment

> Prospective research protocol written before the public timed pair. This is
> a bounded C1 experiment, not a predicted solution to the full v0.1.6 gap.
> The older synthetic threshold-probe diagnostic charged only 6.560 ms to its
> complete 10k prefix sequence; public cold I/O and pipeline overlap may differ.

## Exact treatment and matched control

The control is clean Core product commit `970854f2c`. It runs the existing
`construct_stream` for each native file: reserve a 128-KiB prefix, read up to
the cutoff, then replay a full prefix into CDC for chunked files. The candidate
starts at that exact commit and changes only native Init to call
`construct_stream_known_length` with the scan-time metadata length. Files below
the cutoff use an exact-length buffer and an extra-byte check before emission;
files at or above the cutoff feed CDC directly, capped at expected length plus
one byte and checked after construction. The unknown-length API, four Init
constructors, single C2 owner, ImportBatch, pack BLOBs in SQLite, 4,096-byte
database pages and 128-KiB cutoff remain unchanged. Both arms retain open-time
device/inode and post-read size/mtime checks. The candidate's canonical IDs,
object order, content and filesystem root must match the control exactly.

The seed-1 `namespace-10000` fixture has 100 empty files, 7,899 tiny files,
1,500 small files, 500 medium files and one 100-MB anchor. Thus 9,499 files
are below 128 KiB, including 100 empty files, and 501 are chunked. The 9,399
nonempty small files are eligible for the separate signature hypothesis. This
split corrects any reading of v0.1.6's 601-file streaming bucket as 601 large
files: its bucket includes the 100 empty files.

## One paired public operation

Build both clean identities with the exact `core/benchmark/fs-bench-pro/runner.py`
multi-target locked release command and archive every executable by SHA-256.
Use the same `cold_diagnostic.py --case namespace-10000
--independent-source-copy --fixed-operation-identity` driver, fixture manifest
SHA-256 `c1d7937c9f90d3585558e6d20b35183a737530d386667d08badc3121a6b3d55e`,
fixed Stack/scope seed, Store policy, harness hash, request deadline and output
parser in both arms. Each arm gets an independent writable byte copy of the
closed master and a fresh Store. Source payload pages must be zero-resident at
preflight and at the immediate nonfaulting recheck before its own timer. The
master and copy are setup reuse only; no prior run's warm cache may serve the
measured call. Metadata/dentry residency remains unqualified. Use no host-wide
`purge` or OS-specific cache requirement.

Run the control first and the candidate second, exactly one public Init
performance sample per arm, with separate absent output paths. Do not enable
in-timer verification; record `SKIPPED`. Preserve every failure, incomplete
telemetry event and invalid cache row, with no replacement run. The public
timer is the native Init request through its StackCreated response. Keep build,
copy, cold preparation, readback and cleanup outside it. After both arms, run
the separate full reopened readback on each Store against the manifest;
readback time does not enter throughput.

Freeze before the host window: both source commits, source cleanliness, product
and harness seals, exact binary hashes, manifest hash, exact commands, absent
output paths and selected measurement window. Retain the runner's raw telemetry,
receipt, caller time, process CPU, sampled RSS with coverage, Store/History
apparent and allocated bytes, page size, pack capacity/used/slack, source cold
records, root and readback proof. Compare `history.import_files` and other
available named spans only within matching scopes. A diagnostic of C1 source
read calls/bytes, allocation requests, four producer active/blocked work and
C2 receiver wait/accept may be taken separately with identical instrumentation
on both arms; it is a labelled count-driven diagnostic, not a substitute for
the clean public pair. Never infer those absent counters from overlapping spans.

Decision: report raw one-pair time and throughput, CPU/RSS/Store changes,
canonical identity and every qualification. Reject the candidate if roots or
reopened data differ, either source has resident payload pages, or memory or
Store geometry regresses materially. Treat telemetry `INCOMPLETE` and metadata
cache unknown as qualifications rather than claiming an admitted cold speedup.
The old 0.751-s v0.1.6 row is a contextual target; this pair measures only the
incremental effect of known-length C1 construction over the integrated Core
control.

## Frozen identities and outcomes

To be appended before the public timed pair and after each arm, respectively.
