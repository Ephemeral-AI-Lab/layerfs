# Stage 1.1 Apple/APFS terminal receipt

Date: 2026-08-25  
Disposition: **PASS**  
Campaign: `target/layerfs-stage1-apple-edge-20260825-attempt-014`

## Custody

| Item | Identity |
|---|---|
| Product/evaluator commit | `f3dd4a32273a4c5cbe5e7ca2287c945ba4434c30` |
| Dirty tree at measurement | `false` |
| Source BLAKE3 | `e3ac90dfb3e1a3c48814d972c1b49f7f89cfb841ea47fe847a8f7536bb5253c5` |
| Source manifest SHA-256 | `cacbe0497c05014a1966f152cbfed64f3ae6d4ce3e0656e459f1c1eb3a9ded84` |
| Release executable SHA-256 | `2a7c71cf51b09d4411c1c2cb4c0b33ca1ebc435232c577ddeba4d126aba44c31` |
| Fixture BLAKE3 | `1c21c6d2eec547bc795e3b734ed28f66a4fde4ef18aaaf40737c5c58a3343736` |

## Exact-source closure

```text
cargo test --workspace -- --test-threads=1                         PASS
cargo clippy --workspace --all-targets -- -D warnings             PASS
all_51_exact_edits_match_the_independent_vec_digest_after_every_operation
                                                                  PASS / 13.74 s
cargo build --release -p layerfs-eval                             PASS
stage1 prepare apple-edge                                         PASS / fixture reused
stage1 readiness apple-edge                                       PASS / zero measured rows
```

Readiness measured one reset at `6,045,833 ns` and forecast the campaign at
`45,006,045,833 ns`, below the 60-second hard gate.

## Campaign result

| Gate | Observed | Result |
|---|---:|---|
| Rows | `47/47` | PASS |
| Edit/sub-edit operations | `51/51` | PASS |
| Durable transitions | `34/34` | PASS |
| Physical oracles | `51/51` | PASS |
| Canonical transition oracles | `34/34` | PASS |
| Complete campaign wall | `13.517581334 s` | PASS |
| Publication transactions/COMMITs/rollbacks | `34/34/0` | PASS |
| CDC bytes/payload bytes | `495,616/495,616` | PASS |
| Unaffected canonical payload reads/writes | `0/0` | PASS |
| Patch/shift/FullFallback | `3/12/0` | PASS |
| Workspace materializations/reuses/rematerializations | `1/34/0` | PASS |
| RSS peak | `29,982,720 B` | PASS |
| Largest product buffer | `1,048,576 B` | PASS |
| Q structural reservation high/terminal | `8,388,608/0 B` | PASS |
| FD baseline/terminal | `5/5` | PASS |
| Terminal connections/processes/temp/residue/network | `0/0/0/0/0` | PASS |

Selected performance:

| Operation | p50 | p95 |
|---|---:|---:|
| Physical edit | `1.890 ms` | `11.354 ms` |
| Durable checkpoint | `7.004 ms` | `9.252 ms` |
| Physical edit + checkpoint | `9.011 ms` | `14.657 ms` |
| Direct logical edit | `1.911 ms` | `2.647 ms` |
| Changed-root refresh | `16.499 ms` | `36.756 ms` |
| Logical edit + refresh | `18.316 ms` | `38.585 ms` |
| Append/truncate refresh | `9.127 ms` | `9.573 ms` |

The initial 24 MiB cold materialization measured `208.242 MiB/s`; R15, R30,
and R34 measured `322.598`, `336.958`, and `328.759 MiB/s` respectively.

## Raw artifact hashes

```text
environment.json   7fbeddb4cdb39bb32c0646ce68a7d88d0735c81260b0bc5a209164a548fc461f
master.json        5ec0dc7f6432bfe579bba12c75ae0c6381ebaace06ceef69504a6b11014e9424
readiness.json     e2882f698e26f6aaab16dee2be05b0cd5d9f7781ec7f2e87d4ab8f6221f4cb5b
schedule.json      9c570bbe9005fbaa8ae8e1ca82cf1211909b91c635fe2ff83557d19aaf71a2dc
rows.jsonl         7231c0a8d7dffb561adcc5aff23f77a5ffbdb645e473b62f023b09c62873fa37
campaign-time.txt  b0e5ebe8205580ee27c41d7b567699097c3785b5b25c57147919ec1873389f9e
summary.json       b525ef65dc773e17f0909a6a8e6ddf2b0a49a56aa3ac0e40e905e292463e7fa6
summary.md         a61b4842b52b3e7cd3df00aa759a3ba390e59b5581325a9a4bb27f02d4685881
```

Three independent read-only audits recomputed the population, schedule,
RefState chain, native byte equations, transaction/authentication closure,
locality, timer equation, resource terminal state, and custody directly from
the raw artifacts and current source. All three returned PASS with no material
P0, P1, or P2 defect.

Scope note: this campaign's supported-xattr population is empty. Exact
`com.apple.provenance` filtering and xattr-free equality are measured here;
nonempty supported-xattr roundtrip and retained unsupported refusals remain
covered by the exact-source focused Apple product tests.
