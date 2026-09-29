# #286 compound retained-history profile v1

> **Status:** Dated benchmark specification; frozen before the first compound candidate sample. No release admission.

This implements the owner-directed [Phase B plan](PHASE-B-PLAN-20260929.md) for family 2. The existing Rust driver remains the C1/C2 construction owner; `families/history_retention.py` is its canonical compound registry/adapter. Original `history-stride10/3/1`, C2-only receipts and `g1.o6-below-v016` retain their original meanings.

| New case | Existing backend selection | States | Strict C2+C5 allocated ceiling | Complete driver limit | Separate verifier limit |
| --- | --- | ---: | ---: | ---: | ---: |
| history-retention-stride-10-total-storage-v1 | history-stride10 | 17 | <49344512 B | 60 s | 10 s |
| history-retention-stride-3-total-storage-v1 | history-stride3 | 53 | <64024576 B | 170 s | 20 s |
| history-retention-stride-1-total-storage-v1 | history-stride1 | 157 | <83947520 B | 170 s | 30 s |

Order is stride10, then stride3, then explicit run-only stride1. A default family selection excludes stride1; completing family 2 requires its separate explicit case. One sample per case/arm at each meaningfully changed product/harness/method identity. No comparative speed claim or cold time claim is registered.

Complete-command ceilings are prospectively derived from the independent sealed-reference stride10 acquisition's 46.799667833 s, before candidate optimization: `min(170 s, ceil((1.25 × baseline × states/17) / 5 s) × 5 s)`. The cap respects the handoff's sub-three-minute command constraint. These history-specific ceilings do not import Init/Workspace's 15/25-second rule or the backend's obsolete 60-second verifier default. They will not be raised after a miss. Raw child-sum product time and external launch-to-exit driver time remain separate diagnostics.

## Declared operation and C5 work

Profile `c1-c2-c5-retained-history-v1`; receipt `core-history-retention-receipt-v1`; primary gate `g1.o6-total-retained-below-v016-v1`. Host-only public C1 construction/update, C2 create/save/finish and public C5 transitions; no SDK/FUSE performance claim follows. Setup is InProcess. Immutable corpus acquisition is reusable, but neither Store nor History is prepared or cloned. The driver creates both owners in the measured invocation and closes them before readings.

The fixed corpus is the original `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271` manifest, pinned tip `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`, all original selected transitions and logical bytes. Construction and Storage policies remain `frozen_default()`. Corpus reads happen between timed children, never inside one. Previous-root/content reads from the growing Store stay inside the corresponding child. No changed-state skipping, prepared Store, harness-held canonical base or fixture reduction is allowed.

The existing scope is `scope_for_seed(ObjectId::for_bytes("layerfs/history/" + row.token()).bytes)` and filesystem profile is C1's `profile_id()`. C5 binding key is the literal `layerfs/issue286/retained-history/v1`, incarnation 1, cursor key 32 bytes of `0x28`, stack authority body 16 bytes of `0x28`, Workspace authority body 32 bytes of `0x2a`. Branch body is 16 bytes initially `0x29` with its final byte replaced by the selected ordinal 1–157; name `state-<ordinal>`. Stack name is `retained`.

State 1's already-saved filesystem becomes genesis; its timer also pays for creating C5, initializing the stack and forking `state-1`. Every later state's child constructs/saves through the existing C1/C2 path, forks the prior published Layer, stages the exact new root, commits that exact token and adds one Layer. A C5 Branch base is immutable; successive publications therefore use per-state Branches. There is no raw-SQL insertion, mutable-base substitute, extra first-state Commit or retry. Retain every immutable Layer/Commit/Branch, with no pruning. C5 inode allocation is not used by this component driver; C1's existing deterministic chain allocator remains unchanged.

Expected persisted C5 counts for N selected states: `history_meta=1`, `layer_stacks=1`, `branches=N`, `layers=N`, `commits=N-1`, `workspace_stages=0`, `scope_allocator=0`. Genesis has no source Commit/Branch; subsequent Commits have no Commit parent and use the prior Layer as base. Each subsequent Layer names its own Branch/Commit and prior Layer. Verification reopens C5 and queries every selected Layer and Commit, checking root, context, provenance and final Branch/stack/stage state through public APIs. SQL is used only for independent source-aware accounting/integrity/row counts.

## Independent oracle and pinned counts

[The immutable root ledger](oracles/history-reference-v1/manifest.json) identifies the independent source, every vector's SHA256 and its acquisition/reuse provenance. The owner approved independent reference generation and then delegated subsequent decisions to this construction agent without further questions. Sealed reference is `2f07f1f37af3e06a92a00880c68882b3c91923ef`, tree `faec3221657ebaf2ba055929fe60c16822ae0928`. Stride10/3 roots come from one acquisition each in its separate clean managed checkout. Stride1 reuses the full retained historical vector whose recorded source `8be0ae1a779a455145ea6c2b75e2154521536fad` has byte-identical C1/corpus-reader/history-driver source to the selected reference. Its old numeric INELIGIBLE status remains unchanged. This is oracle reuse, not candidate sample reuse or a speed arm.

The candidate's own trace supplies observed roots only. Every root must match the separately committed, hashed reference vector in ordinal order. Native verification reads and authenticates the vector, and Python retains its source/seal in the receipt. C5 persistence comparison against the performance roots is a custody check in addition to independent O1, not a substitute for it.

Every state's complete listed path/kind tree and logical file sizes are compared against its fixed `oracles/<sha>.json`; extra, duplicate, missing/cyclic paths fail. Directory listing uses identity and grouped inode lookups rather than one full path resolve per entry. Content selection is each state's ordered non-directory manifest: index mod10==0 plus both endpoints. All bytes of every selected object are read through C1/C2 and hashed, with digest and length deduplication across states by distinct content object. The old at-most64-path sampled verifier remains unchanged for historical C2-only cases. The corpus's portable-mode limitation and absence of mode-only transitions remain explicit, as in the existing history contract; this lane does not invent a POSIX attribute oracle absent from its declared representation.

Canonical O3 pins remain strict: stride3 **589423458 B /73476 objects**, stride1 **871588115 B /104705 objects**. Stride10's first-run pins are now **380921328 B /52032 objects**, established by the independent reference and never reset. Canonical counters use distinct `object_id` with consistent role/length, with raw locator and per-role counts also retained. The sealed reference's stride3 observation **589916570 B /72560 objects** disagrees with the old fixed pins by **+493112 B /-916 objects**. This is an explicit unresolved canonical-profile finding, not a new pin or permission to qualify the candidate. The original pins remain unchanged; candidate qualification must meet them or retain FAIL while the shared cause is corrected.

## Storage, eligibility, cleanup and custody

The primary hard metric is actual closed at-run `st_blocks*512` of C2 `sample.sqlite` plus C5 `history.sqlite` plus any distinct required persistent index. The current profile requires no separate index file: C2's content signatures and both owners' embedded indexes already belong to their database allocation and count once. Actual allocated/apparent bytes are distinct from `page_count*page_size`, freelist, canonical bytes and pack bodies. Report all axes per owner and as applicable totals.

Exclusive allocation is established by fresh at-run creation under the sealed InProcess route, no prepared/copy/reflink input and one link per regular nonsymlink owner. Shared/reflink/unknown attribution is INELIGIBLE. Missing/unreadable allocation is INCOMPLETE. Valid exclusive total >=ceiling is FAIL, including equality. Native evaluator, independent verifier, Python collector and retained report rederivation enforce that distinction. Schema identity is C2 application ID1279677261/user_version10/six application tables and C5 application ID1279677256/user_version1/seven; SQLite `schema_version` remains a separate observed value. Integrity, foreign keys, retained row counts, pack accounting and absence of journal/WAL/SHM sidecars are required. Archive-copy blocks never decide the gate.

Storage gate, correctness, command budget, numeric time eligibility and cleanup have separate statuses. Under-budget uncontrolled-cache time remains INELIGIBLE and supplies no time PASS or speedup. Additional RSS/cgroup memory qualification stays #283; actual ownership/Budget/quota safety is retained. A partial or timed-out driver retains raw files with incomplete custody; it cannot qualify. Successful stores/catalogs remain evidence, with no automatic deletion or crash-durability claim.

Build with locked release artifacts and root ARMv8 flags. Reuse only identical compilation seals and verified immutable executables in the same worktree's archive. Hold the existing worktree run lock and never build during its timed child. A case gets a new output path; source/product/harness/dependency/binary/corpus/root/config identities, method environment and observed competing work are retained. A build or setup miss also gets an append-only round record. No previous successful family runs during family2 iteration; the Init regression checkpoint becomes due only after all required history correctness/storage gates pass.

```sh
CARGO_BUILD_JOBS=8 LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/benchmark/fs-bench-pro/runner.py run --case history-retention-stride-10-total-storage-v1 --out benchmark-results/fs-bench-pro/NEW_HISTORY_OUTPUT
python3 core/benchmark/fs-bench-pro/runner.py verify --run benchmark-results/fs-bench-pro/NEW_HISTORY_OUTPUT
python3 core/benchmark/fs-bench-pro/runner.py report --run benchmark-results/fs-bench-pro/NEW_HISTORY_OUTPUT
```

`verify`/`report` only consume retained evidence; they do not repeat product operations. A separate native verifier invocation already reopens the actual owners under its per-tier deadline. This profile freezes engineering evaluation, not release admission, an issue closure or a PR merge.
