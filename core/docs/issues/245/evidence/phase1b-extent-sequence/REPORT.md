# #245 Phase 1B: length-indexed extent sequence and bounded cursor

> **Status:** packages A, B and C are implemented and pass their own focused
> gate on the committed source. The mounted ordinary-shell route has **not** been
> re-run on this source, packages D, E and F are **NOT_RUN**, and no latency row
> exists. No release admission is claimed.

Source identity: `466d6da6a` on `30a280e7b` (the Phase 1B handoff) in the
`codex/issue245-range-cow-plan` worktree. Build, binary, harness and image
identities are in [`IDENTITIES.txt`](IDENTITIES.txt); the raw run output is
[`attempt-01.stderr`](attempt-01.stderr).

## 1. What was implemented

| Package | Gate | Status |
| --- | --- | --- |
| A. Versioned length-indexed page format | versioned read, plus a refused old-format decode | **PASS** |
| B. COW path update | focused write/resize/read tests for overlap, holes, append, truncate, piece boundaries, old generations and rollback | **PASS** |
| C. Cursor | a Commit walk that never materializes a whole file | **PASS** |
| D. Transport coordinator | exact final bytes with no unbounded `Vec<Vec<u8>>` | **NOT_RUN** |
| E. Generation capture and reconciliation | mounted three-generation observation | **NOT_RUN** |
| F. Freeze and verify | every registered cell reported | **NOT_RUN** |

The format, the replacement algorithm and the cursor are described in
[the architecture document](../../../../architecture/proposal/fuse-workspace-snapshot-overlay/59-length-indexed-extent-sequence.md).

## 2. The gate

One Linux run of the new external suite, on the committed tree:

```sh
cargo +1.85.1 zigbuild --manifest-path core/Cargo.toml --locked --offline \
    --target aarch64-unknown-linux-musl -p layerfs-workspace --test pieces_sequence
docker run --rm -v "$PWD/core/target/aarch64-unknown-linux-musl/debug/deps:/d:ro" \
    alpine:3.22 /d/pieces_sequence-<hash> --test-threads=1
```

`12 passed; 0 failed; 0 ignored`, in 0.03 s. The suite drives the production
traversal through a bounded in-memory page store that speaks the identical page
format, so every case is checkable without a mount, a Store or a service:

| Case | What it establishes |
| --- | --- |
| `reads_a_sequence_in_order_with_derived_starts` | a leaf's extents are read in order with starts derived from subtree lengths, not stored |
| `overwrites_inside_one_extent_without_touching_its_neighbours` | a middle overwrite splits only the extents it touches; the local edit after it and the canonical tail are retained byte-exact |
| `covers_a_hole_and_a_position_past_the_end` | a write past EOF fills the gap with `Zero`, a tail replacement keeps the retained head, and a truncation keeps exactly the hole |
| `appends_at_the_end_and_extends_with_zeros` | an append at EOF and a zero extension both land in order, with the edit count and replacement bytes reported |
| `refuses_a_replacement_beyond_the_base_or_past_max_file` | a canonical read past the recorded base length and a length past `MAX_FILE` are refused, and a refusal writes no page |
| `splits_an_extent_larger_than_a_leaf_into_page_sized_parts` | an extent above `MAX_EXTENT` is stored as adjacent parts, a page boundary and not a new logical byte |
| `an_unchanged_old_generation_stays_readable_after_a_splice` | the older root still reads exactly the sequence it published |
| `a_shared_subtree_is_not_rewritten` | rewriting the first 64 bytes of a 700-extent file writes at most 3 pages; every other page is still the one the previous root owns |
| `a_cursor_seeks_and_reads_only_the_path_it_needs` | a lookup at offset 40,000 reads at most 4 index pages |
| `one_cursor_walk_never_holds_the_whole_sequence` | a 3,000-extent walk visits every byte in order from a bounded cursor |
| `refuses_a_page_that_does_not_declare_the_length_indexed_format` | a body relabelled as the pre-length-indexed form is refused by `PageKind::of` |
| `a_refused_splice_leaves_the_published_sequence_unchanged` | after a refusal the old root still reads its sequence and the store holds exactly the pages the first build wrote |

## 3. What is not established

The suite is a **component** gate on the page store and the traversal. It is not
a mounted proof and it does not replace one:

- The ordinary-shell route (`WorkspaceApi::exec` → `/bin/sh -c` → POSIX/VFS on
  the mounted Workspace → explicit `Commit`) has **not** been run against this
  source. The retained Phase 1A attempts stay the newest mounted evidence, and
  they were taken before this change.
- The Linux-gated suites in `crates/layerfs-workspace/tests/` compile for the
  musl target but need the live native service fixture; they were **not**
  executed. On the host (macOS) those suites compile out, as `core/AGENTS.md`
  records.
- The Commit transport still enforces the 256-edit, 8 MiB replay and 1,024-piece
  ceilings; C1 and the server still materialize replacement buffers. Package D
  is unstarted.
- `reconcile_commit` can still return `Busy` after a successful Store
  publication; package E is unstarted.
- No Phase 1 performance selection exists and no latency number is reported.

## 4. Honest complexity statement

The splice folds the extents of the replaced interval and the leaves that hold
them, copies their ancestors and shares every other page: `O(H + K)` extents for
one callback plus the bytes the command actually sent. Two limits on that claim
are recorded here rather than left implicit:

- A call's replacement extents are materialized before the splice, so a
  complete-file construction is bounded by the file rather than by one frame.
  Package D is what removes that.
- `Inode::edits` and `Inode::replacement` are recorded exactly only when the
  splice folded the whole sequence; when it shared an untouched subtree the edit
  count is recorded as `u16::MAX` and lowering derives the exact value from the
  sequence itself. `Commit` lowers such a file rather than reusing its base.

## 5. Checks at this source identity

| Command | Result |
| --- | --- |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --offline --workspace --all-targets` | exit 0 on the host (Darwin ARM64); Linux-gated mounted suites compile out |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --offline --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | exit 0 |
| `python3 core/tools/check_product_boundary.py` | exit 0, 293 production files scanned |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 9 tests, OK |
| `cargo +1.85.1 zigbuild ... -p layerfs-workspace --tests` (musl) | exit 0; every workspace test target builds |
| `cargo +1.85.1 zigbuild ... --workspace --all-targets` (musl) | **FAILED**: `layerfs-history` test targets cannot link `-lsqlite3` for musl on this host. Unrelated to this change and recorded rather than worked around |

Production LOC: **55,134 → 56,035** (delta **+901**); legacy reference 68,728
unchanged; combined **123,862 → 124,763**. Method:
`python3 core/tools/production_loc.py --root <snapshot>` against
`git archive HEAD` for before and `git checkout-index -a` for after.
