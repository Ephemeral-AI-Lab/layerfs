# Issue 261: corrected 100-write phase diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This selection is declared after the original 100-write diagnostic
(`issue261-separated-100-v1`, source `b094a4a1f`) and the retained 512-write
FAIL (`issue261-separated-512-v1`, source `725b4379a`), and before changing
the instrumentation or running a new sample. The 512 failure showed that the
optional snapshot ran after a successful FUSE reply while the projection
mutation permit remained live. Source supports a reply-to-permit-drop race;
the exact failing Busy site was not separately logged. This v2 diagnostic
places each snapshot before the reply and records cumulative time spent in
payload acquisition and file publication, so the 100-write cost can be
attributed without a per-write output write or RPC. The v1 receipt stays
unaltered and is not pooled with v2 latency.

Scenario `issue261-separated-100-v2` uses the same family, exact 8,194-byte
old file, one shell-launched writer, one fd, 100 one-byte positional syscalls
at even offsets, writer binary/source, public mount → one Exec → one Commit,
independent old/new-head and 100-run oracle, worker count, uncontrolled-cache
contract and 15 s complete-command limit as v1. The writer prints only four
progress lines at 25/50/75/100. A distinct diagnostic image prints one
counter snapshot at the same four successful FUSE WRITE counts. The snapshot
includes cumulative payload-acquisition nanoseconds and file-publication
nanoseconds, with the latter including projection completion/invalidation;
unattributed callback/kernel/IPC time is not assigned to either. The
snapshot's own record scan/logging occurs before the reply and is diagnostic
overhead. Record actual FUSE classes, ledger/page counters, resources,
Exec/Commit/complete/cleanup wall, route, cache state and all failures.

Reuse the closed old-head prepared master only after validating its Store and
history hashes, fixture bytes, old-head proof, cursor-key custody, unchanged
Store/history/SDK/verifier/writer compilation inputs, and source compatibility.
The instrumentation-only change to FUSE/daemon source needs a new locked
release daemon binary and image; SDK driver/verifier and writer binaries may
reuse their matching seals. The clone remains an independent writable byte
copy and is not a cold-cache claim. Complete-command diagnostic limit is 15 s,
verifier target under 10 s, focused tests under 30 s. Exactly one v2 attempt
per frozen source identity; no failed or ineligible receipt is replaced.
No 512-write retry is part of this selection.
