# Stage 5 review area 4: reads (resolve/stat/list/readlink) and attributes

> Independent read-only review. Snapshot: HEAD `c99a8d9f9e05a20ecc8e47b11b8f22fa791664e6`
> (branch `main`), clean tree. No product source was modified; no cargo command was run
> (build/test/clippy belong to the lead reviewer). Scope note: the reviewed artifact is the
> replacement product under `core/`; the task's relative paths
> (`filesystem/read.rs`, `path.rs`, `attributes/*`, `directory/read.rs`,
> `inode/codec.rs`, `symlink.rs`, `limits.rs`) resolve there. Root `crates/` is the
> pinned reference and is cited only as the oracle/behaviour comparison.
> Governing: `filesystem-tree.md` §3/§6/§7, `stage-5-handoff.md` §3C, `stages-1-5-reviewer-handoff.md` lines 163–164.

## 0. Findings (severity ordered)

### F1 — `list` silently truncates to an empty page when the byte bound is below one row width (MEDIUM; code defect)
- Trigger: `list(path, after, max_entries, max_bytes)` with `max_bytes < 2 + 8 + name.len()` (15 bytes for a 5-byte name).
- Location: `core/crates/layerfs-content/src/filesystem/directory/read.rs:219-224`.
  The guard `if entries.len() == max_entries || bytes + width > max_bytes` returns
  `ListingPage { entries, continuation: entries.last().map(..) }`. With `entries` empty the
  result is `entries = []`, `continuation = None` — a successful page that means "end of
  directory" (the same value returned for a genuinely exhausted or empty directory at
  `directory/read.rs:240-243`).
- Observed: `Ok(ListingPage { entries: [], continuation: None })`. Expected: an explicit
  refusal. The pinned reference refuses: `crates/layerfs-content/src/tree/directory/read.rs:297-299`
  returns `CoreError::ObjectLimitExceeded` when the first candidate entry does not fit.
- Consequence: a caller that honours the continuation token cannot distinguish "no more
  entries" from "your byte bound cannot fit one entry" and reports a non-empty directory as
  empty (or as fully listed while entries are dropped). This is a produced WRONG truncation
  value, which `stages-1-5-reviewer-handoff.md:163` requires to be rejected.
  The shipped callers use 8192 bytes, so no current test or example hits it; the defect is
  reachable through the public API with an accepted argument (`max_bytes = 1` is accepted,
  only `0` is refused at `directory/read.rs:195-197`).
- Smallest remedy: in the byte-bound branch return
  `Err(ContentError::ObjectLimitExceeded { .. })` when `entries.is_empty()` and a row was
  rejected, i.e. mirror the reference's `entries.is_empty()` check; keep `None` only for the
  genuinely exhausted path.
- Test gap that hides it: `core/crates/layerfs-content/tests/filesystem_read.rs:144-148`
  asserts `matches!(too_small, Err(ContentError::InvalidRecord("listing limit")) | Ok(_))`,
  which accepts both the correct refusal and today's silent empty page. The assertion is
  non-discriminating for the only case that matters.

### F2 — `read_portable`/`read_attribute`/`attribute_keys` on the path API are untested (MEDIUM; proof gap, not a code defect)
- Location: `core/crates/layerfs-content/src/filesystem/read.rs:191-231`.
- Evidence: no test or example calls `FilesystemRead::read_portable`, `read_attribute` or
  `attribute_keys` (grep over `core/crates/layerfs-content/{tests,examples}`). The
  `Session` harness always supplies synthetic metadata roots
  (`tests/support/filesystem.rs:189-201`, used at `tests/filesystem_read.rs:16-29`), so no
  test ever binds a real attribute tree to an inode and reads it back through a path.
  `filesystem_attributes.rs` exercises the attribute layer directly
  (`read_portable`/`read_opaque` against a root ObjectId), never through
  `FilesystemRead`.
- Consequence: the only end-to-end path-level attribute read route (resolve → kind →
  metadata_root → portable/opaque value) has no acceptance proof; the kind check at
  `read.rs:193` (portable read is allowed for every kind) and the resolve-plus-read
  combination are unverified. Reported as INCOMPLETE, not a defect.

### F3 — the 1 MiB attribute-value bound is a read default, not an enforced write bound (MEDIUM; enforcement + report overclaim)
- Location: `core/crates/layerfs-content/src/filesystem/limits.rs:42`
  (`MAXIMUM_ATTRIBUTE_VALUE_BYTES = 1 MiB`) is referenced only at
  `attributes/read.rs:166` (`read_value_bounded`'s default read bound). Neither
  `attributes/value.rs:18-24` (`emit_value`) nor `attributes/patch.rs:100-110`
  (`AttributePatch::Set`) checks it; the only write-side check is `u32` range at
  `value.rs:24`.
- Observed vs expected: a patch or direct emit of a 2 MiB value succeeds; the profile's own
  bounded read (`read_value_bounded`) then fails `ObjectLimitExceeded` on data the profile
  itself stored. `stage-5-report.md:287` states this limit as **enforced**.
- Consequence: an enforced-limit claim without enforcement; a caller can create attribute
  values its own bounded reader refuses. No test covers a value above 1 MiB.
- Smallest remedy: reject `value.len() > MAXIMUM_ATTRIBUTE_VALUE_BYTES` in `emit_value`
  (one check; `apply_patches` reaches it through the same function), or downgrade the report
  row to "read bound".

### F4 — batch read requests have no cardinality bound (LOW–MEDIUM; code/contract gap)
- Location: `core/crates/layerfs-content/src/filesystem/directory/read.rs:108-119`
  (`lookup_many`), `inode/read.rs:85-101` (`lookup_many`),
  `attributes/read.rs:43-54` (`lookup_many`), reached from the public
  `FilesystemRead::lookup_names`/`lookup_inodes` (`read.rs:155-178`).
- Observed: the demand slice size is whatever the caller passes; `answers` and each
  `read_canonical_batch` wave grow with it. Expected: the reference bounded the grouped
  reader at 128 keys (`crates/layerfs-content/src/tree/directory/read.rs:53-55`,
  `CoreError::ObjectLimitExceeded`), and `stage-5-handoff.md:249-252` asks for bounded read
  waves while `stages-1-5-reviewer-handoff.md:163` asks for "bounded count and bytes per call".
- Consequence: an unbounded single call/wave is possible from the public API; no test pins
  any cap. `filesystem_bounds.rs:219` asserts `peak_wave() <= 32` only for the *update* path,
  never for reads.

### F5 — read work counters under-report and are never consumed (LOW; accounting gap)
- Location: `read.rs:115-120` resolves each component through
  `directory::read::lookup`, which builds and discards a private
  `DirectoryReadWork::default()` (`directory/read.rs:41-48`). Only `list`,
  `lookup_names` and the inode lookup (`read.rs:241`) charge `self.work`.
- Evidence: `FilesystemReadWork::work()` (`read.rs:99-101`) is read by no test and no
  example (grep over `core/crates/layerfs-content`). `DirectoryReadWork.pages_read` is
  therefore 0 for every `resolve`/`stat`/`readlink`/`read_portable`/`read_attribute` call,
  even though each walks root + one directory page per component + one inode page per
  component.
- Consequence: the read path has no usable bounded-work evidence, which is what
  `stage-5-handoff.md:410` (`filesystem_bounds`: "counters showing no unrelated whole-tree
  scan") and the report's §5 ledger imply. INCOMPLETE.

### F6 — `attribute_keys` is an unbounded whole-tree collection with no limit (LOW; scope gap)
- Location: `read.rs:219-231`: `let mut keys = Vec::new(); visit_keys(..)`, over
  `attributes/patch.rs:131-142`. `visit_keys` itself is a bounded one-leaf cursor, but the
  caller accumulates every key with no count or byte bound and no cancellation.
- Consequence: a read entry point whose result grows with the whole attribute tree; the
  contract's sufficient surface is "Bounded generic read/set/remove/patch is sufficient"
  (`filesystem-tree.md:326`). No test covers it (grep: only `src/`).

### F7 — report/test-claim mismatch for the "paged-read" family (LOW; evidence accuracy)
- `stage-5-completion-report-20260917.md:258` claims `filesystem_bounds` contains
  "a 60-name listing at 4 names / 8 KiB per wave".
- Actual: `filesystem_bounds.rs` never calls `list` (only update-path counters at
  lines 210-215, 292). The 4-entry/8192-byte pagination loop is
  `tests/filesystem_read.rs:154-170` over `wide(20)` (`filesystem_read.rs:136`);
  `wide(60)` at `filesystem_read.rs:174` feeds the batch duplicate-demand test, not paging.
  `stage-5-verification.md:55`'s `filesystem.bounds | paged-read` row therefore has no
  exact C1 test match. Prose claim, not a code defect.

### Non-findings verified (checked, no counterexample found)
- No Apple/APFS/platform dispatch anywhere in product source: grep for
  `apple|apfs|darwin|xattr|APFS|Apple|macos|osx` and for
  `cfg(target_os|cfg(unix|cfg(windows|std::os::` over
  `core/crates/layerfs-content/src` returns nothing. `apple.xattr` appears only as fixture
  data in tests (`tests/filesystem_attributes.rs:67`,
  `crates/layerfs-content/tests/stage5_reference_fixtures.rs:737-757`), which the replacement
  legitimately stores as generic opaque data.
- Portable grammar matches the pinned reference exactly (mask, symlink exactness, nanosecond
  bound, byte widths) apart from the intended whitelist removal; see the table below.

## 1. Check table

| Check | Location | Evidence | Status | Gap |
| --- | --- | --- | --- | --- |
| resolve/stat/list/readlink public | `core/crates/layerfs-content/src/filesystem/read.rs:104,127,132,181`; re-exported `filesystem/mod.rs:25`, `lib.rs:34` | `filesystem_read.rs` target calls all four through the public API | PASS | also `lookup_names/lookup_inodes/read_portable/read_attribute/attribute_keys` public (`read.rs:155,175,191,202,219`) |
| Duplicate-demand cardinality and ORDER (names) | `directory/read.rs:113-114,139-149,152-171` | `filesystem_read.rs:173-194` (`e0000` demanded twice → index 2 answers `serials[0]`, line 192; absent stays `None`) | PASS | — |
| Duplicate-demand cardinality and ORDER (inodes) | `inode/read.rs:91,120-128` | `filesystem_read.rs:195-206` (order preserved, absent ≠ missing) | PASS | duplicates of the same serial are not covered by the test |
| Duplicate-demand (attribute keys) | `attributes/read.rs:49,74-95` | none | INCOMPLETE | no test duplicating a key demand |
| Duplicate PATH: resolved once or repeatedly? | `read.rs:104-124` — no page cache, no path batch API (grep: no `resolve_many`) | none | INCOMPLETE | each `resolve` call re-descends; no counter/test measures it (F5) |
| Shared ancestor reads | batches: `directory/read.rs:152-171`, `inode/read.rs:130-140`, `attributes/read.rs:84-94` | `filesystem_read.rs:173-207`; `filesystem_bounds.rs:219` (`peak_wave<=32`, update path only) | PASS (batches) / INCOMPLETE (cross-call) | no operation-local page reuse across separate `resolve` calls; the reference kept `DirectoryLookupCache` (`crates/layerfs-content/src/tree/directory/read.rs:172-248`) |
| Bounded count per call | `directory/read.rs:195-197` | `filesystem_read.rs:139-140` (`max_entries=5` → 5) | PASS (list) | batch lookups unbounded (F4) |
| Bounded bytes per call | `directory/read.rs:217-225` (`2 + 8 + name.len()`) | `filesystem_read.rs:141-143` (`max_bytes=15` → exactly 1 entry) | PASS | the sub-row bound is silently truncated (F1) |
| Progressing pagination | `directory/read.rs:202-207,213-216,221,242` | `filesystem_read.rs:153-170`: 20 names, strictly ascending, no repeats/skips; `filesystem_reference.rs:435-448` pages a real tree | PASS | cannot loop forever; can end early on F1 |
| Wrong summary rejected (directory) | `directory/read.rs:253-272` (ceiling, fill, level, last key), `292-298` (sibling minimum ordering) | `filesystem_failure.rs:237-320` (wrong identity / foreign grammar demanded page) | PASS | — |
| Wrong summary rejected (inode) | `inode/read.rs:162-177` | indirect via `filesystem_reference.rs` read-back parity | PASS | no dedicated malformed-inode-page read test |
| Wrong summary rejected (attribute) | `attributes/read.rs:172-194` | `filesystem_attributes.rs:342-376` (decode level), `415-434` | PASS | child-summary mismatch on the read path is not directly tested |
| Wrong kind rejected | `read.rs:56,88,109,122,140,161,183` (`WrongLogicalRole`) | `filesystem_read.rs:120-123` (list on a file), `239-242` (readlink on root) | PASS | — |
| Wrong scope/profile rejected | `root.rs:133-138` (profile), `read.rs:85-90` (root loaded once) | `filesystem_topology`/`filesystem_codec` profile refusals; reads take only a root id, so no scope argument exists to forge | PASS / NOT_APPLICABLE | — |
| Wrong truncation rejected | `directory/read.rs:219-224` | `filesystem_read.rs:144-148` accepts both outcomes | **FAIL** | F1 |
| Hidden full-tree collection behind list/read | `directory/read.rs:200-243` stops at the limit; `read.rs:104-124` walks one path | `filesystem_reference.rs:435-448`; `filesystem_bounds.rs` update counters | PASS for list/resolve / INCOMPLETE for `attribute_keys` | F6 (contract surface: `filesystem-tree.md:326`) |
| Portable mode grammar | `attributes/portable.rs:28-46,60-73` (4 BE bytes; `0o777` file/symlink, `0o1777` directory, symlink exactly `0o777`) | `filesystem_attributes.rs:197-274` (`0o7777` rejected, symlink `0o755` rejected); identical to reference `crates/.../metadata/portable.rs:16-44` | PASS | — |
| Portable mtime grammar | `attributes/portable.rs:49-57,76-94` (12 bytes: `i64` seconds BE + `u32` nanos BE, nanos ≤ 999 999 999) | `filesystem_attributes.rs:225-227` round trip, `260-266` bound rejected | PASS | `mtime_seconds` is unrestricted in both reference and replacement (no divergence) |
| Generic key bounds | `attributes/keys.rs:29-58` (domain non-empty ≤64 UTF-8 no NUL; key ≤255 no NUL; portable domain limited to `mode`/`mtime` at 54-56) | `filesystem_attributes.rs:98-114` | PASS | decoder re-checks the same bounds (`codec.rs:331-351`) |
| Key ordering | `keys.rs:86-93` (domain bytes then key bytes), `keys.rs:102-106`, `patch.rs:76-81`, `codec.rs:233-235,265-267,366-375` | `filesystem_attributes.rs:342-376`; `filesystem_sorted` unsorted rejection | PASS | strict ordering enforced on both build and decode |
| Opaque untouched-key value-root preservation | `patch.rs:73-75,99-124` (base entry pushed with its stored `value_root`, never decoded) | `filesystem_attributes.rs:117-194`: `work.preserved == 22` (148) and every untouched key's `value_root` compared to the root that built the base (186-193) | PASS | — |
| Bounded extent-only values | `value.rs:18-46` (always Chunk + ExtentLeaf + FileState), `49-72` (read bounded by `maximum_bytes`, exact-length check) | `filesystem_attributes.rs:379-398` (1/4/12/4096 bytes, `extent_count == 1`, empty refused), `234-249` (real FileState root) | PASS (read) / **FAIL** (write bound) | F3 |
| Exact page sizing + partition rule | `codec.rs:73-102,114-116` (44 + Σ rows; fill ≥ 3277 = `limits.rs:30`), `build.rs:104-142` split, `353-374`/`377-398` tail rebalance | `filesystem_attributes.rs:277-339` (200 entries; all ≤ 8192, non-final ≥ 3277; oversized row is an error, not a page-full signal) | PASS | the rebalance keeps the predecessor ≥ fill (moved ≤ 3 589 B out of a group that was > 7 792 B when it split), so no page drops below the rule |
| No Apple-specific/APFS dispatch | whole `core/crates/layerfs-content/src` | grep: no `apple/apfs/darwin/xattr/Apple/macos/osx`, no platform `cfg`/`std::os` | PASS | — |
| Attribute-page equality with SUPPLIED value roots | `attributes/build.rs:422-433` + sealed fixtures | `filesystem_attributes.rs:47-95` (built root ObjectId == sealed reference root, canonical bytes == sealed file, object set == sealed set) with byte-identical synthetic value roots on both sides (`tests/support/filesystem.rs:198-201` vs `crates/layerfs-content/tests/stage5_reference_fixtures.rs:83-88`); `filesystem_codec.rs:151-172` | PASS | only 1/2/60-entry trees; exact partition equality beyond those is not sealed |
| Equality of two INDEPENDENTLY constructed complete attribute trees | no location | none | **NOT_TESTED** | the patch route (`patch.rs`) and the build route are never compared for the same final logical entry set; see §2 |
| Self-referential oracles | `filesystem_attributes.rs:347-348`, `filesystem_codec.rs:172` | `decode(encode(page)) == page` on the same value | WEAK (codec symmetry only) | does not establish independent construction |
| Wrong profile / unsupported required profile refuses before mutation | `root.rs:56-60,133-138`; `keys.rs:54-56` | `filesystem_codec`, `filesystem_topology` refusals | PASS | — |
| Report-prose accuracy (paged-read family) | `stage-5-completion-report-20260917.md:258` vs `filesystem_bounds.rs` | grep: `filesystem_bounds.rs` has no `list(` call | FAIL (prose) | F7 |

## 2. The two equality claims (review item 3/4)

They are **not the same claim**, and only the first is tested.

```text
Claim A (tested):  builder(supplied value roots) == pinned reference bytes/roots
                   -> an external-oracle statement about the page grammar and its
                      partition function for identical value-root inputs.
Claim B (untested): construct(route 1, logical input) == construct(route 2, same input)
                   -> an intra-implementation statement that the build route and the
                      patch/merge route reach the same canonical tree for the same
                      final logical key set. Claim A cannot imply B: apply_patches
                      introduces a second feed path (streamed base cursor + patch list)
                      into AttributeTreeBuilder, and nothing compares its output to a
                      from-scratch build.
```

- Claim A evidence: `tests/filesystem_attributes.rs:47-95` compares
  `build_attribute_tree` output against sealed roots/bytes/objects for
  `attr-single`/`attr-pair`/`attr-wide`; both sides receive the identical synthetic value
  roots (`support/filesystem.rs:198-201` = `crates/layerfs-content/tests/stage5_reference_fixtures.rs:83-88`).
  This is a meaningful comparison: an external, separately generated oracle.
- Claim B evidence: none. `tests/filesystem_attributes.rs:117-194` builds a 24-key base,
  applies `set(key-01)` + `remove(key-07)`, verifies the read-back values and the 22 preserved
  value roots, and stops. The patched root `updated` is never compared with a fresh
  `build_attribute_tree` over the same final entry set, nor with a second patch order that
  reaches the same final state. The strongest assertion in that test about construction is
  `work.preserved == 22` (line 148), which is a counter, not an identity.
- `assert_eq!(decode_attribute_page(&bytes).expect("decodes"), page)`
  (`filesystem_attributes.rs:348`) and `assert_eq!(encode_attribute_page(&page).expect("encodes"), attr_bytes)`
  (`filesystem_codec.rs:172`) are round trips of one value: useful codec symmetry checks, not
  independent construction.
- Smallest remedy for B: add one case that builds the base, patches it to a final logical
  set, builds the same final set from scratch, and asserts equal root `ObjectId` and equal
  reachable object sets; a second variant should reach the same final set by a different
  patch order/split point. (A test that merely builds the same tree twice would be a
  self-referential oracle and should not be credited.)

## 3. Method and limits

- Read-only inspection of `core/crates/layerfs-content/src/filesystem/{read.rs,path.rs,limits.rs,symlink.rs,attributes/*,directory/read.rs,inode/read.rs,inode/codec.rs}`,
  the Stage 5 external targets `filesystem_read`, `filesystem_attributes`, `filesystem_bounds`,
  `filesystem_failure`, `filesystem_reference`, `filesystem_codec`, the test support harness,
  and the sealed-fixture generator in the reference workspace.
- No cargo build/test/clippy/fmt was run (lead reviewer's scope). Every "PASS" above is a
  source + committed-test reading, not an executed result; execution status stays with the
  lead reviewer's `cargo-test.log`.
- F1, F3 and F4 are source-visible defects/gaps; F2, F5, F6 and F7 are proof or claim gaps and
  are labelled as such rather than as code defects.
