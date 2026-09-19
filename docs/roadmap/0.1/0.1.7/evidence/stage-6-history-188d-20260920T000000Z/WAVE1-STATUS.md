# Wave 1 status — W2 and W3 complete, W1 mid-flight

**Diagnostic.** Machine load 4.09-5.46; one `cargo` process (W1) running.

## The result so far

@```
  the registered lane      128,864,256    2.61304x
  today's default           56,049,664    1.13654x
  W2 + W3  (the product)    45,432,832    0.92126x    <- gate cleared by 3,883,008 B
                                                        target cleared by 1,615,603 B
  v0.1.6 (the gate)         49,315,840    1.00000x
  the T1 target             47,048,435    0.95402x
@```

**Verified independently** — `space.py`, `quick_check = ok`, sha256 `729f9ecd7e5dcbbcfedcbcf144a8af73…`.

## W2 — the persisted cross-save similarity index. COMPLETE

**The honest trade, measured on a frozen binary, quiet before AND after:**

| | PRODUCT index | HARNESS index ON | delta |
| --- | --: | --: | --: |
| apparent | **45,432,832** | 44,175,360 | +1,257,472 |
| whole-file lane | 34,761,166 | 33,554,278 | +1,206,888 |
| pooled / ordinary / native | identical to the byte | | 0 |
| @operation_ns@ | **37.86 s** | 40.35 s | **-2.49 s** |
| @cpu_user + cpu_system@ | **44.29 s** | 46.80 s | **-2.51 s** |
| @process_peak_rss_bytes@ | **247,857,152** | 255,049,728 | **-7,192,576** |
| @delta.ineligible_candidates@ | 683 | 2,675 | |

**The product form is 1,257,472 B (2.85 %) behind the harness form, and is 2.49 s faster with 7.19 MB
lower peak RSS.** It reproduces 98.2 %. **That is a genuine Pareto point**, not a regression: 1.26 MB of
storage bought back as 2.5 s of CPU and 7.2 MB of memory.

**Bounded memory, as the mandate asks:**

@```
  slot array    8,192 x 68 B = 557,056     (32 B id + 32 B folded signature + 4 B Option)
  references   65,536 x  2 B = 131,072     (u16; NO_ENTRY 65,535 > SLOTS 8,192)
  measured live_bytes         688,128 B    [M] asserted by an external test
  declared INDEX_BYTES        720,896 B    [C] = 704 KiB
  previous SLOTS 32,768     2,359,296 B    => 1,671,168 B of resident memory for ZERO coverage
@```

**The saturation mechanism, confirmed on the real lane:** `select.rs` increments @delta.no_candidate@ and
**only then** calls `candidates.insert`; a PREFIX-stored object is never inserted. So the index holds the
objects that **failed to get a base** — it is **self-limiting**. Measured: **6,594 rows of 8,192 slots**
(product arm) and 6,011 (harness arm), with @max stamp == rows == distinct slots@ — **the ring never
wrapped in either arm.** Sized for the declared-only arm, the more conservative of the two.

**Access cost: the read path is 0 B.** `Store::open` reads <= 516,096 B (<= 8,192 rows, refuses more);
`flush` writes only that save's admissions; **reads touch the index not at all.**

**Rejected with numbers:** rebuild-at-open (would cost the whole Store's pack bytes per open);
`UNIQUE(object_id)` (~40 B/row, cannot fire); widening `REFERENCES` past 65,536 (+184 objects for
+128 KiB).

**The 6,377,472 B is RELOCATED, not newly bought** — the capability now exists in the product with the
harness switch OFF. W2's derived attribution: **5,390,336 B to the index (84.5 %)**, residual 1,335,296 B
**not measured**. And the 45,432,832 is the **tree's** number, because W3's change is in the same binary.

## W3 — the row grammar. COMPLETE

**`objects.base_object_id` is REMOVED, not replaced** — the packed record is the single source of a
dependency edge. Isolated on a byte copy: `objects` 4,145,152 -> 2,809,856 B = **1,335,296 B = 326 pages
exactly**, with the page arithmetic stated and residual 0.

**The trade the earlier note flagged was avoided, not paid:** 94.4 % of edges cross packs and the mean pack
is 193 KB, so a writer depth-walk moved onto records would read ~375 KB per cache-missing walk to save
32 B/row. `cost_of`/`depth_of` therefore take a base provider over the **selection's own pack cache**,
which the trial of the same base fills next — marginal pack reads ~0. **Pack fetches unchanged.**

**Rejected with numbers:** 3-int locator (saves 1,044,480 but is 290,816 *worse* and does not remove the
seek); delta-encoded locator (1,069,056); dropping `object_role` alone (49,152, and **not derivable** —
the Singleton lane holds both WholeFile and Chunk).

**And W3 rewrote two tests that were failing for W2's reason** — their fixtures stored a *near copy* and
asserted no trial ran, but W2's persisted index now legitimately proposes that copy. W3 made the payloads
wholly different so each tests the boundary it names, assertions unchanged. **The parent's assessment: that
reading is correct.** The fixtures were accidentally relying on the index being empty; the assertions were
never weakened.

## W1 — the codec. MID-FLIGHT, and the tree is RED because of it

W1 has changed `codec.rs`: **payload level 3 -> 9**, **group level 1 -> 19**, and `ENCODE_WORKSPACE_BYTES`
resized from 2 MiB to what the new parameters need.

**Current suite: 483 passed / 3 failed.** All three failures are W1's own in-flight territory:

| failing test | file | why |
| --- | --- | --- |
| `a_frame_beyond_the_profiles_window_is_refused` | `tests/codec_frames.rs:22` | the window/level parameters changed under it |
| `a_larger_incompressible_whole_file_record_uses_the_singleton_lane` | `tests/policy_capacity.rs:124` | ditto |
| `boundaries_follow_the_configured_cutoff_not_a_frozen_one` | `tests/policy_capacity.rs:172` | ditto |

**This is expected mid-work, not a regression** — W3 measured **486 passed / 0 failed** before W1's change
landed, and the three failures are exactly the codec/policy tests W1 must update. **They must be green
before the wave is called done.**

## Integrity incidents, recorded

**The shared harness binary was rebuilt under W2 mid-campaign** (mtime 03:41:08), so its first arm pair is
**confounded and discarded** — receipts kept. **A concurrent squad ran a lane while W2 held
`/tmp/lane.lock`.** And **the parent overwrote `/tmp/lane-baseline` and `/tmp/lane-after` with binary
copies**, which is why W3's store paths no longer resolve. All three are coordination failures worth
naming: the lane lock was advisory and two writers ignored it.

## Not claimed

**No stride3 confirmation exists.** The verify phase is a declared sample reporting @INCOMPLETE@. The
complete command is **44.53 s against a 15 s ceiling — FAIL, and it already was one.**
