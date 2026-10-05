# S6 lifetimes and reclamation exit audit

> **Status:** Completion audit for the local S6 completion commit containing this
> document: `983c2ee6d36a4417d8fff2d14db6b141f5386c8c`, after `be651a048`.
> Component acceptance, not release evidence or
> integrated FUSE/Exec/Commit qualification. Exact source/build/receipt identities
> are in [s6-pressure](checks/s6-pressure/identity.json).

## Criterion reconciliation

| Required S6 exit | Delivered behavior and owning evidence |
| --- | --- |
| Independent orphan custody | Minted open/lookup/read tokens, exact immutable inheritance root and gen=-1 independent mutable domain. Last unlink/replacement does fixed metadata work, retains <=2 existing lower domains and never adds a domain on later namespace captures. [Custody checkpoint](S6-CUSTODY-CHECKPOINT.md), [name checkpoint](S6-NAME-CHECKPOINT.md), final [Workspace proof](checks/s6-pressure/workspace-final.log) |
| Stable open-unlinked logs through success/failure | Actual canonical-root owner proof executes24 captures:12 known installs and12 fenced definite failures, appends/truncate/regrow,9180 orphan bytes and10 inherited bytes. A final independent read outlives descriptor close. The non-file owner proof executes12 captures with directory metadata and original symlink-root bytes. Both reach idle last-owner cleanup |
| Bounded repeated failure/success composition | One active plus one consolidation namespace domain; next capture waits, writes continue. One cell/name per fold, selective generation retirement, engine-maintained relative cutoffs and absolute born values. Unknown history does not enter failure resolution. [Live composition](../../architecture/32-live-composition.md), final overlay lifetime and three-layer tests |
| No growing orphan read depth or payload-sized foreground fold | Orphan depth<=3 while its fixed lower sources remain, then1. Capture/failure/unlink/install/releases do fixed metadata work. Retained snapshot sources park migration. A2MiB/512-cell proof never increases stored payload cell count during migration and preserves every byte; physical pages628->629, not a payload-sized duplicate. [Final resources](checks/s6-pressure/resources-sealed-host.log), [Linux resources](checks/s6-pressure/linux-resource-observation.log) |
| Exact reader/capture/operation/lookup ownership | Never-recycled minted owner IDs, retained request observations, stale release refusal, original-root captured readers after install and separate processing scratch. Old operation cleanup cannot delete new scratch. Removed non-file lookups/request readers survive close. Native FORGET/RELEASE/interrupt mapping is still S8 |
| Last release has bounded foreground work | A generation release now queues a wake instead of updating arbitrary target populations.128/1024/4096 targets all cost11 statement executions,411 VM,6 changed rows on macOS (396 VM Linux). First64-key wake page costs135 executions/11006 VM (10614 Linux); cumulative turns131/1041/4161. Exact generation/ready index plans accompany actual counts |
| Live/idle automatic weighted reclamation | Existing daemon rotates live/terminal items after at most8 foreground jobs and while idle. Jobs process<=14 payload cells/65536 bytes,<=64 small records or one transfer. Stale epochs/cutoffs are rechecked; abandoned staircase rows and redundant whiteouts are reclaimed; exact source release wakes parked work. Failed attempted maintenance retains its first error/cursor/custody without replay |
| Physical reservations/headroom | One private descriptor and database per daemon,128MiB mutation growth plus128MiB cleanup reserve, actual range allocation and committed freelist credit before BEGIN. Root/schema/profile creation also reserves first. No sync/VACUUM/zero-fill fallback. Conservative source arithmetic and qualification limits are in [reservation selection](S6-RESERVATION-CONTRACT.md) and [architecture35](../../architecture/35-shared-physical-capacity.md) |
| Actual device-full admission and cleanup | Exclusively owned Linux ext4,512MiB loop image. Original allocation ENOSPC is retained; refusal occurs before BEGIN with old capture/bytes and unrelated namespace intact. Overlay case:82->118 DB pages,0->72 reusable pages,268771328 physical bytes unchanged. Real owner accepts one distinct reserved window, refuses the next, then idle cleanup frees all129 files/528384 bytes:190->192 pages,0->146 free pages,269213696 physical bytes unchanged. Exact normal unmount/detach/artifact teardown passes. Device receipts are in s6-pressure |
| Actual accounting/debt | Transactional namespace/global primary-key counts include stored row/BLOB/mask/scratch bytes, source/lease/reference detail, reply tickets, orphan waits and maintenance/retirement/readiness targets. No live COUNT/sweep. Shared allocation/page/freelist/tail observations are distinct from logical attribution. Debt is a conservative logical upper bound, includes held targets and is not exclusive obsolete physical bytes |
| Definite/uncertain failure custody | Real SQLite page-quota FULL remains one known rollback with prior publication retained. Actual backing-path loss retains unchanged renamed backing and causes Uncertain before BEGIN. Real backing corruption produces original SQLite DatabaseCorrupt and quarantine; exact capture/release authority is retained, never guessed. Eight repeated resource snapshots avoid expiring-statement reprepare. History/transport/COMMIT integration uncertainty is S9/S10 |

R4 and R6 are resolved at this S6 engine/Workspace/owner scope. R8's independent
local custody and bounded owner resources are resolved here; aggregate kernel,
transport, output and authenticated runtime obligations remain S8/S9/S12. R1/S5
continues to pass with explicit new accounting costs, not unchanged old VM numbers.
S0 stays open. P3's deferred edit refusal remains explicitly carried to S10 as
recorded by the S5 audit; this audit does not silently remove it.

## Verification and reuse

Every Rust test invocation has a prior no-run build and explicit<=120s wall limit;
none in this completion slice reached it. `LAYERFS_CONSTRUCTION_WORKERS=1` is set.
Retained raw commands/outputs include all failures and unrun tails; [failure ledger](checks/s6-pressure/FAILURES.md)
explains the repairs and changed expectations. No loop repeats unchanged test
commands, no attempted product operation is replayed, and no third-party package,
version, checksum or source is changed. The overlay adds only an edge to the
already locked nix0.31.3 safe fs capability; the lockfile diff is that one edge.

Host verification covers all11 active packages through scoped outputs and
unaffected earlier-family reuse. The full-core attempt is permanently FAIL after
302 passed and one compound assertion failed; it is not relabelled aggregate PASS.
After repair, overlay42/Workspace32/daemon9/SDK6 pass; final Resources adds its sixth
case and qualifies the changed resource-observation path separately. Unchanged
Bridge/content bodies passed before that failure. Persistence/project/storage/
telemetry tails have separate retained outputs. Native telemetry evidence at
`1775fdf98` is reused at unchanged std-only source/build scope.

Linux ARM64 Rust1.85.1 uses the pinned image
`e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`, repository root
config, separate core/target/cluster2-linux and cluster2-linux-cargo. The four-package
selection passes83, with two explicit device cases ignored until run in their owned
fixtures; SDK's owning provider cases are cfg-disabled on Linux and execute on
macOS. Final Resources executes6 on both hosts. Final affected resource/device
checks qualify later guaranteed-range and noncached observation changes; earlier
namespace/payload/custody profiles are reused because those SQL/data paths are
unchanged. Linux emits an existing unused macOS-only Persistence backend error
variant warning; it is not hidden or called warning-free.

macOS all-target Clippy with -D warnings, fmt, the product boundary guard and26 tool
tests are retained. The guard scans563 production Rust/SQL files and does not prove
semantics by itself. Current architecture/API documentation and link checks are
included. No CI/aggregate pre-push gate is claimed or restored.

This is correctness, capacity and count evidence. Cache state is uncontrolled;
no cold speed/RSS/latency/storage-improvement or universal sustained-throughput
claim is made. Full physical reservation/high-water pages count as storage,
including the256MiB per-daemon startup cost. Repeated Linux allocation requests
can overlap the same owned range; requested_bytes is syscall argument volume,
not newly allocated bytes. S7 owns integrated cost/amortization/residency gates.
The two selected device resource proofs include setup/normal teardown under1s.
Functional package wall times retain the120s test scope. The declared compilation
interference has no eligible timing interpretation.

## Remaining boundaries

- Owning real device-full qualification is Linux ext4. macOS allocation and
  functional correctness pass on this host; macOS device-full/COW behavior and
  other filesystems are unqualified. External clone/punch/truncation/mutation,
  snapshots/COW interference and device failure are outside the exclusive dense
  backing assumption. Unsafe outcomes quarantine rather than manufacture success.
- Disposable backing has no crash durability/recovery promise. Logical cleanup
  need not shrink the shared file. Global history is untouched.
- Raw cell APIs remain stored observations; composed source/captured/reader APIs
  define the effective view. Kernel caches, writeback-off profile, request/open/
  lookup ownership, streamed output and terminal native teardown remain S8.
- Fuser0.18.0's retained pre-callback signed timestamp blocker remains S0/S8/S12.
  No third-party patch or contract reduction is introduced.
- P3/P6/P7/P13/P14 backed construction, complete native import, authenticated
  runtime/history resolution and actual incremental Commit remain S9/S10. Root
  legacy reference retirement follows S12/S13, not S6.

The completion commit records exact first-parent/staged production LOC, including
shipped accounting SQL and excluded predecessors. The counter is unchanged;
reference65417 remains separate. See the commit message, staged receipt under
core/target/cluster2-307/loc and the [next handoff](HANDOFF-S7-S13.md).

Production LOC for this completion slice:150533->151299(delta+766),
core85116->85882, reference65417 unchanged. Exact parent/staged-tree archives use
tools/production_loc.py at the pinned SHA256 above; shipped SQL is included and
all test/document/tool/harness/build sources are excluded. Receipt:
core/target/cluster2-307/loc/s6-complete-staged.json. Current unique host coverage
is632 passing bodies across scoped/reused outputs, with History's empty direct
unit target and its actual public behavior exercised through Persistence. Linux
unique component bodies are84 after the new snapshot case, plus the two separately
executed owned device proofs. These are coverage counts, not a relabelled aggregate
PASS or universal native/runtime acceptance.

Completion tree be2744223a450eaa01b9f31c4e3c850bbd141d72 matches the counted
staged tree. [Tracker receipt](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6000716717)
is posted, S6 is checked, and S0/S7–S13 remain unchecked. The exact text is
adopted in [checkpoints/983c2ee6d.md](checkpoints/983c2ee6d.md).
