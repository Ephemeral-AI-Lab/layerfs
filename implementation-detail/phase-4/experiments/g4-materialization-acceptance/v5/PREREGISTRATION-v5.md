# Prospective G4 materialization acceptance v5

Status: frozen before any v5 preparation or measured row.

V5 preserves the repaired candidate, exact 30-record / 50-arm chronology, every correctness/authority/durability/work/resource gate, all hard latency targets, append-only/no-rerun rule, independent analyzers, and <=120-second complete wall. V4 was rejected pre-execution after a final benchmark audit; it created no result root, invoked no child, and measured no row. V1–v4 history remains immutable.

## Frozen custody

- Repository / branch / HEAD: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`, `codex/empty-worktree`, `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`.
- Candidate executable: `69a9574efaaa6cb36467ba9008f4b87b2b7c7438c18dc8156426369cf7841d58`.
- G3 control: `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`.
- Protected-operation control: `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5`.
- Round-1 handoff: `8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00`.
- Rust sources: `phase4_create_edit_benchmark.rs` `fa2954b1d453ff2fd2d31c84488b60cb014dad4c86b652a8df0de8c398806077`; `phase4_g3_materialization.rs` `0a85af61213740e242721a2adfe2b9de4f692ab328516e130f1e34f773dc63cd`; unchanged `canonical_v2.rs` `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc`; `Cargo.lock` `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8`.
- Frozen v1 runner / methodology manifest: `9888aab78edb2bb0d8d4f38ea69062829afe493ec199a85ae22e9cdc3d018624` / `d6675db6cd340b12453f1719d55f183d3ef5f2b56cc07b9cac4c4f7b75b3c517`.
- V3 measured terminal / verification / complete wall: `71bc117a3917c32a824066bd101da958cb2f4232c3bb7af1b8bece86dc9ecfad`, `3803421f08858c066c433075943f9aed1904c424028a50ca3dfa88acd48fdae8`, `8434037889208627fa3756d0c07623b9acfcc7a35a52eb08490864ab407efc67`.
- V5 result root: `target/phase4-g4-materialization-acceptance-20260822-v5/`, required absent before the atomic fail-fast `target/BENCHMARK_LOCK` attempt and never reusable.

## G3 protected-route estimator correction

The original protocol simultaneously required exactly one prospective observation per G3 control/candidate arm, zero reruns, and a per-row 5% relative non-inferiority decision. V3 proved that estimator underidentified: operations span 6 microseconds to 358 milliseconds; one threshold miss was 333 nanoseconds; phase direction varied among payload, rename, fsync, cleanup, and unattributed time; and a 31.7% inner-timer miss occurred while the complete candidate child was 0.20 seconds faster. One observation cannot distinguish a 5% effect without the prohibited repeated paired observations, fixed order balancing, and preregistered estimator.

Prospectively for v5:

`g3_per_row_relative_noninferiority = Unavailable("one prospective observation per arm cannot resolve a 5% effect without prohibited reruns")`.

All 12 control/candidate operation times and ratios remain raw and normalized diagnostics. They cannot decide PASS/REVISE. This is not a larger percentage, absolute allowance, heterogeneous aggregate, historical subtraction, imported v3 result, or post-operation relabeling.

The applicable protected G3 gates remain hard:

- exact scenario/outcome/error parity;
- byte and mode exactness;
- old-or-new/publication parity;
- terminal Q zero and temp/seed residue zero;
- clone/no-op 100 MiB <=10 ms;
- one-byte 100 MiB <=10 ms;
- 1-MiB patch at 10 MiB <=20 ms;
- exact authority/permit/fallback/reconciliation/counter behavior.

An exact 5% G3 non-inferiority claim remains unavailable, not passed. The full-create/edit/range/reopen adjacent 5% gates remain applicable and unchanged because their accepted operation timers are sufficiently large and the v3 direct pairs were stable.

## Complete-wall allocation correction

V3 completed in 74.020 seconds but charged 60.814 seconds of whole-child G3 fixture/operation/verification work to a declared 50-second preparation bucket. V5 prospectively allocates and enforces:

- lock and measured preflight <=5 s;
- private base/shared preparation and whole G3 children <=65 s;
- row dispatch and measured operations <=15 s;
- exact row verification <=5 s;
- primary and independent analysis <=5 s;
- cleanup/storage/mode audit <=5 s;
- manifest/terminal/verification <=10 s;
- declared bucket ceilings total 110 s, leaving 10 s unallocated reserve;
- complete wall through fsynced terminal verification <=120 s;
- exact bucket sum equals complete wall.

The wrapper seals the bucket attestation and then independently rejects any observed bucket overrun. No interval is relabeled or split using nested or historical timer subtraction.

## Unchanged gates

- Same-binary R1 closure-on/off: candidate <=333 ms and at least 5% faster; exact root/output/occurrence/SQL/row/BLOB/authentication/Q parity; closure-off work exactly zero.
- Fresh reconstruction <=400 ms.
- Batched first/full native materialization <=400 ms with exact S1-100 170-query / 5,371-row / 83-batch / 5,284-chunk proof shape, 104,857,600 native bytes, no short/error writes, sync/metadata/exclusive rename/dirsync, exact verification, and zero residue.
- Seed no-digest full read <=50 ms, digest cost separate, identity stable.
- Protected returned range/full-create/same-count-edit/reopen adjacent pairs <=5% with exact semantic/work parity.
- RSS <=20,971,520 bytes; every buffer <=1 MiB; no full-file buffer; terminal Q zero; no workers/async/background work; SQLite and Canonical-v2 identities/profile unchanged.
- Two cold cells remain honest administrative `Unavailable`; true device/controller cold and physical I/O bytes remain unavailable.
- Exactly 30 append-only records, 50 arm observations, zero within-campaign reruns, operation-local timer sum <=20 s, primary/independent normalized ledgers equal, complete cleanup/manifest/terminal verification.

The retained G3 status adapter remains exact and limited to explicit outcome/error, byte/mode, Q, and residue invariants. Static closure and two fresh read-only final auditors remain required after measured PASS. G5 remains out of scope and no commit is authorized.
