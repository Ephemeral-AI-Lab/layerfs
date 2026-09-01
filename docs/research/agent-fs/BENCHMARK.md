# AgentFS Benchmark

Date: 2026-09-01

## Environment

- Host: Apple Silicon macOS (`Darwin 25.4.0 arm64`)
- AgentFS checkout: `/Users/yifanxu/Ephemeral-AI-Lab/agentfs`
- Source commit: `0a014eb Make clippy happy (#337)`
- CLI package: `agentfs` `0.6.4`
- Build: Rust nightly, optimized release profile
- Database: fresh temporary local AgentFS database
- Container: none

The release binary was built with:

```bash
cargo +nightly build --release --manifest-path cli/Cargo.toml
```

## Results

| Test | Result |
| --- | ---: |
| `agentfs exec` mount, one write, one read, unmount | 0.33 s |
| `agentfs run` with macOS NFS + `sandbox-exec`, one write, one read, unmount | 0.28 s |
| 100 direct CLI writes | 2.10 s (~21 ms/write) |
| 100 direct CLI reads | 2.06 s (~21 ms/read) |
| 100 writes + 100 reads in one mounted session | 0.58 s |
| Sequential 16 MiB write through the mount | 95.7 MB/s |
| Sequential 16 MiB read through the mount | 387 MB/s |

The direct CLI tests launched a new process and reopened the database for every
operation. The mounted-session test kept one mount and one process alive for
all operations.

## Interpretation

AgentFS is fast enough for ordinary agent workloads. The dominant cost for
repeated `agentfs fs ...` commands is process startup plus opening and closing
the SQLite database, not the filesystem operation itself.

For many operations, use one of these approaches:

- Keep a mounted session alive.
- Use `agentfs exec` for a command that performs multiple operations.
- Use the SDK for in-process access.

The mount path is not intended to outperform a native local filesystem. The
16 MiB read result is likely assisted by the filesystem cache, so it should not
be treated as a cold-storage benchmark. These are single-run smoke measurements,
not a statistically rigorous performance study.

## Count-changing edits

The following tests used a fresh 32 MiB file and the macOS mounted path. “Inner”
is the timed operation inside the mounted command; “outer” includes command and
mount/session overhead.

| Operation | Inner | Outer | Notes |
| --- | ---: | ---: | --- |
| Positional overwrite, 10 bytes | 0.63 ms | 0.17 s | One affected 4 KiB chunk |
| Append, 10 bytes | 0.99 ms | 0.20 s | Cheap in-place extension |
| Truncate 32 MiB → 16 MiB | 29.4 ms | 0.32 s | Deletes the removed chunk rows |
| Truncate 32 MiB → 64 MiB | 6.6 ms | 0.43 s | Logical sparse growth; stored data stayed 32 MiB |
| Prepend, 10 bytes; 1 MiB buffered copy chunks | 2.17 s | 2.75 s | Application-level full rewrite, 32 large writes |
| Prepend, 10 bytes; one large write | 0.38 s | 0.61 s | Same rewrite, but one filesystem write |
| Middle insertion, 10 bytes; one large write | 0.379 s | 0.59 s | Also an application-level full rewrite |

AgentFS has no native insert/splice operation. Prepend and middle insertion are
therefore implemented by the caller as “write the new contents elsewhere, then
rename”; the cost is proportional to the file size. A positional overwrite does
not have that cost.

The important correction is that the original streamed-prepend result was not a
fair measure of the rewrite algorithm alone. Batching the same rewrite into one
large write reduced the inner time from 2.17 s to 0.38 s. The remaining cost is
the rewrite plus AgentFS persistence, while the streamed version adds one
AgentFS/SQLite transaction per write request.

## Where the overhead comes from

On macOS, AgentFS presents the filesystem through a userspace NFS server over
loopback (`127.0.0.1`); this is not remote network latency. The NFS protocol and
mount machinery still add overhead, especially for tiny operations, but they are
not the primary cause of large count-changing-edit slowdowns.

The dominant path is:

1. There is no splice primitive, so the application rewrites the affected tail
   or the whole file.
2. File data is stored in fixed 4 KiB chunks in SQLite (`fs_data`).
3. Each NFS write is routed to `AgentFSFile::pwrite`.
4. `pwrite` starts and commits its own SQLite transaction.

Consequently, the streamed prepend performs roughly 32 separately committed
1 MiB writes. A single large write performs the same logical rewrite with one
transaction. For small one-off edits, the roughly 0.17–0.20 s outer time is
mostly mount/process/protocol overhead; for large rewrites, data movement and
transaction granularity dominate.

For a cold `npm install`, registry/network download time is a separate cost. The
filesystem-side result above is specifically about local persistence and does
not include package download time.

## Commands exercised

Basic filesystem operations:

```bash
agentfs init bench
agentfs fs bench write hello.txt "hello"
agentfs fs bench cat hello.txt
agentfs fs bench ls
```

macOS mount-backed execution:

```bash
agentfs exec bench zsh -lc 'printf "mounted-ok" > mounted.txt && cat mounted.txt'
```

macOS sandboxed copy-on-write execution:

```bash
agentfs run \
  --session codex-speed-check \
  --no-default-allows \
  zsh -lc 'printf "sandbox-ok" > sandbox.txt && cat sandbox.txt'
```

The sandbox test left its resumable delta database under
`~/.agentfs/run/codex-speed-check`. No AgentFS mount remained active afterward,
and the original temporary project directory was unchanged.
