# #273 → #264 continuation: functional proofs, retained failures, NO MERGE

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Checked 2026-09-29 UTC. Proof source `05875fae8369bc5e4a7994447fa9aba42945cb75`
> (tree `c9ad8305566204c7fc1b23fe1df5d5b3a1229ca9`); its product source is
> `b551ebbd80c6e419e75f63ccfad2e5abe6a1ec19`. This docs-only report
> follows that source. It neither updates a PR head nor re-dates the
> [pre-merge completion ledger](PREMERGE-COMPLETION-20260928.md), the
> [partial union checkpoint](PREMERGE-FINALIZATION-PARTIAL-20260928.md), the
> [initial union repair](PREMERGE-UNION-REPAIR-20260928.md), or any raw receipt.

## 1. Live ownership / PR decision inputs

Rechecked with `gh pr view --json number,state,isDraft,headRefOid,baseRefOid,baseRefName`
and `gh issue view 276 --json state,updatedAt,comments` on 2026-09-29 UTC.
No tracked PR head or #276 owner comment moved; no PR was merged, rebased,
updated or closed here. The remote `main` base of unrelated #275 is
`f74dbe77…`, not an instruction to update the stale local `main`.

| PR | Head | Base commit | State | Recommendation on these heads |
| --- | --- | --- | --- | --- |
| #262 | `6bcfa464` | `6af2c5c` | OPEN draft | NO MERGE: the prior bounded source review stands; no admission from this union. |
| #272 | `48b51e8` | `3c3343e` | OPEN draft | NO MERGE: no matched numeric admission. |
| #274 | `29fc5747d` | `48b51e8` | OPEN draft | NO MERGE: this PR head lacks the owned C1 repair, SDK lease and union corrections. |
| #263 | `ef3a3104` | `6af2c5c` | OPEN | NO MERGE: reviewed ancestry is in the owned union, not in this PR's merged decision. |
| #269 | `6eb7553` | `ef3a3104` | OPEN draft | NO MERGE: its owned seal repair and union corrections are not this PR head. |
| #275 | `c39d6085` | `f74dbe77` (`main`) | OPEN | Unrelated statfs PR, NOT REVIEWED. |

#276 remains OPEN; its latest comment is the owner direction at
2026-09-28T15:40:26Z. Functional-only deferral of #256 and #270 is **not** a
numeric waiver or permission to merge drafts. No new owner ruling was invented.

## 2. Source chain and production LOC

All commits are in the new owned branch `codex/issue273-finalize-owned`; its
first parent is the inherited docs-only `1a0a48d66`. Every row was counted
against its **first parent and final staged/committed tree** using the same
`python3 tools/production_loc.py --json --root <archive snapshot>` counter.
The snapshot archive may contain only `core/crates crates`: these are the
counter's complete declared source scopes (product Rust `src` and runtime
`sql`), excluding tests, tools, manifests and docs. The earlier full-tree
archives and these scope-preserving archives produced the same totals. Reference
LOC is **65,417** in every row; Core and combined deltas coincide.

| Commit | First parent | Core before → after | Combined before → after | Delta | Scope |
| --- | --- | ---: | ---: | ---: | --- |
| `f54bbb123` | `1a0a48d66` | 70,457 → 70,408 | 135,874 → 135,825 | −49 | Identity fallback / in-process service fixture; partial union checkpoint. |
| `543fa4e75` | `f54bbb123` | 70,408 → 70,582 | 135,825 → 135,999 | +174 | Charged extended active paths, attached/retained ancestry, real refund points. |
| `c0ab3e5c8` | `543fa4e75` | 70,582 → 70,591 | 135,999 → 136,008 | +9 | Distinguish internal retained ancestry from public `getattr`. |
| `e97a544f5` | `c0ab3e5c8` | 70,591 → 70,591 | 136,008 → 136,008 | 0 | Fix route receipt's Linux Cargo target identity. |
| `6826d852d` | `e97a544f5` | 70,591 → 70,592 | 136,008 → 136,009 | +1 | Report known post-MetadataSave local failure as LocalBookkeeping. |
| `277afbc5f` | `6826d852d` | 70,592 → 70,592 | 136,009 → 136,009 | 0 | Four-MiB native SaveFile fault input. |
| `e06eebb0e` | `277afbc5f` | 70,592 → 70,645 | 136,009 → 136,062 | +53 | Charged resident canonical-name memo for G2 during G1 SaveFile. |
| `c6875bb5c` | `e06eebb0e` | 70,645 → 70,645 | 136,062 → 136,062 | 0 | 64-MiB declared distant-read input, bounded setup windows. |
| `b551ebbd8` | `c6875bb5c` | 70,645 → 70,653 | 136,062 → 136,070 | +8 | Retain known FileSave root before fallible metadata call. |
| `05875fae8` | `b551ebbd8` | 70,653 → 70,653 | 136,070 → 136,070 | 0 | Metadata-only test uses a real public attribute mutation, not a zero-byte no-op. |
| This report commit | `05875fae8` | 70,653 → 70,653 | 136,070 → 136,070 | 0 | Documentation-only, source-pinned NO-MERGE handoff. |

The prior combined-tree merge conflicts and their first-parent LOC are recorded
in the [completion ledger](PREMERGE-COMPLETION-20260928.md#6-combined-tree-owned-worktree-issue273-combined).
No RootOwner mutation route was resurrected; the canonical 4,096-byte format,
budgets, worker limit and benchmark limits were not raised.

## 3. Product/profile/image/fixture identities and raw custody

Host: Darwin 25.4.0 arm64, rustc 1.85.1. Owned Docker Desktop Linux:
`6.12.76-linuxkit` aarch64, **ext4** private volume, `/dev/fuse`, privileged
`SYS_ADMIN`/`MKNOD`. Linux functional rust image was pinned by immutable ID
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`
(`rust:1.85.1-bookworm`). Host Service/fixture binaries were locked Cargo
release; Linux external test binaries were locked 1.85.1 debug in this
worktree's `core/target/issue273-linux-owned`. The attached Workspace ran
with `LAYERFS_CONSTRUCTION_WORKERS=1`. This was a **functional-only** profile;
`cache_claim=None` and `performance_claim=False` in stage route receipts.
No phase-local memory peak or numeric performance was sampled.

The final daemon was built from the product source with locked
`cargo +1.85.1 zigbuild --release --manifest-path core/Cargo.toml --locked
--target aarch64-unknown-linux-musl -p layerfs-daemon`, using the repo-root
ARMv8 AEAD config (SHA256 `3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9`).
It is static aarch64-musl, SHA256
`d2170979d19b1537e072ed7a945ac1af303ae7b536d12e009e14fa7abd5bcded`.
The owned image uses **FROM**
`alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`,
`COPY layerfs-daemon /layerfs-daemon`, daemon ENTRYPOINT, and immutable image
ID `sha256:c86215e4015e6b653b2f6985e8c3da3d1f7b6b3f96f61abdcb3e14183d6a1cab`.
No mutable image tag was passed to the SDK sandbox owner.

Fixture: independent byte copy of the **read-only** #271 closed Store master
(SHA256 `20ea70dc9509e1819b11667bf97ab2a5ab3bd2b5d628d0a48f4c4fa6876691be`),
prepared once in this worktree by `prepare_large_edit.py` from the owned
product binaries; its `result.json` SHA256 is
`493e1067fd1063c3d3617426ff6f5b96ed3fc0587e44561af70225d743445d22`.
The fixture's closed Store SHA256 is
`b33cfa3b7a3cef0ce29073fe77f6d33b7afccdc5502cd365bca32047cd51eba6`;
it was reused by independent byte copy outside the tested operation. This is
**not** a cold-cache or speed claim. The fixture and all 135 attempts have
new `SHA256SUMS` files in their **owned, gitignored** `core/target/issue273*`
folders; none is a committed or externally published receipt. Every earlier
FAIL remains on disk; a failed harness container/volume was forcibly removed
*after* its logs/inspect data were captured, **not** counted as a product
`close_clean` PASS. No other owner's worktree, target, receipt, image, volume or
container was modified.

Final stage receipts pin `source=05875fae8`, product-input SHA256
`618ca7c162cfcc…`, stage-driver SHA256 `074bd799e572f1…`, the Linux stage
binary SHA256 `2cc9e2731b58…`, the closed master SHA above and the immutable
rust image ID. The final lease image ID is separate from that rust route image.
These suffix-truncated SHA labels are pointers; use the complete identities
in each local-only `result.json`/manifest before replay. One example:
`core/target/issue273-058-stage-active_hot_continuity/SHA256SUMS` has SHA256
`75c5e2cfc88a638cb93485a92bc08074c86345d02b882f7a9c6bf417ea3cdc14`;
`core/target/issue273-058-mounted_kernel/SHA256SUMS` has
`5b246f345fcc2113d56351cfa2aa74fc86f42fc8ac7af31012bc83a0443c1ccb`.
These are **local-only paths**, not GitHub raw links.

## 4. Exact clean-tree functional matrix (PASS and missing cells)

Use the public registered drivers; each creates a fresh output directory and
its own disposable container/volume, then records cleanup. The command shape
from the repository root, with full worktree-local absolute paths substituted:

```sh
PATH="/usr/local/bin:/opt/homebrew/bin:$PATH" LAYERFS_CONSTRUCTION_WORKERS=1 \
 python3 core/crates/layerfs-workspace/tests/stage_route.py \
 --fixture core/target/issue273-fixture-v1/result.json \
 --binaries core/target/release \
 --test-binary core/target/issue273-linux-owned/debug/deps/stage-d07f247be8978535 \
 --output core/target/issue273-stage-NEW-OUTPUT-active_hot_continuity \
 --image sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4 \
 --case active_hot_continuity
```

Use a fresh, previously nonexistent `--output` path for reproduction; the
recorded PASS attempt is in the separately named
`core/target/issue273-058-stage-active_hot_continuity` local-only directory.
The driver resolves all paths before use and requires them inside this
worktree; its recorded invocation uses absolute paths. `namespace_route.py`
and `mounted_namespace_route.py` use the corresponding source and Linux test
binary and the same fixture/image, selecting each registered `--case` once.
The native-service route and the actual kernel-mounted route are different
proofs; a direct cargo test on overlayfs is not substituted for either.

| Cell | Exact result / evidence scope at `05875fae8` |
| --- | --- |
| #269-era inherited SDK union | **11/11 PASS**, Linux/ext4, `cargo test --offline --manifest-path core/Cargo.toml --locked -p layerfs-sdk --test inherited_workspace -- --nocapture`; includes >4,096 active moves, move-back, detached-parent refusal, quota and refund. |
| Readable Workspace | **18 PASS, 0 FAIL, 2 ignored**, same Linux/ext4, `-p layerfs-workspace --test readable`; ignored mounted CREATE/notifier tests remain **NOT_RUN through their own drivers**. SDK agent route **3/3 PASS**. |
| Native namespace route | **10/10 selections PASS**, each with checked native clean close, including `rename_base`, generation, replacement, unlink/orphans and refusal. |
| Kernel namespace route | **2/2 selections PASS** (`kernel` and `durability`), actual FUSE writes/renames, old/open identities and committed readback; checked clean close. |
| Active G1/G2 and handles | Stage `active_hot_continuity`, `active_hot_publication`, `active_generation`, `active_namespace`, `active_close`, `active_mounted`, `semantics`, `head_moved`: **PASS** for their named checks. `active_hot_continuity` holds the same mounted process/fd across SaveFile/C5 and continued G2 write. |
| Tiny/Base/Zero/Payload/overlap/slot | Stage `active_tiny_input_4097`, `active_generic_profile`, `active_mixed_compact`, `active_split_slot`, `active_payload_refund`, `active_repeated`, `active_page_profile`, `active_separated4096`, `active_retained32`: **PASS** for their named checks. These are **not** the original registered #248 gate. |
| Quota/Budget/custody | Stage `active_quota_refusal`, `active_known_budget_g2`, `active_known_c5_g2`, `active_cleanup_failure`, `native_save`, `unknown_save`, `metadata_only`: **PASS** for their named checks. The `unknown_save` observer (read-only `F_GETLK`) was self-tested; observed service PID owned SQLite RESERVED byte before/after SIGSTOP, G2 local progress while stopped, then an external SIGKILL. This is an uncertain **SaveFile**, not an uncertain SDK view release. |
| Further stage route inventory | **27/27 selections PASS, 59/59 named checks**, plus native namespace **10/10** and mounted namespace **2/2** (39 registered route selections, **83/83 checks** including route cleanup) on this source. Other stage selections are **NOT_RUN on this source**; earlier-source results are not silently transferred. |
| Public SDK retained-view route | **1/1 live PASS** against the final immutable daemon image: pinned G1 root/revision and exact old G1 bytes vs live G2 bytes, listing after rmdir, malformed/forged/cross-lease/stale refusals, 32 admit/33rd Capacity refusal, checked release and unmount. Host locked test `-p layerfs-sdk --test workspace_view -- --nocapture` with `LAYERFS_TEST_IMAGE=sha256:c86215e4…`. |
| Original #248 public **4,097 separated WRITEs** with anchored C1 edit-load-zero provenance | **NOT_RUN on this source**. The 4,097 tiny/repeated/source-grouped stage PASSes and 4,096-separated stage PASS are different workloads and cannot fill this cell. Historical INCOMPLETE/INELIGIBLE #248 rows retain their statuses. |
| SDK lease deadline, exhausted response Budget, held lease across known C1/local C5 failure, uncertain checked release | **NOT_RUN**, exact deterministic forcing/physical-fault mechanisms not implemented. The stage C1/C5 and SaveFile loss probes above do **not** hold an SDK lease or force its release. |
| Registered clean/one-edit public SDK benchmark controls | **NOT_RUN** under the required prospective matched harness/cache identity. `active_quick_controls` at older product source is a stage functional probe, not this registry. |
| #256 many-file/package scale; #270 path-local C1 move Commit | **NOT_PROVED** (functional-only deferral, separate follow-up). One active 128-file case is not package-scale admission. |

For the route proof, `memory.peak` reset is **falsified** on this Docker
Desktop host, and prior `memory.reclaim` plus phase-local current/stat deltas
remain only a **PARTIAL** capability result. No VM/backend/device cache identity
or sealed observer/matched frozen-control profile was established in this pass.
No product performance arm was sampled. The nine historical numeric rows remain
**INELIGIBLE** and matched frozen control **NOT_RUN**. No 25-second censored
control wall was made a denominator. Thus functional PASS is **not** speed or
release admission.

## 5. Retained FAILs, gaps and exact stop reason

The local-only raw attempts are append-only and must not be rewritten as PASS:

- `core/target/issue273-e06-stage-lowering`: **FAIL** at `InvalidInput`
  because the then-default imported `data.bin` was 326,300 bytes but the
  test's distant oracle read at 32 MiB. A prospectively corrected 64-MiB
  fixture was tried at `c6875bb5c`:
  `core/target/issue273-c687-stage-lowering-fixed` **FAIL** at
  `Backing(Acquire, StorageFull)` during the test helper's full-tail splice
  under the existing 64-MiB backing quota. Neither failure was re-labelled,
  the quota was not enlarged, and that 64-MiB case is **NOT_RUN on final
  source**. Bounded test/profile or product-custody ruling remains open.
- `core/target/issue273-c687-stage-headroom`: **FAIL** at
  `Backing(Allocate, StorageFull)` under the intentionally occupied 2-MiB
  quota. The active mutation route holds no RootOwner completion escrow; the
  old route's success-under-full-quota expectation does not follow. No raised
  quota or weakened assertion is claimed. Explicit owner disposition is open;
  final-source headroom **NOT_RUN**.
- `core/target/issue273-c687-stage-metadata_denied` **FAIL** (missing known
  pending FileSave root); repaired with `b551ebbd8`, proved **PASS** on that
  source with the original custody assertions. On `05875fae8`, this exact
  case was **NOT_RUN**; the product and test body are unchanged, but its test
  executable changed for a different case and is not silently promoted.
- `core/target/issue273-c687-stage-metadata_only` **FAIL** (a zero-byte active
  WRITE is a no-op). The external test now uses the actual public
  metadata-only `set_attributes` operation and asserts selected mtime, one
  MetadataSave and no FileSave; **PASS** at `05875fae8`. No assertion was
  deleted.
- Earlier native SaveFile-loss attempts failed first on an undersized
  four-MiB input and then because G2 lookup called the paused host service.
  The exact failures remain. A charged, baseline-bound resident name memo
  repaired local G2 progress; the final `native_save` and `unknown_save`
  registered functional routes both **PASS** at `05875fae8` with external
  lock observation and cleanup.
- The live SDK view-lease attempt at `c0ab3e5c8` **FAILed** at its
  unregistered-token refusal assertion (the unexpected error code was not
  printed). A labelled diagnostic with only an added error print passed but
  was not kept as a new admission receipt. Later clean-source route proofs
  at `e06eebb0e` and `05875fae8` **PASS**; the earlier FAIL is retained,
  its precise cause is **UNRESOLVED**, and it cannot be called pre-existing.

**Decision: NO MERGE on all reviewed PRs, NO RELEASE ADMISSION.** The original
#248 gate, four SDK falsifiers, cache/phase observer and matched numeric
contract remain missing; the legacy lowering/headroom FAILs need explicit
bounded dispositions. Passing 39 functional route selections and all scoped
Core checks does not answer those independent gates or provide owner
permission to merge or close #276.

## 6. Broad checks and cleanup

On the clean `05875fae8` tree, locked macOS Core tests were split by package
because the earlier aggregate command timed out at 160 s on its own older
source. **All 12 packages completed**: `layerfs-api-core` 0, bridge 78,
content 276, daemon 3, fuse 0, history 30, sandbox 1, SDK 9, server 51,
storage 228, telemetry 43, Workspace 17: **736 passed, 0 failed** on the
macOS-visible test subset. Exact command per package:
`cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p <package>`.
This does not run Linux `#[ignore]` routes. `cargo +1.85.1 build --manifest-path
core/Cargo.toml --locked --workspace --examples`, warning-denying
`cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace
--all-targets -- -D warnings`, `cargo +1.85.1 fmt --manifest-path
core/Cargo.toml --all -- --check`, `python3 core/tools/check_product_boundary.py`
(359 files), its nine self-tests, and `git diff --check`: **PASS**.
The full aggregate all-platform or locked-release Core suite is **NOT_RUN**
on this final source; no CI/preflight claim is made.

Owned containers and volumes used by successful tests were removed by their
registered drivers. Failed-driver containers/volumes were identified from
*our own* retained `result.json`, their inspect/log output saved to that
attempt's local directory, and then forcibly removed; this is **harness
cleanup**, not proof of product clean-close. The worktree was clean at the
proof commit; no other owner's Docker resources, target or receipts were
touched. The held local image IDs and incremental Cargo targets are retained
only as owned build artifacts, not as numeric evidence.
