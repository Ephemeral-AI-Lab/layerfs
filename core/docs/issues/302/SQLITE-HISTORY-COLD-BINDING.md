# Retained-history cold boundary prerequisite

> Status: prospective cache observer integration, no qualification sample.

Based onada5e3811. The source inventory is exactly checkpoint-manifest.json,
inputs/ and oracles/ under the original pinned corpus root. The native helper
can attest these mixed roots in one two-pass invocation without payload reads;
unrelated source.git/snapshot/report files are excluded. Corpus identity checks,
selected oracle validation, blob acquisition/hash checks and construction remain
inside the existing producer lifecycle. This source-cold attestation must precede
the measured child and remains in the performance-command budget.

History cold boundaries now run inside the complete producer clock before states
2..N and before final custody read-back. They invalidate and attest whole current
C2/C5 database and extant WAL file content pages, requiring resident_after=0;
there is no selected-range warming or expected-data priming. Candidate combined
Store and reference original separate Store/history are checked with the same
sealed helper. SHM coordination pages and filesystem metadata residency are
outside the content-page claim. SQLite's original bounded internal buffers and
profile/cache geometries remain unchanged and disclosed. Product read/save APIs
and canonical production remain unchanged. State readers are operation-owned;
no prior sample or prepared Store is reused. The observer neither reopens nor
converts a live database and adds no SQLite statements or durability downgrade.

Every boundary emits state index, real helper counters and enclosing wall time;
total checks/wall are separately recorded. Boundary time stays in the compared
whole operation and cannot be subtracted or called product-only time. These are
nested observer spans. Complete mode requires a helper; exploratory probes may
omit it and remain diagnostic. The future runner must validate helper source/
binary seals, exactlyN checks, all zero-residency results and matching file scope.
A failed attestation is INELIGIBLE; it cannot be retried to select a passing run.
The parent watchdog kills its process group, including boundary descendants,
on the original command deadline; no orphan observer is allowed to survive.

The helper's existing Init single-directory contract remains unchanged. New
--files mode accepts only regular distinct-inode files (including empty files),
checks stable inventory across both passes and refuses duplicate owners. New
--paths mode inventories regular files under explicit mixed source roots with
the same first/final custody check. Required writable invalidation capability
remains fail-closed; there is no platform/failure fallback.

Six focused native checks PASS, including live MEMORY and WAL owners with1MiB
synthetic body, preserved content, duplicate refusal and mixed roots excluding
unrelated data. These are explicitly disposable capability fixtures, never
benchmark input pre-touching or speed arms. Five registry/watchdog checks PASS,
including descendant termination. Three reference-adapter/proof-input checks
PASS. Candidate history example Clippy PASS; Core fmt applied. Both updated
history vehicles build release/locked: reference2.906589333s and candidate
2.082791083s, each within30s; original reference clean before/after. Build outputs
in checks/history-cold-vehicle-build1. No product child/performance arm ran.
Production source unchanged; these files are benchmark/examples/tooling only.

Still required: integrate original corpus identity/preparation, source attestation
and state-boundary evidence with the actual sole runner, frozen compiled artifact
seals, effective selected-profile readbacks, observer attribution and the full
independent namespace/custody/closed-owner proof within9.5s. All seven direct
supported Disposable rows remain NOT_RUN; Durable follows their qualification.
This prerequisite neither proves full-history correctness nor changes budgets,
canonical counts, strict allocation ceilings, transaction/queue/worker bounds.
