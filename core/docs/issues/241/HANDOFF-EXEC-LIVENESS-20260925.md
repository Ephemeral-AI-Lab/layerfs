# Next-agent handoff: resolve the baseline SDK Exec `Unknown` in #241

**Decision:** Continue the Linux ioctl/range-splice path. Its carrier and
edit/Commit function have passed many positions, including full 10 MiB,
100 MiB and capped 500 MiB v3 slices. Keep Phase 3 and release/latency
admission open. The blocking failure is an intermittent *pre-edit* baseline
SDK Exec `Unknown,true`, not a demonstrated ioctl or splice blocker.

## Exact current failure

The last frozen 264-position selection is
[v3](evidence/phase3-postfix-v3/REPORT.md), source `c1dc56aca`,
**229 PASS / 1 FAIL / 34 NOT_RUN**. Case `1mib-overwrite-band15`, offset
1,016,765, failed on `printf baseline > .position-baseline` after
5.044964542 s. No range EDIT or Commit ran for it. The daemon's mandatory
`HistoryCommand::ReserveInodes` spent 5.004232 s in `daemon.service_connect`;
there was no Service Hello/call child and no matching host HistoryCommand.
Two preceding Inspect calls failed quickly while resolving the baseline
path; the path and refusal code are inferred from the source and command,
not carried in LFT1. The shell spawned
promptly; Docker reported the daemon alive and not OOM killed; owner Hello
later worked; unmount and Sandbox deletion/absence passed. The connect span
includes both TCP connect and Noise authentication. Neither subphase, nor
the host's live session occupancy, was recorded. A
[read-only post-run private-Store query](evidence/phase3-postfix-v3/DERIVED-STORE-POLICY.tsv)
found writer budget two, yielding a host cap of four with the two-reader
allowance; the private copy itself is not in Git. Capacity exhaustion is a
hypothesis, **not** a finding. The exact raw case and logs are under
[v3 raw](evidence/phase3-postfix-v3/raw/1mib/).

The first integrated campaign was **243 PASS / 1 FAIL / 20 NOT_RUN** at
`10mib-delete-band13`, also baseline Exec before edit. Later diagnostic
attempts were distinct identities: one baseline-only control passed in 23 ms;
`sleep 6` proved the five-second no-progress clock can return `Unknown,true`;
an instrumented 10 MiB selection stopped at WorkspaceOpen/GetBranch; a
post-host-fix 10 MiB diagnostic passed 66/66. None proves the original
failure's sole cause. Preserve every receipt and status.

## Fixes and approaches already tried

| Source/evidence | Change or observation | Outcome |
| --- | --- | --- |
| `0dab33f29` and [post-fix diagnostic](evidence/postfix-position-diagnostic-v1/REPORT.md) | Stop polling closed stdin in flag-driven host acceptor; add daemon connect/Hello/call LFT1 spans. | Real CPU spin fixed; 10 MiB diagnostic 66/66, but causal link to historical Unknown unproved. |
| `a864af41b` | Align host Service session cap with bridge writer budget + two readers (default four). | Contract mismatch fixed; cap occupancy was not captured in later failure. |
| `50414c386` | Harden stale ioctl precheck, non-root writable-handle rights, mounted read oracle, cleanup and v3 verifier gates. | Focused Linux routes passed; does not remove baseline Exec stall. |
| `7b2181fd7` and [v1 selection](evidence/phase3-postfix-v1/REPORT.md) | Replace per-thread retained daemon transports with one shared session; reconnect on out-of-order request ID. | v1 showed four first-case fresh-reopen Service failures; v2 cleared that pattern. |
| `c1dc56aca` and [v2 selection](evidence/phase3-postfix-v2/REPORT.md) | Let Docker assign loopback control port, removing reserve/release/bind race. | v2 had 227 PASS / 1 Docker port-bind FAIL / 36 NOT_RUN; v3 live route passed and 100 MiB reached 66/66, but 1 MiB baseline Unknown recurred. |

The native protocol closes both client and server sessions after a definite
missing-name Inspect refusal. Keeping `Transport` alone after that error is
unsafe because `Client` is closed. Changing that behavior requires a
coordinated protocol change; it has not been attempted. Do not infer that
rapid reconnects alone saturated the host cap.

## Next investigation to perform

1. Keep the v3 failure unchanged. Add production-safe, bounded observation
   of the host acceptor's accepted, live, reaped and capacity-dropped
   sessions. Its `Vec<Session>` and capacity drop currently have no retained
   counter; Service LFT1 starts only after request admission. The telemetry
   operation key is a correlation ID, so do not encode occupancy in it. If
   using a best-effort shutdown summary, require its presence or classify
   the diagnostic `INCOMPLETE`.
2. Split the existing daemon `service_connect` span into TCP connect and
   Noise authentication spans while preserving the same deadline, wire
   behavior and single attempt. Keep Hello as its existing sibling span.
   `layerfs-bridge` currently has no telemetry dependency; expose the two
   bounded connection steps and compose them in the daemon instead of adding
   a dependency or changing timeout semantics.
3. Freeze **one separate, labelled diagnostic** on a tiny public SDK
   missing-path/create route, with exact source, image, master, output and
   cache identities. Record host acceptor counts, daemon TCP/Noise/Hello
   spans, Exec result, Docker state and cleanup. Controlled idle-session
   pressure may test the four-slot hypothesis, but declare it as synthetic
   and never use it to replace the frozen failed case or as performance
   admission. A plain success without count coverage does not resolve the
   unknown cause.
4. Use the observed stalled subphase and counts to make the narrow product
   fix. Preserve a single attempt, original request IDs, deadlines,
   construction worker count and cache policy. Then freeze a *new* complete
   264-position selection and run each size once. Do not resample v1/v2/v3
   arms or promote their PASS slices to a new identity.

The separate four-case release Edit→Commit gate remains open after Phase 3.
The v3 verifier is fail-closed until canonical root/count expectations are
pinned; it needs an exact portable-metadata oracle and complete caller,
Service and daemon LFT1 including the daemon shutdown summary. See
[SPEC.md](SPEC.md). The #232 parent retains its 56-case gate.

Repository rules: read root/core/benchmark AGENTS and the measurement
contract before measurement work. Use locked release binaries, append-only
outputs and independent writable master copies; never inflate a timeout,
rerun to choose a pass, warm a measured phase, change workers, patch a
dependency, or claim CI/preflight. Every commit needs exact first-parent
production LOC before/after/delta. Branch:
`codex/issue241-ioctl-feasibility`; raw evidence and reports are committed
under `core/docs/issues/241/evidence/` once the final handoff commit lands.
