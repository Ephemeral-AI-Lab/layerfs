# SA2 review — Stage 5 sorted copy-on-write tree and whole-tree validity

Reviewer area: Stage 5 sorted COW (`core/crates/layerfs-content/src/filesystem/sorted/*`),
`filesystem/update.rs`, `filesystem/validate.rs`, `filesystem/directory/*`,
`filesystem/root.rs`, `filesystem/inode/*`, `filesystem/references/*`.

Snapshot reviewed: `c99a8d9f9e05a20ecc8e47b11b8f22fa791664e6` (main), clean tree.
Method: read-only source inspection; no build/test/clippy was run by this reviewer.
Test outcomes cited below come from the lead reviewer's log
`docs/roadmap/0.1/0.1.7/evidence/stages-1-5-review-20260917T160000Z/cargo-test.log`
(lines 174-282: `filesystem_bounds` 4 passed, `filesystem_reference` 2 passed,
`filesystem_sorted` 6 passed, `filesystem_topology` 8 passed).

## 1. Contract text this area is judged against

| Requirement | Source |
| --- | --- |
| Root inode: directory, count zero. Other directories and symlinks: exactly one binding. Regular files: at least one. No directory/symlink hardlinks | stage-5-handoff.md:140-142 |
| No dangling bindings, wrong kinds, duplicate names/identities, count underflow/overflow, multiple directory parents or effective-tree cycles | stage-5-handoff.md:146-147 |
| Sorted-page finality and whole-operation retention are two different proofs; never emit a locally final page for a directory later discarded as disconnected input; never serialize a provisional count | stage-5-handoff.md:151-153 |
| Public native entry points enforce their real trust preconditions | stage-5-handoff.md:167-170 |
| Preserve untouched-subtree reuse, unresolved neighbouring pages, exact fill/partition/height rules, grouped child acquisition; optional base root; initial construction needs no provisional empty seed | stage-5-handoff.md:226-244 |
| Non-root directories/symlinks exactly one reference; incremental validation of affected bindings/identities/effective parents with bounded work | filesystem-tree.md:206-211, 147-149 |
| Boundary gate: every non-root page satisfies its fill rule (inode leaf >= 50 rows, inode branch >= 64, directory page >= 3,277 bytes); no page > 8,192 bytes | stage-5-verification.md:101-103 |
| Review scope: whole-tree validity incl. cycles from several moves, duplicate names/identities, single parent, root ops, disconnected dirty inputs, membership before final emission | stages-1-5-reviewer-handoff.md:160 |

## 2. Check table

| Check | Location | Evidence | Status | Gap |
| --- | --- | --- | --- | --- |
| Optional base: build vs update entry separation | update.rs:70-72, 83-85 (`build_filesystem*` reject `base.is_some()`), update.rs:95-97, 108-110 (`update_filesystem*` reject `base.is_none()`); finish.rs:34-51 | `apply_root` reads the base root only when `root.is_some()`; `None` fabricates an in-memory `Wire{level:0,count:0}` | PASS | The internal `Option` is not asserted separately in tests, but both public routes are guarded |
| No provisional empty seed on construction | finish.rs:36-50; emit sites page.rs:364, finish.rs:175-180 | The synthesized base wire is never passed to `objects.emit`; the only build-path emits are real final pages. Test tests/filesystem_sorted.rs:39-66; genuinely empty result tests/filesystem_sorted.rs:69-101 | PASS | — |
| Empty directory emitted only as the real result | update.rs:171-179 (`empty_directory` only for a parent with no changes that is new or in a build); input.rs:142-148 (one update per parent) | A directory with `changes.is_empty()` can never be revised by a later update in the same operation because parents are strictly increasing | PASS | — |
| Exact page arithmetic | format.rs:22 (`EMPTY_PAGE_BYTES=44`), 274-282 (directory leaf 2+len+8, branch 2+len+32), 505-511 (inode leaf 81, branch 40), 649-666 | Page = 44 + sum(row widths); encoders write the same widths they size with | PASS | — |
| Fit ceilings (no over-occupation) | format.rs:304-306 (directory size <= 8192), 537-544 (inode leaf <= 100 rows, branch <= 127), 292-298, 521-526 (`page_items`: 741 leaf / 234 branch); merge.rs:57-62, 71-79; format.rs:586-605 (`validate_count`); format.rs:669-678 (`finish_node` <= 8192) | `push` refuses at the row ceiling and splits the moment `fits` is false; `fits`/byte checks bind before the row ceilings (740 leaf rows of 11 B = 8,184; 232 branch rows of 35 B = 8,164) | PASS | Row ceilings are safety nets, not the binding rule; no test asserts them directly |
| Fill rule (no under-occupation of a non-root page) | format.rs:300-302, 528-535; limits.rs:20-30 (50/64 rows, 3,277 bytes), limits.rs:45-47 (2/5 rule); page.rs:385-387 (`persist_entry` refuse); page.rs:259-268 (`check_page` on read) | Every non-root page is `filled`-checked when its parent persists it; the root is exempt by contract and is the only page that may be under-occupied; a 1-row branch root is collapsed before `persist` (finish.rs:95-103) so `decode`'s `count >= 2` guard (format.rs:190, 389-393) cannot be violated by output | PASS (derived) | Derived from source, not asserted by a test: no test checks the fill rule of emitted directory/inode pages (grep of `filled_page`/3_277 in tests/ finds only tests/filesystem_attributes.rs:25,321). Boundary gate covered only indirectly by reference-root equality |
| Split rule and derived split occupancy | format.rs:566-584 (`nearest_half`: smallest prefix whose doubling is closest to the total, ties to the smaller left, right always >= 1 row); merge.rs:80-92 | A split happens only after a page exceeds 8192 B (directory) or its row ceiling (inode), so both halves are ~half: directory >= ~4,074 B and inode leaf >= 50 / branch >= 64 rows; with uniform widths the tie rule yields exactly 50/51 (inode) and 370/371 of 741 (1-byte-name directory) | PASS | — |
| Tail rebalance across underfull neighbours | merge.rs:145-167 (`sibling` merges when either neighbour is unfilled), merge.rs:97-141 (`merge` re-partitions the pair: level 0 re-pushes through `push`, level > 0 re-pushes grandchild entries through `sibling`) | Sealed-reference parity: build `wide` = inode leaves 50/50/50/53/50/50 + branch 6 (tests/fixtures/filesystem/manifest.rs:71-78) and update `wide-remove` (one unlink) = 99/50/53/50/50 + branch 5 (manifest.rs:1410-1418): the 49-row leaf merged into one 99-row page with its 50-row neighbour, no re-split at 100, branch child count 6 -> 5. The replacement reproduces the sealed root and page shapes (tests/filesystem_reference.rs:107-159; cargo-test.log:248) | PASS | Parity covers the observed pairs only; no case with two simultaneously underfull siblings or a 3-level cascade |
| Height collapse | finish.rs:95-103 (`while root_node.level > 0 && root_node.items == 1` materialize and descend); finish.rs:71-94 (top-spine selection) | Collapse is applied to the final root only, after the whole merge, so a single-child root never becomes a 1-row branch page | PASS | An emptied sub-directory yields "no output" for its page (merge.rs:308-311) and is repaired by the same collapse; exercised only indirectly |
| Under/over-occupancy verdict at the end of a build or update | as above | A non-root page can only reach `persist_entry` as (i) a split half (~half full), (ii) a page that passed `filled` earlier, or (iii) the last pending node of its parent. `sibling` merges an unfilled node with its neighbour, and any merge with a filled page (>= 3,277 B / >= 50 rows) re-splits into halves that are themselves filled, so case (iii) survives only when a page's whole pending chain is underfull, which then propagates to the root and is absorbed by the collapse. Under-occupied pages are therefore refused (operation error) rather than written; over-occupied pages are impossible (`fits` + `validate_count` + `finish_node`) | PASS (negative claim, derived) | No test constructs an underfull chain; the refusal path `NonCanonicalPagePartition` from page.rs:386 is untested for the filesystem formats |
| Untouched subtrees keep exact IDs (structure sharing) | merge.rs:182-188 (`!changes.in_range(bound)` -> `Node::existing(id, &read.wire)`, counted in `work.untouched_subtrees`); merge.rs:275-279 (the range bound is the child's own max key, `None` only for the last child); page.rs:314-321 (`persist` returns an identity-holding node untouched when nothing is pending); page.rs:349-352 (a rebuilt page whose canonical bytes are unchanged keeps its old identity) | Out-of-range children are never materialized, so their subtrees are never read or re-encoded; their entries carry `id: Some(stored)` into the parent page | PASS | Identity preservation is proven by code path; `pages_reused`/`untouched_subtrees` counters are reported but no test asserts exact ID equality of an untouched child across an update |
| No hidden whole-tree walk in the merge | merge.rs:182-188, 247-295 | Range pruning happens before any child read; only nodes whose key range contains a pending change descend | PASS | — |
| Hidden subtree walk in validation | validate.rs:239-284 (`check_effective_cycles`), 287-339 (`effective_entries`) | For every changed binding whose child is an *existing* directory the walk lists that directory's whole effective subtree (base entries merged with this operation's changes) and looks for the new parent | INCOMPLETE (see F3) | Cost is O(subtree) entries plus one inode lookup per visited entry (validate.rs:274, 354); bounded at 4,096 entries per binding with a refusal, and the read work is discarded |
| Grouped reads per level (provider calls) | finish.rs:35 -> page.rs:177-186 -> objects.rs:64-68 (1 point read of the base root; a point read is a 1-id batch, access.rs:23-24); merge.rs:247-249 with page.rs:25 (`BATCH_CHILDREN=32`) and page.rs:217-256; objects.rs:76-90 | Per in-range node at each level: ceil(children/32) grouped provider calls, one call per <= 32 children; every direct child of an in-range node is read, in range or not, because `Node::existing` needs the child's own count/bytes/items (page.rs:115-126) and the parent's recorded subtree totals are re-verified (merge.rs:298-300). Point-read fallback only when no chunk fits the scratch budget (page.rs:226-255, merge.rs:265-267) | PASS | — |
| "Final-only child-first emission" is really final-only | page.rs:314-370 (a page is emitted only when nothing is pending); finish.rs:1-7; test tests/filesystem_sorted.rs:39-66 | No provisional page object is ever written for a base-less build: the seed is an in-memory `Wire` (finish.rs:41-49) and is never emitted. Locally final pages are emitted child-first as designed; the root is emitted last (update.rs:355-359) | PASS | — |
| Duplicate names | input.rs:38-41 (per-update strict order), input.rs:142-148 (one update per parent), merge.rs:39-46 (`take` rejects a duplicate or descending next key) | Test tests/filesystem_sorted.rs:158-178 (duplicate, descending and repeated-name cases all refused, nothing emitted) | PASS | — |
| Duplicate identities (reused serial) | input.rs:159-168 (`new_inodes` unique, non-zero, not the root in an update), validate.rs:191-209 (`check_new_identities`: a declared-new serial must have no base record), merge.rs:42-44 (final stream unique) | Test tests/filesystem_topology.rs:132-150 | PASS | For a build the check is vacuous (`topology.table` is `None`, validate.rs:199-201), which is correct because there is no base to collide with |
| Single parent for directories/symlinks | validate.rs:150-159 counts only *this batch's* additions for a non-regular child; reduce.rs:106-129/509-543 accept any final count > 0; inode_leaf.rs:101-113 (`InodeValue::validate`) implements the exact invariant and has no call site in the filesystem path | Test tests/filesystem_topology.rs:153-176 covers two additions in one batch | **FAIL** (F2) | A second binding added for an inode that already has one binding in the base is accepted; the stored record gets count 2 |
| Effective-tree cycles | validate.rs:239-284; per binding: existing directory child (251), walk (260-280), rejection at 271-273, work limit 305-308 | Test tests/filesystem_topology.rs:179-214 (two-part cycle of existing directories, refused) and 217-232 (self binding, refused) | **FAIL** (F1) for cycles built entirely from declared-new directories; PASS for existing ones | Children with no base record are skipped at validate.rs:251-253, so the walk never runs for new directories |
| Root operations | validate.rs:137-139 and 211-231 (no binding may name the root; build root must be a Directory; root cannot be declared new in an update), input.rs:162-168, reduce.rs:516-527 (root final count pinned to 0), update.rs:395-424 (`*serial != root_serial`, so the root is never in the zero-count release set) | Test tests/filesystem_topology.rs:79-130 (`the_root_stays_a_zero_count_directory`) | PASS | A build that omits `root_serial` from `new_inodes` is not rejected up front; see F6 |
| Disconnected dirty inputs | reduce.rs:493-498 (`Row::Count` with count 0 and serial != root -> `new inode without binding`); update.rs:394-424 (zero-count release) | Test tests/filesystem_topology.rs:235-250; tests/filesystem_hardlinks.rs:287-307 | PASS for count 0; **FAIL** for a disconnected cycle whose members each keep count 1 (F1) | Reachability from the root is never established; only the count is |
| Membership before final emission / page finality vs whole-tree validity | update.rs:153 (validate first, before any mutation), 261-274 (content roots rewritten), 287-311 (release), 315-341 (reducer finish + inode merge), 349-359 (cleanup, then root emit); finish.rs:1-7 | The root object is emitted only after every final count exists and the ordering resources are released; a failing operation returns before any root exists | PASS for the root; see F4 for intermediate pages | Directory/inode-table pages are emitted during the merge, before membership is known; that is the declared child-first design, but F4 shows it is applied even to a directory the same operation deletes |
| Validation reads are charged | update.rs:32-51 (`FilesystemUpdateCounters` has no validate field); validate.rs:302, 354 (`DirectoryReadWork::default()` / `InodeReadWork::default()` discarded) | filesystem-tree.md:156-157 requires topology checks and repeated reads to be charged where they occur | **FAIL** (F5, reporting only) | The cycle walk's page reads and inode lookups are invisible in the returned counters |

## 3. Counterexamples

### F1 (HIGH): a cycle of declared-new directories is accepted

Rejection sites that do **not** fire: validate.rs:251-253 skips every binding child with no
base record, and `declared_new` directories have none; validate.rs:305-308 (work limit) is
unreached; reduce.rs:493-498 only rejects a *zero* count.

Minimal sequence (public API, base = any readable root whose inode table lacks `n1`, `n2`):

```text
update_filesystem(objects, FilesystemInput {
  base: Some(base_root), scope, root_serial: 1,
  directories: &[ DirectoryUpdate { parent: n1, changes: [("a", Some(n2))] },   // parents ascending
                  DirectoryUpdate { parent: n2, changes: [("b", Some(n1))] } ],
  inodes:  &[ InodeUpdate { serial: n1, value: <Directory> },
              InodeUpdate { serial: n2, value: <Directory> } ],
  new_inodes: &[n1, n2], resources }, backing)
```

Trace: each new parent takes `base_directory = None` (validate.rs:110-118, update.rs:183-185)
and produces a fresh 1-row leaf; `additions[n1] = additions[n2] = 1` (validate.rs:155-159);
the walk is skipped twice; `note_retained_binding` gives each `Row::Count{count: 1}`
(reduce.rs:106-117, 222-226); `finish_row` accepts count 1 (reduce.rs:493-508); the table is
rebuilt and the root is emitted. Observed: `Ok(FilesystemResult)` whose inode table holds two
records whose content roots reference each other and which no root binding reaches. The same
input against `build_filesystem` also succeeds because `check_effective_cycles` returns early
when there is no base (validate.rs:243-245).

Consequence: an accepted final filesystem that violates stage-5-handoff.md:146-147 and
filesystem-tree.md:506 ("cycle ... rejection"); any consumer that walks the inode table or
resolves the namespace transitively can loop. The report's claim of cycle refusal
(stage-5-report.md:182-184) is true only for cycles among base-resident directories.

Smallest remedy: in `check_effective_cycles`, do not `continue` for a child that is in
`checked.declared_new`: seed the pending stack with an empty base content and that child's own
`update_for` changes (the existing `effective_entries` merge already applies changes to an
empty base list). Add a topology case for two new directories bound to each other, in both the
build and the update route.

### F2 (MEDIUM-HIGH): a directory or symlink can acquire two parents

`validate.rs:150-159` counts only bindings added by this batch, so an inode that already has a
base binding passes with `additions[child] == 1`; `reduce.rs:509-543` then stores
`base.namespace_ref_count + delta = 2` and never consults `InodeValue::validate`
(inode_leaf.rs:101-113, whose `kind == RegularFile || count == 1` rule is exactly the missing
check; grep shows no call site for it on filesystem values — `src/file/mapping/codec.rs:97` is
a different type).

Minimal sequence on the `nested()` fixture (tests/filesystem_topology.rs:38-77; `/d/e` where
`e` is a directory with count 1):

```text
DirectoryUpdate { parent: d, changes: [("e2", Some(e))] }   // no removal of "e"
```

Observed: `Ok`; `/d/e` and `/d/e2` both resolve to `e`, whose stored
`namespace_ref_count` is 2. The cycle walk passes because `d` is not inside `e`'s subtree
(validate.rs:251-279). For a symlink the same sequence yields a symlink inode with count 2.

Consequence: directory/symlink hardlinks (a namespace DAG), contradicting
stage-5-handoff.md:140-142 and filesystem-tree.md:206-211; downstream consumers that assume a
tree (namespace walks, release accounting, future mount/GC) see one subtree twice.

Smallest remedy: validate every final row with the existing helper — for a non-root row,
`value.validate(false)?` in `FinalRows::finish_row` (reduce.rs:486-545) or in
`apply_inode_values`. That single call rejects count 2 for directories and symlinks, keeps
regular-file hardlinks, and needs no change to the input contract. (Alternatively require
`additions[child] - removals[child] <= 0` for a non-regular child that exists in the base.)

### F3 (MEDIUM): the cycle check refuses legal updates to large directories, and its work is unreported

validate.rs:24 (`MAXIMUM_CYCLE_CHECK_ENTRIES = 4_096`), 260-280 (per-binding walk), 305-308
(`Err(InvalidRecord("cycle check work limit"))`). Any binding of a directory whose *effective
subtree* exceeds 4,096 entries - e.g. a rename or a move of a directory with 4,097 descendants
- fails the whole operation with an error that a caller cannot distinguish from a genuine
cycle. Each visited entry also costs a base inode lookup (validate.rs:274, 354) and one listing
page (validate.rs:296-303), and the walk repeats per changed binding with no memoization.
Neither the limit nor the refusal is in the report's limits table (stage-5-report.md:278-293),
and no test covers it (grep for `MAXIMUM_CYCLE_CHECK_ENTRIES`/`cycle check work limit` in
tests/ returns nothing). Remedy: declare the limit and its consequence, add a boundary case, and
charge the walk's reads in a counter; the walk itself can reuse the membership ledger instead of
re-listing subtrees.

### F4 (MEDIUM): pages of a directory the operation deletes are still rebuilt and emitted

update.rs:166-256 merges *every* supplied directory update before membership is known, and
update.rs:287-311 releases zero-count directories afterwards; the release then reads the
*directory's new content root* because `note_value` (update.rs:261-274) runs first. Input:
`DirectoryUpdate{parent: d, changes: [...]}` for a directory `d` whose last binding is removed
in the same batch. Observed: content pages for `d` are emitted to the consumer
(page.rs:314-370) and then `d` is released; the objects are orphans in the consumer's store.
stage-5-handoff.md:151-153 and filesystem-tree.md:295-297 require disconnected dirty inputs not
to be emitted ("the current caller already skips known disconnected dirty inodes; preserve that
behavior"). Related failure mode: binding a *declared-new* inode inside such a directory makes
the release call `note_removed_binding` on a `Row::Count` row and the operation fails with
`InvalidRecord("new inode removal")` (reduce.rs:120-129) instead of releasing the new inode.
Remedy: skip (or defer) a directory update whose parent's derived final count is zero, or state
the deviation explicitly; add a case for "delete a directory and change its content in one
batch".

### F5 (LOW-MEDIUM): validation-phase reads and lookups are not charged

`FilesystemUpdateCounters` (update.rs:32-51) has no validation fields, and validate.rs:302 and
354 pass throwaway `DirectoryReadWork::default()` / `InodeReadWork::default()`. The cycle
walk can read thousands of directory pages and inode records that no returned counter shows.
filesystem-tree.md:156-157 requires topology checks and repeated reads to be charged where they
occur. Remedy: return a small `ValidateWork` (pages read, entries visited, inodes looked up)
and add it to the counters; no algorithm change.

### F6 (LOW): a build whose root serial is not declared new fails with an unrelated error

input.rs:159-168 does not require `root_serial ∈ new_inodes` when `base.is_none()`. The root
then gets a `Row::Effect` (reduce.rs:222-232), and `FinalRows::fill_wave` looks it up in the
placeholder table built from a zero `ObjectId` (validate.rs:71-76; reduce.rs:438-451), so the
provider is asked for the all-zero object and the operation fails with `MissingObject`/I/O
rather than a precondition error. Remedy: require the root in `new_inodes` for a build, or
short-circuit the base lookup when `base.is_none()`.

### F7 (NOT_APPLICABLE): a symlink whose target is itself

C1 stores symlink targets as opaque bytes (symlink.rs:1-8, 24-31, 51-80); no path resolution,
normalization or platform interpretation exists, and no read path follows a symlink:
`resolve`/readlink return the symlink's own inode and refuse to descend through a non-directory
(read.rs:104-124, 181-188), and the release traversal only descends directories (release.rs:86-92).
A self-referential target is therefore representable but has no tree-level meaning in C1, so
there is no cycle to reject. Note for the record: C1 offers a resolving consumer no protection
against self-referential symlink chains; that is by contract (symlink.rs:1-5) and remains an
adapter concern.

## 4. Verdict summary

| # | Finding | Severity | Status |
| --- | --- | --- | --- |
| F1 | Cycle of declared-new directories accepted (walk skips new children) | High | FAIL |
| F2 | Directory/symlink hardlink accepted (additions-only parent rule; `InodeValue::validate` uncalled) | Medium-High | FAIL |
| F3 | Legal rename/move of a >4,096-entry directory refused; limit undeclared, untested, uncharged | Medium | INCOMPLETE |
| F4 | Pages rebuilt and emitted for a directory deleted in the same operation | Medium | INCOMPLETE (contract deviation) |
| F5 | Validation-phase reads/lookups absent from the operation counters | Low-Medium | FAIL (reporting) |
| F6 | Build root not required in `new_inodes`; confusing failure | Low | INCOMPLETE |
| F7 | Symlink self-target | — | NOT_APPLICABLE |

Checks that pass with source-derived proof: optional base, no provisional seed, exact page
arithmetic, fit ceilings, fill rule (non-root pages refused rather than under-occupied; root may
be under-occupied), split rule, tail rebalance (with sealed-reference parity evidence),
height collapse, untouched-subtree identity reuse with no whole-tree merge walk, grouped reads
per level, duplicate name/identity rejection, root operations, membership-gated root emission.

Evidence gaps (not defects): no test asserts the emitted-page fill rule for directory/inode
pages; no test exercises an underfull merge chain or a multi-level cascade; no test covers the
validation work limit; no test covers a directory deleted together with changes to its content.
