# #245 Phase 1 ordinary-shell repair: diagnostic contract

> **Status:** current diagnostic record; no performance result and no release
> admission follows from it.

This contract was written to bound the **cause-finding diagnostics** of
[#245 Phase 1](../../VERIFICATION_PHASE1.md) items 1 and 2, before the fix
attempts below were collected. It does not amend the frozen
[#243 Phase 1 selection](../../../243/PHASE1_CONTRACT.md), its registry, or any
retained receipt.

## What these attempts are

- One ordinary `WorkspaceApi::exec(command)` per attempt, passed unchanged to
  `/bin/sh -c` inside the mounted Workspace, followed by one explicit
  `WorkspaceApi::commit` only when the command exits zero.
- Purpose: (a) identify the exact `WorkspaceError` variant and stage behind the
  `EIO` that the #243 mixed package refresh observed at
  `mv -f package-lock.json.next package-lock.json`; (b) count the kernel's own
  FUSE callbacks per case; (c) re-check the four registered #243 functional
  oracles plus two newly labelled rename shapes on the repaired candidate.

## What these attempts are not

- They are not a registered performance selection, not a matched control/candidate
  arm pair, and not an eligible latency sample. The source identity, the image
  and the command set differ from the #243 run, so no latency comparison with
  the retained #243 numbers is made or implied.
- The retained #243 mixed-refresh `FAIL` row stays failed. Nothing here rewrites,
  re-labels or promotes it; the repair is reported as new, separately identified
  attempts on a changed source.
- They do not qualify #232's 56 ordinary-shell shapes, the deferred
  [load-bearing cases](../../LOAD_BEARING_CASES.md), the piece-index or Commit
  streaming work, or the continuously writable Commit contract.

## Identities

| Field | Value |
| --- | --- |
| Reused sealed state | #243 `issue243-shell-package-v1-prepared-02` (image, release `benchmark_shell`, release `verify_shell`, `benchmark_init`, three closed masters) |
| Prepared-state seal | `prepared.json` of that directory; per-attempt binary and master SHA-256 values are copied into each `receipt.json` |
| Product source for the pre-fix attempts | retained #243 source `6d7421c6d` with the candidate's intermediate, uncommitted working tree |
| Product source for the post-fix attempts | `dd00c92e2` (FUSE diagnostic traces) on `a0258cd6f` (correctness fixes) |
| Candidate daemon (post-fix set) | built `--release`, `aarch64-unknown-linux-musl`, `zigbuild --locked --offline`; `a660d7b0426a8b53ff9cab471204881b0d7c97d5d4892ffffb2989881dd103b9` |
| Image | one `alpine@sha256:5291449c…` layer per attempt set; the post-fix attempts share `sha256:26e081383867ec0e59276f38470bbb2002e09167843d7eacaaca13067ed429d1` |
| Fixtures | the sealed #243 v1/v2 recipe; the two labelled rename diagnostics use manifests generated from that same recipe |
| Clone | `shutil.copyfile` independent writable byte copy of a closed master; seal re-checked before and after the copy |
| Construction workers | one, in product wiring, and `LAYERFS_CONSTRUCTION_WORKERS=1` in every child |

## Cache contract

Identical and uncontrolled in every attempt: the host Store copy and the Linux
FUSE backing cache, including bytes written through FUSE and read back during
Commit, are neither primed nor invalidated, and no residency check was enforced.
Every attempt is therefore `performance_status: INELIGIBLE`. The raw driver
timings are retained as context only and are not compared between attempts.

## Diagnostics enabled

Two env-gated traces are set in the diagnostic images only:

- `LAYERFS_FUSE_TRACE=1` — one `LFS_FUSE_CALLBACK` line per admitted callback
  (operation, name, parent, offset/length), which Status cannot provide because
  it folds namespace mutations into the `write` counter.
- `LAYERFS_FUSE_ERROR_DIAGNOSTIC=1` — the `WorkspaceError` variant before
  `errno` maps several variants onto `EIO`.

Both are absent from non-diagnostic images, and their absence is reported as
`UNAVAILABLE` rather than zero where an attempt predates them.

## Receipt fields and provenance

Each `attempts/<id>/receipt.json` (and `pre-fix/<id>/receipt.json`) records the
route, the exact command bytes, the source/image/binary/fixture identities, the
clone method, the driver receipt, the counted FUSE callbacks, the observed
`WorkspaceError` variants, the independent verifier result, the cache contract,
and a per-field provenance map. Missing counters stay `UNAVAILABLE`:

- `fuse_callbacks` — measured when the attempt's image carried the trace, for
  READ, WRITE, SETATTR, CREATE, MKDIR, RMDIR, RENAME and UNLINK; `mknod`,
  `symlink` and `link` are not traced and stay `UNAVAILABLE`.
- `workspace_errors` — measured under the same condition.
- driver timings — one raw observation per attempt, not phase-isolated.
- `LFS_PIECE_COUNT`, `LFS_PIECE_PAGES`, container memory and disk — not enabled
  in these attempts, therefore `UNAVAILABLE`.

Raw `driver.stdout`, `driver.stderr`, `case.before`, `case.verify`,
`verifier.stdout` and `SHA256SUMS` are retained beside each receipt. Store and
History copies are omitted exactly as the #243 evidence omits them.

## Attempt discipline

One attempt per command per source identity. The pre-fix attempts are labelled
diagnostics of the observed failure on an intermediate working tree, and they
are retained even though later attempts superseded them; no attempt was
resampled to obtain a passing row, and no command, fixture, manifest or limit
was changed after seeing a result. The post-fix rows and the pre-fix rows are
never pooled.
