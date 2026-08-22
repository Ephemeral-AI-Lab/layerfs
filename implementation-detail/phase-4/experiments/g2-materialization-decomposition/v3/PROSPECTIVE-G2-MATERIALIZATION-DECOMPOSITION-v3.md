# Prospective G2-v3 protocol closure

Status: `FROZEN BEFORE DRY-RUN OR MEASURED ROW`

Date: 2026-08-22

Repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`

Starting HEAD: `d79f0e0e2582d1bc491410224fec2b6cef7482e9`

## Scope and historical boundary

G2-v1 remains immutable historical `G2 REVISE`. Its product behavior did not
fail: its final analyzer applied `allocated_store_delta_bytes == 0` to every
operation, including the intentionally mutating `same-middle` edit. The
independent v1 recomputation was also too weak because it did not validate
storage endpoints. Neither v1 analyzer is amended or relabeled.

G2-v3 changes one thing only: storage predicates are scoped by operation. It
composes the sealed v1 decomposition and read-only guard rows with exactly one
fresh `BA` same-middle guard. It changes no Rust source, binary, profile,
schema, PRAGMA, transaction, durability, identity, materialization behavior,
or performance mechanism. It makes no edit-speed claim.

## Frozen evidence composition

The v3 analyzers consume, without copying or modifying, these sealed v1 facts:

| Artifact | SHA-256 |
|---|---|
| Raw v1 JSONL, 18 rows | `6f7124cc8d4fdd248b89770da5576f2546f105304e3d486ddb2f9c7ce5352af2` |
| Primary v1 analysis | `0840dcf353eff15a53eaa07f748678bfcab5b02b732ec9c592c12d0f38127282` |
| Observer probes | `bfe2e85b7a1fd61d84699cab4f1f3727731e955965a1370e0cfad8d8a406e717` |
| V1 terminal | `b859de6dce9aef9caba43dbf43fd5eb2b7ea24630f7f18ff206749d431e6f2a1` |
| V1 payload manifest, 178 entries | `28c1b86a3fd3715785617da84195e5ed2cbd5a880dcc883f57f8e51d5edd2d13` |

The v1 primary decomposition must independently retain 10 primary rows, 8
measured primary rows, four complete A/B pairs within the 1.05 observer bound,
four instrumented rows with exact timer equations and one timer-region count,
five observer probes within the frozen gate, and no eligible removable family.
The v1 primary analysis must remain the sealed PASS/INSUFFICIENT_EVIDENCE
artifact. V3 does not reinterpret the two historical edit rows as new rows.

All 178 sealed manifest entries are rehashed and size-checked. All 18 v1 rows
are recomputed: the 16 read-only rows (10 primary plus materialize-fresh,
range, and reopen A/B guards) use the corrected read-only predicate, and the
two historical edit rows use the prospective mutation equations as
corroboration only. Their
semantic identities, zero transactions/COMMITs, zero terminal Q, absent
residue, database/authority/expectation hashes, logical/apparent/allocated
endpoints, A/B parity, and existing wall allowances must remain exact.

## Operation-scoped storage predicates

For every read-only row:

```text
allocated_store_delta_bytes == 0
pre logical/apparent/allocated database endpoint == post endpoint
pre logical/apparent/allocated store endpoint == post endpoint
pre database/authority/expectations hash == post hash
transactions == commits == 0
sqlite_page_size_bytes == "Unavailable" (honest v1 read classification)
```

For the fresh mutating same-middle pair, each arm must match these prospective
expectations and the two arms must be equal:

```text
pre logical/apparent database                 109,199,360 B
pre logical/apparent store                    109,199,392 B
pre allocated database                        109,199,360 B
pre allocated store                           109,203,456 B
post logical/apparent database                109,314,048 B
post logical/apparent store                   109,314,080 B
post allocated database                       125,976,576 B
post allocated store                          125,980,672 B
logical/apparent database and store delta         114,688 B
allocated database and store delta             16,777,216 B
transactions / COMMITs                               1 / 1
```

The pair must also have identical root, transition, closure, occurrence
commitment, reference count, object/work counters, final database hash,
authority hash, expectations hash, modes, terminal Q, and residue. The frozen
semantic endpoints are:

```text
root        8df9bc09f9ba99351f11f3cb01b039713090120873b6dea8903e7d835a2a9faf
transition  b185f7670f748b5713d4d8538c513bce4b3019e17991840c369575f404fbf2ed
closure     d7614133f35f1a254d0d2222815cdbcbdcd69915baf30c3a801831e6497b1683
occurrence  2f7cd2e85591ad9dbca8005402c4b209624bb5a058c7d7358620b5d2f2575bec
database    b69861ee81c4a01906cf2fb70fe4ef49c4de534cab9ab9b000006efe6802fe31
```

The removed/inserted hex fields must each decode to 18,854 bytes and hash to
`fdc04dd5bea39e9480dd5559068fd72e0e9c2c3ce5b92fa8c62798ee3425a8fa`
and `8a4df9c28dcf4e0625ba08fa5f92c4ea2e462274c081072823900ab0e75611d6`
respectively. The edit offset is 52,480,416; references remain 5,284; dirty
pages/pager bytes/spills are 45/184,320/0; dispatch/return/success/error are
1/1/1/0; publication is `Committed`; Q high-water/terminal is 2,222,803/0;
and runtime is 4-KiB pages, FULL+DELETE, FILE temp storage, and mmap zero.

Fresh walls are retained descriptively but have no acceptance ratio and make
no timing claim. The already sealed v1 AB edit pair retains its original 1.05
guard; the complementary single BA closure does not add an order-sensitive
performance decision.

## Frozen inputs and schedule

| Input | SHA-256 / size |
|---|---|
| G1 control executable | `42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55` / 1,372,784 B |
| G2 instrumented executable | `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5` / 1,390,512 B |
| Restored G1 benchmark source | `157699e0cd4cb1e3b5ec631cefb7c967ff7433bdeeb10ee1336e70961b402ad2` |
| Sealed 100-MiB source | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` / 104,857,600 B |
| Sealed base database | `7db8d50de42b994546789cb67fc7a9b650e2e551dab118e15003e02106b19890` / 109,199,360 B |
| Sealed base authority | `7855ea6096359925f639b91c8d6b9708cfe0bc0df4a3ffd97a280a8e9a9ded48` / 32 B |
| Sealed base expectations | `a7489b01445e53aa8a0c5824059b8a6b04f92e15a3b6cf953fbb4c83d6b5e18a` / 1,096 B |

Fresh result namespace:

`target/phase4-g2-materialization-decomposition-20260822-v3`

The schedule is exactly:

```text
01 prepare B with the snapshotted instrumented executable
02 prepare A with the snapshotted instrumented executable
03 measured same-middle B, instrumented executable with decomposition disabled
04 measured same-middle A, exact G1 control executable
05 primary analyzer
06 independent analyzer
```

Before any preparation or row, both `/tmp` executables are byte-copied into
`results-v3/operands-v3`, rehashed, and used only from those snapshots. The v1
recorded instrumented source and source-only diff bytes were reverted and are
not retained, so v3 explicitly does not claim they are byte-reverifiable.
Operand custody records source/copy paths, hashes, sizes, modes, device/inode
pairs, and proves the copies are distinct files. The FastCDC source remains
the retained `bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6`
file. The sealed v1 root must remain read-only and its lock absent.

The sealed fixture and base database/authority are referenced read-only; v3
does not copy either into an input namespace. `--fast-prepare` creates exactly
two distinct row database/authority/expectation copies under the single
declared `results-v3/rows-v3/work-v3` path. Apparent and allocated regular-file
usage is sampled after both preparations and both measured rows; each peak
must be at most 300 MiB. Preparation and hashing are outside row timing.
Acquisition refuses an existing result root or
lock, has a 15-second child ceiling and a 59-second global ceiling through
terminal verification, never replaces a row, and seals only after verification.
Each child timeout is the smaller of 15 seconds and the remaining global
budget. The dry-run declares all six child invocations. Chronology requires
the exact start/complete interleaving, command, and zero exit status for both
prepare children, both row children, and both analyzer children.
Execute mode additionally
requires the exact parent-authorization environment value documented by the
runner and the frozen dry-run hash.

Every row must report the benchmark-accepted provenance literal
`physical-byte-copy-identical-database-authority-expectations`.

## Analysis and terminal decision

The primary and independent analyzers implement the predicate separately and
must agree exactly on status, disposition, and a normalized evidence ledger.
Each checks all 178 v1 manifest entries, decomposition, every sealed row and
guard pair, full fresh edit custody/parity, retained executable bytes and
inode custody, exact chronology prefix/plan, and completed cleanup. The common
normalized ledger contains named gate booleans/failure IDs; all v1 hashes,
probes, pair/position/center values, timers, families, eligibility and work;
all fresh semantic/edit/storage/durability/Q/RSS/runtime values; operand
custody; chronology; cleanup; and every resource ceiling. The runner compares
status, disposition, sorted failures, and the complete ledger. Synthetic self-checks prove that a read-only allocation is
rejected, a valid mutation allocation is accepted, and a wrong mutation
endpoint is rejected.

Terminal dispositions are:

- `G2 PASS / INSUFFICIENT_EVIDENCE FOR A CONSTANT-FACTOR CANDIDATE` only when
  every composed gate passes and both analyzers agree.
- `G2 REVISE` for any custody, row-shape, semantic, storage, resource, wall,
  analyzer-agreement, or time-ceiling failure.

A complete payload manifest and terminal verification are written on success
or failure, including status/disposition, entry count/hash, zero mismatches,
and a chronology with exactly one start/completion for B followed by A.
The terminal directly binds the fresh raw rows, both analyses, agreed
normalized ledger, chronology, transient cleanup report, and sealed v1
terminal/verification. Any `REVISE` exits nonzero, and lock cleanup is
unconditional even when cleanup, manifesting, verification, or sealing fails.
Methodology files are copied into results before acquisition. Verification
precedes sealing and preserves any failure reason.

The v1 preflight and analyzers prove complete expected file-node closure,
internal symlinks only, a nonwritable entire v1 subtree, and an absent v1 lock.
The v3 lock is removed and its absence proved before any authoritative PASS
terminal. Payload files are sealed first; the terminal then binds every direct
hash, final-mode policy, actual global elapsed/59-second ceiling, and lock
absence. Post-seal verification requires no retained symlinks, files 0444,
directories 0555, exact payload hashes, and time compliance. Any failure
unseals only v3, regenerates authoritative REVISE evidence, reseals it, and
exits nonzero; no failure path can leave authoritative PASS.

Only the two private row copies are transient. Cleanup requires the exact work
path to exist, verifies both rows and the <=300-MiB peak, deletes exactly
`results-v3/rows-v3/work-v3`, and proves its absence before either analyzer.
Both analyzers gate cleanup PASS and its exact deletion set. The executable snapshots remain. This keeps
retained v3 evidence below 10 MiB without deleting any v1 evidence.

A PASS closes only G2's diagnostic protocol. It selects no constant-factor
micro-optimization. The following separately preregistered direction is
destination-authority-gated incremental materialization. G3, product edits,
builds, full reruns, page-size/spill/pipeline experiments, 500-MiB work, WP5,
commits, staging, pushes, and history operations are outside this protocol.
Even after G2-v3 PASS, G3 is ineligible until full workspace tests, Clippy
with `-D warnings`, formatting, and diff-check static closure all pass.

## Authorized commands before execute mode

After `METHODOLOGY-MANIFEST-v3.tsv` freezes this document and both analyzers:

```text
python3 analyze_g2_v3.py --self-test
python3 recompute_g2_v3.py --self-test
python3 -m py_compile analyze_g2_v3.py recompute_g2_v3.py run_g2_v3.py
G2_V3_METHODOLOGY_SHA256=<frozen-manifest-sha256> python3 run_g2_v3.py --dry-run
git diff --check -- implementation-detail/phase-4/experiments/g2-materialization-decomposition/v3
```

These checks create zero measured rows, zero database copies, and zero
benchmark child invocations. Execute mode remains forbidden until the parent
explicitly authorizes it.
