# Shared prerequisite: logical fragments and native frame limits

> **Status: implemented and verified through the strict saved-root route.**
> Exact implementation parent: `c4f965357381870b3784e3423e75783496c0b7c7`.
> Product input seal: `e61a74254ee6fb80befa06948081d70a085749e63f5c600c640031a7034d2584`.
> This shared transport prerequisite precedes the pending handle-write round.

## Observed failure and cause

The pending handle-write selection `write-frontier-01` made 256 alternating
one-byte changes, refused the 257th, overwrote the first 64 changes and completed
its real Commit. Attribute/alias inspection of the saved root succeeded, but
reading its first 514 bytes returned known Io. The original 25.869655584-second
FAIL, source seal `3428c8b2f0cb6a5c0547362d9c62cd06dcf634f2bd4272041603a1616d055bc8`,
caller, input identities and logs remain unchanged. The owned failed runtime was
inspected and removed with a separate external-cleanup record; no successful
Workspace cleanup is inferred.

C1 emits each requested logical extent through Write. Native Output previously
created a ResultData frame for every such fragment. The legal 514-byte result has
512 fragments: 256 changed bytes, 255 retained one-byte gaps and a three-byte
retained tail. Its unchanged frame allowance is ceil(514/1024)+257 = 258, and
Output refused the 258th data frame. That I/O rejection becomes the service's Io
failure; canonical content and Workspace coordinates were not the cause.

A read-only diagnostic on an independent byte copy of the retained saved Store
confirmed the exact boundary using the original immutable binaries:

| Requested bytes | Declared response budget | Data frames/bytes received | Terminal |
| --- | --- | --- | --- |
| 257 | 257 | 257 / 257 | Success |
| 258 | 258 | 257 / 257 | Io Failure |
| 514 | 67,108,864 | 512 / 514 | Success; all expected bytes match |

The widened budget in the last row is **diagnostic only**. It proves the stored
bytes are correct; it is not the selected fix, an acceptance pass or a substituted
workload. The strict 514-byte acceptance read must pass with its original budget.

Independent authenticated socket tests reproduce both directions before the fix:
one-byte logical output returns known Io, and one-byte Source input fails with
unknown delivery after exhausting its wire allowance. The Source/Write contracts
allow short positive fragments; the wire's anti-abuse limit must not depend on
those trusted local fragment boundaries. Raw incoming tiny-frame limits stay intact.

## Correction in existing owners

Native Output holds a fixed 1 KiB pending array and combines short writes until
they reach the existing frame budget's 1,024-byte quantum. Large writes still use
direct frames up to FRAME_BYTES (16 KiB), preserving Sender's established batching
profile. Ordinary frames contain at least 1 KiB except the final tail, so their
count is at most ceil(logical bytes/1024). Explicit caller flush remains deliberate
frame work and is subject to the original finite allowance.

The server flushes a pending tail only after handler success and complete input,
before Success. Handler failure discards unsent bytes and preserves the original
Failure; already sent bytes remain partial output under the existing failed-call
contract. There is no Drop flush. Every output write/flush error is latched,
including bounds, send and deadline errors. Pending ownership is cleared before
attempting its send; a caught error cannot resend an uncertain frame or turn into
Success. The original absolute deadline is checked before each logical write and
flush, including a sub-1-KiB buffered result.

Client upload fills its existing 16 KiB array across short Source reads and emits
at least 1 KiB per ordinary frame. Its final short tail is sent only after exact
declared length and actual EOF are established, followed by EndInput. Oversized
returns, early EOF and excess input retain refusal; deadline and cancellation are
checked before and after every Source pull. Early remote refusal still cancels
upload and returns its original typed result. No automatic retry is introduced.

Only bridge native payload.rs/client.rs/server.rs change. No wire operation,
profile, limit, C1 algorithm, service operation, worker, queue or dependency changes.
Selected 64-bit fixed-state arithmetic: Output grows 40 → 1,096 bytes (+1,056:
1,024 pending bytes, length cursor, Instant, failure marker/padding). Upload adds
one 8-byte cursor while retaining its original 16 KiB array. Existing per-frame
owned Vec remains bounded by 16 KiB. Thread stack limits do not grow. These figures
describe fixed working state; they are not RSS/cgroup or performance measurements.

## Verification identity and order

The pending Workspace write diff, external caller and packet page were preserved
with exact hashes and temporarily removed. This prerequisite is built, tested and
committed against the exact c4f9653-based product tree plus the three bridge files;
its source seal above does not include the pending write operation. The archived
write work will be restored and verified on the corrected bridge in its own round.

Four external bridge tests cover short Source input, short logical output,
successful tail delivery, original handler failure with and without explicit
flush, absence of any post-Failure Drop output, and a real expired deadline before
another buffered fragment. The initial test compile typo (Saved representation
instead of its existing inserted/reused fields) remains in before-01.log; the
subsequent two genuine pre-fix failures remain in before-02.log. Product checks,
strict saved-root verification and actual route regressions are recorded below
when complete. No prior receipt is relabelled and no performance claim is made.

## Actual checks and strict read result

The corrected retained-root selection passes in 2.980889041 complete functional
seconds. Requests for 257, 258 and 514 bytes each use a response budget equal to
their requested length, return one data frame plus Success, and match every
expected byte. The 514-byte acceptance did not widen its response budget or
shrink the original selection. See the exact input/driver identities and receipt
under [native-fragmentation evidence](evidence/native-fragmentation/functional-index.json).

Host locked Rust 1.85.1 whole-core tests pass **653 / 0 failed / 3 ignored**.
Linux bridge+Workspace tests pass **52 / 0 failed / 72 ignored**, including all four
new authenticated socket tests. Existing native early-refusal and raw protocol
frame-budget tests also pass. Whole-core host/Linux Clippy with all targets and
denied warnings, examples/binaries builds, fmt, the 240-file boundary guard and
its six self-tests pass. The only pre-fix check failures are the retained test
field-name typo and the two intended reproductions described above.

Exact host commands, run from this worktree:

```sh
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --offline
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --offline --all-targets -- -D warnings
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --offline --examples --bins
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Linux uses the existing `layerfs-pair1-rust-tools:c331f3815ef3cfb5c760` image with
this worktree at `/work`, read-only Cargo registry and target
`/work/core/target-linux`. Its test command is `cargo test --manifest-path
core/Cargo.toml --locked --offline -p layerfs-bridge -p layerfs-workspace`; build
and Clippy use the same whole-core arguments as host. Root ARMv8 configuration
SHA is unchanged. No dependency, CI or retired preflight operation occurs.

Strict reproduction after building the recorded binaries:

```sh
python3 core/crates/layerfs-bridge/tests/fragmented_saved_root.py \
  --store core/target/pair1-evidence/write-frontier-01/service/store.sqlite \
  --root ed0ac43c4a454dbcb42c209faf76a4c36936df9b3350256d85906f88d9ba3b41 \
  --binaries <host_binaries-from-fragmentation-inputs-01.json> \
  --output <fresh-owned-output>
```

Twelve actual native regressions pass: Stage frontier/lowering/actual-save/loss;
composite clean/frontier/repeated/successor/actual-save/lost-result; and
CommitStaged post-success reconciliation failure/denial. They cover Source input,
streamed read results, unchanged C2/C5 boundaries, actual save continuation and
failure retention on the corrected transport. This targeted prerequisite does
not rerun all 72 native Workspace selections; the pending handle-write round will
verify its changed shared mutation/splice paths after restoration.

Actual Linux read-only mount/authenticated Status also passes in 24.997700292
complete functional seconds. Current-source route evidence totals 14 PASS (strict
saved-root command, 12 selected native regressions and mounted Status), alongside
the original write-frontier FAIL and separately labelled pre-fix diagnostic.
All complete commands remain below 60 seconds. No same-worktree build overlaps
a selection; observed other-worktree activity remains declared in receipts.
No performance, cold-cache, writable-mount or full Pair 1 qualification is claimed.


## Exact production source comparison

**Production LOC: 107378 -> 107459 (delta +81)**. Reference remains
65,417 -> 65,417 (0); core is 41,961 -> 42,042 (+81), entirely bridge
4,102 -> 4,183. Pending Workspace write is excluded from both exact snapshots.
This is the minimum shared framing correction, not relocation or legacy retirement.

Both snapshots use counter blob `b5b9617d08204977176302311e0b2c72a811b420` and
`git archive <revision> crates core/crates`, followed by
`python3 tools/production_loc.py --root <archive> --json`. Identical nonblank,
non-comment first-party Rust/runtime-SQL scope excludes inline/external tests,
examples, tools, docs, manifests and generated output. The counted staged tree is
`fed745c4bea5c2c60596b96161645ef1b197d1a5`; final evidence additions do not alter
its production source. See [the exact comparison](evidence/native-fragmentation/production-loc.json).
