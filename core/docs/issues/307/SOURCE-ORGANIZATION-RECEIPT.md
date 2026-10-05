# Active core source organization checkpoint

> **Status:** Dated implementation checkpoint after `32bd3bec0`; not S6
> acceptance, release evidence or a performance result.

The owner-selected [source organization](SOURCE-ORGANIZATION.md) is applied to
the 11 active packages. Eighty production files move to responsibility folders;
the Workspace port file splits into its existing overlay and owning-length
traits/implementations. Every new entry module declares/reexports existing
implementation. Bridge retains its already matching native layout. Test/example
target paths, active membership, dependencies, schema/query semantics, canonical
codecs/profile and unsupported-platform behavior remain unchanged. Excluded
packages, root reference source and unrelated owner files are preserved.

Public root/module paths reexport the original definitions. No forwarding
function, wrapper type, factory or dependency is added. Private aliases remain
only where imports need them. Relocation changes compilation identities without
duplicating types/traits or changing fields, methods or representation. Existing
public callers are compiled and exercised through the production libraries.

Telemetry's portable operation/observation members remain available without
`native`; only monitor/session/platform/output retain native gating, so runtime
exists in both modes. Content `construction/bytes.rs` retains all former
content.rs helpers and ordinary stream APIs. The overlay SQL include is rebased
to `../../sql/schema.sql`, referencing identical bytes. These necessary details
preserve behavior while applying the selected destination shape.

The [relocation map](checks/source-organization/relocations.json) records exact
old/new paths and original hashes. [Leaf equivalence](checks/source-organization/equivalence.json)
compares all 80 moved bodies modulo formatting and module/include rebasing,
with no mismatch. Schema/manifests/lockfile are unchanged. Current maintained
architecture/handbook/API links point to moved files. Dated handoffs/raw receipts
keep their original paths/pins: use `git show <pin>:<original path>` for that source.

Build first with all-target `--no-run`, then each package under an explicit
120-second process-group ceiling. All 11 active core packages: 614 tests PASS on
macOS. Native telemetry additionally: 54 PASS on macOS. Linux ARM64 content/
overlay/Workspace/daemon/SDK plus native telemetry: 406 PASS; SDK's six actual
global-provider cases execute on macOS and compile on Linux. Locked Rust1.85.1,
root ARM64 flags, owned target/cache and existing Linux image
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`.
All-target Clippy -D warnings, native telemetry Clippy, fmt, boundary549 and
tool26 pass. Initial unused-alias warnings are retained; aliases were removed
without suppression. Raw logs keep their original stdout/trailing whitespace.
No test hits its ceiling; no latency/cache-cold/RSS campaign is run.

The organization commit records exact parent/staged production LOC and verifies
the committed tree. Declarations/imports count. Relocation is not algorithmic
simplification or legacy retirement. After that commit, update the guide's
alignment boxes from this receipt and continue S6's orphan/custody/whiteout/
physical-headroom work. S6 stays unchecked until all required exits pass.

Production LOC: 148227 -> 148327 (delta +100). Core 82810 -> 82910 (+100);
reference 65417 unchanged. The existing `tools/production_loc.py` SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`
counts exact first-parent/final-staged archives of `core/crates` and `crates`,
including excluded implementation and shipped SQL, excluding tests/inline tests,
docs/tools/examples/harnesses/manifests/builds. Receipt:
`core/target/cluster2-307/loc/source-organization-staged.json`. The 100 added
production lines are grouping declarations/imports; no algorithmic shrink,
legacy retirement or uncounted application implementation is claimed.
