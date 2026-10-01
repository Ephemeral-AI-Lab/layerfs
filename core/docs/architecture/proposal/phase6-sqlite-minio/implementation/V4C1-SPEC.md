# V4c1 indexed dirty construction and immediate-base file edits

> Status: Research; informative and not a product contract.

Parent `0e3fee4ba365d7ddb75fb58c54c1d2e0f0af5621`, #294/#293. V4b is complete
only for its storage gate; all named/family/DeepSeek/locality obligations remain.
V4c is split into real dependencies: V4c1 daemon changed-state construction,
V4c2 certified incremental authority publication/paged import/inherited mount,
then V4d generic syscall composition and requested workload collection.

## Current replacement and first delivery

Replace `Engine.live_ids` / complete `FilesystemInput(base=None)` on Commit with
indexed dirty inode/name catalogs and a replayable SQL prepared-row source over
an actual immediate base root. Unchanged inode values/subtrees are retained by
C1 filesystem COW. A per-operation changed-row admission is separate from total
Workspace population; retain its finite declared memory bound until paged
construction/certification is complete. Do not increase the legacy512 quota to
pretend that a larger supported profile exists. Total-population removal and
streamed import are V4c2 gates, not claimed by this first dependency.

Persist fresh-serial and changed-name facts, including tombstones, in mutation
transactions. Partial dirty index and names-by-inode index support exact visits;
no repeated whole-inode/name count scan in the Commit path. Keyset cursors and
exact ordinal lookup replace OFFSET scans/replay of historical writes. Prepared
values are operation-owned SQL state, not a whole namespace copy. A directory
row contains changed names only; a fresh empty directory gets its required empty
row. Existing directory content comes from the selected immediate filesystem
root, not a synthetic replacement or path scan. C1 validates the actual update.

For a previously published file, SQL tracks its immediate content root/length and
visible base prefix. Local extents are final-state overlays on that immutable
base; shrink reduces visible prefix and regrowth yields zeros. Reads overlay
immutable source files on authenticated base ranges. Known publication installs
the exact returned root and retires displaced local extents only after their
source ownership is safe; accepted writes are not rolled back on failures.

Construct a replayable indexed edit sequence from current overlay spans below
the visible prefix, followed by at most one final tail edit (delete prior tail
and replace with current zero/local suffix). Replacement sources read exact
immutable source slices or stream that suffix. This never introduces recursive
WRITE history or reads an unchanged large-file suffix as replacement data.
Call existing public C1 apply_edits with the exact immediate base; fresh files
use construct_stream. Small-file whole-object assembly remains the actual C1
policy; do not imply large-file locality from a small-file proof.

## Publication and honest remaining gate

V4c1 keeps real C2/MinIO finish/ACKs, candidate validation and actual conditional
C5 publication. The authority's current complete candidate walk is still
population-proportional, so this submilestone cannot pass end-to-end locality.
V4c2 must replace it with an authenticated, namespace-certified transition against
a certified base, with actual paged facts and precise failure/Unknown custody.
Authentication alone is not certification. C1's repeated effective-cycle walks
also need source/count-driven repair or a certified parent index before claiming
no quadratic namespace behavior. Do not remove those checks silently.

Installation visits only this capture's prepared rows and adopts the exact known
result, clearing their dirty/new/name facts in bounded operations. It must not
clear every inode or erase a later accepted write. Initial implementation keeps
Commit/mutations serialized, as the declared profile; overlap is not inferred.
One producer, current C2/SQLite/transport windows and deadlines unchanged.

## Owning checks and collection

Own focused benchmark engine/edit/prepared-row modules and their actual callers.
External real-SQLite checks prove overwrite/truncate/regrow, exact edit ordinal/
source/EOF, changed-name tombstones and selected dirty counts. Actual C1/C2
construction checks use independent literal bytes and existing canonical grammar;
no candidate-generated expected roots. Real-provider full SDK/FUSE/C1/C2/MinIO/C5
proof follows on the exact frozen source. Add fixed work counts for dirty rows,
prepared names, source/base bytes and C1 nodes/payloads; unavailable physical
observations remain unavailable.

First affected two-head4KiB create/overwrite treatment, one source-pinned run,
fresh output,15 s complete child/9.5 s proof, all storage/publication included.
Its byte/mode/head/parent/cleanup and actual changed-row counts must pass; cache
unknown remains INELIGIBLE. Larger-file and population-locality cases require
separate prospective cases before collection. No unchanged source resample,
benchmark recognizer, extra worker, hidden cap/timeout increase or whole-goal
completion claim. Append every checkpoint to #294 and major results to #293.
