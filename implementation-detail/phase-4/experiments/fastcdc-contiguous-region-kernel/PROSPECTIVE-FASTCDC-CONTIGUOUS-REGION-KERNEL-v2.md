# Prospective FastCDC contiguous-region kernel v2

Status: **FROZEN BEFORE THE FIRST SCREEN ROW**. This is the final authorized
safe-Rust exact-boundary CDC candidate. It preserves the sealed v1
active-mask-field `NO-GO / REVERT`; it does not relabel v1 or modify its target
root. If this v2 screen saves less than 8.000 ms, serial safe-Rust
exact-boundary CDC tuning closes.

## Custody and one variable

| Item | Frozen value |
|---|---|
| repository / branch / checkpoint | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` / `codex/empty-worktree` / `daf4cefc1fd7861681de3f94bf042b556cc21ccb` |
| accepted Canonical-v2 source / profile | `16e9beedd2fe49d6da65f89f53f488cffbfdcfc71f10477e854cd2d37d00e120` / `94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b` |
| accepted durable executable | `f3dd4c9420cc7bb7e7390960db9bf6e4a4a44de3d15dc0573002d3172b570280` |
| control `cdc/mod.rs` | `82d8463101675e8f0e5632b532a3a96893405adaa09d311fddb25ca322620940` |
| candidate `cdc/mod.rs` | `bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6` |
| exact candidate diff | `72ed9fee8e6a203a15d88df8e1c555f13a52a8ed4f0ef5eabdad742e0b8a3d76` |
| screen control / candidate | `2368ce9e3ca5dad69a593e2f5f7a78730d9685f6564f8816cac1be33574e628f` / `ba61681005f996213d10a071955101ad8f691ad7012b63e65df8b54d27d84139` |
| candidate durable executable | `454bc2f3deacd8581a3cc352c8b7495215cdc103a85580606246ea12bb25eba8` |
| screen harness | `a5293e4479d4e9160a3b0c4e161f4ef42e9604446a8ca0410a001f7386d6f187` |
| machine-code preflight | `988e6694d27fe95e788661e9080df53308bddc3d3ee4abe847b273a03727be50` |
| retained fixture | 104,857,600 bytes; SHA-256 `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4`; BLAKE3 `bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7` |
| artifact root | `target/phase4-fastcdc-contiguous-region-kernel-20260821-v2` |

The one product variable is the contiguous region kernel. `Scanner::consume`
bulk-fills the existing bounded chunk to 8 KiB, resolves a retained pending
byte once at fragment entry, and calls one fixed-mask region kernel for the
small region and one for the large region. Hash, cursor, slice limit, and masks
are loop-local scalars. The accepted slice is appended to the existing chunk
`Vec` only at a cut, maximum, or fragment exit. One odd trailing byte becomes
the unchanged pending byte. The callback and `finish` behavior are unchanged.

The candidate retains the exact Gear table/recurrence, first-byte then
second-byte judgments, four masks, normalization, 8/16/32-KiB sizes,
boundaries, bytes, fragmentation, errors, callback precedence, and 32,768-byte
capacity. It adds no dependency, unsafe code, worker, thread, task, queue,
pool, SIMD framework, schema, identity, profile, durability, or storage change.
Cached mask fields do not return. Pending-only, larger reads, unrolling,
compiler flags, and any second implementation are outside this cycle.

## Focused validation and build

Before release construction, these passed:

```text
cargo test -p layerfs-core --offline cdc::tests
cargo test -p layerfs-core --offline cas::tests::scan_identity_deduplicates_and_preserves_callback_bytes
cargo test -p layerfs-core --offline content::tests::full_replace_is_a_separate_streaming_path
cargo test -p layerfs-core --offline --test canonical_v2_fixture_oracle independent_actual_fixture_oracle_freezes_complete_v2_corpus
cargo check -p layerfs-core --offline
cargo fmt --all -- --check
git diff --check
rustfmt --check implementation-detail/phase-4/experiments/fastcdc-contiguous-region-kernel/fastcdc_region_screen.rs
```

The tests cover the retained 100-MiB 5,284-occurrence oracle, frozen small
vectors, empty/short/minimum/maximum edges, five existing fragmentation
patterns, callback failure propagation, exact reconstructed callback bytes,
no empty/duplicate/over-maximum chunk, and fixed capacity.

Control and final candidate screen binaries were built from independent clean
checkpoint archives plus the identical screen harness. The final candidate
screen and durable binaries were built together once after the machine-code
source-shape correction. Complete Cargo build directories are temporary and
are excluded from the v2 artifact root and terminal manifest.

An initial unmeasured v2a preflight found LLVM rematerializing the small masks
inside the loop. Its operands are retained only as code-shape rejection
evidence under `preflight-rejected-v2a`; no timing occurred. The final source
uses one out-of-line region kernel with masks passed in registers.

## Frozen machine-code gate

The read-only final preflight passed every gate:

- control and candidate `timed_scan` are separate, equal-size 216-byte
  functions with the same instruction-mnemonic topology;
- both call `FastCdc::scan` out of line between `Instant::now` and `elapsed`;
- both 32-KiB stack-probe loops are inside the out-of-line scan call, hence on
  the same post-timer side;
- the candidate scan calls the same 172-byte region kernel twice;
- the small/large shifted and normal masks enter that kernel in registers
  `x3/x4` and are tested directly;
- the region kernel has no 16-KiB target comparison and no mutable mask load;
- candidate scanner state contains neither cached mask fields nor `next_even`.

Any codegen custody mismatch is a parity failure and forbids measurement or
durable work.

## Corrected CDC-only screen

The screen binaries preload the exact fixture before timing. The out-of-line
timed boundary begins immediately before `FastCdc::scan` and ends on its
successful return. The callback only checks nonempty/bounded count and appends
one `u32` length to a preallocated 5,284-entry vector. Reconstruction BLAKE3,
ordered transcript BLAKE3, min/max, terminal-end validation, and the optional
TSV write happen after the timed interval.

Exactly one exact authority TSV is written, by measured pair-1 control. Every
other row reports its own ordered transcript and reconstructed-source
fingerprints without retaining a redundant full TSV.

Schedule:

```text
uncounted warmup AB
measured pair 1 AB
measured pair 2 BA
measured pair 3 AB
measured pair 4 BA
```

The measured arm sequence centers are both exactly 6.5, so the schedule is
position-balanced and time-symmetric. The screen clock begins immediately
before warmup A and includes all ten invocations, acquisition custody,
analysis, disposition, and manifest. It must be less than 20,000,000,000 ns.
No measured row may be rerun, deleted, repaired, or replaced.

Every row must report exactly 104,857,600 consumed/scanned bytes, 5,284
callbacks/occurrences, the same ordered start/end/length transcript, the exact
reconstructed source BLAKE3, same min/max and terminal end, fixed capacities,
and frozen binary/fixture/schedule custody. The single TSV must independently
parse into contiguous nonempty boundaries no larger than 32,768 with exact
terminal end. Focused tests protect fragmented readers and exact callback
failure. Any parity, machine-code, or custody failure is `REVERT` and forbids
durable work.

The direct signal is the position-balanced mean of `boundary_wall_ns`. A pair
or position wins only on a strictly lower candidate value. Advance requires:

- at least 8.000 ms direct position-balanced saving;
- at least three of four measured pair wins;
- both execution positions win;
- equal temporal arm centers;
- candidate user/system CPU means no more than 5% above control;
- paired median maximum-RSS ratio no more than 1.05 and at least three of four
  ratios no more than 1.05;
- identical fixed capacities and every parity/codegen/custody gate.

Relative improvement is descriptive only; no 10% denominator gate applies.
If direct saving is below 8 ms, preserve `NO-GO / REVERT`, restore exact
`daf4cef` product source, close serial safe-Rust exact-boundary CDC tuning, and
stop without another CDC implementation.

## Conditional durable A/B

Only `advance_to_durable=true` authorizes this campaign. Control is the sealed
Canonical-v2 executable and B is the once-built candidate durable executable.
Each arm is prepared once outside row timers and each row receives an
independent byte-identical database, authority, expectations, and fixture copy.

Schedule is the same warmup `AB` plus measured `AB / BA / AB / BA`. The
conditional campaign clock includes preparation, ten invocations, analysis,
disposition, and manifest and must be no more than 120 seconds. No selective
rerun is allowed.

Hard gates are the exact Canonical-v2 source/profile/occurrence commitment,
root `93d1b461...c6d1`, transition `2de8d2ce...fd89`, closure
`29233d60...c0c1`, 104,857,600 source/scanned bytes, 5,284 occurrences,
5,372/0 objects, 105,122,466 canonical bytes, 196,174 mapping bytes, 5,381 SQL
calls, 10,748 BLOB writes, one transaction, one successful COMMIT,
`FULL + DELETE`, `temp_store=FILE`, `mmap_size=0`, exact timer equations,
publication with zero graph rescan, Q no greater than 86,181 and terminal zero,
exact logical/apparent endpoints, paired allocated-store protection, and no
journal/WAL/SHM residue.

Retain requires at least 10.000 ms and 2% position-balanced durable saving,
at least three of four B wins, both execution positions, equal temporal
centers, lower candidate mapping wall consistent with the clean screen, and
every semantic/durability/Q/storage/custody/resource gate. CPU, RSS, and
allocated-store protection use a paired median ratio no more than 1.05 with at
least three of four pairs no more than 1.05.

## Closure

Only after both screen and durable pass: run offline full-workspace all-target
tests, offline all-target Clippy with warnings denied, rustfmt, scoped
whitespace checks, independent recomputation, and a final read-only audit. A
complete pass freezes a versioned successor report/manifest and updates the
roadmap control without commit.

Any failed gate preserves evidence, reverts only v2 product code, verifies the
exact committed source, retains Canonical-v2 as control, and stops. H09,
SQLite/page-size, concurrency, materialization, reopen authority, migration,
production integration, another CDC candidate, and commit are prohibited.

Physical I/O, sync-call counts, instructions, cycles, true cold-cache state,
phase-local CPU, and unobserved heap work remain `Unavailable`; no wall, RSS,
logical-byte, or historical-subtraction proxy is permitted.
