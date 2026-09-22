# Fresh-file complete-content streaming

> **Status: implementation, build/review checks and both live prerequisites have now run once. `mounted_dsh` PASSED its first attempt; `captured_replay` FAILED its first attempt on one post-rebase refusal assertion and stays open. The four registered regressions PASSED.**
> Implementation parent: `74d4a2ace173d09689b8fbb42953658e94277bab`.
> Frozen product input seal: `ffa9fa10899063601d7520581f7db932644a9c45a34b2123fa895f012320da08`.

The next operation uses the existing Workspace write and ConstructFile path for a
fresh regular file. Complete fresh contents are streamed from retained Local/Zero
pieces; they are not an EditFile replacement vector. The construction profile is
selected from semantic state before delivery: fresh, not captured, and not a symlink.
A first D1 mutation against retained G converts to CapturedBase before selecting
the profile, so D1 replay keeps the existing8-MiB cumulative replacement bound.
Existing canonical-file edits retain that bound too.

Fresh construction still requires zero canonical base, no Base pieces, and exact
complete length equal to the sum of Local/Zero pieces and recorded replacement
bytes. Individual native payloads remain at most8 MiB; FUSE writes remain128 KiB;
MAX_FILE,1024 pieces, disk quota,4096 payload records, resource windows, worker
count and deadlines remain unchanged. The existing bounded ReplacementSource
feeds one ConstructFile operation and the existing content/metadata R result.
This adds no Service operation, fallback after failed EditFile, or whole-file buffer.

The largest already-preinstalled DSH file is18,259,144 bytes and needs at least140
128-KiB writes, fitting the unchanged piece count. Its bytes will be reused from
the pinned prepared corpus, with no npm/network installation or regenerated master.
The selectors, source identities and results are recorded below and in the [functional index](evidence/fresh-stream/functional-index-02.json.gz) and [failure record](evidence/fresh-stream/captured-replay-failure-01.json.gz).
The older native-create caller's fresh8-MiB+1 refusal becomes obsolete under this
profile; updating that current oracle does not rewrite Round43's historical failed
capacity receipt or close that gate.

This prerequisite does not make the full tree admissible. The pinned corpus needs
28,743 dirty identities and28,742 bindings, versus current128-entry admission. Its
prepared representation needs at least2,489,482 bytes even with one-byte names,
versus32 KiB. At128-KiB writes it needs at least26,317 retained payload records,
versus4096. Kernel lookup pins also remain subject to256 resident Nodes. Genuine
bounded prepared-record construction, frontier/completion traversal and payload
ownership remain prerequisites to one full-upload Commit. No hidden smaller
Commits, workload shrink or numeric limit inflation is proposed.

The source clamps each read length in u64 to the existing output/MAX_READ_BYTES
window before converting to usize. This avoids zero progress for a valid4-GiB
Zero span on a32-bit platform; actual32-bit execution is NOT_RUN. No data layout,
backing format, retained allocation shape or Service input format changes.

## Live outcomes (first attempts, no reruns)

Two serial invocations, one attempt per selection, fresh output directories and
complete60-second budgets; `source-freeze-02.json` was re-verified as494/494
source-and-caller hashes against the working tree immediately before them. The
[first invocation](evidence/fresh-stream/selections-01-stdout.log.gz) ran the two
new cases; the [second](evidence/fresh-stream/selections-02-stdout.log.gz) ran the
four regressions after the failure was diagnosed and does not retry it.

`mounted_dsh` **PASS**,40.81s complete/37.47s test: the pinned18,259,144-byte DSH
file was uploaded through the writable mounted projection in140 caller buffers of
131072 bytes with SHA256 verified during the upload, then one complete
ConstructFile/Commit and one4-byte EditFile/Commit against the prior
content/metadata roots, with the complete canonical digest checked. The kernel
profile was `rw,nosuid,nodev,noatime`; native close and teardown were clean. The
run reported `FRESH_STREAM_INPUT bytes=18259144
sha256=264d3092d69de80f5acdb71c930efec8db5bd9627f41659ed3416566b9ae34b4
caller_blocks=140 buffer_bytes=131072
edited_sha256=472d7857e055fa5e78388a9f41456f982b1dd85f8b0db021961ffba3beaf33ce`.
This qualifies the fresh construction profile end to end for the largest
already-preinstalled file. It is **not** the full-corpus upload, and140 caller
buffers are not a kernel request count.

`captured_replay` **FAIL**,1.13s, at `fresh_stream.rs:387` from the **second**
`refuse_extra` call (line441, after `commit_staged`): the +1 write returned
`Capacity` and changed no observed state, but one native `Inspect` was delivered
before the refusal, and the caller asserts that a refusal appends no operation.
The first `refuse_extra` (line424, before G completion) passed, so the
pre-rebase refusal consumed nothing. Four of the five test-side helper deliveries
in the [labelled diagnostic](evidence/fresh-stream/fresh_stream-captured_replay-diagnostic-01/test.stderr.gz)
occur only after `release()`/`join`, which is what places the failure at line441
rather than424.

Regressions against the same frozen product: `write envelope`, `create
semantics`, `create successor` and `symlink semantics` all **PASS** in1.3-3.2s
complete, each with its native-clean-close check where the route claims one. The
legacy write-envelope route records external teardown only and claims no clean
Workspace close; no owned container or volume and no forced cleanup remained
after any selection. These are functional receipts with an **undeclared** cache
state, not cold or performance evidence.

## Open failure

The post-rebase refusal ordering is unresolved on purpose. The evidence shows
where and what, not why: the product write path was not instrumented, no product
or caller source was changed, and the caller assertion was not relaxed to make
the case pass. Reproduce through the public Workspace API and decide whether an
over-limit refusal must be decided before any native consultation, or whether
the caller oracle is wrong to require zero operations once the file carries an
inherited canonical base. Round43's historical capacity FAIL remains open and
separate, and no passing Round49 check is reclassified by this failure.

## Production LOC

Implementation commit: **112949 ->112975 (delta +26)**; reference65417 ->65417
(+0), core47532 ->47558 (+26). Evidence-and-documentation commit on parent
`9499012926f661ce4a7b357b18810e76c3179806`: **112975 ->112975 (delta +0)**;
reference65417 ->65417 (+0), core47558 ->47558 (+0), because it changes no
production source. The unchanged counter
`b5b9617d08204977176302311e0b2c72a811b420` compares exact first-parent and final
staged archives of crates and core/crates, excluding tests including inline
cfg(test), docs, fixtures, tooling, manifests and generated/dependency code. No
relocation or reference retirement.
