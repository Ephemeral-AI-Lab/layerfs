# SP1 append-only implementation log

> Status: Dated planning checkpoint; not release evidence or a product contract.

## L1 — unchanged producer fixture seal, 2026-10-02

Source inspected: `8c926b9392f3636ae156236dc26d0510ee069d8d`, clean
`codex/phase6-sp1` worktree. Existing S2 worktree and edits were untouched.
Before product edits, an external acquisition test ran the unchanged C1/C2
producer. The [manifest](evidence/old-producer-v1/manifest.json) pins its binary,
acquisition source, command, canonical IDs/bytes, original SQLite packs and
value groups. These are correctness fixtures, not benchmark samples.

Command: `SP1_SEAL_OUT=<fresh evidence/old-producer-v1> cargo +1.85.1 test
--manifest-path core/Cargo.toml --locked -p layerfs-storage --test
sp1_fixture_seal -- --ignored --nocapture`. PASS, one acquisition; product
compilation unchanged. The acquisition target is explicitly ignored in routine
checks so a candidate cannot silently regenerate the independent oracle.
The original fixture is retained without replacement.

Actual old outcomes: whole base FULL and edited whole PREFIX; chunk base FULL,
two edited chunks PREFIX, unrelated chunk FULL after one losing trial; pooled
40-row leaves one FULL then three deltas, each changed leaf reuses36 values and
introduces4. Same-save whole candidate yields one FULL, one PREFIX and one
trial. Boundary roots/partitions at131071/131072 and200000 bytes are recorded.
Additional integrated predecessor/metadata expectations remain unsealed; this
checkpoint does not claim the entire four-witness preregistration is complete.

Current checks: this explicit acquisition PASS; no candidate C2/adapter check,
MinIO read, SDK/FUSE or history qualification yet. SP1 runtime remains PARTIAL.
Numeric timing and physical-memory/storage admission remain NOT_RUN.

Source review allocated a captured read boundary shared by SQLite/MinIO, with
owner/order/generation cache identities and explicit validation before cached
answers. Global schema and action allocation will be recorded with their
implementing source. Current profile observed: C2schema10/format1, global
P6L2/version2, P6META7/actions0–7/statistics8. No silent migration is selected.

Resource finding: eager encode16MiB cannot coexist with any Workspace data
within16MiB if charged there. A serialized daemon operation owns codec/caches
and indexes; Workspace pending/read results retain existing byte bounds, and
allocation/physical admission is still open. No worker, timeout or quota change.

Next: neutral C2 reader/selector/pool access and global save/closure catalog,
followed by complete real-provider reader and bounded private writer.
