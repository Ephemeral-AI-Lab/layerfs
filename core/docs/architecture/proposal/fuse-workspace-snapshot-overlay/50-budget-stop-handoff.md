# Next-agent handoff: continue Pair1 after the budget stop

This is the handoff prompt. The owner stopped work because the budget was exhausted,
then explicitly requested an issue progress update, this prompt, and a commit of
all existing changes. This file is committed with the unfinished Round49 work.
Read the exact checkpoint from `git log -1 --format=%H -- core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/50-budget-stop-handoff.md`.
Its first parent is `74d4a2ace173d09689b8fbb42953658e94277bab`; its frozen product
seal is `ffa9fa10899063601d7520581f7db932644a9c45a34b2123fa895f012320da08`.

## Objective and checkout

Continue #179 Pair1 toward the full prepared DSH upload, one explicit full-upload
Commit, subsequent incremental Commits, then separately qualified matched R6/#207.
Use this existing checkout and its ignored prepared inputs:

- Workspace: `/Users/yifanxu/.codex/worktrees/795c/layerfs`
- Branch: `codex/pair1-remote-mount`
- Remote: `https://github.com/Ephemeral-AI-Lab/layerfs`
- Main issue: https://github.com/Ephemeral-AI-Lab/layerfs/issues/179
- Comparison issue: https://github.com/Ephemeral-AI-Lab/layerfs/issues/207

Inspect HEAD/status first. Do not start from default main, discard the handoff's
changes, regenerate inputs, or push/merge/close issues. If moving checkouts becomes
necessary, preserve both committed code and ignored fixtures/receipts with exact
identities. No new task or automation was created by this handoff.

Read root/core AGENTS.md, the measurement/release/docs rules, this packet's30
historical handoff,31 current source map,34 DSH input,43 open failure, and47–49
operation records. Existing completed component milestones stay closed. One public
operation per round; close concrete prerequisites first. Use bounded independent
agents with exclusive ownership; all heavy builds/tests/samples remain serial.

## Completed and current

- Earlier Mount/Attach/Commit, mkdir and CREATE work remains scoped by32–44.
- Native symlink47: commit `9060c26bcc3e905e031415da20cec54352b94192`;
  10 live selections/19 checks first PASS. Typed corrupt-payload case has external
  teardown only, not a clean native-close claim.
- Mounted symlink48: commit `74d4a2ace173d09689b8fbb42953658e94277bab`;
  seven live selections/14 checks first PASS; normal native close and teardown.
  Native targets0..4096 remain exact; kernel creation1..4095, FUSE Readlink4096
  explicitly ENAMETOOLONG. Empty SDK target has actual mounted readback proof.
- Fresh streaming49 is implemented and independently reviewed, and all required
  host/Linux build/check commands passed. **Its six live selections have zero
  attempts and are NOT_RUN.** It is committed only because the owner explicitly
  requested committing existing changes at the budget stop.

49 touches Workspace `overlay/pieces.rs`, `filesystem/write.rs`, `commit/lower.rs`
and `commit/source.rs`. Fresh noncaptured regular files use complete ConstructFile
streaming. Existing or captured-G files retain8MiB replay including Zero bytes.
Selection occurs after inherited-G conversion. Fresh contents require no Base
pieces, zero canonical base and full sum==logical length==replacement length.
Per-payload8MiB, FUSE/read128KiB, MAX_FILE4GiB,1024pieces,256edits, quotas,
windows/workers/deadlines remain. Source clampsu64 beforeusize for32-bit4GiB
Zero progress;32-bit execution is NOT_RUN. No new fields, codecs or whole-file
buffer. Current create-capacity oracle removes only obsolete fresh8MiB+1 refusal;
old source is archived. **Round43's historical FAIL remains open.**

## Exact next actions — resume here

1. Inspect `core/target/pair1-evidence/fresh-stream/`. Check `source-freeze-02.json`
   hashes against the product and caller files before reusing proof. At handoff
   these matched. `handoff-status-01.json`, `checks/*-command.json` and logs show
   every command exit0: host702/3ignored, Linux700/162ignored, all-target Clippy,
   fmt, host bins/examples, product checks,255-file boundary and six self-tests.
   Do not rerun passing checks absent a relevant source change or actual concern.
2. Archive the final Linux test executables named by `checks/linux-test-01.log`
   for `fresh_stream`, `write`, `create`, `symlink` into this worktree's existing
   content-addressed `binary-archive/<sha>/`. No rebuild is currently needed.
   Create `test-binaries-01.json` in the family with `binaries` mapping each name
   to its absolute `path`, SHA256 and original compiler output path, as in48.
3. Complete `fresh-stream/runner-01/`: its input is already copied/hashed once.
   Copy the final fresh_stream ELF there, write the binary/input identity manifest,
   make files readonly/executable as appropriate and seal the directory readonly.
   Point the `fresh_stream` binary manifest path at this bundle's executable.
   Existing driver mounts its parent at `/runner`, so no driver/framework change
   is needed. Preserve `input-reuse-01.json`; do not redo setup.
4. Run `python3 core/target/pair1-evidence/fresh-stream/run-selections.py` once.
   It serializes the two new cases plus write-envelope, create-semantics,
   create-successor and symlink-semantics, with fresh outputs and60s complete
   budgets. Stop on failure, retain every attempt, diagnose before changing/rerun.
   `mounted_dsh` uploads the whole18,259,144B input in140 caller buffers<=128KiB
   (not a claim about kernel request count), verifies SHA during upload, performs
   one ConstructFile/Commit, verifies complete canonical digest, writes4B, then
   verifies one EditFile/Commit against prior content/metadata roots and full digest.
   `captured_replay` preserves4Base bytes with3Local+(8MiB-3)Zero, then checks
   +1 refusal before and after G completion and the final exact canonical bytes.
   Legacy write-envelope has external teardown only; qualify its cleanup correctly.
5. Update49/14/30/31/README with actual outcomes, archive all json/jsonl/log/txt/rs/py
   AND stdout/stderr files, including files under service/. Exclude DBs/binaries,
   commit snapshots and caches. gzip mtime0; append-only receipts. Commit only
   after exact parent/final-staged LOC comparison, and confirm the committed tree.
   The current checkpoint's LOC is112949→112975(+26), reference65417 unchanged,
   core47532→47558. Recompute for every new commit; never reuse it as a later delta.

## Prepared inputs and environment

User selected `npx @deepseek-ai/dsh web`, preinstalled locally. **Never time npm or
network installation. Upload the full prepared tree, then one explicit Commit;
no hidden smaller Commits.** Package0.1.5-rc.2, Node24.21.0/npm11.19.0,
Linuxarm64/glibc2.36. Corpus:25,433 regular files/223,614,913 bytes,3,299 directories,
10 relative symlinks. App at `core/target/pair1-evidence/preinstalled-dsh-01/app`.
Prepared tar SHA694eb0421c021a162c6a87f0d19208f477c0b4c064f89c787c6f0058c3d4d0b7;
manifest SHAa6a447a5ef5fe01d8d28e3e1a382ae293c54f503adf60d22f1c950e19b5368cb.

49input: `app/node_modules/@img/sharp-libvips-linux-arm64/lib/libvips-cpp.so.8.18.6`;
18,259,144B; SHA264d3092d69de80f5acdb71c930efec8db5bd9627f41659ed3416566b9ae34b4.
Prepared copy: `core/target/pair1-evidence/fresh-stream/runner-01/dsh-largest-input`.
Its setup/cache state is unknown; these are functional tests, not cold/performance
proof. Host binary bundle is recorded in `fresh-stream/host-binaries-01.json`:
`binary-archive/8ccfa5e3f8c9c29383dd12ad75c38918d40979b10b30f84d240d925ffc903de6/host`.

Reuse `fixtures/large-edit-master-01/result.json` and its closed64MiB Store;
receipt SHA6632361ed27f5405cf188434abaf8c89f064795ebb8f3838fa25298f90757d6d,
Store SHA51f5f5027a9974c19827fa28cda9a6a64f5a55d4827ec39eddf5cab49e5370c9.
Drivers independently byte-copy it and create fresh live C5 authority. The RO
fixture is `fixtures/mounted-07/result.json`. Never reuse mutated run state.

Use Rust1.85.1 locked/offline from repo root; preserve root .cargo/config.toml
ARM AEAD flags. Host target core/target; Linux target core/target-linux.
Unset RUSTFLAGS/CARGO_ENCODED_RUSTFLAGS; export CARGO_BUILD_JOBS=2 and
LAYERFS_CONSTRUCTION_WORKERS=1. Docker context desktop-linux per command,
--cpus2. Tool image sha256:afa02fb391d3dec196c399131c3d3fe4f34b0e8d7352339220f664bf91eae408;
runtime sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4.
Registry mount read-only; no Cargo target outside this worktree. No aggregate
preflight/CI, no third-party patches/dependencies, no deadline/worker/buffer
relaxation to pass. Do not interrupt other owners. The previously authorized old
PID11323 was terminated; the cancellation spin correction was committed earlier.
No task-owned build/test/daemon remained running at this handoff.

## Remaining substantive work

The full DSH corpus has not been uploaded or committed.49 only removes the largest
file's incorrect fresh-construction replay ceiling. The full tree still needs:

- 28,743 dirty identities/28,742 names vs current128; at least2,489,482 prepared
 bytes even with one-byte names, vs32KiB. C1 needs a replayable indexed prepared
 input plus bounded validation/reference/update working state; Service/Bridge
 bounded acquisition and C5 finalization must follow without a partial Commit.
 Existing directory/inode builders accept iterators. OrderingBacking/FileBacking
 supplies spill runs, not a replacement for all input/maps. Service currently
 supplies no ordering backing. Follow03-commit-integration's end-to-end contract.
- At least26,317 payload records at128KiB writes vs4096 resident records; genuine
 backed indexing/packing is needed. Node256 is a lookup/handle pin limit, not
 authoritative namespace size; kernel references cannot be guessed away.
- Workspace captured-frontier/result traversal and completion bounds must evolve
 with that shared operation. No full resident namespace mirror or limit inflation.
- Full prepared upload, one full Commit, incremental Commits, remaining required
 shell/app behavior, and eligible matched R6 against v0.1.6 commit
 `44cf748486863ab7c21ca47e731bd88e2b9a7b4a` remain unrun.

Preserve43's capacity FAIL: one labelled diagnostic PASS did not explain/close it.
A read-only audit also suspected that C1 validate.rs::check_effective_cycles may
miss a disconnected cycle among three fresh directories against an existing base.
**Unconfirmed: no reproduction or candidate file was produced before stop.**
Reproduce through the public C1 API before calling it a bug or changing product.
Do not conflate it with the passing49 checks or reopen historical component claims.
