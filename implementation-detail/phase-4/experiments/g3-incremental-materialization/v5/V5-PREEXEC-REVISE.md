# G3-v5 pre-execution revision

Disposition: **REVISE_BEFORE_MEASUREMENT**

No v5 build, measured row, analyzer campaign child, or finalizer ran. At
classification time both
`target/phase4-g3-incremental-materialization-20260822-v5` and its `.lock` were
absent. All frozen v5 files remain zero-row historical evidence.

## Exact audit defect

The v5 contract promised that row stdout, stderr, and the enriched raw JSONL
entry were durably captured before the cleanup PREPARE. The runner actually used
`Path.write_text` for stdout/stderr and an ordinary buffered append for raw JSONL
without flush+fsync or parent-directory durability. A crash after durable
PREPARE could therefore preserve the intent to delete while losing the child
streams or raw row that PREPARE claimed had already been durably captured.

The same runner used ordinary buffered chronology appends and `write_text` for
`FAILURE-v5.json`, while its reports implied stronger preservation. Those files
were process-preserved on ordinary completion, not crash-durable.

v6 changes only evidence durability: child streams and enriched raw entries are
file-fsynced with their directory entries durably established before PREPARE;
chronology and failure records use the same explicit durable helpers. Cleanup,
source custody, counters, schedule, and candidate bytes remain unchanged.

## Frozen v5 hashes

| File | SHA-256 |
|---|---|
| `PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v5.md` | `09b12993ef0b501ff3d1e515c0f0f58ac843306befaa0f6e4ec7958b4710ab5f` |
| `COUNTER-DICTIONARY-v5.md` | `c0beaab729828b014968c381017eb0e0be85a681722478d78a8d01efe25bbfb3` |
| `run_g3_v5.py` | `0758874d16d14baacc136c6b559deac95591de57286c3ca9581b0288c5f8eea7` |
| `analyze_g3_v5.py` | `30a5f4012d34f6115db859a981c945e64a460157e896d424c714843f5ba8d289` |
| `recompute_g3_v5.py` | `5a24bd94734db24d8efcee8dfa690d7d10b2fe26651752f080cf96c0f45c1fa5` |
| `finalize_g3_v5.py` | `f48e173d53468989175475abc06885d18c4f5c89b4ed7a925062df9d2119e4c4` |
| `DRY-RUN-v5.json` | `611a9e9265023e966c8f2c0cf6b45509faa031cadcc0c11c78d1727d157643e3` |
| source-set digest | `70ef2606389813ebd980bf2e5fe9f4585333717fd7dabf21fb69cb4e4c140c9f` |
| methodology-set digest | `1cc302e1fa2fa648ee73dec9d63852bb695a542bf9070546dd3925b745d2e8b2` |
