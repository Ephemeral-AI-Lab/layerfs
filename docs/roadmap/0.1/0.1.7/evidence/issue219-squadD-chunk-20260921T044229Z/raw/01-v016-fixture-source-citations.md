# raw/01 — v0.1.6 fixture source citations

Worktree HEAD `9c46930b846600e5f3c6ca4a4c4cbcf44ecdc356`. Line numbers are from that tree.

## Profile identity

```
benchmark/fs-bench-pro/families/init_namespace/mod.rs:7
pub(crate) const NAMESPACE_FIXTURE_PROFILE: &str = "synthetic-small-heavy-v2";

benchmark/fs-bench-pro/families/init_namespace/mod.rs:72-75
    id: "namespace-10000",
    alias: "namespace-10000-files-300mb",
    ...
    fixture_profile: NAMESPACE_FIXTURE_PROFILE,
```

Self-check asserting the profile name and the other evidence identities:

```
benchmark/fs-bench-pro/workload/main.rs:1862-1867
if NAMESPACE_FIXTURE_PROFILE != "synthetic-small-heavy-v2"
    || NAMESPACE_DIGEST_PROFILE != "namespace-file-digest-tree-v2"
    ...
```

## Declared counts (the plan oracle)

```
benchmark/fs-bench-pro/workload/main.rs:1816-1825
let expected_counts = [
    ("namespace-100-compact-v3", [1, 78, 15, 5, 1], 5_000_000),
    ("namespace-1000-compact-v3", [10, 789, 150, 50, 1], 20_000_000),
    ("namespace-10000", [100, 7_899, 1_500, 500, 1], 300_000_000),
    (
        "namespace-100000",
        [1_000, 78_998, 15_000, 5_000, 2],
        500_000_000,
    ),
];
```

The array order is fixed at `workload/main.rs:1828-1834`:

```
first.empty_files, first.tiny_files, first.small_files, first.medium_files, first.anchor_files
```

so `namespace-10000` declares **100 empty / 7,899 tiny / 1,500 small / 500 medium /
1 anchor**, `logical_bytes = 300_000_000`.

## Band percentages

```
benchmark/fs-bench-pro/workload/main.rs:194-200
let classes = [
    NamespaceClass::Empty,
    NamespaceClass::Tiny,
    NamespaceClass::Small,
    NamespaceClass::Medium,
];
let percentages = [1_u64, 79, 15, 5];
```

These are applied to `non_anchor = regular_files - anchor_files = 9,999`
(`workload/main.rs:190-193`) and reconciled by Hamilton largest-remainder
(`workload/main.rs:204-230`) before the oracle check at 231-240 passes.

## THE BANDS ARE WEIGHTS, NOT SIZES

```
benchmark/fs-bench-pro/workload/main.rs:424-430
fn namespace_relative_weight(class: NamespaceClass, role: u64, count: u64) -> Result<u64> {
    let (lower, upper) = match class {
        NamespaceClass::Empty | NamespaceClass::Anchor => return Ok(0),
        NamespaceClass::Tiny => (1_u64, 8_u64),
        NamespaceClass::Small => (32, 256),
        NamespaceClass::Medium => (1_024, 8_192),
    };
```

```
benchmark/fs-bench-pro/workload/main.rs:434-448
    let width = upper - lower + 1;
    let numerator = (role * 2 + 1) * width;
    let denominator = count * 2;
    lower + (numerator / denominator)
```

Weight is a monotone staircase across a band's roles, so it is a *shape*, not a size cap.

## Size is a separate largest-remainder pass

```
benchmark/fs-bench-pro/workload/main.rs:324-328   (initial placeholder)
    size: match role.class {
        NamespaceClass::Empty => 0,
        NamespaceClass::Anchor => scenario.anchor_bytes,
        _ => 1,
    },

benchmark/fs-bench-pro/workload/main.rs:332-344
let positive = counts[1] + counts[2] + counts[3];              // 7,899 + 1,500 + 500 = 9,899
let anchor_bytes = scenario.anchor_files * scenario.anchor_bytes;  // 1 * 100,000,000
let distributable = scenario.logical_bytes - anchor_bytes - positive;
                                                              // 300,000,000 - 100,000,000 - 9,899
                                                              // = 199,990,101

benchmark/fs-bench-pro/workload/main.rs:345-348
let weight_sum = files.iter().try_fold(0_u64, |total, file| total.checked_add(file.relative_weight))?;
                                                              // = 2,555,546

benchmark/fs-bench-pro/workload/main.rs:358-363
let product = distributable * file.relative_weight;
let floor = product / weight_sum;
file.size = 1 + floor;

benchmark/fs-bench-pro/workload/main.rs:367-386
let extra = distributable - floor_sum;                        // = 4,214
... sort by remainder desc, then path asc ...
for (index, _) in size_remainders.into_iter().take(extra) { files[index].size += 1; }
```

**Conclusion:** the `(1,8)` / `(32,256)` / `(1_024,8_192)` triples are weights.
The realized byte ranges are 79-627 / 2,505-20,035 / 80,684-640,537. See
`raw/03-v017-ladder-output.txt`.

## Independent corroboration in the repository

`core/benchmark/fs-bench-pro-storage-content/src/ops/namespace_content.rs:25-28`

```
//! The declared ranges are **weight bands, not size caps**: v0.1.6's own declared
//! weights sum to 102,555,546 bytes and the plan is then scaled up to the declared
//! logical total, so a "tiny" file there is larger than 8 bytes. Sizes here are
//! therefore not confined to the bands either.
```

The qualitative claim ("a tiny file there is larger than 8 bytes") is correct and is
confirmed above. The numeric claim (`102,555,546`) is **not** what the cited formula
produces; the formula yields `2,555,546`. Reported as a documentation defect in
`raw/03`.

## Receipt cross-check (what the run actually reported)

From `benchmark-results/issue219/s1-ns10000-candidate-20260921T031259Z/perf.jsonl`,
line 2, `records[0]`:

```
"fixture_profile":   "synthetic-small-heavy-v2"
"regular_files":     10000
"empty_files":       100
"tiny_files":        7899
"small_files":       1500
"medium_files":      500
"anchor_files":      1
"anchor_bytes":      100000000
"logical_bytes":     300000000
"data_directories":  100
"scanned_files":     10000
"scanned_bytes":     300000000
"store_canonical_objects": 25158
```

These match the source declaration exactly. The receipt does **not** publish per-band
size ranges, which is why the weights-vs-sizes question could only be settled from
source — and it is settled in `raw/02`/`raw/03`.
