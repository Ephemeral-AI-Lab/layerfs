# #232 edit complexity: historical G2 and current Exec/FUSE route

> **Status:** Research; informative and not a product contract.

This is a static source trace plus a reading of retained evidence. It adds no
performance sample and does not change the #232 cache contract or target. The
historical reference is the #152 **C1/G2 candidate** at
`8b5e0955e808ab04fe0ce62e5e28f4b8ff94c807`, which reported 5.63 ms for
`overwrite-middle-4k-on-1mib-ops-1`; the final `v0.1.6` tag resolves to
`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`. Current source was traced at
`b0bd104228fed66726867302aa37fd39942b0b64`. The #232 scenario-v2 samples
were collected earlier at `1b4dfdf7efc702e67fe335ea3cad6a0d50fd550d`;
subsequent changes touched reporting, documentation, and verifier formatting,
not the measured product edit path. See the [#152 report][g2-report], [#232
baseline][v2-report], and [56 retained rows][v2-rows].

## Terms and limits of the claim

Let `N` be total file bytes, `k` replacement bytes, `M` the suffix bytes moved
by an insert/delete implementation, `P` Workspace overlay pieces, `E` canonical
content extents, and `C` actual FUSE write callbacks. A fixed-size local edit
should avoid work proportional to `N`; tree lookup and update can still cost
`O(log E)` or `O(log P)`. Four file sizes and one sample per case cannot prove an
asymptotic bound. The source trace below identifies actual loops and data
structures; the timings show where their cost appeared in this campaign.

The G2 timer contains direct SDK range Edit followed by explicit Commit, with
Workspace End outside the timer ([exact G2 timer][g2-timer]). #232 times
`WorkspaceApi::exec` of a shell command that edits through FUSE followed by
explicit Commit ([#232 timer contract][spec]). They have similar start/end
events and the same intended final bytes, but different mutation operations
and cache states. The G2 number remains an aspirational engineering target,
not a matched regression ratio.

## Historical C1/G2 path at `8b5e0955e`

| Step | Source-derived work | Dependence on total file size |
| --- | --- | --- |
| SDK Edit submission | One `edit_workspace_file_range` sends one edit group, including the `k` replacement bytes, through the live backing protocol ([SDK][g2-sdk], [wire][g2-wire]). | At least `O(k)` transport bytes; no shell command or FUSE WRITE. |
| First equal-length overwrite of a committed base | The piece tree can store one bounded compact splice descriptor ([piece edit][g2-piece]). | Constant-size metadata for this special first overwrite, plus path/lookup and `k` byte work. This is not an `O(1)` bound for the entire Edit→Commit route. |
| Insert/delete and later splices | A persistent treap splits/merges pieces instead of copying the untouched suffix ([piece edit][g2-piece]). | Expected work follows tree height, commonly `O(log P)` plus `k`; a strict worst-case logarithmic bound is not established for this treap. |
| Commit | Construction visits `P` pieces and groups changed ranges; content construction scans replacement bytes and edits affected extent-tree paths ([construction][g2-construction], [rope edit][g2-rope], [rope paths][g2-rope-paths]). | `O(P + k + affected mapping paths)` plus Store, transport, and publication work. For one local edit with small `P`, the mapping work is approximately logarithmic in `E`, not strictly constant. |

The [historical G2 ledger][g2-ledger] reports 4 KiB of live non-base bytes and
4 KiB of Commit CDC scanning for the fixed 4 KiB cases across 1–500 MiB, with
zero FUSE kernel write bytes and zero spool-write bytes. These counters support
localized measured work; flat wall times alone would not establish it.

There is an important conditional exception. An opened FUSE inode can be marked
cached ([cache mark][g2-cache-mark]). For a length-changing edit, the owner can
set reconciliation through EOF and read/store the affected suffix in steps
([range][g2-cache-range], [reconciliation][g2-reconcile]). That branch is
`O(M)` when the inode is cached. The retained G2 summary does not establish
whether it ran for each structural case. Accordingly, **G2 is not
unconditionally `O(log N)` for every cache state**, even though its logical
piece splice avoids suffix copying.

## Current #232 Exec/FUSE route

The SDK sends one authenticated `WorkspaceExec` control operation; the daemon
runs `/bin/sh -c` in the mount; the benchmark tool uses positional writes,
`set_len`, or an in-place shift ([SDK Exec][v2-sdk], [daemon Exec][v2-daemon],
[tool][v2-tool]). The shell command text is not classified as an edit by the
SDK or daemon. Its syscalls reach ordinary FUSE [WRITE][v2-fuse-write] or
[SETATTR][v2-fuse-setattr] callbacks. Commit is a separate SDK call.

| Operation | Current source-derived work | Implication |
| --- | --- | --- |
| One 4 KiB overwrite | One observed FUSE WRITE. Workspace materializes, splices, and rebuilds all `P` overlay pieces for that write ([write path][v2-write], [pieces][v2-pieces], [piece index][v2-index]). Content Edit follows affected extent-tree paths ([content edit][v2-content]). | `O(k + P)` overlay work plus tree-path, control, process, and storage costs. `P` stays small after this single edit, so there is no source-visible `N`-byte Edit loop. No strict whole-route `O(1)` proof follows. |
| Truncate/extend | One size SETATTR, then overlay mutation. | No suffix byte copy; overlay work still depends on `P`. |
| Insert/delete/prepend/grow/shrink | The tool moves `M` suffix bytes with 128 KiB read/write blocks through FUSE ([shift][v2-shift]). Each projected write rebuilds the growing piece vector and index. | At least `O(M)` data movement. With `P` growing with actual write callbacks `C`, repeated `O(P)` rebuilds give an `O(C²)` metadata-work mechanism before capacity bounds intervene. |

The 128 KiB tool block is **not** one FUSE callback in the retained rows. A
middle insert on 1 MiB moves 512 KiB in four tool blocks but records eight FUSE
READ and nine WRITE callbacks. At 10 MiB it moves 5 MiB in 40 blocks and
records 80 READ and 81 WRITE callbacks. The observed callback split is about
two per tool block here; the exact kernel negotiation cause was not retained.
Complexity is stated in actual callback count `C`, not assumed tool blocks.

### What the retained timings show

For the same fixed 4 KiB middle overwrite, the [scenario-v2 receipts][v2-rows]
show one FUSE WRITE at each size:

| File | Edit | Commit | Service `EditFile` | `service.finish` |
| --- | ---: | ---: | ---: | ---: |
| 1 MiB | 15.55 ms | 18.82 ms | 5.53 ms | 1.78 ms |
| 10 MiB | 21.68 ms | 25.22 ms | 9.22 ms | 2.06 ms |
| 100 MiB | 17.59 ms | 27.71 ms | 13.84 ms | 8.48 ms |
| 500 MiB | 18.76 ms | 57.09 ms | 41.10 ms | 33.67 ms |

Edit is roughly flat over these four observations. Commit grows by 38.28 ms
from 1 to 500 MiB; 31.89 ms of that increase appears in `service.finish`.
Storage finish drains the batch, seals/publishes objects, flushes indexes and
commits SQLite ([save finish][v2-save-finish], [storage finish][v2-storage]).
The retained LFT1 does not divide that scope further. This locates an observed
size-associated cost but **does not prove an `O(N)` Commit algorithm**: Store
size, cache and I/O effects remain possible. The current [candidate index][v2-candidates]
is a fixed-size ring, so a full-file candidate-index scan should not be inferred.

The structural middle-insert Edit grows from 105.85 ms for 1 MiB to 2,733.45
ms for 10 MiB; the 100 and 500 MiB rows fail during silent Exec at about five
seconds before Commit ([baseline][v2-report]). The source-derived `O(C²)`
mechanism is stronger evidence than fitting a quadratic exponent to these few
timings. Extending that silent progress window alone would not remove the
work: [replay validation][v2-replay] caps existing-file non-base pieces at
8 MiB, below the 50–250 MiB suffixes in the larger middle-shift cases. That
is an eventual source-derived capacity limit, not the directly observed
failure code in those receipts.

All #232 wall times above are **cache-INELIGIBLE diagnostics** under the frozen
Linux FUSE/backing contract. They identify work to investigate; they are not
eligible latency passes or a fair ratio against G2's cache state.

## Conclusions and next diagnostic

1. G2's first fixed-size overwrite uses constant-size splice metadata, and its
   logical edit and content construction touch replacement bytes and tree
   paths. With small `P` and no cached-inode suffix reconciliation, that
   algorithmic core is approximately `O(k + log E)`; the full route includes
   Store, transport and publication costs and has no proven `O(1)` bound.
   Cached structural reconciliation can be `O(M)`.
2. The current fixed-size FUSE overwrite is also local during Edit. Its
   500 MiB **Commit** slowdown is observed inside `service.finish`; the exact
   cause and complexity there remain open. Count batch objects/bytes, metadata
   pages, candidate-index work, SQL operations and phase time with
   `layerfs-telemetry` before choosing a product fix. Retain daemon Exec
   telemetry to assign the fixed Edit floor.
3. The current structural shell editor is intrinsically suffix-moving and
   repeatedly rebuilds increasing overlay metadata. It cannot meet a
   file-size-insensitive target through this algorithm. A real POSIX/FUSE
   range operation backed by a single Workspace piece splice is a possible
   product direction, subject to syscall semantics, alignment, cache coherence
   and the existing capacity contract. A changed editor algorithm needs a new
   prospective scenario identity; no historical receipt is relabelled.

[g2-report]: ../../../../docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md
[g2-ledger]: ../../../../docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md
[g2-timer]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/8b5e0955e808ab04fe0ce62e5e28f4b8ff94c807/benchmark/fs-bench-pro/src/sdk_file_edit.rs#L36-L79
[g2-sdk]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/8b5e0955e808ab04fe0ce62e5e28f4b8ff94c807/crates/layerfs-sdk/src/client.rs#L252-L259
[g2-wire]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/8b5e0955e808ab04fe0ce62e5e28f4b8ff94c807/crates/layerfs-workspace/src/live_backing.rs#L1047-L1094
[g2-piece]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/8b5e0955e808ab04fe0ce62e5e28f4b8ff94c807/crates/layerfs-workspace-core/src/file_edit.rs#L970-L1072
[g2-construction]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/8b5e0955e808ab04fe0ce62e5e28f4b8ff94c807/crates/layerfs-workspace/src/changes.rs#L1914-L1997
[g2-rope]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/8b5e0955e808ab04fe0ce62e5e28f4b8ff94c807/crates/layerfs-content/src/file/rope/edit.rs#L69-L100
[g2-rope-paths]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/8b5e0955e808ab04fe0ce62e5e28f4b8ff94c807/crates/layerfs-content/src/file/rope/edit.rs#L415-L639
[g2-cache-mark]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/8b5e0955e808ab04fe0ce62e5e28f4b8ff94c807/crates/layerfs-fuse/src/live_owner.rs#L1414-L1417
[g2-cache-range]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/8b5e0955e808ab04fe0ce62e5e28f4b8ff94c807/crates/layerfs-fuse/src/live_owner.rs#L2240-L2249
[g2-reconcile]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/8b5e0955e808ab04fe0ce62e5e28f4b8ff94c807/crates/layerfs-fuse/src/live_owner.rs#L2327-L2363
[spec]: SPEC.md
[v2-report]: exec-fuse-edit-v2-baseline.md
[v2-rows]: evidence/exec-fuse-edit-v2-baseline-20260924-01/report-edit.tsv
[v2-sdk]: ../../../crates/layerfs-api/sdk/src/workspace.rs#L75-L115
[v2-daemon]: ../../../crates/layerfs-daemon/src/execution.rs#L53-L123
[v2-tool]: ../../../benchmark/fs-bench-pro/workload/src/main.rs#L214-L241
[v2-shift]: ../../../benchmark/fs-bench-pro/workload/src/main.rs#L140-L211
[v2-fuse-write]: ../../../crates/layerfs-fuse/src/adapter.rs#L528-L580
[v2-fuse-setattr]: ../../../crates/layerfs-fuse/src/adapter.rs#L424-L525
[v2-write]: ../../../crates/layerfs-workspace/src/filesystem/write.rs#L586-L690
[v2-pieces]: ../../../crates/layerfs-workspace/src/overlay/pieces.rs#L218-L320
[v2-index]: ../../../crates/layerfs-workspace/src/backing/metadata_index.rs#L288-L321
[v2-content]: ../../../crates/layerfs-content/src/file/edit/apply.rs#L244-L357
[v2-save-finish]: ../../../crates/layerfs-server/src/service/save/content.rs#L183-L220
[v2-storage]: ../../../crates/layerfs-storage/src/cas/lifecycle.rs#L238-L292
[v2-candidates]: ../../../crates/layerfs-storage/src/encoding/delta/candidates.rs#L81-L103
[v2-replay]: ../../../crates/layerfs-workspace/src/overlay/pieces.rs#L287-L319
