# raw/09 — timer boundaries, both arms

## A. v0.1.6 — `layerstack_init_ns`

```
benchmark/fs-bench-pro/src/main.rs:2149
    let init_started = Instant::now();
benchmark/fs-bench-pro/src/main.rs:2150-2153
    let initialized = client.initialize_layerstack(
        EntityName::new(format!("{}-{source}-{seed}", scenario.id))?,
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
benchmark/fs-bench-pro/src/main.rs:2154
    let layerstack_init_ns = elapsed_ns(init_started);
```

The timer wraps **exactly one call**, `initialize_layerstack`, and nothing else.
The scan receipt is taken *after* the timer (`main.rs:2155-2156`) and validated at
`main.rs:2159-2167` against `fixture_manifest.regular_files` / `logical_bytes`.

Receipt cross-check: `record[0].scanned_files = 10000`,
`record[0].scanned_bytes = 300000000` — i.e. the receipt's own validation confirms the
scan ran to completion *inside* the timed call.

### What is inside, per file

```
crates/layerfs-layerstack-store/src/layerstack.rs:2732-2735
    let mut input = CountedSourceReader {
        file: std::fs::File::open(entry.path())?,
        calls: 0,
        bytes: 0,
    };
crates/layerfs-layerstack-store/src/layerstack.rs:2737-2744
    let candidate = filesystem::write_file(
        objects, *root, &path, &mut input, metadata.permissions().mode(), seed,
    )?;
crates/layerfs-layerstack-store/src/layerstack.rs:2748-2750
    *scanned_bytes = scanned_bytes
        .checked_add(input.bytes)
        .ok_or(StoreError::Integrity("Layer initialization scan counter"))?;
crates/layerfs-layerstack-store/src/layerstack.rs:2751-2753
    *cdc_bytes_scanned = cdc_bytes_scanned
        .checked_add(candidate.counters().rope.cdc_bytes_scanned)
        ...
```

The walker opens and fully reads each source file, then calls `filesystem::write_file`,
which is where representation selection (whole-file vs FastCDC chunking) and object
construction happen. `cdc_bytes_scanned` is accumulated separately, confirming the CDC
scan is part of this path.

### INCLUDED in `layerstack_init_ns`

| Work | Where |
|---|---|
| recursive directory walk over the prepared fixture | `layerstack.rs:2732` region |
| **full source content read of all 300,000,000 bytes** | `layerstack.rs:2732-2735`, counted at 2748-2750 |
| whole-file vs chunked representation decision | `filesystem::write_file`, `layerstack.rs:2738` |
| FastCDC scan of every file over the cutoff | same; `cdc_bytes_scanned` at `layerstack.rs:2751-2753` |
| object construction / envelope encode / identity | `filesystem::write_file` |
| accept path + commit | same `initialize_layerstack` call |

### EXCLUDED

| Work | Where |
|---|---|
| Store create, client binding, container binding, fixture materialization | `setup_ns` = `elapsed_ns(setup_started)` at `benchmark/fs-bench-pro/src/main.rs:2969`, published by the `init-only-diagnostic` schema at `main.rs:3043`; named by the parity spec at `namespace-10000-parity-spec.md:42` |
| teardown | `teardown_ns` (`namespace-10000-parity-spec.md:42`) |

The repo states the same boundary in prose at
`core/docs/benchmark/fs-bench-pro-storage-content/namespace-10000-parity-spec.md:41`:

> `| timed phase | scan the fixture (10,000 files / 300,000,000 bytes), construct content objects, admit them to the Store, commit |`

## B. v0.1.7 — `measure("pipeline", …)`

```
core/benchmark/fs-bench-pro-storage-content/src/ops/pipeline.rs:786-847
    let (measured, report) = super::measure("pipeline", |timing: &TimingScope<'_, Active>| {
        let store = Store::open(&sample, timing.child("store.open"))?;          // 787
        let mut operation = store.begin_save(timing.child("storage.begin"))?;   // 788
        ...
            build_filesystem(&mut objects, &input, ordering)?  /  update_filesystem(...)   // 811/813
        ...
        for id in content.insertion_order() {                                   // 834
            let object = content.cloned_object(*id)...;                         // 835-837
            content_bytes = content_bytes.saturating_add(object.canonical_len() as u64);  // 841
            operation.accept(object)?;                                          // 842
        }
        let outcome = operation.finish(timing.child("storage.finish"))?;        // 844
        accept_span_ns = accept_started.elapsed().as_nanos() as u64;            // 845
```

### The C2 supplied-object rule

`core/benchmark/fs-bench-pro-storage-content/src/ops/c2.rs:13-16`, verbatim:

```
//! C2 never runs C1 file construction inside a measured phase. Every canonical
//! object a C2 row saves is supplied by the harness: built before the timer, and
//! offered to the Store as the finalized objects they are, so no envelope is
//! re-guessed on the way in.
```

### EXCLUDED (all constructed before the timer)

| Work | Source |
|---|---|
| tree recipe `Recipe::prepare()` | `pipeline.rs:598-604` |
| the byte plan `namespace_content::plan(...)` | `pipeline.rs:611-619` |
| **content construction for all 10,000 files / 300 MB**, i.e. the chunking decision, the FastCDC scan, envelope encode and object identity for every object | `pipeline.rs:621-664`, wrapped in `Timing::disabled("setup.construct", …)` |
| base store creation `c2::create_and_save_untimed` | `pipeline.rs:678` |
| sample preparation / de-warm `c2::prepare_sample` | `pipeline.rs:679` |
| batch split `prepared.batches(fs::WALK_CEILING)` | `pipeline.rs:691-694` |
| `fs::batch_backings` x3 (plan / oracle / perf) | `pipeline.rs:707-718` |
| reader chain and prefix snapshots | `pipeline.rs:719-767` |
| the oracle replay | `pipeline.rs:860-876` |

`pipeline.rs:621-622` states the intent verbatim:

> `// Construct the content. Untimed: C2's rule is that every canonical object a`
> `// C2 row saves is supplied by the harness, and construction is C1's half.`

`pipeline.rs:834-843` is the **only** place content enters the timed region, and it
re-offers already-finalized objects via `operation.accept(object)`.

### INCLUDED in the pipeline timer

`Store::open`, `begin_save`, `build_filesystem`/`update_filesystem` per batch,
`operation.accept` of the pre-finalized content objects, and `operation.finish`.

The row's own `notes` field (receipt `ns17-final-20260921T031259Z`) states it:

```
"measured_region: Store::open + build_filesystem + content accept + save + acknowledgement"
"content: constructed before the timer, accepted inside it (C2's supplied-object rule)"
```

## C. The asymmetry, stated plainly

| Work | v0.1.6 timer | v0.1.7 timer |
|---|---|---|
| scan the 10,000-file tree | **inside** | outside (recipe + plan are untimed) |
| read 300,000,000 source bytes | **inside** | not performed at all |
| whole-file / chunked decision | **inside** | outside (`setup.construct`) |
| FastCDC scan of every file >= 131,072 B | **inside** | outside (`setup.construct`) |
| envelope encode + object identity | **inside** | outside (`setup.construct`) |
| accept of finalized objects | **inside** | **inside** |
| commit / publication | **inside** | **inside** |

**The v0.1.6 timer pays for the scan and the construction; the v0.1.7 timer pays for
neither.** Per §1's verdict the file-size populations are identical, so the remaining
asymmetry is the timer boundary itself — not chunking.

## D. Cache state (both arms)

| | v0.1.6 `namespace-10000` | v0.1.7 `pipeline-namespace-10000` |
|---|---|---|
| declaration | `fixture_cache_profile: "reused-first-sample-uncontrolled"`; `cache_contract: null` | `PreparedDewarmed` + a residency gate |
| evidence | `perf.jsonl` line 1 `cache_contract`, line 2 `records[0].fixture_cache_profile` | receipt `gates`: `g4.residency` limit `== 0`, measured `0 resident pages`, status `PASS` |
| reuse method | `setup.fixture_reuse_method: "host-prepared-source"` | `c2::prepare_sample`, `pipeline.rs:679` |
| cold claim? | **none** — `cache_contract` is `null` because `shared/cold.py:22-25` restricts `applies()` to `case == "namespace-100000"` | residency-gated, not a cold-source contract |
| disk reads vs bytes claimed | 729,088 B read vs 300,000,000 B scanned = **0.243 %** | not applicable (no source scan in the timer) |

The parity spec already records this asymmetry as a constraint on pairing
(`namespace-10000-parity-spec.md:215-219`):

> `3. **The cache states differ and must not be pooled** — v0.1.6's row is`
> `   \`reused-first-sample-uncontrolled\` with \`cache_contract: null\`; this row is`
> `   \`PreparedDewarmed\` with a residency gate. The pair is therefore a comparison of two`
> `   declared, different cache states, and the report must say so rather than implying`
> `   one contract covers both.`
