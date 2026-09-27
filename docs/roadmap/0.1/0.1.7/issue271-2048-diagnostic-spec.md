# Issue 271: one 2,048-write cost diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This selection is specified before adding it to the existing runner or
executing it. The branch-only 32-child source `94d86303ca6748a33b5a8bf7ce012ef9325646d0`
missed its single 4,097 public gate at the unchanged 25 s complete-command
bound, before it returned a callback count. The gate remains `FAIL`; a runner
change cannot license a retry of that source's 4,097 arm.

Use the same closed 8,194-byte master, independent writable byte-copy clone,
locked-release `benchmark_shell`, generic one-process/one-fd writer and
separate full old/new-head verifier as the retained 100/512 rows. One public
`WorkspaceApi::mount` → one `WorkspaceApi::exec` → one explicit
`WorkspaceApi::commit` issues 2,048 one-byte positional writes at even offsets
of the existing file. The old and expected new file remain 8,194 bytes;
exactly 2,048 changed runs and actual FUSE WRITE callbacks are required.
No internal mutation call enters the driver.

Set the existing diagnostic FUSE/ledger snapshot interval to **512 accepted
WRITEs**, yielding checkpoints 512, 1,024, 1,536 and 2,048. The last is just
beyond the 32-leaf branch transition expected from two separated extents per
write and a 124-record leaf target. Capture cumulative 4 KiB ledger reads and
writes, child and Local edges, root height, extent page visits/writes,
payload/metadata charges, callback count, Store size, Exec/Commit/complete
wall, verifier and cleanup. Record missing checkpoints as missing rather than
interpolating them. This is a count-driven diagnostic, not a second gate arm.

Keep the ordinary **15 s complete-command limit** and separate under-10 s
verifier, one construction worker, unchanged product Exec timer, and
uncontrolled-cache `INELIGIBLE` latency status. The runner may add one named
selection and pinned spec hash but must reuse the same writer, driver, master
and clone code. One attempt at a frozen source/harness/image identity, a fresh
output path, and append-only FAIL/INELIGIBLE evidence. If the command times
out, capture only this attempt's owned daemon and container logs, then remove
only its owned resources. Do not rerun an unchanged arm to obtain a result.
