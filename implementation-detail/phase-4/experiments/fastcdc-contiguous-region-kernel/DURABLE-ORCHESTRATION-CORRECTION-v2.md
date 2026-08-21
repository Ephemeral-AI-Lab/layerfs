# Durable orchestration correction v2

Frozen after the screen passed and before any durable measured row.

The first `durable-v1` orchestration attempted to execute the sealed
Canonical-v2 operand in place. Its bytes are correct but its sealed mode is
`0444`, so process creation returned `PermissionError` before preparation and
before any measured row. `durable-v1` contains no `DURABLE-RAW-v1.jsonl` and is
preserved unchanged.

The only correction is:

- copy the exact sealed control bytes into the v2 operand directory;
- verify the copied SHA-256 remains
  `f3dd4c9420cc7bb7e7390960db9bf6e4a4a44de3d15dc0573002d3172b570280`;
- set only the copy's runtime mode to `0555`;
- run the unchanged schedule, binaries, fixture, timers, gates, and analyzer
  in the fresh `results-v1/durable-v2` namespace.

Corrected runner SHA-256:
`a57137df5cfabf36a3f57612765ba0c6a693dac6ba585b1a58e74e48509336dc`.

No candidate rebuild, benchmark row, schedule change, semantic change, or
threshold change occurred.
