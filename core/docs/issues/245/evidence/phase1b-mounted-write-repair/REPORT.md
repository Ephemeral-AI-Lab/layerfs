# #245 Phase 1B continuation: mounted write-path repair and the frozen control

> **Status:** the mounted ordinary-shell route is repaired on this branch and the
> frozen #243 selection passes all four cases functionally at `fdfc41032`. That
> campaign is the pre-optimization **control** for #245 package F. Packages D
> (streaming transport), E (generations) and the candidate arm of F are
> **NOT_RUN**. No latency is claimed: every row is `INELIGIBLE` under the
> frozen cache contract, exactly as the registry declares.

Source identity: `fdfc41032` on `4ef71e596` (the Phase 1B continuation handoff)
in the `codex/issue245-range-cow-plan` worktree. Production LOC per commit is in
§6; the reproduction commands are in §7.

## 1. What triggered this round

The handoff's first evidence blocker asked for a re-prepare of the retained
#243 state. The first `shell_package.py prepare` at the handoff source failed:
the seed command — a one-byte `dd` overwrite of an existing file through the
mounted route — exited 1. That failure is retained at
`benchmark-results/fs-bench-pro/issue245-shell-package-v1-prepared-01/`
(the failed `masters/package/seed.*` output and the incomplete prepared tree).
The Phase 1B report had recorded that the mounted route was never run on the
extent-sequence source; this was that gap opening.

## 2. The eight defects, and how each was found

All eight share one cause class: the extent-sequence change was gated only by
single-leaf sequences in a bounded in-memory page store, so every path that
only a multi-leaf tree, the ownership plumbing or a mounted mount reaches was
unexercised. Each was pinned by a labelled diagnostic — FUSE tracing and error
diagnostics on, and for the internal sites a dev-profile musl daemon with
temporary step markers that were removed before the commits — never by
re-running a campaign arm.

| # | Defect | Site (before the repair) | Failure it produced |
| --- | --- | --- | --- |
| 1 | `replace` refused any partial splice onto a NULL root, so the first local edit of a base-backed version (never-edited file, or capture-converted) never folded | `metadata_pieces.rs` NULL branch | every first write to an existing file: `EIO`, the failed seed |
| 2 | the write path built replacement parts only when the version had no stored sequence — a later edit spliced an empty replacement — and prepended the implicit base into the parts | `filesystem/write.rs` parts block | bytes deleted instead of replaced on the second edit |
| 3 | `Fold::close` missed the trailing deviation a truncation produces | `metadata_pieces.rs` | commit-time `Io` at the lowering count check after a truncate |
| 4 | the `u16::MAX` unknown-count marker was documented but never produced; a shared subtree recorded an undercount and a partial replacement total | `metadata_pieces.rs` | commit-time `Io` for any multi-leaf splice |
| 5 | shared subtrees charged no length, the branch descend missed the merge-point clamp, and a shared page following a folded leaf broke the parent's page order | `metadata_pieces.rs` walk | structural `Io` for any multi-leaf splice; reordered trees |
| 6 | the cursor demanded absolute top-anchored branch levels while `pack` writes relative ones, and sibling advance re-read the previous child | `metadata_pieces.rs` cursor | `Io` reading any multi-leaf tree; wrong leaves after a sibling step |
| 7 | `write_raw_page` took a new page's ownership edges from the window **after** the ledger write had already reused that window page | `ownership.rs` | every pieces-page publication: `Io` (found after 1–3 were fixed) |
| 8 | routine reclaim decoded the page under cleanup with the keyed-cell decoder and took cell-only edges; the failed decode quarantined a healthy arena | `metadata_reclaim.rs` cleanup step | the first dropped root with an extent page: cleanup `Io`, then every later mutation and the Commit's preparation `Busy`, and a retained ~5.5 s shutdown |

Defects 1–7 were repaired in `76c832f0c`; defect 8 in `fdfc41032` after the
first control campaign exposed it (§4). The algorithm pin
[59-length-indexed-extent-sequence.md](../../../../architecture/proposal/fuse-workspace-snapshot-overlay/59-length-indexed-extent-sequence.md)
was updated in the same commit as defects 1–6: the implicit-base rule, the
relative branch levels, the carried replacement accounting, the trailing edit
and the shared-subtree ordering. The pieces cursor moved to
`backing/metadata_cursor.rs` to stay under the 999-line ceiling.

## 3. The focused gate

`crates/layerfs-workspace/tests/pieces_sequence.rs` grew from 12 to 19 cases.
The additions pin the mounted shapes a single-leaf fixture can never reach:

| Case | What it establishes |
| --- | --- |
| `a_first_edit_folds_into_one_implicit_base_read` | the one-byte-overwrite shape of the failed seed: `[L,B]`, exact counts, and a second edit that replaces bytes instead of deleting them |
| `a_first_edit_inside_an_implicit_base_retains_both_sides` | a mid-file edit keeps prefix and tail; a whole-file replacement is one run |
| `an_implicit_base_append_past_eof_fills_the_gap` | a write past EOF keeps the base, fills zeros, appends |
| `an_implicit_base_truncate_and_extension_are_exact` | a truncation records exactly the one trailing deletion edit; an extension owns zeros |
| `an_empty_sequence_after_a_full_shrink_takes_a_whole_rewrite` | a full shrink publishes a NULL root; the rewrite replaces no base bytes |
| `an_implicit_base_replacement_beyond_the_base_is_refused` | the interval must stay inside the implicit base; a refusal writes no page |
| `a_shared_subtree_records_the_unknown_count_and_the_exact_total` | a real three-leaf tree (300 non-mergeable extents): the splice shares two leaves, records `u16::MAX`, carries the exact replacement total, rewrites only the folded leaf and its branch, and the older root still reads |

`19 passed; 0 failed` on musl in 0.05 s, before and after the module split.

## 4. The two control campaigns (both retained)

The #243 registry is frozen: the same four cases, one attempt per case, the
sealed driver and independent verifier, and the declared clone method.

| Campaign | Source | Rows |
| --- | --- | --- |
| `issue245-shell-package-v2-control-01` | `76c832f0c` (defects 1–7 repaired) | `mixed-refresh-v1` **FAIL** (`cp: write error: Resource busy`), `overwrite-4k-v1` INELIGIBLE (functional PASS), `repeated-one-byte-v1` **FAIL** (3 of 16 writes; Commit `Busy` in `Preparing`), `failed-command-no-commit-v1` INELIGIBLE (functional PASS) |
| `issue245-shell-package-v3-control-01` | `fdfc41032` (defect 8 repaired) | all four **INELIGIBLE** — functional PASS, cleanup PASS, verifier PASS |

The FAIL rows of the first campaign are the recorded evidence that located
defect 8: one labelled diagnostic (§2) showed the third write's payload
acquisition failing inside routine reclaim, quarantining the arena, which made
every later mutation answer `Busy` — the `cp` EBUSY and the stuck Commit were
the same quarantine. The ~5.5 s retained shutdown that the Phase 1A round
could not attribute is the same stuck cleanup: after the repair the
post-command cleanup is ~0.5 s in every passing row.

### The qualifying control (the F pre-optimization arm)

`benchmark-results/fs-bench-pro/issue245-shell-package-v3-control-01/`,
prepared at `benchmark-results/fs-bench-pro/issue245-shell-package-v3-prepared-01/`
(image `sha256:9f76aefe535a9a22daf9b2e6a0df30779dc3c08798864485c98b78c59accf48b`,
registry sha `1a7e1a3f…`, one construction worker, `shutil.copyfile` clone).

| Case | Functional | Cleanup | Verifier | Complete command | Commit called |
| --- | --- | --- | --- | --- | --- |
| `mixed-refresh-v1` | PASS | PASS | PASS 0.09 s | 2.20 s (limit 25) | yes |
| `overwrite-4k-v1` | PASS | PASS | PASS 0.16 s | 0.82 s (limit 15) | yes |
| `repeated-one-byte-v1` | PASS | PASS | PASS 0.09 s | 1.09 s (limit 15) | yes — `write=16 open=16`, the passing rounds' exact shape |
| `failed-command-no-commit-v1` | PASS | PASS | PASS 0.05 s | 5.88 s (limit 15) | **no** — the failure case still publishes nothing |

Every latency cell is `INELIGIBLE` by the frozen cache contract (container
FUSE backing uncontrolled); no numerical target is frozen yet. The comparative
target for package F must be frozen from these receipts before any candidate
attempt at a post-D/E source.

## 5. What is not established

- Package D (streaming transport), package E (generations and reconcile) and
  the candidate arm of F are untouched; the handoff's ceilings
  (256 edits, 8 MiB replay, 1,024 pieces) still stand.
- The Linux-gated native-service suites still need the route harness (the
  handoff's second blocker); the mounted evidence here is the shell route.
- #232's 56 shapes, the deferred load-bearing cases and the namespace
  frontier limits (128 dirty / 128 names / 32 KiB) remain open.
- The `Exec Unknown` five-second diagnosis named in the handoff was not
  reproduced in this round; the ~5.5 s cleanup correlation is now explained
  (defect 8) but that is a different observation.

## 6. Production LOC

| Commit | Before | After | Delta | Scope |
| --- | --- | --- | --- | --- |
| `76c832f0c` | 56,035 | 56,156 | **+121** | core replacement product; legacy 68,728 unchanged; combined 124,763 → 124,884 |
| `fdfc41032` | 56,156 | 56,160 | **+4** | core replacement product; legacy unchanged; combined 124,884 → 124,888 |

Method: `python3 core/tools/production_loc.py --root <snapshot>` against
`git archive HEAD` for before and `git checkout-index -a` for after; both
commits re-verified against the committed tree.

## 7. Reproduction

```sh
# The focused gate (musl, in-memory page store)
cargo +1.85.1 zigbuild --manifest-path core/Cargo.toml --locked --offline \
    --target aarch64-unknown-linux-musl -p layerfs-workspace --test pieces_sequence
docker run --rm -v "$PWD/core/target/aarch64-unknown-linux-musl/debug/deps:/d:ro" \
    alpine:3.22 /d/pieces_sequence-<hash> --test-threads=1

# The frozen selection, one attempt per case, at the committed source
python3 core/benchmark/fs-bench-pro/shell_package.py prepare \
    --output benchmark-results/fs-bench-pro/<fresh-prepared-dir>
python3 core/benchmark/fs-bench-pro/shell_package.py run \
    --prepared benchmark-results/fs-bench-pro/<fresh-prepared-dir>/prepared.json \
    --output benchmark-results/fs-bench-pro/<fresh-campaign-dir>

# A labelled diagnostic (not a sample): clone one closed master, run the exact
# command through the public route against a trace-enabled image, then the
# sealed verifier — the shape used to pin defects 1–8.
```

The diagnostic images used in this round (`issue245-repair1`, `issue245-repair2`,
`issue245-repair2dbg`, `issue245-repair3`) are retained locally; the campaign
images are recorded in each `prepared.json` and receipt.
