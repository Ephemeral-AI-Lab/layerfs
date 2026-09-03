# LayerFS 0.1.2 Developer Preview

> **Status:** Released for LayerFS 0.1.2 Developer Preview.

The first v0.1.2 publication was withdrawn because its edit benchmarks used
the wrong POSIX/FUSE mutation surface. Its old evidence is archival only.
The replacement release uses the corrected SDK-only families and a fresh
namespace/Store release refresh, with explicit source identities.

Issue [#20](https://github.com/Ephemeral-AI-Lab/layerfs/issues/20) now has three
complete SDK-only 1/10/100/500 MiB families: 560 performance rows and 112 passing
source-arm verification proofs. The [evidence selector](sdk-edit-evidence.json)
records final repository-gate completion and the documentation-only source bridge.
Parent #12 separately owns the user-authorized release finalization.

- [Benchmark results and acceptance scope](benchmark-results.md)
- [Refreshed namespace and Store tables](supporting-benchmarks.md)
- [Verification and final gate](verification.md)
- [Release contract](release-contract.md)
- [Limitations](limitations.md)
- [Artifact status](artifacts.md)
- [Release announcement](github-release.md)
