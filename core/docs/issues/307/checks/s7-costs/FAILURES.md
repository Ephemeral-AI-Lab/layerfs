# S7 checkpoint failures

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

- `costs-initial.log`: both new fixtures failed. The metadata assertion omitted
  the 32-byte root returned by route validation. source_read also validates its
  retained source root, so a complete dense-cell read returns 4096+64 BLOB bytes.
  The 96-page quota allowed the chosen 128 KiB mutation. The existing schema14
  SQLite-full proof uses 64 pages; the same explicit fixture quota now causes
  SQLITE_FULL in this one attempted mutation. These are functional count fixtures,
  not altered performance gates. The original outputs are retained.
- `build-history-and-costs.log`: SDK import selected StageDisposition from the
  crate root; the public enum lives at layerfs_history::error::StageDisposition.
- `build-history-repaired.log`: the fixed root reply decoder had changed its result
  to FilesystemRoot, while the caller still tried to decode it as byte input.
  The caller now consumes the already decoded fixed value.
- SDK conflict fixture compiler and native helper failures are separately retained
  in `../s9-history/FAILURES.md`.

No test reached 120 s. No failed product SQL/storage/history operation was replayed.
No timing/RSS/cold-state qualification or S7 completion is claimed. The raw original
crates.io API read was denied HTTP403; the distinct registry sparse index succeeded
and is retained as `fuser-registry-index-20261006.jsonl`.
