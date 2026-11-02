# Handoff: continuous simplification, removal, rename and optimization

> **Status:** Current assignment (owner, 2026-10-14). Branch
> `overlay-open-state-cuts` in worktree
> `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-perf-run`, unpushed. Tracked in
> issue #320 (the per-Workspace overlay work was #321, closed).

Current continuation (2026-10-15): product sweep and separate test pass are
complete at product source `eaaf0de48`. Terminal functional checks pass with
explicit prerequisite/performance gaps in [PROGRESS](PROGRESS.md). The owner
superseded the earlier reader rejection condition: prefer per-Workspace readers
with independent <=10% wall-time and observed peak RSS allowances. The prior
samples remain INELIGIBLE; the shared-pool restoration below is historical.
Per-Workspace readers are reintroduced and functionally verified. See the
[reader design](WORKSPACE-READER-DESIGN-20261014.md) and
[test triage](TEST-TRIAGE-20261014.md). Do not repeat passing unchanged treatments
or regenerate benchmark originals without the specifically required approval.
No further outward action is authorized by this continuation.

## 1. What the owner asked

In the owner's words:
- 2026-10-14: "remove aggressively including the tests".
- 2026-10-14: "work as you recommended, including continous simplification, aggressive removal, rename and optimization".
- 2026-10-14: "update the docs for the aggressive change".
- 2026-10-14: have "a sweep of update then work on test, and assume many test can be dropped rather than wasting time to fix the ones that could be dropped".

Owner decisions of 2026-10-14:
- per-Workspace Store readers are approved with conditions (4.2.5);
- the kernel keeps permission checks (`default_permissions`; 4.5.2);
- runtime-only overlay rows move to memory (4.2.3);
- no `native` names are left in product code, SQL included (4.3).

The recommendation is section 4. Its order:
1. Verify and land the branch.
2. Phase 1, the product sweep (4.0): remove what the architecture shift made redundant (4.2), then rename (4.3).
3. Phase 2, the test pass (4.0): delete by default, and fix only the behaviour on the keep list.
4. Then optimize, from measurements.

Work in small counted commits and keep simplifying between them. A removal that only moves code elsewhere is not a removal.

## 2. Where things stand

Two commits on `overlay-open-state-cuts`, on top of `main` at `265843629`:

| Commit | Change | Production LOC |
| --- | --- | --- |
| `11b2a452d` | Open-state maps keyed without the constant namespace; files, reads, lookups and directory handles share one journaled `OwnerTable`; Status open-state counts are map lengths | 80,476 -> 80,299 (-177) |
| `253291435` | Product code only tests reached, removed with the tests that existed for it; elsewhere only the assertions that called a removed observer | 80,299 -> 79,447 (-852) |

Checks run on `253291435`:
- fmt, `python3 -B core/tools/check_product_boundary.py` and the 50 tool tests: pass.
- Whole-workspace Clippy `-D warnings`, host and Linux: clean.
- Host: all 315 test binaries, 30 s each; 1,312 passed, 4 ignored, none timed out. Three failed:
  - two need inputs that aren't on the host: `complete_installed_roots::huge_native_namespace_is_complete_after_install` needs `LAYERFS_Q1_PREPARED`, and `host_handoff::macos_init_handoff_to_linux_daemon_has_no_host_data_path` needs a Linux daemon binary;
  - the third, `captured_namespace::reader_name_and_symlink_jobs_seek_their_sealed_domain_while_the_active_generation_grows`, was fixed by giving the reader's first job its own untimed call, and then passed 5/5.
- Linux: all 43 binaries listed in `core/target/issue321/b-linux-tests.txt` pass.

**Not run:** the fs-bench-pro engine and families suites and the cluster-two suite. They need a sealed build's native inputs and a clean tree.

### Kept on purpose

Only tests call these, but removing them would delete whole suites or a custody or operator capability. Do not remove them without asking the owner:

| Kept | Why |
| --- | --- |
| `Workspace::view_for_source`, `CanonicalClient::with_lengths` | Constructors that about 13 workspace and daemon test modules and a daemon example build on. Replacing `with_lengths` would paste its construction into about 16 test call sites, which the counting rule forbids. |
| `receive_install`, `Service::serve_one`, `serve_one_control` | The entry points of the install tests and the control-protocol fixture. |
| `TimingNode::with_children`, `PackUnit::from_ranges`, `Store::recognizer_work` | How the telemetry, storage and recognizer tests build trees, implement the pack port and observe the recognizer. |
| `Store::stop_reads` and `ReadPool::stop` | The documented whole-Store shutdown. Without them, the read pool's `stopping` state, `ReadAdmissionError::Stopped` and a diagnostics field are set by nothing. |
| `MountRequests::inspect_failure`, `Store::reader_failures` | The only readers of retained failure custody. |
| `Service::retained_native` | Operator observation of retained teardown custody. |
| `native_retained_mount`, `native_state` (daemon and overlay) | The Linux mounted proofs use them: FORGET underflow, detach stages, forced unmount, `native_custody`. |
| `take_connection_failure` | The documented operator custody transfer (from an earlier pass). |

## 3. Binding constraints (unchanged)

- **Checkouts.** Never edit the main checkout `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` or another agent's worktree; `sdk-refactor` belongs to another agent. Gate every build or run on `bash core/target/issue320/busy.sh` and its exit status. Call helpers by absolute path, or the gate sees your own process as busy.
- **Store profile.** Disposable / WAL / synchronous=OFF only. Durable execution: `NOT_RUN — disabled by owner until explicit reauthorization`. Export `LAYERFS_CONSTRUCTION_WORKERS=1`.
- **Toolchain.** Host cargo is `+1.85.1 --locked`. Repeat the profile flags if you set RUSTFLAGS.
- **Time limits.** No command over 2 minutes. Every test gets an explicit limit of at most 120 s. Never retry, loop or background a test, and diagnose before any rerun. One sample per case and source identity; timings on this shared machine are INELIGIBLE.
- **Code rules.**
  - No `unsafe`.
  - Patch third-party code only within the authorized fuser patches.
  - New production files stay under 999 lines.
  - Don't restore `tools/preflight.sh` or anything like it.
  - Never retry an operation or guess a resend.
- **Commits.** `git add -A`, then `python3 -B core/target/issue321/commit.py <message-file>`. It adds the Production LOC line, the line "reference absent (retired R9c; last counted 65,417 at tree 498dd1917812)" and the attribution line.
  - **Counting rule:** never move product code into tests or tools to lower the count.
- **Outward actions.** Every push, merge, issue post or other outward action needs the owner's explicit OK each time.
- **Pins.** Editing a pinned overlay source changes its sha256 in `core/benchmark/fs-bench-pro/engine/oracle.py`; update the pin and run the engine suite. Pinned today: `database/accounting.rs`, `database/physical.rs`, `lifetime/close.rs`, the two overlay SQL files, daemon `overlay/workspaces.rs`, bridge `control_types.rs`.
- **Known state.** Don't "fix" OWNER-3's two passthrough refusals in `check_fuser_integrity.py`; that's a known state on main.

## 4. The work, in order

### 4.0 Method: sweep the product first, then the tests (owner, 2026-10-14)

The owner asked for one sweep of product changes first, then a separate test pass, assuming many tests can be dropped rather than spending time fixing ones that could be dropped.

**Phase 1: product sweep (4.2 and 4.3).**
- Change product code only: `src/`, SQL and the harness readers that parse product receipts.
- Gate each step on product targets alone: `cargo +1.85.1 check --locked --workspace --lib --bins --examples`, plus Clippy on the same targets, on the host and on Linux.
- Do not open, edit or run a test during this phase. Commit each step as a counted commit and write `tests: not yet updated` in its message. The branch is unpushed, so broken test targets in between are acceptable.
- Re-pin `oracle.py` whenever a pinned source changes.

**Phase 2: test pass.**
1. Compile every target once: `cargo +1.85.1 test --locked --workspace --no-run --keep-going`. List each failing test file with its error count.
2. **Triage each failing file. The default is to delete it.** Keep and fix a test only if it covers behaviour from the list below and no surviving test or mounted proof already covers that behaviour:
   - POSIX filesystem semantics: identity, permissions, rename, unlink while open, hard and symbolic links, large and sparse files;
   - a Commit while commands keep writing: the capture boundary, install keeping later writes, and the base staying readable for open readers;
   - lost-reply recovery through `commit_record`, and exact refusal returning its input;
   - reclamation after the last close and after unmount, including during live activity;
   - bounded memory and streaming for large files and huge namespaces;
   - the Linux mounted proofs, and the fs-bench-pro engine, families and cluster-two suites.
3. **Delete without fixing** tests that check internal mechanisms. These include:
   - job envelopes, `Response`/`NativeReply` unpacking and `JobReceipt` fields;
   - statement counts, query plans and index shapes;
   - pin, lease and accounting rows, and `Resources` row counts;
   - read pool fairness, `ServiceClass` labels and route checks;
   - counted-work tests whose numbers the removed mechanisms produced.

   Also delete a test that duplicates a mounted proof.
4. **Time box: about 5 minutes per test.** If a test outside the keep list needs more than a mechanical edit, delete it. If a test on the keep list resists, record it as a gap; don't rebuild the mechanism it tested.
5. List every deleted test file in the commit message with a one-line reason, and state the coverage given up. Deleting tests does not change production LOC, but the deletion is still reported.
6. Then run the whole suite once, as in 4.1, plus the measurement subset in 4.4.

This replaces the earlier lesson "remove only the calling statement". Within this pass, deletion is the default and fixing is the exception. Deleting knowingly still matters: name what you drop.

### 4.1 Verify and land (first)

1. Seal a build of `253291435`; it takes about 105 s, use `core/target/issue321/go.py`.
2. Run the fs-bench-pro engine and families suites with that build's native inputs, then the cluster-two suite (`unittest discover core/benchmark/cluster-two/tests`). Each runs once.
3. Record the result in `core/docs/issues/320/PROGRESS.md`.
4. Ask the owner to push to `origin/main`.

### 4.2 Remove what the architecture shift made redundant

#321 gave each Workspace its own overlay file and replaced the owner thread, queue, credits and completion tracking with one lock per Workspace. It moved 11 tables to memory and dropped `reclaim` (26 tables down to 12). What remains below either served the old shared database and single writer, or is state that ends with the daemon but still sits in SQLite. The overlay file is deleted at unmount and never outlives the daemon, so a row that only tracks something in flight can become journaled memory, as the open state did in Step A. Cheapest first:

1. **`Route` and constant namespaces.**
   - 38 overlay functions take `route: Route`. Handles (`OpenFile`, `BaseSource`, `LookupOwner`, `FileRead`, `Capture`, …) carry a route that is checked on every call. One `Overlay` holds exactly one Workspace, so these checks always pass. Drop the parameter, the checks and the helpers that took it only for that. Routing stays in the daemon, where the route picks the Workspace.
   - Drop the remaining constant `ns`:
     - `workspace.ns AUTOINCREMENT`;
     - `maintenance::Item.ns`;
     - reply tickets keyed `(ns, revision)` in `lifetime/tickets.rs`;
     - `count(Some(ns))` and `pending(ns)`;
     - the `mounts` map, which becomes one journaled slot.
   - The mechanics are the same as `11b2a452d`: change the key, follow the compiler, then follow unused-parameter warnings.
2. **Generic leases (test-only).**
   - `Overlay::acquire`/`release` and the daemon's `acquire`/`release` jobs (`overlay/lifetime_operations.rs`) are the only producers of `LeaseKind` `Reader`, `Open`, `Lookup` and `Operation` (kinds 1–4). No product code calls them; 7 test files do.
   - Remove them with those kinds, and remove kind 1 from `GENERATION_HELD`.
3. **Runtime-only rows to memory.** Move each one; drop any that has no remaining user. The target is 6 tables (`fs_inode`, `fs_entry`, `fs_data`, `fs_data_ref`, `fs_truncate`, `commit_record`), 0 triggers and no custody rows. Trace each move first; record any that doesn't hold and leave it.
   - **`accounting` table and its 27 triggers** (`sql/accounting.sql`, `database/accounting.rs`): drop. A created file fires three of them. Only the `Resources` diagnostic reads them; compute its counts on demand with `count(*)` and `sum(length(…))`. The numbers stay identical, so reclamation proofs keep their assertions. Update the harness storage oracle and the pins in the same commit.
   - **`workspace` (one row)**: make `active`, `captured`, `installed`, `revision`, `lifecycle`, `dirty_*`, `base_readers` and `consolidating` journaled fields of the overlay, committed and rolled back with the SQL transaction (`JournalMap`/`OwnerTable` already do this). This removes `UPDATE workspace SET base_readers±1` from every base read.
   - **`commit_run`, `commit_reader`, `commit_pin` (kinds 6 and 8 left after item 2), `commit_base_pin`**: in-memory maps keyed by holder: Commit runs, Commit snapshot readers and base readers. `GENERATION_HELD` becomes a map lookup. Their meaning must not change:
     - `CapturedReader` stops cleanup from folding or deleting the captured generation while a Commit reads it;
     - `Processing` and `commit_run` let a Commit be found again by its request id after a lost reply;
     - the base pin keeps the old base root readable for open readers after install moves the base forward.

     Together these are what let a Commit run while commands keep writing.
   - **`cleanup_task`** (12 kinds): an in-memory list. This changes how a failed cleanup step recovers: write a short design first (what is retried, and what stays as exact custody) and keep the automatic, bounded reclamation during live activity. This was §4.5.5.
   - Rename `LeaseKind`/`Lease` and the `StatementKind::Lease` paths to whatever remains (for example `Pin`), or delete them if nothing remains. The daemon's "global, route-free" `Resources` job becomes per-Workspace.
4. **Job/receipt layer → direct typed calls.**
   - Today every daemon call goes through `owner_job`/`overlay_job` closures that return the `Response` or `NativeReply` enums plus a `JobReceipt` (157 mentions in daemon `src`).
   - Replace this with methods on the Workspace handle: take the lock, call the overlay, return the typed result, and count the work in the same place.
   - Keep the exact-refusal rule: a call refused before any effect returns its original input, as `OwnerError::Unattempted` does. Keep the frontier wait, the readiness checks and the counted work.
   - Shrink `ServiceClass` to what still changes behaviour: Mutation and Source drive the frontier wait; the rest only label diagnostics.
   - This is the largest remaining cut. About 470 test sites depend on the envelope; per 4.0 most of those tests are deleted, not converted.
5. **Store read pool → one reader per Workspace.**
   - `store/read_service.rs`, `read_state.rs`, `read_handle.rs` and `read_scope.rs` (639 lines) are fair admission to shared Store readers: per-Workspace lanes, FIFO rotation and a 17 Workspaces × 18 requests quota. With one reader per Workspace there is nothing to arbitrate.
   - **Owner decision (2026-10-14): approved, with conditions.** This reverses part of the #308/#314 decision of one shared pager. The conditions:
     - **Bound each connection's page cache** to a few MiB. The OS page cache sits underneath, and an unbounded cache per Workspace would eat into the per-Workspace memory budget.
     - **Keep read transactions short**, one per request or step. A snapshot held open blocks WAL checkpoints of the shared Store while Commits publish.
     - **Open the reader at mount and close it at unmount.** Never open one lazily in the middle of a request.
     - **Accept only on measurement.** Use the 4.4 subset: `exec_concurrency.py` at 1 and 4 Workspaces plus cell C06, before and after. Record peak memory with `core/docs/issues/321/tools/daemon_memory_sampler.py`. Keep the pool if time at 4 Workspaces worsens or the peak leaves the per-Workspace budget, and record why.
6. **Continue the reachability passes** after each structural cut: the dead-code and test-only pass from `253291435`'s message, over product `src/` only, counting trait methods and functions the benchmark harnesses call as live. Compile Linux-only code with the Linux check, since the host never builds it.

### 4.3 Rename: no `native` left in product code (after 4.2.4, which deletes many of these names)

"native" has four meanings, so it gets four replacement words. Apply them to file and module names, types, functions, fields, error and label strings, SQL tables, columns and query files, test file names and the SDK example. Leave historical docs, receipts and ledgers unchanged. Collisions with existing `Kernel*` or `Mount*` names are caught by the compiler; choose a more specific name there. Each group below is one commit with no behaviour change. Update the oracle pins and the docs that name these items.

1. **The kernel's view of the mount → `kernel`** (overlay, workspace, daemon, fuse, api). The rule is mechanical: `Native…` → `Kernel…` and `native_…` → `kernel_…`. The overlay already says `KernelReference`.
   - Types: `NativeMount`, `NativeMountState`, `NativeSession`, `NativeServing`, `NativeVisitRequest`, `Native{Read,Mutation,Directory,Data}Visit`, `NativeDirectory*`, `NativeCookieOffer`, `NativeWindow`, `NativeEffect`, `NativeDecision`, `NativeObservation`, the fuse `NativeFilesystem`/`NativeRead*`/`NativeMutation*`/`NativeData`, and the daemon `NativeReply`/`NativeWork`/`NativeReceipt`/`NativeStatus`/`NativeConfig`/`NativeFailure`/`NativeEvidence` (those still alive after 4.2.4). Each becomes `Kernel…`.
   - Functions: `native_visit`, `open_native_visit`, `create_native_mount`, `revoke_native_mount`, `retained_native_mount`, `unmount_native`, `with_native`, `native()` → `kernel_visit`, `open_kernel_visit`, `create_kernel_mount`, `revoke_kernel_mount`, `retained_kernel_mount`, `unmount_kernel`, `with_kernel`, `kernel()`. Collapse the doubled forms (`forget_native`/`native_forget`, `close_native_file`/`native_close_file`) to one `kernel_…` name each.
   - Files: the 15 `native*.rs` source files (`lifetime/`, `operations/`, `contract/`, `control/native.rs`, `overlay/native_operations.rs`, `diagnostics/native.rs`) and 19 test files (`native_mount.rs`, `native_visit.rs`, …) become `kernel*.rs`. The SDK example `sandbox_native_mount.rs` becomes `sandbox_kernel_mount.rs`.
   - Strings: the `"native"` label (8 sites) and messages such as `"native teardown"` become `"kernel…"`. The fs-bench-pro readers (`receipts.py`, `oracle.py`, `workspace.py`, `identity.py`, `prepared_inputs.py`, `project.py`, `fixtures.py`, recipes) change in the same commit.
2. **Host files at Project Init → `host`** (project, persistence, storage acquisition port).
   - `NativeIdentity` → `HostIdentity`, and the field `native` → `host`.
   - `native_path`, `natives`, `native_rows`, `native_bytes`, `native_statement`, `native_binding_bytes`, `native_one` → `host_path`, `host_files`, `host_rows`, `host_bytes`, `host_statement`, `host_binding_bytes`, `host_one`.
   - `put_native`, `bind_native`, `remove_native`, `unplaced_native`, `discard_native*` → `put_host_file`, `bind_host_file`, `remove_host_file`, `unplaced_host_file`, `discard_host_file*`.
   - **SQL**:
     - the table `init_native_file` → `init_host_file`, and its index → `init_host_file_identity`;
     - the columns `native_path` → `host_path` and `native` → `host_identity`;
     - the query files `put_native.sql`, `bind_native.sql`, `discard_native*.sql` → `put_host_file.sql`, `bind_host_file.sql`, `discard_host_file*.sql`.

     This changes the Store schema. The owner allows format changes for #320. Commit it last, and first check whether prepared benchmark originals still open. If they must be prepared again, say so and ask before spending the disk and time.
3. **Storage pack lane → `chunk`.** `PackLane::Native` (chunk payload records, beside `Ordinary`, `WholeFile` and `Singleton`) → `PackLane::Chunk`, and `VERSION_NATIVE` → `VERSION_CHUNK`. Only the Rust names change; the on-disk version number stays.
4. **Bridge → `mount` and `duplex`.**
   - `control_native.rs` (host↔daemon attach records) → `control_mount.rs`, and `NativePhase`, `NativeReceipt`, `NativeStatus`, `NativeWork` → `MountPhase`, `MountReceipt`, `MountStatus`, `MountWork`. This keeps them distinct from the daemon's `Kernel*`.
   - `src/native/` (the authenticated duplex channel) → `src/duplex/`; `put_native` and `tests/native.rs` follow.
5. **Prose.** Comments in telemetry, sandbox and project ("native assembly", "native process start identity", "native mounting") become "platform", "host" or "kernel mount" by meaning. The error "native telemetry requires…" becomes "platform telemetry requires…".

Done means `grep -rnw -e 'native' -e 'Native' --include='*.rs' --include='*.sql' crates/*/src crates/*/sql crates/*/tests` finds nothing, and `find crates -name '*native*'` finds no file.

### 4.4 Expectations and the measurement subset (owner: "we should just run a small subset")

Production LOC expected from 4.2. These are estimates from file sizes at `253291435`, not commitments:

| Item | Expected cut |
| --- | --- |
| Overlay `Route` parameter, constant namespaces, `ServiceClass` | 110–210 |
| Generic leases, kinds 1–4 (test-only) | 40–80 |
| Runtime-only rows to memory: `accounting.sql` (104 lines) and 27 triggers, `workspace` row, Commit pins and runs, `cleanup_task` | 150–400 net (memory maps replace SQL and statements) |
| Job/receipt layer: the four `*_operations.rs` (789), `results.rs`, `receipt.rs`, `job_sql.rs`, `owner_work.rs`, part of `owner.rs` and the ports | 400–700 net |
| Store read pool (639 in the four `read_*` files) → per-Workspace reader | 450–550 net, if it passes the 4.2.5 conditions |
| Further reachability passes | 0–200 |
| Rename | 0 |

That is about 1,150–2,200 lines, or 1.5–2.8% of 79,447. The optimizations in 4.5 may add code (memory-served GETATTR, write gathering). Report that growth honestly.

Expected speed, based on the #321 record in `core/docs/issues/320/PROGRESS.md`:
- In the twelve cells at Step E, statements fell 25–57% while command time stayed flat. Owner jobs now equal kernel requests. So command time is set by kernel round trips per request, not by SQL or the job layer.
- In tier 1, statements fell 57% while the command fell 10% and the Commit 53%.
- Expected effect of each change:

| Change | Expected effect |
| --- | --- |
| 4.2 removals and the rename | about 0 on command and Commit (a few % CPU at most). 4.2.3 removes statements and trigger runs per write, create and read, but #321 showed statements are not what limits command time |
| Per-Workspace readers | less waiting only when several Workspaces read the Store at once |
| GETATTR | 22% of requests in the tier-1 sample. At C01's ~68 µs per request (339 ms / 5,002), 5,766 GETATTRs are roughly 0.2–0.4 s of the 6.7 s command, about 3–6%. Arithmetic, not a measurement; memory-served GETATTR saves only the daemon's share of that |
| Commit step sizes | perhaps 10–30% of Commit time, unmeasured |
| Write gathering | small for command time, since statements are not the bottleneck; measure before designing further |

Measure only this subset, one sample each, at three points: the branch tip before 4.2, after 4.2, and after each optimization. Each item is one command of at most 2 minutes:
1. The cells C01 (create-heavy, 5,002 requests), C03 (mixed) and C06 (small) through `go.py run cells:C01 …`.
2. `core/docs/issues/320/tools/exec_concurrency.py`, at 1 and 4 Workspaces.
3. Tier 1 `v016-mixed-development-100mb-5000-k100-v1`: command and Commit, with request, job and statement counts.

Don't run the full tiers or the four-Workspace context case (its preparation order is fragile). Counts are the evidence; times on this shared machine stay INELIGIBLE.

### 4.5 Optimize (measure first)

Priority from the owner (2026-10-13): exec under concurrent Workspaces first, then Commit, then mount. Don't chase a few milliseconds on a Commit, and never buy speed with storage, unbounded memory or benchmark-fitting.

1. **Baseline.** After 4.2, measure time per small file for 1 and 4 Workspaces (`core/docs/issues/320/tools/exec_concurrency.py`) and a Commit beside writers, one sample each. The last known numbers (650–750 µs per small file against 42 µs on the native filesystem) predate #321.
2. **Kernel round trips.**
   - GETATTR was 5,766 of 26,078 requests in the Step C tier-1 sample.
   - The 6.12 kernel invalidates a parent directory's attributes on every create, unlink or rename, and `default_permissions` then asks again. A longer attribute timeout can't prevent it.
   - **Owner decision (2026-10-14): keep `default_permissions`; the daemon does not take over permission checks.** A FUSE request carries only uid, gid and pid, not supplementary groups. Reading them from `/proc/<pid>` is racy, and copying root, sticky-bit and setgid rules adds security-sensitive code against the permission-preservation contract. The estimated gain (about 3–6%, see 4.4) does not justify it.
   - Instead:
     1. Measure GETATTR's real time share in the tier-1 sample, using the existing request counters.
     2. Make GETATTR as cheap as possible: answer from attributes already held in memory, with no SQL statement and no job beyond the lock. This saves daemon work, not the kernel round trip.
     3. Bring daemon-owned checks back to the owner only if the measured share is well above about 5%.
3. **Statements per write.**
   - Each FUSE WRITE is a 7-statement transaction, and a created file is 18 statements.
   - Gathering a file's writes in memory and writing them in one transaction (on release, before a Commit captures, or at a size bound) would cut statements for files written in many pieces.
   - The overlay is disposable, so durability is not the obstacle; exact counts and proof re-pins are. Write a short design first.
4. **Job sizes.**
   - Commit plan steps and cleanup pages were sized for one writer shared by every Workspace.
   - They now only need to keep this Workspace's own FUSE requests flowing during its Commit, a rule the owner kept.
   - Measure Commit time and the latency of concurrent requests at two or three step sizes before changing anything.
5. **In-memory cleanup list.** Moved into 4.2.3.

## 5. Lessons from this pass

- **Removing a function.**
  - Delete the product function, then compile `--workspace --all-targets --keep-going`.
  - For each test error, remove only the statement holding the call, and follow the follow-on "cannot find value" errors.
  - Delete a whole test only when the test exists for the removed function, such as plan, index-shape and envelope tests.
  - A whole-test delete over one incidental assertion lost about 2,000 lines of unrelated coverage and was reverted. In the phase-2 test pass (4.0), deletion is the default: check the file against the keep list, then delete it and name it.
  - Check that removed calls were observers. An action, such as a release or a stop, changes what later assertions see; that test either goes whole or keeps the function.
  - Watch counted-work tests: a removed call may have paid a one-time cost before the measured loop. Give that cost an explicit untimed call with a comment.
  - Look for comments the removal left orphaned.
- **Restoring a removed function.** If removing something makes a product field or state write-only, or empties the only reader of retained custody, restore it. The scratch helper that re-inserts a function from `HEAD` is lost with the session's scratchpad. Re-insert by hand from `git show HEAD:<file>` and check its placement; one restore landed inside a struct definition.
- **Linux builds.** A Linux build killed at its 120 s limit can leave broken incremental objects (undefined `anon.*.llvm.*` symbols at link). Remove only that test target's `incremental/<name>-*` directory and its `deps/<name>-<hash>*` objects under `core/target/issue320/linux-test/debug`, then build again. Build before testing, in pieces if needed.
- **Helpers** (ignored, under `core/target/issue321/`):
  - `t_hostrun.py <from>` runs every host test binary from `t-host-bins.json`, 30 s each, and starts none after 85 s. Regenerate the list from a `cargo test --no-run --message-format json` build.
  - `lbatch.sh <tag> <from>` runs the 43 Linux binaries.
  - `clippy.sh`, `check.py` and `core/target/linux_step.py` run Clippy and checks on the host and in the pinned image.

## 6. Reporting

Each commit message carries the Production LOC comparison, with removed, kept and restored items named and the reason for each kept one. Each section of `PROGRESS.md` states:
- what was measured, with source, build and cache state;
- what was not run, and why;
- every failure, as it happened.

Report a removal that doesn't pay as such, and stop it. Ask the owner before:
- every push;
- preparing benchmark originals again for the 4.3.2 schema rename;
- bringing back daemon-owned permission checks, if GETATTR measures well above 5%.

The test pass reports deleted test files and the coverage given up (4.0).
