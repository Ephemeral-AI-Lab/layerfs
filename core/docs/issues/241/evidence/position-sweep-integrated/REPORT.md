# #241 integrated functional position campaign

> **Decision: Phase 3 remains incomplete.** This is one functional campaign
> at the frozen 264-offset manifest, with no Edit→Commit performance sampling
> or cold-cache admission. All four size selections and every PASS, FAIL and
> NOT_RUN row are retained. Earlier isolated-worktree diagnostics remain
> separate in [their append-only report](../position-sweep/README.md).

The [identity record](IDENTITY.json) pins source cdd3ea863, manifest SHA-256
e12509411e225e6eea497dc08e3cc0730f1c2ab7ce3f104cfc232eb16c7f2b6a,
the host SDK test binary/source hashes, validated recipe file hashes,
atomically published [integrated masters](../qualified-masters-integrated/REPORT.md),
and immutable Linux [image receipt](image.json). Each size used one independent
writable Store/history byte copy and fresh sibling Branch/Workspace state per
offset. Each mounted view used its own SDK Sandbox, with unmount, SDK delete
and list-absence recorded in each case TSV. The bounded mounted verifier
checked head, edit seam, tail, size and EOF; a separate C1/Store reader
checked each new full-file digest, content root and extent count, with
retained old Commit and pristine source Branch checks. A clone never
constituted a cold-cache claim.

| Pristine file | PASS | FAIL | NOT_RUN | Final Sandbox list | Functional command wall |
| --- | ---: | ---: | ---: | --- | ---: |
| [1 MiB](1mib/summary.tsv) | 66 | 0 | 0 | empty | 199.85 s |
| [10 MiB](10mib/summary.tsv) | 45 | 1 | 20 | empty | 146.66 s |
| [100 MiB](100mib/summary.tsv) | 66 | 0 | 0 | empty | 255.58 s |
| [capped 500 MiB](500mib-capped/summary.tsv) | 66 | 0 | 0 | empty | 476.92 s |
| **Total** | **243** | **1** | **20** | | |

These walls cover functional setup, 66 or fewer SDK Branches, multiple mounted
views per case, full-content verification and cleanup. They are not per-edit
latencies and cannot be compared with the #241 G2 context targets. The
[SHA256SUMS](SHA256SUMS) manifest seals all 288 copied raw files plus the
read-only diagnostic, including
each size's console, master-copy output, run/summary and all 66 case rows.
The private database copies remain under ignored core/target paths for
diagnosis; their content is not embedded in Git.

## Retained 10 MiB failure

The sole failed row is
[10mib-delete-band13 at byte 8,584,572](10mib/10mib-delete-band13.tsv).
After 45 completed cases, the public SDK Exec used to create this case's
baseline file returned a typed Unknown outcome before any range EDIT call or
baseline Commit. SDK unmount then returned a definite Io failure; SDK
Sandbox deletion returned Ok and the final list was empty. The harness
stopped and emitted 20 explicit NOT_RUN rows, preserving the first 45 PASS
rows and the FAIL. It did not rerun or replace the case.

The [read-only history diagnostic](10mib/HEAD-DIAGNOSTIC.txt) found 47
Branches and 90 Commits in the private copy. The 90 Commits are consistent
with two per earlier PASS; the failed Branch and the pristine source Branch
both have no head Commit. This supports the boundary classification:
the failure was before an admitted range edit/Commit. It does not establish
whether the baseline shell write took effect in the discarded Workspace, nor
why Exec lost its response. The daemon stderr captured before deletion
contains only its readiness line; no OOM event was found in the inspected
Docker event window. The outcome remains **UNKNOWN**, with no automatic retry.

The good outcome is broad functional evidence for the Linux projected splice:
all frozen offsets on 1, 100 and capped 500 MiB passed exact byte, mounted
visibility, old/new Commit and cleanup checks. The bad outcome is a real
intermittent SDK Exec/liveness gap in the 10 MiB campaign, leaving 21
positions unqualified. The full 264-case Phase 3 gate is **not PASS**.
The four registered release Edit→Commit cases and their separate verifiers
have not been sampled. The earlier carrier timings remain diagnostic and
cache-ineligible.

The prospective [baseline Exec liveness diagnostic](../../EXEC_LIVENESS_DIAGNOSTIC.md)
specifies the request-scoped event counts needed to locate this Unknown
before changing product progress handling or making another qualification
claim.
