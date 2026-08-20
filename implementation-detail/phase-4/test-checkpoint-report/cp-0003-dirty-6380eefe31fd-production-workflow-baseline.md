# CP-0003 — production-workflow baseline attempt 1

Status: `REVISE`
Date: 2026-08-20
Experiment mode: `baseline`
Total experiment wall before stop: 4.8 seconds
Retained artifact bytes: `91,280`
Transient databases and fixtures deleted: `yes`

## Identity

| Field | Value |
|---|---|
| Parent checkpoint | `CP-0002` |
| HEAD while built | `d781173a08ab4092eb539c3a0870056e6c6a77ff` |
| Compiled-source diff SHA-256 | `6380eefe31fd7c80ff279aa7371e567a60daa4552a21bedb77974f7556d34dc7` |
| Benchmark source SHA-256 | `159d473534af104a9228ca749ae046feb171a7a8d56cfd578acede65ad870376` |
| Release executable SHA-256 | `c2441d89a6d7b8c425f1e20a40373d79f154b88c297c1853d63d6079969966ec` |
| Runner SHA-256 | `ff8d09c93023bbdbd0da8daaa0755526f08de3d9a7febe64b099a128db2b3da3` |
| Raw JSONL SHA-256 | `ae2facbceb8e0699317c16dae3f00925abf366175f299b3147705df5a0e52291` |

## Result

The six 1-MiB smoke rows all passed:

```text
edit-same
edit-plus1
materialize-warm
materialize-fresh
read-range
reopen
```

Every returned row had exact identities, the expected reference count, and
terminal Q zero. The next row, 10-MiB `edit-same`, stopped before raw
publication with:

```text
LengthMismatch { expected: 531, actual: 530 }
```

The deterministic middle replacement changed the exact FastCDC result from
531 to 530 references. Therefore the 10-MiB operation is not a same-count edit
and must not be labeled or benchmarked as one. This is a fixture/operation
classification failure, not evidence of an engine identity or publication
failure.

## Decision

Decision: `REVISE`

Preserve this attempt. Do not rerun or overwrite it. The revised schedule will:

```text
1/10 MiB: materialization, range, and reopen smoke only
100 MiB: frozen proven same-count edit samples
100 MiB: one count-changing +1 structural guard
```

No threshold, result, or engine algorithm changes. Only the invalid small-file
same-count classification is removed prospectively.

## Compact evidence

```text
raw rows: 6
raw bytes: 89,166
raw SHA-256: ae2facbceb8e0699317c16dae3f00925abf366175f299b3147705df5a0e52291
temporary residue: none
```
