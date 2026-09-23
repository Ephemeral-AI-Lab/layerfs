# #237 SDK 100k Init diagnostic contract

> Frozen before the first 100,000-file SDK call. This is one unregistered,
> admission-ineligible Cargo **debug** diagnostic, separate from #236 SDK v2.
> The #236 two-case selector, 15 s command and 5 s verifier gates, and its
> `namespace-100000` `NOT_RUN` row remain unchanged.

## Selection and operation

The sole `core/benchmark/fs-bench-pro/runner.py` accepts an explicit
`--diagnostic-100k` selection. Receipt schema
`core-fs-bench-pro-sdk-init-100k-diagnostic-v1`, selection status
`UNREGISTERED_DIAGNOSTIC`, one sample, seed 1, case `namespace-100000`, route
`host-direct-sdk-v2`, and fixture `core-sdk-init-fixture-v2` are fixed. The
existing case plan is 100,000 files, 1,000 data directories plus root,
500,000,000 logical bytes including two 100,000,000-byte anchors, with class
counts `(2, 1000, 78998, 15000, 5000)`. Use its SHAKE bytes and manifest.

The debug `benchmark_init` example creates `Host` then times exactly one public
`Client::init_project` call with Rust `Instant`. Source scan/read, C1/C2/C5,
and publication are inside the timer. Python records the complete driver
process wall separately. Build and independent verification are outside both
timers. Four Init producers, one C2 save owner, the 128-KiB whole-file cutoff,
SQLite BLOB packs, and 4,096-byte SQLite pages are fixed product inputs.

## Preparation, bounds and outcomes

Prepare and seal one v2 source master outside timing. For the attempt, make an
independent writable byte copy and validate every manifest path, metadata,
size and SHA-256 before timing. Invalidate source payload pages after setup
reads. A nonfaulting whole-input residency check must show 100,000 files,
500,000,000 bytes, expected page count from each file's size and host page
size, and zero resident pages. Check again immediately before driver launch,
recording the elapsed gap. Refuse the call if any check fails or the host has
no proven invalidation/residency backend. Directory/inode metadata residency
is unqualified, so there is no fully cold namespace or numeric latency PASS.

The **610 s outer driver watchdog** and **600 s separate verifier watchdog**
are predeclared safety limits, not performance gates. They retain the
product's independent 600 s Service deadline or its outcome, plus teardown.
Do not widen a bound after a miss or rerun an unchanged arm. Preserve timeout,
unknown, failure, partial and `NOT_RUN` receipts. Do not promote this result
into #236 or compare it as a matched speed arm with daemon-host/release rows.

If the driver confirms a root and exits, run the full independent verifier
once, reopening C5/C2 and checking 101,001 paths, 1,001 directories, 100,000
files, 500,000,000 bytes, portable metadata and every SHA-256. A completed
diagnostic requires that exact oracle result and clean driver exit/cleanup.
Report a failed or timed-out oracle as such; operation time stays raw evidence.

## Required evidence

Pin source commit/tree, product/harness/Cargo/config seals, build command,
debug binary SHA-256, fixture and source-copy identity. Retain source-copy,
cold preflight and immediate launch checks, raw driver stdout/stderr and
`operation_ns`, complete command wall, exact typed result or error, one-call
count, subprocess exit and timeout, competing work and cleanup status.
Record external driver lifecycle user/system CPU and per-child peak RSS with
host unit normalization. Neither is an operation-phase resource measure.

After close, report apparent `st_size` and allocated `st_blocks * 512` for
Store and History, each database's `PRAGMA page_size` and `page_count`, Store
pack-row count and `sum(length(data))`, and any retained scratch. The full
verifier's wall, stdout/stderr, exit and outcome are separate. Keep a fresh,
append-only result path and a hash manifest of the retained receipts. A
successful raw throughput is `500000000 / (operation_ns / 1e9)` decimal
bytes/s, explicitly labelled diagnostic and admission-ineligible.
