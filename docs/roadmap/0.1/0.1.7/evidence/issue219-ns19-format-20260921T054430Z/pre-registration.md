# Pre-registration — #219 single-thread optimization of `pipeline-namespace-10000`

Worktree `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000`, branch `codex/219-ns10000`
at `9c46930b8`. Written **before** any measurement of this campaign was taken, at
2026-09-21T05:45Z. Nothing here is a result; every number quoted as "today" is copied from the
baseline row named below, not from a run of mine.

Comparison identity (not mine, the campaign's):

```
core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/
  ns17-squadA-packcounters-20260921T044814Z/pipeline-namespace-10000/receipt.json
```

## What is already established, and therefore not re-derived here

- 2,292,865,337 bytes handed to the pager for 302,023,232 bytes persisted (7.5917x), of which the
  WholeFile lane is 51.21x on 102 packs and the Native lane 3.64x on 1,142.
- `commit_ns` 1,145,315,268 over 17,378 commits (65.9 us/commit) and `sql_ns` 796,535,871 over
  16,595 object statements plus 16,802 pack writes; remainder 1,050,422,310 (31.4 % of the
  `accept_span_ns` denominator).
- The rewrite is caused by a directory that sits at the front of the pack and grows: every append
  moves every body behind it.

## A0 — identity arm (no treatment)

A fresh run of the unmodified tree in this worktree, `--no-build`, same command as the baseline
row. It exists to prove the row reproduces here before any treatment is measured against it. A0 is
**not** a treatment and carries no expected movement; if A0's pinned counters or root digest differ
from the baseline row, every later comparison is against A0 and the difference is reported.

Command (exact):

```sh
cd core/benchmark/fs-bench-pro-storage-content && python3 runner.py perf --lane smoke \
  --out benchmark-results/issue219/ns19-A0-baseline-<UTC> \
  --case pipeline-namespace-10000 --verify full --no-build
```

## D1 — remainder instrumentation (DIAGNOSTIC, not a treatment)

Adds charge sites only, in the product's own `SaveProfile` shape, for regions that are currently
charged to nothing: `BEGIN IMMEDIATE`, `validate_candidates`, `ObjectRow` vector construction, the
per-wave `lookup::locations` + presence seeding in `flush_batch`, the parts of `offer` outside
`resolve`/`full`/`delta`, the tail of `write_pack` (the `saves` ceiling update and the caches), the
publication path in `finish_inner` outside its `sql`/`commit` charges, and the harness-side accept
plumbing measured from the driver.

- Predicted: the named fields cover **>= 80 %** of the published remainder (1,050,422,310 ns).
- Refuted if: the named fields total < 50 % of the remainder, or the row stops being PASS.
- This change must move **no** pinned counter and **no** root digest. If any moves, the instrument
  is not an instrument and is reported as a failed diagnostic.
- D1 is a diagnostic. It is reported as a diagnostic and never as an optimization.

## T1 — reserved-directory pack framing + in-place append (THE TREATMENT)

One difference, stated once:

> The pack directory moves to a **reserved fixed-width region** immediately after the 16-byte
> header (`directory_entry_len(lane) x GROUP_COUNT_LIMIT` bytes), so group bodies start at a fixed
> offset and never move; the pack row's BLOB is allocated at the lane's pack limit with an explicit
> `used` length, and an append writes only the new directory entries and the new bodies through
> SQLite incremental blob I/O (`zeroblob` + `sqlite3_blob_write`) instead of binding the reassembled
> whole pack to `UPDATE object_packs SET data = ?2`.

Expected movement in the instrument's own units:

| instrument | today | expected after T1 |
| --- | ---: | ---: |
| pack bytes written (new counter `pipeline.pack_bytes_written`; today's figure 2,292,865,337 is the campaign's directory parse of the pinned store) | 2,292,865,337 B | ~3.1e8 B (302 MB of bodies + ~2.5e5 B of directory entries) |
| `pipeline.profile_commit_ns` | 1,145,315,268 ns | <= 5.0e8 ns (dirty pages per commit fall from ~34 to ~2 WholeFile / ~11 Native) |
| `pipeline.profile_sql_ns` | 796,535,871 ns | lower: no whole-pack parameter binding, no whole-pack assembly |
| `operation_ns` | 3,351,043,917 ns | a fall of 0.7-1.4 s |
| CPU (user+system) | 3,307,937,000 ns | a comparable fall |

Must NOT move: `digest:filesystem_root` `1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847`
and all 14 pinned counters, of which the two load-bearing ones are `pipeline.commits` 17378 and
`pipeline.inserted` 25245.

Declared in advance — the store is **expected to differ**:

- `sample.sqlite` is capacity-padded (`zeroblob` at the lane pack limit), so its sha256 is **not**
  expected to equal `03918d61a9f004b292deafeadbea6992f870b8bdd96b3a631d4bc3309408db29`. The claim
  is that the *stored canonical identity* is unchanged (root digest + pinned counters), not that the
  file is byte-identical.
- `pipeline.packs_created` / `pipeline.pack_appends` may move slightly: the reserved directory
  region costs each pack a fixed number of body bytes, so a pack may hold one group fewer. These
  two are **not** pinned. A movement of more than 5 % of `packs_created` is a red flag that the
  reserved region was sized wrongly and must be reported.
- `SCHEMA_VERSION` 8 -> 9 and the pack framing versions are bumped to new numbers. Old framing
  versions are **refused explicitly** (`UnsupportedPolicy { field: "pack framing version" }`), which
  is the same mechanism that already refuses versions 0, 3, 5, 8 and 99; old *Stores* are refused
  at open by the existing user_version check, before any pack is read. Both directions are handled
  by refusal, not by a guess.

Refuted if:

1. `commit_ns` and pack bytes written both fall but `operation_ns` does not move — then the rewrite
   was not the binding cost and the campaign's mechanism claim is wrong;
2. any pinned counter or the root digest moves;
3. `packs_created` moves by more than 5 %;
4. the store cannot be reopened and fully verified by `--verify full`;
5. `cargo test -p layerfs-storage` goes red on any of `cas_reuse`, `delta_payload`,
   `pack_watermark`, `multi_writer`, `visibility`, `persistence_failure`, `pack_locator`.

## T2 — pack geometry (L4), only if T1 lands and time remains

`PACK_LIMIT` 256 KiB -> 128 KiB with the whole-file threshold clamped so a store still accepts its
own whole-file objects. Registered here so it is not a post-hoc idea: expected ~12 % of the row,
refuted if it changes the root digest or if the threshold clamp lets an oversized whole-file record
be stored as a non-singleton group.

## Method that binds this campaign

- One sample per case per arm, fresh `--out`, never an existing path, never a re-run to make a
  number look right.
- Single thread: `LAYERFS_CONSTRUCTION_WORKERS=1`, no extra lane, no helper thread.
- No relaxation of a limit to convert a miss into a pass. The `PACK_LIMIT` change in T2 is a
  deliberate, pre-registered engineering trade and is measured as such.
- Every claim below is sourced to a receipt field, a raw file or a `file:line`, or marked
  `NOT_MEASURED`.
