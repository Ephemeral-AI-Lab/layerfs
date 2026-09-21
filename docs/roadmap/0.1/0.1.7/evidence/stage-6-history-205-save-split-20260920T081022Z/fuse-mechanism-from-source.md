# v0.1.6's live FUSE mechanism, read from source

> Status: **Research; structural reading of reference source, not a measurement of the
> replacement core.** Every latency figure marked `[MEASURED]` is already receipted in
> this lane or in `issue154/final-complete-matrix.json`; everything else is the
> mechanism's shape read from code, and no new latency claim is made. Posted to
> [#207](https://github.com/Ephemeral-AI-Lab/layerfs/issues/207#issuecomment-5749557789)
> and referenced from [#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179#issuecomment-5749558562).
>
> Source read at branch `codex/190-pooled-scope`: `crates/layerfs-fuse/src/live_wire.rs`,
> `crates/layerfs-workspace/src/live_backing.rs`, `crates/layerfs-workspace/src/capture.rs`,
> `crates/layerfs-workspace/src/cow_tree.rs`,
> `crates/layerfs-layerstack-store/src/workspace.rs`,
> `crates/layerfs-layerstack-store/src/objects.rs`,
> `crates/layerfs-workspace/src/lifecycle.rs`.
>
> One correction is recorded inside §1: an earlier claim of mine that v0.1.6 has no
> prefetch was **wrong** — the four prefetch telemetry counters are indeed dead, but the
> prefetch path itself is live at `live_backing.rs:81` with an 8 KiB threshold.

### How v0.1.6's live FUSE mechanism actually works — diagrams, from source

Read from the reference source at `codex/190-pooled-scope`. **Everything below is structural, from code; the only *measured* numbers are marked `[MEASURED]`** and come from this lane's runs or `issue154/final-complete-matrix.json`. There are no new latency claims here — the point is to fix the mechanism's shape before `#207` measures it.

## 1. Read path — the map is cached, the territory is selectively prefetched

```
  kernel  read(2)/stat(2)/readdir(3)/execve(2)
     │
     ▼
┌──────────────────────── FUSE (crates/layerfs-fuse) ─────────────────────────┐
│                                                                             │
│   namespace / inode / attr queries                                          │
│     └──► LiveWorkspace nodes + DirectoryLookupCache         ──── CACHED     │
│                                                                             │
│   file content                                                              │
│     ├── edited this session ──► piece tree + host spool     ──── LOCAL      │
│     │                                                        (on disk)      │
│     └── immutable (FileData::Base)                                          │
│           ├── len ≤ 8 KiB ──► PREFETCHED into host backing  ──── CACHED     │
│           │                   live_backing.rs:81                            │
│           └── len > 8 KiB ──► SnapshotReader                ──── UNCACHED   │
└──────────────────────────────────┬──────────────────────────────────────────┘
                                   │ miss
                     ┌─────────────▼──────────────────┐
                     │  SnapshotCache                 │
                     │  8 MiB FIFO  (workspace.rs)    │
                     │  ✗ SKIPS >1 KiB chunks:        │
                     │    "large content chunks must  │
                     │     not displace the metadata  │
                     │     needed to locate them"     │
                     └─────────────┬──────────────────┘
                                   │ miss
                     ┌─────────────▼──────────────────┐
                     │  overlay (uncommitted writes)  │
                     └─────────────┬──────────────────┘
                                   │ miss
                     ┌─────────────▼──────────────────┐
                     │  db.read_object_row(id)        │  objects.rs:3852
                     │   location lookup              │  ✗ no cache beneath
                     │   + visit_locations → pack     │
                     └────────────────────────────────┘
```

**The load-bearing constant is `IMMUTABLE_PREFETCH_FILE_BYTES = 8 * 1024`** (`crates/layerfs-fuse/src/live_wire.rs:17`). It splits the workload in two:

| | tier | consequence |
| --- | --- | --- |
| ≤ 8 KiB immutable | prefetched into host backing | cheap, cached |
| > 8 KiB immutable | `SnapshotReader`, uncached at every layer | **pays on every read** |

So `ls -R`, `find`, `git status` live in the cached band. `grep -r` over large files, and **`execve` of any binary above 8 KiB**, live in the uncached band and re-pay per invocation.

**Correction to something I posted earlier in this thread:** I said v0.1.6 had no prefetch and that the prefetch counters were dead code. The counters are dead (`small_file_prefetch_eligible`, `small_file_prefetch_bytes`, `anchor_prefetch_count`, `snapshot_store_wide_scans` each have exactly one reference — their own declaration), **but the prefetch itself is real** at `live_backing.rs:81`. I was wrong, and the 8 KiB threshold is the number that matters, not the telemetry fields.

## 2. Write path — where the accumulator actually is

```
  kernel  write(2)/create(2)/mkdir(2)/rename(2)/unlink(2)
     │        per-call, concurrent, UNORDERED
     ▼
┌──────────────────────────────────────────────────────────────────────────┐
│  LiveWorkspace  (crates/layerfs-workspace-core)                          │
│                                                                          │
│   write() → prepare_write(node, offset, bytes, spool_slice)              │
│              • validates bounds, bumps inode revision                    │
│              • mutates the piece tree          ← in-memory, fast         │
│              • bytes land in the HOST SPOOL    ← on disk, bounded        │
│                                                                          │
│   create/mkdir/unlink/rename → live tree namespace ops                   │
│                                                                          │
│   NO STORE TRAFFIC ON THIS PATH.                                         │
│   "DB traffic happens at four events: create · commit · merge · end"     │
└───────────────────────────────────┬──────────────────────────────────────┘
                                    │  accumulated, unordered
                                    ▼
                        ┌───────────────────────────┐
                        │  COMMIT POINT             │
                        └───────────┬───────────────┘
                                    ▼
                        ┌───────────────────────────────────────┐
                        │  CaptureState::Running                │
                        │   sender + thread (capture.rs)        │
                        │   construct_stream → chunk/hash       │
                        │   → CapturedFile{root, objects}       │
                        │   → DeferredObjectStore               │
                        └───────────┬───────────────────────────┘
                                    │ atomic publish
                        ┌───────────▼───────────────────────────┐
                        │  Store: canonical objects + packs     │
                        └───────────────────────────────────────┘
```

**This is the answer to pair 1's "where does the accumulator live".** The gap between FUSE's per-call unordered mutations and C1's demand for sorted, unique, **complete** per-directory bindings is closed by **three stacked mechanisms**, not one:

1. the live piece tree — in-memory edits, the mutable view;
2. the host spool — the bytes, on disk, bounded;
3. the capture thread — construction, deferred to the commit point.

## 3. The same operation in both systems

```
  "mount a workspace, then read a 1 MiB file that already exists"

  v0.1.6  ─────────────────────────────────────────────────────────────
     create_workspace_session                     9-11 ms   [MEASURED]
       branch pin + dir + FUSE attach + tree metadata read
     ─────────────────────────────────────────────────────────
     first read of that file                      per read  [from code]
       SnapshotReader → cache MISS (>8 KiB)
       → SQLite lookup → pack extract → authenticate
       → and AGAIN on every subsequent read

  core    ─────────────────────────────────────────────────────────────
     content   (construct the file)                1.585 s   [MEASURED]
       = 1.039 s construction + ~0.55 s predecessor reads
     filesystem (build + publish the tree)         5.146 s   [MEASURED]
     accept_loop (store the objects)               8.820 s   [MEASURED]
     ─────────────────────────────────────────────────────────
     read of that file                             ~0        [structural]
       already a canonical object, already stored
```

**The asymmetry in one line:** v0.1.6's mount is fast because it **reads a map a previous Commit already paid to build**; the core's construction is slow because it **is that Commit**. They are not the same operation, which is why the 10 ms vs 1–5 s comparison is not a speed comparison.

## 4. Why this is a crossover, not a winner

```
  total cost
     │
     │                                        ╱  core: eager
     │                                       ╱   pays once, up front
     │                                      ╱
     │                                     ╱
     │  v0.1.6: lazy                     ╱
     │  ┌───────────────────────────────╱──────────────────
     │  │ flat mount (9-11 ms)         ╱
     │  │ + rising per-read line      ╱
     │  │                            ╱
     │  │                           ╱
     │  └──────────────────────────╱───────────────────────
     │                          ╱
     │                        ╱
     └──────────────────────╳──────────────────────────────► reads
                          crossover
                    (unmeasured on both sides)

  left of crossover:  read a few files → v0.1.6 wins (never built the rest)
  right of crossover: read everything  → core wins   (built once, read free)
```

This lane's own workload sits **far right** — 904,143 path-states over 4,936,693,030 bytes of cumulative history, reading everything — which is why the core's eager cost is the correct bill for *that* workload rather than a defect.

## 5. What this implies for `#207` and pair 1

**A concrete first experiment, testable before any crossover curve exists:** measure **`execve` latency for a binary above 8 KiB, repeated**. It is the most common shell operation, it falls in the uncached band *by construction*, and it is the case where an eager model should win outright. If the core cannot beat a lazy loader there, it will not beat it anywhere.

**For pair 1's open design items**, these diagrams supply two of them:

- *"Overlay byte ceiling"* and *"flush policy"* — the reference's answer is the host spool with a high-water mark (`spool_high_water`, `spool_bytes_peak`) and construction deferred to commit. That is a working precedent to price, not a guess.
- *"Where the accumulator lives"* — three tiers, above. A second projection (materialization) would share tier 1 and 2 and not tier 3, which is the trade pair 1 has to decide.

**Unchanged bounds:** these are structural readings of reference source, not measurements of the core; nothing here claims a core-side number, and the `[MEASURED]` figures are the ones already receipted in this lane and in `issue154`. No v0.1.6 re-run was performed and none is proposed.
