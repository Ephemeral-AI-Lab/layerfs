# Actual ordinary-runtime stream delivery: exploratory observations

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Written 2026-10-10 at `a8aa0b2b6` (product tree `9a76077239b4`, unchanged),
registration v2 release binaries, owned volume
`layerfs-r8-full-proof-20261010-2cd69f68db34`, pinned image, command identity
501:20. Exploratory: no timing claim, no admission. The judged tool
`core/benchmark/r8-tools/run_streams.py` runs once more at the final identity.

| Receipt | Outcome |
| --- | --- |
| [014](014-streams-exploratory/) | Tool FAIL from two harness defects: the delivered-count pattern did not match the runtime's row, and the descendant case expected the late line and then met a `Busy` Unmount because the descendant's working directory was in the mount. Every byte comparison of the size cases passed |
| [015](015-streams-exploratory-2/) | Tool FAIL from one harness defect: the nonzero-exit case looked for output files by name instead of the runtime's own command row |
| [016](016-streams-exploratory-3/) | PASS |

Observed in 016, each case one ordinary command through the real Engine:

- Both streams exact in length and SHA-256, and equal to the runtime's own
  delivered counts, at (stdout, stderr) sizes 0/0, 1/0, 0/1, 8191/8193,
  8192/8192, 8193/8191, 65535/65537, 1 MiB / 1 MiB + 1, and 64 MiB / 64 MiB
  interleaved. Blocks carry their index, so loss, repetition or reordering
  would change the digest.
- A command that writes 1 MiB to each stream and exits 7: both streams
  complete, typed status `exit_code: Some(7)`, reported as the original
  failure. Status and stream completion are distinct observations.
- A descendant that keeps stdout open and writes one line 3 s after the
  command exited 0: the command's 6 bytes and status are delivered; **the late
  line is not delivered and nothing reports that**. The Engine ends the
  attached stream when the command's own process exits. This is recorded and
  not judged: architecture 71 says end of stream establishes transport
  completion only, while the proof row asks for complete delivery or an
  explicit failure. In 014 the same descendant, with its working directory in
  the mount, made the following normal Unmount answer `Busy`: command exit did
  not release the filesystem owner.

Not constructible through the R7 runtime protocol and therefore not observed:
a stalled or failing sink (backpressure), a delayed consumer after a fast
exit, and a connection cut between frames.

The raw output files larger than 65,537 bytes were removed from receipts 014
to 016 before commit; each one's length and SHA-256, taken from the file, is
in [017-removed-stream-bytes.txt](017-removed-stream-bytes.txt). From 016's
tool version onward the controller drops a verified large stream itself and
keeps a differing one.
