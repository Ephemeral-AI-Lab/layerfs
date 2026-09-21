# Pre-registration — #219 round 10: split the C1 build span

Written **before** the first run of this arm and before any edit. **No product line changes**: the
product's own phase-instrumented entry point already exists and is wired up by this round.

Control: **I1** (`benchmark-results/issue219/ns19-I1-boundary-20260921T080734Z`, `31326f7a8`) and
**H2** before it: `operation_work_ns` 1653.8 / 1656.2 ms, CPU 1661.5 / 1682.5 ms,
`span_build_ns` **311.0 ms** (`diag_accept_plumbing_ns` 1460.0 ms over 25,245 accepts),
`span_content_ns` 1321.1 ms, 13/13 gates, 14/14 pins.

## The one difference

**`span_build_ns` is charged in its parts**, where it is one 311 ms span today.

- The product already has a phase-instrumented entry point — `build_filesystem_timed` /
  `update_filesystem_timed` with `FilesystemPhases::new(scope)`, recording six phases inside the
  build: `validate`, `directories`, `references`, `inodes`, `cleanup`, `root.encode`
  (`layerfs-content/src/filesystem/update.rs`). **Nothing in the tree calls it**: `FilesystemPhases`
  has no call site outside its own definition, so the machinery has never run. The measured closure
  switches to it, so the split lands in the product's own `timing.json` and in the receipt.
- The consumer path is separated from the tree build: `CountingConsumer` accumulates the wall time
  its `accept` calls take, which is charged to `pipeline.build_accept_ns`. The build emits **382**
  metadata objects, and 311.0 − accept is the tree build.

One difference: the build span's parts. No product bound, no format, no timer boundary, no pin moves.

## Prediction, in the instrument's own units

Derivable today, from the row's own counts:

| instrument | predicted | derivation |
| --- | ---: | --- |
| `pipeline.build_accept_ns` | **15–30 ms** | 382 metadata accepts at the row's measured 57.8 us per accept (1460.0 ms / 25,245) |
| tree build | **280–296 ms** | 311.0 − accept |
| `validate` + `directories` + `references` + `inodes` + `cleanup` + `root.encode` | must sum to the tree build **minus a residual** | the six phases do not cover `unreachable_parents`, `ReferenceReducer::new`, `check_backing_capacity` and `register_values` |

The prior on the *distribution* is weak and is stated so: `validate` walks all 4,096 bindings of
each batch (`validate.rs`, 824 lines) and `references` reduces every binding's reference runs
(`references/reduce.rs`, `sorted/merge.rs`), so those two are expected to be the largest, and
`cleanup` and `root.encode` the smallest. **The round does not predict a winner; it measures one.**

## What would refute it

1. `build_accept_ns + sum(phases)` accounts for **less than 70 %** of `span_build_ns` on this
   workload: the instrument would be missing a part of the build, and the split would be reported as
   incomplete rather than used.
2. Any of the 14 pinned counters moves, `pipeline.commits` != 284, the root digest changes, or the
   row is not PASS 13/13.
3. `span_build_ns` moves outside **311.0 ± 100 ms**: switching to the timed entry point must be
   work-neutral, and a larger movement means the instrumentation is not.
4. `operation_work_ns` outside 1653.8 ± 250 ms, or CPU outside 1661.5 ± 250 ms.
5. The harness's own suite gains a failure beyond the three pre-existing `registry_negative` cases.

## What is not claimed

Nothing about v0.1.6. This round attributes this row's own largest unnamed span; it fixes nothing
and predicts no movement. Its output is the target for the round after it, and if the split shows the
280–296 ms is spread evenly across six phases with no dominant term, that is the answer and it will
be reported as one.
