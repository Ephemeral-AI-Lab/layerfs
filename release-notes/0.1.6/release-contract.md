# LayerFS 0.1.6 release contract

> **Status:** LayerFS 0.1.6 release record.

This is a source-only Developer Preview. The owner directed the v0.1.6 closure and
the preparation of this release; [acceptance](acceptance.md) records what was
accepted, [verification](verification.md) records what was measured, and
[artifacts](artifacts.md) records what is published. Nothing here is a production
storage promise.

## What v0.1.6 is

v0.1.6 moves the **live mutable state of a Workspace into the sandbox**. The
container-side owner holds the mutable namespace, the file data and a private
packed payload backing created for that mount; the host retains canonical
construction, the SQLite Store and publication, and learns the workspace's
mutable state only through Commit-time transfer. There is no pause or quiesce step
in the Commit path, and canonical construction runs with one construction worker.
[The changelog](../../docs/releases/v0.1.6/CHANGELOG.md) lists every change; the
[manual](../../docs/versioned/0.1.6/README.md) is the versioned product contract.

## Compatibility boundary

| surface | v0.1.6 boundary |
| --- | --- |
| Store format | **unchanged**: `SCHEMA_VERSION` stays 10, no schema or static SQL change landed since v0.1.5, and every benchmark receipt of this release observed schema 10 |
| Canonical identity | **unchanged**: `layerfs-content` and `layerfs-layerstack-store` are byte-identical to the v0.1.5 release, so ObjectId domains, chunking, content roots and Commit derivation are untouched |
| Public CLI | **unchanged**: no command, flag or output schema changed; `layerfs --version` prints `layerfs 0.1.6` |
| Public SDK | **unchanged**: the only additions to `Client` are two methods behind the `test-instrumentation` feature; default builds see the v0.1.5 surface |
| Daemon protocol | **additive**: the mount request carries the workspace's snapshot backing root, and the daemon creates and removes that private directory per mount. A stale directory from a crashed mount is refused, not reused. |
| Live sessions | **incompatible across versions**: a v0.1.6 sandbox owner and a v0.1.5 host do not share a mutable workspace. Match SDK, CLI owner, daemon and runtime components. |

Supported schema-6/7/8/9 Stores keep connecting without promotion, exactly as the
v0.1.5 boundary stated; schema-5 and other unsupported versions are rejected
without mutation. There is no in-place promotion, no downgrade and no
retained-history transfer command. Explicit compaction stays removed by the
2026-09-11 owner decision, and previously compacted Stores remain readable through
the retained authenticated LFCNT1 read path.

## Known costs carried by this contract

* **One construction worker** is mandatory, so construction-heavy work is slower
  than v0.1.5: six material regressions (dedup/CDC construction, 1.50–1.65×) are
  recorded and owner-accepted with their diagnosis, and the cold
  `namespace-100000` Init miss (4.99 s, 1.13×) is owner-waived.
* **Bounded sandbox memory** replaces page-cache residency: the sandbox spool keeps
  a bounded resident window (≤ 2.6 MiB in the B2 control), and the Commit-time
  transfer pays a storage read (~2.1 GiB/s) instead of a cache-served one
  (~19 GB/s).
* Kernel-dirty shared `mmap` is still not captured by a Commit; the boundary is
  isolated to exact byte level and no registered selection is affected.

## Not promised

Crash or power-loss durability, mixed-version live sessions, sandbox *process*
memory numbers (the frozen harness emits container-scoped numbers only), and any
universal throughput or storage guarantee. Benchmark values are single-sample
campaign evidence with declared cache stance and recorded host load.
