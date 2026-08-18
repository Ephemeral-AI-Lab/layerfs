# WP4-M M4.5 repair benchmark and terminal evidence

## Final-status pointer

This file intentionally remains the withdrawn v2 checkpoint. Final M4.5
acceptance uses the later, independently retained v3 report
`wp04-opt-milestone-4-5-v3-terminal-benchmark.md` and artifact root
`target/wp4m-m45-repair-k64-20260819-v3-terminal/`. Nothing below is reused
as terminal timing, Q, or custody evidence.

## Historical disposition — superseded v2 candidate evidence

- **FAIL / REVISE.** The PASS claim below is withdrawn by the 2026-08-19
  independent audit.
- This v2 campaign remains retained and hash-valid, but it is direction-only
  evidence: it was run before the controlling XOR experiment amendment and
  before the remaining BEGIN, exact-Q, COMMIT-boundary, and diagnostic repairs.
- None of its `443.143416 ms -> 9.000667 ms` timing, Q, or audit conclusions may
  support final acceptance. A separately retained v3 campaign is required.
- F0, qualification, promotion, profile selection, and production integration
  remain blocked/not started.

Everything below is preserved as the superseded v2 checkpoint.

- Date: 2026-08-18
- Disposition: **PASS for the private M4.5 same-count changed-spine milestone**.
- Scope: retained 100-MiB K64/F64 same-middle edit in the private benchmark
  `Store`; not production `Engine` integration.
- Qualification: `false`.
- Promotion: `false`.
- Profile selection/rejection: `false`.
- Full-create, 512-MiB, `+1`, directory, F0, and later Phase 4 work: not run.

## 1. Hypothesis and one changed variable

The repaired causal comparison is:

```text
C0 = exact same-open authority + exact edited-stream CDC/COW
     + complete pre-COMMIT closure qualification

C1 = byte-identical C0 substrate
     + transaction-witnessed changed-spine qualification
```

The executable, pair base, authority sidecar, expectations, edit bytes, local
CDC/rejoin, newly written objects, root, transition, COMMIT, fresh reopen,
full scrub, reconstruction, ranges, counters, and reporting are identical.
Only the pre-COMMIT qualification algorithm changes.

The predeclared gate is at least 5% median durable-edit improvement and at
least four of five adjacent paired wins, with correctness, authority,
atomicity, exact Q, CPU, memory, and storage protected.

## 2. Superseded invalid operation and evidence

The independent audit was correct: the former operation replaced an old chunk
with uniform `0x5a` bytes but chose boundaries from the old source. Exact
FastCDC over that actual edited stream produces **5,283**, not 5,284,
references. It is count-changing and cannot enter the same-count C1 path.

The repaired operation applies the fixed bytewise transform
`old_byte XOR 0x5a` to the same 18,854-byte middle chunk at offset 52,480,416.
An independent full edited-stream scan produces exactly 5,284 references. The
local mutation starts at the authenticated predecessor, stops after two exact
suffix confirmations, and stores only the five changed pre-rejoin chunks.

Consequently, all prior uniform-`0x5a` identities and the old `431.490 ->
2.437 ms` comparison remain preserved only as mechanism-direction evidence.
They are not combined with or reused by this result. In particular:

- old expectation SHA-256: `81b2eaf5...` — superseded;
- old edited CDC fingerprint: `58b61bbd...` — superseded;
- old root: `cc8f31ad...` — superseded;
- old transition: `2686d6ff...` — superseded; and
- old raw JSONL SHA-256: `f6f1e698...` — preserved under the original artifact
  directory and inadmissible for M4.5 acceptance.

## 3. Complete measured custody tuple

| Item | Value |
|---|---|
| Repository | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` |
| Branch | `codex/empty-worktree` |
| HEAD | `f3df30a80172131b74b5949a6a55234c962dac67` |
| Measured implementation diff SHA-256 | `0c8d70bc6aa5944f40ead21ffefb335457df251f7df8351bef02c04acda0ac1e` |
| Measured benchmark source SHA-256 | `07df5f2b6124af8be4e8ad0f0213875108c0809d38e436a7c020ab83125188dc` |
| Release executable SHA-256 | `37643a4eb99a0ab8fcbeaa326ebb2ceada98a9716c9dbe677c6f4a53e7320d02` |
| Fixture SHA-256 | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| Fixture manifest SHA-256 | `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca` |
| Expectation SHA-256, every pair | `70520375af87d5227e28775a59879067d3b942cd82eb3f2fd2e15bb942b169ff` |
| Raw 12-row JSONL SHA-256 | `be708e3ccd4a5b5ed16f53e816543a7c88ab303c370c1078c698b5ef2903a8a6` |
| Preflight TSV SHA-256 | `e88bc7f241615f31b400a3ebc841e97eb0b6b9de81de0b2750bf8282d70e9912` |
| Structural summary SHA-256 | `22da16ef5eb6a5dd3e7103a10a647494161bbde3616027748e9511674c8bd887` |
| Commands SHA-256 | `548170dc0c422cab92e2e28625743bbb657aafd5be85100bdad98aa4c6eb163d` |
| macOS resource observations SHA-256 | `0e1949b85e08c8842997c678fd9ae05f6d5ed14e9b1df8956bc320954e4d75b8` |
| Summary generator SHA-256 | `3cd846832dc417ba3106f703604f9b45bf8bd170ce1af33d5ad3eac16501d840` |
| Campaign script SHA-256 | `2d320776be636cdc97f0a6ba8e95c5a2ab7bea6f505e3570d1e32a2d2e66740e` |
| Final hash manifest SHA-256 | `7528d52fb8cfa089dda4c19c9feee7d4076e26532108f94ef4b177b52c6754b0` |

Each warmup/measured pair was prepared once. The database, 32-byte authority
sidecar, and expectation file were copied with `/bin/dd`, compared byte for
byte, hashed, and then supplied separately to C0 and C1. All 12 arm copies
matched their pair base:

```text
database hashes equal:    12/12
authority hashes equal:   12/12
expectation hashes equal: 12/12
```

The six pair-specific base database and authority hashes are retained in
`wp4m-m45-repair.preflight.tsv`; bases themselves remain under `bases/`.
No clone/reflink API was used or claimed.

The first script invocation failed before any timed row because it referenced
`/usr/bin/unlink` instead of the host's `/bin/unlink`. Its empty raw file,
preflight header, command log, and explanation are preserved under
`failed-preflight-1/`. No measured row was discarded or rerun.

## 4. Exact operation identities

All 12 warmup/measured rows agree on:

| Identity | Exact value |
|---|---|
| Base source BLAKE3 | `bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7` |
| Edited source BLAKE3 | `527b215f91735e023b23a2e970f86c9e25ea303d38a1e4006f3f3a2a98f9db49` |
| Edited reference count | `5,284` |
| Edited CDC-sequence BLAKE3 | `e6d6d858ab6ff9804839630df90a2e621ae06291e55ab12aea9957c566ec83f7` |
| Before file | `a94d42f6357b621ea51e306fe0a242854ed95d02d3e3dc7a88e3c2a20c194786` |
| After file | `ab1f98a2c44c60f1b88f8aaec368ab2bbd68de9580e6e79b4dbf859800f2e7c8` |
| Result root | `d1a69475b0f8e25e44d7bd625a679b596ea2a8b3347ef8c15fafa13f654b299b` |
| Transition | `f11cc9d84deae7f1871adca62cc562ab63dbb01e9c39771ed3522eab4007cee1` |
| Ordered closure | `c0f6a39bf9939c89301bedb564516c5ec851321a1d89c69b2e95d4b1844a9587` |
| Edit | offset `52,480,416`; remove/insert `18,854` bytes; inserted byte `removed XOR 0x5a` |

The expectation manifest binds the exact operation, removed/inserted bytes,
edited source fingerprint, ordered FastCDC sequence, before/after file IDs,
root, transition, closure, and range outputs. The measured child cannot learn
or rewrite these expected values.

## 5. Commands and correctness gates

Controlling commands:

```text
cargo test --workspace --offline --all-targets
  -> PASS: 92 tests; 0 failed
     layerfs-core 44
     layerfs-engine lib 4
     private benchmark 27
     engine parity 12
     layerfs-eval 5

cargo clippy --workspace --offline --all-targets -- -D warnings
  -> PASS

cargo fmt --all -- --check
  -> PASS

git diff --check
  -> PASS

cargo build --release -p layerfs-engine \
  --bin phase4_create_edit_benchmark --offline
  -> PASS; measured executable 37643a4e...20d02

target/debug/phase4_create_edit_benchmark --self-test <temporary-dir>
  -> PASS; root=f1cfdd7fdd658506caf39ace169dd83f1a95fbb25aafcbb7f9b72864f9d2e42a;
     objects=20; auth_bytes=1,054,925
```

Focused direct regressions prove:

- exact edited-stream FastCDC and inequality with old callback substitution;
- local early rejoin plus same-count final reference equality;
- same-count cumulative-length redistribution;
- complete singleton namespace closure before witness issuance;
- same-open/open/transaction/tuple/single-use/mutation/rollback invalidation;
- exact complete prior-head/receipt comparison and genesis/ABA rejection;
- centralized pre-COMMIT rollback with exact `MissingObject(ObjectId)` first
  cause and separately preserved cleanup cause;
- actual SQLite COMMIT errors reconciled through requested-visible,
  prior-visible, different-head, and ambiguous outcomes on a fresh connection;
- committed publication remains explicit if later verification fails;
- 128-KiB expectations cap, 1-GiB pre-admission rejection, real-path summed
  capacity overlap, and terminal Q zero; and
- Memory/SQLite identity and range parity.

The three non-prior COMMIT outcomes use test-only post-error authority
injection after an actual SQLite commit-hook rejection; the production
reconciliation function and fresh read-only connection are unchanged and are
the exact path under test.

## 6. Timing result

One warmup and five adjacent balanced pairs were run. Odd pairs were AB and
even pairs BA. Values are median / min / max / spread in milliseconds.

| Phase | C0 full closure | C1 changed spine | Median change |
|---|---:|---:|---:|
| Same-open authority establishment | 235.888 / 231.701 / 244.951 / 13.250 | 233.667 / 231.467 / 246.493 / 15.027 | -0.941% |
| Exact CDC + mapping/COW | 6.191 / 6.119 / 6.583 / 0.464 | 6.211 / 6.083 / 6.678 / 0.595 | +0.315% |
| Pre-COMMIT qualification | 434.819 / 424.146 / 444.415 / 20.269 | 0.297 / 0.273 / 0.345 / 0.072 | -99.932% |
| SQLite COMMIT | 1.951 / 1.636 / 2.644 / 1.008 | 2.066 / 1.943 / 2.824 / 0.881 | +5.893% |
| **Durable edit** | **443.143 / 431.945 / 453.642 / 21.697** | **9.001 / 8.314 / 9.494 / 1.179** | **-97.969%** |
| Fresh reopen/head | 0.918 / 0.780 / 0.996 / 0.216 | 0.842 / 0.803 / 1.658 / 0.855 | -8.205% |
| Fresh full scrub | 274.189 / 267.600 / 285.429 / 17.829 | 263.791 / 262.776 / 277.943 / 15.166 | -3.792% |
| Reconstruction | 422.179 / 416.330 / 440.258 / 23.928 | 438.246 / 416.837 / 441.210 / 24.372 | +3.806% |
| Ranges | 0.715 / 0.641 / 0.747 / 0.106 | 0.696 / 0.672 / 0.726 / 0.053 | -2.644% |
| Post-COMMIT verification total | 698.166 / 691.292 / 727.429 / 36.136 | 702.497 / 681.967 / 720.677 / 38.710 | +0.620% |
| Same-open complete lifecycle | 1,134.436 / 1,125.236 / 1,181.071 / 55.835 | 710.947 / 690.281 / 730.170 / 39.889 | -37.329% |
| First-open edit lifecycle | 1,379.387 / 1,358.423 / 1,423.783 / 65.361 | 943.741 / 921.748 / 976.664 / 54.916 | -31.581% |

The five paired durable-edit results are:

| Pair/order | C0 ms | C1 ms | Delta ms | Delta | C1 win |
|---|---:|---:|---:|---:|---:|
| 1 / AB | 434.156 | 9.332 | -424.825 | -97.851% | yes |
| 2 / BA | 431.945 | 8.314 | -423.631 | -98.075% | yes |
| 3 / AB | 447.069 | 9.001 | -438.068 | -97.987% | yes |
| 4 / BA | 453.642 | 9.494 | -444.149 | -97.907% | yes |
| 5 / AB | 443.143 | 8.449 | -434.694 | -98.093% | yes |

Result: **performance PASS**, with a 97.969% arm-median reduction and 5/5
wins. This is durable edit latency. No 100-MiB/edit throughput is calculated.

## 7. Timer equations and lifecycle separation

The summary generator checks every warmup/measured row:

```text
durable_edit
  = exact_CDC_and_mapping_COW
  + precommit_qualification
  + SQLite_COMMIT

postcommit_verification
  = fresh_reopen_head
  + fresh_full_scrub
  + reconstruction
  + range_verification

same_open_complete_lifecycle
  = durable_edit + postcommit_verification

first_open_edit_lifecycle
  = same_open_authority_establishment
  + same_open_complete_lifecycle
```

All 12 durable and same-open equations match exactly. The first-open value is
derived row by row from retained disjoint raw fields and is included in the
hashed structural summary. Authority establishment and fresh postpublication
verification remain linear and are never hidden inside the 9.001-ms local
edit metric.

## 8. Work/counter causality

Measured deterministic arm values:

| Counter | C0 | C1 | Meaning |
|---|---:|---:|---|
| Source replacement bytes read | 18,854 | 18,854 | invariant |
| Exact local CDC bytes inspected | 143,709 | 143,709 | invariant; below 1-MiB ceiling |
| New canonical bytes | 110,745 | 110,745 | invariant |
| Mapping bytes rewritten | 7,382 | 7,382 | invariant |
| Statement-cache acquisitions | 16,334 | 10,976 | C1 -5,358 |
| SQL query calls | 16,418 | 11,060 | C1 -5,358 |
| SQL execute calls | 18 | 18 | invariant |
| Rows returned | 21,619 | 16,261 | C1 -5,358 |
| Rows changed | 12 | 12 | invariant |
| Row-BLOB reads | 21,647 | 16,289 | C1 -5,358 |
| Row-BLOB writes | 26 | 26 | invariant |
| Borrowed row-BLOB reads | 21,201 | 15,922 | bounded path difference |
| Transactions / COMMITs | 1 / 1 | 1 / 1 | invariant |
| Covered equal edges | 0 | 123 | C1 authority optimization |
| New/different edges | 0 | 8 | C1 complete changed proof |
| Fully authenticated new objects/bytes | 0 / 0 | 5 / 103,363 | C1 changed chunks |

The exact changed-edge equation is one namespace edge, one root edge, one
branch edge, and five chunk edges. The covered equation is one root sibling,
63 branch siblings, and 59 unchanged leaf references.

W and D are **Unavailable**. The row does not invent substitute meanings;
new-write, authenticated-nonnew, rewrite, CDC, SQL, BLOB, and output metrics
retain their precise labels.

## 9. CPU, Q, RSS/peak, SQL/BLOB, and storage

| Resource | C0 | C1 | Classification |
|---|---:|---:|---|
| CPU median | 1.810 s | 1.370 s | PASS; paired median -24.157%, 5/5 lower |
| Instructions median | 11,218,753,632 | 7,748,532,155 | Observed; about -30.9% |
| Cycles median | 3,592,004,887 | 2,455,229,498 | Observed; about -31.6% |
| Exact logical Q | 2,278,037 bytes | 2,278,037 bytes | PASS; every row ends at zero |
| RSS median | 18,579,456 bytes | 18,907,136 bytes | +1.764%; below extension trigger |
| Paired RSS median | — | -147,456 bytes / -0.776% | mixed; 3/5 lower, 2/5 >5% regressions |
| Peak-footprint median | 12,501,376 bytes | 12,861,824 bytes | +2.883%; below extension trigger |
| Paired peak median | — | -81,920 bytes / -0.634% | mixed; 3/5 lower, 2/5 >5% regressions |

Neither arm-median RSS nor peak footprint exceeded the 5% trigger, so the
predeclared 15-pair extension was not run. The original five remain the
official resource evidence.

Endpoint storage is identical in every pair:

| Endpoint/measure | Before | After | Status |
|---|---:|---:|---|
| Main DB apparent | 109,268,992 | 109,383,680 bytes | exact |
| Rollback journal apparent | 0 endpoint bytes | 0 endpoint bytes | Observed endpoint only |
| Authority sidecar apparent | 32 | 32 bytes | exact |
| Main DB allocated | 109,268,992 | 126,046,208 bytes | exact |
| Authority allocated | 4,096 | 4,096 bytes | exact |
| Total allocated store | 109,273,088 | 126,050,304 bytes | exact; +16,777,216 |

Peak journal/temp allocation, sync/fsync counts, SQLite page-cache bytes, and
byte-level physical I/O are **Unavailable**. macOS process block-input/output
operation counters were observed as zero; they are not relabeled byte counts.
Native SQLite prepares remain **Unavailable**; statement-cache acquisitions
are reported separately.

## 10. Planning-estimate deviation

C1's 9.001-ms median is above the most-likely 3–5 ms planning estimate but
inside the conservative 8–10 ms envelope. The initial fixed implementation
measured 16.212 ms and exposed the cause: it scanned/stored the entire 1-MiB
maximum window before checking rejoin. Early exact termination reduced CDC
inspection to 143,709 bytes and submissions to the five truly changed chunks.

The residual difference from the old 2.437-ms synthetic result is a healthy
correctness cost:

- actual edited FastCDC changes five chunks rather than swapping one prepared
  reference;
- exact local CDC/hash work is 143,709 bytes;
- eleven objects are created rather than seven; and
- COMMIT median is about 2.066 ms.

There is no hidden 100-MiB scan or full pre-COMMIT closure regression: C1
qualification is 0.297 ms, local mapping/COW is 6.211 ms, and total C1 is
9.001 ms. The deviation is classified as bounded exact-CDC/new-object work,
not an algorithm failure.

## 11. Final decision and non-claims

Decision: **retain the repaired private M4.5 implementation and mark M4.5
PASS**. The authority, correctness, durability, exact-Q, custody, speed, CPU,
external-memory, and endpoint-storage gates pass for the specified same-count
row.

The accepted complexity is:

```text
mutation:
  O(changed CDC bytes + changed references + K + F*H)

pre-COMMIT qualification:
  O(K + F*H + changed/new authenticated closure + H^2)

resident LayerFS memory:
  O(H + K + F + bounded chunk/page/SQL/output buffers)
```

Same-open witness establishment and fresh scrub remain linear in complete
reachable authenticated closure. Reconstruction remains linear in source
bytes/output. Fixed-ordinal count-changing edits remain suffix-linear and were
not run.

This PASS does not claim:

- production `Engine` integration;
- final compatibility profile selection;
- promotion or profile rejection;
- full-create improvement or the 200/300-MiB/s target;
- 512-MiB or 100-GiB measured performance;
- a logarithmic `+1`/count-changing edit; or
- Phase 4 completion.

The terminal five-lane read-only re-audit is PASS. F0 may begin only as a
separate next work item; it is not included in this repair.

## 12. Reproducible artifact paths

All repaired evidence is retained under:

`target/wp4m-m45-repair-k64-20260818-v2/`

Key files:

- `run_repair_campaign.sh` — physical-copy campaign generator;
- `summarize_repair_campaign.py` — standard-library structural summary;
- `wp4m-m45-repair.raw.jsonl` — 2 warmup + 10 measured verbatim rows;
- `wp4m-m45-repair.preflight.tsv` — pair-base and arm-copy hashes/sizes;
- `wp4m-m45-repair.summary.json` — phase/resource/counter/equation summary;
- `wp4m-m45-repair.commands.txt` — retained commands;
- `wp4m-m45-repair.macos-time.txt` — external observations;
- `wp4m-m45-repair.final-hashes.txt` — final key-artifact hashes;
- `source/measured-implementation.diff` — reconstructible measured source;
- `binaries/phase4_create_edit_benchmark` — measured release executable;
- `bases/pair-{0..5}.sqlite{,.authority,.expectations}` — immutable bases; and
- `failed-preflight-1/` — retained no-row orchestration failure.
