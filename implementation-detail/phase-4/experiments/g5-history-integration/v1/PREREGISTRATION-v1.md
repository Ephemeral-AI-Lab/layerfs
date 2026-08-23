# G5-3 v1 preregistration

Status: `READINESS PASS`. Settled source, release, native compact receipt, and
focused evidence are authoritative. Input adoption, freeze, and zero-row proof
remain required in that order before any campaign row.

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
`REUSED-AUTHORITY-v1.json` and are not rerun.

Random edits, branch DAGs, backup/restore, destructive GC, and new
shutdown/restart populations are outside the narrowed contract.

## Time and resources

- preparation and zero-row proof: preferably and hard `<20 s` each;
- mechanism screen: `<20 s` complete wall;
- gate target: 30-60 s; hard `<=150 s` complete wall;
- combined long-lived child RSS: `<=20,971,520 bytes`;
- any individually owned buffer: `<=1,048,576 bytes`;
- terminal child, Q, temporary residue, descriptors, and work roots: zero.

There is no product preparation process and no regenerated oracle. The one-shot
input action only hashes/stats and adopts the sealed 1 MiB H11 fixture, its
1,001-row expected-roots table, and the sealed 10 MiB concurrency source; it
creates only a tiny manifest and targets subsecond wall time. Freeze binds these
exact external operands, the exact executed
runner, both analyzers, method files, focused evidence, reused authority,
settled product/build/inherited sources, release executable, and input-adoption
manifest. Gate additionally requires the accepted `<20 s` screen and a compact
post-screen cached-project fmt/clippy/diff closure; its two focused source tests
are hash-bound from the settled ledger and are not rerun. Pre-row harness plumbing
defects use append-only attempts within v1. A product row or population,
threshold, source, or semantic change requires a new version.
