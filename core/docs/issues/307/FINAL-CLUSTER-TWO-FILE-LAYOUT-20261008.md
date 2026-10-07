# Final cluster-two product: proposed file-level layout

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Prepared 2026-10-08 against local `main` `752fc39ac7ae906d3e1e4ae50011c6d4d882cb50`.
> This is a destination proposal after S8/S10/S11/S12/S13, not a statement that
> these modules, native behavior or qualification already exist.

The owner's latest direction is incorporated: the SDK exposes Project, Workspace
and Sandbox APIs. Commands run through ordinary sandbox/container execution or
an external executor. The filesystem daemon owns no Bash supervisor, launcher
mode, per-Exec cgroup, Exec registration or Exec stream/status/cancellation
protocol. It still owns native requests, independent lookup/open/capture custody,
Store/Commit work and exact filesystem teardown.

This document supersedes conflicting folder recommendations in the older
[source organization](SOURCE-ORGANIZATION-S7-S13.md) and the managed-Exec portions
of the [S8 file plan](S8-IMPLEMENTATION-PLAN-20261008.md) for the destination shown
here. The detailed S8 specification/proof reconciliation to caller-owned execution
is still required; this file does not relabel those old proof IDs or receipts.

## 1. Scope and legend

The complete product tree below includes every tracked active-member `src/` and
shipped `sql/` file at the source pin, each package manifest, and proposed native,
SDK and S10 additions. It preserves the current cluster-one owning homes rather
than recreating their implementation. The final product has 13 crates: the current
11 plus actual replacement FUSE and Sandbox. `layerfs-api/sdk` is the path of the
`layerfs-sdk` package; API-core is not reintroduced.

- `[E]`: an existing tracked file's owning home is retained. This does not promise
  that its implementation remains byte-identical through later milestones.
- `[M]`: proposed relocation of existing source, with its exact origin shown.
- `[P]`: proposed destination. Add it only when actual implementation needs that
  responsibility. Names may be adjusted coherently during implementation; this
  is not a requirement to create empty files or a fixed final file-count target.

Every module entry is included. `lib.rs`/`mod.rs` remain declaration/delegation
only and at most 200 physical lines; new implementation files stay at most 999.
A large responsibility splits before the ceiling; no wrapper/interface is added
just to populate the tree. Each crate keeps external `tests/`, `examples/` and
`benches/` as applicable. The next section lists prospective native proof files;
existing test fixtures and immutable evidence are not product source.

The exact destination/origin/blob map and preserved-file hashes are in the
[manifest](checks/final-cluster-two-layout-20261008/02-destination-manifest.json).

## 2. Complete product source tree

```text
└── core/
    └── crates/
        ├── layerfs-api/
        │   └── sdk/
        │       ├── src/
        │       │   ├── control/
        │       │   │   ├── connection.rs  [M] ← core/crates/layerfs-api/sdk/src/control.rs
        │       │   │   └── mod.rs  [P]
        │       │   ├── project/
        │       │   │   ├── api.rs  [P]
        │       │   │   ├── history.rs  [P]
        │       │   │   ├── init.rs  [M] ← core/crates/layerfs-api/sdk/src/init.rs
        │       │   │   ├── init_types.rs  [M] ← core/crates/layerfs-api/sdk/src/init_types.rs
        │       │   │   ├── install.rs  [M] ← core/crates/layerfs-api/sdk/src/install.rs
        │       │   │   ├── install_types.rs  [M] ← core/crates/layerfs-api/sdk/src/install_types.rs
        │       │   │   └── mod.rs  [P]
        │       │   ├── sandbox/
        │       │   │   ├── api.rs  [P]
        │       │   │   ├── exec.rs  [P]
        │       │   │   ├── lifecycle.rs  [P]
        │       │   │   ├── mod.rs  [P]
        │       │   │   └── types.rs  [P]
        │       │   ├── workspace/
        │       │   │   ├── api.rs  [P]
        │       │   │   ├── commit.rs  [P]
        │       │   │   ├── mod.rs  [P]
        │       │   │   ├── mount.rs  [P]
        │       │   │   ├── status.rs  [P]
        │       │   │   ├── types.rs  [P]
        │       │   │   └── unmount.rs  [P]
        │       │   └── lib.rs  [E]
        │       └── Cargo.toml  [E]
        ├── layerfs-bridge/
        │   ├── src/
        │   │   ├── native/
        │   │   │   ├── channel.rs  [E]
        │   │   │   ├── error.rs  [E]
        │   │   │   ├── handshake.rs  [E]
        │   │   │   ├── io.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── profile.rs  [E]
        │   │   │   └── types.rs  [E]
        │   │   ├── control.rs  [E]
        │   │   ├── control_history.rs  [E]
        │   │   ├── control_native.rs  [P]
        │   │   ├── control_reply.rs  [E]
        │   │   ├── control_request.rs  [E]
        │   │   ├── control_types.rs  [E]
        │   │   ├── lib.rs  [E]
        │   │   ├── provision.rs  [E]
        │   │   ├── provision_wire.rs  [E]
        │   │   └── wire.rs  [E]
        │   └── Cargo.toml  [E]
        ├── layerfs-content/
        │   ├── src/
        │   │   ├── contract/
        │   │   │   ├── error.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   └── policy.rs  [E]
        │   │   ├── file/
        │   │   │   ├── cdc/
        │   │   │   │   ├── gear.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   └── zeros.rs  [E]
        │   │   │   ├── construction/
        │   │   │   │   ├── bytes.rs  [E]
        │   │   │   │   ├── chunk_runs.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   └── runs.rs  [E]
        │   │   │   ├── edit/
        │   │   │   │   ├── apply.rs  [E]
        │   │   │   │   ├── backing.rs  [E]
        │   │   │   │   ├── compare.rs  [E]
        │   │   │   │   ├── concat.rs  [E]
        │   │   │   │   ├── draft_codec.rs  [E]
        │   │   │   │   ├── engine.rs  [E]
        │   │   │   │   ├── finish.rs  [E]
        │   │   │   │   ├── input.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   ├── objects.rs  [E]
        │   │   │   │   ├── references.rs  [E]
        │   │   │   │   ├── resolution.rs  [E]
        │   │   │   │   ├── runs.rs  [E]
        │   │   │   │   ├── source.rs  [E]
        │   │   │   │   ├── split.rs  [E]
        │   │   │   │   ├── state.rs  [E]
        │   │   │   │   ├── tree.rs  [E]
        │   │   │   │   └── zero.rs  [E]
        │   │   │   ├── mapping/
        │   │   │   │   ├── build.rs  [E]
        │   │   │   │   ├── codec.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   ├── predecessor.rs  [E]
        │   │   │   │   ├── read.rs  [E]
        │   │   │   │   ├── repeated.rs  [E]
        │   │   │   │   └── types.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── read.rs  [E]
        │   │   │   └── view.rs  [E]
        │   │   ├── filesystem/
        │   │   │   ├── attributes/
        │   │   │   │   ├── build.rs  [E]
        │   │   │   │   ├── codec.rs  [E]
        │   │   │   │   ├── keys.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   ├── patch.rs  [E]
        │   │   │   │   ├── portable.rs  [E]
        │   │   │   │   ├── read.rs  [E]
        │   │   │   │   └── value.rs  [E]
        │   │   │   ├── directory/
        │   │   │   │   ├── changes.rs  [P]
        │   │   │   │   ├── codec.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   ├── read.rs  [E]
        │   │   │   │   └── update.rs  [E]
        │   │   │   ├── inode/
        │   │   │   │   ├── codec.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   ├── read.rs  [E]
        │   │   │   │   └── update.rs  [E]
        │   │   │   ├── qualify/
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   ├── proof.rs  [E]
        │   │   │   │   ├── records.rs  [E]
        │   │   │   │   └── walk.rs  [E]
        │   │   │   ├── references/
        │   │   │   │   ├── backing.rs  [E]
        │   │   │   │   ├── indexed.rs  [E]
        │   │   │   │   ├── indexed_release.rs  [E]
        │   │   │   │   ├── indexed_rows.rs  [E]
        │   │   │   │   ├── indexed_validation.rs  [P]
        │   │   │   │   ├── indexed_wire.rs  [E]
        │   │   │   │   ├── meaning.rs  [E]
        │   │   │   │   ├── merge.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   ├── operation.rs  [E]
        │   │   │   │   ├── record.rs  [E]
        │   │   │   │   ├── reduce.rs  [E]
        │   │   │   │   ├── release.rs  [E]
        │   │   │   │   └── runs.rs  [E]
        │   │   │   ├── rows/
        │   │   │   │   ├── check.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   ├── source.rs  [E]
        │   │   │   │   ├── spool.rs  [E]
        │   │   │   │   ├── stream.rs  [E]
        │   │   │   │   ├── stream_update.rs  [E]
        │   │   │   │   ├── update.rs  [E]
        │   │   │   │   └── view.rs  [E]
        │   │   │   ├── sorted/
        │   │   │   │   ├── budget.rs  [E]
        │   │   │   │   ├── finish.rs  [E]
        │   │   │   │   ├── format.rs  [E]
        │   │   │   │   ├── merge.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   └── page.rs  [E]
        │   │   │   ├── state/
        │   │   │   │   ├── codec.rs  [E]
        │   │   │   │   ├── initial.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   ├── roots.rs  [E]
        │   │   │   │   ├── store.rs  [E]
        │   │   │   │   └── types.rs  [E]
        │   │   │   ├── validate/
        │   │   │   │   ├── backed.rs  [P]
        │   │   │   │   ├── cycles.rs  [E]
        │   │   │   │   ├── entries.rs  [E]
        │   │   │   │   └── incremental.rs  [P]
        │   │   │   ├── identity.rs  [E]
        │   │   │   ├── input.rs  [E]
        │   │   │   ├── limits.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── objects.rs  [E]
        │   │   │   ├── path.rs  [E]
        │   │   │   ├── read.rs  [E]
        │   │   │   ├── root.rs  [E]
        │   │   │   ├── symlink.rs  [E]
        │   │   │   ├── update.rs  [E]
        │   │   │   └── validate.rs  [E]
        │   │   ├── object/
        │   │   │   ├── context/
        │   │   │   │   ├── dispatch.rs  [E]
        │   │   │   │   ├── inode.rs  [E]
        │   │   │   │   ├── mapping.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   ├── namespace.rs  [E]
        │   │   │   │   └── read.rs  [E]
        │   │   │   ├── access.rs  [E]
        │   │   │   ├── admission.rs  [E]
        │   │   │   ├── codec.rs  [E]
        │   │   │   ├── id.rs  [E]
        │   │   │   ├── inode_leaf.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── output.rs  [E]
        │   │   │   └── predecessor.rs  [E]
        │   │   └── lib.rs  [E]
        │   └── Cargo.toml  [E]
        ├── layerfs-daemon/
        │   ├── src/
        │   │   ├── bin/
        │   │   │   └── layerfs-daemon.rs  [P]
        │   │   ├── control/
        │   │   │   ├── attach.rs  [P]
        │   │   │   ├── commit.rs  [P]
        │   │   │   ├── failure.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── native_state.rs  [P]
        │   │   │   ├── operations.rs  [E]
        │   │   │   ├── registry.rs  [E]
        │   │   │   ├── serve.rs  [E]
        │   │   │   ├── status.rs  [E]
        │   │   │   ├── types.rs  [E]
        │   │   │   └── unmount.rs  [P]
        │   │   ├── native/
        │   │   │   ├── config.rs  [P]
        │   │   │   ├── mod.rs  [P]
        │   │   │   ├── readiness.rs  [P]
        │   │   │   └── session.rs  [P]
        │   │   ├── overlay/
        │   │   │   ├── captured_run_port.rs  [E]
        │   │   │   ├── commands.rs  [E]
        │   │   │   ├── credits.rs  [E]
        │   │   │   ├── file_port.rs  [E]
        │   │   │   ├── indexed_operation_record.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── native_ownership_commands.rs  [P]
        │   │   │   ├── operation_record_port.rs  [E]
        │   │   │   ├── owner.rs  [E]
        │   │   │   ├── queue.rs  [E]
        │   │   │   ├── read_commands.rs  [P]
        │   │   │   └── read_port.rs  [E]
        │   │   ├── request/
        │   │   │   ├── steps/
        │   │   │   │   ├── attributes.rs  [P]
        │   │   │   │   ├── directory.rs  [P]
        │   │   │   │   ├── lookup.rs  [P]
        │   │   │   │   ├── mod.rs  [P]
        │   │   │   │   ├── mutation.rs  [P]
        │   │   │   │   ├── open.rs  [P]
        │   │   │   │   ├── read.rs  [P]
        │   │   │   │   └── refused.rs  [P]
        │   │   │   ├── credits.rs  [P]
        │   │   │   ├── mod.rs  [P]
        │   │   │   ├── queue.rs  [P]
        │   │   │   ├── types.rs  [P]
        │   │   │   └── workers.rs  [P]
        │   │   ├── service/
        │   │   │   ├── completion.rs  [E]
        │   │   │   ├── job_sql.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── native_observations.rs  [P]
        │   │   │   ├── observations.rs  [E]
        │   │   │   └── startup.rs  [E]
        │   │   ├── store/
        │   │   │   ├── bind.rs  [E]
        │   │   │   ├── commit.rs  [E]
        │   │   │   ├── commit_types.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── open.rs  [E]
        │   │   │   ├── operation.rs  [E]
        │   │   │   ├── ports.rs  [E]
        │   │   │   ├── read_service.rs  [P]
        │   │   │   ├── settle.rs  [E]
        │   │   │   └── types.rs  [E]
        │   │   ├── bootstrap.rs  [E]
        │   │   ├── install.rs  [E]
        │   │   ├── install_file.rs  [E]
        │   │   ├── install_types.rs  [E]
        │   │   └── lib.rs  [E]
        │   └── Cargo.toml  [E]
        ├── layerfs-fuse/
        │   ├── src/
        │   │   ├── mount/
        │   │   │   ├── mod.rs  [P]
        │   │   │   ├── profile.rs  [P]
        │   │   │   ├── session.rs  [P]
        │   │   │   └── syscalls.rs  [P]
        │   │   ├── request/
        │   │   │   ├── decode.rs  [P]
        │   │   │   ├── mod.rs  [P]
        │   │   │   ├── reply.rs  [P]
        │   │   │   └── types.rs  [P]
        │   │   ├── attributes.rs  [P]
        │   │   ├── diagnostics.rs  [P]
        │   │   └── lib.rs  [P]
        │   └── Cargo.toml  [P]
        ├── layerfs-history/
        │   ├── src/
        │   │   ├── contract/
        │   │   │   ├── catalog.rs  [E]
        │   │   │   ├── error.rs  [E]
        │   │   │   ├── identity.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── query.rs  [E]
        │   │   │   └── records.rs  [E]
        │   │   └── lib.rs  [E]
        │   └── Cargo.toml  [E]
        ├── layerfs-overlay/
        │   ├── sql/
        │   │   ├── accounting.sql  [E]
        │   │   ├── native_ownership.sql  [P]
        │   │   └── schema.sql  [E]
        │   ├── src/
        │   │   ├── contract/
        │   │   │   ├── custody.rs  [E]
        │   │   │   ├── error.rs  [E]
        │   │   │   ├── indexed_operation_record.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   └── types.rs  [E]
        │   │   ├── database/
        │   │   │   ├── accounting.rs  [E]
        │   │   │   ├── allocation.rs  [E]
        │   │   │   ├── connection.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── profile.rs  [E]
        │   │   │   ├── startup.rs  [E]
        │   │   │   └── statements.rs  [E]
        │   │   ├── diagnostics/
        │   │   │   ├── access_plan.rs  [E]
        │   │   │   ├── captured_runs.rs  [E]
        │   │   │   ├── indexed_operation_record_plan.rs  [E]
        │   │   │   ├── lifetime_plan.rs  [E]
        │   │   │   ├── metrics.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── native_ownership_plan.rs  [P]
        │   │   │   ├── payload.rs  [E]
        │   │   │   ├── source_plan.rs  [E]
        │   │   │   └── startup.rs  [E]
        │   │   ├── lifetime/
        │   │   │   ├── captured_reader.rs  [E]
        │   │   │   ├── close.rs  [E]
        │   │   │   ├── composition.rs  [E]
        │   │   │   ├── file_owners.rs  [E]
        │   │   │   ├── frontier.rs  [E]
        │   │   │   ├── generation.rs  [E]
        │   │   │   ├── indexed_operation_record.rs  [E]
        │   │   │   ├── lookup.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── native_group.rs  [P]
        │   │   │   ├── native_lookup.rs  [P]
        │   │   │   ├── operation.rs  [E]
        │   │   │   ├── operation_record.rs  [E]
        │   │   │   ├── orphan.rs  [E]
        │   │   │   ├── retire.rs  [P]
        │   │   │   ├── source.rs  [E]
        │   │   │   └── workspace.rs  [E]
        │   │   ├── maintenance/
        │   │   │   ├── debt.rs  [P]
        │   │   │   ├── garbage.rs  [E]
        │   │   │   ├── indexed_operation_record.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── native_ownership.rs  [P]
        │   │   │   ├── orphan.rs  [E]
        │   │   │   ├── ready.rs  [E]
        │   │   │   ├── reclaim.rs  [E]
        │   │   │   └── source_wait.rs  [E]
        │   │   ├── namespace/
        │   │   │   ├── captured_namespace.rs  [P]
        │   │   │   ├── compound.rs  [E]
        │   │   │   ├── directory_entry.rs  [E]
        │   │   │   ├── inode.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   └── read_compound.rs  [P]
        │   │   ├── payload/
        │   │   │   ├── access.rs  [E]
        │   │   │   ├── captured_runs.rs  [E]
        │   │   │   ├── captured_types.rs  [E]
        │   │   │   ├── cells.rs  [E]
        │   │   │   ├── layers.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   └── stream.rs  [E]
        │   │   └── lib.rs  [E]
        │   └── Cargo.toml  [E]
        ├── layerfs-persistence/
        │   ├── sql/
        │   │   └── sqlite/
        │   │       ├── acquisition/
        │   │       │   ├── queries/
        │   │       │   │   ├── abandoned.sql  [E]
        │   │       │   │   ├── advance.sql  [E]
        │   │       │   │   ├── begin.sql  [E]
        │   │       │   │   ├── bind_native.sql  [E]
        │   │       │   │   ├── bindings_first.sql  [E]
        │   │       │   │   ├── bindings_next.sql  [E]
        │   │       │   │   ├── charge.sql  [E]
        │   │       │   │   ├── claim_epoch.sql  [E]
        │   │       │   │   ├── complete_file.sql  [E]
        │   │       │   │   ├── credit.sql  [E]
        │   │       │   │   ├── directories.sql  [E]
        │   │       │   │   ├── directory_path.sql  [E]
        │   │       │   │   ├── directory_sizes.sql  [E]
        │   │       │   │   ├── discard_entries.sql  [E]
        │   │       │   │   ├── discard_entry_keys.sql  [E]
        │   │       │   │   ├── discard_entry_tail.sql  [E]
        │   │       │   │   ├── discard_native.sql  [E]
        │   │       │   │   ├── discard_native_keys.sql  [E]
        │   │       │   │   ├── discard_native_tail.sql  [E]
        │   │       │   │   ├── entries_first.sql  [E]
        │   │       │   │   ├── entries_next.sql  [E]
        │   │       │   │   ├── entry_dependencies.sql  [E]
        │   │       │   │   ├── file_roots.sql  [E]
        │   │       │   │   ├── job.sql  [E]
        │   │       │   │   ├── job_sizes.sql  [E]
        │   │       │   │   ├── jobs.sql  [E]
        │   │       │   │   ├── owner.sql  [E]
        │   │       │   │   ├── place_entry.sql  [E]
        │   │       │   │   ├── put_entries.sql  [E]
        │   │       │   │   ├── put_entry.sql  [E]
        │   │       │   │   ├── put_native.sql  [E]
        │   │       │   │   ├── release.sql  [E]
        │   │       │   │   ├── release_abandoned.sql  [E]
        │   │       │   │   ├── set_directory_root.sql  [E]
        │   │       │   │   ├── unplaced_first.sql  [E]
        │   │       │   │   └── unplaced_next.sql  [E]
        │   │       │   └── schema.sql  [E]
        │   │       ├── queries/
        │   │       │   └── history/
        │   │       │       ├── allocation_advance_scope.sql  [E]
        │   │       │       ├── allocation_insert_scope.sql  [E]
        │   │       │       ├── allocation_scope_by_id.sql  [E]
        │   │       │       ├── branch_branch_by_id.sql  [E]
        │   │       │       ├── branch_branch_first_page.sql  [E]
        │   │       │       ├── branch_branch_name_taken.sql  [E]
        │   │       │       ├── branch_branch_next_page.sql  [E]
        │   │       │       ├── branch_insert_branch.sql  [E]
        │   │       │       ├── commit_advance_branch.sql  [E]
        │   │       │       ├── commit_commit_by_id.sql  [E]
        │   │       │       ├── commit_delete_stage.sql  [E]
        │   │       │       ├── commit_insert_commit.sql  [E]
        │   │       │       ├── layerstack_advance_stack.sql  [E]
        │   │       │       ├── layerstack_insert_layer.sql  [E]
        │   │       │       ├── layerstack_insert_stack.sql  [E]
        │   │       │       ├── layerstack_layer_by_id.sql  [E]
        │   │       │       ├── layerstack_layer_by_source.sql  [E]
        │   │       │       ├── layerstack_layer_root.sql  [E]
        │   │       │       ├── layerstack_stack_by_id.sql  [E]
        │   │       │       ├── layerstack_stack_first_page.sql  [E]
        │   │       │       ├── layerstack_stack_head.sql  [E]
        │   │       │       ├── layerstack_stack_name_taken.sql  [E]
        │   │       │       ├── layerstack_stack_next_page.sql  [E]
        │   │       │       ├── staging_bump_token.sql  [E]
        │   │       │       ├── staging_delete_stage.sql  [E]
        │   │       │       ├── staging_insert_stage.sql  [E]
        │   │       │       ├── staging_next_token.sql  [E]
        │   │       │       ├── staging_stage_by_workspace.sql  [E]
        │   │       │       ├── staging_stage_first_page.sql  [E]
        │   │       │       └── staging_stage_next_page.sql  [E]
        │   │       ├── history.sql  [E]
        │   │       ├── metadata.sql  [E]
        │   │       ├── objects.sql  [E]
        │   │       ├── objects_units.sql  [E]
        │   │       └── objects_units_index.sql  [E]
        │   ├── src/
        │   │   ├── backend/
        │   │   │   ├── sqlite/
        │   │   │   │   ├── acquisition/
        │   │   │   │   │   ├── accounting.rs  [E]
        │   │   │   │   │   ├── cleanup.rs  [E]
        │   │   │   │   │   ├── entry_windows.rs  [E]
        │   │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   │   ├── reads.rs  [E]
        │   │   │   │   │   ├── root_windows.rs  [E]
        │   │   │   │   │   ├── statements.rs  [E]
        │   │   │   │   │   └── writes.rs  [E]
        │   │   │   │   ├── connection.rs  [E]
        │   │   │   │   ├── file_control.rs  [E]
        │   │   │   │   ├── metadata_allocation.rs  [E]
        │   │   │   │   ├── metadata_locations.rs  [E]
        │   │   │   │   ├── metadata_policy.rs  [E]
        │   │   │   │   ├── metadata_pooling.rs  [E]
        │   │   │   │   ├── metadata_signatures.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   ├── objects_read.rs  [E]
        │   │   │   │   ├── objects_selection.rs  [E]
        │   │   │   │   ├── prepared.rs  [E]
        │   │   │   │   ├── profile.rs  [E]
        │   │   │   │   ├── publish.rs  [E]
        │   │   │   │   ├── query.rs  [E]
        │   │   │   │   ├── reclamation.rs  [E]
        │   │   │   │   ├── rows.rs  [E]
        │   │   │   │   ├── schema.rs  [E]
        │   │   │   │   ├── seal.rs  [E]
        │   │   │   │   ├── seal_allocation.rs  [E]
        │   │   │   │   ├── statement_work.rs  [E]
        │   │   │   │   ├── transaction.rs  [E]
        │   │   │   │   ├── unit_io.rs  [E]
        │   │   │   │   ├── unit_layout.rs  [E]
        │   │   │   │   ├── units_publish.rs  [E]
        │   │   │   │   └── units_read.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   └── records.rs  [E]
        │   │   ├── history/
        │   │   │   ├── allocation.rs  [E]
        │   │   │   ├── bindings.rs  [E]
        │   │   │   ├── branch.rs  [E]
        │   │   │   ├── catalog.rs  [E]
        │   │   │   ├── commit.rs  [E]
        │   │   │   ├── layerstack.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── provider.rs  [E]
        │   │   │   ├── rows.rs  [E]
        │   │   │   ├── staging.rs  [E]
        │   │   │   └── transaction.rs  [E]
        │   │   ├── metadata/
        │   │   │   ├── allocation.rs  [E]
        │   │   │   ├── locations.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── policy.rs  [E]
        │   │   │   ├── pooling.rs  [E]
        │   │   │   └── signatures.rs  [E]
        │   │   ├── objects/
        │   │   │   ├── mod.rs  [E]
        │   │   │   └── read.rs  [E]
        │   │   ├── storage/
        │   │   │   ├── acquisition/
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   └── provider.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── provider.rs  [E]
        │   │   │   └── publication.rs  [E]
        │   │   ├── store/
        │   │   │   ├── config.rs  [E]
        │   │   │   ├── handles.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── open.rs  [E]
        │   │   │   └── seal.rs  [E]
        │   │   └── lib.rs  [E]
        │   └── Cargo.toml  [E]
        ├── layerfs-project/
        │   ├── src/
        │   │   ├── import/
        │   │   │   ├── backing.rs  [E]
        │   │   │   ├── batch.rs  [E]
        │   │   │   ├── error.rs  [E]
        │   │   │   ├── files.rs  [E]
        │   │   │   ├── init.rs  [E]
        │   │   │   ├── metadata.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── namespace.rs  [E]
        │   │   │   ├── scan.rs  [E]
        │   │   │   ├── source.rs  [E]
        │   │   │   └── work.rs  [E]
        │   │   └── lib.rs  [E]
        │   └── Cargo.toml  [E]
        ├── layerfs-sandbox/
        │   ├── src/
        │   │   ├── access/
        │   │   │   ├── identity.rs  [P]
        │   │   │   ├── mod.rs  [P]
        │   │   │   └── visibility.rs  [P]
        │   │   ├── backend/
        │   │   │   ├── docker/
        │   │   │   │   ├── create.rs  [P]
        │   │   │   │   ├── exec.rs  [P]
        │   │   │   │   ├── inspect.rs  [P]
        │   │   │   │   ├── logs.rs  [P]
        │   │   │   │   ├── mod.rs  [P]
        │   │   │   │   ├── ports.rs  [P]
        │   │   │   │   ├── remove.rs  [P]
        │   │   │   │   └── streams.rs  [P]
        │   │   │   └── mod.rs  [P]
        │   │   ├── lifecycle/
        │   │   │   ├── create.rs  [P]
        │   │   │   ├── delete.rs  [P]
        │   │   │   ├── mod.rs  [P]
        │   │   │   └── readiness.rs  [P]
        │   │   ├── owner/
        │   │   │   ├── config.rs  [P]
        │   │   │   ├── identity.rs  [P]
        │   │   │   ├── mod.rs  [P]
        │   │   │   └── routing.rs  [P]
        │   │   ├── types/
        │   │   │   ├── execution.rs  [P]
        │   │   │   ├── failure.rs  [P]
        │   │   │   ├── mod.rs  [P]
        │   │   │   └── sandbox.rs  [P]
        │   │   └── lib.rs  [P]
        │   └── Cargo.toml  [P]
        ├── layerfs-storage/
        │   ├── src/
        │   │   ├── encoding/
        │   │   │   ├── delta/
        │   │   │   │   ├── candidates.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   ├── read.rs  [E]
        │   │   │   │   ├── record.rs  [E]
        │   │   │   │   └── select.rs  [E]
        │   │   │   ├── pool/
        │   │   │   │   ├── counters.rs  [E]
        │   │   │   │   ├── delta.rs  [E]
        │   │   │   │   ├── index.rs  [E]
        │   │   │   │   ├── leaf.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   ├── read.rs  [E]
        │   │   │   │   └── value_group.rs  [E]
        │   │   │   ├── codec.rs  [E]
        │   │   │   ├── decode.rs  [E]
        │   │   │   ├── full.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── pack_cache.rs  [E]
        │   │   │   └── pack_units.rs  [E]
        │   │   ├── pack/
        │   │   │   ├── assemble.rs  [E]
        │   │   │   ├── layout.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   └── placement.rs  [E]
        │   │   ├── port/
        │   │   │   ├── acquisition/
        │   │   │   │   ├── contract.rs  [E]
        │   │   │   │   ├── mod.rs  [E]
        │   │   │   │   ├── rows.rs  [E]
        │   │   │   │   └── work.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── pack.rs  [E]
        │   │   │   ├── persistence.rs  [E]
        │   │   │   ├── read_pack.rs  [E]
        │   │   │   ├── read_scoped.rs  [E]
        │   │   │   └── read_selection.rs  [E]
        │   │   ├── read/
        │   │   │   ├── counters.rs  [E]
        │   │   │   ├── fetch.rs  [E]
        │   │   │   ├── length.rs  [E]
        │   │   │   ├── locator_cache.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── objects.rs  [E]
        │   │   │   ├── prefetch.rs  [E]
        │   │   │   ├── provider.rs  [E]
        │   │   │   └── units.rs  [E]
        │   │   ├── save/
        │   │   │   ├── batch.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── operation.rs  [E]
        │   │   │   ├── pooled.rs  [E]
        │   │   │   ├── profile.rs  [E]
        │   │   │   ├── provider.rs  [E]
        │   │   │   ├── publication.rs  [E]
        │   │   │   ├── reservation.rs  [E]
        │   │   │   ├── seal.rs  [E]
        │   │   │   ├── select.rs  [E]
        │   │   │   ├── source.rs  [E]
        │   │   │   ├── state.rs  [E]
        │   │   │   ├── wave.rs  [E]
        │   │   │   └── work.rs  [E]
        │   │   ├── store/
        │   │   │   ├── error.rs  [E]
        │   │   │   ├── handle.rs  [E]
        │   │   │   ├── location.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── policy.rs  [E]
        │   │   │   ├── reservations.rs  [E]
        │   │   │   └── source.rs  [E]
        │   │   └── lib.rs  [E]
        │   └── Cargo.toml  [E]
        ├── layerfs-telemetry/
        │   ├── src/
        │   │   ├── output/
        │   │   │   ├── collector.rs  [E]
        │   │   │   ├── encode.rs  [E]
        │   │   │   ├── health.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── queue.rs  [E]
        │   │   │   └── retention.rs  [E]
        │   │   ├── platform/
        │   │   │   ├── linux.rs  [E]
        │   │   │   ├── macos.rs  [E]
        │   │   │   └── mod.rs  [E]
        │   │   ├── runtime/
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── monitor.rs  [E]
        │   │   │   ├── observation.rs  [E]
        │   │   │   ├── operation.rs  [E]
        │   │   │   └── session.rs  [E]
        │   │   ├── timer/
        │   │   │   ├── bounded_json.rs  [E]
        │   │   │   ├── format.rs  [E]
        │   │   │   ├── json.rs  [E]
        │   │   │   ├── limits.rs  [E]
        │   │   │   ├── mod.rs  [E]
        │   │   │   ├── recording.rs  [E]
        │   │   │   ├── report.rs  [E]
        │   │   │   └── scope.rs  [E]
        │   │   └── lib.rs  [E]
        │   └── Cargo.toml  [E]
        └── layerfs-workspace/
            ├── src/
            │   ├── base/
            │   │   ├── cache.rs  [E]
            │   │   ├── client.rs  [E]
            │   │   ├── file.rs  [E]
            │   │   ├── mod.rs  [E]
            │   │   └── view.rs  [E]
            │   ├── construction/
            │   │   ├── captured/
            │   │   │   ├── context.rs  [E]
            │   │   │   ├── mod.rs  [E]
            │   │   │   ├── normalize.rs  [E]
            │   │   │   ├── owner.rs  [E]
            │   │   │   ├── scan.rs  [E]
            │   │   │   ├── source.rs  [E]
            │   │   │   └── state.rs  [E]
            │   │   ├── namespace/
            │   │   │   ├── assemble.rs  [P]
            │   │   │   ├── cursor.rs  [P]
            │   │   │   ├── directories.rs  [P]
            │   │   │   ├── inodes.rs  [P]
            │   │   │   ├── mod.rs  [P]
            │   │   │   └── normalize.rs  [P]
            │   │   ├── scratch/
            │   │   │   ├── mod.rs  [P]
            │   │   │   ├── records.rs  [P]
            │   │   │   └── release.rs  [P]
            │   │   ├── context.rs  [P]
            │   │   ├── driver.rs  [P]
            │   │   ├── mod.rs  [E]
            │   │   ├── outcome.rs  [P]
            │   │   └── records.rs  [E]
            │   ├── mutation/
            │   │   ├── driver.rs  [E]
            │   │   ├── eval.rs  [E]
            │   │   ├── facts.rs  [E]
            │   │   ├── job.rs  [E]
            │   │   └── mod.rs  [E]
            │   ├── operations/
            │   │   ├── file/
            │   │   │   ├── mod.rs  [E]
            │   │   │   ├── read.rs  [E]
            │   │   │   └── write.rs  [E]
            │   │   ├── namespace/
            │   │   │   ├── create.rs  [E]
            │   │   │   ├── list.rs  [E]
            │   │   │   ├── mod.rs  [E]
            │   │   │   ├── remove.rs  [E]
            │   │   │   └── rename.rs  [E]
            │   │   ├── attributes.rs  [E]
            │   │   ├── mod.rs  [E]
            │   │   ├── read_plan.rs  [P]
            │   │   └── types.rs  [E]
            │   ├── ports/
            │   │   ├── captured_namespace.rs  [P]
            │   │   ├── captured_runs.rs  [E]
            │   │   ├── files.rs  [E]
            │   │   ├── lengths.rs  [E]
            │   │   ├── mod.rs  [E]
            │   │   ├── operation_record.rs  [E]
            │   │   └── overlay.rs  [E]
            │   ├── workspace/
            │   │   ├── install.rs  [E]
            │   │   ├── mod.rs  [E]
            │   │   ├── serials.rs  [E]
            │   │   ├── state.rs  [E]
            │   │   └── view.rs  [E]
            │   └── lib.rs  [E]
            └── Cargo.toml  [E]
```

## 3. Deepest native and integrated proof destinations

These are proposed external test homes. Existing active tests and qualifying
unchanged receipts are reused where applicable; excluded predecessor tests must
be ported or retired explicitly. No proof has run for this layout document.

```text
└── core/
    └── crates/
        ├── layerfs-api/
        │   └── sdk/
        │       └── tests/
        │           ├── project_api.rs  [P]
        │           ├── sandbox_api.rs  [P]
        │           └── workspace_api.rs  [P]
        ├── layerfs-daemon/
        │   └── tests/
        │       ├── external_executor.rs  [P]
        │       ├── mounted_commit.rs  [P]
        │       ├── mounted_concurrency.rs  [P]
        │       └── native_drain.rs  [P]
        ├── layerfs-fuse/
        │   └── tests/
        │       ├── native_mmap.rs  [P]
        │       ├── native_mount.rs  [P]
        │       ├── native_mutations.rs  [P]
        │       ├── native_requests.rs  [P]
        │       └── native_teardown.rs  [P]
        ├── layerfs-overlay/
        │   └── tests/
        │       └── native_ownership.rs  [P]
        ├── layerfs-sandbox/
        │   └── tests/
        │       ├── lifecycle.rs  [P]
        │       ├── normal_exec.rs  [P]
        │       └── workspace_visibility.rs  [P]
        └── layerfs-workspace/
            └── tests/
                ├── captured_namespace.rs  [P]
                └── incremental_namespace.rs  [P]
```

## 4. Ownership and shared construction

| Home | Responsibility |
| --- | --- |
| SDK `project/` | ProjectApi; existing Init, seal/install manifest and fork/history control |
| SDK `workspace/` | WorkspaceApi; mount/location/Commit/status/unmount. An optional exec convenience resolves the mount directory and delegates ordinary Sandbox execution |
| SDK `sandbox/` | SandboxApi facade over actual sandbox lifecycle/execution; no Store data service |
| Sandbox `backend/docker/` | Concrete ordinary container execution and standard streams/results. The caller/runtime owns exit and explicit cancellation; a lost result is never automatic re-execution |
| Sandbox `access/` | Establish user identity, mount visibility and Store/overlay/credential protection. Actual platform proof is required; a folder is not isolation evidence |
| FUSE `mount/` and `request/` | Only fuser-typed implementation; direct mount/abort/plain detach, callback conversion and one reply attempt |
| Daemon `request/` | Bounded receive/handoff custody, fair runnable filesystem service and exact result disposal |
| Daemon `control/` | One registry and explicit lifecycle/Commit admission. No process lifetime is inferred from command registration or shell exit |
| Daemon `overlay/` | The existing single fair SQL owner and typed command adapters |
| Daemon `store/commit.rs` | Existing capture → constructor → Save finish → History publication → known local install orchestration. Extend this owner rather than adding a second Commit driver |
| Workspace `construction/captured/` | Existing regular-file normalization from authenticated retained base plus final overlay changes |
| Workspace `construction/namespace/` | Complete captured names, links, metadata and changed file roots through bounded cursors |
| Workspace `construction/scratch/` | Attempt-scoped operation records and exact release; no global Init acquisition backing |
| Content `filesystem/` and `file/` | Canonical algorithms, owning validation, backed reducers and file edits |
| Overlay `lifetime/` and `maintenance/` | Independent lookup/open/capture/processing custody and bounded last-owner cleanup |
| Storage / History / Persistence | Immutable Save/encoding, history publication semantics, concrete global Store provider |

```text
 ProjectApi.init → Project import ───────────────┐
                                               ├→ Content → Storage Save → Persistence
 Workspace captured files + namespace ──────────┘
      daemon store/commit: Save finish → History publication → local base install

 ordinary Bash / external executor → kernel → FUSE → daemon request service
                                               → Workspace / Overlay / Store
```

The two input routes share canonical construction libraries and formats while
retaining their own input and publication owners. Commit does not invoke native
Project Init or materialize a temporary native tree. Full captured namespace
assembly, incremental topology, failure disposition and mounted survival remain
real S10 work. Source reuse alone does not close their gates. See the
[shared construction guidance](../../../AGENTS.md#shared-construction-and-completion-boundaries)
and [current Store Commit composition](../../architecture/65-store-commit-composition.md).

Normal filesystem access has no Exec identity requirement. Bash exit does not
prove descriptors/mappings or descendants are gone. Kernel mount-busy evidence
and daemon filesystem/Commit guards govern unmount. Forced connection teardown
does not implicitly kill caller-owned processes or resolve unknown Commit state.

## 5. Qualification, runtime files and final retirement

The existing `core/benchmark/fs-bench-pro/` runner, families, registry, shared
helpers, diagnostics and tests remain development/qualification owners. New
integrated routes extend those existing families at prospective identities;
no harness-selected behavior enters product source. `core/tools/` retains
scoped provenance/boundary checks and the root LOC counter remains reproducible.
Architecture documentation describes implemented source; issue plans/receipts
retain proposed/measured/failed/unrun distinctions.

Database placement is separate from repository layout:

```text
 named in-VM shared Store volume/
 └── store.sqlite             Disposable / WAL / OFF; shared daemon processes
                              live WAL/SHM sidecars have normal SQLite ownership

 daemon-local backing/
 └── overlay.sqlite           one owner connection; Workspace-prefixed mutable state
                              MEMORY / OFF / EXCLUSIVE; no Workspace sync

 sandbox-visible Workspace mounts/
 ├── workspace-A/             ordinary filesystem view
 └── workspace-B/             ordinary filesystem view
```

There is no database per Workspace, Commit or command. The host initializes and
installs one sealed Store and subsequently sends control calls. Daemon Store
adapters use opened Storage/History ports; SQLite paths and concrete provider
opening stay at persistence/application composition. Native Store files never
live under the repository bind mount. Durable execution stays disabled until
explicit owner reauthorization.

After S12 replacement coverage and acceptance, S11/S13 retire the superseded
core predecessors, excluded Server/old FUSE/Sandbox wiring and root `crates/`.
The old SDK host `client/`/`runtime/`, Bridge data codec/framing, API-core and
Daemon `upstream/` are already retired and are absent from this destination.
No daemon `exec/`, command launcher or custom Exec wire appears here.
Keep shared root `.cargo/config.toml`, required documentation/tools/patch
provenance, the authorized `core/vendor/fuser-0.18.0/`, and immutable historical
receipts/Git identities. Obsolete reference manifests/build/test entrypoints are
retired only after their dependency audit; no removal is performed by this plan.

## 6. Preparation checks and remaining reconciliation

This document is checked against the exact current active source inventory,
destination collisions, current-source origin coverage, all proposed module
entries, forbidden retired/daemon-execution destinations, local links/anchors,
status and whitespace. Both already-modified AGENTS guides and three protected
untracked notes remain byte-identical and unstaged by this checkpoint.

No product implementation, relocation, manifest activation, database schema,
dependency, build, runtime test, mount or performance measurement is performed.
Final file counts are a planning inventory, not production LOC or a budget.
Implementation must still reconcile S8 control/status/drain/proof requirements
to caller-owned execution, complete the real Sandbox API/backend integration,
finish S10 normalization and prove S12 before S13 retirement.

## 7. R0 implementation-boundary amendment, 2026-10-08

The original destination manifest/receipt remains unchanged at its source pin.
R0 read-only construction review identifies one additional real adapter home:
`layerfs-daemon/src/overlay/captured_namespace_port.rs`, proposed (P), which
implements Workspace's neutral retained captured-namespace port through the
existing OwnerClient/typed Commands and preserves original attempted Completion
custody. It introduces no second SQL owner or mutable graph. Exact destination
amendment is [retained separately](checks/r0-owner-reconciliation-20261008/04-destination-amendment.json).
The original planning inventory remains 523 E / 5 M / 108 P; this prospective
addition is +1 P, yielding 109 P, and creates no implementation or LOC.

Actual Sandbox replacement also preserves/relocates excluded source before path
replacement, with exact migration accounting; it never activates the old manifest
with its retired API-core/host-service dependencies. Current
[S8 implementation plan](S8-IMPLEMENTATION-PLAN-20261008.md#2-cargo-activation)
and [R0–R9 ledger](ROLLOUT-LEDGER-20261008.md) govern activation/coverage.
