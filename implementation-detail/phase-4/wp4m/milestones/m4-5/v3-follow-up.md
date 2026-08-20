# WP4-M M4.5 final checkpoint-quality follow-up

## Disposition and scope

- Date: 2026-08-19.
- Final checkpoint disposition: **PASS; ready for a separate F0 freeze**.
- The private exact-XOR same-count changed-spine algorithm remains accepted.
- The retained v3 terminal campaign and report remain unchanged as the prior
  accepted baseline.
- A release-path capacity-adoption guard changed executable bytes, so this
  follow-up uses the fresh versioned v4 campaign at
  `target/wp4m-m45-checkpoint-k64-20260819-v4/`.
- Qualification, promotion, profile selection/rejection, production
  integration, F0 source work, and later Phase 4 work remain not started.

## Controlling-comparison clarification

`spec.md` §13.5A records the terminal causal
comparison as one executable with C0 complete-closure versus C1 changed-spine
pre-COMMIT qualification. CDC, CAS/COW mutation, copied pair base, authority,
expectations, COMMIT, reopen, scrub, reconstruction, ranges, counters, and
reporting are identical. Retained M3 is historical continuity evidence only.

This is a post-measurement wording clarification, not a retroactive workload
change. The accepted v3 measured-spec SHA-256 remains
`55980c049e5e3ce824664070c11c358428c69ad1fb4f3a4fc0af925ce941756b`.
The v4 measured-spec SHA-256, including the clarification, is
`739620380446c8fc2fee5f7edc96c867bc32ed83bb6b54dcc98ecd76d5eab4c8`.
Sections 13.3 and 13.3A, all frozen XOR identities, and the v3 artifacts were
not rewritten.

## Capacity-adoption repair

`ChargedVec::from_exact_builder` already precharged the declared element
count but previously accepted a separately allocated `Vec` by length alone.
It now requires both exact length and exact capacity. A `len=declared,
capacity>declared` return is rejected as `CoreError::AllocationFailed`; the
precharge and vector both drop and logical Q returns to zero.

This one shared check covers the existing file, delta, and directory canonical
builders. It adds no allocator abstraction and does not alter the authoritative
96-byte file-reference, 256-byte tree-node, or 64-byte DFS-frame charges.

Focused proof:

```text
test tests::exact_builder_rejects_excess_capacity_and_cleans_q ... ok
test result: ok. 1 passed; 0 failed
```

## Direct H=2 changed-spine evidence

The synthetic fixture reuses existing canonical encoding and store helpers. It
constructs shared immutable nodes instead of a giant source buffer:

```text
K/F                         64/64
references                  262,145 = 64*64*64 + 1
leaf occurrences            4,097
level-1 branch occurrences  65
derived root level / H      2
root children               2
```

The valid edit changes:

- ordinals 63-64, crossing a leaf boundary;
- ordinal 4,096, the first reference in the first leaf of a second inner
  branch; and
- ordinal 262,144, the final partial leaf under the second root child.

Exact direct counters:

| Counter | Value |
|---|---:|
| Changed leaf union | 4 |
| Changed branch union | 5: three level-1 plus two level-2 |
| Prior spine objects authenticated | 11 |
| Replacement spine objects authenticated | 11 |
| Receipt-covered equal edges | 376 |
| New/different edges | 14 |
| Fully authenticated new chunks | 4 |
| C0 complete-closure occurrences | 266,309 |
| C0 SQL queries | 266,318 |
| C1 complete-closure occurrences | 0 |
| C1 SQL queries | 34 |
| C1 leaf-batch queries | 0 |
| Two-sided active-ancestry charge | 640 bytes = `(H+3)*64*2` |
| C1 exact Q high-water | 43,488 bytes |
| Terminal Q | 0 |

The same test constructs a malformed cumulative summary in a deep level-1
branch. C0 and C1 both reject it as typed `CoreError::LengthMismatch` before
publication. The test output is:

```text
deep H=2 leaves=4 branches=5 covered=376 different=14 q_high_water=43488 c0_queries=266318 c1_queries=34
test tests::deep_changed_spine_proves_height_union_and_bounded_qualification ... ok
test result: ok. 1 passed; 0 failed
```

This directly exercises the H-dependent `F*H`, `H^2`, and active-ancestry
terms without changing the algorithm or its complexity bounds.

## Correctness and build gates

```text
cargo test --workspace --offline --all-targets
98 passed; 0 failed
  44 layerfs-core
   4 layerfs-engine library
  33 phase4_create_edit_benchmark
  12 phase4_engine_parity
   5 layerfs-eval
```

```text
cargo clippy --workspace --offline --all-targets -- -D warnings
PASS; no warning or error

cargo fmt --all -- --check
PASS; no difference after applying rustfmt

git diff --check HEAD
PASS; no whitespace error
```

Debug self-test:

```text
self-test PASS root=f1cfdd7fdd658506caf39ace169dd83f1a95fbb25aafcbb7f9b72864f9d2e42a objects=20 auth_bytes=1054925
```

## v4 custody

| Item | SHA-256 / value |
|---|---|
| Branch / HEAD | `codex/empty-worktree` / `f3df30a80172131b74b5949a6a55234c962dac67` |
| Measured implementation diff | `efc18e05d85c0ecb7a7dc02dd72205d873ad173521848800614511a7f1a1f449` |
| Final tracked dirty diff | `49d1734aae97d30cadc2d7224e6729e40c22eef91f625e8dbaf40ecd9061d281` |
| Benchmark source | `0a078b25216fdc4da83722807dd8e921b523f99f074c86e5480a38e2a9ea2061` |
| Release executable | `7c395935457f99acfd3b02e08cdba976b971c61a407aeb65c243f2f26ddaf1a2` |
| Measured controlling spec | `739620380446c8fc2fee5f7edc96c867bc32ed83bb6b54dcc98ecd76d5eab4c8` |
| Final complexity analysis | `3a6892a44755f4492765391e67cadfc99a9b7aff5b7cdc6a9aecc6e6f5237660` |
| Fixture / manifest | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` / `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca` |
| Raw JSONL | `411d1c4144c20b06cbadd17bd72f21b0c6c85e4d6fcdea0fae320a1b4a949a0c` |
| Preflight | `84a1ff0a18c44e6978aab3d91c908f1a760f22ba2c1a0d72544c8b91441888c7` |
| Summary / independent | `1d189cd135cec493ad065fb808c18af0f5995160cea3f2e944479325f8d4d478` / `f5f520cb25c9fa520be300f9262c88a4f8e199f591d8dfcf6e72fa94ffbfacb3` |
| Final-hashes manifest | `533be670b8c5863e8de4645de4507bfe2b7d20ca35c399434c3422733f460eaa` |
| Complete 61-file manifest | `1b1621735ad949abe4755e94dcd2487699af5502479dd99b707cc4d4a20e99c1` |

All 61 complete-manifest entries verify. All 12 official C0/C1 arm copies
match their pair's database, authority sidecar, and expectation base. Every
expectation file hashes to
`70520375af87d5227e28775a59879067d3b942cd82eb3f2fd2e15bb942b169ff`.

Prepared bases:

| Pair | Database SHA-256 | Authority SHA-256 |
|---:|---|---|
| 0 warmup | `1c0d5f4ed39e57808253f031b07d36583a2617e9a550655397c5eb43f0f0eaec` | `2cba86af9312f0fba31153a1d54363ff48042a2661a2fd341135a8bb53cbdd01` |
| 1 | `188083cc09589ca77bc7e335390766d69423c0a1bdde6ddca436d7262e62050c` | `1f31181f072481df35c7ba884268fd21648b8a037c63abe53080baf5ce7ab51f` |
| 2 | `b5d69d8b77cf370a4bb50652070b8031f0a7fd8046b4fd9b8a1ed6271a3f6487` | `f2f7fe3beb71fa7b61e9925d871fbc9ce55fe6e02b442b40ef9b41d144e89d24` |
| 3 | `17e0117f70377006ab8fa3e1f8d74b8b634ddd624975242c3331db4f2ff71bd4` | `22b2b7a6cbadc8db8efed270e9ce3a43105b8b569c32f6aa6aeedd15cfcbb0ea` |
| 4 | `b840ba900665330af05dee25d96ea3713759122d1bacd040781c4bcdbc0bb8e4` | `201e9d410cf3bd397ec5b5455698acf6e8d0483ced7fd8c10ccb46681134e19b` |
| 5 | `4491dc740e63267e6e507b729fbd22ea2517c4606d7cea1a2162246b8e35302a` | `a94d38e271a5441d07f41155fdee0dfa567dacd84301ebbc8b9a44fc1b2873d7` |

## v4 performance result

| Metric | C0 | C1 | Result |
|---|---:|---:|---|
| Durable edit median | 446.457042 ms | 8.540708 ms | **-98.087003%**, 5/5 wins |
| Min / max / spread | 440.208000 / 470.015792 / 29.807792 ms | 8.027541 / 14.043292 / 6.015751 ms | affected-speed PASS |
| Mapping + CDC/COW median | 6.660084 ms | 6.546500 ms | common path |
| Pre-COMMIT qualification median | 437.020042 ms | 0.284500 ms | changed variable |
| SQLite COMMIT median | 2.468083 ms | 1.935000 ms | common path |
| Same-open authority median | 238.427958 ms | 238.941167 ms | separate linear phase |
| Post-publication median | 703.200667 ms | 704.987209 ms | separate linear phase |
| Same-open lifecycle median | 1,153.324459 ms | 716.367834 ms | not the local timer |
| First-open lifecycle median | 1,394.917293 ms | 953.201251 ms | includes authority |

Paired durable rows:

| Pair | Order | C0 ms | C1 ms | Delta ms | Delta % |
|---:|---|---:|---:|---:|---:|
| 1 | AB | 450.123792 | 8.540708 | -441.583084 | -98.102587% |
| 2 | BA | 440.208000 | 9.128958 | -431.079042 | -97.926217% |
| 3 | AB | 442.291334 | 14.043292 | -428.248042 | -96.824877% |
| 4 | BA | 446.457042 | 8.027541 | -438.429501 | -98.201945% |
| 5 | AB | 470.015792 | 8.361125 | -461.654667 | -98.221097% |

Pair 3's 14.043-ms C1 row is a one-row COMMIT/timing outlier. The C1 median
remains 8.541 ms, exact CDC work remains 143,709 bytes, all counters and
identities are invariant, and there is no 200-430-ms hidden full scan.

CPU medians are 1.820 s for C0 and 1.390 s for C1; paired median change is
-23.626%, with C1 lower in 5/5 pairs.

Protected memory did not trigger §13.6's extension:

| Metric | C0 median | C1 median | Arm change | Paired median | >5% regression pairs |
|---|---:|---:|---:|---:|---:|
| RSS | 18,743,296 | 18,710,528 | -0.175% | -0.175% | 2/5 |
| Peak footprint | 12,697,984 | 12,714,368 | +0.129% | +0.129% | 2/5 |

Neither arm median regresses by more than 5%, so no 15-pair extension was
authorized or run.

## Exact Q and invariants

Every v4 row retains the v3 equation:

```text
38,959  base live
1,085,490 old authenticated CDC window
12,864  old RejoinChunk slots
1,085,490 exact edited scan input
---------
2,222,803 bytes exact Q high-water
```

`q_cdc_overlap_current == q_high_water == 2,222,803` for both arms and every
row ends at zero. Root, transition, ordered closure, 5,284-reference CDC
sequence, 143,709 CDC bytes, eleven canonical writes, 110,745 canonical bytes,
7,382 mapping bytes, one transaction, one COMMIT, SQL/BLOB counts, and endpoint
storage all remain identical to the accepted v3 semantics.

## Final five-lane checkpoint audit

| Lane | Verdict | Follow-up evidence |
|---|---|---|
| Authority/publication | PASS | unchanged v3 authority and real COMMIT proofs; no format or authority change |
| Exact CDC/COW/closure | PASS | frozen XOR identities unchanged; direct H=2 multi-ancestor and malformed-summary proof |
| Durability/provenance | PASS | unchanged v3 transaction/provenance path; 98-test suite green |
| Logical Q/resources | PASS | exact-capacity adoption enforced; excess-capacity rejection cleans Q; v4 Q remains 2,222,803 |
| Custody/performance | PASS | fresh release and 12 byte-identical arm copies; independent 446.457 -> 8.541 ms recomputation; memory extension not triggered |

No P0/P1 checkpoint blocker remains. Final decision: preserve v3 as the prior
accepted result, accept v4 as the release-path checkpoint follow-up, retain the
changed-spine algorithm, and allow F0 to begin only as a separate freeze/task.
