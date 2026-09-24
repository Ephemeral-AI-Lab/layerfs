# #232: generic shell editing and efficient Commit

> **Status:** Research; informative and not a product contract.

This design study follows the [G2 versus #232 source trace](edit-complexity-v016-v017.md)
and the retained [scenario-v2 baseline](exec-fuse-edit-v2-baseline.md). It changes
no product code, benchmark contract, target, or receipt. The three source audits
were read-only; no performance sample was repeated. An implementation needs its
own reviewed contract, exact source identity, and new scenario version wherever
the editor operation changes.

## The precise promise

`WorkspaceApi::exec(command)` passes opaque bytes to `/bin/sh -c`. LayerFS does
not inspect the command string to decide that it is an edit. The generic
filesystem boundary is the syscall stream from the program in that shell:
WRITE, SETATTR, CREATE, RENAME, and any explicitly supported range operation.

For a fixed `k`-byte local write, the **product data structures** can aim for
work proportional to `k` and tree height, independent of untouched file bytes
`N`. This can help any language or command that issues the same local write.
It cannot make an arbitrary command that itself reads and rewrites `N` bytes
take `O(log N)` time: the command and FUSE must handle the requested `N` bytes.
Comparing old and new content after such a rewrite also reads `O(N)` bytes.
Thus “command-text agnostic” is achievable; “I/O-algorithm agnostic with
constant-time Exec” is not.

## Before and proposed after

```text
CURRENT #232 (scenario v2)
SDK WorkspaceApi::exec(opaque shell string)
  -> daemon /bin/sh -c -> program
       fixed overwrite: pwrite(k) -> FUSE WRITE
                              -> load P pieces -> splice vector -> rebuild P pages
       insert/delete: read and rewrite suffix M through C FUSE callbacks
                              -> the same O(P) rebuild on each write
                              -> O(M) bytes + O(C^2) metadata as P grows
  -> SDK WorkspaceApi::commit
       -> lower all P pieces -> EditFile save -> metadata save -> History Commit
       -> EditFile service.finish: 1.78 ms at 1 MiB, 33.67 ms at 500 MiB

PROPOSED PRODUCT ROUTE (no shell-command parsing)
SDK WorkspaceApi::exec(opaque shell string)
  -> daemon /bin/sh -c -> any program
       ordinary local WRITE/SETATTR/CREATE/RENAME
           -> one checked projected mutation -> path-local COW piece update
       optional cooperating range-edit tool, invoked by any shell syntax
           -> versioned FUSE range-splice operation
           -> one checked projected Workspace RangeEdit (no suffix copy)
       a program that rewrites N bytes -> still pays for N bytes
  -> SDK WorkspaceApi::commit
       -> lower final changed ranges -> existing canonical content tree edit
       -> preserve metadata/history/Branch transaction and publication
       -> optimize storage finish only after substep counts locate its cost
```

The diagram describes **algorithmic direction**, not measured latency. Current
#232 Edit and Commit samples are cache-INELIGIBLE. The proposed range operation
changes the editor algorithm and must never inherit a scenario-v2 PASS or its
historical G2 target as a matched comparison.

## What to migrate from v0.1.6

| Historical G2 mechanism | Current Core state | Treatment |
| --- | --- | --- |
| First equal-length overwrite uses a compact base/replacement descriptor; later edits use a persistent length-indexed piece treap ([G2 source][g2-piece]). | Workspace already has Base/Local/Zero pieces, but each mutation materializes and rebuilds the whole `P`-piece index ([write][core-write], [pieces][core-pieces], [index][core-index]). | First use the existing splice for one projected range operation: with an unedited file, `P` is small and no suffix copy is needed. For repeated generic writes, replace the all-`P` rebuild with path-local updates in Core's owned disk-backed piece index. Do not copy the in-memory `Arc` treap blindly into Core's page custody model. |
| Direct SDK edit sends explicit byte-range intent to the execution-side owner. | Core already has a byte-granular `Workspace::edit_file_range`, but it admits a local mutation, not a projected FUSE callback ([range API][core-range]). | Expose one projected range mutation through FUSE, reusing `RangeEdit` semantics and the same handle authorization, mutation permit, version stamp, payload custody, projection completion and cleanup rules as WRITE. No second edit engine. |
| Commit lowers final pieces into changed ranges, then constructs canonical content from replacement bytes and mapping paths. | Core already lowers final pieces and applies content edits using retained mapping subtrees ([lower][core-lower], [content][core-content]). | Keep those algorithms. Improve the input piece index and diagnose Store finish separately; porting the old content implementation wholesale adds little. |
| Cached length-changing inode may reconcile through EOF ([G2 cache][g2-cache]). | Writable FUSE has direct I/O and explicit projection coherence. | Do not port through-EOF reconciliation as a “generic fast” mechanism. Prove size and byte visibility through open descriptors and actual Linux mounts. |

For repeated writes, a length-augmented copy-on-write page tree is one plausible
Core representation: subtree byte lengths allow split/replace/join near the
edited range without renumbering every later piece. It must retain the current
page ownership ledger, quotas, captured-generation roots, rollback/uncertain
outcome rules, and explicit reclamation. Target per local write is roughly
`O(k + log P)` in data/metadata work, subject to page splits and payload
custody. Current Commit still scans all `P` final pieces once; a path-local
write index alone does **not** make whole Edit→Commit `O(log P)`.

## Which range interface is viable through Linux FUSE?

| Candidate | Result |
| --- | --- |
| Parse `sed`, `printf`, or the shell command string | Reject. Syntax does not reliably describe the program's eventual syscalls or side effects. |
| Ordinary WRITE plus `set_len` | Generic and useful for overwrite, append, and truncate. It cannot express byte insertion without moving the suffix. |
| `fallocate(INSERT_RANGE/COLLAPSE_RANGE)` | Not a route through Linux v6.12 or current mainline FUSE: [FUSE rejects these modes before userspace][kernel-fallocate]. Implementing fuser's fallocate callback cannot enable them. |
| Versioned FUSE ioctl on an open writable file | Smallest plausible byte-granular request for a cooperating tool. The ioctl calls a **projected** Workspace range mutation. The request ABI, bounded replacement-byte delivery, size/attr invalidation, and failure/partial-publication semantics require design and live-kernel proof. [Linux's ioctl return path][kernel-ioctl] does not itself update file size like WRITE/SETATTR. |
| `copy_file_range` to a temporary file, then RENAME | More standard for a cooperating tool and supports arbitrary byte offsets, but much larger product scope. Core has no cross-inode shared canonical-piece reference; a fresh file currently takes the ConstructFile path. A callback alone would still copy/construct the full file. Linux may fall back to byte copying on unsupported FUSE copy ([kernel copy fallback][kernel-copy]). Same-file overlapping ranges cannot provide one-call suffix shift ([copy manual][copy-manual]). |

A range ioctl could request one atomic replacement if its bounded payload can be
delivered safely. The ordinary restricted ioctl path derives input size from
`_IOC_SIZE`, whose encoding has a [14-bit size field][kernel-ioctl-size]; 4 KiB
fits, while an atomic 64 KiB replacement needs another validated transport.
A shift-only ioctl followed by an ordinary pwrite avoids a large ioctl payload
but exposes the intermediate shifted version to other readers; it is a
different semantic contract. Freeze that choice and its observable results
before implementation. A tool must explicitly request the range operation;
any language or shell syntax can invoke such a tool, but an unmodified editor
that rewrites a whole file will still do so.

## Commit: a separate measured cost

For the one 4 KiB middle overwrite, retained `layerfs-telemetry` LFT1 shows:

| Input | SDK Edit | SDK Commit | Service EditFile | Metadata save | History Commit | EditFile `service.finish` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 MiB | 15.55 ms | 18.82 ms | 5.53 ms | 3.71 ms | 4.90 ms | 1.78 ms |
| 500 MiB | 18.76 ms | 57.09 ms | 41.10 ms | 4.02 ms | 6.16 ms | 33.67 ms |

Of the 38.28 ms observed Commit increase, 31.89 ms is inside EditFile
`service.finish`. That scope drains and flushes the pending batch, seals pack
groups, flushes candidate signatures, publishes the save, and commits SQLite
([Service][service-finish], [storage][storage-finish]). It **excludes** owner
destruction. Read-only inspection of the retained Stores found three objects /
6,507 canonical bytes at 1 MiB versus nine objects / 30,243 bytes at 500 MiB
in the first post-master save (the EditFile save by retained operation order),
with two pack rows in each case. Pack BLOB allocation length is not a measure
of physical bytes written. These small output counts argue
against whole-file output construction as the explanation, but do not identify
whether indexed lookups, page misses, pack writes, lock time, or transaction
work caused the measured finish difference. Four size points do not prove an
`O(N)` storage algorithm. Metadata and history saves remain near 4 and 6 ms
on the large Store, so they are not the observed growth center.

The Store count is reproducible from the local ignored artifacts: compare
`benchmark-results/prepared/master-1048576-c800bc996247d1bb/store.sqlite`
with the `edit_length_preserving/overwrite-middle-4k-on-1mib-ops-1-exec-v2`
Store under `benchmark-results/fs-bench-pro/sdk-exec-fuse/`, and likewise
`master-524288000-8f62faee769090d0` with the 500 MiB middle-overwrite
Store. Their master/receipt compatibility keys match. Open both databases
read-only, attach the master as `pristine`, then use:

```sql
SELECT MAX(save_id) FROM pristine.saves; -- 3 at both sizes
SELECT save_id, COUNT(*), SUM(canonical_length)
FROM main.objects
WHERE save_id > (SELECT MAX(save_id) FROM pristine.saves)
GROUP BY save_id ORDER BY save_id;
-- first post-master save: (4,3,6507) at 1 MiB; (4,9,30243) at 500 MiB
SELECT save_id, COUNT(*), SUM(LENGTH(data))
FROM main.object_packs
WHERE save_id > (SELECT MAX(save_id) FROM pristine.saves)
GROUP BY save_id ORDER BY save_id;
-- first post-master save: (4,2,524288) at both sizes; allocated BLOB length
```

The next **diagnostic** should add bounded nested `layerfs-telemetry` scopes
inside `service.finish` for drain, seal, signature flush, publish, and SQLite
commit; record existing `SaveOutcome` counts plus explicit lookup, statement,
and read-only SQLite page/cache count deltas and physical reads with their source.
Compare the exact 1 and 500 MiB shapes under one declared cache contract.
Use `layerfs-telemetry` alone for wall/CPU/RSS. This is a labelled cause-finding
diagnostic, never another performance sample of the frozen arm. Optimize the
substep that actually grows; preserve one construction worker, canonical roots,
atomic save/publication, no added sync, and unchanged cache policy. Combining
the three Service saves might lower the fixed floor, but changes transaction
semantics and is not the first change justified by these receipts.

## Implementation sequence and proof gates

1. **Freeze semantics and diagnostic evidence.** Retain scenario-v2 receipts.
   Capture missing daemon Exec scopes and storage-finish children in a labelled
   diagnostic. Resolve cache eligibility separately; no warm diagnostic becomes
   a performance PASS. Specify the new range operation, bounded request and
   callback counters under a new prospective scenario identity.
2. **Implement the smallest projected range path.** Reuse Core RangeEdit and
   current piece splice for one operation. Require authorized writable handle,
   checked offset/deletion/replacement bounds, exact inode/incarnation/version,
   owned payload, projection mutation permit and coherent reply. Test open-FD
   read/fstat, hard-link alias, EOF write, failure after publication, Commit and
   fresh reopen on actual Linux FUSE. No benchmark-only branch or direct Store
   mutation is allowed.
3. **Improve ordinary-write metadata only where needed.** Count pieces/pages
   rebuilt and callback latency on repeated local pwrite and structural traces.
   If growth is material, replace the full-vector rebuild with path-local
   persistent page updates. Keep per-callback read-after-write visibility and
   checked ownership/quotas. Commands that issue local writes benefit without
   adopting a special CLI.
4. **Fix the proven Commit substep.** Choose an indexed lookup, batch, pack, or
   transaction change only after the diagnostic distinguishes them. Verify
   canonical roots, retained old Commit, Branch head, resource scope and exact
   typed failure/unknown outcomes.
5. **Collect a new full campaign.** Use release binaries, public SDK
   Project/Branch/Sandbox/Workspace lifecycle, `workspace.exec` and explicit
   Commit, one sample per case and identity, separate verifier, 15-second
   complete-command budget and raw LFT1. The range-operation cases have new
   scenario IDs; do not pool them with the original suffix-copy v2 rows or
   call them a matched v0.1.6 speedup.

## Honest flexibility claim

The strongest general promise is: **any shell program that issues a localized
supported filesystem mutation gets the same efficient Workspace path, without
LayerFS knowing its command text.** A cooperating tool can additionally issue
one explicit byte-range splice for fast insert/delete/replace. Programs that
read/write an entire file still pay at least for those bytes. This division is
part of the product contract and must appear in benchmark claims.

[g2-piece]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/8b5e0955e808ab04fe0ce62e5e28f4b8ff94c807/crates/layerfs-workspace-core/src/file_edit.rs#L970-L1072
[g2-cache]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/8b5e0955e808ab04fe0ce62e5e28f4b8ff94c807/crates/layerfs-fuse/src/live_owner.rs#L2240-L2363
[core-write]: ../../../crates/layerfs-workspace/src/filesystem/write.rs#L586-L690
[core-pieces]: ../../../crates/layerfs-workspace/src/overlay/pieces.rs#L218-L320
[core-index]: ../../../crates/layerfs-workspace/src/backing/metadata_index.rs#L288-L321
[core-range]: ../../../crates/layerfs-workspace/src/filesystem/write.rs#L310-L330
[core-lower]: ../../../crates/layerfs-workspace/src/commit/lower.rs#L119-L179
[core-content]: ../../../crates/layerfs-content/src/file/edit/apply.rs#L244-L357
[kernel-fallocate]: https://github.com/torvalds/linux/blob/v6.12/fs/fuse/file.c#L2904-L2924
[kernel-ioctl]: https://raw.githubusercontent.com/torvalds/linux/master/fs/fuse/ioctl.c#L390-L407
[kernel-ioctl-size]: https://github.com/torvalds/linux/blob/v6.12/include/uapi/asm-generic/ioctl.h#L3-L10
[kernel-copy]: https://github.com/torvalds/linux/blob/v6.12/fs/fuse/file.c#L3103-L3107
[copy-manual]: https://man7.org/linux/man-pages/man2/copy_file_range.2.html
[service-finish]: ../../../crates/layerfs-server/src/service/save/content.rs#L183-L220
[storage-finish]: ../../../crates/layerfs-storage/src/cas/lifecycle.rs#L238-L293
