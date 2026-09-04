# Explicit empty-file edit-generation reclamation

The required real-FUSE sustained proof actually failed on source 34224330 at
`attempts/workspace-sustained-600s-proof-s1-verify-c3db3ad3ff04`. It was not a
timeout or OOM, and supervisor cleanup passed. Decoded original output reports
completed-cycle progress 191,391,600,810,994,1195,1417,1630,1847, followed by
worker 0 EINVAL and worker 1 prompt peer-disconnect. The repaired channel error
path preserved the original error instead of waiting for the 900-second guard.
`required-proof.json` binds that original attempt's immutable manifest; its
original files remain unchanged. The decoded text is explicitly attributed to
its retained SDK OutputPage byte arrays.

The shared Workspace edit counter remained charged after the complete logical
file had been deleted by truncate. The live pieces and inline allocation were
already empty: this was stale logical edit-generation accounting, not evidence
of leaked PieceNodes. Physical append-only spool bytes remain retained for
in-flight read safety and remain subject to their unchanged separate budget.

One focused public Workspaces API regression establishes the exact boundary:
initial 64-byte insertion followed by 2048 delete-all/fill pairs fails on the
4097th requested mutation, after 4096 successful mutations, with live_len=0 and
Storage(InvalidInput("workspace edit limit")). The initial run failed once:
0 passed,1 failed,11 filtered,4.15 s test time; its output and pre-run source hashes
are preserved. The integration route uses public singular file-range edits and
real Materialize presentation refresh for the same logical state transitions;
it is not represented as another real-FUSE endurance measurement.

The product change is only in `Workspace::install_edit` in file_io.rs. A
successful old.len()>0 to next.len()==0 installation retires the old logical
edit generation and stores edits=0. Existing admission checks run first.
Nonempty rewrites, partial deletion, no-op empty edits, all constants, 4096 and
other limits, physical spool history, public write fast-path bodies, and struct
layouts remain unchanged. No counter resets merely because a nonempty rewrite
has few pieces. The exact patch is retained in `product.patch`.

The identical regression passed once after the fix: 1 passed,0 failed,11 filtered,
8.03 s test time. It completes 2049 file generations, then exercises exactly 4096
edits within the final nonempty generation and rejects the next edit with the
unchanged exact error, verifying bytes before/after rejection. Finally it
requires Created, Clean End, reopened exact bytes and cleanup. Before/after
corrected-source hashes match. The existing native nonempty edit-limit test
remains unmodified; no full suite or additional passing family was rerun.

This qualification does not establish a 600-second sustained pass. The required
proof must be rerun on the new sealed product after source/build qualification,
with its original duration, filesystem work, oracle and resource gates intact.
Keep the actual failed proof and this failed diagnostic in the evidence record.
