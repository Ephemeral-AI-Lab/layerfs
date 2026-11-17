# home family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `home` calls: set a home up on purpose, and read what this build would
use with it without changing anything, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/home`](../../../../api/crates/layerfs-api/src/home), [`layerfs-control/src/home.rs`](../../../../control/crates/layerfs-control/src/home.rs) |
| Runs by default | the caller's process. Neither call is opened by the dispatcher (`Call.opens` is false): `init` opens the home itself, `get` never does |
| Design | [Home](../../control/home/README.md), [Migration](../../control/home/migration.md) |
| Clients | `lfs.home` is this family (`lfs.home.get()`, `layerfs.home.init()`); a client's home is the one given to `client()` and is no field of it |

## Calls

```text
 home.get   reads layerfs.json, the folder's mode, bundle.json,
            native's record and the daemon file          -> changes nothing
 home.init  migrate::open (create | convert | finish)
            + the built-in machine records                -> {created, converted}
```

| Call | Returns |
| --- | --- |
| [`home.init`](#homeinit) | `{ home, created, converted, layout }` |
| [`home.get`](#homeget) | `{ home, bundle, daemon, cache, warnings }` |

### home.init

Makes the home ready for calls, as the first call of any other kind does: an
empty or missing folder is created (0700, `layerfs.json`, `machine/native/`
and `machine/docker/` with the `machine` feature), a 1.1.0 or layout 2 home
is converted, and an interrupted conversion to layout 3 is finished. A current
home is left as it is (`created` and `converted` both false). It writes one
call-log line.

```text
JS      await layerfs.home.init()
Python  layerfs.home.init()
CLI     layerfs home init
```

Refused `InvalidInput` inside a machine's worker root (not a home), `InUse`
while a layout 2 home has live Sandboxes (its conversion lists them), and as
opening a home is refused otherwise (a newer layout is `Unsupported`).

### home.get

What the home is and what this build would use with it. It reads files only
(no process, Docker, network, lock or call-log line), so a client can poll it,
and it answers for a home that does not exist yet, creating nothing.

```text
JS      await layerfs.home.get()
Python  layerfs.home.get()
CLI     layerfs home get [--text]
```

| Field | Meaning |
| --- | --- |
| `home` | `{ path, state, layout, mode, mode_ok }`. `state` is `missing` (no `layerfs.json` and no 1.1.0 record), `current`, `needs_conversion` (1.1.0, layout 2, or an unfinished conversion) or `unknown` (a `layerfs.json` this build cannot read, or a newer layout). `mode_ok` is false when group or others may read or write it; others' execute is allowed (namespaces Sandboxes add it) |
| `bundle` | The Linux bundle this build would deploy: `{ dir, source, present, commit, dirty, matches }`; `source` is `env:LAYERFS_BUNDLE` or `cache` (`<cache>/layerfs/bundle/<version>/linux-<arch>`); `commit` and `dirty` from `bundle.json` `source` |
| `daemon` | macOS: native's daemon, `{ path, source, present, commit, dirty, matches }`, `source` `recorded` (native's record) or `default` (beside the macOS bundle). Null elsewhere |
| `cache` | `{ path }`: `XDG_CACHE_HOME`, else `~/.cache` |
| `warnings` | `[{ code, message }]`, the next step in the message: `HomeMissing`, `HomeNeedsConversion`, `HomeUnknown`, `HomeMode`, `BundleMissing` (macOS), `BundleMismatch`, `DaemonMissing`, `DaemonMismatch` |

`matches` compares a part with the build making the call: false when the
commits differ, true when they are equal and both trees were clean, null when
either commit is unknown or a tree was dirty. The build's commit is the one
the build tools set (`LAYERFS_BUILD_COMMIT`, `tools/worktree.py`); a plain
`cargo build` names none, so its answers are null. The daemon's commit is the
text after `layerfs-build-identity:` in its file, stamped by its build script
the same way; `tools/linux_bundle.py` writes the bundle's `source.dirty`. A
mismatch is reported, not refused: a bundle from another commit with the same
worker protocol still starts, as before.
