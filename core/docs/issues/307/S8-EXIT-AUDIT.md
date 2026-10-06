# S8 native filesystem and daemon audit

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Milestone state: INCOMPLETE product implementation/qualification; fuser correction accepted through Docker.

## Latest Docker verification acceptance

The owner subsequently stated **"docker verification is enough"**. The existing
native Docker results are sufficient verification of the fuser correction;
additional QEMU/custom-kernel qualification is not a prerequisite for that fix.
The fractional signed-minimum result remains a retained Linux platform limitation,
with its actual FAIL verdict unchanged. This supersedes the earlier kernel-case
blocking disposition for the dependency correction, not the raw evidence.
S8 remains unchecked because native FUSE, daemon/control/Bash lifecycle and complete
product acceptance remain unfinished. Continue independent S7/S8/S9 work through
Docker. [Boundary and scope](LINUX-TIMESTAMP-DOCKER-20261006.md).

## Subsequent authorized dependency correction

The owner subsequently requested "use fuser 0.18.0 from crates io and apply patch".
The [registry patch record](FUSER-REGISTRY-PATCH-20261006.md) owns the new source,
qualification and retained Linux boundary limitation. The earlier prerequisite of
waiting for a corrected published release is superseded for this one patch. S8
remains unchecked: correcting the dependency does not implement native FUSE,
daemon/control/ordinary Bash lifecycle or close the complete timestamp contract.
The earlier source/build/decision evidence below remains a dated checkpoint.

## Earlier checkpoint under the no-patch ruling

S8 remains unchecked. No replacement native FUSE, mount/control registry, ordinary
Bash runner or complete detach/join lifecycle was introduced in this batch.
The active daemon owner remains independently built and verified. Kernel request,
open/lookup/reply disposal, mmap/coherence/writeback-off, blocked dispatch, repeated
same-mount calls and complete native teardown still need product implementation
and mounted acceptance; engine/component tests do not satisfy them.

The required external gate remains the [retained fuser timestamp blocker](FUSE-TIME-BLOCKER-20261005.md):
published0.18.0 converts signed fractional input incorrectly and panics on signed
minimum before the LayerFS callback. Original negative FAIL and minimum forced-abort137
are preserved; no unsafe parser workaround, raw-wire substitution, third-party
patch, git package replacement or smaller timestamp contract is adopted.

On2026-10-06 the official crates.io sparse registry index was read once from
https://index.crates.io/fu/se/fuser. [Retained index](checks/s7-costs/fuser-registry-index-20261006.jsonl)
ends at non-yanked0.18.0, checksum
b82b6597d216503555ead6b358f341ef748869bf5c6fbae6a0cb9dd231baecfd.
The initial distinct crates.io JSON API read failed HTTP403; this failure is
reported, not interpreted as no release. Upstream's [current changelog](https://github.com/cberner/fuser/blob/master/CHANGELOG.md)
keeps the correction under Unreleased. Existing package/source and native failure
receipts remain the qualifying evidence; the unchanged known-failing probe was
not resampled.

Smallest external requirement: an allowed corrected published package carrying
signed fractional/minimum timestamp support, followed by locked actual mounted
native qualification. No owner approval to reduce the contract is requested.
Independent S7 engine observations and S9 host handlers progressed while this
capability remained unavailable. S7 has additional internal evidence work and S9
has additional transport/import/fence work; they are not described as finished
or blocked solely by fuser.

The organization recommendation selects FUSE mount/requests/dispatch/ownership/
coherence and daemon service/registry/lifecycle/execution/control/upstream homes.
Only real service observations were added; no empty native scaffolds were created.
S0 native capability closure and S12 acceptance also retain this gate. S10–S13,
root-reference retirement, pushes, releases and deployments remain outside this batch.
