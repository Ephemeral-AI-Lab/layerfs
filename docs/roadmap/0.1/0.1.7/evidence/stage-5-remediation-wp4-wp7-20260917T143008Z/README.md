# Evidence: WP4-WP7 second remediation round (2026-09-17)

Fresh directory. Nothing here overwrites
`../stage-5-remediation-20260917T090752Z/` (the WP1-WP3 round) or the retained
review evidence under `../stages-1-5-review-20260917T160000Z/`.

| Field | Value |
| --- | --- |
| Tree measured | `eb42c13477f85612fc864474af4489c2548d688d` (branch `main`) |
| Reviewer's diagnostic client | retained copy, built against this tree |
| Log | `diagnostics/reviewer-client-rerun.log` (all four binaries, every exit recorded) |
| Adaptation | `diagnostics/dangling-adaptation.diff` - see below |
| Component comparison | `../stage-5-component-comparison-20260917T143008Z/` |

## The reviewer's diagnostic client, before and after

Built from a copy of the retained client
(`../stages-1-5-review-20260917T160000Z/diagnostics/stage5-diagnostics/`) with
`CARGO_TARGET_DIR=/tmp/lfs-diag-target-wp45 cargo +1.85.1 build --offline`. The
"before" column is the reviewer's own `run.log`, `topology.log`,
`dangling-reference.log` and `ordering-growth.log` in that retained directory.

| probe | before | after |
| --- | --- | --- |
| `list.max_bytes=1,14,15` | `Ok entries=0 continuation=None` | `Err ObjectLimitExceeded` (R6) |
| `list.max_bytes=16` | `Ok entries=1` | `Ok entries=1` (unchanged) |
| `build.disconnected_cycle` | `ACCEPTED root=889aaec4...` | `REFUSED InvalidRecord("effective tree cycle")` (R8) |
| `build.orphan_declared_new` | `REFUSED new inode without binding` | unchanged |
| `T1.update_new_dir_cycle` | `ACCEPTED` | `REFUSED effective tree cycle` (R8) |
| `T2.dir_second_parent` | `ACCEPTED`, `stat d/e=Ok((2,2))`, `stat d/e2=Ok((2,2))` | `REFUSED multiple parents` (R1) |
| `T3.symlink_second_parent` | `ACCEPTED`, `lookup_serial_4=Ok(Some(2))` | `REFUSED multiple parents` (R1) |
| `T4.same_batch_duplicate` | `REFUSED` | `REFUSED` (control unchanged) |
| `store.open.old_schema` | `ACCEPTED policy=...` | `REFUSED UnsupportedPolicy { field: "object role constraint" }` (R7) |
| `read_work.two_stats` | `directory.pages_read=0` | `directory.pages_read=1 directory.read_waves=1 inode.pages_read=3` (R34) |
| `boundary.*` (name, path, components, symlink target, inode serial) | as recorded | byte-for-byte identical (no boundary regressed) |

`dangling` still shows `file_value_roots_present_in_store=0 (of 2)` for its
dangling arm, which is the **documented** outcome: R2 was closed by writing the
exclusion into `admission-and-persistence.md` and pinning the accepted input
(the dangling arm's root reads back as `Err(ObjectMissing)`), not by making the
save refuse it. The adapted probe prints `commits=1` where it used to print
`acknowledged=true`, because that field was hard-coded and is gone (R38).

### The client does not build unmodified, and the WP4 comment said it did

`src/bin/dangling.rs` uses `SaveOperation::accept(object, scope)` and
`SaveOutcome.acknowledged`; R18 and R38 removed both, so `cargo build` fails with
E0061 and E0609. The other three binaries (`stage5-diagnostics`, `ordering`,
`topology`) build unchanged. The comment posted for WP4 claims the client "still
builds unmodified" - that claim is **wrong for one of the four binaries** and is
corrected in a dated comment on #170. The adapted copy here changes exactly those
two call sites and nothing else; the diff is retained as
`diagnostics/dangling-adaptation.diff`.

## The ordering-growth probe, before and after

Same probe, same inputs, same machine class, one sample per point.

| entries | before elapsed_ns | after elapsed_ns | before `rows_read` | after `rows_read` |
| ---: | ---: | ---: | ---: | ---: |
| 200 | 145,656,500 | 57,135,792 | 2,866 | 52,466 |
| 400 | 530,958,375 | 177,277,791 | 7,394 | 212,769 |
| 800 | 1,988,077,958 | 554,897,750 | 17,986 | 848,208 |
| 1,600 | 7,604,108,916 | 1,994,595,333 | 42,370 | 3,378,078 |

Per-doubling factors, spilling arm: **3.10x, 3.13x, 3.59x** after, against
**3.65x, 3.74x, 3.83x** before; the no-spill control is 2.09x, 2.14x, 2.28x. The
spilling arm is therefore still superlinear against its own control, and this
directory states that plainly rather than claiming a linear path: what R3 fixed is
the *lookup* scan (one cursor per tier instead of a rescan per serial) and the
counter honesty - `rows_read` now moves with the reads that actually happen
(2,866 -> 42,370 before, 52,466 -> 3,378,078 after) instead of reporting a linear
figure for quadratic work. The remaining growth is the tier-merge work itself,
which is charged and bounded but not constant per entry.

## What this directory does not contain

No complete-operation comparison (`VF-6`, `NOT_RUN`), no cold-cache row, no
pack-footprint row and no whole-process memory row. Those belong to Stage 6
(#171) and were never part of this round's scope.
