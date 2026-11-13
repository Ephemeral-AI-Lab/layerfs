# Owner authorization: fuser receive buffer sized by the negotiated max_write

> **Status:** Dated decision record; not release evidence or a product contract.
> Owner decision 2026-10-14, after the dense-rewrite daemon peak investigation.

## Question and answer

The [peak investigation](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/320/PROGRESS.md#issue-321-follow-up--where-the-dense-rewrite-daemon-peak-comes-from-2026-10-14)
traced about 48 of the 147 MiB daemon peak to fuser's receive buffers.
- Each mount runs 2 receive loops.
- Each loop allocates `vec![0; 16 MiB + 4 KiB]`, fuser's own maximum write,
  although LayerFS negotiates `max_write` = 128 KiB.
- After the first unmount frees such a buffer, glibc raises its dynamic mmap
  threshold. Every later buffer is then zeroed in an arena and stays resident:
  about 32 MiB per Workspace mounted at the same time, never returned.

The owner was offered three fixes:
1. size the buffer by the negotiated `max_write` in the pinned fuser copy;
2. fix glibc's threshold with an `unsafe` `mallopt`;
3. tune the container environment.

The owner answered: **"you are allowed to modify fuser with patch"**.

## Scope

This authorizes one more delta to the pinned fuser 0.18.0 copy. It is applied
after the timestamp and receive-loop lifecycle patches, and both of those
stay unchanged.
- The `Session` remembers the receive size from INIT:
  `max_write + 4096`, from `read_buf::negotiated_size`.
- Each receive loop allocates that size. Before INIT, and in the handshake
  itself, fuser's full `16 MiB + 4096` stays.
- The kernel requires a read buffer of at least
  `max(8192, sizeof(fuse_in_header) + sizeof(fuse_write_in) + max_write)`,
  and it never sends a write larger than the `max_write` it accepted. Every
  other request is smaller.

Files: `src/read_buf.rs`, `src/session.rs` and `src/session/lifecycle.rs`.
`core/patches/fuser-0.18.0/receive-buffer.patch` and
`receive-buffer-provenance.json` pin this delta. The focused
`core/tools/check_fuser_integrity.py` verifies it after the two earlier
records. No other dependency change and no `unsafe` exception is authorized.

The two passthrough harness refusals the checker reports (OWNER-3) are an
earlier, separate disposition. This record does not change them.
