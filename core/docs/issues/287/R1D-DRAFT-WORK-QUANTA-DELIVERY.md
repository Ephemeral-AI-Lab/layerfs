# R1D draft reference preparation delivery

The root-owned covering command passed the four new public native reference
vectors, nine existing native draft tests and four temporary-owner tests on
2026-10-01. The current maximum128 vector witnessed four insert preparations and
six delete preparations while acknowledging the same128+128=256 reference row
effects. This is an actual scoped work-count proof, not a speed or deadline PASS.

The prospective contract is
[R1D-DRAFT-WORK-QUANTA-FREEZE.md](R1D-DRAFT-WORK-QUANTA-FREEZE.md). Root retained the
exact commands, source-change hashes and results in
[command.json](../../../../benchmark-results/fs-bench-pro/issue287-r1-draft-fixture-native-quanta-cover/command.json)
and the storage
[result](../../../../benchmark-results/fs-bench-pro/issue287-r1-draft-fixture-native-quanta-cover/2.json)
and [log](../../../../benchmark-results/fs-bench-pro/issue287-r1-draft-fixture-native-quanta-cover/2.log).
These local append-only receipts are not a published release seal or a new remote
source-identity claim.

## Delivered mechanism and exact source

`construction_state/draft_write.rs` walks the complete retained original effect
slice in its original order. Only a contiguous run of identical reference-insert
or reference-delete SQL shares a local prepared statement. Each execution binds
the original full id, ordinal, child and linked flag; each delete retains all four
expected predicates; each execution requires cardinality one. The local statement
ends before the existing owner CAS and guarded transaction finish, including
error unwinding. No persistent statement cache, new collection or retained arena
was added.

The before/proposed capsule,128-effect/actual-capacity limits,65536-byte window,
aggregate metadata admission, engine guard, native reservation/observation and
COMMIT Unknown custody remain unchanged. The captured native scratch class and
existing1MiB draft owner expression were not increased. Four u64 fields in the
existing public `DraftStats` are included by its actual compiled owner size.
Native SQLite working memory remains unqualified.

The covering receipt pins these SHA256 values:

| Source | SHA256 |
|---|---|
| `core/crates/layerfs-storage/src/construction_state/draft_write.rs` | `7874262fd018df944e8439093250e98e6fb7c7cd98a784cf01a1ab45959b8ac8` |
| `core/crates/layerfs-content/src/file/edit/draft_port.rs` | `f4c6e89307d9b1e37f8dbfc9355b84922a693f06f7439e205e15f965e2f1d2a3` |
| `core/crates/layerfs-storage/tests/draft_reference_quanta.rs` | `4816550cf2c139d3f9d71e868fbf52fd247779372badaf81e9c513cfef1e23f7` |

Prepare counters record issued calls immediately before actual preparation,
including failure. Row counters record successful cardinality-one SQL executions.
They retain the issued work if a later effect, rollback or COMMIT acknowledgement
fails. They do not represent committed live records, do not select an algorithm,
and reset only with a new owner. No private connection exposure, trace feature or
product test hook is used.

## Witnesses and covering scope

The public native vector passed exact ordinal/value/linked readback, repeated-child
count multiplicity, bounded retirement and final empty cleanup for these waves:

| References | Insert row acknowledgements | Insert preparations | Delete row acknowledgements | Delete preparations |
|---:|---:|---:|---:|---:|
|128|128|4|128|6|
|33|33|2|33|2|
|49|49|2|49|3|

The old per-row preparation expression is128+128=256; the current maximum vector
witnesses4+6=10. The246 fewer preparations are a scoped source-and-current-count
comparison, with the same256 reference row effects. There is no old/new timing
pair or dominant-cost attribution.

The other new vectors passed:

- A foreign supplied job count cannot consume the actual queued job or reference
  rows, and leaves the work stats unchanged.
- A malformed stored linked flag fails checksum validation before any delete
  preparation or retirement effect.
- An external unique-index fence makes the second insert in a32-row run fail on
  the real selected SQLite engine. Known rollback removes the first partially
  executed reference, while work telemetry retains one issued preparation and
  one successful row acknowledgement. The prior CreatingStart owner remains
  exactly acknowledged; known native cleanup succeeds.

The existing native draft tests also passed independent v1 edit root/bytes,
shared counts/revival, pre-effect oversized owner and overflow refusal, corrupt
body refusal, captured16/48MiB classes, real SHARED-reader COMMIT Unknown and
replay refusal, and known mapping acceptance followed by real acknowledgement
COMMIT Unknown without file-root emission. Existing Unknown barriers precede
their selected transition; they do not claim an instrumented internal reference
wave interleaving. The temporary-owner tests passed shared pin custody, exact
target supersession, foreign owner refusal and real supersede COMMIT Unknown.

The storage command was:

```text
cargo +1.85.1 test --manifest-path core/target/r1-eight-source/core/Cargo.toml --locked -p layerfs-storage --test draft_temporaries --test construction_drafts --test draft_reference_quanta -- --show-output
```

Root ran it against its copied source using the recorded absolute manifest path.
The command exited0 in14.016297 seconds, including compilation. Test-target
reported bodies were0.10 seconds for the nine draft tests,0.04 seconds for the four
reference vectors and0.02 seconds for the four temporary tests. These correctness
command durations are not benchmark speed arms. The receipt's separate C1
`edit_reference` command also passed three tests, preserving the sealed independent
reference roots and partitions; its command wall was8.152135792 seconds including
compilation and its reported test-target body0.21 seconds.

## Remaining scope

The prior full-Core direct test target reported66.69 seconds with nine passes,
one Deadline failure and one ignored test. That duration is the whole target,
not the failing Service call's duration. Its request still has2049 separated
runs,4098 extents,131136 replacement bytes,262272 base bytes and the unchanged
10000ms deadline. No retained per-call/query-time receipt establishes the dominant
term. Its root-owned failed-only covering run on the coherent new source is
pending at this delivery; no deadline increase, reduced workload or repeated
speed sample is authorized by this count proof.

BindingClaims8, final clippy/examples and coherent source publication have their
own root-owned results. This delivery does not claim their pending checks pass,
whole-process memory fit, strict/native/protected capability, physical progress,
benchmark eligibility or end-R1 release admission. Production was frozen after
the reported source; this document makes no additional production changes.
