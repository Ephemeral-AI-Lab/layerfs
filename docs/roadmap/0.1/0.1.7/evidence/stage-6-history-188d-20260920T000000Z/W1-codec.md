# W1 — the codec configuration: level, window, flags

> **Every number below is `diagnostic`.** None is admission evidence. The lane
> `history-stride10` is outside the 217-row registered set by construction.
>
> **The machine was NOT quiet for the lane runs.** Two sibling squads held or
> queued for `/tmp/lane.lock` throughout, and a W2 lane ran concurrently with
> both of my arms. The mandated quiet check is reproduced verbatim below; it was
> **not** empty. Every **byte** number is load-independent and stands. Every
> **wall-clock** number is reported as **NOT TAKEN** for any gate claim, with the
> raw reading and the contention evidence beside it. Process **CPU** (user+sys)
> is reported because it excludes descheduled time, but it is labelled contended.

Repo HEAD `66bce8378` + the shared uncommitted working tree. Lane
`history-stride10`, corpus `deepseek-history-data`,
`LAYERFS_CONSTRUCTION_WORKERS=1`, one sample per arm, no best-of.

---

## 0. What I own, and what moved under me

I own `core/crates/layerfs-storage/src/encoding/codec.rs` and nothing else. I
edited exactly that file. **No other file was touched.**

**The shared working tree moved between the parent's baseline and my run.** The
parent's "today's default" is **56,049,664 B**; the same command on the tree as
it stood at 03:28–03:41 measures **49,324,032 B**. Between those two points
other wave-1 squads wrote `cas/*.rs`, `encoding/delta/*.rs`, `policy.rs`,
`sql/schema.sql` and added `encoding/delta/candidates.rs`, a `content_signatures`
table and `file/mapping/predecessor.rs`. I verified that **no product file other
than `codec.rs` changed between my baseline binary and my rebuilt binary**
(`find core/crates -name '*.rs' -newermt ...`; the only product file newer than
my 03:28 baseline build is `codec.rs` at 03:39, and the rebuild recompiled only
`layerfs-storage` + the harness). The A→B delta below is therefore attributable
to `codec.rs` alone, but **the absolute baseline is not the parent's number**.

---

## 1. What I changed

`core/crates/layerfs-storage/src/encoding/codec.rs`, one file:

| # | change | before | after |
| --: | --- | --- | --- |
| 1 | payload compression level, both `compress` and `compress_prefix` | literal `3` | `PAYLOAD_LEVEL = 9` |
| 2 | the workspace guard's `ZSTD_getCParams` level, both call sites | literal `3` | `PAYLOAD_LEVEL` |
| 3 | `GROUP_LEVEL` (ordinary + pooled group bodies) | `1` | `19` |
| 4 | `ENCODE_WORKSPACE_BYTES` | `2 MiB` | `16 MiB` |
| 5 | module + item documentation | level 3 / level 1 / 2 MiB | the measured values |

**Item 4 is not cosmetic and it is not optional.** `CompressionWorkspace::new`
builds a **static** `ZSTD_CCtx` inside `ENCODE_WORKSPACE_BYTES`
(`codec.rs:201`, `ZSTD_initStaticCCtx`). A static context that does not fit
returns `ZSTD_error_memory_allocation`, which `encode_checked` maps to
`StorageError::Integrity("bounded Zstandard workspace unavailable")` — the save
fails. Measured, with the exact parameter sequence this module sets:

```
workspace 2097152  level 3   compress2 ok
workspace 2097152  level 9   ZSTD_error_memory_allocation (code 64)
```

So the prior "~2 constants, no format change" estimate was **one constant short**:
raising the level alone makes every save fail from level 9 upward. The third
thing — the guard's hardcoded `ZSTD_getCParams(3, ...)` — was silently
*under*-estimating the need and would have let the failure through the check
meant to catch it.

**And the product's own test suite caught the second-order version of this.**
With `ENCODE_WORKSPACE_BYTES = 4 MiB` the default 128 KiB cutoff worked
(measured, all 44,141 records) but `layerfs-storage --test codec_frames` failed:

```
test a_frame_beyond_the_profiles_window_is_refused ... FAILED
  panicked at tests/codec_frames.rs:22:38:
  a valid frame: Integrity("bounded Zstandard workspace unavailable")
```

The test builds a `StoragePolicy::new(1, 1_048_576, 8, 4)` — the **widest
construction cutoff the public API accepts** — and at level 9 that profile's
whole-file context needs **13,100,048 B**, not 4 MiB. The constant is a property
of the widest accepted policy, not of the record in hand, because one save
allocates it once and every role shares it. 16 MiB is the smallest power of two
above 13,100,048. **See §11 for what that costs and for the change that would
remove the cost.**

### 1.1 Diff summary and LOC

```
 core/crates/layerfs-storage/src/encoding/codec.rs | 73 ++++++++++++++++++-----
 1 file changed, 59 insertions(+), 14 deletions(-)
```

| file | production LOC before | after | delta | physical lines before → after |
| --- | --: | --: | --: | --- |
| `layerfs-storage/src/encoding/codec.rs` | 515 | **516** | **+1** | 663 → 708 |
| `core/crates/` subtotal | 20,109 | 20,116 | +7 *(only +1 is mine)* | — |

Counting method (reproducible):

```sh
python3 - <<'EOF'
import re, subprocess
def count(t):
    t = re.sub(r'/\*.*?\*/', '', t, flags=re.S)
    return sum(1 for l in t.splitlines()
               if l.strip() and not l.strip().startswith('//')
               and not l.strip().startswith('--'))
EOF
```

over `core/crates/**/src/*.rs` plus `core/crates/**/*.sql`. **My counter reads
20,109 for the tree today; the parent's stated subtotal is 19,766 — a 343-line
difference I did not reconcile** (scope of the SQL walk and of `layerfs-telemetry`
are the likely causes). The **delta** is method-insensitive: one file, and all but
one of the inserted lines are `///` documentation, which the rule excludes.
`codec.rs` is 708 physical lines, under the 999 ceiling.

**The subtotal row moves for reasons that are not mine.** The first reading was
20,109 and the last was 20,116, six minutes apart, because sibling squads are
editing other files in the same working tree. **The only delta attributable to
this change is `codec.rs`: +1 production line.**

### 1.2 Files I do NOT own that this change obliges someone to update

`core/AGENTS.md` requires the affected architecture document to move in the same
commit. **NOT EDITED — not mine:**

| file | line | says today | must say |
| --- | --: | --- | --- |
| `core/docs/architecture/13-physical-writing.md` | 28 | `zstd level 3 (payload) / level 1 (group)` | level 9 / level 19 |
| `core/docs/architecture/13-physical-writing.md` | 73 | `GROUP_LEVEL = 1, GROUP_WINDOW_LOG_MAX = 16` | `GROUP_LEVEL = 19` |
| `core/docs/architecture/13-physical-writing.md` | 77 | *"level 3, a role-specific window log…"* | level 9 |
| `core/docs/architecture/13-physical-writing.md` | 87 | `ENCODE_WORKSPACE_BYTES = 2 MiB` | 4 MiB |
| `core/docs/benchmark/fs-bench-pro-storage-content/c2-families.md` | 28 | `zstd level 3` | level 9 |

---

## 2. The instrument, and why its numbers are `measured` and not `modelled`

A scratch Rust tool (`/tmp/w1codec`, **not** product source, **not** in the
repository) links `zstd-sys =2.0.16` — the same zstd 1.5.7 the product pins — and
reproduces this module's FFI sequence **exactly**: `reset(session_and_parameters)`
→ the same six `ZSTD_CCtx_setParameter` calls → optional `ZSTD_CCtx_refPrefix` →
`ZSTD_compress2` → clear the prefix.

The Store is read by `shared/space.py` for the pack directory and lane split (no
second pack decoder was written); the record grammars are parsed from it and the
bases are taken from the records themselves.

### 2.1 Calibration — the instrument is byte-exact on **both** regimes

| population | records | recompressed at level 3, frames identical to the Store | residual on the lane total |
| --- | --: | --: | --: |
| whole-file FULL (tag 0) | 6,628 | **6,628 / 6,628** | 0 |
| whole-file PREFIX (tag 1) | 37,513 | **37,513 / 37,513** | 0 |
| native FULL | 448 | 448 / 448 | 0 |
| native PREFIX | 650 | 650 / 650 | 0 |

Whole-file lane: `18,294,359 + 18,547,871 = 36,842,230` frames `+ 44,141` tag
bytes `+ 32 × 37,513` base identities `= 38,086,787` `=`
`space.pack_directory().by_lane["whole-file"]`, **residual 0**. Native:
`3,926,763` frames `+ 5 × 1,098 + 32 × 650 + 4 × (96 + 1,098) = 3,957,829` `=`
`by_lane["native"]`, **residual 0**. Group lanes at level 1:
`1,022,795 + 2,019,419 = 3,042,214` `=` the Store's group bodies, **residual 0**.

**This retires the prior caveat.** `V3 §0.3` could only model PREFIX frames
("113/400 byte-identical, model 2.2 % smaller") because Python exposes no
`refPrefix`. This instrument reproduces **37,513 / 37,513** of them exactly. Every
number below is `measured`; nothing on this page is `est` except where marked.

### 2.2 Round trip at the shipped level

`roundtrip 9 18`: **44,141 / 44,141** records decode back to their own raw bytes,
**0 bad**. The product's read-path checks are satisfied too: at level 9 all 2,000
sampled based frames still carry `single_segment = 1` and
`windowSize = 120,364 < 1 << 18 = 262,144`.

---

## 3. The level sweep — measured, both regimes

Population: the **44,141** whole-file records of the arm-A Store,
**348,460,295** canonical bytes, **6,628** stored FULL and **37,513** stored
PREFIX. Regime A = compressed alone; regime B = compressed against **its actual
base** as a raw prefix.

| level | A: alone, base-less (6,628 rec) | A: alone, based (37,513 rec) | **B: prefix, based (37,513 rec)** | product (bodies, `min(full, prefix)`) | Δ frames vs L3 | codec CPU s | Δ CPU vs L3 s |
| --: | --: | --: | --: | --: | --: | --: | --: |
| 1 | 18,932,447 | 105,339,798 | 20,007,978 | — | **+853,638** | 1.151 | −0.250 |
| 3 | 18,294,359 | 101,477,038 | 18,547,871 | **38,086,787** | 0 | 1.401 | 0 |
| 5 | 17,673,884 | 97,714,732 | 16,920,142 | — | −2,248,204 | 2.996 | +1.596 |
| **9** | **17,429,995** | 95,963,936 | **16,079,041** | **34,753,589** | **−3,333,194** | **7.764** | **+6.364** |
| 12 | 17,151,793 | 94,629,660 | 15,848,340 | — | −3,842,097 | 25.072 | +23.671 |
| 15 | 16,964,669 | 93,368,901 | 15,568,049 | — | −4,309,512 | 37.405 | +36.004 |
| 19 | 16,912,060 | 93,024,856 | 15,500,549 | **33,657,157** | −4,429,621 | 68.131 | +66.731 |
| 22 | 16,911,572 | 93,020,213 | 15,491,974 | — | −4,438,684 | 88.538 | +87.138 |

`product (bodies)` is the `levels_min` mode: it applies the product's own
decision — a based record stores whichever **complete record** is smaller,
`1 + full_frame` or `33 + prefix_frame` — and reports body bytes. At level 3 it
returns **38,086,787**, the Store's whole-file lane to the byte, **residual 0**.

**CPU** is `CLOCK_PROCESS_CPUTIME_ID` around the compression loops only (decode,
file I/O and the store parse are outside the clock), on one thread. The column is
the product's actual work: for every object it computes the FULL frame, and for
every object with an admitted candidate it *additionally* computes the PREFIX
frame and keeps the smaller (`encoding/delta/select.rs:270` then `:302`).

**Marginal bytes per added CPU-second** (the shape that decides the budget):

| step | bytes | Δ CPU s | B per CPU-s |
| --- | --: | --: | --: |
| L3 → L5 | 2,248,204 | +1.596 | **1,408,000** |
| L5 → L9 | 1,084,842 | +4.768 | 227,500 |
| L9 → L12 | 508,903 | +17.307 | 29,400 |
| L12 → L15 | 467,415 | +12.333 | 37,900 |
| L15 → L19 | 120,109 | +30.727 | 3,900 |
| L19 → L22 | 9,063 | +20.407 | 444 |

### 3.1 Negative — level 1 is worse than level 3, and level 22 is dominated

* **L1 costs 853,638 B more than L3.** A "cheaper" codec is not a free lever;
  the level curve is not monotone in cost terms either (L12→L15 is a better
  marginal rate than L9→L12, because the strategy changes from `btlazy2` to
  `btopt`).
* **L22 buys 9,063 B over L19 for +20.4 s.** 444 B per CPU-second, 2.4 % of the
  L3→L5 rate. Refused.

### 3.2 The prior estimate, corrected

`V3 §5.2` estimated **~4,270,443 B** for level 3 → 19 on a 57,749,504 B Store
(9,702 base-less / 34,439 based), half of it modelled. Measured here, on the
arm-A population, the level 3 → 19 whole-file saving is **4,429,621 frames**
(−3,333,194 at level 9). The **based half is no longer a model**; the estimate's
sign was right and its magnitude was 3.7 % low.

---

## 4. The window log — `windowLog` is not binding, and 17 is the real minimum

### 4.1 Every frame is single-segment, so no window descriptor exists

Measured over all **44,141** whole-file frames of the arm-A Store:

```
Frame_Header_Descriptor bit 5 (Single_Segment_Flag):  1 for 44,141 / 44,141
Window_Descriptor byte present:                        0 / 44,141
```

A single-segment frame carries **no window descriptor at all**; the decoder's
window is the frame content size. `ZSTD_c_windowLog` therefore does not appear in
the frame. **Confirmed: `windowLog` is not binding on this lane today.** The
read-path check `header.windowSize > (1 << profile.window_log())`
(`codec.rs:465`) is then `raw_length > 262,144`, which the 131,071-byte raw limit
can never trip.

### 4.2 What a PREFIX frame actually needs

| `windowLog` | base-less frames | **prefix frames** | Δ prefix vs 18 |
| --: | --: | --: | --: |
| 16 | 17,435,588 | 16,967,545 | **+888,504** |
| 17 | 17,429,995 | **16,079,041** | 0 |
| 18 (today) | 17,429,995 | **16,079,041** | 0 |
| 20 | 17,429,995 | **16,079,041** | 0 |

and at level 19 on the same population: `windowLog` 17, 18, 19, 20 are
byte-identical (`16,912,060 / 15,500,549`).

**So: 17 is the minimum window log that loses nothing, and 18 is the provably
sufficient one.** The raw limit is 131,071 and the prefix limit is the same, so
`prefix + raw ≤ 262,142 ≤ 2^18`; 17 happens to suffice because no match on this
population reaches further back than 128 KiB. **I keep 18.** Lowering it to 17
would buy 128 KiB of encoder window buffer and zero bytes, and would make the
prefix reach depend on a measured accident rather than a bound.

**Negative:** on the base-less regime `windowLog` 16 is 5,593 B *better* than 17
(and on the earlier `b0` population 7,054 B better). The knob is not monotone and
is not a lever — it is worth 0.02 % on one regime and costs 5.5 % on the other.

---

## 5. Long-distance matching — a measured negative

`ZSTD_c_enableLongDistanceMatching = 1`, with and without `ldmHashLog` and
`ldmMinMatch` tuning, level 9, `windowLog` 18, arm-A population:

| `ldm` | `ldmHashLog` | `ldmMinMatch` | prefix frames | Δ vs off | base-less frames | Δ vs off | prefix CPU s |
| --- | --: | --: | --: | --: | --: | --: | --: |
| off | — | — | **16,079,041** | — | **17,429,995** | — | 2.721 |
| on | auto | auto | 16,440,634 | **+361,593** | 17,460,038 | +30,043 | 2.905 |
| on | 16 | auto | 16,575,114 | +496,073 | 17,504,762 | +74,767 | 5.781 |
| on | 20 | auto | 16,578,913 | +499,872 | 17,505,906 | +75,911 | 7.491 |
| on | auto | 32 | 16,579,626 | +500,585 | 17,515,877 | +85,882 | 2.840 |
| on | 20 | 32 | 16,974,337 | +895,296 | 17,694,429 | +264,434 | 6.599 |

**LDM loses on every arm and costs more CPU on every arm.** At level 19 it is
worth −1,683 B out of 33,657,157 (0.005 %) for +1.3 s — also refused.

It is not merely useless, it is **the wrong shape**: the prefix is at most
131,071 B, so the distance LDM exists to reach (≥ 2^24) cannot occur. It is
read-path *compatible* — measured, the frame header is unchanged
(`maxWindowSize 120,364`, `single_segment = 1` with LDM on) — so this is a clean
negative on bytes and CPU only.

---

## 6. The optimum, and the CPU budget I assumed

### 6.1 The budget, and why it is that number

**Stated budget: the payload and group codecs together may add at most +10 s of
single-threaded CPU on the `history-stride10` whole-file + native + group
populations.**

The reasoning, in order:

1. **The lane's complete command is already 40.0 s against a 15 s ceiling** —
   2.67×, a 25 s overrun. Any CPU this lever adds is charged to an account that
   is already overdrawn, so "level 19 because it is smaller" is not available
   without an owner ruling.
2. **+10 s is 25 % of the lane's current complete command** and takes it to
   ≈ 48.5 s, a 1.21× wall increase — the largest increase I am willing to
   defend as "arithmetically optimised, at speed" without a ruling.
3. **It is 7.1× the codec's present share.** The codec costs 1.40 s today
   (3.5 % of 40 s); the budget allows 11.4 s.
4. **The budget is not the binding constraint on the gate.** Even at +10 s the
   lever is 3.33 MB of the 6.73 MB gap on the parent's baseline; buying the last
   1.1 MB costs +60 s more. The trade is not close.

### 6.2 The optimum under that budget

| knob | value | added CPU s | bytes saved |
| --- | --- | --: | --: |
| payload level (whole-file, 44,141 records / 348 MB canonical) | **9** | +6.364 | 3,333,194 |
| payload level (native, 1,098 records / 22 MB canonical) | **9** | +0.213 | 281,518 |
| group level (ordinary + pooled, 1,809 groups / 7.9 MB decoded) | **19** | +1.012 | 262,222 |
| **total** | | **+7.589** | **3,876,934** |

**The single recommended number, for T2 and for every other squad: `PAYLOAD_LEVEL = 9`.**

`GROUP_LEVEL = 19` is the second number. The group population is **44× smaller**
than the payload population, which is why the *same* budget reaches a much higher
level there; level 22 adds 566 B for +0.075 s — less than one seventh of a
4,096-byte page — and is refused as immaterial.

### 6.3 What each further step costs, for the owner to rule

Whole-file + group terms are measured at every level; the native term is measured
at level 9 only and is included in the total column there.

| configuration | whole-file + group saved | native (L9 only) | total | added CPU s | verdict |
| --- | --: | --: | --: | --: | --- |
| payload 5, group 19 | 2,510,426 | — | ≥ 2,510,426 | +2.608 | inside budget, 65 % of the bytes |
| **payload 9, group 19** | **3,595,416** | **281,518** | **3,876,934** | **+7.589** | **recommended** |
| payload 12, group 19 | 4,104,319 | — | ≥ 4,104,319 | +24.896 | 2.5× the budget |
| payload 15, group 19 | 4,571,734 | — | ≥ 4,571,734 | +37.229 | 3.7× the budget |
| payload 19, group 19 | 4,691,843 | — | ≥ 4,691,843 | +67.956 | 6.8× the budget |
| payload 22, group 19 | 4,700,906 | — | ≥ 4,700,906 | +88.363 | dominated by 19 |

**If T2 turns out to require level 19, that is an owner ruling of +68 s, not a
free parameter.** The parent should not assume it: the T2 note's "closes the gate
only at L19" was computed on the *modelled* level curve that §2.1 has now
replaced. T2 must re-measure at level 9 before that claim is carried forward.

---

## 7. The `GROUP_LEVEL` sweep — V3 verified, one part refuted in magnitude

Recompressing the Store's own group bodies at the Store's own partition
(`stored` includes the 16-byte directory entry per group; subtracting it
reproduces the lane bodies exactly at level 1, **residual 0**):

| level | ordinary (72 groups) | Δ vs L1 | pooled (1,737 groups) | Δ vs L1 | **total** | **Δ vs L1** | CPU s | Δ CPU s |
| --: | --: | --: | --: | --: | --: | --: | --: | --: |
| 1 | 1,022,795 | 0 | 2,019,419 | 0 | **3,042,214** | 0 | 0.018 | 0 |
| 3 | 1,023,829 | **+1,034** | 2,015,771 | −3,648 | 3,039,600 | −2,614 | 0.019 | +0.001 |
| 5 | 1,003,398 | −19,397 | 1,975,620 | −43,799 | 2,979,018 | −63,196 | 0.035 | +0.017 |
| 9 | 966,287 | −56,508 | 1,974,291 | −45,128 | 2,940,578 | −101,636 | 0.069 | +0.051 |
| 12 | 962,278 | −60,517 | 1,918,212 | −101,207 | 2,880,490 | −161,724 | 0.235 | +0.217 |
| 15 | 898,628 | −124,167 | 1,910,503 | −108,916 | 2,809,131 | −233,083 | 0.513 | +0.495 |
| **19** | **875,547** | **−147,248** | **1,904,445** | **−114,974** | **2,779,992** | **−262,222** | **1.030** | **+1.012** |
| 22 | 874,981 | −147,814 | 1,904,445 | −114,974 | 2,779,426 | −262,788 | 1.094 | +1.076 |

**Verified.** V3 §5.4: ordinary level 19 = 875,547 (−147,248) — **exact**;
pooled level 19 = 1,904,848 (−114,571) — **mine is 1,904,445 (−114,974), 403 B
lower at every level**. My level-1 row equals the Store exactly
(1,022,795 + 2,019,419), so mine is anchored and V3's pooled column carries a
constant +403 B, most likely from not re-testing the 31 groups the Store kept
raw. The **total** −261,819 (V3) vs **−262,222** (measured) is the same 403 B.

**Verified, and it is a genuine trap:** level 3 is **worse** than level 1 on the
ordinary lane, by **+1,034 B**. The group window is capped at
`GROUP_WINDOW_LOG_MAX = 16` (`codec.rs:59`), so at these group sizes the level
knob is not monotone and cannot be inferred — it has to be measured, which is
what V3 said and what this run confirms.

---

## 8. The lane, end to end — bytes and wall time

Command, both arms, one lock hold, one sample each:

```sh
LAYERFS_CONSTRUCTION_WORKERS=1 /usr/bin/time -lp \
  core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content \
  --case history-stride10 --out <arm> --corpus deepseek-history-data
```

| | arm A (baseline: payload 3, group 1) | arm B (this change: payload 9, group 19) | arm B again, **shipped 16 MiB build** | Δ |
| --- | --: | --: | --: | --: |
| `sample.sqlite` apparent (`st_size`) | **49,324,032** | **45,432,832** | **45,432,832** | **−3,891,200** |
| allocated (`st_blocks`×512) | 49,418,240 | 45,498,368 | 45,498,368 | −3,919,872 |
| vs v0.1.6 (49,315,840) | 1.000166× | **0.921267×** | **0.921267×** | — |
| vs T1 target (47,048,435) | +2,275,597 | **−1,615,603** | **−1,615,603** | — |
| harness verdict | `history-stride10 PASS gates=2` | `PASS gates=2` | `PASS gates=2` | — |
| real | 58.88 s | 64.99 s | **46.46 s** | **NOT TAKEN** |
| user + sys (process CPU) | 51.54 s | 63.20 s | 43.03 s | **NOT TAKEN** |
| maximum resident set size | 236,634,112 B | 236,634,112 B | **252,723,200 B** | **+16,089,088** |

**The third column is the decisive one for the workspace change.** It is the same
arm re-run with the *shipped* 16 MiB constant after the `codec_frames` failure was
fixed, and it produces **45,432,832 B — byte-identical to the 4 MiB build**. The
workspace constant is output-neutral on the real lane, not merely in the
instrument. (It is also one sample, not a best-of: it is reported because the
*bytes* are the claim, and the two runs agree to the byte.)

**No wall or CPU claim is made, and the three rows are the proof.** The *same arm
B byte output* measured **64.99 s and 46.46 s of real time** on two runs — a 40 %
spread — and arm A, which does strictly less work, measured 58.88 s. The spread is
the machine, not the codec.

**The wall-clock numbers are NOT TAKEN.** The mandated quiet check was run
immediately before and after each arm and was **not** empty:

```
--- quiet check BEFORE arm A ---
 3:39  up  1:43, 1 user, load averages: 5.7x 5.6x 6.0x
   (no cargo/rustc/fs-bench line)          <- arm A began clean
--- quiet check BEFORE arm B ---
 3:41  up  1:43, 1 user, load averages: 5.77 5.69 6.03
yifanxu 22585 98.7 ... fs-bench-storage-content --case history-stride10 \
        --out .../LANE/w2-harness-index ...          <- A SIBLING SQUAD'S LANE
--- quiet check AFTER arm B ---
 3:42  up  1:44, 1 user, load averages: 5.16 5.52 5.94
```

A W2 lane started at 03:40 and ran concurrently with arm B; a W3 sequence was
queued on `/tmp/lane.lock` for the whole window; the lock I found at 03:25 was
**stale** (no lane process alive) and I removed it. Load average never fell below
5. **The cache state is declared as: warm** — the corpus had been read by every
arm of every squad for hours, so the corpus pages were resident for both arms
equally, and the two Stores are fresh outputs written inside the timed command.
No priming was performed for either arm.

**What survives:** the byte deltas (load-independent, and the two Stores differ
only by `codec.rs`), the PASS verdicts, and max RSS. **What does not:** any claim
about the lane's wall time. The parent's 40.0 s and my 58.88 s are not
comparable either — different tree, different machine load.

---

## 9. The arithmetic, with residuals

### 9.1 Arm A → arm B, apparent bytes

| term | arm A | arm B | Δ | source |
| --- | --: | --: | --: | --- |
| whole-file lane bodies | 38,086,787 | 34,761,166 | **−3,325,621** | `space.pack_directory` |
| native lane bodies | 3,957,829 | 3,676,262 | **−281,567** | `space.pack_directory` |
| ordinary lane bodies | 1,022,795 | 875,547 | **−147,248** | `space.pack_directory` |
| pooled-metadata lane bodies | 2,019,419 | 1,904,445 | **−114,974** | `space.pack_directory` |
| pack framing (header + directory) | 211,124 | 210,900 | **−224** | `space.pack_directory` |
| SQLite pages (`st_size` − pack blobs) | 4,026,078 | 4,004,512 | **−21,566** | `st_size` − `pack_bodies` |
| **apparent** | **49,324,032** | **45,432,832** | **−3,891,200** | `st_size` |

**residual 0.** The SQLite term decomposes exactly too (`dbstat`, both sides
sum to `st_size` − freelist, residual 0): `object_packs` page padding **+31,682**,
`objects` **−8,192**, freelist **−45,056** (30 → 19 pages), every other table
unchanged; `31,682 − 8,192 − 45,056 = −21,566` ✓.

### 9.2 The model against the lane — how much of the 3,891,200 B the sweep predicts

| term | modelled Δ | measured Δ | residual |
| --- | --: | --: | --: |
| whole-file (level 3 → 9, `min(full, prefix)` per record) | −3,333,194 | −3,325,621 | **+7,573** |
| native (level 3 → 9) | −281,518 | −281,567 | **−49** |
| ordinary (group level 1 → 19) | −147,248 | −147,248 | **0** |
| pooled (group level 1 → 19) | −114,974 | −114,974 | **0** |
| **subtotal** | **−3,876,934** | **−3,869,410** | **+7,524** |
| pack framing | not modelled | −224 | — |
| SQLite pages | not modelled | −21,566 | — |
| **total** | | **−3,891,200** | |

**Model residual +7,524 B = 0.19 % of the saving.** It is explained, not waved at:
the sweep prices each record against the base **arm A stored**, and arm A stored
**11 whole-file and 1 native record FULL**, so those records' base bytes do not
exist in the artifact the sweep reads; arm B gives them a base (+32 B of record
each) and their frames are not in the model at all. A second, smaller effect is
that the chain-encoded budget (`select.rs:291`) is charged in *encoded* bytes, so
a different level can accept a different base for a handful of records. 7,524 B
over 45,239 records is 0.17 B per record.

### 9.3 Where the saving lands on the parent's gap

```
parent's today's-default (declared only)        56,049,664
  this change, measured on the parent's population   −3,876,934  (modelled, §9.2)
                                                 -----------
  projected                                        52,172,730   1.05795x v0.1.6
  gate                                             49,315,840   still +2,856,890
```

```
this round's arm A (the tree as it actually stands)  49,324,032   1.000166x
  this change, measured end to end                    −3,891,200
                                                     -----------
  this round's arm B                                 45,432,832   0.921267x  PASS
  gate                                               49,315,840   −3,883,008
  T1 target                                          47,048,435   −1,615,603
```

**Both rows are true and they answer different questions.** The second is a
matched pair and closes the gate; the first is the codec's own contribution to
the parent's stated baseline and does not, because other squads' changes are not
mine to claim. **The codec alone is not the gate.**

---

## 10. Access cost — bytes per read, never seconds

The level changes **no** read-path structure: one record per whole-file group,
the same chain depth, the same `PACK_LIMIT`/`GROUP_TARGET` placement, the same
one-frame-per-record decode, the same decoded byte count. Only the frame is
smaller, so **bytes per read falls**:

| read | arm A (L3) | arm B (L9) | Δ |
| --- | --: | --: | --: |
| whole-file FULL, mean frame bytes/read | 2,760.7 | **2,629.8** | **−130.9 (−4.74 %)** |
| whole-file PREFIX, mean frame bytes/read | 494.4 | **428.6** | **−65.8 (−13.31 %)** |
| whole-file, decoded bytes/read | unchanged | unchanged | 0 |
| native FULL, mean frame bytes/read | 5,304.2 | **4,992.5** | −311.7 (−5.88 %) |
| native PREFIX, mean frame bytes/read | 2,384.7 | **2,167.1** | −217.6 (−9.13 %) |
| records per read / chain depth / group count | unchanged | unchanged | 0 |

A PREFIX read additionally reads its base chain, which the level does not touch.
Group bodies are read whole either way; their decoded size is unchanged
(7,877,849 B) and their stored size falls 3,042,214 → 2,779,992.

---

## 11. Bounded memory

| | before | after | delta | bound |
| --- | --: | --: | --: | --- |
| `ENCODE_WORKSPACE_BYTES` | 2,097,152 | **16,777,216** | **+14,680,064** | one per save (`cas/lifecycle.rs:56`), one save in flight per construction worker |
| `DECODE_WORKSPACE_BYTES` | 1,048,576 | 1,048,576 | 0 | one per read, materialised on first frame |
| process max RSS, lane (4 MiB build) | 236,634,112 | 236,634,112 | 0 | measured, both arms |
| process max RSS, lane (shipped 16 MiB build) | 236,634,112 | **252,723,200** | **+16,089,088** | measured; the constant delta is +14,680,064, the remaining 1,409,024 is page rounding and allocator slack |

**Why 16 MiB and not "it depends".** The workspace is sized by
`ZSTD_estimateCCtxSize_usingCParams`, which is now asked at the level actually
used. Measured, at the exact parameters this module sets:

| call | level | source | window log | estimate B | fits 16 MiB |
| --- | --: | --: | --: | --: | --- |
| whole-file, 128 KiB cutoff, alone | 9 | 131,071 | 18 | 3,662,864 | yes |
| whole-file, 128 KiB cutoff, with a prefix | 9 | 262,142 | 19 | 6,808,592 | yes |
| whole-file, **1 MiB cutoff**, with a prefix | 9 | 2,097,150 | 22 | **13,100,048** | yes |
| chunk | 9 | 32,768 | 16 | 1,057,808 | yes |
| group body | 19 | 65,536 | ≤ 16 | 2,883,726 | yes |
| *(level 19 payload — not taken)* | 19 | 131,071 | 18 | 5,505,166 | yes |

and the empirical check agrees with the arithmetic: a *static* context at level 9
over a 1,048,575-byte source with window log 20 **fails at 8 MiB and succeeds at
12 MiB**, and the 128 KiB cutoff **succeeds at 2 MiB without a prefix and fails at
2 MiB with one**.

**Output-neutral, verified:** a static context in 4,194,304 B and one in
16,777,216 B both reproduce the *dynamic* context's output byte for byte at level
9 over all 44,141 records — `17,429,995 / 16,079,041` with identical 235/6,039
exact-frame counts. The constant is therefore a **capacity** bound only; zstd
sizes its tables from the parameters, never from the buffer, once the buffer is
large enough. **The arm B lane measurement, taken with the 4 MiB build, stands
unchanged for the 16 MiB constant.**

**The memory is not free and it is measured: +16,089,088 B of peak RSS (+6.8 %)
for 3,891,200 B of bytes saved — a 4.1:1 memory-for-bytes trade on this lane.**
That ratio is what the profile-derived sizing below is for.

**What the 12 MiB of provision costs, and how to remove it.** The default 128 KiB
cutoff needs 4 MiB (its prefix case needs 6,808,592 B → 8 MiB by the conservative
estimate). The product ships only the default policy and the lane exercises only
the default policy, so **12 MiB of this constant is provision for a construction
cutoff nobody configures**. The arithmetically right form is to size the workspace
from the policy the save is running under — `CompressionWorkspace::new(capacities)`
at `cas/lifecycle.rs:56`, where `capacities` is already in scope — which would
charge 8 MiB at the default cutoff and 16 MiB only at the widest. **That is a
change in a file I do not own; it is reported here, not made.**

The encoder's level-dependent tables therefore live **inside the declared
workspace**, not in the allocator: the level's memory cost is a constant that is
charged once per save and cannot grow with the Store, the record or the chain.
The lane confirms it — max RSS is identical to the byte on both arms.

---

## 12. Negative results, retained

| # | hypothesis | how it was tested | result |
| --: | --- | --- | --- |
| N1 | a cheaper level is a free lever | level 1, both regimes | **+853,638 B** over level 3. Refuted. |
| N2 | level 22 is worth its CPU | level 22 vs 19 | **+9,063 B for +20.4 s**, 444 B/CPU-s. Refused. |
| N3 | LDM helps when the prefix is large | 6 arms at level 9, 6 at 19 | **loses 361,593 B at level 9**; −1,683 B at 19 for +1.3 s. Refuted. |
| N4 | LDM would break the read path | frame header parse with LDM on | **no** — header unchanged, `single_segment = 1`. The negative is bytes and CPU only. |
| N5 | `windowLog` is binding on FULL frames | all 44,141 descriptors | **not binding** — every frame is single-segment, no window descriptor exists. |
| N6 | a bigger window buys bytes | 16/17/18/19/20/27 | **no** — 17..27 identical; 16 *loses* 888,504 B on prefix and *gains* 5,593 B on FULL. Non-monotone, immaterial. |
| N7 | `GROUP_LEVEL = 3` is an improvement on 1 | ordinary lane, levels 1..22 | **refuted and confirmed the trap**: level 3 is **+1,034 B worse** than 1. |
| N8 | V3's pooled group column | level 1 vs the Store | **+403 B constant offset** against mine; my level-1 row equals the Store exactly. |
| N9 | the PREFIX half of the prior level estimate | 37,513 PREFIX frames recompressed | **the model is unnecessary** — 37,513/37,513 byte-identical. The prior `est` becomes `measured`. |
| N10 | raising the level is "one constant" | static context at 2 MiB, level 9 | **refuted** — `ZSTD_error_memory_allocation`; a third change (the workspace) is required. |
| N11 | 4 MiB is enough for the level-9 workspace | `codec_frames`, then `fit` at 2/4/6/8/12/16 MiB | **refuted** — the widest accepted policy (1 MiB cutoff) needs **13,100,048 B**; 4 MiB made the product's own test suite red. The constant is 16 MiB. |
| N12 | a bigger workspace changes the frames | static context at 4 MiB vs 16 MiB vs dynamic | **no** — all three produce `17,429,995 / 16,079,041` with identical exact-frame counts. Capacity only. |
| N13 | the guard's estimate is the real need | `fit` at the default profile | **the estimate is conservative**: the 128 KiB cutoff compresses without a prefix in 2 MiB although the estimate says 3,662,864 B. Conservative is the safe direction and is left alone. |

---

## 13. Checks run, and checks NOT run

| check | result |
| --- | --- |
| `python3 core/tools/check_product_boundary.py` | **PASS** — 121 production Rust/SQL files scanned |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | **OK** — 6 tests |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked` | first run **RED**: `codec_frames::a_frame_beyond_the_profiles_window_is_refused` failed with `Integrity("bounded Zstandard workspace unavailable")` — a real regression my change introduced, caught by the product's own tests. After the workspace correction: **`EXIT=0`, 486 passed, 0 failed** across every target. |
| `cargo +1.85.1 test ... -p layerfs-storage --test codec_frames` | **7 passed, 0 failed** after the fix |
| `cargo +1.85.1 build --release --manifest-path core/benchmark/.../Cargo.toml --locked` | **OK**, warnings only, and they are pre-existing in `ops/history.rs` |
| `history-stride10` end to end, both arms | **PASS gates=2** on both |
| instrument round trip at level 9 | **44,141 / 44,141**, 0 bad |

**NOT run, with the reason:**

| check | reason |
| --- | --- |
| `cargo clippy -- -D warnings` | not run. The change is 3 constants, 4 literal substitutions and documentation; the machine was at load 5–8 with two sibling squads building throughout, and I chose not to add a third full-workspace build. **Gap.** |
| `cargo fmt --check` | not run, same reason. **Gap.** |
| the 217-row registered lane (`--lane full`) | **not run.** This is a *product* codec change, so it alters the stored bytes of every registered case that writes whole-file or chunk payloads. The guardrail forbids touching the lane; it does not make this re-run optional. **The parent must schedule it before this change is called safe.** |
| `--lane smoke` | not run, same reason. |
| `history-stride1` | **not run and not optimised** — the guardrail. |
| `history-stride3` | not run. |
| the two retained negative harness arms (`ORDERED_PREDECESSORS=1`, `FULL_PRODUCER=1`) | not run; this change is orthogonal to predecessor selection. |
| `LAYERFS_HISTORY_SIMILARITY_CANDIDATES=1` | not run; the sweep and the matched pair both use today's default (declared only). |
| `VACUUM`, L4 cursor, L5 objects grammar | not mine; untouched. |
| a commit | **not made.** The parent sequences commits; the tree carries several squads' work. |

**One caveat the parent must carry:** because the tree moved (§0), my arm A is
**not** the 56,049,664 B baseline. The matched pair is internally consistent, but
the end-to-end −3,891,200 B is measured against *this round's* tree, and the
−3,876,934 B projection onto the parent's baseline is `computed` from the sweep,
not measured on that Store.

---

## 14. Reproduction

```sh
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs
S=docs/roadmap/0.1/0.1.7/evidence/stage-6-history-188d-20260920T000000Z/W1

# build the scratch instrument (outside the repo; zstd-sys 2.0.16 from the cache)
cd /tmp/w1codec && cargo build --release --offline

# pull the lanes out of a Store (read-only; the pack directory is shared/space.py)
python3 $S/extract.py /tmp/w1/armA/sample.sqlite /tmp/w1/armA
python3 $S/native.py  /tmp/w1/armA/sample.sqlite /tmp/w1/armA

# the sweeps
/tmp/w1codec/target/release/w1codec levels       /tmp/w1/armA/records.bin 1,3,5,9,12,15,19,22 18
/tmp/w1codec/target/release/w1codec levels_min   /tmp/w1/armA/records.bin 3,9,19 18
/tmp/w1codec/target/release/w1codec levels       /tmp/w1/armA/native.bin  1,3,5,9,12,15,19 18
/tmp/w1codec/target/release/w1codec window       /tmp/w1/armA/records.bin 16,17,18,20 9
/tmp/w1codec/target/release/w1codec ldm          /tmp/w1/armA/records.bin 9 18
/tmp/w1codec/target/release/w1codec groups       /tmp/w1/armA/records.bin 1,3,5,9,12,15,19,22 x /tmp/w1/armA/groups_ordinary.bin
/tmp/w1codec/target/release/w1codec groups       /tmp/w1/armA/records.bin 1,3,5,9,12,15,19,22 x /tmp/w1/armA/groups_pooled-metadata.bin
/tmp/w1codec/target/release/w1codec estimate     x 5,9,19 65536,262142,131071
/tmp/w1codec/target/release/w1codec levels_static /tmp/w1/armA/records.bin 9 18 4194304
/tmp/w1codec/target/release/w1codec static       /tmp/w1/armA/records.bin 3,9,19 2097152
/tmp/w1codec/target/release/w1codec roundtrip    /tmp/w1/armA/records.bin 9 18

# the lane, one lock hold, one sample per arm (see /tmp/w1/lane.sh)
/tmp/w1/lane.sh
```

The scratch tool and its raw outputs live in `/tmp/w1/` and `/tmp/w1codec/`
(not committed). The two analysis scripts are retained beside this file:
`W1/extract.py` and `W1/native.py`.

---

## 15. Bottom line

1. **`PAYLOAD_LEVEL = 9`, `GROUP_LEVEL = 19`, `windowLog` unchanged at 18, frame
   flags unchanged.** Measured: **−3,891,200 B** end to end (49,324,032 →
   45,432,832, reproduced byte-identically by the shipped build), **+7.6 s** of
   codec CPU, **+16,089,088 B** of measured peak RSS (declared constant delta
   +14,680,064), **0** bytes of extra read amplification — bytes per read *fall*
   4.7–13.3 %. Wall time: **NOT TAKEN**, the machine was not quiet.
2. **The single number for T2 is 9.** Level 19 is 1,096,427 B better and
   **+66.7 s** worse; the owner must rule that, it is not a free parameter.
3. **`windowLog` is not a lever and 18 is already correct** — every frame is
   single-segment, no window descriptor is written, and 17..27 are byte-identical.
4. **LDM is a measured negative** at every arm and every level.
5. **`GROUP_LEVEL = 3` is worse than 1 on the ordinary lane**, verified; the
   optimum is 19 and 22 is immaterial.
6. **The change is not complete on its own:** the workspace constant and the
   guard's level had to move with the level (and the constant had to cover the
   widest accepted policy, which the product's own tests proved), and two
   architecture documents plus the 217-row lane re-run are owed by someone who
   owns them.
7. **The one change I need from a file I do not own:**
   `CompressionWorkspace::new(capacities)` at `cas/lifecycle.rs:56`, so the
   encode workspace is sized from the policy the save runs under. That turns the
   default cutoff's charge from 16 MiB into 8 MiB and removes 12 MiB of provision
   for a policy nobody configures. The lane's byte result is identical either
   way.
