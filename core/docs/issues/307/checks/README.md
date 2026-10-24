# Issue 307 check receipts

> **Status:** Archived; retained for historical evidence only.

This directory keeps the receipts, ledgers and raw text output of the #307
checks and qualification runs. The completion records in the parent directory
summarize them.

## Raw SQLite databases removed (#398, 2026-10-22)

The raw SQLite files were removed from `main` to reduce the checkout size. These
were the three mounted-proof oracle databases
(`r8-qualification-20261010`, `r8b-requalification-20261010` and
`r8c-close-20261010`, `.../oracle.artifacts/expected.sqlite`, about 250 MB) and
the leftover `store.sqlite-wal`/`store.sqlite-shm` files under
`init-acquisition-benchmark-20261006/raw/`. Links to a removed file now resolve
only at the recovery tag.

## Raw run output removed (#398, 2026-10-22)

The raw machine output of four runs was also removed: every `.json`, `.jsonl`,
`.stderr`, `.stdout` and `.log` file under `r7-optimization-20261009`,
`r8-qualification-20261010`, `r8b-requalification-20261010` and
`r8c-close-20261010` (15,494 files, about 661 MB). This covers the per-call
`receipt.json`, `result.json` and `invocation.json` files, the numeric
validation dumps and the mounted-proof `expected.jsonl`/`observed.jsonl` logs.
The runs' `.md`, `.txt`, `.sh`, `.py`, `.sha256` and `.diff` files remain, and
the completion records in the parent directory (`R7-OPTIMIZATION-COMPLETION-*`,
`R8-COMPLETION-*`, `R8B-COMPLETION-*`, `R8C-COMPLETION-*`) are unchanged.

## Recovery

The last commit holding all removed files is tag `docs-raw-sqlite-final`
(`792715f00a`). Git history still contains every file. To restore one file or a
whole run directory:

```bash
git restore --source=docs-raw-sqlite-final -- <path>
```
