# #219 round 9 — the boundary, the 700 MB/s figure, and what the gap actually is

Pre-registration: `pre-registration.md` (written before the edit and before the run). Row:
`benchmark-results/issue219/ns19-I1-boundary-20260921T080734Z`, **PASS, 13/13 gates, 14/14 pinned
counters**, `source_commit` `31326f7a8`, clean, harness binary `89abebffc61d16d1…`.
**This round changes no product line**: it is one instrument, 33 added lines in the harness.

## 1. Where "700 MB/s" comes from, and what the reference's own receipts say

The figure is the **`init_namespace` `namespace-10000` acceptance bar**, and this case counts
**400 MB** (300 MB logical + 100 MB anchor). Six `namespace-10000` rows exist in
`benchmark-results/**/perf.jsonl` — all `candidate`, all **73 transactions** — and none of the
published summaries carry the number that decides the question: **the CPU they spent, and therefore
how many cores they used.**

| `layerstack_init_ns` | MB/s @ 400 MB | total CPU | **cores** | objects written |
| ---: | ---: | ---: | ---: | ---: |
| 402.721 ms | 993 | 1,783.3 ms | **4.43** | 54,465 |
| 407.598 ms | 981 | 1,649.2 ms | **4.05** | 54,458 |
| 578.245 ms *(the bar)* | 692 | 1,690.8 ms | **2.92** | 54,463 |
| 928.022 ms | 431 | 1,789.5 ms | **1.93** | 25,158 |
| 1,020.422 ms | 392 | 2,116.7 ms | 2.07 | 25,158 |
| 1,100.711 ms | 363 | 2,195.3 ms | 1.99 | 25,158 |
| **this row (I1)** | 180 | **2,227.1 ms** | **1.016** | 25,245 |

Two facts fall out of the reference's own numbers. **CPU is nearly flat (1,649–2,195 ms) across a
2.7x spread of wall time**, and wall time tracks the core count. And **the three fast rows are not
this workload**: they write 54,46x objects where the three slow ones and this row write 25,15x for
the same 300 MB — a different object decomposition.

The core count is a **rule**, not an accident: `AGENTS.md` §3.8 pins one construction worker for
every case **except `init_namespace`**, and every run exports `LAYERFS_CONSTRUCTION_WORKERS=1`. All
six reference rows are the exempt path. This row is `pipeline-namespace-10000`, a pipeline row.

The repo already recorded the provenance and the caveats
(`docs/roadmap/0.1/0.1.7/issue219-v016-gap-rca-handoff.md` §1b): all 75 `init_namespace` receipts are
`verification_status: NOT_RUN` with `cache_contract: null`, the candidate 100k rows span 279 ms to
105.9 s, and *"a bare '700 MB/s' is a best-of."* The bar is 578.245 ms → 691.8 MB/s; the best row is
402.721 ms → 993 MB/s.

## 2. Why the comparison could not be made before this round

The two timers contain different work. The reference's `layerstack_init_ns` **includes** reading its
300 MB fixture and building the 25,158 objects it admits; this row's `operation_work_ns`
**excludes** that construction (C2's supplied-object rule puts it in untimed setup) while
**including** the C1 tree build — which the reference also pays. Only the *sum* was published
(`preparation_wall_ns`), and a reader could not subtract it.

The instrument: two `Instant` pairs around existing code, published beside the formula and inside
neither.

## 3. The measurement — both predictions hit, all five refutation clauses clear

| pre-registered | predicted | I1 | verdict |
| --- | ---: | ---: | --- |
| `pipeline.construct_ns` | 380–560 ms | **445.74 ms** | **hit** (the 617 MiB/s derivation said 466 ms; measured 675.7 MB/s) |
| `pipeline.construct_noise_ns` | 100–260 ms | **119.86 ms** | **hit** (2,512.7 MB/s) |
| their sum | 480–820 ms | **565.60 ms** | **hit** |
| inside `preparation_wall_ns` | must be | 565.60 of **912.88 ms** | consistent; **347.27 ms of preparation stays unattributed** |
| `operation_work_ns` | 1656.2 ± 250 ms | 1653.81 ms | work-neutral |
| CPU user+system | 1682.5 ± 250 ms | 1661.54 ms | work-neutral |

Refutation clause 1 (`sum < 150 ms`, which would have made the correction nil) **did not fire**, and
neither did clauses 2–5: every pin and every work counter reproduces (`commits` 284, `inserted`
25245, `statements` 16595, `pack_bytes_written` 302,406,480), `digest:filesystem_root` is unchanged,
the row is PASS 13/13, and the harness's own suite is **120 passed / 3 failed**, the three being the
pre-existing `registry_negative` cases.

## 4. The correction — and it corrects this lane's own claim

**Boundary-matched: `operation_work_ns` 1653.81 + 565.60 = 2,219.4 ms of work, 2,227.1 ms of CPU.**

Against the reference's three **same-shape** rows (25,158 objects, 1.93–2.07 cores):

| | CPU | single-core-equivalent wall (wall x cores) |
| --- | ---: | ---: |
| reference best | 1,789.5 ms | 1,791 ms |
| reference middle | 2,116.7 ms | 2,112 ms |
| reference worst | 2,195.3 ms | 2,190 ms |
| **this row, boundary-matched** | **2,227.1 ms** | **2,219 ms** |

**L68 said this row was 6–23 % ahead of the reference on CPU. That was a boundary artifact and it is
withdrawn here.** Once the excluded construction is charged, the row lands **1.4 % beyond the worst
of the three reference rows and 24 % behind the best** — inside the same band, at its slow end. The
pre-registration predicted exactly this outcome if the excluded work was large
("2160–2500 ms … the standing hypothesis under test is: our CPU lead disappears"), and it did.

**The wall gap is then the core count, arithmetically.** 2,219 ms of single-core work at the
reference's ~2.0 cores is ~1,110 ms, i.e. **360 MB/s@400 MB against the reference's own 363–431
MB/s** for these three rows. There is no remaining unexplained factor between this row and the
same-shape reference rows once both boundaries and both core counts are stated.

## 5. What is still not measured, and the standing caveats

- **2,219.4 ms is a lower bound.** 347.27 ms of `preparation_wall_ns` is unattributed, and it
  contains product work that is still uncharged: the three untimed chain `build_filesystem` /
  `update_filesystem` calls and the oracle replay's three more. Charging those would move this row
  further *behind*, not ahead.
- The reference reads a fixture from a workspace over FUSE inside a Linux container; this row
  generates its bytes in-process (`construct_noise_ns`, 2.5 GB/s) and runs natively on macOS. The
  noise term is the analogue of the reference's cache-served 300 MB scan
  (`initialization_disk_read_bytes` 0–0.73 MB) and is charged on our side for that reason.
- **No pairing is claimed.** Different workspaces, no matchable seal, and the reference's
  `verification_status` is `NOT_RUN` throughout. The comparison above is a boundary-matched
  hypothesis between two implementations, and it is recorded as one.
- `construct_ns` is charged *around* a `Timing::disabled` node, so it includes that node's own
  overhead; the node is the harness's, not the product's.

## 6. What this implies for the next round

The row's own figure (1,653.8 ms) is still what the target of 1 s is measured against, but the
comparison that motivated it now has a floor under it: **the same work costs ~2.22 s of CPU here and
~1.79–2.20 s there, and the reference spends 2 cores to our 1.** So the two levers are (a) the
remaining ~1.65 s of timed work, of which ~336 ms is commit, ~311 ms the C1 build, ~243 ms encode,
~207 ms pack writes and ~134 ms row inserts, and (b) the owner rule that holds this case at one
worker. (b) is not a product change and is not mine to make; it is recorded as the standing
question, with the six reference rows as the evidence that it is worth ~2x on the wall figure.

Checks as run: harness suite `--no-fail-fast` **120 passed / 3 failed** (the pre-existing
`registry_negative` cases, byte-identical to `9a3bcd501`). Not run: the product's suites, which this
commit cannot affect, and any other harness case or lane.

Production LOC: **31377 -> 31377 (delta 0)**; the harness under `core/benchmark/` is not product
source. Method `python3 tools/production_loc.py --root <tree>`, first parent `9a3bcd501` against the
committed tree.
