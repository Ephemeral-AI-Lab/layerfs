# R7-retire: coverage audit, scope and method

> **Status:** Dated checkpoint receipt. Written before any removal; it claims no
> timing, storage or memory result and no sample was taken.
>
> **Addendum 2026-10-10:** after this document was written the owner authorized the full cleanup
> ("yes, do the full cleanup"). All seven directories and the two `layerfs-api` placeholder directories
> are now removed; `core/reference-tests` stays for R9. The text below is kept as written at audit time.
> Final state: [completion record](../../R7-RETIRE-COMPLETION-20261010.md#addendum-full-cleanup-by-owner-authorization).

Assignment: section "R7: covered integration cleanup (S11)" of the
[R5–R9 handoff](../../HANDOFF-R5-R9-20261009.md), row R7-retire of the
[rollout ledger](../../ROLLOUT-LEDGER-20261008.md). Audited at `cf5bd62ce`;
the one test this audit caused was added at `a6655ef45`.

## What is audited

The seven directories `core/Cargo.toml` excludes. Production lines from the
pinned counter (`tools/production_loc.py`, SHA-256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`), per file in
[receipt 01](01-production-loc-head.txt). The totals equal the handoff's.

| Directory | Production lines | Source files | Test functions | Audit | Verdict |
| --- | ---: | ---: | ---: | --- | --- |
| `layerfs-server` | 3996 | 31 | 54 | [10](10-layerfs-server.md) | remove |
| `layerfs-fuse-legacy` | 1447 | 5 | 25 | [11](11-layerfs-fuse-legacy.md) | remove |
| `layerfs-sandbox-legacy` | 1106 | 5 | 1 | [12](12-layerfs-sandbox-legacy.md) | remove |
| `layerfs-sdk-legacy` | 505 | 5 | 42 | [13](13-layerfs-sdk-legacy.md) | **not removed** (O-10) |
| `layerfs-daemon-legacy` | 2151 | 11 | 3 | [14](14-layerfs-daemon-legacy.md) | **not removed** (O-10) |
| `layerfs-bridge-legacy` | 6834 | 32 | 80 | [15](15-layerfs-bridge-legacy.md) | **not removed** (O-10) |
| `layerfs-workspace-legacy` | 26835 | 101 | 204 | [16](16-layerfs-workspace-legacy.md) | **not removed** (O-10) |
| Total | 42874 | 190 | 409 | | 6549 removed, 36325 kept |

What still refers to each directory, and the decisions on
`core/reference-tests` and the `layerfs-api` placeholder directories, are in
[20-references-and-decisions.md](20-references-and-decisions.md).

## Two facts that frame every row

1. **None of the seven packages can be loaded by Cargo at HEAD**
   ([receipt 02](02-cargo-cannot-load.txt)). Each inherits its package fields
   from a workspace it is excluded from. Three also depend on
   `../layerfs-api/core`, which F12 removed; the others depend on active
   crates whose API was replaced under them. No test in these directories has
   been buildable since its relocation, so none is coverage today. Removing a
   directory changes no built artifact and no test result.
2. **The predecessors are a different design, replaced as a whole.**
   [303/07 §4](../../../303/07-implementation-validation.md#4-source-removal-relocation-and-estimates)
   records the excluded cluster-two source as "replaced as a whole": the private
   backing "is deleted outright", while filesystem and runtime "implement
   behaviour that is kept and rewritten". A row is therefore classified by the
   behaviour a file or test stood for, not by whether its code was moved. No
   code was moved; nothing in this stage is a relocation.

## Classes

| Class | Meaning |
| --- | --- |
| A | The behaviour is covered by an active crate. The row names the file and the test |
| C | Dropped by a recorded direction. The row cites it by the tags below |
| A+C | The behaviour is covered as the current contracts define it, and the predecessor's mechanism for it is dropped. Both halves are named |
| B | Needed and not covered. Migrated into an active crate with its test in this stage, before the predecessor is removed |
| O | Neither covered nor dropped by a recorded direction: the documents record it as an open owner question. The crate that carries it is not removed |

A row is A only when the named active test exists; a generator checked every
named file, test and test count against the tree
(`core/crates` at `a6655ef45`, 1307 active test functions). The generator, its
classification data and its three inputs are in [tools/](tools/); it runs only
against a tree that still holds the seven directories. "Covered" means the
same product behaviour is asserted, at the scope the named test states. It is
not a claim that the test is equivalent line for line, and it inherits every
limit recorded in the R2–R6 completion records.

## Recorded directions cited

| Tag | Direction | Where it is recorded |
| --- | --- | --- |
| D1 | `layerfs-server` and the host-mediated runtime are retired; the daemon opens the Store itself; the host is control-only after install | Root [AGENTS.md](../../../../../../AGENTS.md) (target, owner direction 2026-10-07, "Do not revive `layerfs-server`"); [303/08](../../../303/08-decisions-provenance.md) K24, K28, K31, K33 and §4.3 "layerfs-server retired — retain"; [F12 record](../../PRE-S8-F12-20261007.md) |
| D2 | No daemon Exec supervisor, launcher, registration or custom Exec wire; no automatic command timeout; managed-Exec selections withdrawn | [core/AGENTS.md](../../../../../AGENTS.md) "Filesystem access and execution ownership" (owner clarification 2026-10-08); 303/08 K21, K35; [Exec contract](../../../303/workspace-api/exec.md) |
| D3 | No private backing files; a daemon-owned SQLite overlay holds names, inodes and payload | 303/08 §4.1 first row ("retain — Owner requirement"); 303/07 §4.1 ("`backing/` … is deleted outright"); [303/01 §8](../../../303/01-architecture.md#8-what-is-removed) |
| D4 | The cached mount profile replaces direct I/O; per-mutation kernel invalidation is deleted | 303/08 K11 and §4.2 ("Kernel invalidation kept, one per mutation — delete"); [architecture 77](../../../../architecture/77-native-mutation-coherence.md) |
| D5 | One attempted operation: no deadline-driven retry, no busy handler, no blocking admission; a contended Store is one before-effect Busy | Root AGENTS.md §4; 303/08 K13, K30; [architecture 23](../../../../architecture/23-native-bridge.md) ("Retired deadlines, timeout retry loop …") |
| D6 | Terminal unmount includes close and cleanup; no separate close | 303/08 K22, O-14 (resolved) |
| D7 | Branch Commit is overwrite-only; stage and publish are one transaction and a refused publish leaves no stage | [Branch overwrite decision](../../BRANCH-OVERWRITE-DECISION-20261007.md); 303/08 K15, K34, O-23, O-25 (resolved) |
| D8 | No artificial total cap: the 128-handle table, charged dirty/name/edit frontiers, the 256 MiB prepared upload and the 4 GiB file constant are retirement targets | Root AGENTS.md mental model; 303/01 §8; 303/08 O-11 (resolved), §4.2 "128 handles — replace" |
| D9 | Host-side construction and the logical data wire are removed: prepared stream, metadata constructors, Source, the daemon's host transport and headless delivery, Bridge codec/contract/framing | 303/07 §4.1 removal table; F12 record; 303/08 §4.3 ("Construction contracts deleted") |
| D10 | The excluded source is renamed `-legacy` and deleted at the end; it is never source-included or restored | 303/08 §4.3 ("Rename the old crate to `-legacy`, delete it at the end — retain"); Exec contract §8; architecture 20, 21, 22, 23, 71, 75 ("S11 must remove it …") |
| D11 | SandboxApi is lifecycle, ordinary execution, standard streams, exit status and cancellation; R1 closed at the owner-selected scope | [S8 specification §10.1](../../S8-SPECIFICATION-20261008.md); ledger row R1 ("COMPLETE — owner-selected R1 scope") |
| D12 | No per-Workspace private-disk budget; the overlay page ceiling is daemon-wide, with physical headroom and device-full handling | 303/08 K18 and §4.1 ("Storage quota as a page budget — revise"); [303/02 §12](../../../303/02-base-overlay.md#12-quota-disk-full-and-errors) |
| D13 | A Workspace is one mutable view; there is no separate read-only mount profile | Root AGENTS.md mental model, first bullet; [mount contract](../../../303/workspace-api/mount.md) |

D10 is the direction to delete. D1–D9 and D11–D13 say which predecessor
mechanisms the replacement does not carry.

## The one open owner question that blocks removal: O-10

[303/08 §7](../../../303/08-decisions-provenance.md#7-questions-only-the-owner-can-answer)
lists O-10, "Are pinned read-only SDK views kept? Each one is an extra live
generation", with the recommendation "Defer them" and no recorded answer. The
same document records "Pinned SDK views are not designed here … no owner
answer exists" (K20) and "View model with pinned SDK leases — unresolved"
(§4.2); the [design index](../../../303/README.md#questions-for-the-owner)
still names the SDK view among the remaining owner choices.

The predecessors implement this feature (`pin_view`, `view_lookup`,
`view_list`, `view_read`, `view_readlink`, `view_status`, `release_view`). No
active crate has it. It is therefore not class A, and no direction drops it, so
it is not class C. Whether it is needed is the owner's decision.

Production lines whose subject is the view lease:

```
core/crates/                                  [1656]  (part: the seven view-lease files only)
  layerfs-bridge-legacy/                       [404]  (part)
    src/                                       [404]
      adapters/                                [199]
        native/                                [199]
          protocol/                            [199]
            workspace_view.rs                   199
      contract/                                [205]
        workspace_view.rs                       205
  layerfs-daemon-legacy/                       [199]  (part)
    src/                                       [199]
      control_view.rs                           199
  layerfs-sdk-legacy/                          [200]  (part)
    src/                                       [200]
      workspace_view.rs                         200
  layerfs-workspace-legacy/                    [853]  (part)
    src/                                       [853]
      filesystem/                              [720]
        active_view.rs                          309
        view_reads.rs                           411
      runtime/                                 [133]
        view_leases.rs                          133
```

Scope of the listing: `cf5bd62ce`, pinned counter, the seven files whose whole
subject is the view lease. Other files in the same four crates carry single
view variants or call sites; the per-crate audits name them. Four test
functions assert it.

This session has no authority to answer O-10, and the task's rule for such an
item is: do not remove that crate, list the item, continue with the others.
**The four crates that carry view-lease code are left in place.** Everything
else in them is class A or C, and their removal is prepared in
[30-prepared-removal.md](30-prepared-removal.md).

Two things an owner answer should know:

- The predecessor's views are built on the private backing (D3). If the answer
  is "keep them", they need a design on the SQLite overlay (K20); the old
  source can inform it but cannot be the implementation.
- If the answer is "defer" or "drop", the four removals are mechanical and
  change no built artifact.

## Class B: what was migrated

One behaviour, asserted by three predecessor tests and by no active test:
executing a program stored in the mount.

| Predecessor test | Migrated to |
| --- | --- |
| `layerfs-fuse-legacy` `kernel_write.rs::kernel_write_exec` | `layerfs-daemon/tests/mounted_execute.rs::a_stored_program_executes_and_its_in_place_rewrite_executes_next` |
| `layerfs-workspace-legacy` `coherence.rs::coherence_exec` | same |
| `layerfs-workspace-legacy` `readable.rs::root_owner_reads_mode_zero_but_execute_requires_an_execute_bit` | same |

Test only; no product source changed and the test passed on its first run
([Linux receipt](execute-attempt1-linux-mounted_execute.txt)). The old tests
asserted the direct-I/O profile's behaviour; the new one asserts the cached
profile's, including an equal-length in-place rewrite of a program whose pages
are cached.

## Not carried, with no sentence naming them

These predecessor conveniences are absent from the active crates. They are
classified C because the governing scope excludes them (the tag is given), not
because a document names each one. They are listed so the owner can disagree.

| Item | Predecessor | Why C |
| --- | --- | --- |
| `SandboxApi::list` (enumerate owned sandboxes) | sandbox, SDK | D11: not in the selected SandboxApi scope; the registry it read routed calls to the host Server (D1) |
| Bounded daemon log capture at Sandbox delete | sandbox, SDK | D11 |
| Read-only mount profile (`mount` beside `mount_writable`) | FUSE, daemon | D13 |
| FUSE `ACCESS` served by the daemon | FUSE | The mount uses `default_permissions`; the active crate declares ACCESS unsupported and counts it ([FUSE contract](../../../303/fuse.md)) |
| Headless stdin/stdout delivery | daemon, Bridge | D9 |

## Not verified

- No predecessor test was run: none can be built ([receipt 02](02-cargo-cannot-load.txt)).
  Their intent was read from source and from their `check("…")` identifiers.
- Class A rows were matched by reading test names and, where a name was not
  decisive, the test body. The named active tests were not individually rerun
  for this audit; the stage's final suites run every active test binary once.
- The Docker route scripts (`*.py`) of the predecessors were classified by
  their header and the binaries they start, not run.
