# Count round trips above every threshold

> **Status:** Lesson; distilled from merged work, informative and not a product contract.
>
> **Category:** measurement · **Applies when:** measuring or reviewing the cost of a Store call (Commit, Init, `fs.*` reads, history) on PostgreSQL · **From:** [#810](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810), [#816](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816), [#818](https://github.com/Ephemeral-AI-Lab/layerfs/issues/818)

## Rule

Count round trips, not only time, at sizes above every cache and threshold on the
path, and add latency inside the database container to see what a remote Store costs.

## In short

- On local PostgreSQL a round trip costs 0.065 ms (container loopback) to 0.34 ms
  (Docker's port forwarder), both measured. A per-item loop looks like a little CPU
  until the Store is 10 ms away.
- A loop can stay hidden below a cache's capacity or a size threshold. A size that
  fits under all of them measures a different code path from the one users hit.
- A latency arm turns trips into time. If the added time is not about trips × delay,
  the counter is missing something.

## What happened

**#818: Init at 1k files looked fine; 10k files did not.** Same source
(`056e920166`), one sample per case, counts from the Store's own `SqlWork.round_trips`
(measured; [round-trip table](https://github.com/Ephemeral-AI-Lab/layerfs/issues/818#issuecomment-7203068532)):

| `project.init` case | Round trips | Hidden step it crossed |
| --- | ---: | --- |
| 1k files in 10 directories | 286 | none |
| 1k files in one directory | 2,246 | a directory of 512+ children is "wide" |
| 10k files in 100 directories | 10,953 | the 4,096-entry locator cache overflows |
| 10k files in one directory | 31,041 | both |

Ten times the files cost 38 times the trips. Two per-file loops did it, and neither
shows at 1k files in small directories:

- **Wide directories.** `scan.rs` (`engine/crates/layerfs-project/src/import/scan.rs`)
  routes a directory with `WINDOW_ROWS` (512) or more children to `place_wide`. Its
  PostgreSQL `place_children` (`backend/postgresql/acquisition/writes.rs` in
  `layerfs-persistence`) sends one insert and one update per child. The server showed
  exactly 1,000 of each at 1k flat and 10,000 at 10k flat.
- **Logical size.** Init's `logical_size`
  (`core/crates/layerfs-content/src/filesystem/statistics.rs`) opens each file's root
  with a single-object read. Past the 4,096-entry locator cache, as #818 traced it,
  that is one locate per file: 10,142 at 10k files, none at 1k.

The same comment found history calls scale per version: `fs.blame` 246 trips at 10
versions and 2,233 at 100, about 22 trips per version from the pair. The
[latency comment](https://github.com/Ephemeral-AI-Lab/layerfs/issues/818#issuecomment-7203237505)
then timed the calls with 10 ms added: Init of 1k files in one directory went from
1,367 ms to 27,041 ms, about 11.4 ms per trip, against a calibrated `SELECT 1` of
11.0–11.9 ms. Server execution time barely registered; the trips were the cost.

**Commit work (#810, #816): time on loopback hid a 2.7× trip cut.** In
`bench-262ef260be` (measured, one sample per arm, cold, PostgreSQL on loopback inside
its own container), B1 (npm install, 10,002 files) took 1,731 ms at 729 trips on v2
`7e6caa86b2` and 1,725 ms at 270 trips on `262ef260be`. Time said nothing changed.
With +10.6 ms added the same arms took 8,426 ms and 3,924 ms. Caches hid trips too: a
warm Workspace counted 52 trips for B5e where a cold one counted 90, and the pooled
value cache (`POOLED_VALUE_CACHE_BYTES`, 512 KiB) overflows at about 1,000 changed
files, so larger changes reread pages in every phase. See
[state the cache behind a cap](state-the-cache-behind-a-cap.md) and
[round trips follow depth](../store-access/round-trips-follow-depth.md).

## How to spot a weak measurement

- Only time is reported, or only on local PostgreSQL. Local time is mostly client CPU.
- Every case fits under the caches and windows on the path: one size, or sizes like
  1k files in directories of 100.
- Trips do not scale linearly between two sizes. A jump means a threshold was crossed.
- Counts with no route, cache state or role. `fs.blame` at 100 versions counted 2,233
  trips through `StoreService::open_postgres` and 2,325 through `open_store_observed`
  (`engine/crates/layerfs-store-service/src/bootstrap.rs`), split across processes.
- `pg_stat_statements` totals used as round trips. A pipelined flight is many
  statements in one trip. With `pg_stat_statements.track = all`, nested foreign-key
  trigger checks count too: about 30k at Init 10k in #818.
- A latency arm whose added time is far from trips × delay, or a delay that was never
  calibrated, or one added on the host network.

## What to do

1. **List the caches and thresholds on the path**, with their sizes: locator caches,
   value caches, window rows and bytes, wide-directory limits. Pick at least two sizes
   above every one of them, and one shape per threshold (flat versus nested, few versus
   many versions). Two sizes give a per-unit slope; a slope that changes is a finding.
2. **Count trips by role and by phase.** `SqlWork.round_trips`
   (`engine/crates/layerfs-persistence/src/backend/sqlite/session/connection.rs`)
   counts each exchange that waited for a reply. Read the writer, the control reader
   (session 0) and the Workspace readers apart, as `mount_round_trips.rs` does in its
   `work` helper. Attribute Commit trips to phases with `telemetry::attribute_sql` and
   `telemetry::phase_sql` (`engine/crates/layerfs-store-service/src/telemetry.rs`);
   the phase names are in `store/commit.rs`.
3. **Cross-check on the server.** Take `pg_stat_statements` deltas per query id,
   counting only `toplevel` calls. The in-repo snapshots (`Server::snapshot` in
   `postgresql_round_trips.rs` and `postgresql_phase_attribution.rs`) read `calls`
   with no `toplevel` filter, so check the setting before trusting their totals.
4. **Calibrate one trip** before and during the arm: the median of many `SELECT 1`
   probes. #818 measured 0.34 ms through Docker Desktop's port forwarder; the Commit
   bench measured 0.065 ms on the container's own loopback.
5. **Add latency inside the PostgreSQL container's network namespace only**, never on
   the host network (owner rule; #815 acceptance). The Commit bench ran PostgreSQL with
   `--network none`, the client with `--network container:<pg>`, and a sidecar with
   `NET_ADMIN` in the same namespace that delays only PostgreSQL's replies:

   ```text
   tc qdisc add dev lo root handle 816: prio bands 3 priomap 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
   tc qdisc add dev lo parent 816:3 handle 817: netem delay 8200us limit 10000
   tc filter add dev lo protocol ip parent 816: prio 1 u32 \
       match ip protocol 6 0xff match ip sport 5432 0xffff flowid 816:3
   ...measured call...
   tc qdisc del dev lo root handle 816:      # then confirm `qdisc noqueue` again
   ```

   Check `tc -s qdisc show dev lo` before (noqueue), after arming (netem packets and
   filter hits above zero, no drops) and after removal. The dial is not the delay: 8.2
   ms measured 10.70 ms against 0.065 ms unshaped, so the arm is +10.6 ms. In the
   repository, `storage_gate_bench netem add|del|show`
   (`engine/crates/layerfs-persistence/examples/storage_gate_bench/monitor.rs`,
   `netem`) adds a fixed 1 ms on the server container's `eth0`; #818 did the same with
   10 ms on `eth0`.
6. **Check that added time is explained by trips**: time(+d) − time(0) ≈ trips × d.
   B1 on `262ef260be` added 2,199 ms for 270 trips at +10.6 ms (at most 2,862 ms). #815
   wrote its acceptance as time(+10 ms) − time(0 ms) ≤ 15 × trips × 10 ms, which only
   catches gross misses; compare against 1× as well. Per-trip cost well above d means
   waits the counter misses: connection handshakes are counted in `connects`, not
   `round_trips`, and #818 noted its write counts might miss a separate lane
   connection. Below d can mean sessions waited in parallel.
7. **Record** the source, build profile, route used to open the Store, sizes and
   shapes, cache state, calibrated trip, delay dial and measured delay, and trips by
   role and phase beside every time.

Practical traps from #818: the `main` Branch refuses file changes (`ReadOnly`, see
[Store calls](../../architecture/engine/store-calls/README.md)), so build history on a
forked Branch. macOS has no `timeout`; bound a command with
`perl -e 'alarm 120; exec @ARGV' -- <command>`.

## Limits

- One sample per case and arm. The calibration probe is an instrument check, not a
  second sample of the call. A slope from two sizes is two cases, not a resample. See
  the [measurement workflow §3](../../general/agent-measurement-policy.md#3-running-a-measurement).
- Every test command stops at 120 s. #818 projected Init 10k at +10 ms (≈139 s and
  ≈379 s) as local time + trips × 11.43 ms instead of running it; label such numbers
  as projections.
- Counts are comparable only under the same cache state and counting method; that
  lesson is [state the cache behind a cap](state-the-cache-behind-a-cap.md).
- Fewer trips is not the goal by itself. The fix patterns and their traps are in
  [round trips follow depth](../store-access/round-trips-follow-depth.md) and
  [costly patterns](../store-access/costly-patterns.md).
- A latency arm models a fixed delay, not bandwidth limits, loss or jitter.

## Evidence

- #818 [round-trip table and causes](https://github.com/Ephemeral-AI-Lab/layerfs/issues/818#issuecomment-7203068532)
  and [SQLite / local / +10 ms timing](https://github.com/Ephemeral-AI-Lab/layerfs/issues/818#issuecomment-7203237505);
  the probe was an untracked test at `056e920166`.
- #815 [acceptance and latency arms](https://github.com/Ephemeral-AI-Lab/layerfs/issues/815);
  #816 [results at merge](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7202528418).
- Pinned count tests in `engine/crates/layerfs-store-service/tests/`:
  `postgresql_round_trips.rs`, `postgresql_phase_attribution.rs`, `mount_round_trips.rs`,
  `commit_batched_reads.rs`, `commit_batched_base_files.rs`,
  `commit_batched_directories.rs`, `commit_similar_tiny_files.rs`.
- Receipts on the owner's machine under `2026-11-26-commit-batching/`:
  `bench-262ef260be/` (local and +10.6 ms lanes, calibration) and
  `stage2-816-c4s4-profile-corrected-s4pcef4n/latency/` (shaper and controller).
