# Unmodified official fuser candidate and fresh-download verification

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Owner chose crates.io releases only; the official Git candidate is not adopted.

**Subsequent owner direction:** "use fuser 0.18.0 from crates io and apply patch".
The [authorized registry patch record](FUSER-REGISTRY-PATCH-20261006.md) supersedes
this investigation's no-patch/published-release-only next step. The Git candidate
remains unadopted. All original receipts and verdicts below are historical evidence
and remain unchanged; they are not qualification of the subsequently patched crate.

Owner request2026-10-06: make a permanent fix and allow no patch to fuser;
re-downloading is allowed. This investigation implements no third-party patch,
fork, vendoring, registry edit, raw-wire product proxy, overflow/profile bypass
or smaller timestamp contract. It does not complete S8.

## Fresh registry package

Downloaded https://static.crates.io/crates/fuser/fuser-0.18.0.crate into owned
core/target material. SHA256 matches crates.io/Cargo.lock:
b82b6597d216503555ead6b358f341ef748869bf5c6fbae6a0cb9dd231baecfd.
Every one of85 archived files matches both existing host and Linux Cargo source
installations. No file is missing/changed. [Verification](checks/fuser-official-candidate/redownload-verification.json)
proves the installation is pristine; re-download does not remove the published
parser defect. The [fresh official sparse index](checks/fuser-official-candidate/registry-index.jsonl)
still ends at0.18.0, with no corrected release.

## Official upstream source and exact proposal

The maintainer's existing [commit](https://github.com/cberner/fuser/commit/e48279fab0ddbe4a6e22aefad3cbfdf796d0b0b7)
fixes negative fractional conversion and negation overflow. Cargo fetched that
exact revision from the official repository. [Checkout integrity](checks/fuser-official-candidate/upstream-integrity.json)
records the full revision and clean tracked files. No upstream file was modified.
It requires Rust1.85 and preserves the inspected public callbacks.

The concrete proposed dependency declaration is:

```toml
fuser = { git = "https://github.com/cberner/fuser.git", rev = "e48279fab0ddbe4a6e22aefad3cbfdf796d0b0b7", default-features = false }
```

An isolated external harness has [this manifest](checks/fuser-official-candidate/candidate-manifest.txt)
and [resolved lock](checks/fuser-official-candidate/candidate-lock.txt). Overlapping
transitive dependency versions/sources match the prior platform proof; only
fuser's source changes from registry to the pinned official Git revision. It is
an unmodified official dependency, not a locally applied patch. Production
manifests/lockfiles are unchanged. The owner subsequently answered **Keep crates.io
releases only**. The official Git proposal is rejected for product adoption; the
original corrected-published-package requirement remains binding.

## Actual qualification and retained failures

Linux ARM64 kernel6.12.76-linuxkit, Rust1.85.1, image
sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4.
Locked debug build retains overflow checks and repository ARM64 flags. Cases have
explicit8s complete-command ceilings; none times out. No speed/RSS qualification.
First resolution/exploratory build is retained as such; the qualification build
is [locked](checks/fuser-official-candidate/build-locked.log).

| Owning boundary / input | Actual result | Cleanup / scope |
| --- | --- | --- |
| Mounted negative `(-2,800000000)` | PASS: exact callback/returned time | Native unmount/join succeeds |
| Original mounted minimum `(i64::MIN,200000000)` | FAIL: callback/returned nanos are0; no parser panic | Native unmount/join succeeds; failure is retained |
| Additional mounted whole minimum `(i64::MIN,0)` | PASS: exact callback/returned time, no panic | Native unmount/join succeeds; not a relabeling of fractional failure |
| Public Session::from_fd parser/reply: negative, min0, min200M, min+1/200M, max/999999999 | Five exact checks PASS | Explicit protocol DESTROY, normal session join; external fixture, not a kernel mount or product proxy |

[Mounted receipts](checks/fuser-official-candidate/native-receipts.json),
[whole-minimum receipt](checks/fuser-official-candidate/native-minimum-whole-receipt.json)
and [parser receipt](checks/fuser-official-candidate/request-proof-repaired-receipt.json)
retain commands, walls and binary identities. The initial parser fixture returned
InvalidData on shutting down a socket without the FUSE DESTROY operation; all
five conversions already passed. Its initial failed output is retained. The
first-party fixture now sends explicit DESTROY and joins; no fuser code changed.

The additional fractional minimum finding is a Linux VFS boundary. The exact
[Linux6.12.76 timestamp_truncate](https://github.com/gregkh/linux/blob/v6.12.76/fs/inode.c#L2455)
clamps seconds and zeros nanos at the filesystem's minimum/maximum;
[notify_change](https://github.com/gregkh/linux/blob/v6.12.76/fs/attr.c#L358)
applies it to explicit mtime before the filesystem setattr callback. The observed
native minimum is consistent with that code, while the direct public Session
proof preserves200M nanos when actually presented with them. This separates
kernel canonicalization from the library parser. The original native fractional
case remains FAIL; no timestamp contract is silently reduced or kernel patched.

## Enforce no patch

[Focused dependency integrity check](../../../tools/check_fuser_integrity.py)
rejects fuser Cargo patch/replace/path/custom-registry sources, unapproved Git
revisions, floating branches/tags and directory/path Cargo overrides. Optional
package verification checks the locked archive hash and every source file,
including extra/redirected source detection. It modifies nothing and runs no
aggregate pre-push/CI wrapper. Its [ten external unit tests](checks/fuser-official-candidate/guard-tests-initial.log)
and [101-manifest/lock/config plus package check](checks/fuser-official-candidate/guard-repository-and-package.json)
pass. The approved-Git list is empty, consistent with the owner's registry-only decision.
This check establishes provenance/integrity, not timestamp capability.

Next: keep the registry dependency and no-patch checks. A corrected published
release is still an external prerequisite; re-downloading the identical pristine
0.18.0 package cannot fix it. Independent S7/S9 work is ready and must not be
stopped solely by this gate. Existing S0/S8/S12 checks remain
open; historical forced-abort137 and all failures are preserved.
