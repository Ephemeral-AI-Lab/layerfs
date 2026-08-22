# Prospective G4 materialization acceptance v1

Status: frozen before every development performance screen and before every measured row.

This protocol evaluates G4 only. It does not authorize G5, persistent native caches, new identities, new storage formats, concurrency, VFS/SDK integration, or commits.

## Custody

- Repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`
- Branch: `codex/empty-worktree`
- Starting HEAD: `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`
- Candidate executable SHA-256: `a3573879d55f2fcfb031a334ce208102c7c0c78fa21a99339a8d5585187150c6`
- Frozen G3-v13 scalar/native control SHA-256: `535bfa17c01ac227024587d131b44d1decbdd07058e108455952fbe46fa4061e`
- Frozen protected-operation control SHA-256: `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5`
- Protected Round-1 handoff SHA-256: `8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00`
- Final result root: `target/phase4-g4-materialization-acceptance-20260822-v1/`; it must be absent before the atomic global-lock attempt and is never reusable.
- Global fail-fast lock: `target/BENCHMARK_LOCK`; no waiting is permitted.

The runner verifies the methodology manifest, source custody, executable hashes, handoff hash, branch, HEAD, result-root absence, and zero-row dry-run before preparation or measurement.

## Frozen meanings

- `R0-control` is the current complete authenticated batched reconstruction with content closure enabled. It is a correctness/work control and has no optimized target.
- `R1-attribution-control` is the same G4 executable and same accepted traversal with content closure enabled.
- `R1-candidate` is the same G4 executable and traversal with only the content-closure fold disabled. Raw output digest, ordered occurrence commitment, identity-first authentication, role/length checks, topology/order/partition/extent/cycle/limit validation, SQL work, reconstructed bytes, and exact errors remain enabled.
- `M0-control` is a separately frozen G4 orchestration mode calling the byte-identical G3 scalar `stream_root` algorithm against an absent destination. It is labeled `g3-fallback-algorithm-control`, is diagnostic, loses batching, omits content-closure and occurrence proof products, and can never be promoted.
- `M0-candidate` is the accepted batched authenticated traversal with a direct bounded native-file sink and absent-target exclusive publisher.
- `S1-candidate` is the consumed same-open protected seed full read plus the current clone/patch/fallback publisher. S1 G3 routes are adjacent to the sealed G3-v13 executable.

Content closure may be `derived-not-computed` only in the versioned R1/M0 candidate modes. The proof is: the authenticated expected namespace root fixes the namespace object; authenticated identity-first reads fix every referenced canonical mapping/chunk object; validated logical roles, exact lengths, ordering, radix partition, cumulative extents, cycle/limit bounds, and complete leaf traversal fix the graph and emitted byte stream; the retained ordered occurrence commitment fixes reference multiplicity/order; the retained raw digest fixes emitted bytes. Historical closure fields, goldens, capture/commit paths, and legacy callers remain closure-on.

## Frozen implementation boundary

The only reconstruction-algorithm change is an optional closure policy and authenticated chunk sink in the existing accepted `reconstruct_file` / `verify_file_inner` / `stream_file` path. A sink receives a chunk only after object-ID authentication, bytes-object grammar decoding, and exact referenced raw-length validation. Buffers are at most 1 MiB, authenticated chunks are at most 32 KiB, no complete application file buffer is permitted, and execution is synchronous with no workers, async, pipeline, or background maintenance.

First/full publication creates a private no-follow temp, binds cleanup to its device/inode, writes authenticated bytes, validates exact root/head/output/length/reference count, fsyncs data, sets mode 0644, fsyncs metadata, rechecks descriptor/name identity and type, performs `RENAME_EXCL | RENAME_NOFOLLOW_ANY`, and fsyncs the directory. A final name appearing after preflight is a conflict. Named cleanup never unlinks a substituted inode. Post-operation exact verification and cleanup are separately timed.

The SQLite profile is unchanged: schema 5, Canonical-v2 identities, K64/F64 fixed radix, 8/16/32-KiB FastCDC, `synchronous=FULL`, rollback journal `DELETE`, `temp_store=FILE`, `mmap_size=0`, and `cache_spill=2000`.

## Exact schedule

`schedule_g4_v1.py --dry-run` must report exactly 30 records, sequences 1–30, 50 durable arm observations, nine R0/R1 observations, two administrative cold records, four seed timed passes, zero actual rows, zero child invocations, zero database copies, and zero reruns. The exact chronology is:

1. R01 warm 1 MiB: R0, closure-on attribution, closure-off candidate.
2. R01 warm 10 MiB: same three arms.
3. R01 warm 100 MiB primary: same three arms; direct R1 decision.
4. R1 fresh-process 100 MiB.
5. R1 controlled-cold 100 MiB or exact administrative `Unavailable`.
6. S1 seed read 10 MiB, no-digest and digest timers separated.
7. S1 seed read 100 MiB primary, no-digest and digest timers separated.
8. Protected returned 1-MiB range, adjacent frozen control/current candidate.
9–11. M0 scalar diagnostic control at 1/10/100 MiB.
12–14. M0 batched candidate at 1/10/100 MiB.
15. M0 controlled-cold 100 MiB or exact administrative `Unavailable`.
16–27. Adjacent sealed G3/current S1 pairs for clone/no-op 10/100, one-byte 100, 1-MiB patch 10, count-change 1/100, invalid authority 1/100, external mutation 1, symlink 1, before-publication fault 1, and lost acknowledgement 1 MiB.
28–30. Adjacent frozen/current 100-MiB full-create, same-count-edit, and reopen/head guards.

Every arm stdout/stderr is fsynced before its append-only arm record. Every completed envelope is fsynced before the next record. No measured row may be deleted, reordered, edited, replaced, or rerun.

## Gates

- Warm R1 candidate: `<=333,000,000 ns` and `candidate_ns * 100 <= attribution_control_ns * 95`.
- Fresh R1 candidate: `<=400,000,000 ns`.
- M0 100-MiB candidate: `<=400,000,000 ns`.
- Seed 100-MiB no-digest full read: `<=50,000,000 ns`; digest pass is separate and not relabeled as byte delivery.
- Clone/no-op 100 MiB: `<=10,000,000 ns`; one-byte 100 MiB: `<=10,000,000 ns`; 1-MiB patch at 10 MiB: `<=20,000,000 ns`.
- Every adjacent protected arm: `candidate_ns * 100 <= control_ns * 105`, plus applicable root/transition/output/old-or-new/error parity.
- Accepted S1-100 operation shape: 170 SQL queries, 5,371 rows, 87 singleton mapping queries/rows, 83 leaf-batch queries, 5,284 chunk rows/references, maximum batch 64, 5,284 borrowed chunk BLOB reads / 104,926,292 bytes, 5,371 authenticated objects / 105,122,401 canonical bytes, one output digest / 104,857,600 raw bytes, and 5,284 occurrence entries / 190,224 bytes.
- Closure-off must have zero closure updates/bytes. All other R1 proof/work/output fields must equal closure-on.
- Whole-child maximum RSS `<=20,971,520` bytes; every buffer `<=1,048,576` bytes; Q terminal exactly zero; no residues.
- Sum of operation-local measured timers `<=20,000,000,000 ns`.
- Primary and independently implemented recomputation ledgers must be byte-equal after normalized JSON serialization.
- Exactly two cold records may be `Unavailable`. Here they must be unavailable unless exclusive-host custody and a successful privileged `/usr/sbin/purge` are both established before immediate launch. Fresh process, reopen, `F_NOCACHE`, and slower wall are not cold evidence. True device/controller cold and stable-media physical bytes remain `Unavailable`.

## Complete wall

`T_complete = t_terminal_verification_fsynced - t_global_lock_attempt_started <= 120,000,000,000 ns`.

Every adjacent interval is assigned to exactly one bucket: lock/preflight 5 s; private/shared preparation 50 s; row dispatch/measured work 20 s; exact row verification 10 s; primary plus independent analysis 10 s; cleanup/storage/mode audit 5 s; manifest/terminal/verification 10 s. The bucket sum must exactly equal `T_complete`; reserve is `120 s - T_complete`. A post-measurement complete-wall attestation binds the fsynced terminal-verification hash without pretending its own write preceded the endpoint.

The measured payload manifest excludes only the conventional manifest/terminal/verification/complete-wall/final-hash cycles. The final artifact hash list binds all already-created artifacts. Result files are sealed 0444 and result directories 0555 after verification.

Static workspace validation and fresh final read-only auditors happen once after the measured terminal and outside its clock. G4 cannot receive final PASS until those closures pass.
