# R7-retire completion: the seven excluded predecessors are removed

> **Status:** Dated checkpoint record, 2026-10-10; not release evidence or a
> product contract. No sample was taken and no timing, storage or memory claim
> is made.
>
> **Closed 2026-10-10** with R7 by owner direction ("proceed to close r7"). The
> [questions for the owner](#questions-for-the-owner) stay open and are carried
> in the [rollout ledger](ROLLOUT-LEDGER-20261008.md) row.

R7-retire is the stage the [R5–R9 handoff](HANDOFF-R5-R9-20261009.md) calls
"R7: covered integration cleanup (S11)": audit the seven directories excluded
in `core/Cargo.toml`, migrate what no active crate covers, then remove them.
**All seven are removed.** `core/crates` holds only the 13 active members.

It ran in two passes on the same day:

1. The audit, one migrated test and the removal of three crates. The session
   had no delegated owner authority, so it stopped at the four crates that
   held the only implementation of pinned read-only views (303/08 O-10, no
   recorded answer) and recorded the stage as partial at `55aa5b000`.
2. After the owner authorized the full cleanup, the removal of those four
   crates and of the two `layerfs-api` placeholder directories. See the
   [addendum](#addendum-full-cleanup-by-owner-authorization).

- Start: `cf5bd62ce`. Final product source identity: `7999a6356`. The commit
  that adds this version of the record changes documentation and receipts only.
- Local `main`, one checkout, nothing pushed, no pull request, no new worktree.
- Global Store profile: Disposable / WAL / synchronous=OFF only. Durable:
  `NOT_RUN — disabled by owner until explicit reauthorization`.
- Evidence: the audit and receipts in
  [`checks/r7-retire-20261010/`](checks/r7-retire-20261010/00-scope-and-method.md).

## Result per crate

Production lines from the pinned `tools/production_loc.py` at `cf5bd62ce`
([receipt 01](checks/r7-retire-20261010/01-production-loc-head.txt)); each
removal commit's own count matches its row.

| Crate | Lines | Removed in | Audit | Last commit holding the source |
| --- | ---: | --- | --- | --- |
| `layerfs-server` | 3996 | `15e33e22c` | [10](checks/r7-retire-20261010/10-layerfs-server.md) | `a7ba85d3b` |
| `layerfs-fuse-legacy` | 1447 | `5bc23bbcb` | [11](checks/r7-retire-20261010/11-layerfs-fuse-legacy.md) | `15e33e22c` |
| `layerfs-sandbox-legacy` | 1106 | `bc314e73d` | [12](checks/r7-retire-20261010/12-layerfs-sandbox-legacy.md) | `5bc23bbcb` |
| `layerfs-sdk-legacy` | 505 | `d6b7539f7` | [13](checks/r7-retire-20261010/13-layerfs-sdk-legacy.md) | `55aa5b000` |
| `layerfs-daemon-legacy` | 2151 | `b56950220` | [14](checks/r7-retire-20261010/14-layerfs-daemon-legacy.md) | `d6b7539f7` |
| `layerfs-bridge-legacy` | 6834 | `eeeffb6b5` | [15](checks/r7-retire-20261010/15-layerfs-bridge-legacy.md) | `b56950220` |
| `layerfs-workspace-legacy` | 26835 | `e6794521a` | [16](checks/r7-retire-20261010/16-layerfs-workspace-legacy.md) | `eeeffb6b5` |

Retired: 42,874 production lines, the whole excluded scope. All seven are
retirement of a replaced predecessor. No line was relocated: each active
replacement was written in an earlier stage, and this stage changed no
production source in an active crate.

Recover a removed file with `git show <last commit>:core/crates/<crate>/<path>`.

| Total | Start `cf5bd62ce` | End `7999a6356` |
| --- | ---: | ---: |
| Core, active members | 79258 | 79258 |
| Core, excluded predecessors | 38878 | 0 |
| Core, excluded integration (`layerfs-server`) | 3996 | 0 |
| Core | 122132 | 79258 |
| Root reference `crates/` | 65417 | 65417 |
| Combined | 187549 | 144675 |

The drop is deletion of replaced code. It is not an algorithmic
simplification of anything that runs.

## Addendum: full cleanup by owner authorization

The first pass left four crates and listed O-10 as the blocking question. The
lead session checked that pass against its receipts and put the decision to
the owner with this recommendation: remove `layerfs-sdk-legacy`,
`layerfs-daemon-legacy`, `layerfs-bridge-legacy` and
`layerfs-workspace-legacy` whatever the answer to O-10 is, because none of
them can be built, the view code rests on the removed private page backing so
it cannot be activated, and the source stays recoverable from Git; O-10 then
decides a future feature, not this deletion.

The owner's reply, relayed verbatim by the lead session: **"yes, do the full
cleanup"**. This record did not receive the reply from the owner directly.

What that reply was taken to cover, and what it was not:

| Item | Done |
| --- | --- |
| The four kept crates | Removed, one commit each, following the [prepared checklist](checks/r7-retire-20261010/30-prepared-removal.md); every expected delta matched |
| `core/crates/layerfs-api/cli` and `mcp` (first-pass question 3) | Removed in `7999a6356`. Each held one README sentence. `core/docs/api/README.md` keeps the adapter plan and says each directory is created with its first real file |
| `core/reference-tests` | Left in place. It goes with the root reference in R9 |
| Root `crates/` | Untouched |
| The five behaviours not carried | Neither implemented nor struck. The owner has not confirmed them one by one; they stay an [open question](#questions-for-the-owner) |

**O-10 is still open as a product question.** Pinned read-only SDK views are
deferred; no implementation is in the tree; a kept feature needs a new design
on the SQLite overlay. The removal is not a decision that views are dropped.
The O-10 row of
[303/08](../303/08-decisions-provenance.md#7-questions-only-the-owner-can-answer)
and `core/AGENTS.md` say the same. The audit rows classed O (1656 lines in
seven files, plus call sites and four test files) describe code that now
exists only in Git history.

## How the audit classified things

Every one of the 190 production source files and 409 test functions in the
seven directories has a row: covered by a named active file and test (A),
dropped by a recorded direction that is cited (C), migrated in this stage (B),
or held by an open owner question (O). Two facts frame all of it:

- None of the seven packages could be loaded by Cargo at the start commit
  ([receipt 02](checks/r7-retire-20261010/02-cargo-cannot-load.txt)); each
  depended on the `layerfs-api-core` package F12 already removed. So none of
  their tests was coverage, and removing them could not change a build or a
  test result.
- The #303 design replaces the predecessors as a whole
  (303/07 §4.1, 303/08 §4). The audit therefore asks of each behaviour whether
  the active product has it or a recorded direction dropped it, not whether
  the old code path survives.

The audit documents are kept as written at audit time, each with a dated
addendum naming its removal.

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
| `core/reference-tests` (60 files, no manifest, counted by nothing) | Its README says "Archived; retained for historical evidence only". By the lead's direction it goes with the root reference in R9. Not source of the seven |
| Root `crates/` | R9's, untouched |
| Benchmark harness lines naming `layerfs-server` | Baseline arms build it in a baseline worktree at a pinned commit, where it still exists. Three families that build it from the current checkout were already unbuildable before this stage. The harness is frozen and this stage takes no sample ([detail](checks/r7-retire-20261010/20-references-and-decisions.md#4-benchmark-harnesses-under-corebenchmark)) |
| Dated issue documents under `core/docs/issues` | Historical records keep their paths and verdicts, including paths into the removed directories |

## What changed besides the removals

- `core/Cargo.toml`: `exclude` now lists only `vendor/fuser-0.18.0`.
- `core/tools/check_product_boundary.py`: `RETIRED_PACKAGES` names all seven
  directories. A directory or a manifest with one of those package names
  fails the scan. Its test file asserts the refusal for each name; a test that
  let a `layerfs-sandbox-legacy` manifest pass now requires it to fail. The
  scan covers 674 production files, down from 823 before the second pass.
- `core/AGENTS.md`: says all seven are removed, that the guard refuses their
  return, and that pinned views are deferred and not to be supplied by
  restoring predecessor code.
- Architecture 03, 14, 16, 20, 21, 22, 23, 71 and 75, `api/operations.md` and
  the `exec2edit` benchmark note: dated notes naming the removal and the
  commit that still holds the source. A live link into `layerfs-server` was
  replaced.
- `core/docs/api/README.md`: the layout shows `mcp/` and `cli/` as future
  directories with a dated note.
- `core/docs/issues/303/08-decisions-provenance.md`: the O-10 row records
  that the question is still open, deferred, with no implementation in the
  tree. No other row changed.

## Closing checks at `7999a6356`

Run on that commit with only documentation edits in the working tree.

| Check | Result | Receipt |
| --- | --- | --- |
| `cargo fmt --all -- --check` | PASS | [70](checks/r7-retire-20261010/70-final-fmt-guard.txt) |
| Boundary guard | PASS, 674 production files scanned | 70 |
| Guard and tooling self-tests | 50 of 50 | 70 |
| Clippy `-D warnings`, host, 13 members, all targets | PASS | [71](checks/r7-retire-20261010/71-final-host-clippy.txt) |
| Clippy `-D warnings`, Linux pinned image, 13 members, all targets | PASS | [72](checks/r7-retire-20261010/72-final-linux-clippy.txt) |
| Host suite, 13 packages, one bounded run per binary | 260 of 262 binaries; 1199 tests passed, 2 failed | [80](checks/r7-retire-20261010/80-final-host-suite.txt) |
| Linux suite, pinned image with FUSE device, 13 packages | 260 of 262 binaries; 1249 tests passed, 2 failed | [81](checks/r7-retire-20261010/81-final-linux-suite.txt) |
| Dependency check | No tracked manifest, lockfile, source include, script or harness path reaches any of the seven directories or the two placeholders | [73](checks/r7-retire-20261010/73-final-dependency-check.txt) |
| Pinned production LOC | Combined 144675; core 79258, all active | [63](checks/r7-retire-20261010/63-loc-remove-workspace-legacy.json), [66](checks/r7-retire-20261010/66-loc-completion.json) |

The four failing invocations are the known ones, each on a precondition that
was not supplied; they are failures of those invocations and unrun as proofs:

- `complete_installed_roots::huge_native_namespace_is_complete_after_install`,
  host and Linux: "explicit closed preparation for clone setup".
- `host_handoff::macos_init_handoff_to_linux_daemon_has_no_host_data_path`,
  host: "explicit build-listed Linux binary: NotPresent".
- `shared_processes::overlapping_saves_ordered_overwrites_and_killed_daemon_preserve_the_shared_store`,
  Linux: "explicit named-volume placement".

Test totals are summed from the full log beside each receipt. The one-line
summaries show 12 fewer on each side because the runner did not capture the
summary line of the `owner` binary (a test name there begins with `result`);
its exit status is 0.

The same checks were run once before, at `bc314e73d` after the first three
removals, with the same results binary for binary: receipts
[40](checks/r7-retire-20261010/40-host-fmt-guard.txt) to
[43](checks/r7-retire-20261010/43-dependency-check.txt),
[50](checks/r7-retire-20261010/50-host-suite.txt) and
[51](checks/r7-retire-20261010/51-linux-suite.txt). No whole-suite run was
made at the start commit. "As before" rests on this: between `cf5bd62ce` and
`7999a6356` the only change inside the 13 active packages, the vendored fuser
and `core/Cargo.lock` is the added test file `mounted_execute.rs`, and the
failures are exactly the named known ones.

The commit that adds this version of the record changes documentation and
receipts only. `fmt --check`, the guard and its self-tests were run again on
its staged tree
([receipt 74](checks/r7-retire-20261010/74-closing-commit-fmt-guard.txt));
Clippy and the suites were not, since it changes no Rust, SQL, manifest or
tool.

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
| `55aa5b000` | First-pass record and receipts | 181000 | 181000 | 0 | documentation |
| `d6b7539f7` | Remove `layerfs-sdk-legacy` | 181000 | 180495 | −505 | retirement |
| `b56950220` | Remove `layerfs-daemon-legacy` | 180495 | 178344 | −2151 | retirement |
| `eeeffb6b5` | Remove `layerfs-bridge-legacy` | 178344 | 171510 | −6834 | retirement |
| `e6794521a` | Remove `layerfs-workspace-legacy` | 171510 | 144675 | −26835 | retirement |
| `7999a6356` | Remove the `cli` and `mcp` placeholders | 144675 | 144675 | 0 | two README files |
| this record | Final record and receipts | 144675 | 144675 | 0 | documentation |

## Questions for the owner

1. **O-10: are pinned read-only SDK views kept?** Still open, now as a
   question about a future feature only. Nothing in the tree implements them.
   A kept feature needs a new design on the SQLite overlay (303/08 K20: they
   change the version bound).
2. **Confirm what was not carried.** `SandboxApi::list`, bounded daemon log
   capture at Sandbox delete, a separate read-only mount profile, FUSE
   `ACCESS` served by the daemon, and headless stdin/stdout delivery are
   absent from the active crates. Each is outside a recorded scope, but no
   sentence names any of them individually, and the owner's reply did not
   confirm them one by one
   ([table](checks/r7-retire-20261010/00-scope-and-method.md#not-carried-with-no-sentence-naming-them)).
   Their predecessor source is now in Git history only.
3. **Host-mediated benchmark families** need a registered
   `NOT_RUN — mechanism removed` disposition. That is R8's, under 303/08 O-16.
4. **May `core/benchmark/r7-passthrough` carry the authorized fuser patch?**
   See the first item under [Noticed, not fixed](#noticed-not-fixed).

Answered since the first pass: the four kept crates (removed), the `cli` and
`mcp` placeholders (removed), `core/reference-tests` (stays until R9).

## Not verified

- No predecessor test was run; none could be built. Their intent was read
  from source.
- Class A rows were matched by reading test names and, where a name was not
  decisive, the test body. The final suites ran every active test binary
  once; the named tests were not rerun one by one for the audit.
- The predecessors' Docker route scripts were classified by their header and
  the binaries they start, not run.
- No benchmark family was run, so the statement that three harness families
  were already unbuildable rests on `cargo pkgid` and on reading the harness.
- The owner's authorization reached this session through the lead session's
  message, quoted there verbatim. It was not confirmed with the owner
  independently.
- The Linux Clippy helper pipes Cargo through `tail`, so its pass is read from
  the `Finished` line with no warning or error line, not from an exit status.

## Noticed, not fixed

- **`core/tools/check_fuser_integrity.py` exits 1** at the start commit and
  now, on `core/benchmark/r7-passthrough` (a harness of the R7 optimization
  run; receipt 70). The harness is already in the product's own form: its
  manifest carries `[patch.crates-io] fuser = { version = "=0.18.0", path =
  "../../vendor/fuser-0.18.0" }`, the same vendored source and the same entry
  that `core/benchmark/cluster2-platform` carries. The checker rejects it
  because the checker authorizes that patch by an explicit list of files, and
  this harness is not on it:
  - `Cargo.toml: patch override of fuser is not the authorized timestamp
    patch`: the path is absent from `PATCH_MANIFESTS`, so the checker has no
    permitted patch path for this manifest and refuses any fuser patch in it.
  - `Cargo.lock: locked fuser has a local/replacement source`: the path is
    absent from `PATCH_LOCKS`, so a fuser entry with no registry source is
    refused.

  Nothing in the harness can be changed to pass while it keeps the vendored,
  patched source. Passing needs one entry in each of those two lists, which
  is a change to the checker's rules and to the scope of the owner's fuser
  authorization. It was left as it is: the harness, its lockfile, the checker
  and `core/vendor` are untouched, and the R7 floor receipts keep their
  identity.
- `core/docs/api/operations.md` still links
  `layerfs-bridge/src/contract/history.rs`, which F12 removed.
- `core/docs/architecture/proposal/service-daemon-transport/07-public-operations.md`
  links a `layerfs-server` file that did not exist before this stage either.
- `fs-bench-pro` `runner.py`, `families/workspace_write.py` and
  `shell_package.py` name packages and examples that no longer exist at HEAD.
- Untracked helpers in `core/target`: `rx-suite.py` takes the first line that
  starts with `test result` as a binary's summary, so a test whose name starts
  with `result` hides the totals of its binary; `r4-build.sh fmt-check` needs
  package arguments and, like `rx-linux.sh clippy`, pipes through `tail`, so
  its exit status is not the check's.
