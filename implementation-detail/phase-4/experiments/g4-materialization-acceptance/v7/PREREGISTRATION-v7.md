# Prospective G4 materialization acceptance v7

Status: frozen before any v7 preparation or measured row.

V7 is the terminal durability repair. V6 is preserved byte-for-byte as measured PASS / terminal REVISE and is not rerun, edited, imported, or promoted. Its final auditors found that the ordinary M0 row passed while the measured publisher still cleared temp custody immediately after rename, returned on directory-sync failure without absent-prior reconciliation, verified through a pathname-following open, and blindly unlinked the final name. V7 repairs that shared source and runs one fresh complete campaign.

## Frozen custody

- Repository / branch / HEAD: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`, `codex/empty-worktree`, `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`.
- V7 candidate executable: `703782924014fa1d990f1b09b6dbb63f3e9230a10c9781e77a357068de1c3ee3`.
- G3 control: `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`.
- Protected-operation control: `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5`.
- Round-1 handoff: `8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00`.
- Rust sources: benchmark `eb00674125d18da66253b31949ecba2f874b64ec6a93ad68fe251d4f0649d169`; G3/G4 module `c70042602a8ab2e0d0c5b4ac0843003a0deae15b8c91ade28504d776fd526025`; unchanged Canonical-v2 `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc`; Cargo.lock `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8`.
- Frozen v1 runner / manifest: `9888aab78edb2bb0d8d4f38ea69062829afe493ec199a85ae22e9cdc3d018624` / `d6675db6cd340b12453f1719d55f183d3ef5f2b56cc07b9cac4c4f7b75b3c517`.
- Frozen v5 primary/independent scientific gate bases: `ab2ba1f7d62ca9b31437f87bdaf2c29b821a7b2ce40887fcf75eb93c421ae25c` / `4a2c7c9f242e51151f8a50962a375eb44d69fa47bc4667dcb94d1ae63155349d`.
- V6 terminal / verification / wall: `d35c56ea92b3c8631f7af0c5f29b54ab510fec783d3617c588e96c5ce32ab78c`, `2659909fa53903f0feb1adb0df68153c025b34242e215a623c9f747a52e14dc7`, `59f7241d5e50a22d9d5455dbff2c0f6fcb7d83485d9418d5b005db2382cbf359`.
- V7 result root: `target/phase4-g4-materialization-acceptance-20260822-v7/`, required absent before atomic fail-fast `target/BENCHMARK_LOCK` acquisition and never reusable.

## M0 old-or-new repair

The absent destination is the explicit prior state. V7 retains the created temp’s descriptor and device/inode identity through payload, data sync, mode, metadata sync, exclusive rename, parent-directory sync acknowledgement, reconciliation, descriptor verification, and identity-bound benchmark cleanup.

After rename or directory-sync ambiguity it classifies the descriptor-relative, no-follow final name as exactly:

- `requested-visible`: the final name is the owned regular inode and the retained descriptor hashes to the exact expected length/digest/mode;
- `prior-absent`: no final name exists;
- `different`: the final name is a symlink, wrong kind, different inode, wrong length/mode, or wrong bytes;
- `unresolved`: descriptor-relative classification itself fails.

Requested-visible is an old-or-new success with typed diagnostic provenance. Prior/different/unresolved returns a typed `G4NativePublicationFailure` retaining first, cleanup, reconciliation, reconciliation-error, and dominant causes. Cleanup checks name/device/inode before unlink and fsyncs the parent directory. It never follows or deletes a substitute. Output verification uses bounded `pread` on the retained descriptor and exact before/after identity, not `File::open(path)`. The already-computed expected ordered source-sequence digest is required by the batched candidate before rename.

Writer call/byte/short/error counters use checked arithmetic. M0 rows directly report data sync, metadata, metadata sync, rename, directory sync, reconciliation count/outcome, publication status/diagnostic, identity-bound temp create/remove, and actual scanned temp/final residue.

## Focused proof and runner repair

The frozen affected suite passes four G4 tests, including:

- closure-on/off successful proof/work parity, sink error, terminal Q, and identity-before-grammar malformed-error equivalence;
- exclusive target-appearance and inode-substitution cleanup;
- normal batched native publication;
- directory-sync lost acknowledgement resolving requested-visible, descriptor-verification failure cleaning the final inode, and post-publication substitution returning different without unlinking the substitute.

The v7 runner must write `CLEANUP.declared_deleted_root = work-v7`; emit the direct M0 durability counters above; label seed read `same-open-protected-seed-warm-or-unknown`; and retain the repository-global `BENCHMARK_LOCK` until the terminal-verification artifact itself is fsynced. Only then may it release the lock and fsync a separate `LOCK-RELEASE-v7.json` attestation. Final terminal verification therefore records that the lock was held through its fsync, not that it was absent beforehand.

## Unchanged schedule and gates

Exactly 30 append-only records / 50 arm observations / zero within-campaign reruns remain required. V7 preserves all v6 scientific gates: G3 per-row 5% inference is honestly `Unavailable` at n=1 while raw ratios, exact semantics/Q/residue, and 10/10/20-ms hard targets remain; full-create/edit/range/reopen adjacent 5% gates remain; wall buckets are 5/65/15/5/5/5/10 seconds with exact sum and <=120 seconds total.

R1 remains <=333 ms and >=5% same-binary closure-on/off improvement with exact root/output/occurrence/SQL/row/BLOB/authentication parity. Fresh remains <=400 ms. M0 remains <=400 ms with exact S1-100 170-query / 5,371-row / 83-batch / 5,284-chunk/write shape and the new direct durability gates. Seed no-digest remains <=50 ms with separate digest and explicit cache class. RSS <=20 MiB, buffers <=1 MiB, terminal Q/residue zero, operation-local sum <=20 seconds, two cold cells/physical I/O honest Unavailable, independent ledgers equal, cleanup/manifest/terminal verification exact.

Only affected focused/static gates run before this campaign. After measured PASS, one final workspace test closure and two fresh v7 read-only audits are required. G5 remains blocked, out of scope, and not started. No commit is authorized.
