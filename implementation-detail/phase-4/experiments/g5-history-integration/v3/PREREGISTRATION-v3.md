# G5-3 v3 preregistration

Status: `READINESS PASS` before input-reuse adoption. V2 is terminal
NO-GO: its sealed-input compatibility repair restored semantic PASS, but the
direct process exceeded the unchanged RSS cap by 671,744 bytes. V3 retains the
exact workload, population, thresholds, timer boundaries, and decision rules.
The controlling resource candidate configures all three simultaneously live
concurrency connections—writer, reader one, and reader two—to 1,280 SQLite
cache pages. Page size is exactly 4,096 bytes, aggregate cache ceiling is
15,728,640 bytes, and reduction from the 2,000-page default across all three
connections is exactly 8,847,360 bytes. Observed connection high-water is three
and terminal count is zero. The controlling durable receipt passes both v3
analyzers at RSS 20,168,704 bytes with 802,816 bytes reserve; the 12-mutation/
28-decision self-check passes. The earlier writer-only v3 direct diagnostic is
preserved as superseded premeasurement evidence.

## Narrowed live population

Each campaign owns exactly one long-lived product child. The gate creates one
continuous 1,000-edit, same-size 1,048,576-byte history and observes checkpoints
at revisions 1, 10, 100, and 1,000. Screen stops after revision 10. History-fill
edits retain only lightweight root, length, route, transaction, COMMIT, Q, and
residue evidence. A separately observed exact 4,096-byte range and same-size
edit run once at each checkpoint; there is no claim of 1,000 range samples.

Every checkpoint records history-coupled exact/latest projection conservation.
G5-2 v3 remains the authority for the semantic distinction itself. Each
checkpoint observes the N parent, then its same-size edit advances to N+1 for
exact-parent/latest-target projection. Full reconstruction runs at each
pre-edit checkpoint and once at the distinct terminal end after the final
N-to-N+1 edit. After the
history sequence, the same child proves `A -> B -> A`, followed by an exact
4,096-byte historical-root read of B.

One separate 10,485,760-byte sentinel runs two immutable readers against one
canonical writer. The writer must complete exactly one transaction and one
COMMIT with Busy/Locked zero. Both already-open read-only connections observe
the prior head before COMMIT and the same new head afterward, with no live
statement or blob scope spanning COMMIT.

## Reused authority

G5-0 H11 v9 supplies read-only reachability and diagnostic unique-revision
slopes. G5-1 v27 supplies the trusted/verified publication boundary. G5-2 v3
supplies exact/latest projection semantics, service/fault lifecycle closure,
and shutdown/restart authority. These sealed milestones are hash-bound by
`REUSED-AUTHORITY-v3.json` and are not rerun. It also binds the accepted v27
G4 patched-control parity, v2 terminal NO-GO, the preserved initial writer-only
supersession, and the append-only all-three-1,280 scope correction.

Random edits, branch DAGs, backup/restore, destructive GC, and new
shutdown/restart populations are outside the narrowed contract.

## Time and resources

- preparation and zero-row proof: preferably and hard `<20 s` each;
- mechanism screen: `<20 s` complete wall;
- gate target: 30-60 s; hard `<=150 s` complete wall;
- combined long-lived child RSS: `<=20,971,520 bytes`;
- any individually owned buffer: `<=1,048,576 bytes`;
- terminal child, Q, temporary residue, descriptors, and work roots: zero.

There is no product preparation process, no input recopy, and no regenerated
oracle. V3 explicitly reuses the already sealed v2-owned root (`input_reuse=true`).
Its one-shot adoption action verifies the exact v2 input-manifest and preparation
hashes, reopens and rehashes all three `0444` files under the `0555` root,
rechecks the 1,001-row oracle, and binds the current executable. Adoption targets
`<1 s` and is hard `<20 s`. Freeze repeats that adoption verification and binds the exact executed
runner, both analyzers, method files, focused evidence, reused authority,
settled product/build/inherited sources, release executable, and input-adoption
manifest. Gate additionally requires the accepted `<20 s` screen and a compact
post-screen cached-project fmt/clippy/diff closure; the one new structural
lifetime test is hash-bound from the settled ledger and is not rerun. Each child
has one lossless PROCESS-EVIDENCE receipt. Final decisions bind terminal/raw/
both analyzers/lock release, followed by one sorted artifact manifest with no
extra product process. Pre-row harness plumbing defects use append-only attempts
within v3. A product row or population,
threshold, source, or semantic change requires a new version.
