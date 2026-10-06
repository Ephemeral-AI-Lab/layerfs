# Native complete-root membership and symlink acquisition

> **Status:** Current general guide.
> S9/P12 checkpoint; bounded complete-root acquisition acceptance remains open.

Project [import/scan](../../crates/layerfs-project/src/import/scan.rs) traverses
all source directory names without consulting Git ignore rules. The ordinary
namespace path includes `.git/index`, ignored files, dependencies, caches and
outputs. Initial acquisition is explicit; later immutable root reads or Workspace
binding use canonical objects and do not rerun the native scan.

The new [source observation](../../crates/layerfs-project/src/import/source.rs)
reads each symbolic-link target once with `read_link`, then checks the source
link's identity, type, length, mode, mtime and ctime against its initial metadata.
A changed link is refused without rescanning/replaying. Targets remain opaque
bytes under the existing canonical4096-byte/no-NUL grammar. Broken, external,
cyclic and non-UTF-8 targets are preserved without traversal, normalization or
UTF-8 conversion. Portable link mode remains the existing canonical0777,
including on macOS where lstat link bits may depend on umask; native mode is still
compared to detect source changes. This does not add a format for host-specific
symlink permission bits. Exact mtime remains recorded. The existing namespace
prerequisite Save emits the target
object and portable metadata before the filesystem Save. Source acquisition
requires a stable host tree; these observations do not create an atomic native
snapshot or resolve arbitrary concurrent source mutation.

The inherited4GiB check is removed from both native file scan and prerequisite
file-role validation. Files still stream through existing C1 constructors with
actual64-bit/platform/format/resource limits, rather than a LayerFS total-file
cap. Public role validation remains. No greater-than4GiB native stream proof is
claimed at this checkpoint; it is explicitly NOT_RUN.

The public complete-root test builds an ordinary tree with ignored bytes, Git
index/object files, a dependency directory, cache/output directories, an empty
directory and five link targets. It publishes through Project Init, deletes the
native source and enumerates/reads only the saved canonical root. Exact file bytes,
opaque targets, complete name count and absence of the external target file are
checked. Memory ports and both real macOS Store profiles are distinct proofs;
none is a cold-speed/RSS or whole-importer bound result.

For N entries, file bytes B, largest directory K and total link bytes T, the
existing scan/construct work includes O(B+T+sum(K log K)) name sorting plus actual
namespace/reference construction. Native target acquisition adds O(T) byte copies
and two metadata observations per link (initial plus post-read), no target walk.
Current prepared entries, jobs, directory frontier, sorted children and namespace
serial/inode/directory collections still grow with input; source counters expose
those capacities. Native hard-link aliases still import as distinct inode entries.
Backed/streamed acquisition and faithful hard-link identity remain required P12
work. P6/P7/P13/P14 Commit validation/parent/release prerequisites remain separately
open; this acquisition change does not silently resolve them or skip validation.

See [S9 audit](../issues/307/S9-EXIT-AUDIT.md) and the append-only
[checkpoint receipts](../issues/307/checks/s7-startup-s9-service/).

Linux qualification also aligns the private global-provider filesystem-error
variant/conversion with its macOS-only allocation owners. This removes unreachable
Linux code without enabling a Linux global Store or changing macOS error behavior.
The original warning-denying failure remains in the checkpoint failure ledger.

[Regular native aliases](41-native-regular-aliases.md) now retain one logical inode
per native device/inode, construct its payload once and distinguish separate files
with equal bytes. Original path-count reservations are consumed without recycling.
Pre/post descriptor and path observations include mode/ctime as well as identity/
length/mtime. Existing resident scan/input collections still require backing; this
identity correction does not establish full-root bounded acceptance.
