# Issue 266: shared public count route clarification

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This append-only clarification follows the committed #266 count and treatment
specifications. It governs the report and any future changed-source selection;
it does not edit, replace or promote the retained #261 512 FAIL or any #266
receipt already taken.

The 100-, 512- and 4,097-write selections are counts on the **same**
`core/benchmark/fs-bench-pro/separated_writes.py` route. That runner invokes
the existing `benchmark_shell` example, which makes one public
`WorkspaceApi::mount`, one `WorkspaceApi::exec` with the generic
`write-separated` shell command, and one explicit `WorkspaceApi::commit` only
after a successful Exec. The one process keeps one fd and issues a one-byte
positional write at offset `2*i` for `i=0..N-1`. No 512-only mutation API,
driver, writer or control RPC exists. Actual FUSE WRITE callback counts come
from Workspace Status; they are not inferred from writer syscalls.

For every declared count `N`, the independent verifier derives the full
8,194-byte expected file by changing those `N` even offsets, checks the old
and new heads and parent, all bytes, and exactly `N` changed runs. The expected
final extent count is `2*N`, so the 4,097 selection covers the last even byte
at offset 8,192. Distinct scenario IDs and append-only receipts identify the
100, 512 and 4,097 selections and each source identity. The old 100 functional PASS and
512 FAIL are not rerun. The #266 post-reply diagnostic changes only optional
snapshot ordering to expose the refusal branch; the mutation route stays the
same.

This managed worktree initialized its own sealed copy of the same 8,194-byte
fixture once because it cannot use another worktree's result root or target.
Every later #266 selection reused that closed, verified master and made an
independent writable byte copy. A clone is not a cold-cache claim. The #266
4,097 attempt at source `a2e986a06` already failed its existing 25-second
complete-command bound; it is not repeated under this clarification. A future
attempt needs a relevant frozen source change and its own prospectively
declared identity, with the unchanged workload, deadline, cache and worker
rules.
