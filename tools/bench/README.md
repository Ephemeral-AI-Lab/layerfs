# Compact build strategy prototype

`compact_build_prototype.py` measures the shipped SQLite Store-compaction
build (`VACUUM INTO` + delete + full `VACUUM`) against a clone + delete +
`PRAGMA incremental_vacuum` prototype. It is a measurement tool, not a
production path.

The synthetic database mirrors the physical tables a build rewrites:
`pack.body` dominates (1 MiB per pack), `object_location`, `kept_node` and
`store_totals` are proportional. The suite is small (256 MiB) and 10 GB;
it never builds a 50 GB case.

```sh
python3 compact_build_prototype.py --suite --dir /tmp/compact-bench \
  --small-mb 256 --large-gb 10 --dead 0.25 --out bench.json
```

Each run is a fresh process. The input is `sync`ed and evicted with
`posix_fadvise(POSIX_FADV_DONTNEED)` before the run, SQLite's page cache is
capped at 32 MiB, and `synchronous=OFF` isolates storage work from commit
policy. Verification is metadata-only: counts, sums and distinct digests; no
column of `pack.body` is selected (`payload_readback: false`).

Strategies:

- `current` - `VACUUM INTO` a staged copy, delete rows the mark excluded,
  full `VACUUM`.
- `prototype` - `shutil.copyfile`, delete, then `PRAGMA incremental_vacuum`
  stepped in transactions of `--batch-pages` pages until the freelist is empty.
- `prototype_no_vacuum` - copy and delete only (the `none` upper bound).

`PRAGMA incremental_vacuum` on SQLite 3.40 relocates one page per statement
execution; `N` does not fan it out. Stepping it in autocommit costs a journal
create/delete per page, so the prototype batches the steps and bounds the
journal to `batch_pages * page_size`.
