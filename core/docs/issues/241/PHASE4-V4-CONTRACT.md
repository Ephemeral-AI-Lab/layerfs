# #241 Phase 4 prospective four-case contract

> **Status:** Current planning checklist; no release candidate exists.

The Phase 3 position sweep passed 264/264 at `8aaf623835cf1384fbf5558e868b8d1b945830bd`.
That is functional evidence. The four Edit→Commit performance and admission
cases below are a separate prospective selection. No v4 timing has been
collected at this contract revision.

- Frozen registry: `core/benchmark/fs-bench-pro/registry/workspace-exec-insert-v4.json`,
  SHA-256 `9decc295b688083db7b1f6899ebbc315eeeddfa28d0fafe68537507d12394fbb`.
  It selects one midpoint 4 KiB insert for each 1 MiB, 10 MiB, 100 MiB and
  capped 500 MiB pristine fixture. The four IDs end in `-exec-v4` and are
  disjoint from v3. The generator seed, input and payload digests, offset,
  range callback expectations, cache contract and G2 targets are inherited
  exactly from the frozen v3 selection; the registry asserts those identities.
- Tool command appends `--output-version 4`. Its exact, untruncated JSON line
  includes post-edit mtime seconds and nanoseconds checked against the same
  open descriptor. The SDK driver requires that line before Commit and retains
  the observed mtime in its performance receipt. The separate verifier binds
  that value and checks published and retained portable metadata, full-file
  digest/size, the pinned canonical root and extent count, Branch head and
  fresh reopen. The fixture mode is `0o640` (`416`). The fixture's absolute
  old mtime is not pinned: its retained version must equal the genesis metadata.
- All operations use the public SDK route. The Edit→Commit timer covers Exec,
  exact output check and Commit. Status, unmount, daemon log capture, Sandbox
  deletion and independent verification are outside it and recorded. The
  complete command hard cap is 15 s; the separate verifier cap is 15 s.
- The complete driver stderr is retained. Its caller/Service and daemon LFT1
  records must match the run, process and Sandbox identity, contain one
  zero-loss summary per producer and prove the edit/commit scope. Resource
  gaps or unavailable windows are reported as unavailable, never as zero.
- Each case gets one sample and one independent verification at the frozen
  source, binary and image identities. Clone setup is an independent writable
  byte copy, never a cold claim. Linux FUSE backing remains `INELIGIBLE` under
  the current cache contract, so a fast raw duration cannot establish G2.

The run report will name the source commit, product/harness/compilation seals,
image digest, every receipt, wall limits, exact outcome and remaining gates.
Historical v3 receipts keep their original status and identity.
