# Area P continuation briefs

Launch prompts for everything after retro finding 5, plus the
remaining-task list. Written 2026-10-30 after private PR #12
(finding 5) merged; p1-p15 tip at writing time: `20b44a0b`.

## What is already landed (private `area-p/p1-p15`)

- P2 (PR #2), P14 (PR #3), P1 (PR #4), P7 (PR #5), P8 (PR #6)
- Retro finding 1 (PR #7), finding 2 (PR #8), P9 v2 (PR #9),
  finding 3 (PR #10), finding 4 (PR #11), finding 5 (PR #12)
- Upstream batch PR Ephemeral-AI-Lab/layerfs #505 (P2/P14/P1/
  P7/P8 + finding 1): CI all green, awaiting maintainer review.

## Launch order

1. `brief-finding-6.md` — fs.read early-stop (local, no
   contract change).
2. `brief-finding-7-design.md` — DESIGN ONLY for the late-line
   `nextOffset` byte cursor (additive contract change). The
   design must be reviewed and approved before any
   implementation brief is written for it.
3. `brief-finding-8.md` — blame single conversion (cleanup).
4. `brief-p10.md`, `brief-p11.md`, `brief-p12.md`,
   `brief-p13.md` — in that order.
5. `brief-p4.md` (D10 Python), `brief-p5.md` (D11 MCP).
6. `brief-p15.md` — preparation ONLY; npm publication needs a
   separate user approval.
7. `brief-connect-integration.md` — connect-side integration;
   needs a separate user approval before any work in the
   layerfs-connect repo.
8. `brief-full-local-test.md` — the final gate, after all P
   tasks land.

`brief-p3-status.md` is NOT a work brief: P3's history half is
done via P1; its push/pull half stays blocked by R7 #42 and must
never be simulated.

## Standing rules for every brief (do not re-derive)

- Pipeline: kimi implements → fresh-session diff review →
  commander verifies in the clone → one private PR per slice →
  merge. Upstream PRs carry COMPLETE code only, and wait for
  #505 to merge first (finding 2, P9, finding 3, finding 4,
  finding 5 each go as their own complete slice/batch).
- Contract first: for any slice with a public surface, write
  the precise public contract beside the P6 cases before
  coding; keep test-first and the per-suite verification
  matrix (JS area-p/calls/cli, Rust neighbor suites, golden
  under umask 022, fmt, workspace clippy excl. node, boundary,
  tool self-tests).
- Base tip: re-verify the p1-p15 tip at launch; never assume
  the tip written in a brief. Line numbers in briefs are from
  the retro report / design doc and must be re-verified
  against the tree at launch.
- Environment facts: this sandbox has no Docker and no
  /dev/fuse — mounted/real-E2E tests are NOT RUN, disclosed
  every time. Durable execution is NOT_RUN by policy.
  Node v24 runs the JS tests (`await using` breaks node 22);
  cargo needs `export PATH="$HOME/.cargo/bin:$PATH"` in a
  fresh shell; 1.85.1 rustfmt/clippy components are installed.
- Never touch: the baseline tree, the connect checkout (until
  the connect brief is approved), `.github/workflows/*` via
  the connector (workflow scope is missing — those files go
  via the signed-in browser), or another slice's files.
- No commits, pushes, or PRs from implementation sessions;
  the commander does all of that.
