# Apple PoC operation profile

## Measurement boundary

| Item | Value |
|---|---:|
| Source baseline | `70d7cc4` plus operation-counter instrumentation |
| Host path | canonical `/private/tmp/...` APFS path |
| Runs | 3 sequential release runs |
| Largest file | 3,145,728 B |
| Second file | 1,048,576 B |
| Hard ceiling in evaluator | 104,857,600 B |
| Whole-workflow median | 3,361 ms |
| Whole-workflow range | 3,312–3,363 ms |
| Median maximum RSS | 18,071,552 B |
| Maximum RSS | 20,414,464 B |
| Operation Q high-water | 4,194,304 B |
| Terminal file descriptors | 4 |
| Terminal residue | 0 |

The workflow uses real APFS files, Bash, `mmap`, SQLite publication, reopen,
history, rollback and compaction. `/tmp` is a symlink on macOS and is rejected
by the adapter's component-by-component no-follow admission; use
`/private/tmp` or another canonical path.

## Median operation wall

| Operation | Native route | Median | Observed product work |
|---|---|---:|---|
| import capture | `CaptureStream` | 38.54 ms | read + CDC 4,194,592 B; verify 79,500 B |
| cold materialize | `MaterializeStream` | 99.85 ms | write 4,194,335 B |
| live warm exact-root refresh | `ExactNoop` | 35.83 ms | **still read + CDC 4,194,592 B** |
| same-size 4 KiB overwrite + publish | `ClonePatch` | 132.58 ms | native write 4,096 B; CDC 4,112 B; verify 83,689 B |
| 8 KiB insert + publish | `InPlaceShift` | 125.79 ms | native read 1,040,384 B; write 1,048,576 B; shift 1,040,384 B |
| 4 KiB delete + publish | `InPlaceShift` | 127.91 ms | native read/write 1,036,288 B; shift 1,036,288 B |
| tail truncate + publish | `InPlaceShift` | 136.31 ms | no suffix bytes; verify 91,934 B |
| rename + publish | `Rename` | 132.76 ms | no native payload bytes; verify 91,933 B |
| Bash execute | native process | 3.14 ms | script execution only |
| Bash + Python `mmap` mutation | native processes | 66.02 ms | external tools; no SDK counter authority |
| external capture | `CaptureStream` | 41.81 ms | read + CDC 4,194,683 B; verify 181,290 B |
| reopen | store open | 7.68 ms | open/admission wall only |
| historical 4 KiB native read | native file read | 39.58 us | **not a canonical LayerFS range measurement** |
| offline compaction | generation replacement | 307.47 ms | retained generation copy/verify/install |
| post-compaction reopen | store open | 10.66 ms | open/admission wall only |

## Amplification and bottlenecks

| Rank | Finding | Evidence | Required next optimization |
|---:|---|---|---|
| 1 | Each managed edit starts from a newly materialized 4 MiB workspace | 1 B–8 KiB edits all cost 119–146 ms and read about 4.24–4.28 MiB of engine objects | retain one live managed workspace across edits; do not rematerialize each revision |
| 2 | Warm exact-root refresh is logically a no-op but physically scans the complete workspace | 35.83 ms; 4,194,335 native B read; 4,194,592 CDC B | retained workspace authority plus root/digest binding; skip capture/CDC when authority is unchanged |
| 3 | Middle insert/delete remains suffix-linear in the native APFS file | 8 KiB insert moves 1,040,384 B; 4 KiB delete moves 1,036,288 B | changed-root refresh and VFS extent reads; native full-file projection remains an explicit fallback |
| 4 | Verified publication authenticates the complete retained root | 94–132 objects and 79,500–181,290 B per changed publication | preserve correctness; reduce duplicate reads or use authenticated retained-root authority only after a separately proved trust boundary |
| 5 | Publication has high fixed SQLite work | managed edit/rename rows issue 207–212 statements independent of 1 B–8 KiB edit size | profile statements; remove duplicate lookups/scratch work without changing one-COMMIT semantics |
| 6 | Full capture is linear in all live file bytes | import/external capture each scan about 4.19 MiB | changed-path capture or retained dirty-set authority; full external capture stays the correctness fallback |
| 7 | The evaluator does not yet measure canonical range read | the 39.58 us row reads an already materialized APFS file | add a thin SDK canonical `read_range(root,path,offset,len)` route before claiming LayerFS random-read latency |

## What these numbers prove

| Claim | Disposition |
|---|---|
| Product paths execute correctly on the compact Apple workflow | PASS |
| Native route and logical/native byte counters are emitted from product code | PASS |
| Same-size patch avoids native suffix movement | PASS |
| Count-changing native edit avoids suffix movement | FAIL — explicit `InPlaceShift` |
| Exact-root warm refresh avoids full scan | FAIL — complete read + CDC observed |
| Arbitrary-file random edit beats G5 | NOT MEASURED — no matched G5 control campaign |
| Canonical LayerFS range-read latency | NOT MEASURED — current row is native APFS read |

## Fast verification commands

```sh
cargo test -p layerfs-core --lib
cargo test -p layerfs-engine --lib
cargo test -p layerfs-vfs --lib
cargo test -p layerfs-sdk --test workflow
cargo clippy -p layerfs-engine -p layerfs-vfs -p layerfs-sdk -p layerfs-eval --all-targets -- -D warnings
cargo build -p layerfs-eval --release
target/release/layerfs-eval apple-poc /private/tmp/<fresh-run-directory>
```

