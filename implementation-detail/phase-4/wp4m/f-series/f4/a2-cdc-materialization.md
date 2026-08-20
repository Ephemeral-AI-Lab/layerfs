# WP4-M F4-A2 — FastCDC scanner-owned materialization diagnostic

## Prospective preregistration — frozen before diagnostic source or timing

- Date: 2026-08-20.
- Scope: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` only, branch
  `codex/empty-worktree`.
- Documentation checkpoint / HEAD:
  `83d085bd80e82ae22b4a9766f2fc8aed03501fb8`.
- Classification: diagnostic-only copy/materialization attribution.
- Required terminal result: exactly `GO`, `NO-GO`, or `REVISE`.
- Artifact root:
  `target/wp4m-f4a2-cdc-materialization-k64-20260820-v1`.

F4-A2 does not implement, retain, or integrate a borrowed-window scanner. It
does not start F4-B, F5, or F6; change CDC boundaries, identity, schema,
profile, durability, or publication; build a carrier; add a dependency; touch
the sibling `layerfs` repository; or commit.

### Entry custody

| Item | Frozen value |
|---|---|
| accepted F2-v3 benchmark source SHA-256 | `c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158` |
| accepted F2-v3 release executable SHA-256 | `68b599b819da9f05c76d35efd807c5d5f03266dfb7d4ed0cc78da269c4b891c0` |
| retained source bytes / SHA-256 | `104,857,600` / `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| retained CDC references | `5,284` |
| retained CDC sequence fingerprint | `5bb376c3c54d8724973a7b160acab599f2f5cee4b4a56e855ff0cbe987425994` |
| F4-A manifest / attestation | `23e3a74d5015342fda59aad5f6046de488cca6a5d688e9f0e2db8514e2dcfe07` / `646d3adaa44d4b23837e13027dcfd887c18bf84b47126f1010c10df54c4513dd` |
| F4-A raw / analysis | `5241b106a9d1d841e124d73ff247f2abadb2bf27759ef54d62a3ab3af3eb212f` / `ee30693a372e0a3bca6a9831055683e2be80e24191012b20eea7d3615ad5a3b2` |
| F4-A final report / audit | `41497414d94b45c55825573d91cec3f765d9043e2a054bb2ce5fc33774a08715` / `27ca2ccf8473a007f55bc20774a65b16bdeb059b945d053229a3dae558aee46e` |
| F4-A live report SHA-256 | `34a54b94284f263e5adb9c96a8109dd93b68a009ba8377a1af93221ccbbccace` |
| diagnostic source / release executable | `NotCreated` / `NotBuilt`; freeze once after validation |
| database/authority/expectation base | `NotApplicable`: this diagnostic performs CDC only and opens no SQLite store |

The retained fixture is
`target/wp4m-f4a-residual-attribution-k64-20260820-v1/S1-100.source`.
The accepted benchmark source begins at the frozen hash above and must end at
that same hash. The private diagnostic is one temporary auto-discovered Rust
binary under `crates/layerfs-engine/src/bin/`; its custody copy is retained in
the artifact root and the live temporary file is removed after measurement.
No production/core scanner file is edited.

### Frozen pre-edit dirty-worktree record

The pre-edit branch/HEAD matched the values above. Exact read-only fingerprints
were captured before this file was created:

```text
git status --porcelain=v1 -z SHA-256
  31a7985598a2bb4a5d2c9d4ca29223438dac0de3c9d590686d23faab25951fc5
git diff --binary SHA-256
  594c2aab450f9ae23a9791b6c1d0af04bc758898cac5ea870dfe29b475110b14
git diff --cached --binary SHA-256
  e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
git ls-files --others --exclude-standard -z SHA-256
  c1d359b4c451544e0e498be993f89c939ee078304be3f39eaec9cc0725b42d68
```

Pre-existing paths are exactly:

```text
D  NOTE_TO_READ_AFTER_PHASE_4.md
M  implementation-detail/README.md
M  implementation-detail/path-map.tsv
M  implementation-detail/phase-4/README.md
M  implementation-detail/phase-4/algorithm/complexity-analysis.md
D  implementation-detail/phase-4/wp4m/f-series/f3/read-after.md
M  implementation-detail/phase-4/wp4m/f-series/planning/full-create-plan.md
M  implementation-detail/phase-4/wp4m/progress.md
?? implementation-detail/phase-4/wp4m/f-series/f4/report.md
?? research/cursor-git-at-any-scale.md
?? research/mem9-drive9-layered-filesystem-distilled.md
```

Their exact current/base SHA-256 values were recorded before this edit. The
artifact custody payload will include the exact pre-existing tracked binary
patch plus untracked-path hashes before either shared ledger is updated.
Every non-F4-A2 byte remains user-owned.

## Question and hypothesis

Question: does the accepted FastCDC scanner spend at least `33,000,000 ns` of
directly removable wall time writing/materializing complete source bytes into
its scanner-owned chunk `Vec`, after subtracting the minimum bounded carry work
and observer overhead that a safe borrowed-window scanner would still require?

Hypothesis: the accepted per-pair `Vec` writes are measurable, but their net
removable median may be below 33 ms. No direction is assumed. A result below
the gate is terminal `NO-GO`, not permission to change the threshold.

## Actual call graph and retained scanner behavior

The retained 100-MiB full-create call graph is:

```text
main --row
  -> run_row(operation = full)
  -> capture_full_create
  -> build_file_proven
  -> build_file_construction
  -> FastCdc::scan(File::open(source), callback)
  -> FileBuilder::push_bytes
```

Other production/benchmark callers were read before this preregistration:

- `LogicalFile::{full_replace,full_replace_timed}` and the bounded edit/rejoin
  helpers route through `FastCdc::scan` and consume each callback slice
  synchronously;
- `InMemoryCas` tests and content tests synchronously hash/copy/store the slice;
- the Phase-4 benchmark uses the scanner for fixture fingerprints, edit points,
  expected observations, rejoin diagnostics, full-create construction, edit
  oracles, and tests; and
- `layerfs-eval` uses it for deterministic layout and SQLite-ingest fixtures.

The scanner implementation in `crates/layerfs-core/src/cdc/mod.rs` is:

```text
source Read
  -> one stack input window [u8; 32,768]
  -> Scanner {
       chunk: Vec<u8> with capacity 32,768,
       pending: Option<u8>,
       hash: u64,
       next_even: usize
     }
  -> fill chunk to 8,192 bytes
  -> process two bytes per gear step
  -> emit before first, after first, or forcibly at 32,768 bytes
  -> callback borrows scanner-owned complete chunk bytes
  -> clear and reuse the same Vec
  -> at EOF append a pending byte and emit the final partial chunk
```

Frozen source bounds and constants are:

```text
input-window bytes             32,768
minimum chunk                   8,192
target chunk                   16,384
maximum chunk                  32,768
normalization shift                 2
profile seed                        0
scanner-owned chunk capacity   32,768
fixed pending lookahead byte        1
```

The gear table and four masks are included from/copied exactly from the current
core source into the private diagnostic build. A compile-time/direct equality
test and complete boundary comparisons prevent drift. The source-read loop,
read buffer size, EOF behavior, checked byte/chunk counters, pair order,
pending-byte behavior, masks, normalization, gear lookups, forced maximum,
and final partial chunk are identical in all comparable lanes.

## Diagnostic lanes

### Lane A — accepted materializing control

Lane A calls the retained public `FastCdc::scan` exactly as shipped, using a
timed/counting wrapper around `File`. It therefore performs the accepted
scanner-owned `Vec` writes and complete-chunk callback. Its callback performs
only the common boundary sink described below. Raw chunk hashing, canonical
encoding, SQLite, mapping, and COMMIT are outside the parent timer.

### Lane B — same-gear boundary-only supplemental diagnostic

Lane B uses the private exact same-gear state machine with the same `Read`
loop and 32,768-byte input window, but represents the current chunk by checked
length/offset scalars and never writes all source bytes into a complete chunk
buffer. It performs no carry copy. It uses the identical boundary sink and
post-timer fingerprint audit.

Lane B is supplemental, not a controlling paired arm. It runs only after the
complete A/C campaign so it cannot perturb A/C ordering or cache state. Its
five rows are reported independently and provide a direct mandatory
gear/boundary parent observation; no A/B nonadjacent subtraction can override
the controlling A/C decision.

### Lane C — boundary-only plus required bounded carry

Lane C is Lane B plus one `Vec<u8>` with fixed capacity 32,768. It copies bytes
only when the exact retained pair/pending decision cannot emit a chunk while
all of that chunk's bytes remain borrowable from the current live input
window. This includes ordinary window-straddling chunks and the one-byte
pending-lookahead case where the old window must be overwritten before the
boundary decision is known. Bytes that remain wholly borrowable at callback
time are not copied.

The implementation appends old-window tail bytes before the next read,
preserves the accepted one-byte pending scalar, appends that byte to the carry
only after its owning chunk is decided, and appends the new-window prefix only
when a carried chunk emits. Every carry append is a directly timed
`extend_from_slice`/one-byte append. Carry contents are compared with the
source slice outside the parent timer in focused tests; the release sink needs
only the exact boundary.

One full input window equals the accepted maximum chunk. With full regular-file
reads, a chunk can intersect at most two input windows. Arbitrary short reads
can create more read fragments; those remain supported and tested, while the
release fixture requires exact A/B/C read-call and read-byte equality.

Lane C is the controlling diagnostic because it includes all mandatory work
for the separately describable future candidate:

```text
borrow a chunk only while it is wholly available in the live input window;
otherwise copy the exact required bytes into one <=32,768-byte carry buffer.
```

The code is diagnostic evidence only and is deleted from the live source tree
after custody capture.

## Common boundary sink and post-timer identity audit

Every lane preallocates exactly 5,284 boundary slots before its parent timer.
For each emitted chunk, the common sink:

1. checks and records ordered `start:u64`, `end:u64`, and `length:u32`;
2. updates one identical BLAKE3 transcript over those fixed-width values; and
3. increments one checked boundary count.

The sink interval is timed identically in A/B/C. It never reads or hashes raw
chunk bytes. After the parent timer ends, a separate postflight reopens the
fixture, reads each recorded range through one 32,768-byte buffer, recomputes
the raw `ChunkId`, and produces the retained sequence fingerprint over
`u32be(length) || raw ChunkId`. That raw hashing/read wall is published but is
not CDC parent, materialization, carry, or sink wall.

Every row must match the preflight accepted boundary list byte-for-byte or its
complete authenticated digest, and must reproduce exact source bytes, 5,284
references, and the frozen CDC sequence fingerprint. Total lengths must equal
104,857,600; starts/ends must be contiguous; min/target/max/final rules, first
and last boundary, final partial chunk, error class, and checked arithmetic
must agree.

## Counters and units

All byte/count fields are checked `u64`; walls are integer nanoseconds from
`Instant` and are never post-hoc rewritten. Each row reports:

```text
lane / pair / position / warmup
parent wall ns
source-read wall ns, calls, bytes, short-read calls, EOF calls
boundary-sink wall ns and calls
source bytes consumed
gear pairs and first/second boundary-decision counts
forced-maximum and final-partial counts
pending-byte stores/resolutions
chunks and complete ordered boundary digest
postflight wall and CDC sequence fingerprint
window-contained/borrowable chunks and bytes
carry-required chunks and bytes
ordinary straddling chunks
pending-lookahead delayed chunks
carry-copy calls, bytes, wall ns
maximum simultaneously live carry bytes and capacity
checked overflow state
maximum and terminal diagnostic heap bytes
timer calls and checked-counter calls
error classification
```

`carry-required bytes` is the sum of exact chunk bytes that cannot be emitted
from one still-live input window under the retained pair/pending decision.
Each source byte is counted at most once in that sum. B reports that modeled
requirement to cross-check C but performs zero carry-copy calls/bytes/wall; A
reports neither because the accepted API does not expose window ownership.
`carry-copy bytes` must equal `carry-required bytes` exactly in every C row.
Carry capacity and maximum live carry must be at most 32,768 bytes. Terminal
diagnostic heap is measured after boundary/carry ownership is dropped and must
be zero.

## Timer boundaries and checked equations

Parent start is immediately before the first scanner `Read`; parent end is
immediately after EOF finalization returns. Preallocation, source/fixture
preflight, postflight raw hashing, JSON construction, and process-resource
collection are outside the parent.

Raw equations are checked per row:

```text
Lane A parent
  = source read
  + accepted gear/boundary plus full materialization
  + identical boundary sink
  + observer
  + nonnegative residual

Lane B parent
  = source read
  + mandatory gear/boundary
  + identical boundary sink
  + observer
  + nonnegative residual

Lane C parent
  = source read
  + mandatory gear/boundary
  + required carry copy
  + identical boundary sink
  + observer
  + nonnegative residual
```

Directly measured children are subtracted once:

```text
A exclusive = A parent - A source - A sink
B exclusive = B parent - B source - B sink
C exclusive = C parent - C source - C sink - C carry-copy wall
```

All subtractions use checked arithmetic and every residual is nonnegative.
No individual gear/materialization wall is invented inside accepted Lane A.

The controlling adjacent-pair directly removable budget is:

```text
raw paired replacement-inclusive delta = A parent - C parent
directly removable CDC budget
  = max(0, raw paired delta - mechanism-specific observer ceiling)
```

Lane C already contains the required carry/replacement work, so it is not
subtracted a second time. The direct carry-copy wall and `C parent - C carry`
nested boundary-only value are reported separately. Descriptive A/B gross
medians cannot override the paired A/C result.

## Observer gate

Before any release campaign row, the final release executable runs an observer
probe using the exact maximum per-row timer-interval and checked-counter call
counts established by the retained 100-MiB preflight. Five complete optimized
probes are retained. Report:

- timer interval/call count;
- checked-counter call count;
- each complete observer wall;
- maximum complete wall as the per-row observer ceiling; and
- the carry-timer subset as the mechanism-specific ceiling.

Raw values are never corrected. The maximum mechanism-specific ceiling is
subtracted prospectively from every A/C delta. `Instant` calls and counter
branches can perturb branch/cache behavior; that limitation is explicit and
the ceiling is conservative. A single boundary/timer/counter call-count drift
after the probe is `REVISE`.

## Focused validation

Before release measurement, one private test module and one release self-test
must cover:

- empty input;
- one byte and sub-minimum input;
- exact 8,192-, 16,384-, and 32,768-byte final/forced boundaries;
- chunks ending immediately before, at, and after a `Read` boundary by using
  a prospectively chosen fragmented-reader schedule around an accepted
  boundary;
- chunks contained in one full input window and spanning the maximum two full
  regular-file windows;
- final partial chunk and pending-byte ownership;
- exact ordered A/B/C equality across deterministic fragmentation schedules;
- exact retained 100-MiB boundary list, boundary digest, count, source bytes,
  and CDC sequence fingerprint;
- common boundary-sink equality;
- carry-required-byte/copy-byte equation and maximum carry capacity;
- checked byte/chunk/copy/counter overflow;
- read error classification and propagation;
- short-read behavior and exact A/B/C read progression;
- allocation/callback error cleanup and terminal diagnostic memory; and
- unchanged production scanner/source behavior.

Then run exactly:

```text
cargo test --offline -p layerfs-engine --bin f4_a2_cdc_materialization
cargo test --workspace --offline --all-targets
cargo clippy --workspace --offline --all-targets -- -D warnings
cargo fmt --all -- --check
target/release/f4_a2_cdc_materialization --self-test FIXTURE
git diff --check HEAD
tracked/untracked whitespace and status checks
accepted benchmark-source SHA-256 check
```

The final release executable is built exactly once after all debug/static
checks pass:

```text
cargo build --offline --release -p layerfs-engine \
  --bin f4_a2_cdc_materialization
```

The release self-test uses the once-built frozen executable directly, so no
second release build is allowed.

## Frozen release schedule and cache preparation

Each row is a fresh child process. Immediately before every child, the runner
reads and hashes the complete fixture outside the child timer and verifies the
frozen size/SHA-256. Cache state is labeled
`warm_or_unknown_after_complete_fixture_preflight`; no cold-cache claim is
made. There is no store/base state.

The runner asserts this exact controlling schedule before invocation:

```text
warmup pair0   AC
measured pair1 AC
measured pair2 CA
measured pair3 AC
measured pair4 CA
measured pair5 AC
```

That is 12 controlling rows: two uncounted warmup rows and ten measured rows.
No row is deleted, replaced, selectively rerun, or relabeled.

Only after all controlling rows complete, run the supplemental Lane-B schedule:

```text
warmup B0
measured B1
measured B2
measured B3
measured B4
measured B5
```

Total planned release rows are 18: 3 warmup and 15 measured. B rows are never
inserted between controlling pairs. Every started/returned/raw row is retained.

## GO / NO-GO / REVISE

`GO` requires all of:

1. exact boundary/identity hard gates in all 18 rows;
2. valid checked timer/counter/carry equations in all rows;
3. median controlling directly removable A/C budget at least 33 ms;
4. at least four of five controlling pairs independently at least 33 ms;
5. required carry/replacement work and the frozen observer ceiling already
   included/subtracted;
6. carry capacity and maximum live carry no larger than 32,768 bytes, plus
   only the fixed 32,768-byte input window, pending byte, and scalar state;
7. a separately preregisterable one-variable borrowed-window candidate that
   preserves exact CDC boundaries, identities, errors, memory, synchronous
   callback lifetime, and downstream contracts; and
8. no source-sized staging, unbounded references, worker/async path,
   schema/profile/durability/format change, or production integration.

Interpretation is frozen:

```text
median >= 60 ms                    strong GO
median >= 33 ms and < 60 ms        low-margin GO
median < 33 ms or fewer than 4/5   NO-GO
identity/timer/custody/carry defect REVISE
```

A single boundary, length, fingerprint, final-chunk, source-byte, read-shape,
error-class, arithmetic, observer, custody, or carry-equation mismatch is
`REVISE`; it can never produce `GO`.

`GO` authorizes only a later separately requested and preregistered F4-B.
`NO-GO` retains F2-v3, forbids the borrowed-window scanner, closes the current
format-preserving F4 optimization search, and keeps F4-B/F5/F6 ineligible.

## Evidence, independent recomputation, cleanup, and sealing

The artifact root will retain the prospective report, exact pre-edit custody,
diagnostic source, source diff, final executable, fixture/linkage hashes,
commands, environment/toolchain, focused/full/static/self-test logs, observer
rows, all 18 raw rows and per-pair order, complete boundaries or their
authenticated digest plus equality audit, timer/counter/carry equations,
CPU/RSS where available, primary analysis, independently implemented
recomputation, final report, and final read-only audit.

Unavailable by construction and reported with reason:

- cold-cache state: no supported deterministic APFS eviction mechanism;
- physical I/O bytes: CDC-only regular-file logical reads do not expose media
  traffic and logical read bytes are not a substitute;
- allocator-internal capacity/RSS attribution below process RSS: the
  diagnostic controls requested capacities but does not instrument malloc;
- individual accepted gear versus materialization wall: the retained API has
  one parent, so the controlling paired equation is used instead; and
- production-candidate performance: F4-A2 is a private diagnostic, not an
  integrated optimization or F5 acceptance row.

After immutable analysis is complete:

1. copy the temporary diagnostic source into evidence;
2. remove only that F4-A2-owned live temporary source;
3. verify the accepted benchmark source remains byte-identical at
   `c8ac86be…cc158` and core CDC sources have no Git diff;
4. distinguish pre-existing user changes, this report/terminal ledger appends,
   removed diagnostic source, and ignored evidence in the final audit;
5. set payloads read-only, generate the complete manifest last, set the root
   read-only, and write only an external attestation afterward; and
6. never rewrite the sealed root.

Earlier F0/F1/F2/F3/F4-A roots are never deleted, renamed, chmodded, appended,
or rewritten. This document receives only an additive terminal section after
the frozen rows; the existing F4-A result remains unchanged.

## Terminal result — VALID / NO-GO

The frozen diagnostic completed exactly as preregistered. Planned, started,
returned, and raw rows are `18 / 18 / 18 / 18`: warmup `AC`, five measured
controlling pairs `AC/CA/AC/CA/AC`, then one B warmup and five supplemental B
rows. No row was deleted, replaced, resumed, or selectively rerun. All rows
pass their child, JSON, source-read, boundary, sequence, timer, carry,
arithmetic, and terminal-memory gates.

### Exact lane result

Lane A is the retained public scanner with its 32,768-capacity complete-chunk
`Vec`; B is exact same-gear boundary-only with no carry writes; C is B plus the
required one-buffer carry implementation. A/C is the controlling adjacent
comparison. B ran only after A/C and is descriptive.

The mechanism-specific observer ceiling is `397,875 ns`. C already contains
required replacement work, so the checked controlling equation is:

```text
directly removable = max(0, A parent - C parent - 397,875 ns)
```

| Pair/order | A parent ms | C parent ms | Raw A-C ms | Directly removable ms | >=33 ms |
|---|---:|---:|---:|---:|---|
| 1 / AC | 128.028333 | 123.928875 | 4.099458 | **3.701583** | no |
| 2 / CA | 127.479708 | 125.718291 | 1.761417 | **1.363542** | no |
| 3 / AC | 127.370000 | 123.895958 | 3.474042 | **3.076167** | no |
| 4 / CA | 128.371292 | 122.456042 | 5.915250 | **5.517375** | no |
| 5 / AC | 126.937250 | 122.328708 | 4.608542 | **4.210667** | no |

```text
median / min / max / spread
  3.701583 / 1.363542 / 5.517375 / 4.153833 ms
rows >=33 ms
  0/5
```

The supplemental A-minus-B gross values are
`5.776291/6.012249/7.248416/7.294084/6.419750 ms`, median
`6.419750 ms`. They are nonadjacent/descriptive only and remain far below the
gate even before carry/observer subtraction.

### Required carry and boundary evidence

Every C row reports exactly:

```text
chunks                              5,284
window-contained/borrowable         2,084
carry-required                      3,200
  ordinary window-straddling        3,199
  pending-lookahead delayed             1
required/copied bytes          67,072,778 / 67,072,778
copy calls                           7,343
maximum live / capacity             32,768 / 32,768 bytes
terminal diagnostic heap                 0 bytes
```

The five direct carry-copy walls are
`1.921987/1.972353/1.906844/1.861943/1.904530 ms`; median/min/max/spread are
`1.906844/1.861943/1.972353/0.110410 ms`. C-minus-B parent tax is separately
reported but noncontrolling because B is later/nonadjacent.

All 18 rows reproduce 104,857,600 source bytes, 3,201 read calls, one EOF, no
short read, 5,284 ordered chunks, boundary digest
`54481a2d54ae2eb7ccb9842c624534c942b580d157511b22bab0d6e302941aa7`,
and sequence fingerprint
`5bb376c3c54d8724973a7b160acab599f2f5cee4b4a56e855ff0cbe987425994`.
Every complete boundary is retained in the evidence root.

Five observer probes used 15,829 timer intervals, 68,596 checked-counter
operations, and 7,343 carry timer calls. Maximum complete observer wall is
`843,792 ns`; the carry/mechanism ceiling above is `397,875 ns`. Raw values
are never corrected. All raw parent/child and pair equations are checked and
nonnegative.

### Validation, custody, and limitations

The focused diagnostic passes 8/8 tests. The full workspace passes 121 tests
(`44 core + 4 engine + 8 diagnostic + 48 benchmark + 12 parity + 5 eval`).
Clippy with warnings denied, rustfmt, diff whitespace, release self-test, and
primary/independent analyzer agreement all pass. The first pre-release
validation attempt is preserved: tests passed and Clippy found one equivalent
single-pattern-match style issue; no release build or row existed. The
versioned r1 rerun passes every gate.

```text
diagnostic source
  dc0876f49491149eebc11cfa86746864ab22beb938ff94322e7696c69f64c6c8
diagnostic executable
  93afe9f090d01974923abb810a90ecfb1c5f673fe7826a301c1212b72438b001
fixture
  63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4
raw JSONL
  745232b8cda015f639795328700c4a7fd857e03f1ee6ace9d804a1645daa614a
primary analysis
  8cbbc3eaadd0b7a2da989686e95e539c167953c681fb0be28dc63f597b8cd4e9
independent analysis
  d83f8bf8bd3f38b9bba01a81e38a68ac5ec48c713ea84ce22fe0b94c18e8e8b2
agreement audit
  0a849f1d170ac1d472523c8926e73014281109c240968133a09c60a46c5586a4
```

Cold APFS state, physical-media I/O, allocator-internal attribution, and
individual accepted gear-versus-materialization wall remain Unavailable for
the prospectively frozen reasons. B cannot override A/C. This is a diagnostic,
not an integrated-candidate or F5 row.

The temporary live diagnostic source is removed after its exact custody copy.
The core scanner has no Git diff. The accepted benchmark source remains
byte-identical at
`c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158`.
No optimization, dependency, F4-B/F5/F6 work, identity/schema/profile/
durability change, carrier, production integration, sibling-repository edit,
or commit occurred.

The valid median directly removable budget is only `3.701583 ms`, and 0/5
rows reach 33 ms. F4-A2 is therefore `NO-GO`: retain accepted F2-v3, do not
implement the borrowed-window scanner, keep F4-B/F5/F6 ineligible, and close
the current format-preserving F4 optimization search. Any broader SQLite
4K/8K/16K physical-profile experiment requires explicit new authorization.

### Terminal evidence custody

The sealed evidence root contains 72 manifested payloads plus the manifest.
Its complete mode/byte/SHA/path manifest is
`f4a2-terminal-manifest.tsv`, SHA-256
`f59b0cd2cf4354611eb0625517300f98ed83709c4db81e92bc70629012388ace`.
An external read-only walk verifies 72/72 payloads, listed/actual equality,
exact modes/bytes/hashes, zero symlinks, and zero owner-writable entries. Its
attestation SHA-256 is
`68f028da3186af4a66b03851e7f65abcd918491569c76e8f630e87ed59cd817d`.

The sealed final report / final read-only audit / machine audit SHA-256 values
are:

```text
9dbbf10a34aac9b7eb227d03ffaf53ec1b96779306523cdcddea1d521a02a286
c4cb50041258748deee2e57dc5d268b6800c5af50a04ed20fe515b74a99d7fc4
0196dc241702d200ee32e0620d84f32dacb49f5492fc98a93cd3c22e9a10e7d1
```

The sealed root is not rewritten after manifesting. The attestation remains
outside it by design.
