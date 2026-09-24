# #241 public SDK Exec→Commit caller gate — 2026-09-24

## Result

**PASS for one small-fixture mounted functional route.** The public SDK
created a Project, Branch and Sandbox, mounted a writable Linux FUSE
Workspace, and issued one `WorkspaceApi::exec` command using the frozen
`/layerfs-bench/bin/layerfs-edit-tool splice` interface. The tool returned
`ExecResult.exit_status=Some(0)` and untruncated JSON `PASS` with final length
12,288 and physical shifted bytes zero. The test-only SDK gate then made an
explicit `WorkspaceApi::commit` call and received `Committed`. Status observed
exactly two `range_state` callbacks, one `range_edit` callback, 4,096 accepted
replacement bytes and zero physically shifted suffix bytes.

The tool's [source](../../../../benchmark/fs-bench-pro/workload/src/splice.rs)
performs its pre-STATE, one EDIT, post-STATE, same-descriptor `fstat` and
bounded boundary readback before it exits zero; its post-EDIT uncertainty
paths exit 75 without retry. This live result exercised the success branch.
A separate public SDK Exec of `exit 75` returned `Some(75)`, and the same
gate helper omitted Commit. That is a **synthetic process exit** used to
prove the SDK branch; it is not an induced product notifier or reply-loss
failure. The optional pure check also rejects `None` and other nonzero exit
statuses. The test explicitly unmounted and deleted its Sandbox through
public SDK APIs. [Raw output and receipt](attempt-001/) retain the actual
Commit result and status counts; [verify.py](verify.py) reproduces the
conclusion from those files.

| Gate | Outcome | Limit |
| --- | --- | --- |
| Real mounted STATE→EDIT→STATE tool via public `WorkspaceApi::exec` | **PASS** | One 8 KiB source fixture and 4 KiB insert; not a position sweep or large-file claim. |
| Commit only after `ExecResult.exit_status == Some(0)` | **PASS** | One explicit public Commit returned `Committed`. |
| Exit 75 omits Commit | **PASS for SDK gate** | The exit 75 command was synthetic; an actual product UNKNOWN path was not induced here. |
| Actual post-EDIT error/lost reply through SDK | **INCOMPLETE** | Tool source handles it; this live route did not trigger it. |
| Independent old/new Commit content oracle and 192-position sweep | **NOT_RUN here** | Owned by the separate functional sweep. |
| Cold-cache or Edit→Commit latency | **INELIGIBLE** | Cache state uncontrolled; no performance arm was run. |

## Frozen case and source custody

The [prospective contract](CONTRACT.md) was committed before this one live
attempt. It chose byte `i % 251` for 8,192 source bytes and byte `i % 239`
for the 4,096 replacement bytes. Both files were imported through
`ProjectApi::init` before mounting; the tool read `payload.bin` inside the
mounted Workspace, never from a host path. The executed command was:

```sh
/layerfs-bench/bin/layerfs-edit-tool splice --file data.bin --expect-size 8192 --offset 4093 --delete-length 0 --length 4096 --payload payload.bin
```

Worktree `/Users/yifanxu/.codex/worktrees/issue241-status-plumbing/layerfs`,
branch `codex/issue241-status-plumbing`, test/contract source commit
`8493d8f8b59216438468758b51e4c5d3b296e9d4`, source tree
`ce6c70a5b925390c2e0c0137a64f760ec650e87f`. The build/run worktree
contained only untracked evidence output; committed product and test source
matched that source tree. [identity.json](identity.json) records test and gate
source SHA-256, lockfiles, repository Cargo flags, host, daemon and tool
binary SHA-256, Dockerfile and immutable image ID. The unmodified published
`fuser` 0.18.0 is reached through the product adapter; no third-party package
was edited.

The release daemon binary was
`a0b83dbda014305300c9126755dc327b60865dd99017806e6b5e7c3f43e9f5f4`;
the release edit tool was
`6bf715665058b1a7800a00a700c4639de69973af9d3eb32914de404e715aab7f`.
The unique image was
`sha256:b5f63144fd930441365ee222db547c11214550c44f6c64bfbba236946b418ffc`,
Linux aarch64 from pinned Alpine base
`sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`.
The independent test Store was created locally; the parent worktree's
ignored prepared master was not read or modified.

## Exact build and execution commands

From the worktree root, locked release builds were:

```sh
cargo +1.85.1 zigbuild --manifest-path core/Cargo.toml --locked --offline --target aarch64-unknown-linux-musl -p layerfs-daemon --release
cargo +1.85.1 zigbuild --manifest-path core/benchmark/fs-bench-pro/workload/Cargo.toml --locked --offline --target aarch64-unknown-linux-musl --release
```

The binaries were copied to the ignored local
`core/target/issue241-sdk-gate-image/` directory. Its Dockerfile used the
pinned Alpine base, copied the daemon to `/layerfs-daemon` and tool to
`/layerfs-bench/bin/layerfs-edit-tool`, then set the daemon entrypoint. The
unique image build command was:

```sh
docker build --no-cache -q -t layerfs-241-sdk-gate:8493d8f8 core/target/issue241-sdk-gate-image
```

The single mounted selection was:

```sh
LAYERFS_TEST_IMAGE=sha256:b5f63144fd930441365ee222db547c11214550c44f6c64bfbba236946b418ffc cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-sdk --test sdk_range_exec mounted_sdk_exec_confirms_before_explicit_commit -- --nocapture
```

The raw [attempt receipt](attempt-001/receipt.json) records exact argv,
environment override, exit 0, no timeout and complete command wall
2,899,247,583 ns. That wall includes Cargo test startup and the full SDK
lifecycle; it is **command accounting only**, with no carrier or product
latency claim. Another worktree may have run functional checks concurrently;
the parent confirmed no timed performance run was active. Cache state was
uncontrolled. The `exit 75` gate check ran inside the same one mounted
selection, without a second performance or functional sample.

The test source is under
[`core/crates/layerfs-api/sdk/tests/sdk_range_exec.rs`](../../../../crates/layerfs-api/sdk/tests/sdk_range_exec.rs)
and shares the public SDK Exec/Commit gate with the sweep through
[`support/range_exec_gate.rs`](../../../../crates/layerfs-api/sdk/tests/support/range_exec_gate.rs).
The helper's return is `(ExecResult, Option<WorkspaceCommitReportWire>)`, so
the sweep can own fixture, Branch and independent old/new Commit oracle.

Final checks at the committed test-source identity: `python3
core/tools/check_product_boundary.py` PASS (291 production files), `python3
-m unittest discover -s core/tools -p 'test_*.py'` PASS (9 tests), `cargo
+1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` PASS,
`env -u LAYERFS_TEST_IMAGE cargo +1.85.1 test --manifest-path core/Cargo.toml
--locked` PASS, and `cargo +1.85.1 clippy --manifest-path core/Cargo.toml
--locked --all-targets -- -D warnings` PASS on the macOS host. The full
workspace test command left `LAYERFS_TEST_IMAGE` unset, so its test discovery
ran the pure exit-status gate while the mounted selection remained the one
recorded attempt above. The locked Linux release daemon and edit-tool builds
both passed. No Linux-target Clippy command was run for this test-only
follow-up; the owning Linux binaries were built and exercised in the image.

## Gaps

The 8 KiB functional result does not prove 1/10/100/500 MiB completion,
position generality, old Commit retention, fresh-Branch readback, or the
Edit→Commit target. The tool's success assertion comes from its own bounded
readback and this test's route/status checks; this selection did not
independently rehash the complete committed file. Cleanup passed at the
public SDK `unmount` and `delete` boundaries; a separate Docker absence
snapshot was not retained. An actual mounted UNKNOWN caused by notifier or
reply failure remains to be tested through the public SDK. No benchmark
registry, product source or third-party package changed in this test-only
follow-up.
