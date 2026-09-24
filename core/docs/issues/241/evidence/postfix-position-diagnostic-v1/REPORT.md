# #241 post-fix 10 MiB position diagnostic v1 result

> **Status:** Dated planning checkpoint; not release evidence or a product
> contract. This is one diagnostic selection, not the 264-position Phase 3
> qualification or a performance sample.

The single frozen attempt at source
`2e3e3e12e0cc5148a7e1583254b32031afd87666` (product fix commit
`0dab33f29`) completed all 66 existing 10 MiB positions. The
[summary](attempt-01/summary.tsv) records **66 PASS / 0 FAIL / 0 NOT_RUN**,
and the final SDK Sandbox list is empty. All 66 baseline Exec calls returned
success, with host context walls from **20.895500 to 25.334417 ms**. The
existing mounted-window, old/new Commit, pristine Branch and full C1 byte
oracle checks passed in each case. The complete test command took 223.69 s,
including setup, mounted checks, verification and cleanup; it is not per-edit
latency. Cache state was uncontrolled and `admission_eligible=false`.

The [identity](attempt-01/IDENTITY.json) pins the clean source tree, test
binary SHA-256
`81fe1ce94089276fb1ae5f99d56a2fe40a39ab997d9297eeaacf04bcc9c0b7db`,
frozen manifest and fixture SHA-256, qualified closed-master receipt and
independent writable byte-copy method. The immutable Linux
[image](attempt-01/image.json) is
`sha256:3ca406d5b27af23317d798104ebacd924796ab6bb875d63ec2128f6af72ee77c`.
The qualified master was produced before the host acceptor fix; this run is
diagnostic and does not promote that producer identity to a new release
sample. The [raw console](attempt-01/console.txt.gz) is losslessly compressed;
the [SHA manifest](attempt-01/SHA256SUMS) validates all 73 copied raw files.
The mutable Store/history copy remains in the ignored target directory.

As additional diagnostic context, the earlier failed 10 MiB selection's
last host LFT1 resource record showed 94.475 s cumulative process system CPU
at 101.581 s of that process's wall; this run's last record showed 9.293 s
at 223.212 s. Those are separate whole-process scopes with different
workloads and telemetry windows, **not** paired phase-local CPU or a speed
comparison. They are consistent with the independently tested closed-stdin
acceptor spin fix. This run did not produce a transport failure for the new
daemon connect/Hello/call child spans to classify.

**Decision:** the new source completed this one diagnostic selection, and
the verified host CPU spin is fixed. Neither the original baseline Exec
`Unknown` nor the earlier diagnostic mount `Unknown` has a proven causal
attribution to that spin. The [historical integrated selection](../position-sweep-integrated/REPORT.md)
remains 243 PASS / 1 FAIL / 20 NOT_RUN; the [earlier diagnostic](../position-exec-liveness-v2/REPORT.md)
remains 29 PASS / 1 FAIL / 36 NOT_RUN. A new full 264-position qualification
at a single source/image/qualified-master identity and the four registered
release Edit→Commit cases have not run. Phase 3 and release performance
claims remain open.
