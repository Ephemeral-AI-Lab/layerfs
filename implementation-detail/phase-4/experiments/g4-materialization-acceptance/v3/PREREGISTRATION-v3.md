# Prospective G4 materialization acceptance v3

Status: frozen before any v3 preparation or measured row.

V3 preserves the complete v1 algorithm, candidate executable, 30-record / 50-arm schedule, gates, analyzers, proof, cache policy, and 120-second complete-wall contract. It preserves the v1 zero-row pre-exec hash rejection and the v2 15-record / 20-arm append-only REVISE package. No v2 row was deleted, edited, reordered, replaced, or rerun inside v2; its work root was cleaned and its failure terminal, verification, and payload manifest were sealed.

The sole v3 protocol change is an explicit schema adapter for sealed/current `phase4-g3-row-v1` payloads. That retained schema intentionally has no top-level `status`. The adapter may add `status=PASS` only when all frozen row invariants already present in the payload are exact:

- `outcome=success` and `error=null`, except `symlink-substitution` must be `typed-error/NativeDestinationSymlink` and `before-publication-fault` must be `typed-error/InjectedBeforePublication`;
- `byte_exact=true` and `mode_exact=true`;
- `q_terminal=0`;
- `temp_residue_count=0` and `seed_residue_count=0`.

It records `status_adapter=qualified-from-retained-g3-v1-exact-outcome-byte-mode-q-residue-invariants`. It does not change, infer, subtract, or relabel any time, work, cache, authority, publication, output, error, CPU, RSS, Q, storage, or physical-I/O field. Payloads failing any invariant remain non-PASS and abort append-only.

Frozen custody:

- Repository / branch / HEAD: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`, `codex/empty-worktree`, `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`.
- Candidate executable: `a3573879d55f2fcfb031a334ce208102c7c0c78fa21a99339a8d5585187150c6`.
- G3 control: `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`.
- Protected-operation control: `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5`.
- Round-1 handoff: `8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00`.
- Frozen v1 runner: `9888aab78edb2bb0d8d4f38ea69062829afe493ec199a85ae22e9cdc3d018624`.
- Frozen v1 methodology manifest: `d6675db6cd340b12453f1719d55f183d3ef5f2b56cc07b9cac4c4f7b75b3c517`.
- V2 REVISE terminal / verification / failure manifest: `9bfb7bc891cf6402180258cbcb49e54f347cd3d9d8bf180a1fde47a99b41860f`, `16c061d7baf0e644f009864115829a8977247b9090025202f939bb28200a2823`, `65bf301a5de77e1e4f0ddb33d73bc7a4f9ee697223c1ab4a4dbcb3caad58b819`.
- V3 result root: `target/phase4-g4-materialization-acceptance-20260822-v3/`, required absent before the atomic fail-fast `target/BENCHMARK_LOCK` attempt and never reusable.

All normative v1/v2 meanings and gates remain unchanged: R0 closure-on correctness control; same-binary closure-on/off R1 attribution with >=5% direct improvement and <=333 ms; fresh <=400 ms; honest scalar M0 diagnostic; batched durable M0 <=400 ms; seed no-digest full read <=50 ms; G3 10/10/20-ms targets; <=5% protected adjacent degradation; exact S1-100 170-query / 5,371-row / 83-batch / 5,284-chunk shape; identity/output/occurrence/error parity; RSS <=20 MiB; buffers <=1 MiB; no full-file buffer; terminal Q/residue zero; two honest cold `Unavailable` cells; operation-local sum <=20 s; independent ledger equality; exactly 30 records / 50 arms / zero reruns; and complete wrapper <=120 s through fsynced terminal verification.

Static closure and two fresh read-only final auditors are still required after measured PASS. G5 remains out of scope and no commit is authorized.
