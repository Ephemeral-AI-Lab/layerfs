# Install-only lane: contract prototype and measurements

**Not product code.** Nothing here is wired into the persistence crate or
`store.compact`; the prototype lives under `tools/bench/` to answer one design
question with measurements instead of assertions.

## The question

The frozen SQLite rebuild (`core/docs/issues/project-41/LayerFS-GC-spec.md`
§7 step 1 and §10) holds `lane::writer_admission` from before the mark through
the rename, and install refuses (`Conflict`) unless the sealed
`(size, mtime, db-header change counter)` tuple and the catalog tuple are
unchanged. That is why a rebuild is a short, whole-Store write outage.

An **install-only** lane would instead: mark on the build's read snapshot,
release the lane, let writers commit during the build, and take the lane again
just for install. That needs four things the frozen contract does not have:

1. an *open* mark with a high-water tuple, so a mark can be extended instead of
   refused when a commit lands;
2. an incremental **re-mark** at install from that tuple, walking the closure
   of roots committed after it (including references to pre-mark packs);
3. a **delta copy** of newly-live packs/objects/metadata into the staged
   target;
4. a bounded WAL and a replacement for the sealed-tuple equality fence, which
   always fails once writers continue.

`store.py` implements exactly that model; `wal.py`, `hazard.py` and `suite.py`
measure whether it can be made safe and bounded.

## Model

Store-shaped SQLite database, not a LayerFS Store:

- `pack` 1 MiB bodies; eight 128 KiB canonical objects per pack;
- `object_location`, `object_ref` (object → reference/base), roots
  (`root`/`root_object`), `kept_node`, and a single `high_water` row holding
  `(serial, next_pack_id, next_ordinal, pack_bytes, logical)`;
- 25% of packs are dead at the mark (no root reaches them). Their objects stay
  in the Store and are the dedupe targets a post-mark commit can reference;
- `store.build` is the fresh-copy build (`journal_mode=OFF` target, pack as a
  unit); `store.remark_delta` is the incremental re-mark plus delta copy;
  `store.verify` compares source and target closure at a fence serial.

## Probes

`wal.py` models the build's read snapshot as a read transaction (in production
it is the `immutable=1` attachment, which also stops every checkpoint):

| mode | what it measures |
|---|---|
| `hold` | WAL growth and writer latency while the snapshot is open; the checkpoint the snapshot blocked |
| `cap` | a commit gate that stalls writers above a WAL cap (backpressure) |
| `cap_drain` | the same cap with a checkpoint loop running (the WAL can shrink) |
| `catchup` | a pre-filled WAL, the snapshot closing, then writers plus a `wal_checkpoint(TRUNCATE)` loop: can install drain the backlog while writers continue? |

`hazard.py` has three correctness probes: a naive "new packs only" delta versus
a closure re-mark (dedupe hazard), a post-mark commit that re-references the
whole dead set (delta bound), and writers running through the delta copy (why a
catch-up fence is required).

`suite.py` runs the small arm and the 10 GiB arm. **The 50 GB arm is never
run.**

```sh
python3 suite.py --arm small   --dir /var/tmp/installlane/small
python3 suite.py --arm ten_gib --dir /var/tmp/installlane/ten_gib
```

## Limits

- A physical-shape model, not a real LayerFS Store: it validates the contract
  and the byte/time ratios, not production wall time.
- The mark is synthetic (metadata-only) and separate from the build.
- The delta copy reads the post-mark packs through the source WAL and writes
  the target with `journal_mode=OFF`; it is a lower bound on install work.
- One run per arm on this host; the host is a 88-core Docker sandbox with the
  backing store reported in `RESULTS.md`. Write and checkpoint rates are
  host-specific; the contract argument is in the ratios and the *busy* results,
  not only in absolute seconds.
