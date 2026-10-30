# Remaining tasks after finding 5 (2026-10-30, p1-p15 tip 20b44a0b)

## Retro follow-ups (report: `retro-p-perf-design.md`)

- [ ] Finding 6 — fs.read early-stop predicate (P8, medium).
  Local scanner change, no contract change.
  Brief: `brief-finding-6.md`.
- [ ] Finding 7 — late line windows rescan from byte zero
  (P8, medium). Additive `nextOffset` byte-cursor contract
  change. DESIGN FIRST, then implement.
  Brief: `brief-finding-7-design.md`.
- [ ] Finding 8 — blame triple conversion (P8, low). Small
  cleanup, no contract change. Brief: `brief-finding-8.md`.

## P slices (parent: layerfs-connect#14, "each of Node, Python,
## CLI and MCP can mount, run a command, commit, read history,
## and push or pull")

- [ ] P10 (connect#61) — single-call result API.
  Brief: `brief-p10.md`.
- [ ] P11 (connect#62) — storage statistics (extend
  get_storage). Brief: `brief-p11.md`.
- [ ] P12 (connect#63) — storage summary (cheap).
  Brief: `brief-p12.md`.
- [ ] P13 (connect#64) — folder-scoped client (options /
  wrapper). Brief: `brief-p13.md`.
- [ ] P3 (connect#54) — history DONE via P1. Push/pull
  BLOCKED by R7 #42: never simulate, never fake.
  Note: `brief-p3-status.md`.
- [ ] P4 (connect#55) — Python SDK, per D10: native PyO3
  binding over the public Rust API on Linux, framed worker on
  macOS/Windows. Brief: `brief-p4.md`.
- [ ] P5 (connect#56) — MCP, per D11: Node SDK + stdio first,
  no HTTP initially. Brief: `brief-p5.md`.
- [ ] P15 (connect#66) — prepare @layerfs/* for npm ONLY.
  Publication needs a separate user approval.
  Brief: `brief-p15.md`.

## Integration and gates

- [ ] layerfs-connect side integration (P9 event stream into
  host SSE, renderer de-polling, run-call bridge). Separate
  user approval required; work happens in the connect repo,
  not here. Brief: `brief-connect-integration.md`.
- [ ] Full local test + environment setup, after ALL P tasks
  land. Brief: `brief-full-local-test.md`. Honest limits:
  full-workspace cargo test exceeds 1500 s here, so run it in
  segments and keep the pass lines; Docker-sealed Linux E2E
  evidence can only come from the user's Windows T1–T4 run.
- [ ] Foundation T1–T4 (user's Windows side: WSL Ubuntu +
  Docker Desktop). Status unknown from this machine; do not
  claim progress without a signal from the user.

## Upstream queue (complete code only, after #505 merges)

Order after Ephemeral-AI-Lab/layerfs #505 merges:

1. Finding 2 upstream PR (lazy branch names; inspect.rs
   overlaps #505, which is why it waits).
2. P9 upstream PR (event stream, v2 contract).
3. Finding 3 upstream PR (streaming graph).
4. Finding 4 upstream PR (actual-size packing + mid-page
   resume).
5. Finding 5 upstream PR (changed_paths keyset cursor).
6. Later slices (6/7/8, P10–P15) as their own complete
   batches, in landing order.

Upstream commits carry NO Co-Authored-By trailer and a
measured Production LOC line. Push via the scratch clone
(fetch = upstream, push = fork); if a batch's base commits
contain `.github/workflows/*` changes, sync the fork in the
signed-in browser first (gh token lacks workflow scope).

## Daily watch

- Cron `layerfs-upstream-watch` (~09:38 CST, report-only):
  upstream main HEAD is still 35d36575 as of 2026-10-30
  morning. Never auto-merge; sync at a slice boundary with
  conflict resolution + tests, after user go-ahead or at a
  scheduled boundary.
