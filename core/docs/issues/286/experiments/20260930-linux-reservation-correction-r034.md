# #286 r034: Linux physical-reservation correction

> **Status: product correctness fix and labelled platform diagnostic, no
> benchmark sample.** A Linux host could have enlarged the C2 SQLite file
> before the old postcondition rejected it. The source now uses a keep-size
> syscall, and a Linux/arm64 test exercised the public Store save. Family-2
> r031/r032 remain valid at their recorded macOS source seal but need fresh
> current-source receipts before a checkpoint.

The r033 Init release build revealed the new `StorageError::Io` service mapping
omission; product commit `d4ca6122f` maps it to wire `Code::Io`. The user's
host-portability question then prompted a review of the Linux reservation
branch introduced at `aed28dda1`. [POSIX `posix_fallocate`](https://www.man7.org/linux/man-pages/man3/posix_fallocate.3.html)
**extends logical file size** if its range goes beyond EOF. The old code's
later length check would detect this only *after* the SQLite file had changed.
There is no evidence that branch was executed in an official sample or
deployed, but it was unsafe to claim host-portable behavior. The current
source uses the existing safe `nix::fcntl::fallocate` wrapper with
`FALLOC_FL_KEEP_SIZE`; the [Linux syscall contract](https://man7.org/linux/man-pages/man2/fallocate.2.html)
states that this preserves the file size even when the reserved range extends
beyond EOF. Darwin retains its separately tested `F_PREALLOCATE` branch.
Unsupported hosts still fail explicitly; this is a Mac/Linux capability,
not an all-OS qualification.

Verification from the repository root:

- `cargo +1.85.1 zigbuild --target aarch64-unknown-linux-musl --manifest-path core/Cargo.toml --locked -p layerfs-storage --test physical_reservation` **could not link**: the cross sysroot lacks dynamic `libsqlite3.so`. No test executed from that binary; no third-party package was modified.
- The same locked build with `--features rusqlite/bundled` **built** a static arm64/musl test executable SHA-256 `1f6dbda92a6a612e3771a1216fd476a7f40500af085be29ac70ac28a95638a13`. This uses the published dependency feature only, so it is a platform semantic diagnostic rather than the default dependency-profile build.
- `docker run --rm --platform linux/arm64 --network none -v <that exact executable>:/test:ro --entrypoint /test alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8 --nocapture` **PASS 1/1**. The public Store save reserved physical headroom, SQLite `st_size` still equaled `page_count × page_size`, and `quick_check` returned `ok`.
- The native macOS `physical_reservation` test **PASS 1/1**; warning-denying `layerfs-storage` Clippy and the product-boundary scan passed. No full server/Linux deployment or family3 measurement is inferred.

The method still requires a real host Server integration check before an
OS-portability claim. The next benchmark actions are to commit this source,
then collect one fresh history selected stride10/3 and explicit stride1
cohort at the new source seal, followed by one Init earlier-family check.
The earlier r031/r032 macOS PASS, r033 BUILD FAIL and all historical failures
remain append-only. Numeric time remains INELIGIBLE. Families3–7 remain
NOT_RUN, #285 draft and #286 open.
