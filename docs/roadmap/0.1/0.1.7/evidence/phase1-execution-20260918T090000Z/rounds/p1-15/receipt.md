# P1-15 receipt — **incomplete** (hybrid binary-search-on-restart)

> **Status:** **Incomplete, reverted, not landed.** No commit for this item; the
> tree is clean at `51d42483e` and the ordering suites are green (15 + 2 passed).
> This receipt records what was built, the failure it hit and the exact next step.

## 1. The item, as planned

A restart — a request behind a tier's cursor — replays every row before the request,
which on the forced-64 shape is up to ~2,048 rows in the top tier. Target: probe for
the request's position instead, `O(log2(rows))` single-row reads (~11 for 2,048),
keeping the sequential resume for ascending sweeps (a pure binary search would
worsen the pinned one-pass sweep).

## 2. What was built

* `first_row_at_or_above` (`runs.rs`): a row-aligned binary search over the run's
  handle, `log2(rows)` probes of one 96-byte row each, every probe charged to
  `work.rows_read`. It returns the position **and the row it proved**, so the caller
  never pays for that row twice.
* The restart arm of `find`: a request behind the cursor (`scan.resume` exists and
  `serial < resume`) now probes instead of `scan.reader.start(run, 0)`. An exact hit
  is answered from the proven row; otherwise the scan continues **after** it.
* `RunScan::start_after` (`merge.rs`): continues one row past a proven position.
  Without it the probe's landing row was re-read by the sequential loop, which cost
  one extra read per restart and broke the pinned one-pass sweep
  (`lookups_allocate_nothing_after_the_tiers_are_built`: `sweep_reads` 787 against
  `total` 768 — two restarts in that fixture, so two extra rows).
* The sequential resume arm is **untouched**, which is what the item requires.

With `start_after` in place the sweep is one pass again and the restart cost is
probe-count instead of position.

## 3. What failed

`filesystem_ordering::run_lookup_answers_every_serial_in_every_order` failed on
serial 89 (`delta` 2 returned where 5 is newest), and this is **not** a counter
difference — it is a wrong row. The instrumented trace of the three `find(89)` calls
in that test:

```text
PROBE find89 tier=1 run=(60,2,90) via_loop found=Some(Effect { serial: 89, .., delta: 5 })
PROBE find89 tier=1 run=(60,2,90) via_loop found=None
PROBE find89 tier=2 run=(90,1,90) via_loop found=Some(Effect { serial: 89, .., delta: 2 })
```

Reading it: the third call's tier-1 line shows the **sequential** loop reporting
`found=None` — so the restart arm did not fire for tier 1, even though
`scan.resume` must have been past 89 for that arm to be skipped. `find` then fell
through to tier 2, whose range `(1, 90)` contains 89 but whose rows hold a stale
duplicate, and returned that.

**The latent bug my change exposed.** `find` treats a tier's `range` as proof that
the tier holds the serial: if `serial` is inside `[first, last]` and the sequential
scan finds nothing, it moves on — and a later tier's range can then match a row the
earlier tier was the authority for. That is normally unreachable, because the scan
is only ever positioned forward through its own run. The probe's
"continue after the proven row" is the first code to position the scan from outside
the sequential walk, and the resume/offset bookkeeping across a restart and an exact
hit is not consistent with it.

So the failure is not the binary search itself (it proves the right row — see the
first line, `delta: 5`) but the interaction between the probe's positioning and the
per-tier `resume` state.

## 4. Disposition

Reverted in full. The item is **incomplete**: the probe mechanism works and one
concrete accounting bug (`start_after`/extra read) was found and fixed in the
attempt, but the positioning/resume interaction is unresolved.

**Exact next step**, in order:

1. Make the restart arm re-verify: after probing, read the row at the proven index
   and compare it, rather than trusting the position and the tier's range.
2. Then make `find` refuse to fall through on a range match alone — if the tier's
   range contains the serial and the scan finds nothing, that is a miss for that
   tier *and* evidence the scan is mispositioned, not a licence to try an older tier
   that holds a superseded duplicate.
3. Only then re-derive `scan.resume` for the three cases (continue, restart-with-hit,
   restart-without-hit) as one table, and assert it with a test that alternates
   ascending and descending demands over a multi-tier store.

## 5. Evidence

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `cargo +1.85.1 test -p layerfs-content --test filesystem_ordering_scan` (attempt, before `start_after`) | 101 | `sweep_reads` 787 vs `total` 768 |
| R2 | the same after `start_after` | 0 | sweep is one pass again |
| R3 | `cargo +1.85.1 test -p layerfs-content --test filesystem_ordering` (attempt) | 101 | 4 failed, `run_lookup_answers...` among them |
| R4 | `git checkout -- …/references/{runs,merge}.rs …/tests/filesystem_ordering_scan.rs` then the two suites | 0 | **15 + 2 passed** |

The failing runs are not stored here: they ran on reverted working trees, and R1–R4
reproduce them from the described diff. Stated that way rather than as a receipt for
a tree that no longer exists.
