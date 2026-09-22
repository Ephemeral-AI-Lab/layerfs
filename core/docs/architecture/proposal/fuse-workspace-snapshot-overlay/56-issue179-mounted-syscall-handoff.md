# Issue 179 mounted-syscall evidence: task-2 handoff

> **Status: Current assignment for the next implementation agent.**
> Written after `a9657b85f` closed task 1 of `55-issue179-realdaemon-handoff.md`
> section 5. The governing plan is still `51-implementation-completion-spec.md`.
> This note does not close the issue; it hands over exactly one task: the
> mounted FUSE syscall evidence for the six namespace operations.

## 1. The worktree and its state

- Worktree: `/Users/yifanxu/.codex/worktrees/795c/layerfs`, branch
  `codex/pair1-remote-mount`, **HEAD `a9657b85f`** ("core: publish a moved
  identity's record and complete the small-project scenario"), parent
  `fd4bbb3b1` (the 55 handoff doc). Working tree clean. **Nothing is pushed.**
- Read `AGENTS.md` and `core/AGENTS.md` first, then, in this order:
  - `51-implementation-completion-spec.md` (governing plan; §5 step I and §6
    "During implementation" define the verification style),
  - `55-issue179-realdaemon-handoff.md` §5 (task list), §6 (verification
    identities and mechanics), §10 (the v0.1.6 study answers).

## 2. What task 1 established (do not redo, do not regress)

Commit `a9657b85f` fixed the post-Commit rename blocker and completed the 51
§6 real-daemon scenario. Receipts, all on this exact source:

| What | Receipt |
| --- | --- |
| `commit_small_project` end-to-end (two explicit Commits, public saved-state reads, root A readable after B, remount, acknowledged read, checked Unmount + CloseClean) | `core/target/pair1-evidence/round56/t1-small-project-0j` PASS |
| All ten native namespace selections | `core/target/pair1-evidence/round56/final-ns-<case>-01` PASS |
| `rename` with the new read-the-replaced-FD-after-Commit assertion | `core/target/pair1-evidence/round56/final2-ns-rename-01` PASS |
| Failing iteration history (kept, append-only) | `round56/t1-small-project-01..0i`, diagnostic builds under `round56/diag/` |

New product invariants your work must not break:

1. A rename publishes the **moved identity's own record** when the live root
   holds none (regular file: base roots + counted dirty key, the link
   treatment; symlink: locally materialised target in fresh form, never dirty).
2. A **moved directory whose namespace record is absent is refused**
   (`Unsupported`): a committed directory's children resolve through the
   canonical tree by path, and the bounded profile has no identity-keyed
   service query to re-anchor them after a move. This is a documented
   limitation — declare it in your route's `NOT_RUN`, and do not write a case
   that expects such a move to succeed. Moving a fresh directory works.
3. A replaced or unlinked committed identity with a live local owner gets a
   canonical carry record, so the reconcile carry's read succeeds.
4. A metadata-only setattr falls back to the **record's current** mode/mtime,
   not the original version's (`write.rs`).
5. `RootOwner::seal` queues and completes cleanup frames for pages it drops to
   zero refs; chained `candidate.update` calls are therefore safe. Remember the
   arena `update` admission: **at most 4 cells per call** for D/I/N keys, 1 for
   E/T keys.

## 3. The task (55 §5 item 2, verbatim scope)

> The mounted FUSE syscall evidence for the six operations. Follow the
> mounted-route pattern (`mounted_mkdir_route.py` is 22 lines specialising the
> shared driver): one mounted route specialising `namespace_route.py`'s
> driver, with Rust cases modelled on `mounted_create.rs`, covering kernel
> `mknod`, `link`, `unlink` with a held FD, `rmdir` for the empty and nonempty
> cases, `rename` for cross-directory, NOREPLACE collision and directory-cycle
> refusal, and `setattr` for chmod plus `UTIME_OMIT` mtime. Assert errno,
> inode identity, link counts, listing visibility and post-Commit durability
> through the real mount. The adapter callbacks already exist in
> `layerfs-fuse/src/adapter.rs`.

The callbacks are at `core/crates/layerfs-fuse/src/adapter.rs`: `mknod` (~579),
`unlink` (~671), `rmdir` (~692), `rename` (~750), `link` (~798), `setattr`
(~423). **This is a test-only task**: no product source should change unless
the mounted route exposes a real product defect (see §7).

## 4. Checklist

1. **Read the pattern first**: `mounted_mkdir_route.py` (the 22-line
   specialisation), `mounted_create_route.py` (adds the optional `BOOTSTRAP`
   manifest-extras override — use it if your cases want pre-existing base
   entries), `mkdir_route.py` (the shared driver: it runs the test binary with
   `--ignored --nocapture --test-threads=1 linux::<prefix><case> --exact` and
   matches `<MARKER> <id> PASS` lines in the output), and `mounted_create.rs`
   (1019 lines: `fixture`, `kernel(f, script)` — python3 against the mount
   path, `writable_mount`, `snapshot`, `commit`, `drained`, `saved` helpers,
   `check(id)` printing the marker).
2. **Create `core/crates/layerfs-workspace/tests/mounted_namespace_route.py`**
   specialising `namespace_route as driver`: `driver.CASES` with descriptive
   selection names, `ENTRY_SOURCE`, `TEST_SOURCE = mounted_namespace.rs`,
   `TEST_PREFIX = 'mounted_namespace_'`, `TEST_MARKER =
   'MOUNTED_NAMESPACE_CHECK'`, `MODE =
   'functional-mounted-workspace-namespace'`, and a `NOT_RUN` list that
   names: symlink operations (mounted_symlink covers them), rename
   exchange/whiteout, cross-Workspace operations, the committed-directory
   move refusal (§2 item 2), prepared npm workload, R6, hard RSS/cgroup
   bounds, crash/restart recovery.
3. **Create `core/crates/layerfs-workspace/tests/mounted_namespace.rs`** with
   `#[cfg(target_os = "linux")] #[path = "support/native_workspace.rs"] mod
   support;` then `mod linux { ... }`, one `#[test] #[ignore = "..."]` fn per
   registered case. Keep the case count small (51 §6: combine related
   assertions). Suggested split —
   - `kernel`: the six operations through actual syscalls on one writable
     mount (the core case, §5 below);
   - `durability`: the same tree after an explicit Commit and a fresh read of
     the mounted tree (names, inode identities, contents, link counts,
     metadata unchanged), modelled on `mounted_create.rs`'s successor case.
4. **Per-operation assertion coverage** (through the real mount):
   - `mknod`: regular file created with mode/umask applied, **no handle
     opened** (size 0, lstat works), duplicate name → `EEXIST`.
   - `link`: two names share one inode (`st_ino` equal, `st_nlink` 2), write
     through either visible through both.
   - `unlink` with a held FD: the name disappears from listings, the FD still
     reads **and writes** its own inode, `st_nlink` 0 via `fstat`, last-name
     removal.
   - `rmdir`: nonempty → `ENOTEMPTY` with no partial change; empty → success
     and absent from the parent listing.
   - `rename`: cross-directory move keeps the inode; **replacement with the
     destination FD held** — the old FD still addresses its own inode and
     bytes while the name resolves to the moved one; `RENAME_NOREPLACE`
     collision → `EEXIST` with both trees unchanged; moving a directory
     beneath its own descendant → `EINVAL` (bounded refusal).
   - `setattr`: chmod visible via `lstat`; `utimensat` with `UTIME_OMIT`
   atime and an explicit mtime → exact mtime, and a later mtime-only setattr
   does **not** revert the mode (§2 item 4).
5. **Build, run, record** per §5/§6 below: one attempt per registered case,
   each into its own fresh output directory.
6. **Check and commit** per §7.

## 5. Build and run mechanics (exact identities)

| Identity | Value |
| --- | --- |
| Host binaries | `core/target/pair1-evidence/binary-archive/57cc5227bb963dc4e0fde628dc594ba088e1411d95d485eca8266dc78c28084e/host` |
| Runtime image | `sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4` |
| Fixture | `core/target/pair1-evidence/fixtures/large-edit-master-01/result.json` |
| Build container | long-lived `lfs-build-179`; tests at `/tmp/s` (target `/tmp/tb`), daemon at `/tmp/daemon-src` (target `/tmp/daemon-tb`) |
| Current passing daemon | `core/target/pair1-evidence/round56/layerfs-daemon` (rebuild if product source changes) |

Build the Linux test binary (the mounted route needs no daemon — it mounts
in-process inside the container):

```sh
COPYFILE_DISABLE=1 tar -cf - -C core Cargo.toml Cargo.lock crates 2>/dev/null \
  | docker exec -i lfs-build-179 sh -c 'rm -rf /tmp/s && mkdir -p /tmp/s && tar -xf - -C /tmp/s 2>/dev/null'
docker exec lfs-build-179 sh -c 'find /tmp/s/crates/layerfs-workspace -name "*.rs" -exec touch {} +;
  cd /tmp/s && export CARGO_TARGET_DIR=/tmp/tb CARGO_BUILD_JOBS=2 \
  RUSTFLAGS="--cfg aes_armv8 --cfg polyval_armv8 --cfg chacha20_force_neon -C target-feature=+aes,+sha2" \
  LAYERFS_CONSTRUCTION_WORKERS=1;
  cargo test --manifest-path Cargo.toml -p layerfs-workspace --test mounted_namespace --no-run --locked --offline'
docker cp lfs-build-179:/tmp/tb/debug/deps/mounted_namespace-<hash> core/target-linux/debug/deps/
```

Run one registered case (fresh output dir every time; receipts are
append-only):

```sh
BIN=core/target/pair1-evidence/binary-archive/57cc5227bb963dc4e0fde628dc594ba088e1411d95d485eca8266dc78c28084e/host
python3 core/crates/layerfs-workspace/tests/mounted_namespace_route.py \
  --fixture core/target/pair1-evidence/fixtures/large-edit-master-01/result.json \
  --binaries "$BIN" --test-binary core/target-linux/debug/deps/mounted_namespace-<hash> \
  --output core/target/pair1-evidence/round57/<fresh-dir> --case <case> \
  --image sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4
```

Process notes that cost real time when ignored: the host archives under
`binary-archive/` are subject to macOS code-signature evaluation — a stale
copy can be `SIGKILL`ed on exec even when byte-identical; rebuild the archive
directory if a host binary mysteriously dies. A run against a test binary
built before the last source change reports `SIGKILL`/`-9` from the
container — that is a stale binary, not a product failure. Runs are
serialized (per-worktree measurement lock); never hold two runs at once.

## 6. If the mounted route exposes a product defect

This is the first mounted evidence for these six operations, and the 55
history shows every new surface found real defects. If a case fails:

1. Reproduce with a **diagnostic build** (temporary `eprintln!`s, clearly
   labelled, reverted before committing) and establish the failing seam before
   editing — do not guess; the two previous rename diagnoses were both wrong
   on the first attempt.
2. Fix the owning layer, keep every numeric bound, and re-run **all ten native
   namespace selections** plus `commit_small_project` plus the new mounted
   cases on the fixed source. Rebuild the Linux daemon for the scenario:
   same tar extract into `/tmp/daemon-src`, `cargo build --target
   aarch64-unknown-linux-gnu -p layerfs-daemon --locked --offline` with the
   same four `RUSTFLAGS`, stage out of
   `/tmp/daemon-tb/aarch64-unknown-linux-gnu/debug/layerfs-daemon`.
3. If a registered case contradicts the operation it registers, do not change
   the assertion to pass: bring the owner the trace and a recommendation.

## 7. Verification and the commit

Before committing (test-only change expected):

```sh
cargo +1.85.1 fmt    --manifest-path core/Cargo.toml --all -- --check
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Production LOC for the commit message (test-only ⇒ delta 0, but report it):
`tools/production_loc.py --root ROOT --json` on `git archive` snapshots of
the parent commit (`a9657b85f`) and the staged tree (`git add -A && git
write-tree`), scope `crates core/crates`. Current totals to carry: core
50,287 (workspace 14,537), reference 65,417, combined 115,704. The
whole-core `cargo test` suite runs **once on the final frozen source later in
the round** — do not claim it for this commit; report exactly which checks
ran. Test files under `tests/` are outside the production-line ceiling and
the boundary scan, but keep `mounted_namespace.rs` focused (mounted_create.rs
is the size precedent).

Commit message: what the route covers, the per-case PASS receipts with
paths, any product fix with its own LOC accounting, and the
`Production LOC:` line.

## 8. What is NOT yours this round

- Task 3 (architecture/operation-matrix update, reproduction commands).
- The final whole-core checks and the #179 closure report.
- The push, the merge, and closing the issue (owner acts).
- Any numeric bound, admission rule, worker count, or cache policy change.

## 9. Copyable prompt for the next agent

```text
Work task 2 of issue #179: the mounted FUSE syscall evidence for the six
namespace operations.

Use /Users/yifanxu/.codex/worktrees/795c/layerfs on codex/pair1-remote-mount.
HEAD is a9657b85f (task 1: the post-Commit rename fix and the completed
commit_small_project scenario), the working tree is clean, and nothing is
pushed.

Read AGENTS.md and core/AGENTS.md, then:
  core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/51-implementation-completion-spec.md
  core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/55-issue179-realdaemon-handoff.md (sections 5, 6, 10)
  core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/56-issue179-mounted-syscall-handoff.md
56 is your assignment: section 4 is the checklist, section 5 the exact build
and run mechanics, section 6 what to do if a case fails, section 7 the
verification and commit discipline.

Deliver two new test files and nothing else unless a real product defect is
demonstrated:
  1. core/crates/layerfs-workspace/tests/mounted_namespace_route.py — the
     mounted route specialising namespace_route.py's driver, modelled on
     mounted_mkdir_route.py and mounted_create_route.py (CASES, TEST_SOURCE,
     TEST_PREFIX 'mounted_namespace_', TEST_MARKER 'MOUNTED_NAMESPACE_CHECK',
     MODE 'functional-mounted-workspace-namespace', NOT_RUN naming the
     committed-directory move refusal among others).
  2. core/crates/layerfs-workspace/tests/mounted_namespace.rs — Rust cases
     modelled on mounted_create.rs, covering kernel mknod, link, unlink with a
     held FD, rmdir empty and nonempty, rename cross-directory plus
     replacement with the destination FD held plus NOREPLACE collision plus
     directory-cycle refusal, and setattr chmod plus UTIME_OMIT mtime.
     Assert errno, inode identity, link counts, listing visibility and
     post-Commit durability through the real mount. Combine related
     assertions; keep the case count small.

Known limits you must respect and declare: moving a committed directory is
refused (Unsupported — no identity-keyed service query in this profile; do
not write a success case for it); the arena update admits at most 4 cells per
call (1 for E/T keys); post-Commit renames of committed files work; a
metadata-only setattr keeps the record's current unnamed fields.

Build the Linux test binary in lfs-build-179 and run each registered case
once into its own fresh output directory under
core/target/pair1-evidence/round57/. One run at a time; receipts are
append-only; report FAIL and INCOMPLETE as plainly as PASS. If a case fails,
diagnose with a clearly labelled diagnostic build before editing anything,
fix the owning layer, and re-run all ten native namespace selections plus
commit_small_project on the fixed source (56 section 6 has the exact
commands). Never raise a numeric bound or change failure ownership to make a
case pass.

Before committing: cargo fmt --check, clippy -D warnings on the core
workspace, the boundary guard and its self-tests. Commit with the exact
per-commit production LOC accounting (test-only: report the unchanged
totals, core 50287 / reference 65417 / combined 115704). Do not run or claim
the whole-core test suite — it runs once on the final frozen source later in
the round. Do not push, merge or touch the issue.
```
