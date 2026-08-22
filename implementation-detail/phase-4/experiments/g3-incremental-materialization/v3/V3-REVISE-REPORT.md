# G3-v3 measured revision report

Disposition: **REVISE — orchestration/storage accounting defect after six once-only rows**

The v3 runner preserved its fresh result root after row 6 and removed its lock.
No row was rerun. The product mechanism did not cause the failure: every
captured row passed its route, byte/mode exactness, old-or-new, residue, Q/RSS,
and operation-time predicates. The runner accumulated all completed row work
roots, then applied the 512-MiB transient ceiling to their cumulative allocation
instead of retiring each independently complete row.

The retained target root is historical evidence and must remain byte-for-byte
unchanged:

```text
target/phase4-g3-incremental-materialization-20260822-v3/results-v3
```

## Terminal failure record

- `FAILURE-v3.json` SHA-256:
  `02b1066f7f005c8635da6d7339313bca46956fd5f7af5ee8a4c26685b55c2093`
- Status: `REVISE`
- Reason: `RuntimeError: transient storage ceiling`
- Global elapsed: `15,479,623,959 ns`
- Fresh raw rows: `6`
- Result root preserved: `true`
- Lock absent after failure: `true`

Exact identity hashes:

| Evidence | SHA-256 / identity |
|---|---|
| source set | `70ef2606389813ebd980bf2e5fe9f4585333717fd7dabf21fb69cb4e4c140c9f` |
| methodology set | `afe661ac6b5ef2019f445fd9ea563d7c4fb1908bb1075380e61987522ffc7851` |
| built/frozen executable | `82136ed86f19e645cb5611b9b520fe0454b947188a824e6b7022491421b34cd3` |
| `SOURCE-CUSTODY-v3.json` | `f114bfb60831f95ad7f9e0d2d1335a1fd2968e370718fda79aaf4356b34c845f` |
| `METHODOLOGY-CUSTODY-v3.json` | `cecdd72cc8e46eedfd5717d91b467dab6186b4ba47b2cee1aac8a4c0ba3da22c` |
| `OPERAND-CUSTODY-v3.json` | `351799b125c116cd8e5b2dcd1a4243386128ff93832ace3d5fde1906c92d8b97` |
| `G3-V3-RAW.jsonl` | `1452ff797a17402dd4ec2711960ccf9ce1c22715dbbfd83a4192a14f6390b063` |

## Six retained once-only rows

All six rows have `byte_exact=true`, `mode_exact=true`, terminal Q `0`, and
zero temporary/seed residue.

| Seq | Scenario | Route / reason | Outcome/state | Changed / patch bytes | Fallback / reconstructed bytes | Primary auth bytes | Operation ns | External RSS bytes |
|---:|---|---|---|---:|---:|---:|---:|---:|
| 1 | qualified-noop | qualified-noop / seed-hit | success/new | 0 / 0 | 0 / 0 | 0 | 657,459 | 16,416,768 |
| 2 | qualified-one-byte | qualified-patch / seed-hit | success/new | 1 / 1 | 0 / 0 | 22,551 | 6,014,833 | 16,465,920 |
| 3 | qualified-one-mib | qualified-patch / seed-hit | success/new | 1,048,576 / 1,048,576 | 0 / 0 | 1,086,013 | 2,936,250 | 16,547,840 |
| 4 | invalid-authority | complete-fallback / invalid-authority | success/new | 0 / 0 | 1 / 1,048,576 | 1,051,531 | 3,454,542 | 8,388,608 |
| 5 | external-mutation | complete-fallback / destination-invalidated | success/new | 0 / 0 | 1 / 1,048,576 | 1,051,531 | 4,065,791 | 8,339,456 |
| 6 | symlink-substitution | typed-rejection / destination-symlink | typed-error/old | 0 / 0 | 0 / 0 | 0 | 3,333 | 8,536,064 |

## Isolated versus cumulative allocated storage

The retained row roots independently remained below 512 MiB:

| Row root | Allocated bytes | Allocated KiB |
|---|---:|---:|
| `01-qualified-noop` | 42,401,792 | 41,408 |
| `02-qualified-one-byte` | 440,541,184 | **430,216** |
| `03-qualified-one-mib` | 43,544,576 | 42,524 |
| `04-invalid-authority` | 4,280,320 | 4,180 |
| `05-external-mutation` | 4,280,320 | 4,180 |
| `06-symlink-substitution` | 4,280,320 | 4,180 |

Largest isolated row: **430,216 KiB**, below the 512-MiB limit of 524,288
KiB. The six retained roots together reached **539,328,512 bytes / 526,688
KiB**, which crossed that limit and triggered the runner before row 7.

## v4 repair

v4 changes only orchestration: after each row's stdout/stderr, parsed row,
external timing, exactness/residue fields, path inventory hash/count, and
pre-delete row/WORK storage snapshots are durably appended, the runner removes
only that exact row root by enumerated no-follow unlink and `rmdir`, proves it
absent, and only then starts the next row. The final peak is the maximum
individual pre-delete row snapshot, never cumulative retained work.
