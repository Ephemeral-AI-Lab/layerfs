# S3 exit audit — immutable base access and effective view

> **Status:** CHECKPOINT; S3 is incomplete. Primary completion target after S1/S2 closure.

The [S3 row](../303/07-implementation-validation.md#3-slices) requires actual
immutable reads, exact caches/EOF/attributes, effective base-overlay merge and
retained roots across install. Existing content reads are at `d9d8d1b04`; the
prepared install slice follows `8e2976e4e`.

| Criterion | Current code/evidence | Remaining required work |
| --- | --- | --- |
| Current public root/child/inode/list/readlink/range APIs | BaseView uses FilesystemRead/FileView, authenticates IDs and checks scope; real content-built alias/symlink/complete-name fixtures pass | Preserve these paths through effective view/native/runtime composition |
| Exact immutable cache keys, EOF and attributes | Object-ID cache with bounded admission/bypass; old/new roots and read plans remain distinct; portable metadata and bounded clamped ranges pass | BaseView stat still classifies FileView for regular-file length; P5 owning runtime lengths must reach actual stat integration |
| Retained roots across actual install | PreparedBase checks one canonical root before short SQL install; selected binding changes alongside engine base; old plans/aliases stay on old root; later engine rows/tickets survive | Independent reader/orphan/kernel custody and live reclamation remain S6/S8; no future distributed GC lease inferred |
| Effective base-overlay merge | Typed engine point values, binary dentry whiteouts and fixed capture windows exist | Implement ordinary effective lookup/inode metadata and bounded ordered live-name merging; no unbounded resident namespace mirror or full-base alias walk |
| No mount scan/copy/special empty branch | Workspace open and prepared install each acquire one root, then fixed engine metadata; no namespace import | Actual native attach/full execution readiness is S8/S9; do not infer it from library binding |

Final prepared-install product identity passes all prescribed core checks: locked
all-target tests566, warning-denying Clippy, formatting, boundary494 Rust/SQL files
and25 tool self-tests. Actual Linux ARM64 Workspace all-target tests4 pass. Raw
[host](checks/s3-install/core-test.log), [Linux](checks/s3-install/linux-test.log)
and [Clippy](checks/s3-install/clippy.log) logs are retained. Rust1.85.1, locked core
manifest/lockfile, root ARM64 flags and image
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`
are unchanged; targets/cache are checkout-local. There are no new SQL templates;
S2 install EXPLAIN/runtime evidence is reused at its exact query scope. No new
performance/native campaign or large-root claim is made.

Next ready slice: effective point lookup/metadata and bounded ordered names,
using the existing canonical and S2 generation boundaries. Resolve concurrent
view selection through short backed/owned plans, without retaining a SQL owner
across provider I/O. Then reconcile every S3 exit before a completion commit.
S0's payload/failure/pressure algorithm gates remain named dependencies for S5/S6;
S1/S2 remain complete and fuser native acceptance remains independently blocked.

## Source-window prerequisite checkpoint

The slice after `6e84b9181` implements the [source fence](S3-SOURCE-FENCE.md):
exact backed root windows, maintained readiness count and fair six-class install/
acquisition ordering. Host571/Linux30 tests and all prescribed checks pass; raw
logs are [s3-source](checks/s3-source/core-test.log). Source-family point work is
macOS14/Linux13 VM with zero scans/sorts beside128/1024/4096 unrelated owners.
Effective lookup/metadata/name merging and actual provider/native actor composition
remain required; S3 is still unchecked. [Architecture](../../architecture/28-base-source-windows.md)
records implementation, work scope and resource debt.

The final source-change evidence also covers insertion VM and counter/delete
point plans with actual99/123 VM macOS and94/117 VM Linux live acquire/release work,
including all invoked families. Covering571/30 tests and boundary496 files pass
after adding the diagnostic API; initial passing570/29 logs are preserved with
their earlier scope. [DML diagnostic](checks/s3-source/source-change-profile.log).
