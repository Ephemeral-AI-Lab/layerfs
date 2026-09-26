# #232 Workspace Exec/FUSE edit registry v1 — frozen case and cache contract

> **Status:** Phase 1 contract freeze. No timed sample exists. This document
> registers cases; it is not a performance receipt and carries no PASS.
> Written 2026-09-24 against the [#232 implementation specification](../../issues/232/SPEC.md).

Frozen artifacts (regenerate and check with `python3 core/benchmark/fs-bench-pro/shared/edit_contract.py --check`):

| artifact | value |
|---|---|
| registry document | `core/benchmark/fs-bench-pro/registry/workspace-exec-edit-v1.json` |
| registry SHA-256 | `e4b4d2fc67cb1630dc8f15283db3085663466071bb373c829a6026ffddb66aab` |
| generator / source of truth | `core/benchmark/fs-bench-pro/shared/edit_contract.py` |
| families | `families/workspace_exec_edit_{length_preserving,length_changing,canonical_chunk_count}.py` |
| focused checks | `core/benchmark/fs-bench-pro/tests/test_exec_edit_registry.py` (13 checks) |

## 1. Route and timing boundary

| contract field | value |
|---|---|
| `operation_contract_id` | `workspace-exec-fuse-edit-commit-v1` |
| route | `sdk-exec-fuse-edit-commit-v1` |
| `operation_surface` | `workspace-posix-fuse` |
| `operation_entrypoint` | `WorkspaceApi::exec` |
| `acknowledgement_boundary` | `WorkspaceApi::commit` |
| `orchestration_executor` | `layerfs-sdk` release benchmark driver |
| `mutation_executor` | `workspace-exec-posix-edit-tool` (sealed release binary invoked by Exec) |
| `implementation_route` | `layerfs-daemon-fuse-workspace-commit` |
| timing | one caller LFT1 root `sdk.edit_commit.fuse` with `edit` and `commit` children |
| `scenario_version` / repetition | 1 / 1 |

The timer starts immediately before `WorkspaceApi::exec` and stops on the typed
`WorkspaceApi::commit` result. Branch fork, Sandbox create, Mount, Status,
Unmount, Sandbox delete, preparation and verification are outside it and are
reported separately. A failed Exec never reaches Commit; an unknown Commit result
is retained as uncertain and is never replayed.

## 2. Fixture contract

Six pristine masters are prepared once and reused; no case regenerates a fixture
and no capped-v1 duplicate is added.

| label | bytes | fixture SHA-256 | initial canonical count |
|---|---:|---|---:|
| 1mib | 1,048,576 | `d7dfe3d2828aceb85177e6efbeb600f23672a326c902e525e401c1545bb05bdc` | 54 |
| 10mib | 10,485,760 | `29c89128c748e4404f31b0147d447bd524d7b75afc98d56ac4debac762ee4b79` | 544 |
| 100mib | 104,857,600 | `1bb2d79d54f72ae15eb0bb76ad715b9aafeba8ff8f9aa4f47bad3e3f101885bd` | 5,394 |
| 500mib capped (insert/append/prepend/zero-extend) | 524,283,904 | `f1b6c61d9c126beba89dd2a310f727fd63cbbf131b793a78fe21247238c98c1f` | 26,994 |
| 500mib capped (replace-grow) | 524,285,952 | `0e2cc5b14abf95553ba633a11b395c80de3f0a1642cdef0adf753cb5984fbe55` | 26,995 |
| 500mib | 524,288,000 | `bd782f202ec4c40a2070a1d08b78f5135a0ac604b871e4907846740bde906157` | 26,995 |

Recipe: `splitmix64` byte stream continued from generator seed `0x4C41594552465331`,
one file `payload.bin` (mode `0o640`, directory `0o750`, mtime `1_700_000_000`).
The recipe in `shared/edit_contract.py` reproduces all six pinned v0.1.6 fixture
digests and, for the canonical family, the pinned v0.1.6 result digests — an
independent check that the frozen payload/offset/result definitions are the
historical ones.

## 3. Cases

Every scenario ID is the historical selection ID with `-exec-v1` appended, under
a distinct `workspace_exec_edit_*` family ID; no row inherits the historical
direct-SDK `edit_*` PASS, and the G2 PASS status is never reused. The per-case
target is that same historical selection's G2 candidate `edit_commit_ns` from the
[#152 final report](../../../docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md)
at 0.01 ms precision.

### `workspace_exec_edit_length_preserving` (12 cases)

| scenario_id | fixture B | edit | algorithm | G2 target ms | status |
|---|---:|---|---|--:|---|
| `overwrite-head-4k-on-1mib-ops-1-exec-v1` | 1,048,576 | pwrite 4096 B at 0 | single-positional-write | 6.31 | REGISTERED |
| `overwrite-head-4k-on-10mib-ops-1-exec-v1` | 10,485,760 | pwrite 4096 B at 0 | single-positional-write | 5.90 | REGISTERED |
| `overwrite-head-4k-on-100mib-ops-1-exec-v1` | 104,857,600 | pwrite 4096 B at 0 | single-positional-write | 5.81 | REGISTERED |
| `overwrite-head-4k-on-500mib-ops-1-exec-v1` | 524,288,000 | pwrite 4096 B at 0 | single-positional-write | 7.34 | REGISTERED |
| `overwrite-middle-4k-on-1mib-ops-1-exec-v1` | 1,048,576 | pwrite 4096 B at 522240 | single-positional-write | 5.63 | REGISTERED |
| `overwrite-middle-4k-on-10mib-ops-1-exec-v1` | 10,485,760 | pwrite 4096 B at 5240832 | single-positional-write | 5.70 | REGISTERED |
| `overwrite-middle-4k-on-100mib-ops-1-exec-v1` | 104,857,600 | pwrite 4096 B at 52426752 | single-positional-write | 6.43 | REGISTERED |
| `overwrite-middle-4k-on-500mib-ops-1-exec-v1` | 524,288,000 | pwrite 4096 B at 262141952 | single-positional-write | 10.01 | REGISTERED |
| `overwrite-tail-4k-on-1mib-ops-1-exec-v1` | 1,048,576 | pwrite 4096 B at 1044480 | single-positional-write | 4.72 | REGISTERED |
| `overwrite-tail-4k-on-10mib-ops-1-exec-v1` | 10,485,760 | pwrite 4096 B at 10481664 | single-positional-write | 6.06 | REGISTERED |
| `overwrite-tail-4k-on-100mib-ops-1-exec-v1` | 104,857,600 | pwrite 4096 B at 104853504 | single-positional-write | 5.82 | REGISTERED |
| `overwrite-tail-4k-on-500mib-ops-1-exec-v1` | 524,288,000 | pwrite 4096 B at 524283904 | single-positional-write | 7.37 | REGISTERED |

### `workspace_exec_edit_length_changing` (32 cases)

| scenario_id | fixture B | edit | algorithm | G2 target ms | status |
|---|---:|---|---|--:|---|
| `insert-middle-4k-on-1mib-ops-1-exec-v1` | 1,048,576 | unfrozen | unfrozen-in-place-window-shift | 5.30 | NOT_RUN |
| `insert-middle-4k-on-10mib-ops-1-exec-v1` | 10,485,760 | unfrozen | unfrozen-in-place-window-shift | 6.40 | NOT_RUN |
| `insert-middle-4k-on-100mib-ops-1-exec-v1` | 104,857,600 | unfrozen | unfrozen-in-place-window-shift | 6.25 | NOT_RUN |
| `insert-middle-4k-on-500mib-result-capped-v2-ops-1-exec-v1` | 524,283,904 | unfrozen | unfrozen-in-place-window-shift | 7.76 | NOT_RUN |
| `delete-middle-4k-on-1mib-ops-1-exec-v1` | 1,048,576 | unfrozen | unfrozen-in-place-window-shift | 5.06 | NOT_RUN |
| `delete-middle-4k-on-10mib-ops-1-exec-v1` | 10,485,760 | unfrozen | unfrozen-in-place-window-shift | 5.75 | NOT_RUN |
| `delete-middle-4k-on-100mib-ops-1-exec-v1` | 104,857,600 | unfrozen | unfrozen-in-place-window-shift | 6.41 | NOT_RUN |
| `delete-middle-4k-on-500mib-ops-1-exec-v1` | 524,288,000 | unfrozen | unfrozen-in-place-window-shift | 12.16 | NOT_RUN |
| `append-tail-4k-on-1mib-ops-1-exec-v1` | 1,048,576 | pwrite 4096 B at 1048576 | single-positional-write | 5.76 | REGISTERED |
| `append-tail-4k-on-10mib-ops-1-exec-v1` | 10,485,760 | pwrite 4096 B at 10485760 | single-positional-write | 5.71 | REGISTERED |
| `append-tail-4k-on-100mib-ops-1-exec-v1` | 104,857,600 | pwrite 4096 B at 104857600 | single-positional-write | 5.60 | REGISTERED |
| `append-tail-4k-on-500mib-result-capped-v2-ops-1-exec-v1` | 524,283,904 | pwrite 4096 B at 524283904 | single-positional-write | 6.40 | REGISTERED |
| `prepend-head-4k-on-1mib-ops-1-exec-v1` | 1,048,576 | unfrozen | unfrozen-in-place-window-shift | 5.12 | NOT_RUN |
| `prepend-head-4k-on-10mib-ops-1-exec-v1` | 10,485,760 | unfrozen | unfrozen-in-place-window-shift | 5.65 | NOT_RUN |
| `prepend-head-4k-on-100mib-ops-1-exec-v1` | 104,857,600 | unfrozen | unfrozen-in-place-window-shift | 6.54 | NOT_RUN |
| `prepend-head-4k-on-500mib-result-capped-v2-ops-1-exec-v1` | 524,283,904 | unfrozen | unfrozen-in-place-window-shift | 6.73 | NOT_RUN |
| `replace-grow-middle-2k-to-4k-on-1mib-ops-1-exec-v1` | 1,048,576 | unfrozen | unfrozen-in-place-window-shift | 5.75 | NOT_RUN |
| `replace-grow-middle-2k-to-4k-on-10mib-ops-1-exec-v1` | 10,485,760 | unfrozen | unfrozen-in-place-window-shift | 6.81 | NOT_RUN |
| `replace-grow-middle-2k-to-4k-on-100mib-ops-1-exec-v1` | 104,857,600 | unfrozen | unfrozen-in-place-window-shift | 8.44 | NOT_RUN |
| `replace-grow-middle-2k-to-4k-on-500mib-result-capped-v2-ops-1-exec-v1` | 524,285,952 | unfrozen | unfrozen-in-place-window-shift | 8.63 | NOT_RUN |
| `replace-shrink-middle-4k-to-2k-on-1mib-ops-1-exec-v1` | 1,048,576 | unfrozen | unfrozen-in-place-window-shift | 5.30 | NOT_RUN |
| `replace-shrink-middle-4k-to-2k-on-10mib-ops-1-exec-v1` | 10,485,760 | unfrozen | unfrozen-in-place-window-shift | 5.89 | NOT_RUN |
| `replace-shrink-middle-4k-to-2k-on-100mib-ops-1-exec-v1` | 104,857,600 | unfrozen | unfrozen-in-place-window-shift | 7.13 | NOT_RUN |
| `replace-shrink-middle-4k-to-2k-on-500mib-ops-1-exec-v1` | 524,288,000 | unfrozen | unfrozen-in-place-window-shift | 7.35 | NOT_RUN |
| `truncate-tail-4k-on-1mib-ops-1-exec-v1` | 1,048,576 | truncate to 1044480 | bounded-truncate | 5.03 | REGISTERED |
| `truncate-tail-4k-on-10mib-ops-1-exec-v1` | 10,485,760 | truncate to 10481664 | bounded-truncate | 9.14 | REGISTERED |
| `truncate-tail-4k-on-100mib-ops-1-exec-v1` | 104,857,600 | truncate to 104853504 | bounded-truncate | 5.44 | REGISTERED |
| `truncate-tail-4k-on-500mib-ops-1-exec-v1` | 524,288,000 | truncate to 524283904 | bounded-truncate | 7.19 | REGISTERED |
| `zero-extend-tail-4k-on-1mib-ops-1-exec-v1` | 1,048,576 | extend to 1052672 | bounded-extend | 4.94 | REGISTERED |
| `zero-extend-tail-4k-on-10mib-ops-1-exec-v1` | 10,485,760 | extend to 10489856 | bounded-extend | 5.52 | REGISTERED |
| `zero-extend-tail-4k-on-100mib-ops-1-exec-v1` | 104,857,600 | extend to 104861696 | bounded-extend | 5.76 | REGISTERED |
| `zero-extend-tail-4k-on-500mib-result-capped-v2-ops-1-exec-v1` | 524,283,904 | extend to 524288000 | bounded-extend | 6.50 | REGISTERED |

### `workspace_exec_edit_canonical_chunk_count` (12 cases)

| scenario_id | fixture B | edit | algorithm | G2 target ms | status |
|---|---:|---|---|--:|---|
| `overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1-exec-v1` | 1,048,576 | pwrite 65536 B at 147456 | single-positional-write | 6.20 | REGISTERED |
| `overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1-exec-v1` | 10,485,760 | pwrite 65536 B at 147456 | single-positional-write | 7.24 | REGISTERED |
| `overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1-exec-v1` | 104,857,600 | pwrite 65536 B at 147456 | single-positional-write | 7.46 | REGISTERED |
| `overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1-exec-v1` | 524,288,000 | pwrite 65536 B at 147456 | single-positional-write | 8.13 | REGISTERED |
| `overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1-exec-v1` | 1,048,576 | pwrite 65536 B at 147456 | single-positional-write | 6.84 | REGISTERED |
| `overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1-exec-v1` | 10,485,760 | pwrite 65536 B at 147456 | single-positional-write | 6.90 | REGISTERED |
| `overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1-exec-v1` | 104,857,600 | pwrite 65536 B at 147456 | single-positional-write | 8.01 | REGISTERED |
| `overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1-exec-v1` | 524,288,000 | pwrite 65536 B at 147456 | single-positional-write | 9.27 | REGISTERED |
| `overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1-exec-v1` | 1,048,576 | pwrite 65536 B at 147456 | single-positional-write | 6.02 | REGISTERED |
| `overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1-exec-v1` | 10,485,760 | pwrite 65536 B at 147456 | single-positional-write | 6.63 | REGISTERED |
| `overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1-exec-v1` | 104,857,600 | pwrite 65536 B at 147456 | single-positional-write | 13.54 | REGISTERED |
| `overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1-exec-v1` | 524,288,000 | pwrite 65536 B at 147456 | single-positional-write | 7.98 | REGISTERED |

### The 20 structural cases (visible, `NOT_RUN`)

`insert-middle-4k`, `delete-middle-4k`, `prepend-head-4k`,
`replace-grow-middle-2k-to-4k` and `replace-shrink-middle-4k-to-2k` at each of
1/10/100/500 MiB remain registered with their historical IDs, payload digests and
G2 targets, and are recorded `NOT_RUN` under the single frozen reason
`structural-shift-algorithm-unfrozen`: no authentic POSIX/FUSE algorithm is
frozen for them yet, their in-place window shift would move up to hundreds of MiB
through the projection, and a temporary-file-and-rename save needs up to 500 MiB
against a 16 MiB `/tmp` and a 1 GiB Workspace disk budget. They are not shrunk,
substituted, skipped silently, or excused as a regression.

## 4. Cache contract (`exec-fuse-edit-cache-v1`)

Each case uses an independent writable byte copy of the closed, validated
prepared master (`clone_method = independent-writable-byte-copy`). A clone is
setup reuse and is never a cold claim.

| domain | acquisition | check | eligibility |
|---|---|---|---|
| `macos-store` (Store copy + history catalog) | map read-only, instantiate resident pages, `msync(MS_INVALIDATE\|MS_SYNC)` them away | non-faulting `mincore` over every page | PASS only at zero resident pages |
| `linux-fuse-backing` (container Workspace backing and spool) | fresh per-case Sandbox and Mount; no host-side or product-side invalidation exists | not checkable without a benchmark-only eviction between Edit and Commit, which the contract forbids | `INELIGIBLE` for a cold claim: Commit may read the bytes Edit just wrote from resident backing pages |

The second domain is why no #232 row may be reported as a cold-qualified PASS: its
raw number is retained as a declared-warm diagnostic with
`admission_eligible=false` and the reason
`fuse-backing-domain-served-by-resident-pages-not-invalidatable`. Making it cold
requires an authentic product residency change with its own review; a
benchmark-only eviction between Edit and Commit is forbidden. Cold and warm rows
are never pooled.

## 5. Eligibility rules (frozen before implementation)

1. one performance sample per registered case and arm at a frozen source identity; a rerun is never a better number.
2. the performance command is the release SDK driver through target/release/examples/.
3. the Exec result must be exit_status == 0 with untruncated output; otherwise the attempt is FAIL.
4. the row needs one caller LFT1 root with complete edit and commit children and no dropped or overflowed output.
5. the row needs the SDK-only route check: expected public call count/order and zero forbidden or fallback route counts, proved by post-timer public Workspace status projection counts.
6. the row needs its declared macOS Store-domain residency check to report zero resident pages; an unacquirable or resident domain is INELIGIBLE.
7. the Linux FUSE/backing domain is declared warm-by-construction and cannot be invalidated; a row carrying it is INELIGIBLE for a cold claim and keeps its raw diagnostic number.
8. the independent verifier must PASS with a fresh reopen and the declared bounded oracle inside its 15 s hard cap.
9. cleanup must be confirmed through public SandboxApi::delete with no orphan container or volume.
10. the complete performance command, including fresh Sandbox create and delete, must finish inside 15 s.

## 6. Budgets

| budget | value |
|---|---|
| recurring preparation per case (1/10/100 MiB) | ≤ 5 s |
| recurring preparation per case (500 MiB) | ≤ 10 s |
| complete performance command (fresh Sandbox create + delete, cleanup included) | ≤ 15 s |
| independent verifier | 15 s hard cap, < 10 s design goal |
| bounded oracle | ≤ 196,608 bytes (edit boundary ± 65,536 bytes) |
| full-file digest oracle | declared for cases whose result is ≤ 104,857,600 bytes |

Measured `500 MiB` rows are verified with the bounded oracle and report
`full_file_bytes_verified=false` with exact coverage; a full 500 MiB digest is not
claimed.

## 7. Gate state

* Registry cardinality (12 + 32 + 12 = 56), uniqueness, six prepared sizes,
  capped inputs, per-case commands, algorithms, syscalls, payloads, oracles and
  G2 targets are committed and independently re-derivable from
  `shared/edit_contract.py`; 13 focused checks pass.
* The 20 structural cases are registered `NOT_RUN` with their frozen reason.
* No timed sample exists. Every row is `NOT_RUN` until the Phase 3 substrate
  exists and the Phase 4 eligibility rules are enforced.
* The eligibility rule above makes the FUSE/backing cache domain
  non-invalidatable, so unless the product's residency behaviour changes, the
  expected terminal status of a collected row is `INELIGIBLE` with a retained raw
  diagnostic rather than `GOAL_MET`. That outcome is recorded, not worked around.
