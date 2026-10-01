# Proposed Commit lifecycle and consistency

> **Status: Current planning checklist; no release candidate exists.**
> Source, scope and evidence: [README](README.md).

## Proposed lifecycle

This is a semantic outline. It allocates no opcode, schema version, handle
grammar or persistence guarantee; those require the source audit and contract
freeze in [DESIGN_PLAN](DESIGN_PLAN.md).

```text
live Workspace --capture--> immutable G1; writes continue in G2
                                |
                    bounded construction and uploads
                                |
                    validated candidate + pack ACKs
                                |
                              READY
                                |
                 short conditional Branch publication
                                |
                  known result / uncertain outcome
                         |                 |
                 current-G2 install    retained custody
                         |
                 bounded safe retirement
```

1. **Capture:** select immutable source/metadata generations and their leases.
   Later accepted writes belong to a successor; unrelated inode operations can
   progress without holding a Workspace-wide lock through construction.
2. **Construct:** stream changes and required canonical/namespace work in bounded
   batches. Build compressed/delta packs with exact locators. Upload immutable
   objects and preserve required delta-base custody.
3. **READY:** prove complete input/result seals, validated namespace/metadata and
   required pack ACKs in the daemon. The trusted daemon supplies READY; the global
   service does not fetch candidate packs for a second filesystem certification.
   Reserve installation, control, outcome and cleanup
   capacity before the final publication request.
4. **Publish:** atomically check the expected Branch head and expose the complete
   committed root/history reference through the existing C5conditional transaction.
   Check authenticated owner, scope/profile, exact current base/head/root and
   registered candidate role. Global SQL owns no second filesystem namespace index.
   Broader deployment fencing/restart outcome resolution remain required decisions.
5. **Install:** apply a known result to the actual current successor, preserving
   intervening writes and candidate captures. Never replace a newer G2 with a
   stale captured version.
6. **Retire:** release known completed resources and process eligible cleanup in
   bounded batches. Active pins, unresolved outcomes and reachable delta bases
   retain custody.

## Publication and failures

**Proposed:** SQLite and MinIO are separate transaction domains. Complete and
validate immutable object uploads before publishing references. Failed or losing
candidates can leave unpublished objects; define safe orphan collection using
operation ownership, selected locators, reachability and pins.

Separate a definite pre-publication refusal, an explicit Branch conflict, known
publication success and an acknowledgement with Unknown outcome. Unknown must
retain ownership/resources and accepted writes. No automatic resend, query-based
guessed adoption, rollback or refund. The supported explicit resolution path is
**open — required**, including process death and restart.

Admit one pending submission per Workspace. Independent Workspaces and commands
need independent ownership; sharing a Branch produces an explicit conditional
publication conflict. SQLite writer serialization must cover only bounded short
transactions, with measurable progress under occupied data/Save capacity.

## Boundary obligations

Mount authority remains independent of Exec launch. Use generic commands and
ordinary POSIX/FUSE mutation paths. Freeze actor/incarnation and issued-handle
access, finite callback/I/O/Commit/cleanup bounds, UntilOwnedExit, cancellation,
disconnect, descendant/reaping/pipe outcomes and explicit discard. Preserve
accepted writes across those outcomes.

Carry forward [#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248) and
[#256](https://github.com/Ephemeral-AI-Lab/layerfs/issues/256) boundary gates,
successive-head/pin proofs and candidate crossings of capture/install. A simpler
storage engine does not remove these consistency obligations.
