# Stages 3–4 retained-evidence audit (#168 / #169)

> **Status:** independent evidence audit, read-only. **Not** an acceptance verdict for
> the implementation, the criteria or the issues; it audits only the *retained raw
> evidence and the statistics quoted from it*.
> **Audited snapshot:** `91c3a0741fff64e8161d5c1b6e759f347ffbf757` (branch `main`),
> working tree clean apart from this review directory
> (`git status --porcelain` → only `?? docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-20260916T233008Z/`).
> **Method:** static reading of the retained files, read-only `git` history/seal checks,
> read-only `python3` re-derivation of every timing tree, `sha256`/`strings` on the
> recorded executables. **No build, no cargo, no test, no new measurement was run.**
> No product source was modified; no issue state, commit, tag or push was touched.

---

## 1. Scope

Seven retained Stages 3–4 evidence generations under
`docs/roadmap/0.1/0.1.7/evidence/` were audited file by file:

| # | Generation | Files | Bytes | Collection clock (host mtime, +0800) | Commit that added it |
| --- | --- | ---: | ---: | --- | --- |
| 1 | `stages-3-4-smoke-20260916T210931Z` | 13 | 10 859 | 2026-09-17T05:09:31 (+ README 05:10:56) | `64e3f9d6a` 05:12:19 |
| 2 | `stages-3-4-oracle-20260916T214846Z` | 9 | 15 120 | 2026-09-17T05:48:52 → 05:58:08 | `b49931570` 05:58:26 |
| 3 | `stages-3-4-oracle-20260916T222738Z-corrected` | 8 | 9 644 | 2026-09-17T06:28:41 → 06:29:31 | `01d9f70f3` 06:44:01 |
| 4 | `stages-3-4-oracle-20260917T034500Z` | 1 | 1 132 | 2026-09-17T07:14:43 | `c99192b16` 07:19:57 |
| 5 | `stages-3-4-fingerprint-collision-20260917T021500Z` | 2 | 559 | 2026-09-17T06:49:44 | `5a0fef716` 07:01:01 |
| 6 | `stages-3-4-timing-20260917T031000Z` | 110 | 5 010 162 | 2026-09-17T07:03:58 → 07:04:55 | `c99192b16` 07:19:57 |
| 7 | `stages-3-4-matched-c1-20260917T050000Z` | 8 | 5 945 | 2026-09-17T07:22:18 → 07:22:33 | `fba18606e` 07:22:52 |

(The review directory itself — `checks.log`, `examples/`, `loc-per-file.csv`, the
issue JSONs and the run scripts — is the **current parent-agent collection in flight**
(`examples/` mtimes 07:34–07:35, after this audit started) and is **out of scope**; it is
not retained Stages 3–4 evidence.)

### Round labels are not reliable UTC stamps

Three round labels match a real clock (label = UTC of the first receipt):
`20260916T210931Z` = 21:09:31Z ✓, `20260916T214846Z` = 21:48:46Z ✓ (first file 21:48:52Z),
`20260916T222738Z` = 22:27:38Z ✓ (first file 22:28:41Z). Three do **not**:

| Label | First receipt (UTC) | Offset |
| --- | --- | --- |
| `20260917T021500Z` (fingerprint) | 2026-09-16T22:49:44Z | label is **3 h 25 m after** the receipt |
| `20260917T031000Z` (timing) | 2026-09-16T23:03:58Z (own `README.txt:3`) | label is **4 h 06 m after** the receipt |
| `20260917T050000Z` (matched C1) | 2026-09-16T23:22:18Z | label is **5 h 38 m after** the receipt |

The timing round discloses this (`ledger.md:3-5`: "the label and the clock disagree …
both are recorded so nothing is inferred from the name"); the fingerprint and matched-C1
rounds do not. **Finding E-H1 (minor, evidence hygiene):** two of seven round labels
misstate collection time, and the disclosed explanation ("by the host's timezone") does
not fit the measured 4 h 06 m offset. No data is affected; citation of these directories
as timestamps is unsafe.

---

## 2. Generation 1 — smoke (`stages-3-4-smoke-20260916T210931Z`)

**What was run.** `measure_edits` (then-current build), seven arms, one sample each:
`README.md:12-20` gives the seven command lines and the outputs (c1/c2 small at
threshold 131 072; pipeline chunked 131 072; grow = small-to-large 1 048 576; shrink =
large-to-small 1 048 576; shrink2 = shrink repeated; batch 262 144).

**Receipts actually retained:** 12 `*.json` timing reports **only**. There is **no
`command.txt`, no `stdout.log`, no `stderr.log`, no tool identity, no commit/seal
record, no wall time and no binary sha256** anywhere in this generation
(`find` inventory; `README.md:22-25`). Every number in the README is therefore
"copied from the run output" with the run output absent.

**Claim-vs-raw check (the only cross-checkable rows):**

| README row | README value | Retained JSON | Verdict |
| --- | --- | --- | --- |
| c1 small `file.edit` | 2.551 ms (`README.md:31`) | `c1/c1-edit.json` root `file.edit` = 2 551 458 ns | MATCH |
| c2 small `storage.save` | 42.079 ms (`README.md:32`) | `c2/c2-save.json` = 42 079 000 ns | MATCH |
| pipeline small-to-large `edit.save` | **158.824 ms** (`README.md:34`) | `grow/pipeline-edit-save.json` = **193 977 292 ns** | **MISMATCH (defect E-D1)** |
| pipeline large-to-small `edit.save` / `verify.readback` | 5.970 ms / 54.928 ms + decomposition block (`README.md:35,41-53`) | `shrink/` = 6 060 625 ns / 54 762 208 ns; **`shrink2/` = 5 970 333 ns / 54 928 042 ns** | quoted numbers are `shrink2`'s, presented as the `shrink`/large-to-small row (conflation E-D2) |

The grow row's logical length (2 097 151 = `T-1` + `T` at `T` = 1 048 576) does match the
retained fixture, so the receipt is the right case; only the time disagrees. The
decomposition block reproduces `shrink2` exactly (`storage.finish` 468 916 ns,
`verify.readback` 54 928 042 ns, `content.traverse` 53 877 416 ns).

**Deleted artifacts.** `README.md:22-25` states the `store.sqlite` files were removed
from the evidence directory ("the `shrink` store was about 3.5 MB, the `grow` store about
2.1 MB"). Only the JSON reports survive, so the store-byte side of these arms cannot be
re-verified. `docs/general/benchmark_rules.md` / AGENTS §3.2 require receipts to be
append-only and discarded attempts to stay on disk; deletion is disclosed but is a
retention gap (**E-D3**).

**Identity/claims.** `README.md:6-10` claims one build for every run, yet `README.md:19`
describes `shrink2` as a repeat "after the harness change that made base preparation
untimed" — i.e. the harness changed mid-round. No binary identity exists for either side
of that change. The round is correctly self-labelled exploratory and
admission-ineligible (`README.md:3-4,60-65`); nothing here is performance evidence.

---

## 3. Generations 2–4 — the reference oracle (`stages-3-4-oracle-*`)

**What they are.** Reference-side example
`crates/layerfs-content/examples/rope_edit_oracle.rs` (314 lines) builds a base with the
reference constructor, applies the reference `FileMutationBatch` and prints a JSON fixture
(`rope_edit_oracle.rs:1-11`). The candidate side is an external test
`core/crates/layerfs-content/tests/edit_reference.rs` that reads the JSON and compares
root, partition and survivor identities (`stages-3-4-oracle-20260916T214846Z/README.md:8-13`).

**(d) Is it a separately sealed reference execution?**

* **Yes, a separate process from separate source**: the oracle binary lives in the
  reference workspace; the comparison runs in the candidate test target. Verified
  statically: `git diff --stat 44cf748486863ab7c21ca47e731bd88e2b9a7b4a HEAD -- crates/`
  lists **only two added example files** (`rope_edit_oracle.rs` 314, `rope_edit_timing.rs` 97);
  `git diff --stat 44cf748… HEAD -- crates/layerfs-content/src crates/layerfs-storage` is
  **empty**, and the APIs the example calls are exported by v0.1.6
  (`crates/layerfs-content/src/file/rope/mod.rs` @44cf748: `build_bytes`,
  `FileMutationBatch`). The reference product source really is the pinned commit.
* **But the seal is self-declared, not computed.** The `reference` field in every JSON is a
  **hard-coded literal**: `rope_edit_oracle.rs:283`
  `println!("  \"reference\": \"44cf748486863ab7c21ca47e731bd88e2b9a7b4a\",");`. It is not derived
  from a build, a tree hash or a version API.
* **No receipt-grade identity is retained for any oracle generation**: no command line, no
  stdout/stderr, no `tool-identities.txt`, no binary sha256, no build/toolchain record, no
  wall time. The only provenance is the directory `README.md` (generation 2 only — generations
  3 and 4 carry **no README, no ledger, no command.txt at all**).
* The generation-2 README's stated check, `git diff 44cf748 -- crates/layerfs-content` "is
  empty" (`README.md:5-6`), **no longer holds as written**: that exact path is now
  non-empty because the two example files are committed (411 added lines). The substantive
  claim (no reference *product* source changed) still holds via the narrower path above.
  **E-D4 (documentation, not data).**

**Supersession and non-rewriting (verified).** `git log --name-status` over all seven
evidence paths returns **only `A` (added) statuses — there is not a single `M`/`D`**:
no retained receipt was rewritten, relabelled or deleted after collection. The correction
created generation 3 as a **new directory** (added by `01d9f70f3`) and left generation 2 in
place. Diffing the six common cases shows generation 3 is **byte-identical to generation 2
except one added line** `"edits": [ … ]` per file, e.g. `root-collapse.json` gains
`"edits": [[0, 3435286, 0]]`; every `base_root`, `edited_root`, `edited_len`,
`nodes_created/read` and page partition is unchanged. Generation 3 adds two cases
(`half-partition-90-90`, `unequal-height-join`); generation 4 holds the ninth case
(`repartition-80-100`), added in its own directory "so the earlier receipts stay untouched"
(`stages-3-4-report.md:217-219`). **The supersession is honest.** What cannot be
established from the retained files is whether generation 3 was *re-run* or *copied and
extended*: identical values are consistent with either, and no run record exists.

**Reading of the earlier comparison.** Generation 2's `candidate-comparison.txt` is raw
cargo-test output; its first line records
`test the_oracle_fixtures_are_the_sealed_reference_revision ... ok` followed by the
deliberate failure of `the_candidate_reproduces_the_reference_root_and_partition`
(`edit_reference.rs:444` at the time), with per-case partitions such as
`join-80-100` oracle `2/94f0c9bd 96/8940b161 97/414e45e5` vs candidate
`2/1d1b0508 128/d19eaa75 65/cf0c738a`. That is a genuine candidate-side counterexample,
not a reimplementation of the reference.

---

## 4. Generation 5 — fingerprint collision (`stages-3-4-fingerprint-collision-20260917T021500Z`)

**Files:** `collision.json` (470 B) and `search.log` (89 B). No command, no tool identity,
no wall time, no budget record.

**(e) Is the pair real?** Independently re-derived here (read-only python):

* `collision.json:5-6` — `first_hex` and `second_hex` are each **73 bytes** (146 hex chars),
  the inode-value width (`core/crates/layerfs-content/src/object/inode_leaf.rs:14`,
  `INODE_VALUE_BYTES = 73`).
* The two values are **distinct** (byte comparison) ✓ and both are well-formed for the
  generator in `core/crates/layerfs-content/examples/fingerprint_collision_search.rs:25-38`:
  kind byte `01` (RegularFile, `inode_leaf.rs:60`), namespace ref count 1 at `bytes[1..9]`
  big-endian (`inode_leaf.rs:106`), the derived content roots satisfy the generator's
  rotate relations exactly (`rotl 29/43/11` of the state word), and the 32-byte metadata
  root is `a5 × 32` ✓.
* The fingerprint definition is `u64::from_le_bytes(ObjectId::for_bytes(value)[..8])`
  (`fingerprint_collision_search.rs:41-46`; identity is BLAKE3 over
  `b"layerfs/object/v2\0"` + canonical bytes, `object/id.rs:16,24-29`). **I could not
  recompute BLAKE3 in this environment** (no `blake3` python module, no `b3sum`), so the
  equality `fingerprint(first) == fingerprint(second) == 972d4e33fafff505` rests on the
  search tool's own print (`fingerprint_collision_search.rs:109`) and on
  `metadata_fingerprint_collision.rs:80-89` — **not** on any retained recomputation. The
  retained log does not contain a verification step either.
* Search bookkeeping is internally consistent: `search.log:2` "collision after 435008316
  hashes in 431 chains" = `collision.json:2-3` (`work_hashes 435008316`, `attempts 431`),
  and the progress line "chains 256 hashes 268618390 endpoints 256" fits the tool's
  `attempts % 256 == 0` print (`fingerprint_collision_search.rs:120-122`).
* **Fixture matches the retained pair (verified):** `metadata_fingerprint_collision.rs:25-37`
  concatenates exactly the two `*_hex` strings and the same fingerprint
  `972d4e33fafff505`; I re-concatenated them and compared byte-for-byte — identical, 73 B each.

**Identity gap.** There is no `tool-identities.txt` here. A release binary exists on disk
(`core/target/release/examples/fingerprint_collision_search`, mtime 06:47, sha256
`7bedb7790a8670077a3a7c74842dc958e14f0927dd7b07ac3f8fb0bd5118e553`) whose mtime precedes the
receipt by ~2 min, but **that hash is recorded nowhere in the evidence**, so the link
receipt→binary is unproven. The tool's documented usage is `--release`
(`fingerprint_collision_search.rs:10`).

---

## 5. Generation 6 — the timing round (`stages-3-4-timing-20260917T031000Z`)

### 5.1 What was actually run

* **Source:** commit `dfd54fd8e8ee5f633f9ff80de2967c4c25b6f2c2` — "tools: print the pooled
  policy and readback identities before collection" (`README.txt:1-2`, `ledger.md:7-8`).
* **Declarations before collection:** `stages-3-4-measurement-addendum.md` @ `c255dcfaf`
  (`README.txt:5`, `ledger.md:9-11`); that commit really is an ancestor
  (`git log --oneline` → `c255dcfaf` 2026-09-17 07:01:01). Ordering is honest.
* **Profile:** debug, `cargo build` without `--release` (`README.txt:4`, `ledger.md:13`).
* **Tools:** `core/crates/layerfs-storage/examples/measure_pooled.rs` and
  `measure_edits.rs`; identities `README.txt:6` claim `measure_pooled.rs @ dfd54fd8e`.
* **Toolchain:** **not recorded** (no build log, no `cargo` invocation, no `rustc -V`).
  The contract's `cargo +1.85.1` (`stages-3-4-verification.md:21`) is an assertion, not a receipt.
* **Worker setting:** `stages-3-4-verification.md:23` freezes "every run exports
  `LAYERFS_CONSTRUCTION_WORKERS=1`". **No retained `command.txt` contains that export, and the
  variable appears nowhere under `core/`** (grep: only
  `crates/layerfs-workspace/src/changes.rs:567,573`). The examples are single-process and
  build one object graph per run, but the frozen identity is **not evidenced. E-D5.**
* **Binary identity — verified.** `tool-identities.txt:1-2` records
  `d7e5c785…17f6  core/target/debug/examples/measure_pooled` and
  `f7cdc5ff…3818  core/target/debug/examples/measure_edits`; I re-hashed both files at the
  audited HEAD and got the **identical** digests. Receipt↔artifact correspondence holds.
* **Commands:** 21 arms, each own `command.txt` (binary + flags + fresh
  `--output <dir>/store`). E1a/b/c: `measure_pooled --leaves {24,128,512} --rows 100`;
  E2: `measure_edits --mode {c1,c2,pipeline} --case {small,chunked,small-to-large,large-to-small,batch}
  --threshold-bytes 131072`; E3a/c repeat E1a and the chunked pipeline arm with
  `--timing off`, E3b repeats the chunked pipeline arm with timing **on**. 3 + 15 + 3 = 21
  declared arms, 21 directories, 21 `stdout.log`, 21 empty `stderr.log` (0 bytes each:
  no error output at all), 28 timing JSONs.
* **Cache state:** declared per arm — fixture built in-process immediately before the timed
  scope, Store created inside the run, no warm-cache claim
  (`README.txt:7`, `ledger.md:14-18`, and the per-lane `exclusions:` lines).

### 5.2 Budgets (complete-command wall)

Every `stdout.log` ends with `wall_seconds:` and `exit_code: 0`. **Neither line is
printed by the product** (grep: no `wall_seconds`/`exit_code` in `core/**` or the reference
examples) — they are appended by an **unrecorded external wrapper**, so the wall is a
process wall time of unknown method, not a documented harness complete-command measure
(**E-D6**). All 21 walls:

| arm | wall s | arm | wall s | arm | wall s |
| --- | ---: | --- | ---: | --- | ---: |
| e1a-pooled-24 | 0.208 | e2-c2-batch | 0.079 | e2-pipeline-large-to-small | 0.079 |
| e1b-pooled-128 | 0.834 | e2-c2-chunked | 0.079 | e2-pipeline-small | 0.070 |
| e1c-pooled-512 | **3.155** | e2-c2-large-to-small | 0.078 | e2-pipeline-small-to-large | 0.103 |
| e2-c1-batch | 0.047 | e2-c2-small | 0.063 | e3a-pooled-24-off | 0.196 |
| e2-c1-chunked | 0.049 | e2-c2-small-to-large | 0.079 | e3b-pipeline-chunked-on | 0.871 |
| e2-c1-large-to-small | 0.049 | e2-pipeline-batch | 0.077 | e3c-pipeline-chunked-off | 0.092 |
| e2-c1-small | 0.068 | e2-pipeline-chunked | 0.090 | | |
| e2-c1-small-to-large | 0.051 | | | | |

* Longest = e1c at 3.155 s ≤ 15 s ✓; no arm overran; no 25 s exception was used ✓. The
  ledger's claims (`ledger.md:52-53` "0.047 s to 0.103 s", `ledger.md:75-78` "longest arm
  is E1c at 3.155 s") **match the raw stdout exactly**. The two E3 equivalence arms are
  inside budget too (0.871 / 0.092 s).
* All 21 arms exited 0; all declared arms are present; none was dropped or rerun into the
  same output directory (the tools refuse an existing `--output`:
  `measure_edits.rs:160-162`, and `create_new` on every JSON:
  `measure_edits.rs:289`).

### 5.3 (a) The actual timing trees

Re-derived from the raw JSON in every `<arm>/store/*.json` (inclusive spans; overlapping
spans are **not** subtractable). `ns`, one sample per case per arm. Sizes are the
`fixture:`/`stream:` lines of the same receipt.

**C1-only lane — `file.edit` (no database, pack, Store or file opened):**

| case | base → final B | root ns | principal recorded children (ns) | undecomposed inside `edit.op` |
| --- | --- | ---: | --- | ---: |
| small | 65 536 → 65 536, 512 B repl. | 10 548 500 | edit.op 10 537 542 · edit.base 37 583 · edit.compare 15 083 · edit.assemble 26 958 · content.encode 17 709 · content.emit 5 916 | **10 434 293 (99.0 %)** |
| chunked | 262 144 → 262 144, 4 096 B | 366 542 | edit.op 363 542 · edit.base 17 042 · edit.compare 49 209 (window 22 125) · base_read 1 750 · 3×edit.split 47 917/25 166/3 542 · content.chunk 125 125 · edit.finish 32 750 | 61 041 |
| small-to-large | 131 071 → 262 143, +131 072 | 10 323 375 | edit.op 10 316 209 · edit.base 30 583 · edit.compare 1 292 · content.chunk 10 164 625 · edit.finish 109 208 | 10 501 |
| large-to-small | 262 144 → 131 072, −131 072 | 159 417 | edit.op 155 917 · edit.base 19 000 · edit.compare 1 125 · base_read 1 666 · 3×edit.split 47 375/31 459/2 584 · edit.finish 23 709 | 8 999 |
| batch | 196 608 → 194 308, 3 edits | 424 125 | edit.op 420 417 (15 children: edit.base 20 916, edit.compare 1 750, 8×edit.split, 2×content.chunk 39 083/23 875, edit.finish 33 416 …) | 136 460 |

**C2-only lane — `storage.save` (Store creation + two supplied whole-file objects +
acknowledgement + one authenticated read, all inside the scope):**

| case | root ns | store.create | storage.begin | storage.finish | storage.read (decode + nested read) | undecomposed |
| --- | ---: | ---: | ---: | ---: | --- | ---: |
| small | 25 784 167 | 3 505 500 | 3 005 334 | 10 428 333 | 5 751 917 (808 125 + 4 566 125) | 3 089 709 (12.0 %) |
| chunked | 40 495 000 | 3 878 083 | 3 162 625 | 21 225 792 | 6 855 917 (812 333 + 5 630 417) | 5 368 542 (13.3 %) |
| small-to-large | 39 862 416 | 3 566 875 | 3 020 125 | 21 019 458 | 6 917 875 (806 833 + 5 681 458) | 5 334 250 (13.4 %) |
| large-to-small | 39 868 417 | 3 317 250 | 3 064 375 | 21 087 125 | 6 856 708 (767 167 + 5 783 916) | 5 539 251 (13.9 %) |
| batch | 41 100 334 | 4 455 416 | 3 127 041 | 21 517 792 | 6 887 541 (826 000 + 5 674 875) | 5 109 627 (12.4 %) |

Note the C2 lane does **not** apply the case's edit: it builds two whole-file objects from
`min(base_len, whole_file_raw_limit)` with a 256-byte patch
(`measure_edits.rs:390-437`), which is why four of five cases read back the same
131 094 canonical bytes and only the 65 536-byte `small` case reads 65 559 and takes the
PREFIX trial (`save: inserted 2 reused 0 prefix records 1 full records 1 trials 1`,
`e2-c2-small/stdout.log:7` vs `prefix records 0 full records 2 trials 0` in the others).

**Integrated (pipeline) lane — `edit.save` and separate `verify.readback`:**

| case | edit.save ns (storage.begin / file.edit / storage.finish) | verify.readback ns |
| --- | --- | ---: |
| small | 12 840 333 (2 903 209 / 5 342 916 / 4 487 042) | 5 664 500 |
| chunked | 11 110 209 (2 794 500 / 7 204 917 / 1 023 541) | 16 299 084 |
| small-to-large | 24 584 458 (2 774 792 / 17 246 083 / 3 780 334) | 13 948 791 |
| large-to-small | 6 621 333 (2 769 000 / 3 466 417 / 314 333) | 10 521 292 |
| batch | 8 214 875 (2 661 125 / 3 785 833 / 1 655 709) | 13 091 792 |

**E1 pooled lane:** `pooled.save` 159 509 917 / 786 711 125 / 3 105 519 375 ns for
24 / 128 / 512 leaves; `pooled.readback` 6 461 875 / 6 720 500 / 6 953 292 ns;
Store file 49 152 / 114 688 / 323 584 B. `pooled.save` per the receipt *includes*
Store creation and fixture construction/identity checks
(`exclusions:` line in each E1 stdout), which is why it dominates.

**(a) Verdict.** This is **single-sample (n = 1 per case per arm), debug-profile,
in-process-fixture, no-cold-contract evidence**. It is a wiring/correctness demonstration.
It is **admission-ineligible for every performance claim**: no release profile, no
repetition, no cache-state control (the page cache is "whatever the previous command
left"), and for the pooled lane the fixture construction is *inside* the timed scope.
The addendum says exactly this (`stages-3-4-measurement-addendum.md:8-25`,
`ledger.md:13-18,89-97`), and no report of this batch converts it into a speedup.

**Integrity of the printed trees (verified).** I re-parsed every rendered tree in the 21
`stdout.log` files and compared node-for-node (name, depth, value) with the 28 JSON
files: **all 21 arms agree**, with the printed value equal to the JSON nanosecond value
truncated to the printed unit's resolution. The ledger's E1 counters, wall times and
Store sizes (`ledger.md:24-28,44-46`) all appear verbatim in the raw stdout.

**Telemetry clipping — disclosed nowhere. E-D7 (the strongest evidence defect here).**

`e1c-pooled-512/stdout.log` prints `pooled.save  3.105s [incomplete]` and
`storage.accept  916ns [incomplete]`, followed by
`timings: INCOMPLETE - detail was clipped, not zero`. The JSON agrees:
`e1c-pooled-512/store/pooled-save.json` carries `"incomplete": true` on the root and on one
`storage.accept` — it is the **only** JSON of the 28 with that flag. That tree also has the
largest undecomposed share of any arm: root 3 105 519 375 ns against 1 320 950 951 ns in
its 1023 recorded children — **1 784 568 424 ns (57.5 %)** of the scope is not attributed to
any child, and part of the detail was clipped. The ledger (`ledger.md`), the verification
contract (`stages-3-4-verification.md:74-80`) and the self-review
(`stages-3-4-final-review.md:281-292`) report e1c's counters and its 3.155 s wall but
**never mention the INCOMPLETE marker**, while `stages-3-4-final-review.md:331` states "the
absolute node counts are in the receipts". Per the reviewer handoff (`§5`) "missing detail
is not zero time"; per AGENTS §1/§3.6 an INELIGIBLE/INCOMPLETE row must be reported, not
dropped.

### 5.4 (b) Timing on/off product-line equivalence — VERIFIED

I diffed the raw stdout of both pairs after removing the timing-tree blocks, the
`timings:`/`disabled:` lines and the wall line:

* `e1a-pooled-24` (on) vs `e3a-pooled-24-off`: **only** `timing: on|off`, the `store:` path
  (different `--output`) and `wall_seconds: 0.208|0.196` differ. Every product line —
  policy, `save-to-ack: inserted 24 reused 0 packs 48 commits 24`, pooled counters
  (123/2277, 3/21, 21 trials, 2 work-exceeded), readback verification, **both readback
  identities** (`9427ef09…` / `330bffb7…`), `retained: pool index 123 entries / 2952 bytes`,
  `store-file: 49152 bytes`, `catalogue: 24 group(s), 123 ordinal(s)` — is identical.
* `e3b-pipeline-chunked-on` vs `e3c-pipeline-chunked-off`: **only** `timing:` and
  `wall_seconds: 0.871|0.092` differ (no `store:` line in this tool). The edited root
  `ca6c30a4…`, length 262 144, `save: acknowledged true inserted 3 reused 0 packs 2 prefix
  records 0 full records 3` and the readback verdict are byte-identical.

The contract's claim (`stages-3-4-verification.md:80`, `ledger.md:65-71`) holds for **every
printed product line**. Caveat: this demonstrates *observable-output* equivalence only —
nothing in the receipts shows that the *work* performed is identical (no counter of
encodes/reads/copies), and the on-arm's node counts are not compared per node.

Secondary observation: `e3b` is a **second timed sample of the same case/arm** as
`e2-pipeline-chunked` (identical command line). Its wall is **0.871 s vs 0.090 s** (9.7×)
and its `edit.save` 12 635 750 ns vs 11 110 209 ns. It is disclosed as the E3 control
(`stages-3-4-measurement-addendum.md:52`), but neither the ledger nor the reports publish
that repeat's wall time next to the E2 range, and the 9.7× spread on an identical command
is itself evidence that these in-process debug numbers are not stable (**E-D8**, minor).

---

## 6. Generation 7 — matched C1 pair (`stages-3-4-matched-c1-20260917T050000Z`)

### 6.1 (c) What was run, and on which source

| | reference arm | candidate arm |
| --- | --- | --- |
| command (`command.txt`) | `./target/release/examples/rope_edit_timing` | `./core/target/release/examples/edit_timing_c1` |
| source (`ledger.md:4-7`) | v0.1.6 `44cf748486863ab7c21ca47e731bd88e2b9a7b4a` | `c99192b16` + the two example tools (uncommitted at collection) |
| binary sha256 (`tool-identities.txt:1-2`) | `63f711c3…a5baa` | `9600bb79…3a5f8` |
| re-hashed at audited HEAD | **identical** ✓ | **identical** ✓ |
| profile | release (`target/release/examples/`) | release (`core/target/release/examples/`) |
| wall / exit | 0.054 s / 0 | 0.049 s / 0 |

**Reference source really is v0.1.6 (verified):**
`git diff --stat 44cf748… HEAD -- crates/` = **only** the two added example files;
`git diff --stat 44cf748… HEAD -- crates/layerfs-content/src crates/layerfs-storage` = **empty**;
the reference binary's string table contains `layerfs-content` and **no** storage/telemetry
reference. **Candidate source really is the pinned product tree (verified):**
`git diff --stat c99192b16 HEAD -- core/crates/layerfs-content/src core/crates/layerfs-storage/src
core/crates/layerfs-telemetry/src core/crates/layerfs-storage/sql` = **empty** (only docs, the
manifests and this evidence changed); the candidate example is C1 + telemetry only
(`edit_timing_c1.rs:19-25`, and `core/crates/layerfs-content/Cargo.toml:9-11` depends on
`blake3` + `layerfs-telemetry` only). **Caveat:** the receipt records no *build* command,
no toolchain and no seal — the link binary↔source is established here only by the
self-recorded sha256 plus the tree comparison above; the tools were uncommitted at
collection (a dirty *tool* seal, clean *product* seal — disclosed at `ledger.md:4-7`).

### 6.2 The declared sample (raw stdout)

| field | reference (`reference-arm/stdout.log`) | candidate (`candidate-arm/stdout.log`) |
| --- | --- | --- |
| base bytes / logical length | 3 300 000 / 3 300 000 (l.2-3) | 3 300 000 / 3 300 000 (l.2-3) |
| edit | `replace [1650000, 1690000)` (l.4) | `replace [1650000, 1690000)` (l.4) |
| elapsed_ns | 224 875 (l.5) | 186 000 (l.5) |
| edited root | `b6dca354…c65207b` (l.6) | `b6dca354…c65207b` (l.6, identical) |
| objects_before | 176 (l.7) | 176 (l.7) |
| objects_written / bytes | 5 / **13 826** (l.8-9) | 7 / **47 357** (l.8-9) |
| nodes read / created | 11 / 3 (l.10-11) | 10 / 7 (l.10-11) |
| mapping_pages | — | 3 (l.12) |
| wall / exit | 0.054 / 0 (l.12-13) | 0.049 / 0 (l.13-14) |

The ledger's table (`ledger.md:37-46`) and the report's §7.6 numbers
(`stages-3-4-report.md:302-305`) reproduce these values exactly. **No mismatch found.**

### 6.3 Every observation, the interleaving, and n = 1

The ledger lists **three** observation rows (`ledger.md:54-58`): dev run 1 reference
351 666 / candidate 468 333 ns; dev run 2 reference `351 666 → 347 583` (tool re-run) /
candidate 365 333; declared sample 224 875 / 186 000. Stated ranges: reference
224 875–351 666, candidate 186 000–468 333 (`ledger.md:60-62`,
`stages-3-4-report.md:299-300`, `stages-3-4-final-review.md:341`).

* **Only the declared sample has retained receipts.** The dev-run observations have no
  `stdout.log`; they cannot be audited, and their existence is attested only by the ledger.
* The ledger's own count contradicts its table: it says "executed four times in total"
  (`ledger.md:50`) while the table enumerates six executions (two per row, with an extra
  reference re-run). **E-D9 (minor internal inconsistency).**
* The ordering genuinely reverses between the declared sample (candidate faster, ratio
  0.83) and dev run 1 (candidate slower, ratio 1.33). **n = 1 cannot support
  "existing-or-better" or "slower".** The ledger says so itself (`ledger.md:64-75`) and the
  report and self-review repeat it (`stages-3-4-report.md:296-305`,
  `stages-3-4-final-review.md:340-344`). The report's "four observations interleave" is
  consistent with its own ranges but is **not** supported by retained raw evidence for
  three of the four.
* The declared sample happens to be the **lowest** observation for both arms
  (ref 224 875 = min of {224 875, 351 666, 347 583}; cand 186 000 = min of {186 000, 468 333,
  365 333}). That is not a rule violation — one sample per arm is the rule, and all
  observations are printed — but a reader should know the declared pair is the favourable
  end of a 1.6× (reference) and 2.5× (candidate) spread.

### 6.4 The storage byte-accounting difference — boundaries are NOT aligned

* Reference: `objects_written = store.objects.len() - objects_before` and
  `objects_written_bytes` = **sum of object bytes now in the in-memory `BTreeMap` minus the
  sum before** (`rope_edit_timing.rs:66-67,80-85,93-94`) — an **insertion delta into a
  deduplicating map** (`put` keys by `ObjectId`, `rope_edit_timing.rs:35-39`).
* Candidate: `objects_written = collector.objects.len()` and bytes = **sum of the canonical
  lengths of the `FinalizedObject`s the consumer accepted**, counting every emission
  (`edit_timing_c1.rs:143-151`) — an **emission total**, not an insertion delta.
* The two windows also differ in shape: reference `replace(...)` + `finish()`
  (`rope_edit_timing.rs:70-79`), candidate one `apply_edits(...)`
  (`edit_timing_c1.rs:116-133`). Neither arm has an acknowledgement/storage boundary.
* **Fixture detail that undermines the comparison further (source-verified):** in *both*
  tools the replacement is `noise(40_000)` initialised from the **same seed** as the base
  (`rope_edit_timing.rs:42-52,57-59`; `edit_timing_c1.rs:76-91`), so the "changed" bytes are
  **byte-identical to the first 40 000 bytes of the base** that both arms already stored.
  Exact-reuse/dedup can therefore absorb the replacement in one accounting scheme and not
  the other; the raw receipts contain no per-object detail, so **neither 13 826 B nor
  47 357 B can be attributed** from the retained evidence.
* The report/ledger already concede this: "the reference's write boundary for that content
  was not established by reading its write path, so no storage conclusion is drawn"
  (`ledger.md:76-82`, `stages-3-4-report.md:301-305`). **Confirmed: the row is
  non-comparative; storage is unqualified.**

### 6.5 (c) Identity matching verdict

| dimension | status |
| --- | --- |
| source commit/seal | MATCHED in substance (v0.1.6 product source vs pinned candidate tree), but the receipt records no build/toolchain seal and the tools were uncommitted |
| binary identity | MATCHED per recorded sha256, re-hash confirmed; the sha256 is self-recorded, not signed |
| fixture | MATCHED (same base length, same edit tuple, both fixtures built in-process; `objects_before` = 176 on both sides is a further consistency signal) |
| profile | MATCHED (release on both sides) |
| cache state | MATCHED as declared (in-process fixture before the scope; no cold claim) — but neither arm can pay a storage read |
| worker count | not recorded in either arm |
| acknowledgement boundary | **NOT MATCHED / not established**: no acknowledgement in either arm, and the byte-accounting rule differs (insertion delta vs emission total) |
| operation shape | partially matched (one localized edit on the same bytes, in memory, returning a root); the reference uses a two-call batch API, the candidate a single `apply_edits` call |

**Row class: INCOMPLETE, not PASS.** Correctness ("same root") is established; latency,
storage and memory are not.

---

## 7. (f) Coverage of the frozen case list in `stages-3-4-verification.md` §2

Mapping every declared case to retained receipts (**exact inputs** required — a different
size is not the declared case):

| Family | Declared case | Receipt |
| --- | --- | --- |
| Payload FULL/DELTA | `whole-file-delta-win` (100 000 B base, 1 000 changed, explicit predecessor) | **NOT_RUN** — no receipt with these inputs. Closest: the C2 lane writes a base + dependent whole-file object with an explicit predecessor (`measure_edits.rs:406-427`) but at 65 536/131 072 B with a 256 B patch; only `e2-c2-small` takes the trial (prefix 1 / full 1 / trials 1). Delta win/lose is covered by the external tests `delta_payload`, which are not retained receipts. |
| Payload FULL/DELTA | `whole-file-delta-lose` (unrelated equal-size base/target) | **NOT_RUN** — no receipt. |
| Payload FULL/DELTA | `chunk-delta-win` (200 000 B chunk pair) | **NOT_RUN** — no receipt (the 262 144 B chunked edit is a localized overwrite, not a chunk-delta pair case). |
| Edit single | `small-overwrite` (base T/2, 512 B overwrite) | **RUN** — `e2-c1-small`, `e2-c2-small`, `e2-pipeline-small` (+ smoke `c1`/`c2`). |
| Edit single | `chunked-overwrite` (base 2T, 4 096 B overwrite) | **RUN** — `e2-*-chunked` (+ smoke `pipeline`). |
| Edit transitions | `grow` (T-1 + T inserted) | **RUN** — `e2-*-small-to-large`; plus smoke `grow` at T = 1 MiB. |
| Edit transitions | `shrink` (2T − 3T/4 deleted) | **RUN** — `e2-*-large-to-small`; plus smoke `shrink`/`shrink2` at T = 1 MiB. |
| Edit transitions | `empty` (full delete) | **NOT_RUN** — no receipt; only the external test `edit_single::complete_deletion_returns_the_empty_representation`. |
| Edit batch | `multi-edit` (three ordered edits) | **RUN** — `e2-*-batch`; plus smoke `batch` at T = 256 KiB. |
| No-op | `equal-replacement` / `empty-stream` | **NOT_RUN** — no receipt (every retained fixture changes bytes); only `edit_noop`. |
| Policy | `cutoff-128k` (T-1/T/T+1 boundary lengths) | **PARTIAL** — the whole timing round ran at T = 131 072, and its cases include base T-1 (`small-to-large`) and 2T, but no T-1/T/T+1 representation-boundary receipt exists. |
| Policy | `cutoff-256k` | **PARTIAL / admission-ineligible** — smoke `batch` only (T = 262 144), with no stdout, no identity and no store retained. |
| Policy | `cutoff-1m` | **PARTIAL / admission-ineligible** — smoke `grow`/`shrink`/`shrink2` (T = 1 048 576) only. |
| Policy | `singleton-1m` (1 048 575 B incompressible whole-file record, own pack, one database file) | **NOT_RUN** — no receipt; only `policy_capacity::a_larger_incompressible_whole_file_record_uses_the_singleton_lane`. |
| Index/footprint | `store-bytes` (each pipeline case: **total retained pack bytes and database bytes**) | **PARTIAL / NOT_RUN as declared** — the pipeline receipts report neither pack bytes nor database bytes (only `packs <n>` counts, `measure_edits.rs:514-522`). The E1 pooled receipts report the **database file size** (`store-file: 49152/114688/323584 bytes`, `measure_pooled.rs:306`) and the pool index bytes, never a pack-byte total. |

Additional facts relevant to this table: the external test targets named in
`stages-3-4-report.md:197-216` are **not** retained receipts — they are test names in a
report; their oracles were not exercised by this audit (no build/test was run).

**Declared-but-unrun reason (uniform):** the batch collected only three rounds
(smoke → oracle → timing/matched), and the addendum declares only the E1/E2/E3 arms
(`stages-3-4-measurement-addendum.md:44-52`). The remaining families were never given a
measurement round; the contract itself records "No v0.1.6 comparison campaign was
collected for this batch, and none is claimed"
(`stages-3-4-verification.md:65-90`).

---

## 8. (g) The two memory conclusions — no inputs exist

**There is no heap, RSS, allocation-ledger, phase-peak or lifetime-cgroup measurement
anywhere in the seven generations.** A case-insensitive scan of every `.log`, `.txt`,
`.json` and `.md` in the seven directories returns only *prose about* memory:

* `stages-3-4-timing-20260917T031000Z/ledger.md:80-87` — the "allocation ledger (as
  declared)": product-reported live capacity (`pool index 123/227/611 entries and
  2 952/5 448/14 664 bytes`) plus a *citation to external tests* (`edit_bounds`,
  `edit_localized`, `memory_bounds`) that this round did not re-measure.
* `stages-3-4-matched-c1-20260917T050000Z/ledger.md:83-86` — "**Memory: not measured.**"
* `stages-3-4-final-review.md:312-320` — heap/RSS attribution listed as **null with reason**.

So: **bounded-resource conclusion = declared capacities + external test accounting, no
retained measurement; memory-safety conclusion = code inspection + external tests, no
retained measurement.** The reviewer handoff's `§5` requirement for two separate memory
conclusions therefore **cannot be satisfied from retained evidence**; #168's
"simultaneous index/codec/SQL memory" gate is unmeasured — as the report concedes
(`stages-3-4-report.md:282-286`). The only retained *numbers* touching memory-adjacent
state are the three `retained: pool index N entries / M bytes` lines, which are a
Store-owned derivation size, not process memory.

---

## 9. (h) Post-collection modification check — CLEAN

* `git log --format=… --name-status` over all seven evidence paths returns **only `A`
  entries**: every file was added exactly once, by the single commit listed in §1, and
  **never modified, renamed or deleted afterwards**. No receipt was rewritten or
  re-labelled.
* File mtimes are all inside a single collection window per round and precede their
  commit by 1–20 minutes (e.g. timing stdout 07:03:58–07:04:18, ledger 07:04:55, commit
  07:19:57; matched-C1 stdout 07:22:18/19, ledger 07:22:33, commit 07:22:52).
* The one deletion in the record is the smoke round's `store.sqlite` removal, done before
  the round was committed and disclosed in `README.md:22-25` (see E-D3).
* Legacy round labels disagree with the clocks (§1) but no *file* was touched after
  collection.

---

## 10. Defect and gap register

**Demonstrated defects (claim vs raw evidence, or rule breach):**

| id | Severity | Finding |
| --- | --- | --- |
| E-D1 | medium | Smoke README quotes `edit.save` **158.824 ms** for `pipeline small-to-large` (`stages-3-4-smoke-20260916T210931Z/README.md:34`); the retained receipt `grow/pipeline-edit-save.json` says **193 977 292 ns**. The retained evidence contains no 158.824 ms anywhere; the run's raw stdout was not retained, so the number is unsupported. |
| E-D7 | medium | `e1c-pooled-512` timing tree is clipped (`[incomplete]` on the root and one `storage.accept`; `timings: INCOMPLETE - detail was clipped, not zero`; `"incomplete": true` in its JSON) and **no ledger, contract table or report line discloses it**, while 57.5 % of that scope is unattributed. |
| E-D3 | medium | Smoke round deleted all `store.sqlite` artifacts (disclosed, `README.md:22-25`), against the append-only/keep-everything rule (AGENTS §3.2, benchmark rules); the store-byte side of those arms is unrecoverable. |
| E-D2 | low | Smoke README's `large-to-small` row and decomposition block quote `shrink2`'s numbers while the table row is labelled `shrink`/`pipeline large-to-small` (`README.md:35,38-53` vs `shrink2/pipeline-edit-save.json` = 5 970 333 ns, `shrink/pipeline-edit-save.json` = 6 060 625 ns). |
| E-D9 | low | Matched-C1 ledger says "executed four times in total" (`ledger.md:50`) while its own table lists six executions; three of the four reported observations have **no retained receipt**. |
| E-D6 | low | The budget metric `wall_seconds` is printed by **no product tool**; it is appended by an unrecorded wrapper, so "complete-command wall time" is asserted, not evidenced, and its method is unknown. |
| E-D5 | low | The frozen identity "every run exports `LAYERFS_CONSTRUCTION_WORKERS=1`" (`stages-3-4-verification.md:23`) appears in **no** `command.txt` and the variable exists nowhere under `core/`; worker count is unrecorded for every arm. |
| E-D8 | low | `e3b` is an undisclosed-in-the-tables second timed sample of the same case as `e2-pipeline-chunked`, with a 9.7× wall spread (0.090 s vs 0.871 s) — a stability signal the ledger does not report alongside the E2 range. |
| E-D4 | low | Oracle generation-2 README's stated check `git diff 44cf748 -- crates/layerfs-content` "is empty" (`README.md:5-6`) is no longer true (two example files, +411 lines); the narrower product-source claim still holds. |
| E-H1 | low | Two round labels (`…T021500Z`, `…T050000Z`) and, with a disclosed but unexplained 4 h 06 m offset, `…T031000Z`, do not correspond to the collection clock. |

**Missing evidence (not defects, but blockers for admission):**

1. **Performance/storage/memory qualification against v0.1.6** — no qualifying receipt
   exists. The only reference comparison is n = 1 per arm with interleaving observations
   and non-aligned storage boundaries (§6); the timing round is single-sample debug
   in-process evidence with fixture construction inside `pooled.save` (§5).
2. **Memory inputs** — zero heap/RSS/allocation measurements in any generation (§8).
3. **Oracle/collision receipts** — no command line, binary identity, toolchain, wall time
   or stdout for generations 2–5; the reference revision is a hard-coded string
   (`rope_edit_oracle.rs:283`); the collision fingerprint cannot be recomputed from the
   retained files (no BLAKE3 available to the auditor, no verification step retained).
4. **Payload FULL/DELTA, no-op, empty-transition, singleton-1m and `store-bytes`
   (pack bytes) cases** — declared in the contract, never collected (§7).
5. **Cold-cache contrast** — never measured; the in-process fixture cannot support it
   (`ledger.md:95-97`; `stages-3-4-final-review.md:322-323`).
6. **Build/seal records** — no `cargo` invocation, `rustc` version or lockfile identity
   for any binary; the shared Cargo target is used but its reuse is not recorded per arm.

---

## 11. Qualification gap and the smallest missing case

**Qualification gap.** Stages 3–4 have *correctness* evidence (sealed reference oracle,
nine edit cases; the matched pair's identical root; the external-test suite named in the
report) and *wiring* evidence (21 timing arms with real counters and trees, identities
that re-hash, budgets respected, on/off product lines identical). They have **no
admission-grade performance, storage or memory evidence**. Specifically: every retained
performance row is (i) candidate-only and (ii) either n = 1 in-process debug
(admission-ineligible by the addendum's own declaration,
`stages-3-4-measurement-addendum.md:8-25`) or the matched pair's n = 1 with interleaving
observations and a non-comparable byte-accounting boundary (§6). A performance PASS, a
green test name or a matched root is **not** release admission; on this evidence the
latency, storage and memory gates of #168/#169, and #168's simultaneous index/codec/SQL
memory gate, are all **unqualified**.

**Smallest missing case (single answer).** For #169's gate: **one owner-approved,
repeated-sample campaign on the existing matched fixture** — release profile, the
3 300 000-byte base and the `replace [1 650 000, 1 690 000)` tuple, the same two examples,
**n ≥ 5 per arm, every sample reported**, plus a **single agreed byte-accounting rule**
(newly inserted store bytes as the reference's map measures it, on both sides) and a
non-duplicating replacement fixture (today the replacement content is byte-identical to
the base's first 40 000 bytes, which lets dedup mask payload writes on either side). For
#168, the equally small and independent case is **one instrumented pooled save at the
E1b shape with phase-local heap/RSS accounting**, because no v0.1.6 public pooled surface
exists to match against.

---

## 12. What this audit could NOT verify

* **No new measurement, build or test was run** (hard constraint): no timing, memory,
  storage or test result here is original; nothing was re-executed, and no external test
  oracle was inspected by executing it.
* **BLAKE3 fingerprints** — `blake3` is unavailable in this environment (no python
  module, no `b3sum`), so `972d4e33fafff505` could not be recomputed; the equal-fingerprint
  property rests on the search tool's own output and the candidate test's assertion.
* **The three non-declared matched-C1 observations** (351 666 / 468 333 / 365 333 /
  347 583 ns) have no receipts and could not be audited.
* **Whether oracle generation 3 was re-run** after the tool changed to print `edits`, or
  copied from generation 2 and extended by hand; the values are identical either way, but
  no run record exists to distinguish them.
* **Toolchain, dependency-lock and build-reuse identity** for every binary (asserted in
  source doc comments only).
* **Wall-time method** behind every `wall_seconds` line (external, unrecorded wrapper).
* **Whether the 431-chain collision search and the 21 arms stayed inside any budget** —
  the collision search records no wall time at all, and the timing arms' walls are
  wrapper-reported.
* **Roots/base identities of the timing-round arms cannot be cross-checked against the
  oracle or the matched pair**: the timing receipts print result/base roots but the
  `pooled` lane prints only readback identities; no arm prints the *base* root of the
  matched pair, so the two arms' identical 176-object base is a count, not a byte-for-byte
  comparison.
* **Anything about the implementation itself** (criteria PASS/FAIL, LOC, limits,
  simplifications) — deliberately out of scope for this evidence audit; the self-review
  (`stages-3-4-final-review.md`) is not independent and was not treated as evidence.
