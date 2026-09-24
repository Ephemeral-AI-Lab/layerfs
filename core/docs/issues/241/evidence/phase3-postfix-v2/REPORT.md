# #241 position selection with shared daemon session: partial functional PASS

The prospectively frozen [v2 selection](CONTRACT.md) ran once at source
`a23507d1303d82c4a96595c4f1db01a113ee84ea`, tree
`43becb56aa87054fb0610563a676a752d3bcbac4`, with the unchanged
264-position manifest SHA-256
`e12509411e225e6eea497dc08e3cc0730f1c2ab7ce3f104cfc232eb16c7f2b6a`.
The locked release test binary SHA-256 was
`c62482b17e4002faa254ce0084cca89d2937dbd1c71ab72cde56ace97d4f5591`,
and the new Linux daemon/tool image was
`sha256:3cf51669252a499135d5d7dad3028a20fe6f83debe634c67f95c037bd30d9b2c`.
The [plan](raw/plan.json), SHA-256
`a7629927095d64d24941c55febc88e6d0cefba5036fe6dadc099ffd7b95c42ce`,
fixed all four fresh outputs, fixture/master/image hashes and telemetry IDs
before execution. All four [closed masters](raw/masters.json) matched their
existing exact release SDK Init compatibility keys; each size used one
independent writable byte copy, not a cache qualification.

| Size | PASS | FAIL | NOT_RUN | Cleanup | Result |
| --- | ---: | ---: | ---: | --- | --- |
| 1 MiB | 66 | 0 | 0 | PASS | Full functional slice PASS |
| 10 MiB | 66 | 0 | 0 | PASS | Full functional slice PASS |
| 100 MiB | 29 | 1 | 36 | PASS | Source Sandbox creation failed on case 30 |
| Capped 500 MiB | 66 | 0 | 0 | PASS | Full functional slice PASS |

Total: **227 PASS / 1 FAIL / 36 NOT_RUN**. The complete 264-position Phase 3
gate is **FAIL**. The first two sizes and the capped 500 MiB size each had 66
attempted positions, no failures and an empty Sandbox list at the end. The
100 MiB size stopped at `100mib-overwrite-band13`, offset 89,962,450, after
29 passing positions. Its edit, fresh and historical views passed; the
pristine-source Sandbox could not be created, so that view and later positions
were not verified. The retained Sandbox was deleted and list-confirmed absent.
No v2 case was rerun to replace this outcome.

The 100 MiB [Docker State](raw/100mib/100mib-overwrite-band13-source-docker-inspect.stdout)
reports `Status=created`, `Running=false`, `ExitCode=128`, no OOM, and a
failure to bind `127.0.0.1:61753/tcp` because the address was already in use.
Host LFT1 has 119 successful `owner.docker_launch` scopes and one failed
launch; all 120 stop, container-removal and volume-removal scopes succeeded.
The Sandbox owner reserves a free local port, drops that reservation, then
asks Docker to publish that fixed port. The gap permits the observed bind
collision. The receipt does not identify which process occupied 61753 at the
instant Docker tried it. This was a setup failure before daemon readiness,
mounting or source-view reads, not an observed 100 MiB content failure.

The v1 fresh-reopen `daemon.service_connect` failure did not recur in the
three complete sizes at this source. An independent audit of the 1 MiB size matched all 66
unique case IDs to the manifest, found 264 unique per-view Sandbox IDs,
successful unmount/delete/list-absence outcomes, 67 Branches (one pristine)
and 132 old/new Commits. The case PASS is written only after the full stored
byte SHA-256/size, pinned old root, mounted windows and source Branch identity
checks. The same test logic produced the two other complete sizes. These are
functional observations with uncontrolled cache state; no latency or
cold-cache PASS follows. The separate release Edit→Commit and verifier gates
remain open.

The [raw directory](raw/) contains the exact plans, case/run/summary receipts,
command output and failure diagnostics, with [SHA-256 inventory](RAW-SHA256.txt).
It excludes private writable Store/history copies. The plan pins fixture and
master identities; each run TSV records the source Store/history hashes and
independent-copy method. Individual run TSVs do not repeat the source commit,
test-binary SHA or image ID, which are joined through the prospective plan.
Docker inspect retains
container State only, without control configuration.
