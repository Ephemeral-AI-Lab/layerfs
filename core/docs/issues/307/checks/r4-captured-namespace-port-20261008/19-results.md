# R4 component checkpoint: captured namespace pages

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Source after5cf3e1220. Full R4 construction and R2–R5 remain incomplete.

Workspace's OverlayCapturedNamespace exposes captured_inode_page and
captured_directory_entry_page over the independently retained CapturedReader.
It extends the existing captured inode/run point contract. Direct Overlay and
OwnerClient reuse ReaderInodes/ReaderDirectoryEntries, the existing64-row
windows, schema, generation indexes and SQL. The port reads local captured
changes, including whiteouts; it does not pretend those changes are a complete
filesystem or substitute active state for the captured root/floor.

The Owner adapter retains an original failed Completion, including its credit
and work receipt, or the original unattempted Command on admission failure.
It is synchronous for constructor threads and must not be used on native
receive/service workers. Its successful bounded Vec clone overlaps the original
credited page briefly; no zero-copy or total-residency claim is made.

| Check | Outcome and receipts |
| --- | --- |
| Host locked all-target Workspace/Daemon build | PASS [build](03-host-build.txt), [result](04-host-build-result.json) |
| Host public tests | PASS5: captured_namespace2, captured_runs3; [commands/hashes/outcomes](07-host-results.json) |
| Linux locked all-target build and warning-denying Clippy | PASS [transcript](09-linux-build-checks.txt), [original command](08-linux-create.json) |
| Linux public tests | PASS same5; [commands/hashes/outcomes](11-linux-test-results.json) |
| Host warning-denying Clippy, format, boundary, tooling | [Final check results](18-check-results.json); exact outcomes retained |
| Owned Linux cleanup | PASS [original container exit/removal](13-linux-cleanup.json) |
| Complete namespace constructor/topology/mounted Commit | NOT_RUN; not implemented by this port |

The new tests compare130 captured inode/name rows across three pages,14 whiteouts
and255-byte names sharing253 prefix bytes. Every inode point agrees with its
page.130 later active rows cannot extend captured EOF. Install and Close preserve
the reader's original state; release makes the direct reader stale. Invalid
cursor input retains the attempted Completion/credit and a stopped owner returns
the exact ReaderInodes command. Test fixtures exercise public engine change rows,
not an invented Content root or a full namespace/byte oracle.

Each actual test command has a100s wall stop and setup-job waits are bounded
at3s. No timeout or failed functional selection occurred. Natural caches are
functional only. There is no global Store execution, performance sample or new
numerical gate; Overlay alone uses its separate MEMORY/OFF/EXCLUSIVE profile.
Durable remains NOT_RUN by owner. The Linux container is removed after exact
exit0; no Store volume or mount was created.

R4 still needs the sealed indexed StreamedRowSource producer, canonical metadata
patches/changed file roots, complete namespace construction, bounded Content
validation and incremental topology, and exact producer failure custody in the
Commit composition. The existing resident demanded/addition/parent/cycle state
and whole-base alias scan were inspected, not changed or qualified by this
checkpoint. No native R2/R3/R5 proof is inferred.

Production LOC:170710 ->170795 (delta +85). Core105293 ->105378;
active62419 ->62504; reference65417, excluded predecessors37431 and excluded
integration5443 unchanged. The [exact snapshot comparison](20-exact-production-loc.json)
uses the pinned counter and unchanged source classification. No relocation,
retirement or new dependency is involved.

The [document/source preservation check](21-document-preservation.json) resolves
52 local targets and confirms original protected notes and proof source hashes.
Full whitespace is FAIL exit2 solely for the final blank line in four preserved
raw Rust transcripts; source/docs/JSON whitespace passes. Raw evidence is retained.
