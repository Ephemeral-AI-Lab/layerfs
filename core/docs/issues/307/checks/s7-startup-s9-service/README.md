# S7 startup / S9 typed service and native-import checkpoint

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> S7 and S9 remain CHECKPOINT/incomplete and unchecked.

Continues primary local main b9f3a8dc9ffcf78c9e625c86be8b4bb2f661ae6c, tree
2a55aa86c2d0ca88c5a6b2059d8f506092dc7ab5. Existing fuser correction/Docker
verification are accepted; no QEMU, kernel campaign or corrected-release wait
was resumed. Root reference and previous S5/S6 stopping record remain intact.

Actual product changes:

- Overlay create_observed and daemon start_observed retain finite startup SQL,
  allocation and exact original failure/success through readiness. Profile,
  schema/accounting DDL and readbacks now use actual statement observation;
  entire256MiB reservation and independent final allocation observation are paid.
- SQL receipts add supplied SQL bytes, all logical column delivery and approximate
  prepared-statement memory samples. Samples are not cumulative allocation/RSS.
- SDK runtime/service adds bounded typed authenticated Workspace/class rotation,
  same-Save ordering, result-held credits/receipt references, local disconnect
  cancellation and fresh service-epoch/rewrap fences over initialized owners.
  Original binding, authority, typed outcomes and one-attempt custody remain.
- Project/import captures opaque link targets without following them, using
  existing canonical0777 link mode and exact mtime, includes ignored/dependency/
  cache/output/.git paths and removes inherited4GiB refusal. Source-change checks
  do not create an atomic native snapshot. Backed collections/hard links remain.
- Private global-provider filesystem error cfg now matches its macOS allocation
  owners; a Linux-only device test dereferences Copy state. No public contract,
  dependency, schema, persistence profile or Linux global-provider capability changes.

[Operation architecture](../../../../architecture/36-operation-cost-observations.md),
[service architecture](../../../../architecture/38-authenticated-runtime-service.md),
[import architecture](../../../../architecture/39-native-import-links.md),
[S7 audit](../../S7-EXIT-AUDIT.md), [S9 audit](../../S9-EXIT-AUDIT.md) and
[current continuation](../../HANDOFF-S7-S9.md) separate implementation and open gates.

| Scope | Actual result |
| --- | --- |
| Host Overlay |48 pass, including3 new startup success/refusal/real-SQL-full bodies |
| Host Daemon |12 pass, including2 new readiness/failure receipt bodies |
| Host Workspace |32 pass with real owner/namespace/payload/EXPLAIN profiles |
| Host SDK |15 pass at frozen production source, including6 new service/history bodies |
| Host Project |9 integration/helper passes plus57 example/harness unit passes (66 total); includes2 new full-root bodies, with real-host body using both Store profiles |
| Docker Overlay/Daemon/Workspace |48/12/32 pass;2 owned-device cases stay explicitly ignored and unchanged S6 device evidence is reused |
| Docker Project |3 selected portable Init/root/scaling bodies pass; real global provider remains macOS-only |
| SDK Linux |Locked all-target no-run build pass; provider test bodies are macOS cfg, no fake Linux provider execution |
| Clippy |Host changed packages and6-package Docker all-target `-D warnings` pass after retained source diagnoses |
| Formatting/boundary/tools |fmt pass;579 production Rust/SQL files pass guard;39 unchanged tool self-tests pass |

One final covering functional check also pins selected binaries before and after
execution to resolve earlier incomplete execution-hash attribution. Earlier checks
remain diagnostic receipts without relabeling. Unchanged device evidence stays
reused. These functional tests are not performance proof-budget exceptions.

Every test invocation has an explicit120s ceiling; no hang occurred. No-run builds
precede execution; [scope notes](check-scope-notes.json) retain minor feature/test
rebuilds and functional-command interference. All original compiler/lint/test
failures and unrun rows stay in [failure ledger](FAILURES.md). Commands, source
fingerprints and actual walls are in individual *-receipt.json files. Functional
walls, statement-memory samples, credit gauges and uncontrolled caches have no
cold speed/RSS/sustained-rate eligibility.

Docker uses pinned ARM64 Rust1.85.1 image e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4;
Clippy uses its previously qualified official-toolchain image
378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6.
Actual Cargo targets are core/target/cluster2-runtime-tests on host and
core/target/cluster2-linux inside Docker, with core/target/cluster2-linux-cargo
Cargo home. Individual receipt `cargo_target` describes the outer host launcher;
Docker command environment records the actual container target. Root ARM64
config/locked dependencies and LAYERFS_CONSTRUCTION_WORKERS=1 apply; native Init
alone retains its previously supported4-constructor exemption. Four unrelated
containers remain running and unmodified; every owned check container is --rm.

Complete-operation page/journal/device I/O, phase whole-system residency and
sustained service/debt acceptance remain open. Logical authenticated transport/
client/restart custody, full context authority qualification, backed faithful
initial acquisition/hard-link identity and owning Sandbox/API-core assembly remain
S9 work. Greater-than4GiB native stream is NOT_RUN. P10 unknown-history resolution
is not guessed; P3/P6/P7/P13/P14 remain later Commit prerequisites. No milestone,
release, push, deployment, CI or root-reference retirement is claimed.

Exact first-parent/staged production LOC and source/tree/tracker receipts are
recorded in the checkpoint/receipt commits and continuation. Counter unchanged:
tools/production_loc.py SHA256
c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb.

## This implementation checkpoint's exact source-size comparison

First parent b9f3a8dc9ffcf78c9e625c86be8b4bb2f661ae6c to final staged product:
core86379 ->87552 (delta+1173), reference65417 ->65417 (delta+0),
combined151796 ->152969 (delta+1173). Counted from exact Git archives using
unchanged tools/production_loc.py SHA256
c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb.
New service/startup/source modules are first-party production; external tests,
receipts/docs/tools/examples/third-party remain excluded. Existing moved creation
code and all excluded predecessors stay in scope. No reference retirement,
algorithmic shrink or performance inference is claimed. Final staged tree and
committed-tree correspondence are recorded in the commit and following receipt.
