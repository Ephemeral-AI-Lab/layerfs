# Extent-aware `copy_file_range` and prepend

> **Status:** Proposal with compatibility gate
>
> Target: LayerFS 0.1.2 when patch-compatible; otherwise LayerFS 0.2.0
>
> This document is not part of the LayerFS 0.1.0 contract.

## Question

Can LayerFS preserve canonical references when a program expresses a copy or
prepend through the standard `copy_file_range` operation, avoiding payload
round trips while retaining normal POSIX results and the 0.1.0 storage format?

The current 0.1.0 prepend benchmark uses ordinary public filesystem behavior:
a temporary file, a short prefix write, a copy of the original payload, and an
atomic rename. It already demonstrates missing-only Commit storage. This
proposal asks whether the execution path can also avoid transferring unchanged
payload bytes when the application explicitly invokes a range-copy primitive.

## Required semantics

An accepted operation must match ordinary byte-copy results for:

- source and destination offsets;
- requested and completed lengths;
- end-of-file behavior;
- partial completion;
- same-file copies;
- overlapping ranges;
- later overwrite, truncate, rename, unlink, and Commit;
- error propagation and deferred FUSE errors.

The optimization must be invisible to callers other than improved resource
use. It must not create a second canonical form for the same file.

## Candidate internal representation

When eligibility is proved, the Workspace may record a borrowed immutable
source range instead of copying its payload through userspace. Later writes
must override borrowed ranges deterministically, and Commit must resolve the
final extent sequence into the existing 0.1.0 canonical representation.

The Store must continue to contain one global canonical-object namespace. No
per-Workspace object copy, refcount, or alternative chunk encoding is allowed.

## Compatibility gate

The proposal targets 0.1.2 only if existing released clients and the released
daemon remain compatible. In particular:

- a new FUSE callback alone is not sufficient evidence of compatibility;
- a new wire opcode requires capability negotiation or a compatible fallback;
- older helpers must fail safely or use ordinary byte-copy behavior;
- the canonical Store and Commit result must be byte-identical to the ordinary
  copy path;
- public SDK additions must be additive and optional.

If these conditions require an incompatible daemon protocol or public API,
the feature targets 0.2.0 instead.

## Resource requirements

The optimized path must:

- bound metadata by final range structure rather than copied byte count;
- avoid materializing the complete source range in host memory;
- avoid unbounded FUSE frames;
- retain bounded Store membership and object batches;
- perform no Store or network scan inside a SQLite write transaction;
- preserve deterministic cleanup after interruption.

## Required proof

Test at least:

| Case | Required proof |
| --- | --- |
| Copy complete 32 MiB file | byte-identical result and bounded transfer |
| Prepend 10 bytes | existing canonical result and missing-only storage |
| Partial source range | exact offsets and length |
| Same-file non-overlap | ordinary copy semantics |
| Same-file overlap | specified platform result or safe fallback |
| Copy followed by overwrite | later write wins |
| Copy followed by truncate | exact final length and closure |
| Interrupted copy | no visible partial Commit or leaked state |
| Unsupported helper | ordinary byte-copy fallback |

Collect:

- kernel and proxy operation counts;
- payload bytes transferred;
- borrowed-range metadata bytes;
- candidate, inserted, and reused canonical bytes;
- Commit phase timings;
- peak RSS and swaps;
- final digest and reopening proof.

## Exit condition

Accept the proposal only if it produces the same released canonical result as
ordinary copying, reduces transferred payload materially, stays within memory
bounds, and does not regress create, edit, prepend, or read lifecycles.
