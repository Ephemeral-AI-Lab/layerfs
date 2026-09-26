# #245 / #248: final file delta at Commit

> **Status:** Research; informative and not a product contract.

**Source pin:** `74ac30e28bcfaef180f405c9bc9045970f1678eb` (2026-09-26). This is an implementation design and audit of the source at that pin, not evidence that #248 has shipped. The [final E/F report](evidence/phase1-e-f-final/REPORT.md) records the strict generation-overlap proof and frozen F comparison at `b2cd0df23`; F's latency cells remain `INELIGIBLE`. [#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248) still describes the post-E/F delta work and cites the older `d6bc594f2` source. Any changed source or measured result needs its own identity and proof.

## The object being aggregated

An accepted write publishes a new **private Workspace root** so the mounted file immediately reads its newest bytes. The file's private sequence consists of `Base` extents into one selected immutable canonical file version, `Local` extents into private backing payloads, and `Zero` extents for holes. The sequence is a **final-state representation**: overwriting a prior local byte replaces its extent; Commit does not replay a log of shell `WRITE` calls. [`metadata_pieces::replace`](../../../crates/layerfs-workspace/src/backing/metadata_pieces.rs) and the [file mutation path](../../../crates/layerfs-workspace/src/filesystem/write.rs) already have that distinction.

```text
canonical Cn:       a  b  c  d  e  f  g  h
shell history:         b:=X; b:=Y; c:=Z; g:=Q   (four writes)
frozen G1 result:   a  Y  Z  d  e  f  Q  h
Commit delta:          [b,c -> Y,Z]       [g -> Q]  (two final runs)

write success -> new local root for read-your-writes
Commit success -> one new canonical file root and one Branch-head publication
```

“No per-edit checkpoint” means no canonical Store file version, change-log entry, or Commit payload for each prior write. It **does not** remove local root publication, which is required for concurrent readers and atomic mutation failure. It also does not make historical write cost or temporarily retained private payloads disappear: overwritten data needs custody and eventual reclamation.

## Current route and where the 4,096 ceiling lives

```text
accepted POSIX write
  -> local Base/Local/Zero path-copy and Fold.edits: u16
  -> private inode (exact count or u16::MAX = unknown)
  -> capture immutable G1; later writes enter G2             [E proved]
  -> lower_file: cursor over G1 -> Vec<Edit>                  [4,096]
  -> EditUpload: packed Vec<u8> of 24-byte descriptors        [4,096]
  -> Bridge EditFile Begin(count:u32, replacement:u64)       [4,096]
  -> server: descriptor block + edits + starts Vecs          [O(R) RAM]
  -> C1: EditStream(Vec<Edit>)                                [4,096]
  -> compare, per-final-run split/scan/concat, final file root
  -> prepared filesystem and Commit(expected_head = Cn)
```

The source supports these precise statements:

| Current stage | Source behavior at the pin | Consequence |
| --- | --- | --- |
| Local write | [`Fold`](../../../crates/layerfs-workspace/src/backing/metadata_pieces.rs) counts runs in `u16`; `replace` rejects a folded count over `MAX_EDITS_PER_OPERATION`. A shared subtree makes the stored count `u16::MAX` (“unknown”). | A write may fail early or the full final count may only be discovered at Commit. The marker is not a valid scalable count. |
| Capture | [`capture_submission`](../../../crates/layerfs-workspace/src/overlay/snapshot.rs) pins a root and permits the successor generation. | The ordered final state is stable for more than one Commit pass. E's strict mounted overlap is separately proved in the final E/F report. |
| Lowering | [`lower_file`](../../../crates/layerfs-workspace/src/commit/lower.rs) walks the frozen extent sequence, converts each non-Base run or base deletion gap to an `Edit`, and stores `Vec<Edit>` up to 4,096. | It aggregates final state already; its representation and count check are the obstacle. |
| Upload | [`EditUpload`](../../../crates/layerfs-workspace/src/commit/upload.rs) materializes `24R` descriptor bytes, then [`ReplacementSource`](../../../crates/layerfs-workspace/src/commit/source.rs) emits final `Local`/`Zero` bytes. | Replacement bytes stream, descriptors do not. `ReplacementSource::pull` currently reopens a cursor on each pull. |
| Bridge | [`Request::validate`](../../../crates/layerfs-bridge/src/contract/request.rs) rejects `edits > 4096` and `input_length = 24R + S > MAX_FILE`. | Removing just the explicit count leaves a descriptor-inclusive 4 GiB input ceiling; a separate declared stream-byte budget is needed. |
| Server | [`edit_stream::read`](../../../crates/layerfs-server/src/service/save/edit_stream.rs) retains the raw descriptor block, parsed `edits`, and replacement `starts`, then spools replacement bytes above 64 KiB. | Several `O(R)` resident allocations remain, although the replacement spool has a fixed application buffer. |
| C1 | [`EditStream::new`](../../../crates/layerfs-content/src/file/edit/input.rs) rejects over 4,096. [`apply_edits`](../../../crates/layerfs-content/src/file/edit/apply.rs) compares for an exact no-op, then applies chunked edits by split, replacement scan and concat. | C1 already emits one **final** file root, not a published root after each run. The per-run algorithm and `Vec<Edit>` still need a scale proof. |

**Independent admission blocker:** [`PayloadHost::reserve`](../../../crates/layerfs-workspace/src/backing/payload.rs) caps the host's live payload records at `MAX_PAYLOADS = 4096`. Each nonempty write can own a separate payload record, and records remain until reclamation. A 4,097-write test may therefore fail **before** it reaches the descriptor path, even after all edit-run checks are removed. This is a separate backing resource policy; a genuinely limit-free final-run route must either replace this count ceiling with budgeted ownership or prove a workload with more final runs can be represented without exceeding it. Track both counts in tests. The 4 GiB logical file limit and actual quota/deadline remain valid limits.

## A deterministic final-delta algorithm

Let `base_cursor` be the next unconsumed byte in canonical base `R_n`, `delta` be `(result bytes emitted so far) - base_cursor`, and `pending` be the length of consecutive `Local`/`Zero` extents. Traverse frozen `G1` in result order. The first pass performs checked arithmetic and counts descriptors. A second pass emits the same descriptors, and a third traversal emits final replacement bytes after the descriptor prefix required by the existing wire format. These are Commit work; no pass is moved into preparation to flatter a timer.

```text
base_cursor := 0; delta := 0; pending := 0; count := 0; S := 0
for each frozen extent in order:
    if Local or Zero:
        pending += extent.length; S += extent.length
    if Base(offset, length):
        require offset >= base_cursor and offset+length <= base_length
        close(base_cursor, offset, pending)
        base_cursor := offset+length; pending := 0
close(base_cursor, base_length, pending) at EOF
require final_length == base_length + delta

close(a,b,m):
    if a == b and m == 0: return              # no change
    start := a + delta; end := b + delta       # current-result coordinates
    emit_or_count(Edit(start,end,m))
    count += 1; delta += m - (b-a)             # checked signed arithmetic
```

The emission pass must recheck the first pass's `count`, `S`, final length, coverage and digest/identity of the pinned root; a mismatch is a failure before publication. `Zero` contributes logical replacement bytes and emits zero bytes unless a versioned canonical protocol explicitly supports holes. A fresh file uses `ConstructFile` from its final content rather than an `EditFile` against a nonexistent base. Adjacent changes without a retained `Base` span become one maximal run; a deletion gap can have `m=0`. The algorithm never examines an overwritten former value. It is exactly the grouping `lower_file` performs today, expressed as bounded traversal rather than a `Vec`.

### Replay without a descriptor array

The present Begin frame contains `count:u32` and `replacement:u64` before the body, so one preflight pass is unavoidable without a protocol version change. Preserve the descriptor-prefix grammar while making each pass monotone:

1. Count and validate the pinned sequence; check `24R`, `S`, their sum, file length and an independently declared stream/disk budget with checked arithmetic.
2. Emit each 24-byte descriptor through a fixed frame buffer. Hold the extent cursor across frame pulls, rather than reseeking from the last offset for every call. Then stream `Local`/`Zero` bytes through another persistent cursor and bounded payload reader. Check exact emitted totals and terminal EOF.
3. At the service, validate each descriptor as it arrives (ordered current-result coordinates, no overlap into an earlier replacement, base/final length and total `S`). Retain a quota-charged, replayable fixed-record descriptor spool. A useful internal record is `(start,end,len,replacement_spool_offset)`; its fixed width permits indexed lookup without a `starts: Vec<u64>`. Retain the replacement spool separately. Reopen sequential descriptor cursors for no-op comparison and construction; change C1's `EditStream(Vec<Edit>)` and `EditSource(index)` seam as needed so neither pass silently recreates `O(R)` arrays. Preserve current edit boundaries because they affect the canonical mapping and root.
4. Build a final canonical mapping and file root, then the prepared filesystem and **one** Branch-head publication. The current C1 split/concat loop already delays its final file root until the end; removing intermediate *published* file checkpoints is not the main C1 change. For large `R`, either prove the existing split/concat and deferred-draft behavior meets the target bounds, or implement an ordered, bounded-frontier construction with unchanged-subtree reuse. An alternative builder must produce identical root and mapping partition for all formerly accepted inputs; if that cannot hold, obtain an explicit versioned identity ruling before shipping it.

The service currently reads the whole descriptor block into a `Vec` and holds parsed edits and start offsets. Its 64 KiB replacement **application** buffer is real; it is not proof of bounded *physical* memory because a file spool can populate the OS page cache. The C1 [mapping page cache](../../../crates/layerfs-content/src/file/mapping/read.rs) has a 64-page ceiling, but `apply_edits` also uses per-run drafts (8 MiB minus one byte ceiling) and its whole-file route allocates the final whole object. Every owned array, frontier, spool, and page-cache charge must be included in the eventual bound.

### Canonical-root compatibility is a design gate

A replayable **sequential descriptor spool** is sufficient for two ordered C1 passes, but it is not a drop-in implementation of today's C1 API. [`EditStream`](../../../crates/layerfs-content/src/file/edit/input.rs) owns a `Vec<Edit>`; [`Plan`](../../../crates/layerfs-content/src/file/edit/input.rs) indexes that vector; and `EditSource::replacement_len(index)` / `read_at(index, offset)` demand random access by edit number. To retain a fixed resident bound, C1 needs a replay-cursor interface and an absolute replacement-spool offset carried with each validated descriptor (or a fixed-width on-disk index whose lookup is charged). `Plan`, no-op comparison, `ReplacementReader`, the whole-file adapter and chunked application must consume that interface. Simply putting `EditStream` behind a file and reconstructing its `Vec` before `apply_edits` would restore the same `O(R)` memory limit.

There are two distinct construction choices; they cannot be called equivalent without evidence:

| Choice | Identity and time implication | Required ruling/proof |
| --- | --- | --- |
| Feed replayed descriptors to today's ordered split / replacement CDC / concat semantics, with bounded draft handling. | Most direct path to exact roots for formerly accepted streams. A useful **cost model** is `R` height-sized split/join walks plus touched mapping work and replacement/base bytes; it is not a proven worst-case bound for the present provider/cache/draft implementation. `EditObjects` currently has an 8 MiB live-draft ceiling; passing 4,097 and >65,535 runs is not implied by removing the descriptor count. | Prove mapping partitions and roots exactly match the old route, measure draft peak and node visits as `R` increases, and give a bounded-memory behavior for a valid stream that would otherwise exceed live drafts. |
| Build the final mapping in one forward pass from retained base subtrees and replacement chunks. | This can target `O(R+M+S+U)` work and a height-sized frontier, but it may repartition nodes or change CDC chunk boundaries and therefore change canonical object IDs even when final bytes match. | Either prove exact canonical equivalence for the full previously accepted domain, or explicitly version the construction/identity rule and obtain a compatibility decision before implementation. Byte equality alone is insufficient. |

The conflict is concrete: [`EditStream`'s contract](../../../crates/layerfs-content/src/file/edit/input.rs) says adjacent edits are deliberately not merged because CDC segmentation and roots depend on declared boundaries. The [chunked implementation](../../../crates/layerfs-content/src/file/edit/apply.rs) scans each replacement separately, then splits and joins the existing mapping; the [tree implementation](../../../crates/layerfs-content/src/file/edit/tree.rs) seals only final reached drafts. A generic “concatenate final bytes and run the complete-file builder” can choose different chunks or page partitions. #248's exact-root acceptance rule and its desired linear one-pass builder are therefore **joint proof obligations**, not two properties to assume simultaneously. If the first choice meets measured budgets, it is the smaller compatible change; if it does not, the identity ruling must precede the second.

## Incremental Commit: compare against the immediately previous saved version

```text
                 captured file version             published head
 Cn:             Rn, filesystem Fn  <-------------- Hn
                   |
                   +-- G(n+1): Base -> Rn, local changes A
                   |       freeze; aggregate A vs Rn
                   |       expected_head = Hn; base = Fn
                   |       publish C(n+1): R(n+1), F(n+1), H(n+1)
                   |
                   +-- G(n+2): accepts later changes B while A is frozen
                           initial Base -> captured G(n+1) version
                           after known C(n+1) success:
                           substitute Base -> saved R(n+1)
                           aggregate B vs R(n+1), expected_head=H(n+1)
                           publish C(n+2)
```

The important existing hooks are [`prepare_changes`](../../../crates/layerfs-workspace/src/commit/save.rs) (`expected_head` and `base` from the captured branch context), [`canonical_inode`](../../../crates/layerfs-workspace/src/commit/reconcile.rs) (substitutes the saved file root for a captured private base), and [Commit completion](../../../crates/layerfs-workspace/src/commit/completion.rs) (checks the predecessor head on the reply). Example: `C_n = abcdef`; `G_(n+1)` changes `b -> X`; `G_(n+2)` changes `e -> Y` while the first Commit is in flight. The first head is `aXcdef`; after known reconciliation, the second delta is only `e -> Y` against `aXcdef`, yielding `aXcdYf`. It must not resend `b -> X` or diff against `abcdef`. A moved external head is a conflict/rebase decision, never an implicit stale-base Commit. A failed or unknown publication retains the frozen generation and does not advance the base on a guess.

## Complexity and resource accounting

Define `N` = final logical bytes; `P` = extents in one frozen file sequence; `L` = private index leaves traversed; `R` = maximal final changed runs; `S` = final `Local`/`Zero` replacement bytes (including zero-fill sent to the existing protocol); `D=24R` = wire descriptor bytes; `H_w`/`H_c` = private/canonical tree heights; `Q` = transport pulls; `M` = canonical mapping nodes/entries actually visited, altered or emitted; `U` = canonical base bytes actually demanded for comparison, boundary chunking or whole-file construction; `W` = a fixed I/O window. `B` = cumulative bytes written in the shell's history, including bytes later overwritten. `R` and `S` describe the **final** state; `B` does not enter the desired Commit delta, but it did cost write-time I/O and may temporarily consume private backing.

| Operation | Current source / defensible bound | #248 target and condition |
| --- | --- | --- |
| Final delta discovery | One logical `P`-extent lowering walk; `O(R)` resident edits. The current cursor rereads ancestor pages when advancing leaves, so physical index reads can be `O(H_w L)`, not proven `O(L+H_w)`. | Two ordered scans: `O(P)` extent visits and `O(L+H_w)` private page visits per pass **if** a cursor retains path offsets and each page is read a bounded number of times. The third replacement-byte pass adds `O(P+S)` work. |
| Body production | `O(D+S)` bytes; `O(D)` descriptor buffer. The replacement source reopens a cursor per pull, adding up to `O(QH_w)` path reads besides leaf advancement. | `O(D+S)` wire bytes, `O(W+H_w)` producer memory, no `QH_w` reseek factor; all I/O and checksum work charged to Commit. |
| Service acquisition | `O(D+S)` input and spool I/O; `O(R+D+min(S,64 KiB))` application RAM across descriptor arrays/block and replacement buffer. File-spool page cache is a separate physical charge. | `O(D+S)` quota-charged disk spool, `O(W)` resident descriptor/replacement buffers plus fixed control state; sequential replay for each required pass. |
| C1 construction | For a chunked result, one loop per run performs tree split/join and CDC on replacement. `R H_c + M + S + U` is a **cost model for structural visits and bytes**, not a proved current worst-case CPU or I/O bound: provider calls, page-cache eviction, draft maps and repeated range traversal need measurement or proof. The 8 MiB deferred-node cap can still refuse dense runs. The whole-file route allocates `O(N)` by policy. | At minimum `Omega(R+S)` work is unavoidable. Desired ordered chunked construction is `O(R+M+S+U)` structural visits/bytes with `O(H_c * page_size + CDC_window + W)` application memory and final-root-only emission; prove exact roots for old inputs. The policy-selected whole-file object remains `O(N)` within its declared cutoff. |
| Historical writes | `B` bytes were accepted and may still have physical payload owners until reclaim. | Commit work must not scale with overwritten history `B`; Workspace write/reclaim cost and retained-disk charge still do. |

This is an **algorithmic target**, not a measured wall-time multiplier or a proof of current worst-case CPU. Current [`metadata_cursor::step_in`](../../../crates/layerfs-workspace/src/backing/metadata_cursor.rs) rereads ancestors for each next leaf, and [`ReplacementSource::pull`](../../../crates/layerfs-workspace/src/commit/source.rs) seeks again per pull. These are concrete places where an apparently linear logical walk can have extra page I/O. C1's ordered split/concat also uses map/set bookkeeping whose CPU cost includes comparison and allocation factors beyond the structural visit counts above. Hashing, compression and storage cost depend on actual object bytes. A full Commit-time scan is `Theta(P)` even for a sparse edit in a highly fragmented file unless the private tree maintains additional safe subtree summaries. Such summaries would be local index metadata, not canonical per-write checkpoints, but they should only be added after the simple scan is measured. For a sparse file with few extents, the scan is small regardless of `N`; for dense final fragmentation, `Omega(R)` descriptor work is inevitable. A shell that rewrites a suffix still pays for those submitted bytes.

**Bounded does not mean constant total work.** A valid 4 GiB file can still produce large `P`, `R`, `S`, and `D`. Enforce explicit file, stream-byte, private-backing, spool-disk, resident-memory and deadline budgets. Check deadlines in every long cursor, parser, replay and builder loop; admission must fail precisely without a partial published head when a real budget is exhausted. A 32-bit descriptor count can remain as a framing field only after checked conversion and a proven representable maximum; it must not become a disguised product run ceiling. Likewise, `MAX_FILE` must bound the logical file, not silently bound `D+S` as if descriptor overhead were file bytes. No fsync or crash-durability guarantee follows from this design.

Physical memory is a separate proof: the service's file spool and private backing can occupy page cache even when heap buffers are fixed. Count anonymous plus file cache in the owning cgroup or equivalent host accounting, identify which process owns each spool, and report actual high-water figures. A cold-cache timing claim needs the repository's equal declared cache contract; the E/F latency cells do not establish it.

## Implementation and proof gates

1. **Remove every count-sized admission path.** Audit the write fold, persisted inode count/sentinel, lowering, Bridge, server and C1 together. Account separately for the 4,096 live-payload-record ceiling and any source format/version change. Keep real quota, logical-length, timer and finite-memory limits.
2. **Prove final-state semantics.** Cover many overwrites of one range (large `B`, small `R`), 4,096 and 4,097 separated final runs, adjacent changes, deletion, insertion, truncate, `Zero` hole, exact no-op root reuse and malformed coordinate/length/EOF failures. A distinct >65,535-run sequence checks that `u16` is not the next ceiling. Isolate descriptor tests from payload-owner admission, then test a real mounted write path with payload counts and quotas visible; do not enlarge its time budget to manufacture a pass.
3. **Prove version chaining.** Two sequential Commits on the same file must expose `EditFile.root = R_(n+1)` in the second request, only second-generation descriptors, exact bytes and canonical identities at both heads, old-head readback, and one Branch-head publication each. Hold a write inside the first Commit's frozen window; check G2 read-your-writes and G3 after the second capture. Preserve unknown-outcome custody and external-head conflict handling.
4. **Prove scale with counts and bounds.** Record `P,L,R,S,D,Q`, private page reads and reseeks, C1 mapping reads/creates, untouched base bytes demanded `U`, C1 deferred-byte peak, descriptor/replacement spool high-water, payload-record peak and full cgroup anonymous/file-cache peaks. Increase `R` past 4,096 under the same declared resource budgets. Show that heap/cache/draft use does not grow with `R` except an explicitly charged, bounded frontier; show no full-base payload read for a sparse chunked edit. Check old accepted cases for identical canonical root and mapping partition, not just equal bytes.
5. **Qualify performance separately.** Use one prospectively frozen workload and the benchmark contract; keep failed and ineligible cells. No candidate result is promoted by a warm page cache or by repeating an arm. The F receipt remains historical evidence at its own source seal.
6. **Qualify write progress across the full Commit, not only reconciliation.** The current [`lower_file`](../../../crates/layerfs-workspace/src/commit/lower.rs) holds the shared metadata writer gate for its complete frozen-file walk, and [`ReplacementSource::pull`](../../../crates/layerfs-workspace/src/commit/source.rs) reacquires that gate for each transfer pull. A mounted write can wait there until its deadline. The E proof covers a held successor-builder page read, not these earlier phases. The #248 cursor/replay implementation should read immutable captured pages under bounded read leases without a mutation gate spanning `P`, `R`, or `S` work. At deterministic barriers in lowering, descriptor emission, replacement transfer, remote construction and reconciliation, report accepted/rejected mounted writes, maximum gate hold and wait, exact phase, deadline, revision, and old/new head bytes. Preserve genuine quota and custody refusals. Independent SDK Exec/Commit overlap remains #249's separate daemon gate.

The implementation is complete only when the genuine Workspace → Bridge → service → C1 path passes those gates. Removing `MAX_EDITS_PER_OPERATION` alone removes a guard, not the architecture's `O(R)` resident allocations or the independent payload-owner cap.
