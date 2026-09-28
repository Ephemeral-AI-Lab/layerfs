# #273 phase 4.5 implementation log

> Append-only record of implementing the [phase 4.5
> proposal](PHASE4.5-IMPLEMENTATION-SPEC.md). Step status here is a source and
> functional statement, never a speed result or a release admission.
> Checkpoint 5 stays **NOT_RUN**.

## Step 4.5.1 — private index v2 format and selected resolver

Source commit `91c9c4938` ("feat(#273): select the phase-4.5 private index v2
resolver"), first parent `941613f65` (the phase-4.5.0 proposal). New
attachments select v2; no v1 index magic, name or page is produced, adopted or
migrated, and no checkpoint-4 receipt is relabelled.

Implemented grammar and behavior (spec §4.1, §4.2, §6.1):

- `page.rs`: index magic `LFSAIDX2` at version 2 for leaves and branches,
  directory magic `LFSAHOT2`, unchanged `LFSAPAK1` version-1 pack records.
  `pages.rs` names index pages `a-index-v2-<id>-<epoch>` and directory pages
  `a-hot-v2-<id>-<epoch>`, and `create_verified` returns the authenticated page
  so a selector decodes exactly the bytes a reader would see.
- `keyed.rs`: branch cell `fence_len:u16, fence, target`; 17-byte target
  `tag:u8, first:u64, epoch:u64` (0 cold page/epoch, 1 hot slot/reuse epoch,
  any other tag refused); only the final child may carry the inherited bound as
  a zero-length fence, and non-final fences must be nonempty and strictly
  increasing. Largest leaf cell 788 bytes, largest branch cell 291 bytes.
- `hot_directory.rs`: fixed `8 + 64*32 = 2,056`-byte body, records 64,
  32-byte entries (reuse epoch, physical page, level, kind, reserved), zero
  vacant/reserved bytes, level 0 exactly for leaves and 1..7 exactly for
  branches, occupancy checked before any entry is exposed.
- `resolve.rs`: every selected get/floor/scan descends by fence partition. A
  hot target resolves only through that view's directory with the requested
  reuse epoch, kind and level; an absent slot, mismatched epoch or superseded
  kind refuses instead of falling back to another page. A per-descent visited
  mask rejects a repeated hot slot, and the eight-level bound holds during
  descent.
- `splice.rs`: generic v2 mutation prunes vanished children, splits at the
  encoded midpoint so both halves of an overflowing node keep at least
  `BODY_BYTES/2 - max_cell` bytes (1,196 leaf / 1,693 branch), keeps a replaced
  hot node's slot and reuse epoch, and stages replacements hot whenever the
  predecessor was hot, so no cold edge can conceal a current hot target.
- `index.rs`: one selected root plus one optional charged directory copy are
  installed in a single publication; a replaced directory copy joins the
  revision-pinned retirement list.

Not implemented at this step, and not claimed: no route admits a hot cursor,
so every publication still selects an empty directory. The directory grammar
and hot resolution path are implemented and format-covered only. Eligible
tiny-WRITE admission, balanced-carry proofs, selective normalization,
selecting-pin retirement cohorts and the incremental G1/G2 reconcile precharge
are steps 4.5.2 and 4.5.3.

### Verification at `91c9c4938`

| Check | Result |
| --- | --- |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --offline -p layerfs-workspace` (macOS host) | PASS |
| `cargo +1.85.1 zigbuild --release --target aarch64-unknown-linux-musl -p layerfs-workspace --tests`, then Docker `alpine:3.22` on an owned ext4 volume | `active_backing` 25/25 PASS; `readable` 18 PASS / 2 mounted-only ignored; `keyed_tree` 1; `attachment` 3; `backing_ownership` 6; `pieces_sequence` 36 PASS |
| `cargo +1.85.1 clippy --release --workspace --all-targets -- -D warnings` | PASS |
| `cargo +1.85.1 fmt --all -- --check` | PASS |
| `python3 core/tools/check_product_boundary.py` | PASS, 345 production Rust/SQL files |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 9 PASS |
| 14 registered `stage_route.py` public cases at this source | PASS; receipts in [evidence/phase4.5/](evidence/phase4.5/) |

The route cases are functional selections with `performance_claim=false` and
`cache_claim=null`. Each reuses an independent byte copy of the closed
64 MiB fixture, runs one owned Linux `ext4` volume in `rust:1.85.1-bookworm`
with `--cpus=2` and `LAYERFS_CONSTRUCTION_WORKERS=1`, and records complete
command wall. No row is a latency PASS or a control comparison.

| Case | Checks | Complete wall | Selected observation |
| --- | ---: | ---: | --- |
| `active_repeated` | 3/3 PASS | 6.28 s | 4,097 same-offset writes; 2 pack fetches, 17 index fetches, one index write |
| `active_separated4096` | 3/3 PASS | 16.90 s | 4,096 separated one-byte writes; 1,843,200 B before, 20,480 B after Commit |
| `active_generation` | 2/2 PASS | 0.85 s | G1 Stage beside G2 live bytes; both Commits |
| `active_many_file` | 3/3 PASS | 4.22 s | 128 files; two shared pack pages before, zero after Commit |
| `active_mounted` | 2/2 PASS | 0.84 s | mounted FUSE write/append, unmount, Commit, canonical bytes |
| `active_split_slot` | 2/2 PASS | 0.84 s | packed-slot split final byte oracle and clean close |
| `active_retained32` | 3/3 PASS | 1.46 s | 32 pinned generations; 274,432 B retained, 12,288 B released |
| `active_mixed_compact` | 3/3 PASS | 3.78 s | G1/G2 mixed sealed pool; 4 pack fetches, 1,099 index fetches |
| `active_mutation_compact` | 2/2 PASS | 3.88 s | ordinary WRITE removes a mixed sealed source page |
| `active_quota_refusal` | 2/2 PASS | 0.93 s | refusal keeps acknowledged bytes and exact charge |
| `active_quick_controls` | 3/3 PASS | 16.16 s | clean Commit skips the retained old journal |
| `active_namespace` | 2/2 PASS | 0.84 s | fresh directory/link/symlink and successor rename/unlink Commit |
| `active_close` | 2/2 PASS | 0.84 s | fresh-name Commit and exact clean close |
| `active_payload_refund` | 3/3 PASS | 0.81 s | large payload owned before Commit, refunded after |

Nonpassing and unrun: no route case failed, but the 14 cases are the whole
registered set this step exercised. Cases that need the unwritten 4.5.2/4.5.3
sources (hot-cursor admission, carry counts, selective normalization, cohort
retirement and the post-Commit hot continuation), every benchmark selection,
the mounted-process-spanning-Commit continuity proof and checkpoint 5 are
NOT_RUN. No performance sample, RSS/cgroup bound, `st_blocks` equality claim
for the hot path or release admission exists at this step.

### Production LOC

`91c9c4938`: core 64,548 → 65,136 (delta +588); reference 65,417 → 65,417
(delta 0); combined 129,965 → 130,553 (delta +588). First parent `941613f65`
compared with the committed tree, `python3 tools/production_loc.py --json`,
unchanged scope/exclusions from the checkpoint-3-5 ledger.

This evidence commit is documentation and receipt storage only: production LOC
delta 0.

## Step 4.5.2 — source-derived implementation constraints (not implemented)

Recorded while implementing 4.5.1, from the same stable source. These are
read-only source findings for the next session; they change no product byte and
make no implementation claim.

1. **One index revision per ordinary WRITE is enforced by the caller.**
   `filesystem/active_file.rs::publish_active_file_mutation` rejects any
   publication whose revision is not the Workspace's next revision, so hot
   admission, selective normalization and the mutation must be merged into one
   candidate publication, exactly as spec §8.1 requires. A separate admission
   publication, or admitting only after the mutation returns, would fail that
   check; admission that cannot fit must fall back to the generic route in the
   same revision rather than publish twice.
2. **A retained hot binding needs a local validity rule, and the obvious
   predecessor check is not sufficient.** After a leaf split at fence `K0`, the
   left node keeps slot `s` (spec §4.3: replacing a logical node retains its
   HotRef epoch) and the right node takes a new slot. A binding for key `K`
   whose recorded predecessor stayed left passes a predecessor-only check even
   when `K >= K0`, which would insert `K` into the wrong node and break parent
   routing. The safe local rules are: (a) an *update* binding is valid only
   while its key is still present in the retained leaf; (b) an *insertion*
   binding whose recorded successor cell is still present is valid when the
   recorded immediate predecessor is still the cell immediately before `K`'s
   insertion point; (c) an insertion binding with **no** successor cell in the
   leaf (the append-at-right-edge case, and the split case above) is valid only
   while the retained slot's content version is unchanged. A charged per-slot
   content version in the incarnation state, bumped on every slot replacement
   and updated by the cursor's own publication, makes (c) cheap; without it a
   split can move the advancing frontier to a right sibling while leaving the
   predecessor behind.
3. **Heating the whole root-to-leaf path costs up to `h` slots per key.** Spec
   §4.3 requires every ancestor of a current hot node to be hot, so admission
   rewrites each path node (the leaf itself can be selected in place because
   its page is unchanged, but each ancestor's child cell must change from cold
   to hot, which rewrites that ancestor). At the eight-level ceiling, one
   inode's I/E/P/R paths can consume roughly 32 slots before shared-path
   savings, so a 64-slot directory supports about two maximum-height files.
   Admission must therefore be best-effort and compute its full affected path
   union before allocating; refusal falls back to the generic route and must
   never enlarge a declared bound.
4. **Slot reuse requires normalizing the parent edge first.** A directory slot
   cannot be repointed to a different logical node while any selected parent
   cell still names its old `(slot, epoch)`: resolution would refuse and the
   tree would become unreadable. Eviction therefore cannot simply reassign a
   slot; it must first make the parent's cell cold (spec §6.2), which is the
   normalization pass. A bounded "normalize the hot nodes no live cursor
   retains, then clear the directory" publication frees the whole table at once
   and is amortized against admissions; it must also preserve the physical
   page's birth/current interval, because writing `Cold(selected physical
   child)` is not a retirement.
5. **Selecting-pin cohorts can decide enqueue-versus-release in `O(log G)`.**
   `Index::frozen_between(birth, retire)` already answers "does any frozen
   revision select this page" from the `frozen` map, so publication can release
   an unpinned owner immediately or enqueue it under the *latest* selecting
   revision without scanning the retired vector. Replacing the `O(J)` scan in
   `Index::maintain` with cohorts keyed by that revision is a contained change
   and is required before the per-WRITE retirement term in spec §9 can be
   claimed.

## Steps 4.5.2 / 4.5.3 — bounded hot publication and affected lifetime work

This product revision continues `91c9c4938`; the evidence follow-up pins its
exact commit. Checkpoint 5 remains **NOT_RUN**. Public route receipts are
reissued only after this product revision is committed.

Implemented:

- Charged selected node copies, at most eight inode cursors / 64 Hot slots /
  1 MiB resident reservations. I/D facts and acknowledged inode attributes
  flow through the filesystem wrappers; one indexed Node update replaces
  the resident-node scan. Shared P/R bindings follow the live pack tail.
- EOF and advancing Base/Zero writes retain exact gaps/suffix/source offsets.
  A binding checks the selected root and every ancestor epoch/content version;
  its own leaf version is deliberately not a validity condition. Existing
  I/D/P keys must remain present and insertion keys must stay in [lower,fence).
- One merged leaf candidate per changed slot, a matching pack and directory,
  and one revision. Carries use recorded parent slots. Branch terminal cells
  inherit the parent upper bound, including after right-side deletion; byte
  balancing counts the actual encoded terminal body. Page staging checks
  kind/level and Hot child levels before exposing a candidate.
- Admission and necessary normalization share the generic mutation, with
  no second publication or candidate-error fallback. Closed siblings are Cold;
  shared Hot closure stays selected. Normalization preserves a child's physical
  birth/charge, and reassignment increments incarnation slot high water.
- Complete quota reservations precede eligible staging: ordinary 6/7 files,
  split-only <=98, restricted merged boundary <=227. Optional cold admission
  refuses at a slot/byte/quota bound and leaves the generic route available.
- Index/directory/pack retirement selects the latest pin in [birth,retire).
  An unpinned owner gets its exact release attempt in the triggering mutation;
  final pin release visits only its cohort. Failed release remains charged
  custody. Birth comes from PageStore custody rather than a page reread.
- Frozen G1 reconcile scans/saved facts are prepared off the state gate and
  rechecked against the live inode revision. Intervening G2 extents survive.
  Prepared rows, cumulative maps/ordered vectors and compaction/reconcile
  clones are precharged. Upload still has charged O(E_f) extent scratch;
  affected installation and legacy/payload maintenance remain real work.

Focused backing checks at the final product implementation (not speed rows):
12,288 interleaved EOF/Base/Zero writes, 12,285 hot writes, 11,316 ordinary
noncarry rows, maximum 6 new active candidate files on those ordinary rows,
1 physical path admission, 1,003 carries, 100 total boundary seeks,
370,820 node visits, 59,629 index writes (including 12,288 directory writes),
12,288 pack writes. Ordinary rows assert zero root seeks/index/pack fetches;
leaf/branch split minima were 1,815 / 1,951 B (limits 1,196 / 1,693).
The three-file final physical sum was 4,755,456 B, with 234,840 B hot resident
reservations; this is not the one-file 3 MiB selection or an RSS claim.
Both selecting-pin release orders pass without unrelated retirement sweeps.
The corrected eviction proof primes 32 files, pins that composite view, then
adds 32 files; slot 6 advances epoch 4 -> 8 while the old view keeps its bytes.
Host Budget refusal preserves revision, acknowledged bytes and allocation.

Retained implementation failures, never relabelled:

1. The first Clippy check reported the explicit `emit` range parameters as
   8/7 arguments; a local lint annotation retained the concrete inputs.
2. Linux backing attempt 1: 26 PASS / 4 FAIL, 13.122 s complete wall. The
   read-only page diagnostic exposed stale root bindings and a finite root
   terminal fence after right-side deletion. Fixed in the shared binding and
   branch codec/grouping paths; the original failed files/logs remain evidence.
3. Linux backing attempt 2: 29 PASS / 1 FAIL, 34.321 s complete wall. The
   epoch fixture's original slots remained shared, so its selected before/after
   pair did not witness reassignment. Its directory already showed epoch 4
   in a later slot. The focused initial fixture correction also failed (0.766 s).
   A prospective 32-file prime then 32-file eviction establishes the pair and
   passes in 1.079 s. No performance arm was rerun or selected from these tests.

The functional source still requires its source-pinned public proof/evidence
follow-up and owner review before checkpoint 5. No performance sample,
release admission, full CPU O(1), constant-RAM Commit or main-lane #264
integration is claimed here.

### Public regression found after the hot proofs

At `f83a186dd`, both added public hot cases passed, and the first reissued
original case (`active_repeated`) passed. `active_separated4096` then passed
its one-file 3 MiB private-backing check but failed C5 with `Capacity`, after
C1 had already committed. Its failed receipt and runtime are retained until
recorded cleanup; that Commit outcome must never be resent.

The source-derived reservation diagnostic counts 8,192 E deletions and
4,096 R deletions in this declared alternating-write case. Before unrelated
resident allocations, the prepared deletion keys, update map, outer ordered
vector, two publisher clones, inner ordered clone and index scratch reserved
10,624,386 B against the unchanged 8 MiB Budget. This is a reservation/duplicate
ownership defect, not permission to grow the Budget or shrink the case.
The fix transfers the already charged C5 map into the publisher and moves its
keys/values into one sorted candidate, while keeping old/new map-vector
capacity charged. Shared prune paths now precharge touched logical-ID sets,
returned owner vectors and P-key additions through publication. A focused
ordinary-API reconcile Budget-refusal test preserves the selected revision,
bytes and physical charge without allocating candidate pages.

At `a2359620a`, all 14 original registered public cases pass, including the
unchanged 4,096-separated Commit. The additional `active_cleanup_failure`
case failed before any of its proof markers: corrupting a retired page's body
no longer causes release to fail, because release checks retained physical
identity/allocation rather than rereading dead body bytes. Its corrected
external fixture preserves the old allocated file under a retained name and
puts a different identity at its registered path. The existing Published
receipt/new-byte/Busy assertions remain, with an explicit failed-pack custody
count. This corrects the injection to a real custody failure; it changes no
product source, work bound, deadline or successful historical receipt.


## Steps 4.5.4 / 4.5.5 — frozen functional candidate and retained evidence

Frozen candidate/proof source `4ae36ad3a9c70b32b66c8280ac9496e9f1e345a9`,
last product change `a2359620a7966314fbb2a96c98e8da958df72c6c`.
The [frozen identity record](evidence/phase4.5/hot-publication-20260928/FROZEN-CANDIDATE.json)
pins product inputs, root `.cargo/config.toml`, locked dependencies, release
profile, driver/test/helper sources, executables, runtime image and fixture.
The [archive catalog](evidence/phase4.5/hot-publication-20260928/README.md)
retains byte-identical raw records and separate cleanup; no historical status
or identity is rewritten. Later evidence/status changes are documentation only.

| Step | Final state at this scope |
| --- | --- |
| 4.5.2 hot publication | DONE: charged EOF and advancing inherited Base/Zero frontiers, wrapper reuse and one composite publication |
| 4.5.3 boundary/lifetime | DONE: byte-balanced carries, affected normalization, epoch reuse, selecting-pin cohorts and precharged G1/G2 reconcile |
| 4.5.4 public proof | DONE: original 14 cases plus three focused custody/hot/continuity cases; 17/17 cases, 42/42 named checks PASS |
| 4.5.5 handoff | DONE: architecture/status, raw evidence, per-commit LOC and frozen candidate; owner review is pending |
| Checkpoint 5 | NOT_RUN: zero performance samples, no enforced cold-cache or release admission |

### Final registered public functional set

All rows below have `performance_claim=false` and `cache_claim=null`. Complete
walls include fixture byte-copy/runtime/oracle/cleanup and are recorded for
functional provenance only; they are not speed arms or latency gate results.
Each case reuses the same closed prepared master, using an independent writable
byte copy and fresh live history producer. No prepared master was regenerated.
All rows use locked release executables, owned Linux ext4 in the same pinned
`rust:1.85.1-bookworm` image, two CPUs and one construction worker. Source,
product inputs, driver, test/helper source and test binary identities match
across all 17 rows. The metadata-only scope correction names the Commit and
mounted subsets actually executed; no deadline or case changed.

| Case and receipt | Named checks | Complete functional command |
| --- | ---: | ---: |
| [`active_repeated`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_repeated/result.json) | 3/3 PASS | 5.982 s |
| [`active_separated4096`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_separated4096/result.json) | 3/3 PASS | 6.447 s |
| [`active_generation`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_generation/result.json) | 2/2 PASS | 0.928 s |
| [`active_many_file`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_many_file/result.json) | 3/3 PASS | 4.176 s |
| [`active_mounted`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_mounted/result.json) | 2/2 PASS | 0.933 s |
| [`active_split_slot`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_split_slot/result.json) | 2/2 PASS | 0.935 s |
| [`active_retained32`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_retained32/result.json) | 3/3 PASS | 1.497 s |
| [`active_mixed_compact`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_mixed_compact/result.json) | 3/3 PASS | 3.524 s |
| [`active_mutation_compact`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_mutation_compact/result.json) | 2/2 PASS | 3.409 s |
| [`active_quota_refusal`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_quota_refusal/result.json) | 2/2 PASS | 0.907 s |
| [`active_quick_controls`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_quick_controls/result.json) | 3/3 PASS | 6.533 s |
| [`active_namespace`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_namespace/result.json) | 2/2 PASS | 0.945 s |
| [`active_close`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_close/result.json) | 2/2 PASS | 0.935 s |
| [`active_payload_refund`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_payload_refund/result.json) | 3/3 PASS | 0.898 s |
| [`active_cleanup_failure`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_cleanup_failure/result.json) | 2/2 PASS | 0.878 s |
| [`active_hot_publication`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_hot_publication/result.json) | 3/3 PASS | 3.372 s |
| [`active_hot_continuity`](evidence/phase4.5/hot-publication-20260928/phase45-route-4ae36ad3a/active_hot_continuity/result.json) | 2/2 PASS | 1.031 s |

The one-file separated row has 4,096 edits of the full 8,194-byte file;
physical private files are 1,814,528 B before Commit and 24,576 B after,
52 pack pages become zero, charge equals `st_blocks*512`, and the named clean
close returns zero files. This is the design-space case, not #248's registered
4,097-write performance gate. The 128-file row uses two shared pack pages
before Commit and zero afterward. All byte oracles and named close assertions
remain; the failed-cleanup row intentionally returns Busy while retaining its
Published receipt, new bytes and failed-pack charge.

The public hot count row keeps all 1,024 interleaved writes: one empty-root
construction plus two cursor admissions plus 1,021 hot writes. Of these,
949 ordinary noncarry publications assert no root seek or index/pack fetch;
73 carry rows pay their boundary work. Totals: 29,770 node visits, 4,114 index
writes including 1,023 directory writes, 1,024 pack writes, 5,049 retirement
inspections, 1,023 separately counted legacy routine scans and zero legacy
lookup scans. Current hot reservations are 160,086 B; physical files are
368,640 B at the recorded point. These are counted mechanism facts, not a
full CPU O(1), machine-RAM or performance statement.

The full 326,300-byte inherited oracle is read in bounded public windows,
without reducing the fixture. G1's native root and G2 live bytes remain exact;
an unrelated G2 cursor immediately continues hot after C5 with no root seek
or normalization, and the affected inode readmits before its continued
sequence. Payload/overlap, truncate/Zero grow, aliases, rename/unlink/forget,
held orphan bytes across two Commits and final canonical names/bytes pass.
The mounted continuity row has one PID 37 / inode 2 through READY, heartbeat,
BC before capture, D during SaveFile, E at the C5 result gate and F after
Commit. Native G1 bytes end in BC; live G2 ends in BCDEF. There is no
pause/restart/remount; unmount occurs only after the child exits.

### Scoped checks and proof coverage

The [final Host checks](evidence/phase4.5/hot-publication-20260928/phase45-transfer-workspace-checks-20260928-1/result.json)
run the following exact commands from this worktree root, with
`LAYERFS_CONSTRUCTION_WORKERS=1`:

```sh
cargo +1.85.1 test --release --manifest-path core/Cargo.toml --locked --offline -p layerfs-workspace
cargo +1.85.1 clippy --release --manifest-path core/Cargo.toml --locked --offline --workspace --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
cargo +1.85.1 build --release --manifest-path core/Cargo.toml --locked --offline -p layerfs-server -p layerfs-daemon -p layerfs-bridge --bins --features layerfs-bridge/native --example public_key
cargo +1.85.1 zigbuild --release --manifest-path core/Cargo.toml --locked --offline --target aarch64-unknown-linux-musl -p layerfs-workspace --tests
```

All PASS: Host Workspace, warning-denying Clippy/all targets, fmt, boundary
349 production Rust/SQL files, 9 guard self-tests and both release builds.
The [final Linux ext4 checks](evidence/phase4.5/hot-publication-20260928/phase45-core-transfer-20260928-1/result.json)
are `active_backing` 32/32, readable 18 PASS/2 mounted-only ignored, keyed tree
1, attachment 3, backing ownership 6 and pieces sequence 36 PASS. The backing
suite's complete command wall is 34.970 s; this is a scoped correctness/count
suite, not a benchmark sample or the independent checkpoint-5 speed verifier.
Its recorded precommit staged product blobs match `a2359620a`; later source
changes affect only external stage tests and documentation. The original
dirty-source metadata remains unchanged. The final stage fixture change is
separately compiled in locked release ARMv8 and format/diff checked.

| Required mechanism | Ordinary-API or public proof |
| --- | --- |
| Growing EOF and inherited Base/Zero, shared nodes/tail, no ordinary root search or ancestor page write | `hot_eof_base_zero_and_shared_frontiers_have_bounded_publication`: 12,288 writes, 12,285 hot, 11,316 ordinary, max 6 new active files; public hot row counts wrappers/legacy separately |
| Actual leaf/branch carry occupancy, promotion, gap/suffix offsets | Same backing case and existing overlap/read/truncate cases; min leaf 1,815 B / branch 1,951 B against 1,196 / 1,693 limits; full public inherited oracle |
| Generic overlap/Payload/attrs/alias/namespace transition | backing overlap/large/metadata cases, readable attribute/notification cases and public hot/split/namespace/mounted cases |
| Cursor ceiling, selective eviction, slot epoch reuse and frozen directory | `hot_eviction_epoch_reuse_and_frozen_directory_keep_exact_bytes`: 8 cursors, slot 6 epoch 4 → 8, old bytes intact; current hot node/reservation ceilings asserted |
| Selecting-pin lifetime, both release orders and no unrelated retired sweep | `selecting_cohorts_only_visit_the_releasing_pin_in_both_orders`, capture/read-view and 32-generation cases; exact files/charge/refund assertions |
| G1 SaveFile/C5 beside live G2 and post-Commit continuation | Both new public hot cases; independent old/new heads and same mounted PID/fd/inode |
| Atomic quota/Budget refusal and candidate failure | backing quota/corrupt-tail/denied-write plus `hot_budget_refusal_keeps_revision_bytes_and_allocation_exact` and `reconcile_budget_refusal_keeps_the_acknowledged_selection`; public quota row |
| Published notification/cleanup failure and custody | readable local notifier-failure test; public cleanup physical-identity conflict retains accepted receipt/new bytes/failed owner and refuses close |

The Budget refusals hold genuine reservations while the live cache/descriptors
remain resident. Old selection, revision and allocation stay intact; reconcile
refusal stages no candidate page. No bound is enlarged and no unsupported
source capability becomes a silent no-op. The 1 MiB figure is charged current
hot reservations, not total heap/RSS/cgroup. Frozen directories, custody,
resident Nodes and O(E_f) upload/patch scratch remain separately growing
charged domains. Affected installation still executes under the state gate;
legacy/input and large-payload maintenance are counted or qualified separately.

### Retained nonpassing attempts

All nine public FAIL receipts, three nonpassing backing/epoch attempts and the
FileExt compile failure remain in the archive. Later manual cleanup records
PASS for removal of only their owned containers/volumes; original FAIL and
RETAINED_FOR_DIAGNOSIS fields are not changed. The first Clippy `emit` argument
count diagnosis is recorded above/session-only, not invented as a raw file.

| Public source / case | Passed markers before FAIL | Cause and resolution |
| --- | ---: | --- |
| `0513f8a1a` / hot publication | 0/3 | Whole-file READ exceeded 128 KiB; chunk full oracle, keep 326,300 B |
| `788de441b` / hot publication | 0/3 | Unsupported ordinary >980 assumption ignored real carries; count ordinary/carry relation |
| `83f69e50a` / hot publication | 0/3 | 1,021 hot writes excluded one empty-root construction; count construction/admissions explicitly |
| `1131c2317` / hot publication | 1/3 | Unrelated cursor fell generic after C5; refresh shared P/R on generic publication without seed |
| `998f256a0` / hot publication | 2/3 | Fresh unbound inode incorrectly declared to C1; retain private saved facts and omit unbound canonical introduction |
| `998f256a0` / continuity | 1/2 | Explicit Local lookup still held at close; forget it |
| `ea7007201` / hot publication | 2/3 | Remaining explicit Local lookups blocked close; forget all known lookup refs |
| `f83a186dd` / separated4096 | 1/3 | Known C1 success, C5 duplicate-patch reservation Capacity; move charged map/keys, never replay Commit |
| `a2359620a` / cleanup failure | 0/2 | Dead-body corruption was no longer a release fault; inject physical path identity conflict, keep all custody assertions |

### Production LOC for every continuation commit

The [exact comparison record](evidence/phase4.5/hot-publication-20260928/phase45-commits-loc.json)
uses `tools/production_loc.py` SHA-256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`,
`git archive` of exact first-parent/committed `crates core/crates` trees, and
the same runtime Rust/SQL classification and inline-test exclusions. Identical
complete production-input blobs share the exact counter result. All commit
message comparisons were confirmed against the resulting commits. Reference
is 65,417 throughout; no relocation, scope reduction or legacy retirement.

| Commit | Core before → after | Combined before → after | Signed delta |
| --- | ---: | ---: | ---: |
| `0513f8a1a` | 65,136 → 67,120 | 130,553 → 132,537 | +1,984 |
| `788de441b` | 67,120 → 67,120 | 132,537 → 132,537 | +0 |
| `83f69e50a` | 67,120 → 67,120 | 132,537 → 132,537 | +0 |
| `1131c2317` | 67,120 → 67,120 | 132,537 → 132,537 | +0 |
| `998f256a0` | 67,120 → 67,140 | 132,537 → 132,557 | +20 |
| `ea7007201` | 67,140 → 67,145 | 132,557 → 132,562 | +5 |
| `f83a186dd` | 67,145 → 67,145 | 132,562 → 132,562 | +0 |
| `a2359620a` | 67,145 → 67,158 | 132,562 → 132,575 | +13 |
| `4ae36ad3a` | 67,158 → 67,158 | 132,575 → 132,575 | +0 |

Continuation total: Core 65,136 → 67,158 (+2,022), reference 65,417 unchanged,
combined 130,553 → 132,575 (+2,022). This evidence/status commit is docs only:
production LOC 132,575 → 132,575 (delta 0), same Core/reference subtotals.

Nonpassing/unrun qualification: the full Core C1 test command was not rerun;
retain checkpoint 4's two `filesystem_ordering` failures (19 objects versus
18) until their lane resolves them. Two mounted-only readable tests remain
ignored. Concurrent SDK Exec and #264 namespace/ancestry integration are out
of scope. Every checkpoint-5 matched performance/resource selection, cold
cache admission, hard RSS/cgroup bound and release admission is NOT_RUN.
Historical #248 FAIL and #271 cache-INELIGIBLE remain unchanged. No CI,
preflight, latency PASS, 2× speed claim or constant-RAM Commit is claimed.

The [frozen handoff](HANDOFF-PHASE45-FROZEN.md) supersedes the earlier
continuation prompt. After owner review, the [checkpoint-5
prompt](HANDOFF-CHECKPOINT5.md) retains the original registry, control,
cache/workers/deadlines and one-sample policy. Main-lane integration does not
block #273; complete #273 before that lane merges.
