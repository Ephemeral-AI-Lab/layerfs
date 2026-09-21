# #219 round 10 — the C1 build span, split: `validate` is 72 % of it and 14 % of the row

Pre-registration: `pre-registration.md` (written before the edit and before the run). Row:
`benchmark-results/issue219/ns19-J1-buildsplit-20260921T081749Z`, **PASS, 13/13 gates, 14/14 pinned
counters**, `source_commit` `b09783b98`, clean. **No product line changed**: the product already
shipped the split and nothing had ever called it.

## 1. The measurement

| instrument | I1 | J1 | share of `span_build_ns` | share of the row |
| --- | ---: | ---: | ---: | ---: |
| `span_build_ns` | 312.17 ms | **308.59 ms** | 100 % | 19.4 % |
| **`validate`** | — | **221.70 ms** | **71.8 %** | **13.9 %** |
| `references` | — | 9.32 ms | 3.0 % | 0.6 % |
| `inodes` | — | 6.02 ms | 2.0 % | 0.4 % |
| `directories` | — | 3.11 ms | 1.0 % | 0.2 % |
| `root.encode` | — | 0.00 ms | 0.0 % | 0.0 % |
| `cleanup` | — | 0.00 ms | 0.0 % | 0.0 % |
| **six phases, summed** | — | **240.16 ms** | **77.8 %** | 15.1 % |
| residual (outside every phase) | — | 68.38 ms | 22.2 % | 4.3 % |
| `pipeline.build_accept_ns` | — | **0.05 ms** | 0.02 % | 0.003 % |

**`validate` is 221.70 ms over 10,101 bindings — 21.9 µs per binding — and it is a single phase
with a single call site** (`layerfs-content/src/filesystem/validate.rs`, 824 lines, reached from
`run_body` at `update.rs:162`). Nothing else in the build is worth a round: the next largest phase
is 9.32 ms, and `cleanup` and `root.encode` are free.

The residual is the work `run_body` does outside every phase — `unreachable_parents`,
`ReferenceReducer::new`, `check_backing_capacity` and `register_values` — and at 68.38 ms it is
larger than every phase but `validate`.

## 2. What the instrument had to be

`span_build_ns` was one span, and a span cannot say whether it is the product's `accept` or the
caller's tree build, nor which part of the build it is. The product **already ships** the answer —
`build_filesystem_timed` / `update_filesystem_timed` with `FilesystemPhases::new(scope)` record six
phases — and **no call site outside its own definition existed anywhere in the tree**: the machinery
has been dead code. The measured closure now uses it, so the split is charged by the product itself,
lands in the product's `timing.json`, and is read back out of the report into six counters. The
consumer half is separated in the harness: `CountingConsumer` accumulates the wall time its own
`accept` calls take.

## 3. The prediction that was missed, and how

| pre-registered | predicted | measured | verdict |
| --- | ---: | ---: | --- |
| `pipeline.build_accept_ns` | 15–30 ms | **0.05 ms** | **badly missed** |
| the tree build | 280–296 ms | 308.54 ms | missed, because the accept term is nil |
| phases account for ≥ 70 % of the span | required | 77.8 % | **not refuted** |

The prediction came from dividing the row's `diag_accept_plumbing_ns` (1460.0 ms) by its 25,245
accepts and multiplying by the build's 382. That average is dominated by the **content** accepts,
which carry large objects into the save; a metadata object is almost free to accept (0.13 µs). The
derivation was wrong in a way the row could have told me: the 382 metadata objects are 1.5 % of the
accepts and are the *small* ones. Recorded as missed rather than explained away.

## 4. Everything else held

All 14 pins and the root digest reproduce, `pipeline.commits` 284, `metadata_objects` 382,
`objects_emitted` 67; the row is PASS. `span_build_ns` moved −3.59 ms, inside its own drift, so
switching to the timed entry point is work-neutral. J1 is uniformly 3–6 % faster than I1 across
every component (`commit` −22.7, `offer` −34.8, `full` −8.0, CPU −44.6), i.e. **the −63.4 ms on
`operation_work_ns` is a machine window**, not the instrument — which is consistent with this lane's
±250 ms rule and is stated rather than banked. Harness suite: **120 passed / 3 failed**, the three
pre-existing `registry_negative` cases.

## 5. The target this round produces

**`validate`, 221.70 ms, 21.9 µs per binding.** That is now the largest single identified term in
the row outside the store's own commit, and it is pure CPU in `layerfs-content` — no SQL, no pager,
no format. The next round reads `validate::check` and prices whatever it finds there against this
figure, on this instrument.

Checks as run: harness suite `--no-fail-fast` **120 passed / 3 failed**. Not run: the product's
suites, which this round cannot affect, and any other harness case or lane.

Production LOC: **31377 -> 31377 (delta 0)**; the measurement harness is not product source. Method
`python3 tools/production_loc.py --root <tree>`, first parent `aedf15ded` against the committed tree.
