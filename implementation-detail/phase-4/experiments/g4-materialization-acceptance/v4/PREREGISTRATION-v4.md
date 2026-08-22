# Prospective G4 materialization acceptance v4

Status: frozen before any v4 preparation or measured row.

V4 is a complete fresh campaign. It does not reuse, replace, or selectively rerun a v3 row. The exact v1 30-record / 50-arm chronology, analyzers, thresholds—including every per-route 5% adjacent gate—proof, cache labels, resource gates, and <=120-second wrapper remain unchanged.

V3 completed and sealed all 30 records and 50 arms in 74,019,901,416 ns. It passed R1, M0, seed-read, SQL/row/BLOB, RSS/Q, all four full-create/edit/range/reopen guards, hard G3 route targets, cleanup, independent-ledger, and wrapper-wall gates. It remained REVISE because six one-shot G3 candidate/control pairs exceeded 5%. The raw phase deltas were inconsistent across identical mechanisms and included a 333-ns threshold miss on symlink preflight, which never touches the changed native-temp code. V3 is preserved by the hashes in `PRE-EXEC-HISTORY-v3.json`.

The durability repair removed the only deterministic candidate overhead identified by source and component evidence: `clone_temp` formerly performed one `stat_at` to bind cleanup ownership and a second identical `stat_at` before descriptor reopen. V4 performs exactly one name `stat_at`, stores that `NativeIdentity`, and reuses it to check the no-follow reopened descriptor. This preserves regular-file/device/inode continuity and makes an injected reopen failure cleanup-safe. The required `create_temp` descriptor `fstat` and cleanup-time owned-inode `stat_at` remain; removing either would permit replacement unlink or unowned cleanup.

Focused G4 tests passed after the repair. The repaired frozen candidate executable is:

`69a9574efaaa6cb36467ba9008f4b87b2b7c7438c18dc8156426369cf7841d58`

Frozen custody:

- Repository / branch / HEAD: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`, `codex/empty-worktree`, `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`.
- Repaired candidate executable: `69a9574efaaa6cb36467ba9008f4b87b2b7c7438c18dc8156426369cf7841d58`.
- G3 control: `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`.
- Protected control: `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5`.
- Round-1 handoff: `8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00`.
- Rust sources: `phase4_create_edit_benchmark.rs` `fa2954b1d453ff2fd2d31c84488b60cb014dad4c86b652a8df0de8c398806077`; `phase4_g3_materialization.rs` `0a85af61213740e242721a2adfe2b9de4f692ab328516e130f1e34f773dc63cd`; unchanged `canonical_v2.rs` `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc`; `Cargo.lock` `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8`.
- Frozen v1 runner / methodology manifest: `9888aab78edb2bb0d8d4f38ea69062829afe493ec199a85ae22e9cdc3d018624` / `d6675db6cd340b12453f1719d55f183d3ef5f2b56cc07b9cac4c4f7b75b3c517`.
- V4 result root: `target/phase4-g4-materialization-acceptance-20260822-v4/`, required absent before the atomic fail-fast `target/BENCHMARK_LOCK` attempt and never reusable.

The v3 G3 schema adapter remains exactly as preregistered: it adds PASS only from exact retained outcome/error, byte, mode, terminal-Q, and residue fields; it never relabels time, cache, work, authority, publication, CPU/RSS, storage, or physical I/O.

All exact gates remain binding: R1 <=333 ms and >=5% direct same-binary improvement; fresh <=400 ms; M0 <=400 ms; seed no-digest <=50 ms; G3 10/10/20-ms hard targets and every adjacent pair <=5%; accepted S1-100 170-query / 5,371-row / 83-batch / 5,284-chunk shape; protected full-create/edit/range/reopen <=5%; RSS <=20 MiB; buffers <=1 MiB; terminal Q and residue zero; two honest controlled-cold `Unavailable` cells; operation-local sum <=20 s; exact independent ledger equality; exactly 30 records / 50 arms / zero within-campaign reruns; and complete wrapper <=120 s through fsynced terminal verification.

No averaging, retries, selective replacement, new cache, dependency, public API, VFS/SDK, G5 work, or commit is authorized. Static closure and two fresh final auditors remain required after measured PASS.
