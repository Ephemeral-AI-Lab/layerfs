# Compact spool focused qualification

This directory persists already-returned execution results. Creating these
artifacts did not rerun the test or the layout probe.

The focused test passed once: **1 passed, 0 failed, 37 filtered out**, with
Cargo reporting 6.20 seconds to build the test target and the test harness
reporting 0.06 seconds for the selected test. See `test-tool-transcript.json`
for the exact original tool output and `test-output.txt` for its readable copy.
The test covers 100,000 compact piece descriptors using 800,000 charged bytes,
arbitrary-edit promotion, range/snapshot behavior, truncation/empty state, and
unchanged range/result/zero/aggregate limit rejection. It is not an end-to-end
100,000-file Workspace workload or evidence of final Phase1 completion.

`layout.rs` is the actual retained temporary source used for the standalone
layout probe. Its output is copied from the original successful tool result
in `layout-output.txt` and `layout-result.json`. The observed model expands
PieceTree16→24, FileData104→112, Data104→112, and Node192→200 bytes, all at
alignment8. Wrapper IDs match their source32-byte representation; the Arc
pointee was replaced with unit because only pointer layout matters. The probe
is a source model, not an observation from a built product binary.

`source.json` records hashes **after** the test/probe execution. No pre-run
source seal exists. The owning agent made no change to file_edit.rs between
the passing test and this capture. `file_edit.tested-source.rs` preserves that
owned source. No claim is made about a historical dependency-wide source seal,
and the current HEAD hash is recording context rather than the tested binary's
complete build identity. Original failed campaign outcomes remain unchanged.
