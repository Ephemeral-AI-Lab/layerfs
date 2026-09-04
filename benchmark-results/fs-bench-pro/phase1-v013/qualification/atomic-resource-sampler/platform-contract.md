# Atomic sampler row contract

Only `workspace_registry.rs::sample_resources` changed. All other bytes in that module and the workload bodies are unchanged, as checked against HEAD by model.py.

Linux defines PIPE_BUF as 4096 bytes in [the kernel UAPI limits header](https://raw.githubusercontent.com/torvalds/linux/master/include/uapi/linux/limits.h). The [Linux man-pages pipe(7) contract](https://man7.org/linux/man-pages/man7/pipe.7.html) specifies atomic complete writes for blocking descriptors when the write length is at most PIPE_BUF.

The sampler opens /proc/self/fd/1 with OpenOptions write mode and no O_NONBLOCK flag, checks that the resulting file is a FIFO, assembles every field and newline, rejects rows above 4096 bytes before emitting any bytes, and calls unbuffered File.write_all followed by flush once per row. This avoids stdout line-buffer splitting. Read, formatting, open, size, and write errors propagate; no metric or partial row is silently discarded.

The existing 10 ms sleep and field order are unchanged. The real artifact contains eight complete rows (maximum 2165 bytes) and a final 1017-byte partial row. The model reconstructs all eight complete rows exactly and retains the original partial failure unchanged. The 4096/4097 boundary is checked. The concurrent host pipe model uses its actual portable 512-byte atomic bound (2048 complete rows); it is not represented as a Linux runtime test. No product build, Docker workload, or performance run was executed.
