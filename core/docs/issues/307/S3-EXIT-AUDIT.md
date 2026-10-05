# S3 exit audit — immutable base access and effective view

> **Status:** COMPLETE; S3 implementation and required exit evidence pass at the local milestone-completion commit after `bdc6ed4af`.

The [S3 row](../303/07-implementation-validation.md#3-slices) requires actual
immutable reads, exact caches/EOF/attributes, effective base-overlay merge and
retained roots across install. Existing content reads are at `d9d8d1b04`; the
prepared install slice follows `8e2976e4e`.

| Criterion | Current code/evidence | Remaining required work |
| --- | --- | --- |
| Current public root/child/inode/list/readlink/range APIs | BaseView uses FilesystemRead/FileView, authenticates IDs and checks scope; real content-built alias/symlink/complete-name fixtures pass | Effective source-qualified lookup/stat/list and actual daemon actor composition now use these paths; native adaptation is S8 |
| Exact immutable cache keys, EOF and attributes | Object-ID cache with bounded admission/bypass; old/new roots and read plans remain distinct; portable metadata and bounded clamped ranges pass | Explicit FileLengths/SDK bound port now serves actual stat from owning metadata; missing capability and authority errors fail without fallback; logical network metadata protocol is S9 |
| Retained roots across actual install | PreparedBase checks one canonical root before short SQL install; selected binding changes alongside engine base; old plans/aliases stay on old root; later engine rows/tickets survive | Independent reader/orphan/kernel custody and live reclamation remain S6/S8; no future distributed GC lease inferred |
| Effective base-overlay merge | Typed engine point values, binary dentry whiteouts and fixed capture windows exist | SourceView implements effective lookup/stat and bounded three-way name merge, including whiteouts/alias metadata/empty-page progress; S4 mutation and native cursor/reference ownership remain separate |
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

At the prepared-install checkpoint, the next ready slice was effective point lookup/metadata and bounded ordered names,
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
At that checkpoint, effective lookup/metadata/name merging and actor composition
remained required. The completion reconciliation below records their delivery. [Architecture](../../architecture/28-base-source-windows.md)
records implementation, work scope and resource debt.

The final source-change evidence also covers insertion VM and counter/delete
point plans with actual99/123 VM macOS and94/117 VM Linux live acquire/release work,
including all invoked families. Covering571/30 tests and boundary496 files pass
after adding the diagnostic API; initial passing570/29 logs are preserved with
their earlier scope. [DML diagnostic](checks/s3-source/source-change-profile.log).

## S3 completion reconciliation

All S3 exits in the criterion table are implemented and proved. The closure source
adds [effective read composition](../../architecture/29-effective-base-view.md),
source-qualified owner reads, actual prepared actor install, SDK owning stat lengths
and the raw canonical resume boundary. Scope remains immutable/effective read
interfaces; ordinary namespace mutation, byte inheritance/truncate/holes, independent
orphan/kernel custody and aggregate resource qualification are S4–S8.

Final product identity passes locked core all-target tests576, warning-denying
Clippy, formatting, boundary502 Rust/SQL files and26 tool self-tests. Actual Linux
ARM64 overlay/daemon/Workspace/SDK all-target selection passes34 tests; SDK global
Store cases execute on macOS and are compilation-only on Linux. Test-only style/
type-alias repairs have covering Workspace7 Linux and SDK5 macOS passes; all
unaffected qualifying rows are reused. Raw [core](checks/s3-view/core-test-final.log),
[Linux](checks/s3-view/linux-test.log), [Clippy](checks/s3-view/clippy-repaired.log),
[repaired Workspace](checks/s3-view/linux-repaired-workspace.log) and
[repaired SDK](checks/s3-view/sdk-repaired.log) receipts are retained. Initial mode-
width/private-field compiler errors, stale dependency-edge guard rejection and
fixture Clippy failures are preserved; none is relabelled. The guard now covers
the documented daemon/SDK composition edges with rejection tests for reverse/
domain/server dependencies. Locked third-party records and root ARM64 config
are unchanged.

The name EXPLAIN/runtime [diagnostic](checks/s3-view/name-window-profile.log) and
final Linux complete window counters show visited-scope bounds; prior source/
install point plans retain their pins. The [owning stat diagnostic](checks/s3-view/owning-stat-profile.log)
returns131071 logical bytes,2 namespace refs and zero Store payload/pack reads;
its path reuses P5 locator EXPLAIN/profile. The blocked actual canonical provider
case demonstrates SQL progress during I/O and actual engine/Workspace root install.
The 507-key/130-whiteout case yields377 visible entries, including an empty progress
page. These are deterministic source/work proofs, not fresh timing samples.

No S3 exit remains. Remaining S0/R1–R8/P1–P14, S4–S13 requirements retain their
own checkboxes and failures. The fuser timestamp blocker remains S0/S8/S12 and
unchanged under the owner restriction. The owner's revised boundary is to produce
HANDOFF-S4-S6.md and stop before starting S4; it supersedes this thread's earlier
continuous-through-S13 direction without changing the overall tracker plan.
