# #241 Phase 4 v4 four-case result, 2026-09-25

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

## Decision

The child #241 implementation and its four-case functional admission are
complete. All four registered middle 4 KiB inserts completed public SDK Exec
and Commit, the independent full-file oracle passed, range route counters were
bounded, and Workspace/Sandbox cleanup succeeded. This closes the baseline
`SDK Exec Unknown` blocker for these cases. The 264/264 separate position sweep
at `8aaf623835cf1384fbf5558e868b8d1b945830bd` remains the functional
position proof.

**There is no G2 latency PASS.** The frozen Linux FUSE backing cache contract
classifies every timing `INELIGIBLE`, and all four raw Edit→Commit durations
exceed their aspirational targets. #232 retains its independent 56-case
eligible performance gate. Future latency work should use a new prospective
identity and a cause-specific diagnostic; these four arms must not be sampled
again to select a better number.

## Frozen identity and method

- Measured source commit `374e9636abb9ae10a041e0f71b1c07e84f2128f4`,
  tree `41a7518625205173875e3317260d35abb95b6b16`, clean at collection.
  Product seal `adff3c732ba428e243799464737f458763c8fa194db9df0127eb0df1e1b0e2d6`;
  harness seal `9e9d56ea7540e38c99f6d69c7a1abeedc1524aefb413a09bf815e3ce73d782fd`;
  Cargo.lock SHA-256 `09b880a18e1c221ba830908467f0985b0419ae0bf3c80c90ede7f7c5178272d6`.
- [Frozen v4 registry](../../../../../benchmark/fs-bench-pro/registry/workspace-exec-insert-v4.json)
  SHA-256 `9decc295b688083db7b1f6899ebbc315eeeddfa28d0fafe68537507d12394fbb`;
  prospective [contract](../../PHASE4-V4-CONTRACT.md). The four commands append
  `--output-version 4`, using one release `aarch64-unknown-linux-musl` image
  `sha256:9f24110e647ba22a4d0d995df6d25ec414552aef4222a5f4ca9ad82785a6edda`.
  Daemon SHA-256 `fd7462819ad7392af91b079980516c05590ceffce69bd0365f4726b17943dabb`;
  tool `5b4ba2cdf4fb7d109a40ea88acb44ab9288f3fcd0f440015948c5f23c53c3361`;
  release driver `51471c27f169944a42b60b4a0d4dc9ad5a0fdb98b2bff7940da1a32f097248f0`;
  verifier `37112928b15f339ea25dc74477f544bad4d690a3994fa1acd39480ad1105fb62`.
- One sample per case, one separate verifier per sample. A validated pristine
  master was prepared once per size and each sample used an independent writable
  byte copy (`clone_method=independent-writable-byte-copy`), outside the timer.
  The macOS Store pages were invalidated and checked resident-free in each
  row. The Linux FUSE backing domain remained declared warm and `INELIGIBLE`.
  Complete-command wall cap 15 s, verifier cap 15 s. No arm was rerun.

## All four registered rows

| Fixture | Edit ms | Commit ms | Edit→Commit ms | Target ms | Raw / target | Complete command s | Verifier s | Retained receipt | Derived admission |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| 1 MiB | 25.548 | 22.073 | 47.632 | 5.30 | 8.987× | 1.642 | 0.622 | [receipt](1mib/receipt.json) | `INELIGIBLE` |
| 10 MiB | 25.342 | 23.062 | 48.413 | 6.40 | 7.565× | 0.982 | 0.068 | [receipt](10mib/receipt.json) | `INELIGIBLE` |
| 100 MiB | 27.859 | 31.414 | 59.284 | 6.25 | 9.485× | 0.938 | 0.523 | [receipt](100mib/receipt.json) | `INELIGIBLE` |
| capped 500 MiB | 28.213 | 53.452 | 81.680 | 7.76 | 10.526× | 0.928 | 2.497 | [receipt](500mib/receipt.json) | `INELIGIBLE` |

Arithmetic: Edit→Commit milliseconds = retained `edit_commit_ns / 1,000,000`;
raw/target = that value divided by the registered `g2_target_ms`. The four
complete commands and verifiers met their hard wall limits. All four raw
durations missed the target; none is a cold-qualified speed result. The
[runner's four-row report](report-edit.tsv) and [JSON summary](report-edit.json)
retain every selected row and count four raw target misses, zero timeouts.

## Verification, route and telemetry

Each [1](1mib/verification.json), [10](10mib/verification.json),
[100](100mib/verification.json), and [500 MiB](500mib/verification.json)
verifier returned `PASS`: full final bytes/SHA-256, size, pinned canonical
root/count, Branch head, published mode/mtime, retained genesis metadata and
fresh read-only reopen all matched. Each route returned two range-state
callbacks, one range-edit callback, zero write callbacks, 4,096 accepted
payload bytes, zero shifted suffix bytes and confirmed unmount/Sandbox Delete.
The fixed callback counts across sizes are mechanism evidence, not a speed PASS.

Each case retains complete [raw driver stderr](1mib/driver.raw.stderr)
(`10mib/`, `100mib/`, and `500mib/` hold their own). Caller/Service and daemon
LFT1 each emitted exactly one run summary with dropped=0, failed=0 and
overflow=false; run, role, namespace, PID, edit/commit root and daemon
WorkspaceExec matched. Host and daemon CPU/RSS were sampled with gaps=0.
Their first/last samples did **not** enclose both scope boundaries, so the
recorded CPU deltas and RSS maxima describe only their sampled process windows,
not exact phase CPU or peak memory. Child Edit and Commit have wall times, but
no exclusive CPU/RSS attribution. The [recheck outputs](1mib/telemetry-check.json)
for all four sizes preserve this partial coverage explicitly.

The original performance receipts remain `INCOMPLETE`: their first parser
incorrectly treated a normal last sample before scope close as missing
telemetry. Parser-only commit `b4547a62d` corrected that interpretation after
the four samples. It re-read the retained raw stderr without resampling or
editing the original receipt; each [rechecked status](1mib/status-rechecked.json)
is `INELIGIBLE` because of the frozen Linux cache contract. This is an
evidence-reader correction, not a promotion of historical v3 evidence or a
new measurement at `b4547a62d`.

The [SHA-256 inventory](SHA256SUMS) covers the copied receipts, raw logs,
verifier output and four-row runner report. The large Store/history copies
and cursor capabilities remain in the worktree-local run folders; the raw
receipts pin their paths and master/clone identities. No cursor capability is
published in this evidence bundle.

## Source checks and limits

At the measured source, `cargo +1.85.1 test --manifest-path core/Cargo.toml
--locked --workspace`, the matching `--workspace --examples` check,
`cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace
--all-targets -- -D warnings`, `cargo +1.85.1 fmt --manifest-path
core/Cargo.toml --all --check`, `python3 core/tools/check_product_boundary.py`
and the Core tool self-tests passed. The focused benchmark Exec Python group
passed 41 tests with one skipped platform case. The parser correction's focused
one-test check passed. The locked release image and host examples built. No
aggregate pre-push or CI gate exists; macOS-host Core unit tests with Linux
kernel requirements were skipped by their platform guards. The live four-case
Docker SDK route and the earlier functional position sweep supply the Linux
FUSE evidence for this child scope.
