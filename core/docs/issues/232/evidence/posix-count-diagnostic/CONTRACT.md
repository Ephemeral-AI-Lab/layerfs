# #232 generic POSIX shift count diagnostic v1

**Prospective, cause-finding only.** Source parent `c98e3147e`; this is not a
new performance arm or a replacement for any retained v2 receipt. Commit this
contract before adding telemetry or taking a live attempt. The source, product
and harness seals, locked binaries, image ID, prepared-master hashes and fresh
output paths will be frozen in `PRE_RUN.json` after the diagnostic build and
before either attempt. Keep every result, including failures, append-only.

Use exactly the frozen scenario-v2 **middle insert** POSIX `shift` commands at
1 MiB and 10 MiB, one attempt each, in that order. They read and write the
affected suffix in 128 KiB blocks, then write the 4 KiB replacement. The
original v2 registry SHA-256 is
`05a133b543db33729d60cc321cc88682046c16bfb1761daec6a57c5df3b11823`.
The two command strings, fixture sizes, replacement SHA-256 and final
full-file SHA-256 come unchanged from that registry. The image may reuse the
published v4 complexity-image builder because its insert-middle payload
has the **same** SHA-256
`568c5408a3f292d4a593d5ffa43736b790b6a5dac749427b0ad53c765e672616`;
the executed operation remains v2 POSIX shift, never ioctl. Build from this
worktree with locked release artifacts and record the actual image and tool
hashes. Reuse each closed, validated prepared master via an independent
writable byte copy outside the measured Exec→Commit interval.

| Diagnostic | Old file | Declared suffix shift | Declared command |
| --- | ---: | ---: | --- |
| `posix-count-insert-middle-1mib-v1` | 1,048,576 B | 524,288 B, four blocks | `/layerfs-bench/bin/layerfs-edit-tool shift --file payload.bin --offset 524288 --delete-length 0 --length 4096 --direction grow --expect-size 1048576 --payload /layerfs-bench/payloads/insert-middle-4k.bin` |
| `posix-count-insert-middle-10mib-v1` | 10,485,760 B | 5,242,880 B, forty blocks | `/layerfs-bench/bin/layerfs-edit-tool shift --file payload.bin --offset 5242880 --delete-length 0 --length 4096 --direction grow --expect-size 10485760 --payload /layerfs-bench/payloads/insert-middle-4k.bin` |

The product path is public SDK Project fork, Sandbox create, Workspace mount,
one `WorkspaceApi::exec` through `/bin/sh -c`, one explicit Commit, Status,
Unmount and public Sandbox delete. The host example may capture daemon logs
for this new diagnostic contract ID; it may not mutate the mounted file.
The daemon's existing `LFS_PIECE_COUNT v=1` and `LFS_PIECE_PAGES v=1` lines
must report each successful ordinary WRITE and size SETATTR. Status must
show POSIX WRITE and zero `range_state`/`range_edit`. Expose an additional
`LFS_FINISH_SUBSTEP v=1` line for the EditFile save using **existing**
`SaveOutcome.profile.diag` totals: whole flush batch, locator/presence wave,
offer, seal, pack write, validation and related nested parts. These fields
are save-wide, may overlap, and must not be summed into a duration. Only when
the save's total object count equals `finish_batch_objects` can they be
associated with that final drain without an earlier-wave subtraction.

The independent verifier reopens the published Store/history read-only and
checks full result SHA-256, size, head, pristine historical root and cleanup.
Record LFT1 and complete-command wall separately; timing is diagnostic only.
One construction worker, a 15 s complete-command ceiling and a separate
under-10 s verifier apply. The Linux FUSE backing cache remains uncontrolled,
so both rows are **INELIGIBLE** for cold performance admission regardless of
raw timing. A failure stays retained, with no retry or changed workload to
make a cell pass.
