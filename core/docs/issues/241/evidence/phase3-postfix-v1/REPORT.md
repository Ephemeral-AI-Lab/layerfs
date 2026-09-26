# #241 full position selection at `50414c386`: failed fresh reopen

This is the first complete, prospectively selected four-size attempt under
[the frozen contract](CONTRACT.md). It is **not** Phase 3 admission. Source
commit `50414c386a4f6b71dfc074dee685ca21d63f1eb0`, tree
`6e2b4fb1f9bc3c68507891f308cf5a2ce7414c78`, manifest SHA-256
`e12509411e225e6eea497dc08e3cc0730f1c2ab7ce3f104cfc232eb16c7f2b6a`,
release position-test binary SHA-256
`c62482b17e4002faa254ce0084cca89d2937dbd1c71ab72cde56ace97d4f5591`,
and Linux image `sha256:5e25b9c0d7268f7a7f410d2916edcea815492b41e7cee37a2356409e6e3b42d4`
were fixed before the first run. The [plan](raw/plan.json) binds the four
independent writable master copies, hashes, diagnostic telemetry run IDs and
fresh output paths. The closed [master receipt](raw/masters.json) records four
first-use release SDK Init preparations. The plan SHA-256 is
`93d9cdc2aefa33cad5703c34cb48c842999dd8a5d0fb09f8d9d6bab6d584c0a2`.

| Size | First case and offset | Baseline SDK Exec | PASS | FAIL | NOT_RUN | Cleanup |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| 1 MiB | `1mib-insert-band00` @ 32,446 | 27.223208 ms | 0 | 1 | 65 | PASS |
| 10 MiB | `10mib-insert-band00` @ 414,596 | 24.169750 ms | 0 | 1 | 65 | PASS |
| 100 MiB | `100mib-insert-band00` @ 4,065,514 | 24.891250 ms | 0 | 1 | 65 | PASS |
| Capped 500 MiB | `500mib-capped-insert-band00` @ 2,131,595 | 26.378042 ms | 0 | 1 | 65 | PASS |

Total: **0 PASS / 4 FAIL / 260 NOT_RUN**. Each size stopped after its first
failure and retained all 66 position receipts; no case was repeated. The
baseline Exec returned exit 0 in all four. The splice, Commit, current mounted
read, Branch-head check and full stored-byte oracle reached the fresh reopen
in the first case. The fresh mount and unmount returned success, but its first
`stat -c %s payload.bin` exited 1 with `I/O error`. Sandbox deletion and
list-confirmed absence passed for both the edit and fresh Sandboxes. The
first-case receipt, run and summary are under each size in [raw](raw/); their
SHA-256 inventory is [RAW-SHA256.txt](RAW-SHA256.txt).

The daemon LFT1 trace places the fresh read failure at `Inspect` →
`daemon.service_connect`, after successful `WorkspaceOpen`. Each size's edit
daemon recorded three successful upstream connections and the fresh daemon
one; then the fresh Inspect connection failed in about 1.3–1.7 ms. The Store
fixture has `max_concurrent_writes=2`; the host cap is that budget plus two
reader sessions, or four. Source inspection found that the daemon retained one
transport per thread in a process-lifetime map while holding its mutex across
each complete call. This is a strong capacity-exhaustion explanation, but the
receipt does not contain a direct host acceptor count for the rejected socket.
The failure is not an observed unknown baseline Exec outcome, and these rows
make no speed or cold-cache claim.

The raw copy excludes each private writable Store/history copy. It retains
the exact plans, identities, case receipts, command output and failure
diagnostics; the original private copies remain under the recorded local
output paths. Docker inspect snapshots contain container State only, without
control configuration.
