# Pair 1 — projection and runtime

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Documentation consolidated on 2026-09-21. The detailed design lives in the
> packet below; no Workspace/FUSE implementation or qualification is claimed.

The complete current packet is
[`fuse-workspace-snapshot-overlay/`](fuse-workspace-snapshot-overlay/README.md).
It incorporates the reviewed C1/C2, Pair 3 and merged Pair 2 APIs at source
`152b9c3a2e8ec2536a1d63601b681e1f7ef34455` and preserves earlier dated research.

| Document | Responsibility |
| --- | --- |
| [Overview and source basis](fuse-workspace-snapshot-overlay/README.md) | Architecture, decisions, ownership and status |
| [Workspace/FUSE contract](fuse-workspace-snapshot-overlay/01-workspace-fuse-contract.md) | Platforms, immutable mount roots, identities, lazy reads and full POSIX table |
| [Overlay/snapshot design](fuse-workspace-snapshot-overlay/02-overlay-snapshot.md) | Frozen G/live G+1, copy-on-write pieces, coherent capture, bounds and completion |
| [Commit integration](fuse-workspace-snapshot-overlay/03-commit-integration.md) | Existing history operations, exact stages, failure observations and Layer publication |
| [Implementation and verification](fuse-workspace-snapshot-overlay/04-implementation-and-verification.md) | Source layout/LOC guidance, dependency order, external tests and qualification |
| [Implementation handoff prompt](fuse-workspace-snapshot-overlay/07-implementation-handoff.md) | Next task instructions, starting with R0-R and the readable Linux mount |

The execution-side daemon assembles separate `layerfs-fuse` and
`layerfs-workspace` libraries under `core/crates/`. Workspace groups its
implementation into runtime, filesystem, overlay, backing and commit folders,
and owns live namespace, inodes, handles, pending bytes and generations. FUSE
depends on its public semantic API; Workspace depends on neither FUSE nor the
daemon. The existing bridge carries
logical operations to service-local C1/C2 and C5. StageChanges constructs and
saves the filesystem tree; ordinary callbacks do not publish history.

Linux is first; macFUSE and Windows adapters are later. `/workspace-id` is an
immutable managed mount identity. The full writable target keeps a frozen G
while live G+1 continues, under one proposed shared consumer budget and one
submission in flight. No concurrency-limit change, automatic mutation replay,
rebase, GC or durability upgrade is authorized by the documentation.

[#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179) owns Pair 1;
[#207](https://github.com/Ephemeral-AI-Lab/layerfs/issues/207) owns its matched
mounted comparison. Pair 2's [#210](https://github.com/Ephemeral-AI-Lab/layerfs/issues/210)
qualification remains distinct from this design and from mounted acceptance.
