# Retained host binary source provenance

This is a retrospective read-only source assessment, not another build, test or
measurement. The original build receipts and failures remain unchanged.

The retained host binary SHA-256 is
`3bc327d5ac14be15b2c585b055f0f148fbad9ecc111592e219a5fb6c560e9adc`.
The dependency file retained in this directory has SHA-256
`567539cbe90cd6ffffc9d5b9a2ebaad8a17464035936520d0203ae0e57a289a5`.
Its 177 repository dependencies resolve through shared target-directory symlinks
to checkout `7124707121634f5677831facdaed941a2a2b8335`. All 177 actual dependency
files byte-match that Git revision. This exposes the cache provenance; the
subsequent short Cargo invocations must not be described as recompilation.

Of those 177 inputs, 176 byte-match
`4c207c70f3282c316d5ab18d832504085835eda3`. The sole difference is
`benchmark/fs-bench-pro/reliability_workloads.rs`: two `setxattr` FFI pointer
parameters changed from `*const i8` to `*const std::ffi::c_char`. Both changed
lines are inside the `#[cfg(target_os = "linux")]` block. That entire block is
excluded from the actual `aarch64-apple-darwin` host compilation. This conclusion
does not require assuming the target meaning of `c_char` or exercising xattrs.
All other host Rust/SQL dependencies, product sources, Cargo manifests, build
scripts and Cargo configuration are unchanged between 712 and4c. The retained
checkouts match their Git contents. Cargo.lock SHA-256 at 712, 4c and f518 is
`d050d2a5b1c429925cfffb2889dca4b116e33f1fc9ef2cb2fa1e832ae957777f`.

Consequently the 4c host binary has the same target-effective compiled source
inputs as 712. Its reuse for the 84 retained ordinary performance observations is
supported by this explicit equivalence proof, while keeping their actual host
binary/image/source receipts. This is not blanket evidence acceptance: the nine
pre-Commit failed Exec rows still lack required physical spool peak observations,
and every actual product failure remains failed under the completion amendment.
The 4c Linux runtime image was separately built after the Linux-only ABI repair;
the failed 712 Linux image build remains failed.

For `f51859c32356a40a5fbe2e52ee30690f995f8746`, only 173 of 177 cached dependency
files match. Changed files are `dedup_workloads.rs`, `reliability_workloads.rs`,
`src/workspace_bench.rs` and `src/workspace_verify.rs` under fs-bench-pro. These
include target-relevant behavior and observations. The cached 3bc host binary
cannot qualify as that implementation. The source-comparison receipt selects
`fs-benchmark-pro` for package-local cache invalidation before the required
build. No old measurements may be relabeled as f518 measurements.

The first f518 image attempt separately failed resolving the Dockerfile frontend
with an external TLS handshake timeout. That does not validate or invalidate the
cached host behavior; both findings and logs remain retained. The new custody
preflight must compare resolved dependency bytes against requested committed
source and invalidate changed owning packages before accepting a host build.
