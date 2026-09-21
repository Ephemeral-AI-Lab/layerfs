# Report — #219 round 21: `pipeline-namespace-100000`, built, pinned and anchored

Pre-registration: [`pre-registration.md`](pre-registration.md), written before the first locked run.
Raw receipts under [`raw/`](raw/), copied out of `benchmark-results` because that tree is not tracked by
git. **One sample per case per arm, `--verify full`, fresh `--out` per run, no receipt retuned.**

## 1. The headline, and what it is comparable to

| | `pipeline-namespace-10000` | `pipeline-namespace-100000` | ratio |
| --- | ---: | ---: | ---: |
| run | `ns21-A1-10000-pinned` | `ns21-C2-100000-pinned` | |
| harness binary sha256 | `e436798007aa3748…` | `288e3c9b248632f6…` | **different binary — see §7.1** |
| session | 15:47Z | 15:51Z | 4 min apart |
| `pipeline.operation_work_ns` — the declared figure | 921,959,083 | **3,673,602,750** | **3.99×** |
| `pipeline.accept_span_ns` — the inclusive closure | 955,937,625 | **4,162,787,625** | **4.36×** |
| `pipeline.establishment_ns` | 2,445,916 | 2,615,416 | 1.07× |
| `pipeline.teardown_ns` (excluded from the figure) | 33,978,542 | 489,184,875 | 14.40× |
| `pipeline.span_build_ns` — the C1 tree build | 68,604,167 | 1,138,887,709 | 16.60× |
| `pipeline.span_content_ns` — the content accept loop | 841,842,542 | 2,523,779,250 | 3.00× |
| `pipeline.span_finish_ns` — the save's seal | 45,490,875 | 500,120,666 | 10.99× |
| entries / directories | 10,000 / 100 | 100,000 / 1,000 | 10× |
| declared content | 300,000,000 + one 100,000,000 B anchor | **500,000,000 + two 100,000,000 B anchors** | 1.67× |
| `pipeline.content_bytes` (canonical, measured) | 301,171,810 | 502,914,928 | 1.67× |
| `pipeline.content_objects` | 24,863 | 109,414 | **4.40×** |
| `pipeline.bindings` | 10,100 | 101,000 | 10× |
| `pipeline.batches` | 3 | 25 | 8.33× |
| `pipeline.commits` / `pipeline.statements` | 95 / 7,666 | 388 / 12,206 | 4.08× / 1.59× |
| `pipeline.packs_created` / `pack_appends` | 1,270 / 6,603 | 2,180 / 12,088 | 1.72× / 1.83× |
| `pipeline.pack_bytes_written` | 301,865,004 | 509,752,317 | 1.69× |
| complete command | 1,559,995,167 | **6,571,717,125** | 4.21× |
| peak RSS (**lifetime** high-water, not a phase figure) | 529,711,104 | 1,048,805,376 | 1.98× |
| **measured-region** RSS increment (§11.1) | **27,197,440** | **37,519,360** | **1.38×** |
| status | `PASS` 13/13 | **`PASS` 13/13** | |

Every figure in the table above is from the two runs named in it, except the two RSS-increment cells,
which come from `ns21-D2-10000-phaserss` and `ns21-D1-100000-phaserss`: the measured-region sampler was
added to the harness **after** the C2 pair, so those runs predate it. §11 says so and §11.6 carries the
cross-check that the instrument did not move anything else.

**`pipeline.operation_work_ns` and `pipeline.accept_span_ns` are both published**, as §7 of the handoff
requires, and so are `establishment_ns` and `teardown_ns`: `establishment_ns + operation_work_ns +
teardown_ns` reconstructs the inclusive closure on both rows.

## 2. The declaration, ported rather than invented

The pre-registration chose the port and the row is the port. `ops::namespace_content::Declaration::LARGE`
is the reference harness's own `namespace-100000` scenario field for field:

| field | value | this row, measured |
| --- | ---: | ---: |
| `regular_files` | 100,000 | `pipeline.declared_files` **100,000** |
| `data_directories` | 1,000 | `pipeline.declared_directories` **1,000** |
| `logical_bytes` | 500,000,000 | `pipeline.declared_content_bytes` **500,000,000** |
| `anchor_files` × `anchor_bytes` | 2 × 100,000,000 | `plan.class_counts()[4]` = 2, each `size == 100,000,000`; `anchor_total()` = 200,000,000 |
| empty / tiny / small / medium | 1,000 / 78,998 / 15,000 / 5,000 | asserted in `the_large_declaration_is_the_scaled_mix_not_the_small_one_repeated` |

The band arithmetic is checked rather than copied: empty (100 → 1,000), small (1,500 → 15,000) and
medium (500 → 5,000) scale exactly ×10; the anchor count goes **1 → 2**, not ×10; and **tiny is the
balancing band** at 78,998, which is **eight more** than 7,899 × 10. A first draft of that test asserted
a flat ×10 and failed on exactly those eight files — the declaration is the reference's and the
arithmetic is the check on it, which is how the eight were found.

**The 1,000 directories are load-bearing, not decorative.** The plan's index space is
`directory × FILES_PER_DIRECTORY + ordinal` with `ordinal = position / directories`, so at
`entries = 100,000` with 100 directories the ordinal runs to 999 and position 10,000 reuses index 100:
100,000 files would collapse onto 10,000 distinct serials and two files would share a constructed-content
identity. `plan` now **refuses** a declaration whose index space wraps
(`a_declaration_that_wraps_the_index_space_is_refused`) instead of silently producing a smaller fixture,
and the row declares 1,000 directories, which is also the reference's own `data_directories` and the
same number `c1.fs.build-scale`'s 100,000-entry rung uses.

**"500 MB" here is decimal.** `500,000,000`, the reference scenario's number. It is *not*
`c1.fs.build-scale`'s 100,000-entry rung, which declares `524,288,000` (500 MiB). Both are 100,000 files
over 1,000 directories; they differ in bytes, and this row ports the reference's.

### 2a. The declaration index and the row's `NAMESPACE_SCALE_BYTES` are now one value

`Declaration::TEN_THOUSAND.total_bytes` is `ops::pipeline::NAMESPACE_SCALE_BYTES`, so the 10,000-entry
row's total has one definition instead of two, and the 100,000-entry row reads its own out of the same
table. The driver refuses to run if `declaration(op)` and `configuration(op)` disagree about the file or
directory count — a harness defect, not a measurement.

## 3. The regression check: the old fixture did not move

`plan` was generalised from `plan(entries, directories, seed, total)` to `plan(&Declaration, seed)`, and
the anchor set from one slot to a set. The check on that refactor is the 10,000-entry row's own pins:

| run | result |
| --- | --- |
| `ns21-A1-10000-pinned-20260921T155500Z` | **`PASS` 13/13**; all **14 pinned counters and the pinned `filesystem_root` digest reproduced exactly** |

`pipeline.batches` 3, `bindings` 10,100, `chain_objects` 382, `commits` 95, `content_bytes` 301,171,810,
`content_objects` 24,863, `declared_content_bytes` 300,000,000, `declared_directories` 100,
`declared_files` 10,000, `inserted` 25,245, `largest_batch_bindings` 4,096, `metadata_objects` 382,
`objects_emitted` 67, `reused` 0, and
`1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847`. **The generalisation moved no
existing fixture**, which is the only thing that makes the new row's numbers comparable with the old
row's at all.

## 4. The session anchor — condition 1 of the owner's ruling, met

The handoff's §5 anchor is `namespace-10000` in `c1.fs.build-scale`: it builds a tree, opens no Store, and
therefore no product lever can reach it.

| | value |
| --- | ---: |
| this session, `ns21-C1-anchor-10000-20260921T160000Z` | **68,951,625 ns** |
| the reference value (`ns20-L1-10000-20260921T113500Z`) | 68,514,625 ns |
| difference | **+437,000 ns (+0.64 %)** |
| registered comparability bound | ±20 % (54.8–82.2 ms) |
| verdict | **comparable** |

The two runs are back to back inside one measurement lock window, the anchor first, the new row second,
4 minutes after the 10,000-entry row's own pinned run. The anchor's `fs_build.operations` /
`fs_build.bindings_added` are **3 / 10,100**, reproducing the reference row's own counters, and its
`g1.o3-pinned-counters` gate **PASSes** against the twelve counters the table pins for `namespace-10000`.

**Condition 1 is met for this round.** It is met for *this session only*: the anchor must be re-run
beside any future row that wants to be compared with this one.

### 4a. What the anchor exposes about this session

The row's pack-free work is the same binary-independent fingerprint the 10,000-entry row already
publishes, and it agrees across the campaign's two sessions to within 1.6 %:

| pack-free quantity | 11:50Z session | 15:47Z session | difference |
| --- | ---: | ---: | ---: |
| `construct_ns` (untimed, no Store exists) | 322,013,388 | 317,651,722 | −1.35 % |
| `construct_noise_ns` | 82,769,536 | 83,212,940 | +0.54 % |
| `span_build_ns` (writes no pack) | 67,551,417 | 68,604,167 | +1.56 % |
| `span_content_ns` (reads and writes one) | 863,894,083 | 841,842,542 | −2.55 % |
| `teardown_ns` (the save's connection close) | 103,048,334 | **33,978,542** | **−67.03 %** |

That last line is the point of the row's declared boundary, measured again: the term the row's formula
excludes moved by **69.07 ms between sessions of the same product, driver and case.**

## 5. The boundary, and what the new row says about it

`pipeline.operation_work_ns` is `accept_span_ns − finish_drop_ns`, where `finish_drop_ns` is the save's
owner being taken apart (99 %+ of it the connection close). Both are published.

| | 10,000 | 100,000 | ratio |
| --- | ---: | ---: | ---: |
| inclusive closure (`establishment + work + teardown`) | 958,383,541 | 4,165,403,041 | 4.35× |
| the declared figure | 921,959,083 | 3,673,602,750 | 3.99× |
| `teardown_ns` / closure | **3.5 %** | **10.5 %** | |

At 100,000 entries the excluded term is **10.5 % of the inclusive closure**, up from 3.5 % at 10,000.
**The larger the row, the more of it the declared boundary excludes** — so a future arm that moves work
across that boundary has more room to hide here than it did at 10,000. That is the strongest argument
for §7's requirement and it is now measured at two scales rather than argued.

### 5a. Byte-normalised cost: the pre-registered flatness claim is REFUTED

Registered: *"save cost per canonical byte flat within 15 %"*. Measured, from the two rows of §1:

| per canonical byte | 10,000 | 100,000 | ratio |
| --- | ---: | ---: | ---: |
| the declared figure | 3.0612 ns/B | 7.3046 ns/B | **2.386×** |
| save half (`accept_span_ns − span_build_ns`) | 2.9463 ns/B | 6.0127 ns/B | **2.041×** |
| `span_content_ns` (the accept loop alone) | 2.7952 ns/B | 5.0183 ns/B | **1.795×** |

It is not flat, and it is **not the product's save path that moved it** — see §6.1. The three readings
disagree by a factor of 1.3 and the disagreement is exactly the C1 build, which is charged to the row's
declared figure and is the one term in it that is not the save.

**What *is* flat, and is the sharper instrument:** the bytes the row hands the engine for packs.

| | 10,000 | 100,000 | ratio |
| --- | ---: | ---: | ---: |
| `pack_bytes_written` / `content_bytes` | 1.0023 | 1.0136 | **+1.13 %** |
| `pack_appends` / `packs_created` | 5.20 | 5.55 | +6.7 % |
| `statements` / canonical kB | 0.0255 | 0.0243 | −4.6 % |

The pack layer's amortisation is flat to 1.1 %, so **the row's extra bytes cost extra bytes and not extra
overhead** — the structural proportionality the pre-registration registered holds at the pack layer even
though it does not hold for the row's declared figure.

## 6. Three findings this row produced, and they are findings rather than this round's subject

Both are **harness-shaped observations about the row and its driver**, not product defects. Neither was
registered in §3 of the pre-registration, because the arithmetic that derived them was the ladder's —
which is what §6.1 shows is not sufficient at this size.

### 6.1 The C1 half is **not** proportional to the entry count, and the ladder does not predict it

Registered: `span_build_ns` **5–10×**. Measured: **16.60×**, i.e. **+66 % per binding**.

| | 10,000 | 100,000 | 100,000 per binding |
| --- | ---: | ---: | ---: |
| `span_build_ns` | 68,604,167 | 1,138,887,709 | 11,276 ns |
| — of which `build_directories_ns` | 2,120,918 | 219,627,499 | 2,175 ns (**×10.4 per binding**) |
| — of which `build_inodes_ns` | 4,366,834 | 295,932,167 | 2,930 ns (**×6.8 per binding**) |
| — of which `build_validate_ns` | 3,443,875 | 48,505,792 | 480 ns (×1.41) |

**This is not the product's build being slow, and it is not a contradiction of L81.** The same tree built
by `c1.fs.build-scale`'s own 100,000-entry rung costs **665,695,208 ns for 101,000 bindings = 6,591 ns
per binding**, flat against that ladder's 10,000 rung (6,784 ns, −2.8 %). The pipeline row's build of the
same tree costs 11,276 ns per binding. **The difference is what the pipeline driver puts between the
product and the Store, and it is the driver's own accounting that exposes it:**

| | 10,000 | 100,000 | ratio |
| --- | ---: | ---: | ---: |
| `validation_objects_read` | 17,975 | 291,615 | **16.22×** |
| `validation_read_waves` | 221 | 4,548 | **20.58×** |
| `validation_inode_pages_read` | 226 | 4,653 | **20.59×** |
| `chain_objects` (= `metadata_objects`) | 382 | 4,221 | **11.05×** |
| `prefix_records` | 4 | 45 | **11.25×** |

The build is batched at `MAXIMUM_WALK_ENTRIES` = 4,096, so it becomes **25 operations instead of 3**, and
each operation validates its base through the reader the driver hands it — a `PairProvider` over the
**prefix chain as it stood before that batch**. That chain grows with the directory count (4,221 objects
against 382) and each of the 25 batches re-validates against a longer one. Validation work per binding is
therefore **1.62× higher at 100,000 than at 10,000** for a tree whose own build is flat.

**What this means for the campaign, stated narrowly:** a `c1.fs.build-scale` rung does **not** predict the
C1 half of a batched pipeline row, because the rung is a single-pass build through a non-retaining
consumer and the pipeline row is a 25-operation build through a counting one over a prefix chain. The
ladder's flatness is a statement about the product's build; this row's `span_build_ns` is a statement
about the product's build **plus the driver's handoff**, and at 100,000 entries the second term is 42 %
of the first. Whether the prefix-chain reader is the right reader for this row is **not decided here** —
it is the round's finding and the next round's subject.

### 6.2 The metadata lane emits **11×** the objects for 10× the entries, and `objects_emitted` does not see it

| | 10,000 | 100,000 | ratio |
| --- | ---: | ---: | ---: |
| `chain_objects` = `metadata_objects` (what crossed the handoff) | 382 | 4,221 | **11.05×** |
| `objects_emitted` (`ObjectWork.objects_emitted`) | 67 | 89 | 1.33× |

The cross-check is independent and agrees exactly: `c1.fs.build-scale`'s own 100,000-entry rung reports
`fs_build.objects_emitted` = **4,221** for the same tree. So the metadata lane really does produce 4,221
objects, and **`pipeline.objects_emitted` is not that number** — it is `result.counters.objects.objects_emitted`
off the last batch only, which is why it reads 89 and is 47× below the count the handoff path measured.
The row publishes both; a reader who took `objects_emitted` for the metadata population would be wrong
by a factor of 47, and the row's own `g2.handoff` gate is the one that compares the right number.

### 6.3 The two declarations put the bytes in different places, and the Store's own reading says so

The row's `resources.space` block is the Store's own accounting, and it shows the two scenarios are not
one shape at two sizes **even in how the content is represented**:

| canonical objects | 10,000 | 100,000 |
| --- | ---: | ---: |
| `whole-file` | 9,444 (24,652,248 B) | **98,998 (302,276,954 B)** |
| `chunk` | 14,466 (275,868,750 B) | **10,330 (200,216,930 B)** |
| `inode-leaf` | 208 (858,923 B) | 2,088 (8,677,224 B) |
| total | 25,245 (302,231,057 B) | 113,635 (513,684,532 B) |
| `page_count` / apparent bytes | 81,987 / 335,818,752 | 141,574 / 579,887,104 |

**Chunked bytes fall while whole-file bytes rise.** The 100,000-entry declaration's larger files are
still under the whole-file cutoff while its tiny band is ten times wider, so 87 % of its objects are
whole-file against 37 % at 10,000 - and 200,000,000 of its declared bytes are the two anchors, which are
neither. A row that changed the *band mix* changes the *content representation*, and a reader comparing
these two rows on time alone is comparing two different distributions of work. That is a consequence of
porting the reference's declaration (section 2) rather than of the entry count, and it is recorded here
because it bounds every comparison section 1 makes.

## 7. What was registered, and what fired

| registered (§3 of the pre-registration) | outcome |
| --- | --- |
| `declared_files` / `declared_directories` = 100,000 / 1,000 | **fired exactly** |
| `declared_content_bytes` = 500,000,000 | **fired exactly** |
| two anchors of exactly 100,000,000 (200,000,000 together) | **fired exactly** |
| `bindings` = 101,000 | **fired exactly** |
| `batches` = 25 | **fired exactly** |
| `operation_work_ns` within 1.10× of 3.0–3.5 s | **fired** (3.674 s) |
| save cost per canonical byte flat within 15 % | **REFUTED** — 2.04× on the save half, 2.39× on the declared figure (§5a; the cause is §6.1, not the save) |
| `construct_ns` + `construct_noise_ns` **5–10×** | **REFUTED, and low** — **1.51×**, i.e. −85 % per file. Construction scales with the *non-empty file count* (109,414 against 24,863), not the file count: the declaration's 1,000 empty files and 15,000 small/5,000 medium files are a different mix, and the anchors are only 2 files. The registered band was derived from the 10,000-entry row's file count and was wrong for a declaration at a different band mix |
| `span_build_ns` **5–10×** | **REFUTED, and high** — **16.60×** (§6.1) |
| complete command ≤ 15 s, no `DECLARED_EXCEPTIONS` entry | **fired** — **6.572 s**, `declared_exception: false`, reason *"6.793 s within the complete-command limit"* |
| `PASS`, all gates, one sample, `--verify full` | **fired** — `PASS` 13/13 |
| session anchor within ±20 % of 68,514,625 | **fired** — 68,951,625, **+0.64 %** (§4) |

Two of the pre-registration's bands were derived from the control row's *file count* and are the two that
failed. The structural predictions — every count that the declaration determines — **fired exactly**,
which is the part of the table the declaration port was responsible for.

### 7.1 A defect in this round's own reporting, recorded rather than smoothed

The 10,000-entry run (§1, §3) and the 100,000-entry run were taken with **different binaries**:
`e436798007aa3748…` and `288e3c9b248632f6…`. The difference is the pinned-expectations table, which is
`include_str!`-ed into the binary; it was re-rendered between the two runs with the new row's pins. The
*wire* between them differs by one static string and no code, and §4a's pack-free agreement is the
evidence for that — but the pair is **not** a single-binary pair, and a reader is entitled to say so
rather than to be told the two columns came from one executable. `ns21-C1`'s anchor and `ns21-C2`'s row
**are** a single-binary pair (`288e3c9b…`), which is the pair §4 is built on.

## 8. The pins, and the choice the handoff asked for

**Bootstrapped from this round's own `PASS` run and pinned as the same fifteen labels the 10,000-entry
row pins** — fourteen counters plus `digest:filesystem_root`. `pipeline.packs_created` and
`pipeline.pack_appends` are **published and not pinned**, deliberately, on both rows.

*Why not pin them:* they are the only counters in the row that a **policy constant** can move without
moving any work. L80 measured them at 1,270 / 6,603 at 256 KiB packs and 295 / 7,578 at 1 MiB, and the
row's real work was identical. Pinning them makes the row's pin set a statement about the pack policy as
well as about the row, and any future round that deliberately changes that policy would have to re-pin a
row it did not mean to touch.

*The price, stated plainly:* **the row's pins would not have caught round 20's arm.** That arm's movement
was in exactly these two counters and in wall-clock terms that are never pinned. Both readings are
defensible and this round takes the second; the alternative is a row whose pins fire on policy changes.

The new row's fifteen:

```text
pipeline.batches 25                       pipeline.bindings 101000
pipeline.chain_objects 4221               pipeline.commits 388
pipeline.content_bytes 502914928          pipeline.content_objects 109414
pipeline.declared_content_bytes 500000000 pipeline.declared_directories 1000
pipeline.declared_files 100000            pipeline.inserted 113635
pipeline.largest_batch_bindings 4096      pipeline.metadata_objects 4221
pipeline.objects_emitted 89               pipeline.reused 0
digest:filesystem_root 2412681d335571082c4dfbc1df2117bc015b7f6132fca17cb0d11bbeaaefd954
```

They were read from `ns21-B2-100000-bootstrap-20260921T155500Z`, a direct `--phase perf` invocation of
the harness binary — **the same route the 10,000-entry row's own bootstrap comment records** — and the
covering run `ns21-C2` reproduced all fifteen. The route was necessary because of a circularity that is
worth naming: a row with no pins cannot be `PASS` through `runner.py` at all. Its two pinned gates are
`INCOMPLETE` by construction (`no pinned counters for this case`), `trace.py`'s row status is the **worst
gate**, so the derived receipt is `INCOMPLETE` and `shared/pin_expected.py` refuses to pin it. The
bootstrap frame is therefore a **PASS frame without a receipt**, which is why §9 labels it as such.

### 8.1 `--verify full` performed no verification on this row

All four pipeline runs carry `verification: {}` and `verification_invocation_ns: 0`. The runner's
`--verify full` is a mode, and the only thing the mode gates is whether a **deferred** verification
invocation is run; `namespace_scale`'s driver never declares one (`oracle_phase: verify-invocation`
appears in no trace), so for this row **`full`, `sample` and `none` are the same run.** The handoff's
description of the 10,000-entry row as *"one sample, `--verify full`, `PASS` 13/13"* is therefore
accurate about the number of gates and the mode it was invoked with, and the mode contributed nothing to
it. This is not a defect in this round's row — it is a property the row inherits — but a reader should
not read "`--verify full`" on this family as a second, independent check.

## 9. What is not claimed, and what was not run

- **No product change.** `core/crates/**` and `core/*/sql/**` are byte-identical to `070305949`. This is
  a harness round: a row, its declaration, its configuration, its tier, its pins and its run.
- **The 100,000-entry row is not a bar measurement.** The bar is `namespace-10000` ≤ 578.245 ms on
  `layerstack_init_ns` in the *reference* harness, a different case with a different timer in a different
  tree. Nothing here moves that number.
- **The row is not evidence about the save's cost per byte in isolation** (§5a): the row's declared figure
  charges the C1 build, which §6.1 shows is not the save.
- **The strict-vs-lax plan-run comparison was not run.** One `plan()` call over the 100,000-entry
  declaration was timed at **≈100 ms**, but it was timed by hand during development, is not a receipt and
  is not reported as a measurement.
- **The prefix-chain finding (§6.1) is not diagnosed.** The row says what the driver's reader costs; it
  does not say what a cheaper reader would cost, and no alternative reader was built or measured.
- **The `--verify full` caveat (§8.1) is not fixed.** It is a property of this row's driver, and changing
  it is a harness change this round was not commissioned to make.
- **The two memory reductions in §11 are not applied.** The prefix snapshots (§11.4) are priced at
  164,347,094 bytes and the content store (§11.3) at ~728 MB, and removing either changes the row's
  lifetime peak and therefore its binary — a fresh measurement round, not an edit to this one.
- **`resources.rss.process_peak_bytes` is still a lifetime high-water** and is still published as one.
  §11.1 adds the phase peak beside it; it does not relabel the lifetime figure, which
  `memory_cpu_space_support.md` §3.2 forbids and which other rows' receipts depend on.
- **No memory ceiling is claimed for this row.** `memory_cpu_space_support.md` §11 is explicit that the
  contract claims no absolute memory target; §11 measures where the bytes are and claims nothing about
  whether 1.05 GB is acceptable.
- **`ns21-B2-100000-bootstrap` is a PASS frame, not a receipt.** It has a `trace.jsonl` and no
  `receipt.json`; it is filed under `raw/` because §8's pins are read from it and a receipt a report
  depends on must survive `benchmark-results` not being tracked by git.
- **The 10,000-entry `ns21-A1` run and the 100,000-entry `ns21-C2` run are not a single-binary pair**
  (§7.1). The `ns21-C1` + `ns21-C2` pair is.

## 10. Reproducing it

```text
# the anchor (a tree, no Store) and the row, back to back, one lock window
python3 runner.py perf --case namespace-10000          --out <fresh> --verify full
python3 runner.py perf --case pipeline-namespace-100000 --out <fresh> --verify full

# the memory attribution of section 11 (diagnostic: registers no row, writes no receipt)
cargo test --release --locked --test namespace_memory_probe -- --nocapture

# the registry's own declaration, and the table
fs-bench-storage-content --emit-registry-tsv tests/golden/registry.tsv
fs-bench-storage-content --self-check
python3 runner.py self-check
```

`runner.py self-check` **passes** on this tree, including the registry rung, for the first time since
`pipeline-namespace-10000` was registered: the frozen cardinality array's `pipeline.*` entry read `4`
against five registered rows, so `registry::self_check` reported `frozen cardinality array` and the whole
self-check failed on the tree this round started from. It now reads `6` against six rows, and
`ADMISSION_CASES` reads `219`. **Two rows were added to the constant in one edit: the one that was
already missing, and the new one.** The `FROZEN_CARDINALITY` defect filed in L80 is fixed here, as the
handoff's §6 asked, because the count had to move anyway.

## 11. Memory: the row's 1.05 GB is 96 % harness, and the product's measured region is 37 MB

Added 2026-09-21 after the row was first filed. `resources.rss.process_peak_bytes` = 1,048,805,376 was
published and **could not be attributed**: it is `getrusage(RUSAGE_SELF).ru_maxrss`, a **lifetime**
high-water, and `memory_cpu_space_support.md` §3.2 says a lifetime figure is never an incremental one.
The row had no phase peak at all, so this section first adds the missing instrument and then answers the
question the instrument was added for.

### 11.1 The instrument that was missing

`namespace_scale` now starts an `RssSampler` (10 ms nominal interval, `instruments.rs:438`) immediately
before the measured closure and stops it immediately after, and publishes the bundle the memory document
declares: `pipeline.rss_phase_peak_bytes`, `rss_phase_baseline_bytes`, `rss_phase_incremental_bytes`,
`rss_phase_final_bytes`, `rss_samples`, `rss_maximum_gap_ns`, `rss_sampling_interval_ns`,
`rss_unavailable_samples`, and `rss_phase_peak_usable` — the bundle's own fail-closed rule
(`RssBundle::peak_is_usable`: no unavailable sample, more than one sample, no gap over twice the declared
interval) published rather than left for a reader to re-derive. None of it is pinned.

Measured on `ns21-E1-100000-final-20260921T164500Z` (`PASS` 13/13, `rss_phase_peak_usable` = 1,
335 samples, 12.55 ms maximum gap against a 20 ms limit):

| | bytes |
| --- | ---: |
| lifetime peak RSS (`resources.rss.process_peak_bytes`) | 1,050,738,688 |
| **measured-region baseline** | **1,013,219,328** |
| **measured-region peak** | 1,050,738,688 |
| **measured-region increment** | **37,519,360** |

**The process's lifetime high-water was already set before the timer.** The peak and the baseline differ
by 37.5 MB, which is everything the measured region adds: the Store's connection, its page cache, the
batch buffers, the seven `SaveProfile` buckets and the tree build.

### 11.2 The product's measured region is bounded, and it gets *better* per byte as the workload grows

| | 10,000 (`ns21-D2`) | 100,000 (`ns21-D1`) |
| --- | ---: | ---: |
| canonical bytes accepted | 301,171,810 | 502,914,928 |
| measured-region increment | **27,197,440** | **37,519,360** |
| increment / canonical bytes | 9.03 % | **7.42 %** |

A region that accumulated the content would show an increment near the canonical bytes. It shows
**7–9 %**, and the share *falls* by 1.6 points when the content grows 1.67×, so the increment is a
per-object and per-batch cost rather than a function of the dataset. **The product's measured memory is
bounded**, which is what the row's 100,000-entry point was commissioned to test.

### 11.3 Where the other 1,013 MB is, measured

A diagnostic, `tests/namespace_memory_probe.rs`, stages the driver's own setup and reads current RSS and
the counting allocator at each boundary. It registers no row and writes no receipt. On the same
`Declaration::LARGE` fixture:

| stage | RSS | live heap |
| --- | ---: | ---: |
| process start | 2,097,152 | 9,191 |
| after `PreparedTree::prepare` (101,000 bindings) | 47,874,048 | 28,888,212 |
| after `plan()` (100,000 files) | 57,491,456 | 32,088,212 |
| **content store alone (109,373 objects)** | **783,859,712** | **558,532,327** |
| real chain alone (4,221 objects) | — | 12,452,849 |
| **chain + the 25 prefix snapshots** | — | **176,799,943** |

`1,013,219,328` decomposes as:

| | bytes | share of baseline |
| --- | ---: | ---: |
| the content store — the declared 500 MB, held in RAM | ~728,465,408 | **72 %** |
| the driver's 25 per-batch prefix snapshots | **164,347,158** | **16 %** |
| the prepared tree and the plan | ~55,394,304 | 5 % |
| everything else, including the measured region | ~65 MB | 6 % |

The probe reads two stages that RSS alone cannot separate, because macOS does not return freed pages to
the OS — `heap_live` is the counting allocator's live bytes and is the honest axis for "what is still
held": **558,516,171 live heap bytes for 502,912,427 canonical bytes** (11.1 % of envelope and hash
overhead) after construction, and **12,452,785** for the chain alone against **176,799,943** with the 25
prefix snapshots. The probe's `rss` column is reported as a delta from its own start for the same reason:
its stages run after a first measurement in the same process, so their absolute RSS carries that
measurement's freed-but-resident pages, which is why the report's §11.3 RSS column comes from a run of
the probe with that first measurement absent.

The content store's **558,532,327 live heap bytes for 500,000,000 canonical bytes** is 11.7 % of
envelope and hash overhead over the payload — the store is not bloated, it is simply holding the whole
fixture because the driver constructs it in memory before the timer.

### 11.4 The prefix snapshots are pure waste, and the measurement would not notice their removal

`prefixes` keeps one `TreeStore` snapshot per batch, and `TreeStore::absorb` **copies**
(`providers.rs:100-107`). At 25 batches the snapshots retain **66,824 objects and 164,347,094 bytes** to
serve a chain whose final state is 4,221 objects and 12,452,849 bytes — **14.2× the chain alone** — and
each snapshot is a strict subset of the one after it, so 24 of the 25 are redundant.

They exist for one reason: `prefixes[index]` is the reader for batch `index` in both the timed pass
(`:984`) and the oracle replay (`:1100`), and a batch must be served the chain *as it stood before that
batch* — measured in round 19, where handing a batch the complete chain returned
`cycle check invalid work limit`. **That requirement is about which objects the reader may serve, not
about holding a separate copy of them.** A provider that holds the one complete chain plus the
`ObjectId` set of a batch's prefix serves exactly the same objects and fails the same way
(`PairProvider` returns `ContentError::MissingObject` for anything absent, `providers.rs:302`), at
12.5 MB instead of 176.8 MB.

**This is filed as a finding with a priced fix, not applied.** Removing it would change the row's
lifetime peak and its baseline, and therefore its binary — a fresh measurement round, not an edit.

### 11.5 The honest comparison with v0.1.6, which is the reason the question was asked

The reference harness does **not** hold its namespace in memory: it writes the scenario to disk through
`NamespaceContentStream` over a `NAMESPACE_SCRATCH_BYTES = 1 MiB` buffer
(`benchmark/fs-bench-pro/workload/main.rs:120`, `:936-948`). **Its fixture is bounded at 1 MiB; this
harness's is 500 MB.** On the one axis where the two can be compared directly — the fixture's resident
cost — v0.1.6's control is better by a factor of ~500, and that is a **harness** difference, not a
product one:

| | v0.1.6 reference harness | this row |
| --- | --- | --- |
| namespace generation | streamed to disk, 1 MiB scratch | 500 MB held in RAM |
| the measured region's own memory | not published per region | **37.5 MB** (measured here) |
| lifetime peak | container/cgroup figures only, workload-specific | 1.05 GB, 96 % pre-timer fixture |

**No claim is made here that the replacement product's memory is better or worse than v0.1.6's.** The
one comparable product-side figure is the measured region's 37.5 MB, which v0.1.6 does not publish for
its `init_namespace` case at all, so the pair cannot be formed. What can be said is narrower and is
measured: **the 1.05 GB is not the product's measured path, and the reference harness's equivalent
fixture does not hold 500 MB in RAM.**

### 11.6 The instrument did not move the row, and the row's own spread is now measurable

Adding the sampler is a harness change, so the first thing to check is whether it moved anything. Three
`pipeline-namespace-100000` runs, all `PASS` 13/13, all fifteen pins reproducing:

| run | `operation_work_ns` | `accept_span_ns` | `teardown_ns` | complete command | phase increment |
| --- | ---: | ---: | ---: | ---: | ---: |
| `ns21-C2` (before the instrument) | 3,673,602,750 | 4,162,787,625 | 489,184,875 | 6,571,717,125 | — |
| `ns21-D1` (with it) | — | — | — | — | 37,306,368 |
| `ns21-E1` (with it) | 3,549,393,833 | 4,033,601,291 | 484,207,458 | 6,446,615,375 | 37,519,360 |

No pinned counter moved in any of them, which is the check that matters: the sampler reads this process's
own residency on its own thread and touches no product state. **The declared figure moved 124,208,917 ns
(3.38 %) between C2 and E1 within one session and one binary's line of development**, `accept_span_ns`
moved 129,186,334 ns, and `teardown_ns` moved only 4,977,417 ns — so this movement is **inside** the
closure rather than across the row's declared boundary, which is a different shape of drift from the one
§4a and §5 record and is worth saying precisely rather than loosely. The phase increment, by
contrast, reproduced to **0.57 %** (37,306,368 against 37,519,360), which is what a per-object cost that
does not depend on the window looks like.

**§1's ratio of 3.99× is therefore a ratio between two single samples, not a ratio of two levels**, and
the honest statement of the new row's position is: *the 100,000-entry row's declared figure is 3.99× the
10,000-entry row's in this session's pair, with a within-session spread of 3.4 % on the larger row.*
