# R7-retire completion: three predecessors removed, four kept for one owner question

> **Status:** Dated checkpoint record, 2026-10-10; not release evidence or a
> product contract. No sample was taken and no timing, storage or memory claim
> is made.

R7-retire is the stage the [R5–R9 handoff](HANDOFF-R5-R9-20261009.md) calls
"R7: covered integration cleanup (S11)": audit the seven directories excluded
in `core/Cargo.toml`, migrate what no active crate covers, then remove them.
**Three of the seven are removed. Four are not**, because they hold the only
implementation of pinned read-only views and whether the product keeps that
feature is an owner question with no recorded answer (303/08 O-10). This
session had no delegated owner authority, so it stopped there.

- Start: `cf5bd62ce`. Final product source identity: `bc314e73d`. The commit
  that adds this record changes documentation and receipts only.
- Local `main`, one checkout, nothing pushed, no pull request, no new worktree.
- Global Store profile: Disposable / WAL / synchronous=OFF only. Durable:
  `NOT_RUN — disabled by owner until explicit reauthorization`.
- Evidence: the audit and receipts in
  [`checks/r7-retire-20261010/`](checks/r7-retire-20261010/00-scope-and-method.md).

## Result per crate

Production lines from the pinned `tools/production_loc.py` at `cf5bd62ce`
([receipt 01](checks/r7-retire-20261010/01-production-loc-head.txt)).

| Crate | Lines | Outcome | Audit | Last commit holding the source |
| --- | ---: | --- | --- | --- |
| `layerfs-server` | 3996 | Removed in `15e33e22c` | [10](checks/r7-retire-20261010/10-layerfs-server.md) | `a7ba85d3b` |
| `layerfs-fuse-legacy` | 1447 | Removed in `5bc23bbcb` | [11](checks/r7-retire-20261010/11-layerfs-fuse-legacy.md) | `15e33e22c` |
| `layerfs-sandbox-legacy` | 1106 | Removed in `bc314e73d` | [12](checks/r7-retire-20261010/12-layerfs-sandbox-legacy.md) | `5bc23bbcb` |
| `layerfs-sdk-legacy` | 505 | **Kept** (O-10) | [13](checks/r7-retire-20261010/13-layerfs-sdk-legacy.md) | present |
| `layerfs-daemon-legacy` | 2151 | **Kept** (O-10) | [14](checks/r7-retire-20261010/14-layerfs-daemon-legacy.md) | present |
| `layerfs-bridge-legacy` | 6834 | **Kept** (O-10) | [15](checks/r7-retire-20261010/15-layerfs-bridge-legacy.md) | present |
| `layerfs-workspace-legacy` | 26835 | **Kept** (O-10) | [16](checks/r7-retire-20261010/16-layerfs-workspace-legacy.md) | present |

Retired: 6549 of 42,874 lines. Kept: 36,325. All three removals are
retirement of a replaced predecessor. No line was relocated: each active
replacement was written in an earlier stage, and this stage changed no
production source in an active crate.

Recover a removed file with `git show <last commit>:core/crates/<crate>/<path>`.

## How the audit classified things

Every one of the 190 production source files and 409 test functions in the
seven directories has a row: covered by a named active file and test (A),
dropped by a recorded direction that is cited (C), migrated in this stage (B),
or held by an open owner question (O). Two facts frame all of it:

- None of the seven packages can be loaded by Cargo at the start commit
  ([receipt 02](checks/r7-retire-20261010/02-cargo-cannot-load.txt)); each
  depends on the `layerfs-api-core` package F12 already removed. So none of
  their tests is coverage today, and removing them cannot change a build or a
  test result.
- The #303 design replaces the predecessors as a whole
  (303/07 §4.1, 303/08 §4). The audit therefore asks of each behaviour whether
  the active product has it or a recorded direction dropped it, not whether
  the old code path survives.

## Migrated (class B)

One behaviour had no active test: executing a program stored in the mount.
`layerfs-daemon/tests/mounted_execute.rs` (commit `a6655ef45`, Linux, real
kernel mount) now asserts it: `EACCES` without an execute bit even for root,
`ETXTBSY` while a writable descriptor is open, the exit status of the stored
program, an equal-length in-place rewrite of a cached program running as the
new program, replacement by rename, a `#!` script, and group execute
permission honoured between two users. It replaces three predecessor tests
([table](checks/r7-retire-20261010/00-scope-and-method.md#class-b-what-was-migrated)).
It is a test only and passed on its first run; no product source changed.

## Kept, and why

| Kept | Reason |
| --- | --- |
| The four crates above | They hold the view lease (`pin_view`, `view_read`, `release_view` and the rest; 1656 lines in seven files, with call sites elsewhere). 303/08 lists O-10 "Are pinned read-only SDK views kept?" with the recommendation "Defer them" and K20 records "no owner answer exists". Everything else in the four is class A or C. The removal is [prepared step by step](checks/r7-retire-20261010/30-prepared-removal.md) |
| `core/reference-tests` (60 files, no manifest, counted by nothing) | Its README says "Archived; retained for historical evidence only"; no document authorizes removal. Not source of the seven |
| `core/crates/layerfs-api/cli` and `mcp` (one README each, zero lines) | `core/AGENTS.md` forbids scaffolding placeholders but does not say to delete these, and the API documents still plan both adapters |
| Root `crates/` | R9's, untouched |
| Benchmark harness lines naming `layerfs-server` | Baseline arms build it in a baseline worktree at a pinned commit, where it still exists. Three families that build it from the current checkout were already unbuildable before this stage. The harness is frozen and this stage takes no sample ([detail](checks/r7-retire-20261010/20-references-and-decisions.md#4-benchmark-harnesses-under-corebenchmark)) |

## What changed besides the removals

- `core/Cargo.toml`: `exclude` now lists `vendor/fuser-0.18.0` and the four
  kept crates.
- `core/tools/check_product_boundary.py`: `RETIRED_PACKAGES` names the three
  removed directories. A directory or a manifest with one of those package
  names fails the scan. Its test file asserts the refusal; a test that let a
  `layerfs-sandbox-legacy` manifest pass now requires it to fail.
- `core/AGENTS.md`: states which three are removed and which four remain and
  why.
- Architecture 03, 14, 16, 71 and 75, `api/operations.md` and the
  `exec2edit` benchmark note: dated notes naming the removal and the commit
  that still holds the source. A live link into `layerfs-server` was replaced.
- Dated issue documents under `core/docs/issues` are unchanged; they keep
  their paths and verdicts.

## Closing checks at `bc314e73d`

| Check | Result | Receipt |
| --- | --- | --- |
| `cargo fmt --all -- --check` | PASS | [40](checks/r7-retire-20261010/40-host-fmt-guard.txt) |
| Boundary guard | PASS, 823 production files scanned | 40 |
| Guard and tooling self-tests | 50 of 50 | 40 |
| Clippy `-D warnings`, host, 13 members, all targets | PASS | [41](checks/r7-retire-20261010/41-host-clippy.txt) |
| Clippy `-D warnings`, Linux pinned image, 13 members, all targets | PASS | [42](checks/r7-retire-20261010/42-linux-clippy.txt) |
| Host suite, 13 packages, one bounded run per binary | 260 of 262 binaries; 1187 tests passed, 2 failed | [50](checks/r7-retire-20261010/50-host-suite.txt) |
| Linux suite, pinned image with FUSE device, 13 packages | 260 of 262 binaries; 1237 tests passed, 2 failed | [51](checks/r7-retire-20261010/51-linux-suite.txt) |
| Dependency check | No active manifest, lockfile, source include, script or harness path reaches a removed directory | [43](checks/r7-retire-20261010/43-dependency-check.txt) |

The four failing invocations are the known ones, each on a precondition that
was not supplied; they are failures of those invocations and unrun as proofs:

- `complete_installed_roots::huge_native_namespace_is_complete_after_install`,
  host and Linux: "explicit closed preparation for clone setup".
- `host_handoff::macos_init_handoff_to_linux_daemon_has_no_host_data_path`,
  host: "explicit build-listed Linux binary: NotPresent".
- `shared_processes::overlapping_saves_ordered_overwrites_and_killed_daemon_preserve_the_shared_store`,
  Linux: "explicit named-volume placement".

The test counts omit one binary on each side (`owner`), whose summary line
the runner did not capture because a test name begins with `result`; its exit
status is 0 and its output is in the full log beside each receipt.

No whole-suite run was made at the start commit for comparison. "As before"
rests on this: between `cf5bd62ce` and `bc314e73d` the only change inside the
13 active packages, the vendored fuser and `core/Cargo.lock` is the added
test file `mounted_execute.rs`, and the failures are exactly the named known
ones. The last commit of the stage adds documentation and receipts only;
`fmt --check`, the guard and its self-tests were run again on its staged tree
([receipt 44](checks/r7-retire-20261010/44-closing-commit-fmt-guard.txt)).
Clippy and the suites were not rerun on it: it changes no Rust, SQL,
manifest or tool.

The dependency check found one dangling reference, in a kept crate:
`layerfs-sdk-legacy/Cargo.toml` still names `layerfs-server` by path. That
manifest was already unloadable (it also names `layerfs-api-core`), is in no
workspace, and is left byte-for-byte as a predecessor.

## Production LOC per commit

Pinned `tools/production_loc.py` (SHA-256 `c0fe7f36…624adb`) through
`core/target/rx-count.py staged`: first parent and final staged tree, each
extracted from Git. The JSON for each commit is in the checks directory.

| Commit | Subject | Before | After | Delta | Kind |
| --- | --- | ---: | ---: | ---: | --- |
| `a6655ef45` | Test: a stored program executes on a real mount | 187549 | 187549 | 0 | test only |
| `a7ba85d3b` | Coverage audit | 187549 | 187549 | 0 | documentation |
| `15e33e22c` | Remove `layerfs-server` | 187549 | 183553 | −3996 | retirement |
| `5bc23bbcb` | Remove `layerfs-fuse-legacy` | 183553 | 182106 | −1447 | retirement |
| `bc314e73d` | Remove `layerfs-sandbox-legacy` | 182106 | 181000 | −1106 | retirement |
| this record | Completion record and receipts | 181000 | 181000 | 0 | documentation |

Subtotals, start to end: core 122132 → 115583; active 79258 unchanged;
excluded predecessors 38878 → 36325; excluded integration 3996 → 0; root
reference 65417 unchanged. The drop is deletion of replaced code. It is not
an algorithmic simplification of anything that runs.

## Questions for the owner

1. **O-10: are pinned read-only SDK views kept?** This alone holds 36,325
   lines. Recommendation: defer or drop them, as 303/08 already recommends.
   The predecessor's views rest on the private page backing, which is removed
   by owner requirement, so that code cannot be activated either way; a kept
   feature needs a new design on the SQLite overlay. With that answer the
   four removals are the prepared checklist and nothing else is owed first.
2. **May `core/reference-tests` be removed, and when?** Recommendation:
   remove it with the root reference in R9. Nothing builds or counts it.
3. **Remove the `layerfs-api/cli` and `mcp` placeholder directories?**
   Recommendation: yes; they contradict "Scaffold no empty placeholder
   directories" and hold nothing.
4. **Confirm what was not carried.** `SandboxApi::list`, bounded daemon log
   capture at Sandbox delete, a separate read-only mount profile, FUSE
   `ACCESS` served by the daemon, and headless stdin/stdout delivery are
   absent from the active crates. Each is outside a recorded scope, but no
   sentence names any of them individually
   ([table](checks/r7-retire-20261010/00-scope-and-method.md#not-carried-with-no-sentence-naming-them)).
5. **Host-mediated benchmark families** need a registered
   `NOT_RUN — mechanism removed` disposition. That is R8's, under 303/08 O-16.

## Not verified

- No predecessor test was run; none can be built. Their intent was read from
  source.
- Class A rows were matched by reading test names and, where a name was not
  decisive, the test body. The stage's final suites ran every active test
  binary once; the named tests were not rerun one by one for the audit.
- The predecessors' Docker route scripts were classified by their header and
  the binaries they start, not run.
- No benchmark family was run, so the statement that three harness families
  were already unbuildable rests on `cargo pkgid` and on reading the harness.

## Noticed, not fixed

- `core/tools/check_fuser_integrity.py` exits 1 at the start commit and now:
  both errors name `core/benchmark/r7-passthrough`, a harness of the R7
  optimization run that patches fuser locally. Not a required check of this
  stage and untouched by it (receipt 40).
- `core/docs/api/operations.md` still links
  `layerfs-bridge/src/contract/history.rs`, which F12 removed.
- `core/docs/architecture/proposal/service-daemon-transport/07-public-operations.md`
  links a `layerfs-server` file that did not exist before this stage either.
- `fs-bench-pro` `runner.py`, `families/workspace_write.py` and
  `shell_package.py` name packages and examples that no longer exist at HEAD.
- `core/target/rx-suite.py` (untracked helper) takes the first line that
  starts with `test result` as a binary's summary, so a test whose name starts
  with `result` hides the totals of its binary.
