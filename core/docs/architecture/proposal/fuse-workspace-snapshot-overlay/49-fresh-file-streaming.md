# Fresh-file complete-content streaming

> **Status: implementation and build/review checks complete; live proof NOT_RUN because the owner exhausted the budget and requested commit/handoff.**
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
Actual selectors, source identities, resource observations and results are pending.
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

## Budget-stop checkpoint

The owner requested a stop, issue progress update, handoff and commit of existing
changes. Product and caller source still match source-freeze-02.json. Independent
product/caller reviews have no outstanding findings. Locked/offline Rust1.85.1
checks passed: host702 tests/3 ignored, Linux700 tests/162 ignored, both all-target
Clippy commands with warnings denied, fmt, host binaries/examples, host/Linux
product checks,255-file product boundary and six boundary self-tests. Builds were
serial with Cargo jobs2, Docker CPUs2 and one construction worker.

All six registered live selections are **NOT_RUN**, with zero attempts. The two
new Linux tests are among ignored tests until their explicit live drivers run.
No large-file upload, large-file Commit or incremental proof is claimed. The input
copy is ready; the final test-binary/runner bundle still needs sealing. See the
[check index](evidence/fresh-stream/checks-index.json.gz),
[NOT_RUN record](evidence/fresh-stream/functional-index.json.gz),
[archive](evidence/fresh-stream/archive-manifest.json), and
[next-agent handoff](50-budget-stop-handoff.md).

Production LOC: **112949 ->112975 (delta +26)**; reference65417 ->65417 (+0),
core47532 ->47558 (+26). The unchanged counter
`b5b9617d08204977176302311e0b2c72a811b420` compares exact first-parent/final-staged
archives of crates and core/crates, excluding tests including inline cfg(test),
docs, fixtures, tooling, manifests and generated/dependency code. No relocation
or reference retirement. Commit confirmation is retained in the local evidence.
