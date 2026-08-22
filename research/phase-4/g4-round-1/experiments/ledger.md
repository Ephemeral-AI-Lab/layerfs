# G4 Round-1 disposable-experiment ledger

Status: **closed / append-only research ledger**
Scope: Round-1 side investigations only; not G4 acceptance
Repository checkpoint: `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`

Exactly one disposable experiment was executed. It was a static OS capability
falsifier, not a performance, cold-cache, physical-I/O, durability, or
candidate-acceptance row. The reconstruction and core-architecture lanes ran
zero experiments. The lead ran zero experiments and no benchmark lock was
acquired.

## Experiment controls freeze and methodology deviation

The exact repository-global fail-fast lock path for every later Round-1/G4
build or timing-sensitive probe is frozen as:

```text
/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/target/BENCHMARK_LOCK
```

It is a single repository-global lock, not a campaign-local versioned lock.
Acquisition must be atomic and fail-fast (`create_new`/`O_CREAT|O_EXCL` or an
equivalent atomic primitive), record owner PID/start/purpose/source hash, and
never wait. Release occurs only after row output, analysis, cleanup, and source
restoration. Static reading/web research may proceed without it. At this freeze
the path was absent and no later Round-1 probe was started.

Starting custody was branch `codex/empty-worktree`, HEAD
`5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`, empty tracked diff, and exactly
one pre-existing untracked directory:
`implementation-detail/phase-4/experiments/g4-materialization-acceptance/`,
containing the preserved handoff SHA-256
`8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00`.

**Recorded deviation:** E-M1 was a non-performance static capability probe, but
its small C compile/probe occurred before this shared ledger and exact global
lock path were instantiated. It did not acquire a historical campaign lock.
It is therefore historical/inadmissible capability scratch: its exact hashes
and embedded direct results may inform API availability, but it is not
admissible as performance, cold, CPU/RSS, or G4 candidate evidence. Only the
conservative cleanup-inclusive `<26 s` bound proves compliance with the
120-second ceiling. No further build or timing probe occurred. This deviation
must not be copied into G4 preregistration.

## E-M1 — APFS descriptor clone, CoW isolation, sparse, preallocation, and cache-policy capability

| Field | Immutable record |
|---|---|
| Owner | materialization specialist |
| Classification | Historical/inadmissible syscall/API capability scratch; compiled before the frozen global `BENCHMARK_LOCK`; no timing conclusion |
| Hypothesis / one variable | On the exact APFS `/tmp` volume, `fclonefileat` accepts an already-authenticated-style unlinked read-only seed descriptor and cloned writes stay private; `F_NOCACHE`, sparse `ftruncate`, and `F_PREALLOCATE` are callable. The variable is native primitive success/direct stat state, not latency. |
| Namespace | `/tmp/layerfs-g4-r1-materialization-capability.t3AFdJ`, device `16777232`, APFS; unique and absent before; absent after cleanup |
| Repository/source custody | Branch `codex/empty-worktree`, HEAD `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`; no tracked repository source copied or edited |
| Probe source | 3,528-byte C source created via `apply_patch` in the disposable namespace; SHA-256 `ad178b73d4fda7a50fc027a36663d3fbb7a68d78d8a468fb768f0523adf288d9`; removed; source bytes are no longer retained or reconstructable from this package |
| Probe binary | `/usr/bin/clang -Wall -Wextra -Werror`; 34,456 bytes, mode 0500; SHA-256 `0210b3fb9f1d82da4b234ea6a931b9e8ec5f0e3f436ccef3e82d58eecc05bb79`; removed |
| Input | Deterministic 4-MiB repeated 64-KiB pattern; seed `fsync`, `F_NOCACHE=1`, unlink while descriptor lives; same-volume clone and one-byte patch; separate 4-MiB sparse and preallocated files |
| Start | UTC `2026-08-22T06:22:32Z`; monotonic `1146805540177958 ns` |
| Measured-command end | UTC `2026-08-22T06:22:34Z`; monotonic `1146807468332166 ns` |
| Compile/probe timer | `1,928,154,208 ns`; covers compile, `chmod`, and probe only because `end_mono` preceded `shasum` and `stat`; excludes hashing, stat collection, and cleanup and is not a complete-experiment wall |
| Successful cleanup end | UTC `2026-08-22T06:22:57Z`; cleanup monotonic endpoint was not captured |
| Full experiment-through-cleanup wall | Exact value `Unavailable`; second-resolution UTC custody proves conservative upper bound `<26 s`, therefore full preflight/build/probe/analysis/cleanup stayed `<=120 s` |
| Timer boundary limitation | The exact cleanup-inclusive nanosecond wall cannot be reconstructed and is not reported as 1.928 s. The conservative bound is sufficient only for the hard-ceiling proof. |
| Resource model | Peak live logical regular data `<=16 MiB` (unlinked 4-MiB seed plus three named 4-MiB files) plus about 38 KiB source/binary; named post-probe data 12 MiB; transient and retained limits passed; retained bytes zero |
| CPU/RSS/Q | Not instrumented and `Unavailable`; no LayerFS Q or SQLite state involved |
| Storage observations | Logical/apparent size and `st_blocks` recorded. Blocks are allocation observations, not proof of physical bytes or shared extents. |
| Retain rule | Retain the native primitive only as a capability if clone succeeds and a patched clone does not mutate the unlinked seed; PASS |
| Cleanup | An initial unsafe cleanup request was rejected before execution. A later `/usr/bin/unlink` attempt failed because the path does not exist and deleted nothing. Final explicit `/bin/unlink` of five named files followed by `/bin/rmdir` succeeded. Namespace absent; zero retained bytes. |

### Exact timed command

```sh
set -euo pipefail
experiment_root=/tmp/layerfs-g4-r1-materialization-capability.t3AFdJ
start_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ)
start_mono=$(python3 -c 'import time; print(time.monotonic_ns())')
/usr/bin/clang -Wall -Wextra -Werror "$experiment_root/probe.c" -o "$experiment_root/probe"
/bin/chmod 0500 "$experiment_root/probe"
"$experiment_root/probe" "$experiment_root"
end_mono=$(python3 -c 'import time; print(time.monotonic_ns())')
end_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ)
/usr/bin/shasum -a 256 "$experiment_root/probe.c" "$experiment_root/probe" "$experiment_root/clone" "$experiment_root/sparse" "$experiment_root/prealloc"
/usr/bin/stat -f 'path=%N mode=%Sp size=%z blocks512=%b dev=%d inode=%i' "$experiment_root" "$experiment_root/probe.c" "$experiment_root/probe" "$experiment_root/clone" "$experiment_root/sparse" "$experiment_root/prealloc"
printf 'start_utc=%s\nend_utc=%s\nstart_monotonic_ns=%s\nend_monotonic_ns=%s\nwall_ns=%s\n' "$start_utc" "$end_utc" "$start_mono" "$end_mono" "$((end_mono-start_mono))"
```

The final cleanup commands were individually targeted `/bin/unlink` calls for
the five known paths followed by `/bin/rmdir` for the one known namespace. No
glob, recursive delete, repository path, broad temp root, or unresolved
variable was a deletion target.

### Raw direct results

```text
seed-linked size=4194304 blocks512=8192 inode=734666560 nlink=1
f_nocache_enable=success
seed-unlinked size=4194304 blocks512=8192 inode=734666560 nlink=0
clone-before-write size=4194304 blocks512=8192 inode=734666561
cow_seed_first=17 cow_clone_first=165 independent=true
clone-after-write size=4194304 blocks512=8192
sparse-truncated size=4194304 blocks512=0
sparse-one-byte size=4194304 blocks512=8192
preallocated size=4194304 blocks512=8192
f_preallocate=success first_contig_errno=0 bytesalloc=4194304
```

Output SHA-256 values before cleanup:

```text
clone       32b5296b0474973598987a4fb7a7acf06235d183ccba4d43510fb9098738ca8f
sparse      e51c132c66a07f8e76e0cf43a91ee5be9045d5ed48e182a836598b909195bdc8
preallocated bb9f8df61474d25e71fa00722318cd387396ca1736605e1248821cc0de3d3af8
```

### Result and limitations

The probe supports only these claims:

- same-volume APFS `fclonefileat` from an unlinked read-only descriptor works
  on this host/volume;
- the cloned file's one-byte change did not change the seed byte;
- `F_NOCACHE` and `F_PREALLOCATE` returned success; and
- a 4-MiB sparse file initially reported zero blocks, but after one byte was
  written this sample reported the full 4-MiB allocation.

It does not support per-syscall performance, clone throughput, physical/shared
extent bytes, true device cold, stable-media completion, cross-volume
behavior, general sparse efficiency, or any G4 PASS. `F_NOCACHE` is an
alternate descriptor I/O policy, not proof that the ordinary path was evicted.

Exact source/binary/output hashes, the exact command text, and the embedded
direct-result subset are also retained in the materialization specialist
report, SHA-256
`5d5a78edf880e8738ab84fa4fcf1212eca135e382130b28b936bd35da78522d4`.
The deleted C source bytes and complete `stat` stdout were not retained and
cannot be reconstructed from the Round-1 documents.

## Zero-experiment lane records

| Lane | Count | Wall | Namespace/source/binary | Reason |
|---|---:|---:|---|---|
| reconstruction | 0 | 0 | none | Decisive next rows are timing-sensitive G4 measurements requiring lock/preregistration; static evidence closed current-path questions |
| core architecture | 0 | 0 | none | Design-only; verified-stream and segment lower-bound experiments remain prospective and separately one-variable |
| lead | 0 | 0 | none | Lead performed read-only inspection, external primary-source research, synthesis, and document validation only |

## Aggregate Round-1 experiment accounting

```text
executed experiments                    1
timing/performance experiments          0
G4 acceptance rows                      0
compile/probe timer only                1,928,154,208 ns
exact cleanup-inclusive wall            Unavailable
conservative cleanup-inclusive bound    <26 s
hard ceiling                            <=120 s per experiment
retained experiment bytes               0
maximum transient logical data          <=16 MiB + source/binary
source restoration required             no tracked source changed
cleanup                                 PASS
```
