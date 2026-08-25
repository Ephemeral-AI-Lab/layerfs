# Stage 1.1 Specification — Single-file APFS Edge Benchmark

Status: **CLOSED — attempt 014 passed the complete source-bound Stage 1.1
campaign on 2026-08-25**
Authority: controls only the Stage 1.1 workload, implementation shape,
readiness and disposition; [10 — handoff freeze](10-handoff-freeze.md) and
[17 — Stage 1.0 closure](17-stage1-closure.md) remain authoritative for
canonical correctness, durability and the accepted A02 exception
Result format: [18 — Stage 1.1 result and handoff template](18-stage1.1-result-template.md)
Scope: bounded supplemental single-file evidence after the completed A01–A17
campaign and Stage 1.0 implementation closure
Platform: Apple/APFS through the public `layerfs-sdk -> layerfs-vfs -> layerfs-os::AppleDriver` path
Sequence: **Stage 1.0 closure -> Stage 1.1 -> Stage 1.2 -> mounted Stage Two**

Accepted terminal result:

```text
source commit       f3dd4a32273a4c5cbe5e7ca2287c945ba4434c30
source manifest     cacbe0497c05014a1966f152cbfed64f3ae6d4ce3e0656e459f1c1eb3a9ded84
release executable  2a7c71cf51b09d4411c1c2cb4c0b33ca1ebc435232c577ddeba4d126aba44c31
rows                47/47 PASS
edit operations     51/51 exact
transitions         34/34 exact
complete wall       13.517581334 s
rows SHA-256        7231c0a8d7dffb561adcc5aff23f77a5ffbdb645e473b62f023b09c62873fa37
disposition         PASS
```

The compact closure receipt is
`poc/evidence/stage1.1-apple-edge-20260825/terminal-receipt.md`; the immutable
raw campaign remains under
`target/layerfs-stage1-apple-edge-20260825-attempt-014` with its per-artifact
hashes recorded there. This completion does not relabel the historical Stage
1.0 A02 exception and does not start Stage 1.2.

## 0. Decision

The completed A01–A17 campaign remains immutable historical evidence. This
document does not change its populations, targets, rows, or disposition.

The requested supplemental evidence is one source-settled real-APFS workflow
combining:

```text
random unaligned locations
+ insert/delete/overwrite/append/truncate
+ replacement sizes around 8/16/32 KiB
+ physical native edits
+ durable checkpoints
+ a balanced logical-edit-to-physical-refresh matrix
+ multi-edit save bursts
+ retained history
+ fresh Verified open/read sessions
+ changed-root refresh
+ exact canonical and physical oracles
```

It is intentionally smaller than the later multi-file developer-workspace
campaign in `poc/15-stage1-workspace-benchmark.md`.

Its result cannot alter, waive, or promote the controlling A01–A17
disposition. It is a separately labeled edge supplement.

Stage 1.1 is complete only after D0–D7 produce one independently audited
terminal disposition. A correctness, durability, authentication, resource,
population or custody failure blocks Stage 1.2. A performance-only `REVISE`
requires explicit acceptance before Stage 1.2 begins.

## 1. What this benchmark answers

| Question | Required evidence |
|---|---|
| Do random physical edits produce exact bytes? | Stream-compare the live APFS file after every edit |
| Do insert/delete operations preserve unaffected canonical data? | Zero unaffected payload reads/writes; bounded changed-spine counters |
| Do edit sizes around CDC thresholds behave correctly? | Frozen 8 KiB, 16 KiB, and 32 KiB minus/exact/plus populations |
| Does a live workspace survive a mixed edit chain? | One initial live-workspace materialization, 15 native edits, 15 logical edits/refreshes, four save bursts, 34 durable transitions, zero live-workspace rematerializations |
| Are prior revisions immutable? | Direct reads from revisions 0, 5, 10, 15, 20, 25, 30, and 34 after reopen |
| Does a fresh Verified reader recover the exact durable head? | Six source-independent Verified open/read sessions while the managed workspace remains live |
| Can a live workspace refresh to changed canonical roots? | Fifteen direct logical edits followed by fifteen exact physical refreshes |
| Is count-changing refresh reported honestly? | Three same-size patch cases and twelve accepted-splice `CloneShift`/`InPlaceShift` cases with exact byte equations; unproven root changes retain `FullFallback` |
| Do editor-like save bursts compose correctly? | Four frozen overlapping/disjoint/EOF edit sequences; one checkpoint and one COMMIT per burst |
| Are resources bounded? | Q, RSS, buffers, descriptors, SQLite connections, scratch, temp, and residue gates |
| What does LayerFS add beyond physical editing? | Separate native-edit and durable-checkpoint timers |

## 2. Explicit non-goals

```text
No mounted filesystem.
No FSKit, FUSE, or File Provider.
No Bash/editor subprocess.
No npm, compiler, cache, or multi-file scenario.
No new canonical format or CDC profile.
No concurrency or crash-cut matrix.
No new rollback or stale-authority matrix; existing Stage 1.0 history/ref tests control.
No 100 MiB edit workload.
No latency SLO invented from the first observation.
No Stage 1.2 developer-workspace implementation or measurement.
No mounted Stage Two, FSKit, macFUSE, File Provider or write-interception work.
```

The later B00–B16 workspace campaign remains responsible for Bash, npm,
multi-file namespace changes, build output, external capture, links, and
realistic developer workflows.

## 3. Budgets

| Budget | Preferred | Hard |
|---|---:|---:|
| Largest native payload/user file | 24 MiB | <32 MiB |
| Largest regular file | 24 MiB initially; 25,227,264 B maximum | <32 MiB at every sub-edit and row |
| Replacement operand | <=32,769 B | <=1 MiB |
| Delete operand | <=61,440 B | <=1 MiB |
| Evaluator buffer | <=1 MiB | <=1 MiB |
| Operation Q structural reservation | <=8 MiB expected | <=8 MiB; terminal 0 |
| Process RSS | <=24 MiB expected | <=32 MiB |
| Preparation wall | <10 s | <=30 s |
| APFS reset | <2 s | <=5 s |
| Measured workflow | <40–45 s expected | <60 s |
| Network | 0 | forbidden |
| Measured workflows | 1 | exactly 1 |
| Measured rows | 47 | exactly 47 |
| Durable state transitions | 34 | exactly 34 |

Store bytes, sealed native-oracle bytes, and total fixture bytes are reported
separately from the largest-user-file ceiling.

## 4. Resulting fixture structure

```text
target/layerfs-stage1-fixtures/apple-edge-v1/
├── bases/
│   └── base/                    sealed LayerFS Store at R0
├── source-native/
│   └── data/
│       └── payload.bin          25,165,824 bytes
└── master.json
```

`payload.bin` is generated by a fixed standard-library byte function and has:

```text
logical bytes       25,165,824 (24 MiB)
mode                frozen regular-file mode
mtime               frozen second + nanosecond pair
byte digest         frozen in master.json
canonical root      R0
StoreId/profile     frozen in master.json
```

Preparation:

```text
generate source-native/data/payload.bin
-> capture through a real external Apple workspace
-> obtain R0
-> close Store
-> Verified reopen and exact stream comparison
-> record APFS identity, StoreId, root, bytes, digest, profile, inventory
-> seal files 0444 and directories 0555
-> verify sealed master once
```

The preparation is reusable. Every measured workflow resets by one APFS clone;
it never regenerates the 24 MiB file.

## 5. Frozen native edit matrix

PRNG:

```text
algorithm  SplitMix64
seed       0x4c46_532d_4544_4745
margin     65,536 bytes for random middle edits
alignment  random middle offset must not be 4 KiB aligned
```

One SplitMix64 output is consumed for each overwrite/insert/delete row and no
output is consumed for append/truncate. For each consumed output:

```text
state += 0x9e37_79b9_7f4a_7c15 mod 2^64
z = state
z = (z xor (z >> 30)) * 0xbf58_476d_1ce4_e5b9 mod 2^64
z = (z xor (z >> 27)) * 0x94d0_49bb_1331_11eb mod 2^64
z = z xor (z >> 31)
available = current_length - delete_length - 2*65,536
offset = 65,536 + (z mod available)
if offset mod 4,096 == 0: offset += 1
```

All offsets below are frozen before implementation. Append and truncate use
the current EOF; all other offsets are fixed unaligned middle locations.

| Seq | Epoch | Kind | Size band | Offset | Delete B | Insert B | Before B | After B |
|---:|---:|---|---|---:|---:|---:|---:|---:|
| 1 | 1 | overwrite | 8 KiB − 1 | 3,378,088 | 8,191 | 8,191 | 25,165,824 | 25,165,824 |
| 2 | 1 | insert | 16 KiB | 4,221,363 | 0 | 16,384 | 25,165,824 | 25,182,208 |
| 3 | 1 | delete | 32 KiB + 1 | 19,479,758 | 32,769 | 0 | 25,182,208 | 25,149,439 |
| 4 | 1 | append | 8 KiB + 1 | 25,149,439 | 0 | 8,193 | 25,149,439 | 25,157,632 |
| 5 | 1 | truncate | 16 KiB | 25,141,248 | 16,384 | 0 | 25,157,632 | 25,141,248 |
| 6 | 2 | insert | 8 KiB | 13,344,955 | 0 | 8,192 | 25,141,248 | 25,149,440 |
| 7 | 2 | delete | 16 KiB + 1 | 19,223,620 | 16,385 | 0 | 25,149,440 | 25,133,055 |
| 8 | 2 | append | 32 KiB + 1 | 25,133,055 | 0 | 32,769 | 25,133,055 | 25,165,824 |
| 9 | 2 | truncate | 8 KiB | 25,157,632 | 8,192 | 0 | 25,165,824 | 25,157,632 |
| 10 | 2 | overwrite | 16 KiB − 1 | 2,461,634 | 16,383 | 16,383 | 25,157,632 | 25,157,632 |
| 11 | 3 | delete | 8 KiB + 1 | 19,138,305 | 8,193 | 0 | 25,157,632 | 25,149,439 |
| 12 | 3 | append | 16 KiB + 1 | 25,149,439 | 0 | 16,385 | 25,149,439 | 25,165,824 |
| 13 | 3 | truncate | 32 KiB | 25,133,056 | 32,768 | 0 | 25,165,824 | 25,133,056 |
| 14 | 3 | overwrite | 32 KiB − 1 | 9,130,636 | 32,767 | 32,767 | 25,133,056 | 25,133,056 |
| 15 | 3 | insert | 32 KiB | 11,257,438 | 0 | 32,768 | 25,133,056 | 25,165,824 |

Coverage equations:

```text
overwrite / insert / delete / append / truncate = 3 each
near-8-KiB / near-16-KiB / near-32-KiB         = 5 each
nonzero CDC replacements at minus/exact/plus     = 3 per threshold
random unaligned middle edits                    = 9
EOF edits                                        = 6
same-length edits                                = 3
growth edits                                     = 6
shrink edits                                     = 6
maximum modeled length                           = 25,182,208 bytes
terminal native-chain length                     = 25,165,824 bytes
```

Replacement bytes use a frozen per-row tag and deterministic generator. No
input depends on elapsed time, cache observations, roots, or prior failures.

## 6. Frozen direct-logical-edit and physical-refresh matrix

After native revision 15, keep the same managed workspace alive. Apply fifteen
direct canonical edits, moving `main` once per row, then refresh that exact
workspace to each accepted target root. This phase mirrors the native edit
classes so every operation kind has an independent `n=3` logical-to-physical
population.

PRNG:

```text
algorithm  SplitMix64
seed       0x4c46_532d_4c4f_4749
margin     65,536 bytes
bands      early / middle / late thirds of the safe interior
alignment  non-EOF offset must not be 4 KiB aligned
```

One SplitMix64 output is consumed for each overwrite/insert/delete row and no
output is consumed for append/truncate. The SplitMix64 step is identical to
section 5. For a non-EOF row:

```text
usable     = current_length - delete_length - 2*margin
band_start = margin + floor(usable * band_index / 3)
band_end   = margin + floor(usable * (band_index + 1) / 3)
offset     = band_start + (z mod (band_end - band_start))
if offset mod 4,096 == 0: offset += 1

band_index: early=0, middle=1, late=2
```

The exact offsets below control; the derivation prevents post-observation
population changes.

| Seq | Epoch | Kind | Size band | Position | Offset | Delete B | Insert B | Before B | After B | Honest route |
|---:|---:|---|---|---|---:|---:|---:|---:|---:|---|
| 16 | 4 | overwrite | 8 KiB - 1 | early | 3,167,684 | 8,191 | 8,191 | 25,165,824 | 25,165,824 | ClonePatch or InPlacePatch |
| 17 | 4 | insert | 16 KiB | middle | 9,979,080 | 0 | 16,384 | 25,165,824 | 25,182,208 | CloneShift or InPlaceShift |
| 18 | 4 | delete | 32 KiB + 1 | late | 20,965,809 | 32,769 | 0 | 25,182,208 | 25,149,439 | CloneShift or InPlaceShift |
| 19 | 4 | append | 8 KiB + 1 | EOF | 25,149,439 | 0 | 8,193 | 25,149,439 | 25,157,632 | CloneShift or InPlaceShift |
| 20 | 4 | truncate | 16 KiB | EOF | 25,141,248 | 16,384 | 0 | 25,157,632 | 25,141,248 | CloneShift or InPlaceShift |
| 21 | 5 | insert | 8 KiB | early | 3,990,642 | 0 | 8,192 | 25,141,248 | 25,149,440 | CloneShift or InPlaceShift |
| 22 | 5 | delete | 16 KiB + 1 | middle | 16,550,428 | 16,385 | 0 | 25,149,440 | 25,133,055 | CloneShift or InPlaceShift |
| 23 | 5 | append | 32 KiB + 1 | EOF | 25,133,055 | 0 | 32,769 | 25,133,055 | 25,165,824 | CloneShift or InPlaceShift |
| 24 | 5 | truncate | 8 KiB | EOF | 25,157,632 | 8,192 | 0 | 25,165,824 | 25,157,632 | CloneShift or InPlaceShift |
| 25 | 5 | overwrite | 16 KiB - 1 | late | 22,880,155 | 16,383 | 16,383 | 25,157,632 | 25,157,632 | ClonePatch or InPlacePatch |
| 26 | 6 | delete | 8 KiB + 1 | early | 4,308,809 | 8,193 | 0 | 25,157,632 | 25,149,439 | CloneShift or InPlaceShift |
| 27 | 6 | append | 16 KiB + 1 | EOF | 25,149,439 | 0 | 16,385 | 25,149,439 | 25,165,824 | CloneShift or InPlaceShift |
| 28 | 6 | truncate | 32 KiB | EOF | 25,133,056 | 32,768 | 0 | 25,165,824 | 25,133,056 | CloneShift or InPlaceShift |
| 29 | 6 | overwrite | 32 KiB - 1 | middle | 10,813,201 | 32,767 | 32,767 | 25,133,056 | 25,133,056 | ClonePatch or InPlacePatch |
| 30 | 6 | insert | 32 KiB | late | 19,272,909 | 0 | 32,768 | 25,133,056 | 25,165,824 | CloneShift or InPlaceShift |

Coverage equations:

```text
overwrite / insert / delete / append / truncate = 3 each
near-8-KiB / near-16-KiB / near-32-KiB         = 5 each
nonzero CDC replacements at minus/exact/plus     = 3 per threshold
early / middle / late non-EOF locations          = 3 each
EOF edits                                        = 6
same-length patch-eligible refreshes              = 3
accepted-splice count-changing shift refreshes    = 12
unproven FullFallback refreshes                    = 0
growth / shrink refreshes                         = 6 / 6
maximum modeled length                            = 25,182,208 bytes
terminal logical-refresh-chain length             = 25,165,824 bytes
```

The benchmark must use the opaque receipt returned by the real direct logical
edit. It must not reconstruct a receipt from the schedule or relabel an
unproven `FullFallback` as incremental. Exact bytes, exact root, honest route,
suffix equations, and real wall time are hard.

## 6.1 Frozen multi-edit save bursts

After revision 30, apply four exact managed-native edit sequences. Each
sub-edit mutates the real APFS file and receives a complete physical oracle;
the sequence then produces one durable checkpoint and one publication COMMIT.
There is no intermediate canonical publication inside a burst.

| Root | Pattern | Ordered sub-edits | Before B | After B |
|---:|---|---|---:|---:|
| R31 | autosave hotspot | eight 4,096-B overwrites at offsets 8,388,611; 8,391,683; 8,394,755; 8,397,827; 8,400,899; 8,403,971; 8,407,043; 8,410,115 | 25,165,824 | 25,165,824 |
| R32 | insertion-boundary edit | insert 16,384 B at 12,582,913; overwrite 8,192 B at 12,595,201; delete 12,288 B at 12,591,105 | 25,165,824 | 25,169,920 |
| R33 | append/rotation | append 8,192 B; append 16,384 B; append 32,768 B; truncate 61,440 B from EOF | 25,169,920 | 25,165,824 |
| R34 | alternating distant edits | overwrite 4,096 B at 1,048,579; 8,192 B at 24,117,251; 4,096 B at 2,097,157; 8,192 B at 23,068,673; 4,096 B at 3,145,731; 8,192 B at 22,020,099 | 25,165,824 | 25,165,824 |

Exact sub-edit schedule:

| Tag | Kind | Offset | Delete B | Insert B | Before B | After B |
|---|---|---:|---:|---:|---:|---:|
| R31.1 | overwrite | 8,388,611 | 4,096 | 4,096 | 25,165,824 | 25,165,824 |
| R31.2 | overwrite | 8,391,683 | 4,096 | 4,096 | 25,165,824 | 25,165,824 |
| R31.3 | overwrite | 8,394,755 | 4,096 | 4,096 | 25,165,824 | 25,165,824 |
| R31.4 | overwrite | 8,397,827 | 4,096 | 4,096 | 25,165,824 | 25,165,824 |
| R31.5 | overwrite | 8,400,899 | 4,096 | 4,096 | 25,165,824 | 25,165,824 |
| R31.6 | overwrite | 8,403,971 | 4,096 | 4,096 | 25,165,824 | 25,165,824 |
| R31.7 | overwrite | 8,407,043 | 4,096 | 4,096 | 25,165,824 | 25,165,824 |
| R31.8 | overwrite | 8,410,115 | 4,096 | 4,096 | 25,165,824 | 25,165,824 |
| R32.1 | insert | 12,582,913 | 0 | 16,384 | 25,165,824 | 25,182,208 |
| R32.2 | overwrite | 12,595,201 | 8,192 | 8,192 | 25,182,208 | 25,182,208 |
| R32.3 | delete | 12,591,105 | 12,288 | 0 | 25,182,208 | 25,169,920 |
| R33.1 | append | 25,169,920 | 0 | 8,192 | 25,169,920 | 25,178,112 |
| R33.2 | append | 25,178,112 | 0 | 16,384 | 25,178,112 | 25,194,496 |
| R33.3 | append | 25,194,496 | 0 | 32,768 | 25,194,496 | 25,227,264 |
| R33.4 | truncate | 25,165,824 | 61,440 | 0 | 25,227,264 | 25,165,824 |
| R34.1 | overwrite | 1,048,579 | 4,096 | 4,096 | 25,165,824 | 25,165,824 |
| R34.2 | overwrite | 24,117,251 | 8,192 | 8,192 | 25,165,824 | 25,165,824 |
| R34.3 | overwrite | 2,097,157 | 4,096 | 4,096 | 25,165,824 | 25,165,824 |
| R34.4 | overwrite | 23,068,673 | 8,192 | 8,192 | 25,165,824 | 25,165,824 |
| R34.5 | overwrite | 3,145,731 | 4,096 | 4,096 | 25,165,824 | 25,165,824 |
| R34.6 | overwrite | 22,020,099 | 8,192 | 8,192 | 25,165,824 | 25,165,824 |

Burst equations:

```text
sub-edits                                      = 8 + 3 + 4 + 6 = 21
durable checkpoints                           = 4
writer transactions / publication COMMITs     = 4 / 4
maximum descriptors in one burst              = 8
maximum workflow length                       = 25,227,264 bytes
terminal length                               = 25,165,824 bytes
```

Every burst receipt serializes each sub-edit's exact tag, offset, delete,
replacement and before/after length. Replay order is authority; overlapping
edits are never sorted or coalesced into a different semantic sequence.

## 7. End-to-end operation sequence

```text
admit sealed fixture/source/executable/APFS identity
  -> clone sealed Store once
  -> TrustedLocalDev reopen at R0
  -> cold managed materialization
  -> epoch 1: five physical edits, checkpoint after each
  -> fresh Verified open/read session; verify R0 and R5
  -> epoch 2: five physical edits, checkpoint after each
  -> fresh Verified open/read session; verify R0, R5, R10
  -> epoch 3: five physical edits, checkpoint after each
  -> fresh Verified open/read session; verify R0, R5, R10, R15
  -> epoch 4: five direct logical edits, refresh after each
  -> fresh Verified open/read session; verify R0, R15, R20
  -> epoch 5: five direct logical edits, refresh after each
  -> fresh Verified open/read session; verify R0, R15, R20, R25
  -> epoch 6: five direct logical edits, refresh after each
  -> fresh Verified open/read session; verify R0, R15, R20, R25, R30
  -> four frozen multi-edit native bursts; checkpoint once after each burst
  -> convert managed workspace to external handle
  -> stream-compare the R34 APFS file with the independent oracle
  -> fresh materialize R15, R30, and R34 into independent destinations
  -> compare every milestone's physical bytes + metadata
  -> terminal resource and cleanup proof
```

## 8. Exact row population

| ID | Rows | Contents |
|---|---:|---|
| C00 | 1 | source, executable, master, APFS, profile, and schedule admission |
| C01 | 1 | APFS clone reset and distinct-inode proof |
| C02 | 1 | cold managed materialization |
| C03 | 15 | one native edit plus one durable checkpoint per row |
| C04 | 3 | fresh Verified open/read session plus retained-history oracle after native epochs 1–3 |
| C05 | 15 | one direct logical edit plus one live-workspace refresh per row |
| C06 | 3 | fresh Verified open/read session plus retained-history oracle after logical epochs 4–6 |
| C07 | 4 | one frozen multi-edit native burst plus one durable checkpoint per row |
| C08 | 3 | fresh materialize R15, R30, and R34 with independent native/canonical oracle; the R34 row also owns managed-to-external conversion and live-tree comparison |
| C09 | 1 | terminal resources and cleanup |
| **Total** | **47** | fixed before the first measured row |

Combining native edit and checkpoint into one C03 row and a whole save burst
into one C07 row keeps custody small while retaining every sub-operation,
separate timers and counter snapshots.

## 9. Independent oracle

The benchmark oracle must not call LayerFS to calculate expected bytes.

Use a bounded evaluator-only piece table:

```text
Original { source_offset, length }
Inserted { row_tag, generated_offset, length }
```

Required operations:

```text
splice(start, delete_length, replacement_descriptor)
logical_length()
stream_range(range, sink)
stream_all(sink)
snapshot()
coalesce_adjacent_compatible_pieces()
```

Bounds:

```text
descriptors in one live table  <= 103 before coalescing
all 35 root snapshots          <= 1,315 descriptors before coalescing
shared replacement backing     = 495,616 bytes (484 KiB)
full expected-file Vec         forbidden in measured runner
comparison buffer              <=1 MiB
```

Before implementation, a focused unit test must apply all 51 exact splice
sub-operations (30 individual edits plus 21 burst sub-edits) to a reduced
`Vec<u8>` and require piece-table equality after every splice and every
retained snapshot.

Oracle checkpoints:

| Revision | Retained proof |
|---:|---|
| 0 | full digest + start/middle/end 64 KiB probes |
| 5 | full digest + start/middle/end probes |
| 10 | full digest + start/middle/end probes |
| 15 | full digest + start/middle/end probes |
| 20 | full digest + start/middle/end probes |
| 25 | full digest + start/middle/end probes |
| 30 | full digest + start/middle/end probes |
| 31–34 | full digest and exact live physical stream after every burst |

The R0–R34 full digest is produced once by the exact full-byte oracle attached
to C02 or its transition row and retained with the accepted root. Fresh C04/C06
history rows reuse that already-synced root-bound digest; they do not regenerate
the same 24 MiB expected digest. Each history read still performs all three
independent 64 KiB byte comparisons per selected root.

Every one of the 63 history probes retains an ordered receipt with its exact
root, ordinal, range, wall, path/plan counters, rope counters, payload-batch
counters, and fetched/authentication/role-decode delta. Ordinal 1 is a plan miss;
ordinals 2 and 3 must be exact root/path plan hits. Probe deltas sum exactly to
the row `history_read` phase.

Fresh milestone materialization oracles:

```text
R15  physical-to-logical chain terminal
R30  logical-to-physical chain terminal
R34  multi-edit burst chain terminal
```

## 10. Measurement boundaries

C03 row:

```text
row_wall
  = native_edit_wall
  + live_physical_oracle_wall
  + durable_checkpoint_wall
  + canonical_witness_wall
  + counter_snapshot_wall
  + row_residual_wall
```

Every applicable product row also takes zero-SQL cumulative counter snapshots
at these boundaries:

```text
before product operation
after logical edit or checkpoint
after APFS refresh when applicable
after canonical witness
```

The retained phase deltas independently close authentication, role decode,
new/incumbent object, transaction/COMMIT, Q, and storage equations and sum
exactly to the row aggregate. VFS-operation scratch receipts are retained
separately inside the owning phase, then combined with Engine scratch using the
same row-level aggregate equation. Snapshot calls do not execute Store SQL.

C05 row:

```text
row_wall
  = direct_logical_edit_wall
  + changed_root_refresh_wall
  + live_physical_oracle_wall
  + canonical_witness_wall
  + counter_snapshot_wall
  + row_residual_wall
```

C07 burst row:

```text
row_wall
  = sum(native_subedit_wall)
  + sum(per_subedit_physical_oracle_wall)
  + durable_checkpoint_wall
  + canonical_witness_wall
  + counter_snapshot_wall
  + row_residual_wall
```

Complete wall:

```text
complete_wall
  = admission
  + APFS reset
  + Store open
  + managed materialization
  + sum(C03 row walls)
  + sum(C04 reopen/history walls)
  + sum(C05 row walls)
  + sum(C06 reopen/history walls)
  + sum(C07 burst row walls)
  + sum(C08 milestone materialization/oracle walls)
  + cleanup
  + artifact writes
  + timer residual
```

Oracle work is outside operation latency but inside row and complete wall. No
hash, stream comparison, or reopen is hidden from the complete timer.

Every C00–C09 row, not only C03/C05/C07, closes:

```text
row_wall = sum(non-overlapping named row phases) + row_residual
```

The complete equation sums all 47 row walls plus only work proven to be
outside every row. Final `summary.json`, `summary.md`, and `campaign-time.txt`
rewrites necessarily occur after the wall they publish and are listed as
explicit terminal receipt rewrites outside accounted wall.

## 11. Statistics

Retain every raw observation. Use nearest-rank order statistics.

```text
n=3: p50=x2, p95=x3
n=4: p50=x2, p95=x4
n=5: p50=x3, p95=x5
n=6: p50=x3, p95=x6
n=12: p50=x6, p95=x12
n=15: p50=x8, p95=x15
n=19: p50=x10, p95=x19
n=51: p50=x26, p95=x49
```

Report:

| Population | n | Metrics |
|---|---:|---|
| native edit by kind | 3 each | min/p50/p95/max/range/sum |
| native edit by size band | 5 each | min/p50/p95/max/range/sum |
| individual-edit durable checkpoint | 15 | raw + min/p50/p95/max/range/sum |
| all managed durable checkpoints | 19 | raw + min/p50/p95/max/range/sum |
| edit plus checkpoint | 15 | raw + min/p50/p95/max/range/sum |
| direct logical edit by kind | 3 each | min/p50/p95/max/range/sum |
| direct logical edit by size band | 5 each | min/p50/p95/max/range/sum |
| same-size refresh | 3 | raw + min/p50/p95/max/range/sum |
| accepted-splice shift refresh by kind | 3 each for insert/delete/append/truncate | raw + min/p50/p95/max/range/sum; never mix with patch or FullFallback |
| logical edit plus refresh | 15 | raw + min/p50/p95/max/range/sum |
| burst save plus checkpoint | 4 | retain each pattern separately plus aggregate raw/sum |
| physical oracle | 51 sub-operations | raw + min/p50/p95/max/range/sum |
| fresh Verified open/history | 6 | raw + min/p50/p95/max |
| cold materialization | 1 | wall + MiB/s |
| milestone fresh materialization | 3 | raw + min/p50/p95/max + MiB/s |
| complete workflow | 1 | wall |

All per-operation latency results are initially `REPORT_ONLY`. Hard PASS/FAIL
comes from correctness, resource, population, custody, and complete-wall gates.

Planning expectations, not acceptance thresholds:

| Operation | Expected order |
|---|---:|
| same-length native edit | low milliseconds |
| count-changing native edit | position/suffix dependent |
| durable checkpoint | low tens of milliseconds |
| fresh Verified open of 24 MiB history | report measured scrub cost |
| same-size refresh | tens of milliseconds |
| count-changing accepted-splice refresh | suffix-position dependent; append/truncate have zero shifted suffix |
| multi-edit save burst | low tens of milliseconds plus sub-edit native work |
| complete workflow | 30–60 seconds |

## 12. Native route equations

Same-length native edit:

```text
route                     ClonePatch or InPlacePatch
patch_bytes               replacement bytes
suffix_bytes_shifted      0
ClonePatch                clone_attempts=1, clone_successes=1
InPlacePatch fallback     clone_attempts=1, clone_fallbacks=1
```

Count-changing native edit:

```text
route                     InPlaceShift required
suffix S                  pre_length - (offset + delete_bytes)
suffix_bytes_shifted      S
native.bytes_read         S
native.bytes_written      S + replacement_bytes
aggregate native bytes    2S + replacement_bytes
```

Changed-root refresh:

```text
same-size overwrite       ClonePatch or InPlacePatch
accepted insert/delete    CloneShift or InPlaceShift
accepted append/truncate  CloneShift or InPlaceShift
unknown root provenance   explicit FullFallback
exact target bytes/root   required for every route
suffix S                  pre_length - (offset + delete_bytes)
native.bytes_read         S
native.bytes_written      S + replacement_bytes
full_fallback_files       0 for the twelve accepted-splice refreshes
workspace rematerialize   0; the same live workspace is retained
```

Multi-edit burst:

```text
each native sub-edit      exact ordinary native route and byte equation
physical oracle           exact after every sub-edit
checkpoint                one after the final sub-edit only
writer transaction        1 per burst
publication COMMIT        1 per burst
sub-edit reorder          forbidden
row native aggregate      exact sum of all retained sub-edit native counters
```

## 13. Canonical and complexity gates

Every checkpoint and direct logical edit:

```text
generation_after                       = generation_before + 1
current head                           = exact returned RefState
writer transactions                    = 1
transactions committed                 = 1
transactions rolled back               = 0
publication COMMITs                    = 1

fetched_rows
  = fetched_row_authentication_passes
  = fetched_row_role_decode_passes

new_object_authentication_passes
  = created_rows + reused_rows
  = put_lookup_statements

incumbent_authentication_passes        = reused_rows
put_insert_statements                  = created_rows
objects_created                        = created_rows
objects_reused                         = reused_rows
objects_validated
  = fetched_row_authentication_passes
  + new_object_authentication_passes
  + incumbent_authentication_passes

total state-changing transitions              = 15 + 15 + 4 = 34
total writer transactions / publication COMMITs = 34 / 34
publication_transactions_started              = publication COMMITs + publication rollbacks
admission_transactions_started                = admission commits + admission rollbacks
integrity_transactions_started                = integrity commits + integrity rollbacks
admission/integrity/publication statements    = actual SQL at the named boundary
retained_roots_validated                      = actual disk-backed unique-root claims
```

Trusted changed-spine gate:

```text
content CDC bytes scanned              = replacement bytes
content payload bytes written          = replacement bytes
unaffected canonical payload reads     = 0
unaffected canonical payload writes    = 0
content edit directory nodes emitted   = 0
payload batch maximum                  <=64
H                                      = pre-edit extent-tree level
rope nodes read                        <=16*(H+1)
rope nodes emitted                     <=16*(H+1)+ceil(B/8KiB)+2
```

For one C07 burst, `B` is the sum of supplied replacement bytes across its
ordered sub-edits. CDC, payload-write and rope-node gates close both per
sub-edit and as the exact sum of those sub-edits. There is still one writer
transaction and one publication COMMIT for the burst.

Complexity claims:

```text
logical edit          O(B + log E + path)
checkpoint replay     O(B + log E + path) plus one durable publication
random witness read   O(log E + C_R + R)
same-size refresh     changed Merkle spines + changed native bytes
accepted count change O(S + B) native bytes after authenticated path/root validation
unknown root change   Theta(changed file bytes) FullFallback, labeled honestly
history read          direct immutable root read; no replay
```

The benchmark must fail if a supposedly local logical edit reads or rewrites an
unaffected suffix.

All C04/C06 historical reads and C08 milestone reconstructions also require
fetched/auth/decode equality, payload-batch `<=64`, zero writer transactions,
and zero native/CDC write work.

Storage reporting closes separately from logical-file size:

```text
initial / terminal database bytes
initial / terminal logical-engine bytes
per-checkpoint database growth
maximum single-row database growth
cumulative canonical object bytes written
physical database growth / canonical object bytes written
append-only database size never moves backward
rollback-journal peak = Unavailable unless continuously observed
terminal journal/WAL/SHM sidecars = absent
```

## 14. Physical correctness gates

After every native edit, direct logical refresh, and C07 burst sub-edit:

```text
stream live APFS payload.bin through AppleDriver
-> compare every byte to independent piece-table oracle
-> compare exact logical length
```

After every checkpoint:

```text
current head = exact returned RefState
new root reconstructs exact current oracle
prior selected roots remain exact
live workspace materializations total = 1; C08 witnesses counted separately
workspace_reuses increments       = 1
rematerializations                = 0
descriptor resets                = 1
```

For one C07 burst, descriptor reset and durable publication occur once after
the final sub-edit, while the live physical oracle occurs after every
sub-edit.

After every direct logical edit and refresh:

```text
current head                    = exact direct-edit RefState
target canonical root           = exact oracle
live physical file              = exact oracle
live workspace materializations total = 1; C08 witnesses counted separately
workspace_reuses increments      = 1
rematerializations               = 0
same-size overwrite              = patch route
accepted count-changing operation = CloneShift or InPlaceShift
accepted suffix/read/write       = exact S / S / S+B
FullFallback operations          = 0
```

During every fresh Verified open/read session while the original managed
workspace remains alive:

```text
exact head recovered
Verified-after-Trusted scrub succeeds
R0 and selected roots R5/R10/R15/R20/R25/R30 readable directly
start/middle/end probes exact
no native authority used for historical reads
Store connection high-water <=2
```

One Verified retained-union scrub may retain authenticated payload lengths in
its existing disk-bounded scratch database. The union closure still fetches,
identity-authenticates, and role-decodes every unique immutable payload. Each
root still validates its namespace reachability, reference counts, file state,
mapping nodes, extent summaries, and payload slice bounds; the already
authenticated length replaces only repeated payload-byte fetches. The scratch
namespace is cleared/dropped with the scrub and is never a persistent trust
cache or changed-only shortcut.

Final:

```text
managed live R34 APFS tree  = independent oracle
fresh R15 materialization   = retained R15 oracle
fresh R30 materialization   = retained R30 oracle
fresh R34 materialization   = independent final oracle
managed R34 tree            = fresh R34 materialization
mode/xattr                  = frozen supported invariants for every milestone
mtime sec/ns                = exact milestone oracle; live R34 equals fresh R34
R34 mtime                   = recorded exactly; not claimed equal to initial fixture mtime
extra user files            = 0 in every destination
```

## 15. Resource gates

| Resource | Gate |
|---|---:|
| Largest product-buffer structural bound | <=1 MiB |
| Operation Q structural-reservation high-water | <=8 MiB |
| Operation Q reservation terminal | 0 after every operation |
| Process RSS peak | <=32 MiB |
| Store cache profile | page 4096; cache 1280; spill 1280 |
| Scratch cache/spill | bounded default, source/executable-bound |
| Payload batch | <=64 references |
| Store connections during C04/C06 | <=2 |
| Active Store connections after dropping every handle | 0 |
| FD terminal | baseline |
| Owned temp/journal/WAL/SHM residue | 0 |
| Product-operation child processes | 0 |
| Terminal child processes | 0; reset/custody helpers reported separately |
| Long-lived converted workspace residue | 0 after explicit owned cleanup |
| Fresh R15/R30/R34 materialization residue | 0 after explicit owned cleanup |

Unavailable values must be reported as `Unavailable`, never zero.

The product-buffer value is the source-bound maximum admitted individual
product buffer, not the evaluator's oracle buffer and not allocator telemetry.
Operation Q is a conservative per-operation structural reservation; report it
as a reservation, never as measured resident allocation.

C09 first performs explicit owned cleanup, drops every `LayerFs`, managed, and
external handle, and only then invokes the existing external FD/connection/
process/temp/residue observers. Destructor-only recursive deletion is not
terminal evidence.

Ordinary rows use the exact SDK/Engine connection count, `/dev/fd`, `getrusage`
peak RSS, and in-process residue traversal, with structural product child count
zero. One known connection-high-water row and both terminal observations retain
the external `ps`/`lsof`/`pgrep` proof. Because `getrusage` is a peak rather than
a current-RSS observer, ordinary `rss_current_bytes` values are `null` plus an
`Unavailable` receipt; decisive external rows retain a numeric current RSS.

## 16. Zero-row readiness

Before the first measured row, readiness must prove:

```text
source and release executable hashes match
fixture master and APFS identity match
StoreId/profile match
fixture remains sealed
exact 47-row schedule is serialized
all 51 edit/sub-edit tags, offsets, lengths and ordering are serialized
all 34 expected state transitions and milestone roots are serialized
one reset observation passes <=5 s
forecast leaves reserve below 60 s
measured_rows_started = false
run directory does not exist
```

A forecast `>=60 s` is a hard readiness failure. It is not permission to
reduce the matrix. If a measured workflow reaches 60 seconds, stop at the
diagnostic boundary, preserve every completed row and do not rerun unchanged
source.

A failed readiness receipt is append-only. It is not permission to alter the
matrix after observing product timings.

Readiness is durably published in the existing external Stage-One readiness
location while the run directory is still absent. C00 creates the new run and
copies the exact readiness bytes plus their digest into it.

## 17. Artifact structure

```text
target/layerfs-stage1-apple-edge-<timestamp>/
├── environment.json
├── master.json
├── readiness.json           exact admitted external receipt copy
├── schedule.json
├── rows.jsonl
├── summary.json
├── summary.md
├── campaign-time.txt
└── stderr.txt              only when nonempty failure exists
```

Every row records:

```text
sequence / epoch / operation class / size band
direction: physical-to-logical / logical-to-physical / burst / witness
offset / delete / replacement / pre-length / post-length
ordered sub-edit array for C07
ordered per-probe array for C04/C06
native route and exact byte counters
native-edit, logical-edit, refresh, checkpoint, oracle, and row walls
pre/post RefState
operation counters and engine deltas
zero-SQL phase counter deltas
database and scratch observations
Q and process-resource observation
oracle length/digest and prior-root witness
```

Rows are append-only and synced before the terminal summary. A failure preserves
all completed rows and the exact first failing equation.

The exact Markdown section order, JSON/JSONL field names, units, availability
rules, statistics objects and final agent response are controlled by `poc/18`.

## 18. PASS / REVISE / FAIL

PASS:

```text
exact 47-row population and 51 serialized edit/sub-edit operations
all physical and canonical byte oracles exact
all 34 state transitions and retained history/reopen/refresh roots exact
all four save bursts preserve exact ordered semantics
all transaction/authentication/locality equations exact
all route labels honest
all resource gates pass
complete wall <60 s
fixture/master unchanged
terminal cleanup exact
```

REVISE:

```text
all hard correctness/resource gates pass
but a report-only latency exposes a product bottleneck worth repairing
```

FAIL:

```text
any byte/root/history/durability/authentication mismatch
unexpected suffix canonical work
unreported fallback/rematerialization
resource/time hard-gate violation
population/custody/timer defect
cleanup residue
```

No latency threshold may be introduced or weakened after observing this first
population.

## 19. Prospective implementation map

Design only; no file below is changed by this document.

```text
tools/layerfs-eval/src/
├── main.rs                   tiny command dispatch only
├── stage1_edge.rs            fixture, schedule, piece table, runner, receipts
└── stage1_fixture.rs         visibility-only reuse of generic seal/verify helpers
```

Expected commands:

```text
layerfs-eval stage1 prepare apple-edge
layerfs-eval stage1 readiness apple-edge
layerfs-eval stage1 run apple-edge <new-run-directory>
```

No product crate, dependency, canonical schema, SQLite schema, benchmark
framework, Python runner, or shell harness should be added for this benchmark.

## 20. Execution stages

| Stage | Work | Exit condition |
|---|---|---|
| D0 | Freeze this document and exact matrix | No open workload choice |
| D1 | Implement/test piece-table oracle and schedule | Reduced Vec model exact after all 51 edit/sub-edit operations and 35 snapshots |
| D2 | Implement reusable sealed 24 MiB fixture | Fresh Verified reopen exact; prep <=30 s |
| D3 | Implement real AppleDriver runner and receipts | Focused runner checks pass |
| D4 | One workspace fmt/check/test/clippy closure | Zero failures/warnings |
| D5 | One release build and zero-row readiness | Hash-matching PASS; zero rows |
| D6 | One <60 s campaign | Immutable PASS/REVISE/FAIL artifact |
| D7 | Independent terminal audit | Honest disposition; no Stage 1.2 or mounted Stage Two work started |

The fastest valid path is one source settlement, one preparation, one
readiness, and one measured workflow.

An accepted D7 disposition makes Stage 1.2 eligible. It does not authorize or
begin mounted Stage Two.
