# Prospective scoped Git index oracle

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Native and LayerFS index stat caches legitimately encode different filesystem
identities. Existing r7-scoped-oracle raw-index comparison remains unchanged
historical source/evidence; it must not silently acquire an exception. No Git
timing sample has run. E09 remains owner NOT_RUN.

Taken under owner2026-10-09 direction and S8's explicit P-3 scoped-verifier ruling
(S8-SPECIFICATION-20261008.md1100 and R7handoff280): define an append-only
r7-git-index-scoped-v2 oracle for exploratory timing. All matched arms must use
the same new verifier and independently sealed expected semantic input. Registry
and receipts name this oracle schema/identity prospectively. Workload bodies,
cache rules, permissions, product index bytes and canonical formats stay intact.

Compare complete ordered path bytes, object mode, full object ID, stage and all
persisted semantic flags, including assume-valid/skip-worktree/intent-to-add.
Validate bounds, pathname order, flag constraints, header, entry count, selected
index version/object format, supported extensions and the actual trailing digest.
Git ls-files may corroborate entries but does not by itself prove checksum
verification; its debug output is not a stable complete-flags decoder. Refuse
unknown forms/extensions, zero/absent checksum or unpinned object format rather
than guessing. Preserve relevant extension semantics exactly or refuse them.

Exclude only the index's filesystem stat-cache fields from the cross-filesystem
semantic comparison: cached ctime/mtime, device/inode, uid/gid and file-size
observations. Record those raw fields, index length and full-byte hash alongside
the semantic result. The staged object-mode field remains compared. Continue
checking the actual index file's filesystem mode/owner/kind/size, every other
selected .git file's full bytes, and selected tracked working payloads. Label
the result scoped; it does not prove raw native/L index-byte equality or stat-cache
correctness. Only an actual expected-versus-observed comparison can return PASS.

The owned full copy's initial header is DIRC/version2/14104 entries,1839891 bytes.
Its config SHA256 is ebfce7f40e09fc5e24715573839c7467ebfe8a125e1b00e51d3d6ef410fd36fc;
repositoryformatversion1, filemode/ignorecase true, objectformat and skipHash not
explicit there. Effective worktree configuration and actual Git binary/version
must also be pinned before authoring; do not infer them from these partial fields.
Support only explicitly verified selected versions/formats. An unsupported case
remains unrun with its exact prerequisite/failure; no contract weakening follows.

Separately prove L preserves the exact index Git produced: full-byte hash/length
through the live mount after Git, normal Commit, explicit unmount, fresh mount,
then the same hash/length before another Git command can refresh the index.
This proof preserves all index bytes, including excluded stat fields, and the
original outcome/custody. It uses existing product APIs and no product test hook.

Implementation is harness-only in r7/git_index_oracle.py and external owning
tests. Prepare/copy/observe/compare remain one-attempt bounded operations; original
failures and unavailable data refuse PASS. Verifier work stays within9.5s and
outside performance. Source/binary/config/fixture/workload/oracle/report seals
and actual byte/metadata input gates precede samples. No new dependency, product
resident state, disk index, retry, normalization or permission change is selected.

Primary protocol references: https://git-scm.com/docs/gitformat-index and
https://git-scm.com/docs/git-config#Documentation/git-config.txt-indexskipHash.
Independent review checked the primary read-cache.c verify_hdr behavior. This
plan relies on the exact selected versions, not an assumed future Git behavior.
