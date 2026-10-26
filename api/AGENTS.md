# Agent rules for API

> **Status:** Current general guide.

Read [repository instructions](../AGENTS.md) first. This layer contains
`layerfs-api`, the Node binding (`layerfs-node` under `binding/`) and the
JavaScript packages. The [root Cargo manifest](../Cargo.toml) and
[SDK guide](../docs/general/sdk.md) establish the built Rust members and public
surface. API may depend on engine and core; dependencies and test source includes
from either lower layer back into API are forbidden.

Project Store Init and install belong to `layerfs-project::store`; the Rust API
retains its public re-exports and the binding makes one direct Project Init call.
SDK-facing daemon integration tests live in `layerfs-api/tests`, with cheap
includes of lower-layer daemon fixtures. Build the ordinary daemon binary before
running tests that launch it. `LAYERFS_TEST_DAEMON` may select an explicit binary;
otherwise the tests locate the sibling Cargo output. The native binding's
[external proof guide](binding/tests/README.md) describes its Linux addon tests.

All API product code follows the
[production size and responsibility rules](../engine/AGENTS.md#production-file-size-and-responsibility)
and [checks and completion rules](../engine/AGENTS.md#checks-and-completion),
including bounded test commands, build-before-run, locked dependencies, ARM64
flags, per-commit LOC and the Disposable-only profile. Preserve the existing
unsafe-code boundaries; do not add test hooks or inline tests to production src.
The owner leaves Node binding/JavaScript CI targets excluded; changed scope must
still be verified explicitly. The named SDK mount tests remain in nightly's
Linux mount lane. CI reports and does not gate a push.
