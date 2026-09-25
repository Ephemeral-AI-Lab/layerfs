# #232 repeated-128 changed-source progress proof

**Status: prospective, frozen before the changed-source live attempt.** This is
one functional proof of the #232 Phase 1C `repeated-128` route after the Exec
control-progress change based on `7137cd7ecc8d6d44c24eccbd277a1366269d62ee`.
It does not replace either failed historical receipt or reopen the completed
56 single-edit campaign. The performance timer is diagnostic and cold-latency
admission remains unavailable for Linux FUSE backing.
The selected mutation is the opt-in cooperating `splice-batch` ioctl tool; this
proof makes no claim about arbitrary shell-command editing through Exec.

Use the sealed 1,048,576-byte Phase 1C master (Store SHA-256
`d57c2d300c4bc3e0b90df27073b931436937383bf63e3f6d65cf54f63bd73ec8`,
history SHA-256
`d4a8b08c9845ac029532d2bbb72e7635213390e7f76131ebe9a7a399f38d1f9a`)
and make an independent writable byte copy before the sample. Run one release
public SDK Exec of the frozen mounted `splice-batch --count 128` command, then
one public SDK Commit. All 128 32-byte insertions retain the frozen offsets,
payload SHA-256 `568c5408a3f292d4a593d5ffa43736b790b6a5dac749427b0ad53c765e672616`,
stamp checks, `fstat` and bounded readback. Expect 256 STATE and 128 EDIT
callbacks, 4,096 accepted literal bytes, zero shifted suffix bytes and zero
edit-caused FUSE WRITE. The resulting file has 1,052,672 bytes and SHA-256
`9895e8e40eb779da8c84625c77bed9a335fbe06a62c85cdb03c5846592949153`.

The changed daemon may send at most one authenticated control progress marker
per second only after a Workspace revision advance; its LFT1
`daemon.exec_progress` child counts marker sends. A marker is never an Exec or
Commit acknowledgement. The bridge's 5-second no-progress boundary, SDK Exec
30-second absolute deadline, FUSE callback budget and single construction
worker stay unchanged. The route must deliver typed Exec, then Commit, public
Status, Unmount and Sandbox Delete. An independent verifier must reopen the
Store/history and check full resulting bytes, root/representation, mode/mtime,
retained old Commit and reopened Branch. Verifier runs separately in under
10 seconds. The complete performance command, including lifecycle and cleanup,
must finish within 15 seconds. A failure, unknown outcome or missing verifier
is FAIL and remains on disk.

Pin exact source/tree, product/compilation/harness/workload seals, binary and
image hashes, prepared-master and clone hashes, scenario ID
`repeated-128-progress-proof-v1`, command and output path before the run.
Use one fresh output path and one attempt only. Report every non-passing line.
MacOS Store pre-sample residency is checked; Linux FUSE backing remains
`INELIGIBLE` for cold latency until invalidation and whole-input residency
checks exist. No deadline inflation, worker-count change, cache priming,
receipt rewrite or retry is permitted.
