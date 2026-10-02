# SP1 implementation handoff

> **Status: Current planning checklist; no release candidate exists.**

Owner-requested specification/test packet, 2026-10-02. SP1 implementation is
planned; this packet is not a new code, provider, speed or storage PASS.
Owning [#295](https://github.com/Ephemeral-AI-Lab/layerfs/issues/295) is a native
sub-issue of [#293](https://github.com/Ephemeral-AI-Lab/layerfs/issues/293).

## Read order

1. [Scope and dependency](../STORAGE-PARITY-SPEC.md).
2. [Implementation and interfaces](IMPLEMENTATION.md).
3. [Minimal tests and stride guidance](test.md).
4. [Independent review and validation](REVIEW.md).
5. [Historical storage baseline](../HISTORY-STORAGE-BASELINE.md) and
   [exact receipt-copy manifest](evidence/history-baseline/manifest.json).

Source audit is against published parent
`8c926b9392f3636ae156236dc26d0510ee069d8d`, branch
`codex/phase6-metadata-experiments`. Actual source is linked from the documents.
Separate local S2 code/docs are uncommitted and unverified; this documentation
checkpoint excludes them. Current prototype packs FULL-only records and supplies
no delta base to its decoder. Proposed adapter signatures and new tests are not
present implementations or registered benchmark selections.

Retain existing C1 canonical content and C2 encoding/selection/packing policy.
The strict owner revision supersedes the earlier all-object MinIO packet. MinIO
stores sealed file-content packs only; global SQLite stores all canonical
filesystem metadata and pooled values/groups, locators and required indexing,
and performs conditional history publication. Canonical identities remain intact.
Logical producer/caller provenance handles metadata using content-object grammar
and identical IDs used in both storage domains. C2's
current SQLite pack-access coupling needs a narrow first-party seam. Preserve
same-save private-base visibility without a network request or tiny pack per
candidate. A source-reviewed protocol/schema allocation is required before
mutating that interface; preserve thin trusted publication.

This lane preserves exact object reuse, eligible FULL/PREFIX selection,
compression, metadata pooling, authenticated bounded chains, retained history and
failure custody. S2 daemon metadata and C1 growing unfinished-draft state remain
separate dependencies. A shared object backend does not itself enable concurrent
Exec/Workspaces, live successor installation or cloud durability.

History schedules 10/3/1 are three independent component workloads. Preserve
r046/r047 identities and statuses. Read-only historical receipt copies are exact
and hashed; no original performance arm was rerun. Future MinIO profile accounting
must include required storage domains; physical pack payload bytes alone do not
replace the old C2+C5 allocated-byte metric.

## Documentation checkpoint

- The original packet review is preserved in [REVIEW](REVIEW.md).
- Strict-split source/contract review and exact documentation publication checks
  are recorded there separately; runtime proofs remain prospective.

Exact first-parent/staged-tree production LOC, resulting committed-tree match,
owned branch publication and #295/#293 checkpoint URLs are recorded by the
integrator after creating the commit. These are publication records, not runtime
implementation exit gates.

The commit URL belongs in the issue checkpoint after commit creation; this packet
never predicts its own future SHA. No product tests or measurement were executed
by drafting/reviewing these documents. Owning runtime checks remain prospective.
