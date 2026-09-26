# #241 post-fix baseline control v1 result

> **Status:** Dated planning checkpoint; not release evidence or a product
> contract. This is one functional integration smoke, not a performance row.

The first frozen [attempt](attempt-01/receipt.tsv) at source
`f9f4943ca5999068ba806b909f65f774572337b3` (product fix commit
`0dab33f29`) passed. Public SDK mount succeeded; the exact
`printf baseline > .position-baseline` Exec returned exit 0 with empty,
untruncated output in **24 ms** of host context wall. SDK unmount, Sandbox
deletion and final absence passed. The complete test command took 5.94 s,
including container lifecycle. No range EDIT or Commit ran. Cache state was
uncontrolled; neither wall is an edit-latency gate.

The [identity](attempt-01/IDENTITY.json) pins the clean source tree, host
test binary SHA-256
`81fe1ce94089276fb1ae5f99d56a2fe40a39ab997d9297eeaacf04bcc9c0b7db`,
the qualified 10 MiB master and independent Store/history byte-copy hashes,
and the new immutable Linux [image](attempt-01/image.json)
`sha256:3ca406d5b27af23317d798104ebacd924796ab6bb875d63ec2128f6af72ee77c`.
The [raw console](attempt-01/console.txt.gz) is losslessly compressed. The
[SHA manifest](attempt-01/SHA256SUMS) validates all retained files.

The closed-stdin [acceptor regression test](../../../../../crates/layerfs-server/tests/acceptor_idle.rs)
passed at the same product source: the idle composed Server used less than a
quarter of one CPU over a one-second interval. Full locked Core tests,
examples, warning-denying Clippy, fmt, the boundary scanner and its guard
tests, plus a locked Linux daemon release build, passed. The daemon now emits
existing LFT1 child spans for fresh Service connect, checked Hello and
request/terminal handling; the smoke did not reproduce a transport failure
that could exercise their failure attribution.

**Interpretation:** the independently evidenced closed-stdin host spin is
fixed and this one public SDK control sequence works on the new source. The
historical baseline Exec `Unknown` and the separate diagnostic mount
`Unknown` have no established common cause. Neither failed receipt changes:
the [integrated selection](../position-sweep-integrated/REPORT.md) remains
243 PASS / 1 FAIL / 20 NOT_RUN and the [10 MiB diagnostic](../position-exec-liveness-v2/REPORT.md)
remains 29 PASS / 1 FAIL / 36 NOT_RUN. Phase 3 and the four registered
release Edit→Commit cases remain open.
