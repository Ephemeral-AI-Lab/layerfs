# G3-v8 measured revision report

Disposition: **REVISE — relocated primary analyzer repository derivation**

The v8 build, all nine once-only rows, and all 18 durable cleanup events
completed successfully. The primary analysis child then failed before emitting
JSON because its frozen copied script derived `REPO` from its own relocated
`methodology-v8` path. `HERE.parents[4]` resolved to
`/Users/yifanxu/Ephemeral-AI-Lab`, one level above the repository, so source
custody tried to stat nonexistent `/Users/yifanxu/Ephemeral-AI-Lab/Cargo.lock`.

No row is reused by v9. The complete v8 root is preserved unchanged and its lock
is absent:

```text
target/phase4-g3-incremental-materialization-20260822-v8/results-v8
```

## Failure and custody hashes

- `FAILURE-v8.json`: `fa359cd6a694a5b1a0dee6348faa66c4ec7aa335528a4048db6e91cc5725c8f0`
- Failure status/reason: `REVISE` /
  `RuntimeError: child failed: primary-analysis: exit 1`
- Global elapsed: `7,147,323,417 ns`
- Source set: `70ef2606389813ebd980bf2e5fe9f4585333717fd7dabf21fb69cb4e4c140c9f`
- Methodology set: `64b94dccfd6a9e180f1911cd321b1ba0f1f83844105b2ba2f38990281ceb5b75`
- Frozen executable: `82136ed86f19e645cb5611b9b520fe0454b947188a824e6b7022491421b34cd3`
- `SOURCE-CUSTODY-v8.json`: `a59bf59211c76e2f081bfc035d44ebcf1fa6c062292dd451c78a126f89fa32f9`
- `METHODOLOGY-CUSTODY-v8.json`: `8ac05f6ed4119a07d3a2b5ccee08963d472bd7f39edd581f9c16295fb1202d4e`
- `OPERAND-CUSTODY-v8.json`: `89b3cea3a84d0f8c85a947cbb1ecddc5ae5f390e9923ab8130f4925585ff9852`
- `G3-V8-RAW.jsonl`: `2008539a6e32ed0a7fab405d45766c024b65d674a89381cb067184195e4da4c4`
- `ROW-CLEANUP-v8.jsonl`: `2c576bc06c2d6862da767f31120cdda56eaa88eed26cbd544090ebf8567084c0`
- `CLEANUP-v8.json`: `2703f8902bd77d7a5f65318d9722dc1ea976e1026a987d9a2a68950beff68834`
- `CHRONOLOGY-v8.jsonl`: `b453345dce0d6bd3f6dc48ccad351d69f6d49edd0b8605cd30a1e0184bc9efaf`

Build custody records one exact offline release build. Build stdout was empty
SHA-256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`;
stderr was 62 bytes with SHA-256
`90c8a1a69c0155761cff4bfe0e61bbee007961d2b96729d2127c2e0c291489dd`.

## Nine retained once-only rows

Every row has exact bytes/mode, terminal Q zero, and zero temp/seed residue.

| Seq | Scenario | Route/reason | Result/state | Changed/patch | Fallback/reconstructed | Primary auth | Operation ns | RSS bytes |
|---:|---|---|---|---:|---:|---:|---:|---:|
| 1 | qualified-noop | qualified-noop/seed-hit | success/new | 0/0 | 0/0 | 0 | 682,834 | 16,596,992 |
| 2 | qualified-one-byte | qualified-patch/seed-hit | success/new | 1/1 | 0/0 | 22,551 | 4,777,333 | 16,465,920 |
| 3 | qualified-one-mib | qualified-patch/seed-hit | success/new | 1,048,576/1,048,576 | 0/0 | 1,086,013 | 3,143,792 | 16,547,840 |
| 4 | invalid-authority | complete-fallback/invalid-authority | success/new | 0/0 | 1/1,048,576 | 1,051,531 | 3,839,584 | 8,404,992 |
| 5 | external-mutation | complete-fallback/destination-invalidated | success/new | 0/0 | 1/1,048,576 | 1,051,531 | 4,226,042 | 8,536,064 |
| 6 | symlink-substitution | typed-rejection/destination-symlink | typed-error/old | 0/0 | 0/0 | 0 | 4,083 | 8,323,072 |
| 7 | count-change | complete-fallback/count-change | success/new | 0/0 | 1/1,048,577 | 1,051,532 | 3,555,041 | 8,388,608 |
| 8 | before-publication-fault | qualified-patch/seed-hit | typed-error/old | 1/1 | 0/0 | 21,882 | 535,083 | 8,585,216 |
| 9 | lost-ack | qualified-patch/seed-hit | success/new | 1/1 | 0/0 | 21,882 | 2,854,166 | 8,372,224 |

## Completed cleanup evidence

Cleanup status is PASS: 9 PREPARE + 9 COMPLETE records, exact anchored deletion
method, all row roots and WORK absent, and no broad deletion. The isolated peak
was 440,541,184 allocated bytes, below 512 MiB. Cumulative allocation was
552,169,472 bytes and remains descriptive only.

## Analyzer failure evidence and chronology

- Primary stdout: 0 bytes, SHA-256
  `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Primary stderr: 2,203 bytes, SHA-256
  `a4f7aa0b37d69206345952389671fd655b7b24e662416739da4b0040b79db1b9`
- Primary start: `7,080,517,875 ns`
- Primary exit 1: `7,147,110,667 ns`

The traceback ends at source custody attempting
`/Users/yifanxu/Ephemeral-AI-Lab/Cargo.lock`. Independent analysis was correctly
not started after the primary child failed. v9 runs a completely fresh schedule
and derives campaign repository custody from the validated supplied results
root, never from the relocated analyzer file.
