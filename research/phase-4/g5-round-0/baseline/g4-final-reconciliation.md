# Final G4 reconciliation for G5

## Authority

- Checkpoint: `d58c5a1307253dfc221fe50de996c183deb9458a`
- Commit subject: `checkpoint phase 4 G4 terminal baseline`
- G4 stage-terminal SHA-256: `0297ca2e3b49ddb7d8d2d435713450dcc336397b53cbaaaee9647a46eebcede8`
- Stage: **PASS / CLOSED**
- Sealed v12: **REVISE** under its original relative-only latency gate; `old_relative_only_gate_passed=false`
- Governing stage disposition: `PASS_WITH_USER_APPROVED_SUB_1MS_MICRO_VARIANCE_POLICY`

The target v12 evidence is local, immutable, and hash-pinned. Its terminal and terminal-verification hashes are `d3c6dba7cd114817c9153a0426d0a9cc92723bf58a7efc9830877673ff111b31` and `2837c7484238282e03b45876100be9cc4ca4fdfa1931b4cb4e173798809e0478`. G5 does not reanalyze or relabel v12.

## Protected scoreboard

| Operation/resource | Final G4 control |
|---|---:|
| Durable full create | 279.463 ms / 357.829 MiB/s |
| Same-open same-count edit | 8.043 ms |
| Early/middle +1 | 5.108 / 4.576 ms |
| Returned 1-MiB range | 2.046 ms / 488.823 MiB/s |
| Reopen/head | 3.583 ms |
| First edit after reopen | 154.019 ms |
| Warm reconstruction | 237.214 ms / 421.560 MiB/s |
| Fresh-process reconstruction | 237.381 ms / 421.263 MiB/s |
| First/full durable native materialization | 307.652 ms / 325.042 MiB/s |
| Same-open protected-seed full read | 10.058 ms / 9,942.582 MiB/s |
| One-byte incremental materialization | 4.104 ms |
| Peak whole-child RSS | 20,578,304 bytes |
| Maximum individual buffer | 1,048,576 bytes |
| Terminal Q / residue | exactly 0 / exactly 0 |

The 20,971,520-byte RSS ceiling leaves only 393,216 bytes over the accepted G4 peak. A future shadow must remain benchmark-private and sequential rather than adding a co-resident full-tree structure to the shared path.

## Final Canonical-v2 mapping correction

The G5 prompt's mapping-rewrite figures `365,495 / 185,915 / 7,098` are historical Canonical-v1-width values. Final G4 Canonical-v2 evidence controls:

| Operation | Final measured counter | File-mapping bytes |
|---|---:|---:|
| Same-count middle | 5,334 | 5,050 |
| 100-MiB early +1 | 196,375 | 196,091 |
| 100-MiB middle +1 | 100,763 | 100,479 |
| Fresh 100-MiB mapping | 196,174 | 196,055 |

The reductions reconcile exactly to the removed 32 bytes per serialized reference. Consequently, current exact 10x operation-counter gates are `<=19,637` early and `<=10,076` middle. Current file-only gates are `<=19,609` and `<=10,047`; live file mapping must be `<=205,857`, and the one-leaf authenticated path must be `<=5,302` bytes.

## Immutable product contract

CAS, FastCDC, Canonical-v2, K64/F64 COW, SQLite `FULL`/`DELETE`/`temp_store=FILE`/`mmap=0`, fail-closed authority, exact Q, native publication/reconciliation, and accepted golden identities remain protected. G5-1 changed none of them.
