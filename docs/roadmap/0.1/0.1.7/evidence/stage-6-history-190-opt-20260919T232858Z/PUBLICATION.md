# Publication and local artifact retention

> Status: Research; informative and not a product contract.

The completed #190 parent-batching campaign is being committed with product source,
external tests, harness instrumentation, raw JSON/JSONL/timing/phase receipts,
per-state tables, check logs, source/binary manifests and independent review.
No original measurement receipt is rewritten or promoted.

Large SQLite Stores and compiled executables remain immutable local evidence,
consistent with the repository's existing local benchmark archive policy. Their
original paths, sizes and SHA-256 identities are in [LOCAL-ARTIFACTS.json](LOCAL-ARTIFACTS.json).
They are intentionally not Git payloads. The original complete campaign directory
on the collection host remains intact. Historical evidence manifests list those
local artifacts as well as committed files; absence from a Git checkout is a stated
retention boundary, not a missing or fabricated hash.

The numeric analysis can be reproduced from committed traces/receipts with
`analyze_results.py`; independently repeating the byte-for-byte Store hash check
requires the retained local files. The review completed that check on the host.
Earlier reports' no-commit/worktree statements describe their collection time;
the subsequent publication commit does not retroactively change sample identities.
