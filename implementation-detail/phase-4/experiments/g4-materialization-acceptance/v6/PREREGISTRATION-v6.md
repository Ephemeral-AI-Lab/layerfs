# Prospective G4 materialization acceptance v6

Status: frozen before any v6 preparation or measured row.

V6 is the exact-source/executable static-closure repair. V5 is preserved as a sealed measured PASS (30 records, 50 arms, 73,577,864,625 ns) whose final static closure required one source-only lint annotation. The final release hash changed, so v5 is not promoted by equivalence argument. V6 reruns the complete campaign against the lint-clean executable; it imports no v5 row and performs no selective rerun.

The only source difference from the v5 measured executable is `#[allow(clippy::type_complexity)]` on the private `g4_seed_read` function returning its five already-measured fields. It changes no branch, buffer, syscall, timer boundary, authority rule, proof product, output, or data structure. The lint-clean candidate executable is frozen as:

`3713f83aca0147f8e4f350f6464240a78f7ae3a14b0b5ef4edcf66007738d14d`

Frozen custody:

- Repository / branch / HEAD: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`, `codex/empty-worktree`, `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`.
- Candidate executable: `3713f83aca0147f8e4f350f6464240a78f7ae3a14b0b5ef4edcf66007738d14d`.
- G3 control: `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`.
- Protected control: `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5`.
- Round-1 handoff: `8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00`.
- Rust sources: benchmark `fa2954b1d453ff2fd2d31c84488b60cb014dad4c86b652a8df0de8c398806077`; G3/G4 module `55a61813fccd442258a91c6ab45c4efb4949f6afdcdc7bd8deadab10c0a14b8b`; unchanged Canonical-v2 `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc`; Cargo.lock `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8`.
- Frozen v1 runner / manifest: `9888aab78edb2bb0d8d4f38ea69062829afe493ec199a85ae22e9cdc3d018624` / `d6675db6cd340b12453f1719d55f183d3ef5f2b56cc07b9cac4c4f7b75b3c517`.
- Frozen v5 primary/independent analyzer sources: `ab2ba1f7d62ca9b31437f87bdaf2c29b821a7b2ce40887fcf75eb93c421ae25c` / `4a2c7c9f242e51151f8a50962a375eb44d69fa47bc4667dcb94d1ae63155349d`.
- V5 measured terminal / verification / wall: `7491b2e133171ca1529fb01502ae8d4620acb01513c62eda644a2a4eb08bcd3a`, `4b1ca74dd3d4a79c4a7e8da87873e365cdd69f73becf79d6c95adf8ff85905b5`, `f2341b1cee3bc47c064fc78e16d1450019867b1a26e29095d70d80284febc878`.
- V6 result root: `target/phase4-g4-materialization-acceptance-20260822-v6/`, required absent before atomic fail-fast `target/BENCHMARK_LOCK` acquisition and never reusable.

Every v5 protocol correction remains binding: exactly 30 append-only records and 50 arms; per-route G3 relative 5% inference explicitly `Unavailable` at one observation without prohibited reruns; all raw ratios retained; hard exact G3 semantics/10/10/20-ms/Q/residue gates; full-create/edit/range/reopen 5% adjacent gates; wall buckets 5/65/15/5/5/5/10 seconds, exact sum, 10-second reserve, and <=120 seconds complete wall.

All G4 gates are unchanged: R1 <=333 ms and >=5% same-binary closure-on/off improvement with exact proof/work parity; fresh <=400 ms; M0 <=400 ms and exact 170-query / 5,371-row / 83-batch / 5,284-chunk S1-100 shape; seed no-digest <=50 ms with separate digest; native output/sync/metadata/exclusive rename/dirsync/exact verification/cleanup; RSS <=20 MiB; buffers <=1 MiB; terminal Q/residue zero; honest cold/physical-I/O Unavailable; operation-local sum <=20 seconds; primary/independent ledger equality; complete manifest and terminal verification.

The retained G3 status adapter remains restricted to explicit outcome/error, byte/mode, Q, and residue fields. After measured PASS, the lint-clean source must run the full final workspace tests, clippy `-D warnings`, fmt, diff, custody/inventory, and two fresh read-only final audits. G5 remains out of scope and no commit is authorized.
