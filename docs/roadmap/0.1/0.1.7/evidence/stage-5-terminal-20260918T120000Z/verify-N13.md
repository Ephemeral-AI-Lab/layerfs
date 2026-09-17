# N-13 verification: zstd FFI isolated behind an audited module boundary

Repo: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, frozen at commit
`99743b2cf3a869b7d8897a1f16b82d742aeedc40` (verified via `git rev-parse HEAD`;
working tree clean per `git status --porcelain`). Claim attributed to round-4
commit `6c00e0f53172ef9ff7645e745a7f2d045461d0ac` ("fix(core): distinguish
provider failures, close the pragma set, verify profiles"), confirmed to exist
and to touch exactly the files in question (codec.rs, encoding/mod.rs, lib.rs,
check_product_boundary.py, its self-tests, physical-encoding-and-packing.md).

## Verdict

**FAIL — narrowly.** The isolation machinery verifies completely: one audited
module, crate-wide deny with an accurate E0453 justification, forbid in both
siblings, a guard that machine-rejects everything else, self-tests, and a design
note that claims no memory-safety proof. One conjunct of the claim is false:
the module documentation's FFI inventory is **not complete**. It lists 17
`zstd_sys` entry points, but the module calls 19; `ZSTD_compressBound` and
`ZSTD_estimateCCtxSize_usingCParams` are imported and called inside unsafe
blocks yet appear in neither the module-doc inventory nor the design note's
inventory, both of which assert completeness.

## Commands run (all from the repository root unless noted)

| Command | Exit | Result |
| --- | --- | --- |
| `git rev-parse HEAD` | 0 | `99743b2cff2470e6634874d7ee14b9d37d0ba16e` |
| `git status --porcelain` | 0 | clean (no output) |
| `python3 core/tools/check_product_boundary.py` | 0 | `PASS: scanned 116 production Rust/SQL files; semantic review still required` |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 | `Ran 6 tests ... OK` |
| `grep -rn "unsafe" core/crates/layerfs-storage/src/` | 0 | hits only in lib.rs (attr+comment), encoding/mod.rs (doc+attr), encoding/codec.rs (code) |
| `grep -rn "unsafe" core/crates/layerfs-content/src core/crates/layerfs-telemetry/src` | 0 | only the two `#![forbid(unsafe_code)]` attr lines |
| `grep -rn "zstd_sys" core/crates/layerfs-storage/src/` (excluding codec.rs) | 1 | no other file references zstd_sys |
| `git show --stat 6c00e0f53` | 0 | touches exactly the claimed files |
| rustc 1.85.1 scratch tests (in `/tmp/n13`, see Falsification) | see below | E0453 reproduced; include! hole compiles clean |

No repository file was modified; scratch artifacts live under `/tmp/n13` and
`/tmp/fake_core`. This file is the only write inside the repository.

## Findings

### 1. lib.rs deny + E0453 comment — VERIFIED

`core/crates/layerfs-storage/src/lib.rs:19-28`:

> 19: `#![deny(unsafe_op_in_unsafe_fn)]`
> 20-27: "// `layerfs-content` and `layerfs-telemetry` are
> `forbid(unsafe_code)`. This crate cannot be: the pinned zstd codec needs the C
> FFI, and a lint that is `forbid`ed cannot be allowed back on for one module
> (E0453). `unsafe` is therefore denied crate-wide and allowed on exactly one
> audited module, `encoding::codec` ... and the product boundary guard rejects
> `unsafe` anywhere else in this crate. The deviation from the siblings' literal
> `forbid` is recorded as a design note in `physical-encoding-and-packing.md`."
> 28: `#![deny(unsafe_code)]`

**E0453 empirically reproduced** on the pinned toolchain (`rustup run 1.85.1
rustc --crate-type lib`, scratch files in `/tmp/n13`): a crate with
`#![forbid(unsafe_code)]` plus `#[allow(unsafe_code)] mod` fails with
`error[E0453]: allow(unsafe_code) incompatible with previous forbid` (exit 1);
the same file with `deny` instead of `forbid` compiles clean (exit 0). The
comment's justification is factually correct, and deny+allow is the minimal
working structure.

### 2. Exactly one allowed module — VERIFIED; inventory — FAILED

`core/crates/layerfs-storage/src/encoding/mod.rs:11-13`:

> 11: `/// The audited zstd FFI boundary; the only module where `unsafe` is allowed.`
> 12: `#[allow(unsafe_code)]`
> 13: `pub mod codec;`

with the boundary doc at `encoding/mod.rs:5-9` ("`codec` is the crate's **only
audited `unsafe` boundary** ... `unsafe` is denied by `lib.rs` and rejected
again by the product boundary guard"). `grep -rn "allow(unsafe_code)"
core/crates/` finds exactly one hit: `encoding/mod.rs:12`.

Unsafe code exists in exactly one file, `encoding/codec.rs`: 11 unsafe blocks
(codec.rs:125, 133, 177, 207, 281, 346, 422, 460, 518, 576, 623) plus one
`unsafe fn parse_frame_header` (codec.rs:615) = **12 unsafe items**, matching
the design note's "twelve unsafe items" (physical-encoding-and-packing.md:273).
Every one of the 12 carries a `// SAFETY:` argument at codec.rs:124, 132,
175-176, 204-206, 278-279, 345, 421, 457-459, 517, 575, 614, 621-622.

**Inventory mismatch (the FAIL).** The module doc claims "The complete FFI
inventory" (codec.rs:6-13) and lists 17 entry points: `ZSTD_isError`,
`ZSTD_getErrorCode`, `ZSTD_initStaticCCtx`, `ZSTD_initStaticDCtx`,
`ZSTD_CCtx_reset`, `ZSTD_CCtx_setParameter`, `ZSTD_CCtx_setCParams`,
`ZSTD_CCtx_setFParams`, `ZSTD_CCtx_refPrefix`, `ZSTD_compress2`,
`ZSTD_DCtx_reset`, `ZSTD_DCtx_setParameter`, `ZSTD_DCtx_refPrefix`,
`ZSTD_decompressDCtx`, `ZSTD_getCParams`, `ZSTD_getFrameHeader`,
`ZSTD_findFrameCompressedSize`. All 17 are indeed imported (codec.rs:35-43) and
called at the listed sites. But two further imported entry points are called
inside unsafe blocks and are absent from the inventory:

- `ZSTD_compressBound` — imported codec.rs:39; called in unsafe blocks at
  codec.rs:227, 301, 362.
- `ZSTD_estimateCCtxSize_usingCParams` — imported codec.rs:40; called in unsafe
  blocks at codec.rs:222, 296.

`grep` confirms neither name appears anywhere in the module doc (codec.rs:1-29)
or in the design note (physical-encoding-and-packing.md). Both are
numeric-computation helpers (worst-case bound; workspace-size estimate), i.e.
the same benign class as `ZSTD_getCParams`, which *is* listed — so the omission
is not a principled exclusion. The design note's parallel claim "every
`zstd_sys` entry point the product calls" (physical-encoding-and-packing.md:262)
and its inventory (lines 273-282) inherit the same 2-of-19 gap.

### 3. Siblings are forbid — VERIFIED

`core/crates/layerfs-content/src/lib.rs:16`: `#![forbid(unsafe_code)]`.
`core/crates/layerfs-telemetry/src/lib.rs:13`: `#![forbid(unsafe_code)]`.
A tree-wide grep shows these are the only `unsafe` occurrences in either
crate's src: no unsafe code, only the attribute (the lint name `unsafe_code`
contains no bare `unsafe` word).

### 4. The guard and its self-tests — VERIFIED

Rule in `core/tools/check_product_boundary.py`:

- :14 `UNSAFE = re.compile(r"\bunsafe\b")`; :20-22
  `UNSAFE_AUDITED_MODULE = {"layerfs-storage": "src/encoding/codec.rs"}`; :23
  `UNSAFE_FREE_CRATES = ("layerfs-content", "layerfs-telemetry")`; :24-28
  `UNSAFE_ROOT_ATTR` mapping storage→`#![deny(unsafe_code)]`, content→forbid,
  telemetry→forbid.
- :40-53 `unsafe_violations`: strips `//` comments (:47), searches the bare word
  (:48), and rejects any hit unless the file is storage's audited module
  (:49-50, "unsafe outside the audited module boundary; see core/AGENTS.md");
  :51-52 requires the exact attr string in each crate's `src/lib.rs`
  ("crate root must declare ...").
- :31-37 `crate_name` derives the crate from the path segment after `crates`;
  :44 skips any crate not in the three-name table.

Self-tests in `core/tools/test_check_product_boundary.py:43-77`
(`test_unsafe_boundary`): the audited module accepts unsafe (:50, :52); the
lint attribute names never trip the scan (:51); unsafe is rejected in
storage-elsewhere, content and telemetry — as a block, an `unsafe fn`, or a
pointer read (:54-65); `unsafe` spelled only in a comment is not a violation
(:59, :62-63); each crate root must declare its documented attr (:67-74, tested
in both directions); and — explicitly documented — "Paths outside the three
known crates are not judged by this rule" (:76-77).

Live run: guard exit 0 over 116 files; `unittest discover` exit 0, 6 tests OK.

### 5. Design note — VERIFIED (except the inherited inventory gap)

`docs/roadmap/0.1/0.1.7/component-decoupling/physical-encoding-and-packing.md:249-284`,
"Accepted design note (2026-09-18): the zstd FFI boundary and the `forbid`
deviation". It states the E0453 reason (:254-256, "allow cannot override
forbid; rustc rejects the combination with E0453 - verified on `+1.85.1`" —
which I reproduced independently); the deny+allow+guard structure (:260-266);
the audited surface as twelve unsafe items with the entry-point inventory
(:273-282); and explicitly disclaims proof (:283-284): "No memory-safety proof
is claimed: this is an audited boundary, not a proof." No memory-safety proof
claim appears. Only the inventory's completeness (:262, :273-282) fails as in
finding 2.

## Falsification: what the guard covers and does not

Tested directly by importing the guard in `/tmp/falsify_guard.py` (read-only;
scratch tree under `/tmp/fake_core`) and with rustc 1.85.1 in `/tmp/n13`:

**Covered (verified rejecting):** a new `.rs` file anywhere else in
layerfs-storage src; unsafe in layerfs-content or layerfs-telemetry; a storage
lib.rs missing the deny; unsafe hidden behind `#[allow(unsafe_code)]` on a new
module (rustc's `deny` alone is overridable by a nested `allow` — the guard,
which scans text regardless of attributes, is the backstop that closes this).

**Not covered (guard blind, honestly stated):**

1. **A new crate under `core/crates/`.** `crate_name` returns the directory
   name; anything not in the three-name table is skipped
   (check_product_boundary.py:44). Verified:
   `unsafe_violations(Path("core/crates/layerfs-fs/src/lib.rs"), "unsafe fn evil() {}")`
   → no violation. The self-test documents this at test:76-77. Its other rules
   (line caps, test markers) still scan a new crate's src, but the unsafe rule
   does not.
2. **`include!` carriers with non-.rs extensions.** `production_files` scans
   only `.rs`/`.sql` under `src/` and `sql/` (check_product_boundary.py:90-96);
   an `src/encoding/codec_extra.inc` is invisible. Empirically
   (`/tmp/n13/inc`): a crate with the exact deny+allow structure whose
   `codec.rs` does `include!("codec_extra.inc")` where the `.inc` contains an
   unsafe block **compiles clean** (rustc exit 0) — the code lands inside the
   allowed module, so neither rustc's deny nor the guard's file scan sees it.
   It also evades the 999-line cap. Mitigation: adding the `include!` requires
   editing a tracked file (`codec.rs`), which review would see; the guard
   alone would not flag the resulting unsafe code.
3. **Manifest target paths outside `src/`** (e.g. `path = "alt/main.rs"` in
   Cargo.toml): not scanned for any rule. `src/bin/*.rs` *is* scanned (rglob
   under src/). core/AGENTS.md assigns manifest target paths to manual review.
4. **Substring attr check.** The lib.rs check is `attr not in source` on the
   full text including comments (check_product_boundary.py:51), so a comment
   containing the literal `#![deny(unsafe_code)]` would satisfy it if the real
   attribute were removed (the unsafe scan elsewhere still holds; the rustc
   backstop would be lost). Minor.
5. **Comment stripping handles `//` only** (:47): a `/* unsafe */` block
   comment leaves the token and over-rejects — a safe-direction false positive,
   noted for completeness.
6. **Third-party unsafe** (zstd-sys 2.0.16 and the C library itself) is outside
   the audit by design; the deny covers only first-party source text.

## UNVERIFIED

- The full `cargo test`/clippy/fmt suite for the core workspace was not run —
  the claim concerns source structure, the guard and its self-tests, all of
  which were exercised directly; nothing in the claim depends on test outcomes.
- Whether a `macro_rules!` defined inside codec.rs could expand unsafe code at
  a denied site (rustc lint-level propagation through macro definitions) was
  reasoned about but not empirically tested; the guard's text scan would still
  see the unsafe in the macro body only if it stays in a scanned `.rs` file.
- The E0453 reproduction used scratch single-file crates in `/tmp`, not a
  modified copy of layerfs-storage (modifying tracked files is forbidden); the
  semantics tested are crate-attribute-level and toolchain-pinned, so the
  transfer is direct, but the literal layerfs-storage tree was not recompiled
  with forbid.
- Whether any historical (pre-6c00e0f53) tree had a different unsafe surface
  was not audited; only the frozen commit was verified.

## RE-VERIFICATION (post-remedy, 2026-09-18)

Scope: inventory completeness only, on the current working tree (the two files
are modified but uncommitted; the main agent is the writer — expected and
confirmed via `git status --porcelain`, which shows ` M` for exactly
`core/crates/layerfs-storage/src/encoding/codec.rs` and
`docs/roadmap/0.1/0.1.7/component-decoupling/physical-encoding-and-packing.md`).
Everything outside these two files remains at the frozen commit.

Re-enumeration (digit-safe `grep -oE "ZSTD_[A-Za-z0-9_]*\("` over codec.rs):
the module imports 27 `zstd_sys` names — 19 functions plus 8 types
(`ZSTD_CCtx`, `ZSTD_DCtx`, `ZSTD_ErrorCode`, `ZSTD_FrameType_e`,
`ZSTD_ResetDirective`, `ZSTD_cParameter`, `ZSTD_dParameter`,
`ZSTD_frameParameters`, used in signatures/enums, not callable entry points).
All 19 functions are called: `ZSTD_isError` (1), `ZSTD_getErrorCode` (1),
`ZSTD_compressBound` (3: codec.rs:227, 301, 362),
`ZSTD_estimateCCtxSize_usingCParams` (2: codec.rs:222, 296),
`ZSTD_initStaticCCtx` (1), `ZSTD_initStaticDCtx` (1), `ZSTD_CCtx_reset` (6),
`ZSTD_CCtx_setParameter` (2), `ZSTD_CCtx_setCParams` (1), `ZSTD_CCtx_setFParams`
(1), `ZSTD_CCtx_refPrefix` (2), `ZSTD_compress2` (3), `ZSTD_DCtx_reset` (4),
`ZSTD_DCtx_setParameter` (2), `ZSTD_DCtx_refPrefix` (2), `ZSTD_decompressDCtx`
(3), `ZSTD_getCParams` (3), `ZSTD_getFrameHeader` (1),
`ZSTD_findFrameCompressedSize` (1). A programmatic cross-check
(`/tmp/invcheck.py`) of these 19 names against both documents reports:

- codec.rs module doc: **19/19 listed, none missing** — the paragraph now reads
  "`ZSTD_compressBound` and `ZSTD_estimateCCtxSize_usingCParams` (size
  arithmetic before any allocation)" alongside the original 17.
- design note: **19/19 listed, none missing** — the audited-surface paragraph
  now includes "two size-arithmetic helpers called before any allocation
  (`ZSTD_compressBound`, `ZSTD_estimateCCtxSize_usingCParams`)", a sentence
  recording that a verification subagent caught the omission on 2026-09-18,
  and a paragraph recording the verified guard blind spots (new crates,
  include! carriers, manifest target paths, substring attr test) matching my
  falsification findings.

No code changed: `git diff` on codec.rs is a single doc-only hunk (+11/-10,
662→663 lines), the import list is byte-identical, and the unsafe surface is
unchanged — still exactly 12 unsafe items (11 blocks + 1 `unsafe fn`) at the
same sites, each still carrying its SAFETY comment.

**Final verdict for N-13: PASS.** The single failing conjunct (inventory
completeness) is remediated and re-verified; all other conjuncts were verified
in the original pass and are unaffected by the doc-only diff. Remaining
qualifications are unchanged: the guard's blind spots (new crates, include!
carriers, manifest target paths outside src/, substring attr check) are now
recorded in the design note itself, and no memory-safety proof is claimed
anywhere.
