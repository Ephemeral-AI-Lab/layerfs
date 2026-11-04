# fuser patches

[Architecture](../../README.md) › [Engine](../README.md) › [Kernel routes](README.md) › fuser patches

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The three authorized patches to fuser 0.18.0.

| | |
| --- | --- |
| Component | [Kernel routes](README.md) |
| Source | [`engine/patches/fuser-0.18.0`](../../../../engine/patches/fuser-0.18.0) |

The FUSE route builds against fuser exactly `=0.18.0`
([`Cargo.toml`](../../../../engine/crates/layerfs-fuse/Cargo.toml)), taken
from a checked-in copy of the crates.io archive at
[`engine/vendor/fuser-0.18.0`](../../../../engine/vendor/fuser-0.18.0). The
root manifest redirects it with one `[patch.crates-io]` entry
([`Cargo.toml`](../../../../Cargo.toml)). The owner authorized exactly three
changes to that copy; no other dependency is patched. The binding rule is in
the [engine guide](../../../../engine/AGENTS.md#fuser-provenance-and-authorized-patch-checks).

```text
  fuser-0.18.0.crate  (crates.io, sha256 b82b6597...cfd, 85 files)
        |
        |  1. signed-timestamps.patch   src/time.rs
        v
        |  2. session-lifecycle.patch   src/lib.rs, src/session.rs,
        |                               + src/session/lifecycle.rs,
        |                               CHANGELOG.md          (86 files)
        v
        |  3. receive-buffer.patch      src/read_buf.rs, src/session.rs,
        |                               src/session/lifecycle.rs
        v
  engine/vendor/fuser-0.18.0   <-- [patch.crates-io] in root Cargo.toml
```

| Patch | Authorized | Record | Diff | Provenance |
| --- | --- | --- | --- | --- |
| Signed timestamps | 2026-10-06 | [FUSER-REGISTRY-PATCH](../../../../core/docs/issues/307/FUSER-REGISTRY-PATCH-20261006.md) | [`signed-timestamps.patch`](../../../../engine/patches/fuser-0.18.0/signed-timestamps.patch) | [`provenance.json`](../../../../engine/patches/fuser-0.18.0/provenance.json) |
| Session lifecycle | 2026-10-08 | [FUSER-LIFECYCLE-DECISION](../../../../core/docs/issues/307/FUSER-LIFECYCLE-DECISION-20261008.md) | [`session-lifecycle.patch`](../../../../engine/patches/fuser-0.18.0/session-lifecycle.patch) | [`lifecycle-provenance.json`](../../../../engine/patches/fuser-0.18.0/lifecycle-provenance.json) |
| Receive buffer | 2026-10-14 | [FUSER-RECEIVE-BUFFER-DECISION](../../../../core/docs/issues/321/FUSER-RECEIVE-BUFFER-DECISION-20261014.md) | [`receive-buffer.patch`](../../../../engine/patches/fuser-0.18.0/receive-buffer.patch) | [`receive-buffer-provenance.json`](../../../../engine/patches/fuser-0.18.0/receive-buffer-provenance.json) |

## Signed timestamps

```text
  timespec (secs, nsecs)          meaning: secs + nsecs / 1e9
  ---------------------------------------------------------------
  (-1, 500_000_000)    unpatched: EPOCH - 1.5 s   (wrong)
                       patched:   EPOCH - 0.5 s
  (-2, 0)              patched:   EPOCH - 2 s
  secs < 0, nsecs > 0  patched:   EPOCH - (|secs| - 1, 1e9 - nsecs)
```

| Change | Where | Source |
| --- | --- | --- |
| `system_time_from_time` counts nanoseconds forward from a negative whole second, as a `timespec` does. | `src/time.rs` | [`signed-timestamps.patch`](../../../../engine/patches/fuser-0.18.0/signed-timestamps.patch) |
| Nanoseconds are clamped to `999_999_999` before use. | `src/time.rs` | [`signed-timestamps.patch`](../../../../engine/patches/fuser-0.18.0/signed-timestamps.patch) |
| Regression tests: negative conversions and a round trip from `i64::MIN` to `i64::MAX`. | `src/time.rs` tests | [`signed-timestamps.patch`](../../../../engine/patches/fuser-0.18.0/signed-timestamps.patch) |

The correction and its tests come from upstream fuser commit
[`e48279f`](https://github.com/cberner/fuser/commit/e48279fab0ddbe4a6e22aefad3cbfdf796d0b0b7)
([README](../../../../engine/patches/fuser-0.18.0/README.md)). LayerFS stores
signed modification times; the adapter's own `timestamp` uses the same rule
([`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs)).

## Session lifecycle

```text
 Session::into_runner()  -> SessionRunner      (consumed by run, once)
                         -> SessionMonitor     (observation handle)

 SessionPhase:  Prepared --> Starting --> Serving --> Stopping --> Joined
                                 |                       ^
                                 +--- startup failure ---+

 each receive loop: allocate buffer -> rendezvous -> receive/dispatch
 Serving  = every configured loop created AND entered
 Stopping = startup failed or any loop exited (Serving revoked)
 Joined   = every created loop joined, then filesystem cleanup ran

 SessionRunner::run() -> SessionOutcome {
     startup_error, receivers[i]: ReceiverOutcome{index, result},
     cleanup }            is_clean(): all three succeeded
```

| API | Added by the patch | Used by LayerFS | Source |
| --- | --- | --- | --- |
| `Session::into_runner` | Builds a runner without starting threads. `Session::run` is unchanged. | `KernelSession::attach` | [`startup.rs`](../../../../engine/crates/layerfs-fuse/src/session/startup.rs) |
| `SessionMonitor::wait_ready` | Waits for Serving or a terminal startup/exit; a deadline only returns the current snapshot. | Attach `Serving` phase | [`startup.rs`](../../../../engine/crates/layerfs-fuse/src/session/startup.rs) |
| `SessionMonitor::wait_for_change` | Waits for a new revision or the deadline. | Fence watcher; drain `Join` wait | [`startup.rs`](../../../../engine/crates/layerfs-fuse/src/session/startup.rs), [`drain.rs`](../../../../engine/crates/layerfs-fuse/src/session/drain.rs) |
| `SessionSnapshot` | `phase`, `revision`, `configured`, `created`, `entered`, `exited`, `joined` | `SessionFacts::serving` | [`state.rs`](../../../../engine/crates/layerfs-fuse/src/session/state.rs) |
| `SessionOutcome` | Original startup error, every receiver's join result (panic payload kept), cleanup result | `Drained::outcome`, `Drained::clean` | [`state.rs`](../../../../engine/crates/layerfs-fuse/src/session/state.rs) |

The unpatched API could not report that every loop started, and lost joins
on its early error paths
([README](../../../../engine/patches/fuser-0.18.0/README.md)). No observer
deadline cancels, detaches, retries or disposes the runner.

## Receive buffer

```text
                     unpatched                 patched
 handshake buffer    MAX_WRITE_SIZE + 4096     MAX_WRITE_SIZE + 4096
 each receive loop   MAX_WRITE_SIZE + 4096     max_write + 4096
                     (16 MiB + 4 KiB)          (set at INIT)

 LayerFS max_write = READ_WINDOW = 128 KiB  (mount/profile.rs)
```

| Change | Where | Source |
| --- | --- | --- |
| `HEADROOM = 4096`; `negotiated_size(max_write) = max_write + HEADROOM`; `FuseReadBuf::with_size` | `src/read_buf.rs` | [`receive-buffer.patch`](../../../../engine/patches/fuser-0.18.0/receive-buffer.patch) |
| `Session::receive_buffer` starts at `BUFFER_SIZE` and is set from `config.max_write` when INIT is answered | `src/session.rs` | [`receive-buffer.patch`](../../../../engine/patches/fuser-0.18.0/receive-buffer.patch) |
| Each event loop allocates `receive_buffer` bytes, also through the lifecycle runner | `src/session.rs`, `src/session/lifecycle.rs` | [`receive-buffer.patch`](../../../../engine/patches/fuser-0.18.0/receive-buffer.patch) |

The kernel never sends a write larger than the negotiated `max_write`, and
every other request is smaller, so the smaller buffer holds every request.
The memory reason and its measurements are in the
[decision record](../../../../core/docs/issues/321/FUSER-RECEIVE-BUFFER-DECISION-20261014.md).

## Provenance checks

```text
 python3 -B tools/check_fuser_integrity.py
   |
   +-- every tracked Cargo.toml, Cargo.lock, .cargo config:
   |     only the root manifest redirects fuser, to
   |     engine/vendor/fuser-0.18.0; only its locks hold the
   |     patched entry
   +-- each provenance record and patch file == its pinned sha256
   +-- every vendor file == archive hash, except the pinned
   |   changed/added files of the three records (86 files)
   +-- no other changed, missing, redirected or added file
```

| Pin | Value | Source |
| --- | --- | --- |
| Archive SHA-256 | `b82b6597…cfd` | [`provenance.json`](../../../../engine/patches/fuser-0.18.0/provenance.json) |
| Timestamp diff, patched `time.rs` | `854e35c1…4af5`, `a333dba1…287d` | [`provenance.json`](../../../../engine/patches/fuser-0.18.0/provenance.json) |
| Lifecycle diff and its four files | `926f4d13…6af5` | [`lifecycle-provenance.json`](../../../../engine/patches/fuser-0.18.0/lifecycle-provenance.json) |
| Receive-buffer diff and its three files | `7af34aee…303f` | [`receive-buffer-provenance.json`](../../../../engine/patches/fuser-0.18.0/receive-buffer-provenance.json) |
| Checker constants | the same hashes, plus each provenance record's own hash | [`check_fuser_integrity.py`](../../../../tools/check_fuser_integrity.py) |

The check reads files and runs no build. For an installed registry copy it
also takes `--archive`, `--package-directory` and `--locked-checksum`. Run it
before a native fuser build. Rebuilding the vendor copy is: extract the
archive, then apply the three patches in the order above with
`patch --batch --forward -p1`. Third-party source, including the patched
files, is outside first-party production LOC.

## Related

- [FUSE](fuse.md): the session that uses `SessionRunner` and the negotiated
  `max_write`.
- [Kernel routes](README.md#limits-and-failures): receive-buffer limits.
- [`engine/patches/fuser-0.18.0/README.md`](../../../../engine/patches/fuser-0.18.0/README.md):
  the patch record.
- [Glossary](../../glossary.md).

---

← [Kernel routes: FUSE](fuse.md) · [Up: Kernel routes](README.md) · [Kernel routes: NFS](nfs.md) →
