# G3-v4 pre-execution revision

Disposition: **REVISE_BEFORE_MEASUREMENT**

No v4 build, measured row, analyzer campaign child, or finalizer ran. At
classification time both
`target/phase4-g3-incremental-materialization-20260822-v4` and its `.lock` were
absent. All frozen v4 files remain historical zero-row evidence.

## Three exact final-audit defects

1. **Unanchored cleanup traversal and delete set.** v4 froze a path list with
   `Path.rglob`, then independently traversed again with pathname-based
   `os.walk`. It did not hold descriptor-relative no-follow custody over every
   ancestor. A late addition, component symlink/substitution, or directory race
   between inventory and deletion could make the actual deletion set differ
   from the recorded set, including deletion of an unrecorded path.
2. **No durable pre-delete intent.** `ROW-CLEANUP-v4.jsonl` received its only
   record after deletion. A crash during cleanup could remove some evidence
   while leaving no fsynced PREPARE record binding the pre-delete snapshots and
   exact intended path set. Append-only evidence therefore could not distinguish
   “cleanup never started” from “cleanup partially executed before death.”
3. **Incomplete independent finalizer enforcement.** The v4 finalizer checked
   several row-cleanup fields but did not independently require every promised
   cleanup field and exact `deletion_method`, nor exact paired durable
   PREPARE/COMPLETE ordering and bindings—because v4 had no such pair. A record
   with an omitted method or incomplete cleanup provenance could pass parts of
   the final closure.

v5 changes only cleanup evidence. It freezes one exact inventory, fsyncs a
PREPARE before removal, deletes only that set through descriptor-relative
no-follow operations rooted at the exact row directory, rejects additions or
substitutions, proves absence, then fsyncs the bound COMPLETE. Both analyzers
and the finalizer independently require nine exact pairs in schedule order.

## Frozen v4 hashes

| File | SHA-256 |
|---|---|
| `PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v4.md` | `c1046463d0ae074e9f16c03559cab3725d6e3320783ee1ea72b50f77e205f210` |
| `COUNTER-DICTIONARY-v4.md` | `7009271a5c42e00692231c899959260cb03ede23cc2e5e4d3c9f00dd4c490581` |
| `run_g3_v4.py` | `aa49e993c9d2c97e13a321ec524121e29cb98921a11413eea3310c9d0c8023e9` |
| `analyze_g3_v4.py` | `c243bf27b8ffed04c31b908a40d30c9eac06a6205a25b8e35c3e1f6da1999342` |
| `recompute_g3_v4.py` | `b624720874c039294728cb67c53a523e0ac14cc94e28f7144a7cc57a2f28209b` |
| `finalize_g3_v4.py` | `6575f24a368c389c940c7aa362c7aa9705c0c6d662618aed979a270eb99783db` |
| `DRY-RUN-v4.json` | `1814673363cf6e1b2a7ca2aee51b31aa089f1c98e02964efae6f5b3b548231b8` |
| source-set digest | `70ef2606389813ebd980bf2e5fe9f4585333717fd7dabf21fb69cb4e4c140c9f` |
| methodology-set digest | `fd6e79dbd44804b3328f6c9358c4fb5f0b8ef9e9beda4e8e6d388de769a278e2` |
