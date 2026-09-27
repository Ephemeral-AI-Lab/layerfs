# Issue 261: 100 mounted-write result and open gate

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

A later [three-pattern 100-write diagnosis](THREE-PATTERN-DIAGNOSIS.md) compares
true append, dispersed and repeated edits of one 10 MiB old head. Its distinct
receipts and source identity do not replace the separated-offset result below.

The change starts at repair commit `6af2c5c59a48d0b6c85d656e55aecc353e346728`
in its own managed worktree. The primary target was an existing mounted file,
one shell-launched [generic writer](../../../benchmark/fs-bench-pro/writers/write-separated.c)
with one fd, 100 one-byte positional writes, one public SDK Exec and one
explicit public Commit. The complete [source and count analysis](DIAGNOSIS.md)
reconciles the independent source audit with the retained [v1](evidence/100-v1/),
[v2](evidence/100-v2-phase/) and [v3](evidence/100-v3-ledger/) receipts. Every
attempt and the [distinct 512 FAIL](evidence/512-v1-fail/) remains append-only.

At candidate source `0a3ec6d0f91db5b27f868a50805df2a8563b5d8c`, the
independent oracle passed: the old 8,194-byte head is unchanged, the new head
has exactly 100 separated `X` bytes, 100 changed runs, 200 file extents and
the expected parent. Actual FUSE Status recorded 100 WRITE callbacks from the
100 successful writer syscalls, with one LOOKUP, one OPEN and three GETATTRs.
The candidate removed one redundant authenticated 4 KiB ownership-ledger read
per completed page publication: **3,679 → 3,440 reads (−239)**; 1,680 ledger
writes, 700 metadata page reads, four Workspace upstream calls, and final
content counts were unchanged. Exec was **455.678 ms**, Commit **40.930 ms**,
the complete command **1.423 s** against its 15 s diagnostic budget, and the
independent verifier **13.240 ms**. The clone's ordinary cache state was not
controlled, so these are diagnostic/raw times and **not** a speed admission
PASS or a qualified before/after latency comparison.

The [4,097 public gate](evidence/gate-4097-not-run.json) is `NOT_RUN` on this
source. The 100-write case was the directed primary optimization target; the
narrow ledger correction does not establish that the remaining ownership
direct I/O will fit the 25 s complete-command exception. The separate 512
diagnostic returned `EBUSY` at callback 258 without a Commit. Its post-reply
snapshot ordering was subsequently corrected, but the exact refusing site
was not logged, and that failed arm was not repeated. The #249 whole-Exec
30 s product timer remains a separate open dependency. No threshold, worker
count, workload or cache rule was changed to turn either row into a PASS.

## Verification at the changed product source

| Check | Result | Retained evidence |
| --- | --- | --- |
| Locked Cargo release SDK driver, independent verifier, ARMv8 daemon and static writer builds | PASS; all binaries sealed by SHA-256 in prepared receipts | [v3 prepared identity](evidence/100-v3-ledger/prepared-public.json) |
| Public 100-write Exec/Commit and independent old/new-head oracle | Functional PASS, cleanup PASS, latency INELIGIBLE | [v3 receipt](evidence/100-v3-ledger/receipt.json), [verifier](evidence/100-v3-ledger/verifier.stdout) |
| Host release Clippy, `layerfs-workspace --lib -D warnings` | PASS, 4.275 s | [command](evidence/checks/clippy-host-workspace.json), [stderr](evidence/checks/clippy-host-workspace.stderr) |
| Linux aarch64 release Clippy, Workspace/FUSE/daemon libs | FAIL, 4.443 s: existing `useless_conversion` at `backing/segments.rs:192` | [command](evidence/checks/clippy-linux.json), [stderr](evidence/checks/clippy-linux.stderr) |
| Full Core `cargo fmt --check` | FAIL, 1.912 s: existing format difference at `runtime/state.rs:267` | [command](evidence/checks/fmt-core.valid.json), [output](evidence/checks/fmt-core.stdout) |
| rustfmt check on changed Rust files | PASS | [command](evidence/checks/fmt-touched.valid.json) |
| Product boundary guard and its self-tests | PASS; 316 files scanned and 9 tests | [guard](evidence/checks/boundary.valid.json), [self-tests](evidence/checks/boundary-tests.valid.json) |

The full Core test command was **NOT_RUN** because the user limits each
focused test command to under 30 s, and that command is an aggregate workspace
suite. Previously passing unaffected tests were not rerun. The public mounted
100-write route plus independent verifier is the behavior check for this
ownership change; the locked release build and targeted checks are listed
above. There is no repository CI or pre-push aggregate gate. The unchanged
format and Clippy findings are retained as gaps, not claimed as passing.
Four check-command metadata files accidentally ended with a literal `\n`
suffix. Their originals remain in `evidence/checks/`; the linked `.valid.json`
copies remove only that suffix and record the original SHA-256. Captured
stdout/stderr is unchanged.

The public run proves one old/new head and sequential write visibility. It
does not exercise concurrent G1/G2 writers, unknown-outcome recovery or exact
container anonymous/file-cache peaks. The shared helper keeps the same owner
record, checksum, direct write, quarantine and edge-completion order by source
inspection; those wider custody gates remain for #248. FLUSH/RELEASE callback
counts and cumulative extent-only page writes are not exported by this source.

## Production LOC by commit

Counter: `tools/production_loc.py --root <exact commit snapshot> --detail`,
reference `crates/` plus replacement `core/crates/*/src` and required runtime
inputs; comments, blanks, tests, examples, benchmark harness, tooling and docs
excluded. Every comparison is first parent → committed tree; reference stays
65,417 and there is no adapter production source in these commits.

| Commit | Change | Core production LOC | Combined production LOC |
| --- | --- | ---: | ---: |
| `0736d50c3` | Prospective 100/4,097 specification | 58,431 → 58,431 (0) | 123,848 → 123,848 (0) |
| `b094a4a1f` | Public diagnostic and FUSE snapshots | 58,431 → 58,467 (+36) | 123,848 → 123,884 (+36) |
| `dd7d8d1e6` | Distinct 512 specification | 58,467 → 58,467 (0) | 123,884 → 123,884 (0) |
| `725b4379a` | 512 harness selection/master reuse | 58,467 → 58,467 (0) | 123,884 → 123,884 (0) |
| `c7c83f5de` | Corrected 100 phase specification | 58,467 → 58,467 (0) | 123,884 → 123,884 (0) |
| `f23263b72` | Pre-reply phase snapshots | 58,467 → 58,499 (+32) | 123,884 → 123,916 (+32) |
| `56d05e532` | Ledger treatment declaration | 58,499 → 58,499 (0) | 123,916 → 123,916 (0) |
| `0a3ec6d0f` | Reuse verified ledger page | 58,499 → 58,509 (+10) | 123,916 → 123,926 (+10) |

The final evidence/documentation commit keeps both production totals unchanged;
its exact comparison is in that commit message. The combined growth is +78
production lines from the repair starting point: +68 operator diagnostics and
+10 ownership implementation, not a measured speedup claim.
