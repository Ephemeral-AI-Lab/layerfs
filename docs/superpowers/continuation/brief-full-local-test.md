# Final gate — local environment + full test (AFTER ALL P TASKS)

Runs only after every P task has landed (including P4/P5,
excluding P3 push/pull which stays blocked by R7 #42, and
excluding P15 publication which needs its own approval).

## Environment

- Set up the local test environment in this sandbox from the
  repo's own instructions (no Docker here, so the sealed
  Linux E2E evidence still comes only from the user's
  Windows T1–T4 run — say that, do not fake it).
- Record the exact environment (toolchain versions, node
  version, umask, /dev/fuse absence) in the gate report.

## Test scope

- Full-workspace Rust tests in segments (a single full run
  exceeds 1500 s here; segment by crate, keep every pass
  line, and report per-segment counts — never claim a full
  run that did not complete).
- Full JS suites (area-p, calls, cli, packaging, init
  watchers, runtime surface/worker-frames, area-p9 events).
- Contract-consistency checks: the P6 cases against every
  client surface (Node, Python, CLI, MCP) — the cross-client
  evidence matrix from the 2026-10-28 design doc §1. Every
  row of the matrix must be green, blocked (with a reason),
  or explicitly deferred — never simulated.
- fmt, workspace clippy (excl. node, -D warnings), product
  boundary, tool self-tests.
- A simplification pass over the whole Area P diff (the
  design doc's final gate requires one): remove accidental
  complexity introduced slice by slice, with tests re-run
  after.

## Report

The gate report states: environment, per-segment counts,
the cross-client matrix, the simplification changes (if any),
and every honest NOT RUN item with its reason. The gate
passes only when the matrix has no unexplained gaps.
