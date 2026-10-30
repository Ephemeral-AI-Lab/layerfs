# P5 — MCP (IMPLEMENTATION, per D11)

Issue: layerfs-connect#56. Parent: layerfs-connect#14.
Design decision D11 (approved 2026-10-28).

Role: implementer. You do NOT commit, push, open PRs, or touch
the git clone, the baseline, or the connect checkout. The
commander verifies your diff, runs the tests itself, and does
all committing, pushing, and PR work.

Workdir: the Area P product tree (re-verify the p1-p15 tip at
launch).

## Contract first (mandatory, before any code)

P5 exposes the LayerFS surface over MCP. Before coding,
write the precise public contract beside the P6 cases: the
tool list (one tool per operation, or a justified grouping),
each tool's input/output schema, the error/refusal mapping
onto the existing refusal codes, and the parity matrix vs.
the Node SDK. Have the contract reviewed before
implementing.

## Implementation shape (D11, fixed)

- Node SDK + stdio transport first
  (@modelcontextprotocol/server 2.x + Zod 4).
- NO HTTP initially.
- The MCP server delegates to the Node SDK's public
  behavior only — no second Store implementation, no
  parallel operation set.

## Tests + verification

- Protocol tests: stdio framing, tool schemas, refusal
  mapping, and at least one end-to-end tool call against a
  real Disposable Store.
- Each new test must FAIL on old code for the intended reason
  and PASS after.
- Matrix: `export PATH="$HOME/.cargo/bin:$PATH"`, build first
  (--no-run), explicit ≤120 s timeouts: the suites you touch
  + the P6 contract suites + `sdk_method_golden` (under
  `umask 022`); `cargo +1.85.1 fmt --all -- --check`;
  workspace clippy (excl. node, `-D warnings`); `python3 -B
  tools/check_product_boundary.py` (+ tool self-tests);
  JS neighbors (area-p, calls, cli.test) if the JS surface
  moves.
- /dev/fuse and Docker do not exist here — do not touch
  mounted/real-E2E tests; disclose as NOT RUN.

## Progress + report

Append one line per suite to PROGRESS-p5.md (append-only).
Final report, concise: the contract (tools + schemas +
refusals), what changed, every suite with exact pass/fail
numbers, anything not verified and why. Do not paste whole
diffs.
