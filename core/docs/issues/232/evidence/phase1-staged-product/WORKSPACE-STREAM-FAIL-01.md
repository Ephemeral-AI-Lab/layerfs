# Workspace stream proof 01: retained Capacity result

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The first live external `projected_range_route.py --case stream` attempt at
source `a1f745221121b27ed0d828bd50501282c154ddeb` used locked release
Linux test code, a fresh independent byte copy of the closed
[64 MiB prepared fixture](fixture-64m.json), and the published Docker image
`sha256:9babee938c8c9c7a6678331c98f76a88a40594569b2f6a726c08d3677d836026`.
The complete functional command took 2.699 s and the test exited 101.
The raw [result, stdout and stderr](projected-stream-01/) retain the exact
binary/test/source hashes, Docker selection, error and missing checks.
The container and volume retained on failure were inspected, then removed;
[cleanup.json](projected-stream-01/cleanup.json) records both absent.

The 64 KiB Bytes splice, deletion, mixed Bytes/Zero splice and byte-restoring
splice reached the later assertion. The assertion expected an exact 8 MiB
Zero extension to succeed, but `edit_file_range_stream` returned `Capacity`.
The test's byte-restoring splice used a new **Local** payload for four bytes.
The canonical base bytes were restored, but the live piece metadata still
contained those non-Base bytes. `overlay::pieces::splice` counts all non-Base
length against the existing 8 MiB replay limit. Adding exactly 8 MiB of Zero
therefore correctly exceeded the cumulative replay envelope. The prior
over-8 MiB refusal did not mutate the Workspace, and the test was not a
performance sample. This failure does not establish product acceptance at
exactly 8 MiB, Commit, cleanup or a passing oracle.

The correction changes the external test only: `stream` retains its 64 KiB
and mixed-segment checks, while `zero_capacity` uses a fresh, pristine
Chunked fixture copy with no earlier Local pieces. It checks >8 MiB refusal
before mutation and exact 8 MiB Zero acceptance separately. The failed
attempt and its classification stay unchanged; each corrected case receives
one fresh output at the new test identity.
