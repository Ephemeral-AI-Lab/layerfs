# Explicit whole-root topology qualification

Terminology update2026-10-07: the current API/SQL names below use overlay
schema16. Earlier algorithm and proof pins retain their original scope; see
[the naming checkpoint](../issues/307/PRE-S8-TERMINOLOGY-20261007.md).

> **Status:** Current general guide.
> Source addition after `21ff451fa`, 2026-10-07; implementation description,
> not release or installed-Store qualification.

Content's [`qualify_root`](../../crates/layerfs-content/src/filesystem/qualify/walk.rs)
accepts an authenticated object provider, a fresh caller-owned
`IndexedConstructionBacking` scope, a `RootContext` and work counters. It checks
the exact root scope/profile/optional root serial, records every inode in serial
order, then walks reachable directories using an indexed FIFO. Each binding
increments its target's guarded count. Directory aliases and reachable cycles
are refused; the final declared/observed count closure rejects unreachable
inodes and islands. Only a completed pass returns `QualifiedRoot`, whose
`admit` method accepts exactly its own root and matching context.

The sequential inode reader descends once per leaf window and checks sibling
ordering. Directory listing windows hold at most 64 names with an explicit
byte bound. A batch holds at most one inode page, or one listing window's
target changes and newly discovered directories. Per-inode records and the
directory queue grow in indexed backing, while resident traversal state stays
bounded by these windows. Page descent work still depends on tree depth;
this does not promise constant physical I/O per namespace entry.

`QualificationWork` counts inode/directory pages, rows, bindings, record reads,
guarded batches and peak batch changes. A failed pass returns its original
Content/provider refusal without a proof. A used backing scope cannot be
silently reused. The caller owns its operation scope and releases it on either
outcome; the pass does not create another database or perform guessed cleanup.

Counter-custody correction after `d5ef7ba4e`: each call resets its work structure
before any provider call. Incoming observations cannot supply missing bindings
or inflate a proof. The [focused regression](../issues/307/PRE-S8-R2-COUNTER-CUSTODY-20261007.md)
retains the originally reproduced invalid proof and corrected evidence.

This is an explicit paid whole-root operation. Mount retains bounded root
checks and never calls it. The proof covers namespace topology, not authority,
file mappings, file bytes, symlink targets, attribute closure, Save completion
or history publication. Their existing readers and operation contracts remain
responsible for those checks.

The [F0 checkpoint](../issues/307/PRE-S8-F0-20261007.md) links the retained
host and Linux evidence, including malformed roots and the real daemon overlay
operation records adapter. Its canonical object provider is a fixture; an installed
Store running the same pass through daemon ports remains F15.
