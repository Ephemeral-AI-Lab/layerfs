# #245 F comparison after the current-main merge

> **Status: Current planning checklist; no release candidate exists.**
> Frozen after the new control arm and before any candidate preparation or run
> at the merged source. The earlier F-1 target and every earlier receipt keep
> their own identities and results.

The merge from `origin/main` changed both the product and broad harness seals,
so the earlier F-1 control cannot qualify the merged PR head. This is a new
matched comparison with the same owner-approved F-1 rule: all four registered
cases need functional, cleanup and sealed-verifier PASS, plus complete-command
wall no greater than twice this control's wall. The registered 15 s command
limit (25 s declared mixed-refresh exception) and 9 s verifier limit still
apply. Container FUSE backing and Edit-written cache state are uncontrolled;
all latency cells are `INELIGIBLE`, with no speedup or cold-cache claim.

The one-attempt control is
`benchmark-results/fs-bench-pro/issue245-shell-package-f-main-control-01/`,
prepared at `issue245-shell-package-f-main-control-prepared-01/prepared.json`.
Its clean source is `ae8cc3a3e2b849caa9b0376c605665d5f4eb7d6a`
(pre-D/E `fdfc41032` plus current `main` and the identical SDK Init conflict
resolution). Product seal
`f97bb05a479f9412be0291f5fda0512431f005d9d17901edcdbf1307a0732ea0`,
harness seal `d12b542bc43a34a8addd29f17943f0a2ae4f29c9dbf06569f5e69cbbbd51d11f`,
registry SHA-256 `1a7e1a3f7ea40ea14ed9f97865260c936df53601cfd5d0082c0db4041849cd3c`,
image `sha256:3a2674f6edfe233448ea18f10550d9465e7fb1230dbf7421bd963976a71fcb5a`.
The closed masters were validated once; each case used an independent
`shutil.copyfile` byte clone and one construction worker. All four control
functional, cleanup and verifier results are PASS; all latency rows are
INELIGIBLE.

| Case | Control complete wall ns | Frozen candidate maximum ns (2×) | Registered complete limit |
| --- | ---: | ---: | ---: |
| `mixed-refresh-v1` | 2,119,044,834 | 4,238,089,668 | 25 s exception |
| `overwrite-4k-v1` | 919,394,834 | 1,838,789,668 | 15 s |
| `repeated-one-byte-v1` | 1,237,038,875 | 2,474,077,750 | 15 s |
| `failed-command-no-commit-v1` | 5,876,203,333 | 11,752,406,666 | 15 s |

Prepare once at a clean merged candidate source and run one candidate attempt
per registered case. The candidate must have the same harness and registry
seals, one construction worker, independent byte-copy clones and the same
cache contract. The failed-command case must make zero Commit calls and retain
the old head. Report an envelope miss as FAIL, not INELIGIBLE; report every
unattempted case as NOT_RUN. Do not rerun the control or select a best wall.
