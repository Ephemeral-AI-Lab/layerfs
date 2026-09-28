# #273 checkpoint 5 evidence catalog

> **Status:** Retained checkpoint-5 evidence index. Raw attempt receipts stay
> append-only in the gitignored results tree; this directory publishes the
> derived summary, the raw-receipt hash index and the correctness/space route
> receipts.

## Contents

| File | What it is |
| --- | --- |
| `RESULTS.json` | Derived summary of all 24 performance attempts and the 10 route executions: statuses, walls, counters, space scopes, identities. |
| `CAMPAIGN-RECEIPTS.sha256` | SHA-256 index of every raw `campaign-*/<arm>/<NN>-<case>/receipt.json`, with repo-relative paths. |
| `checkpoint5-route-<case>-result.json` | The route row's own receipt (identity, checks, isolation, wall). |
| `checkpoint5-route-<case>-test.stdout` | The product test's own stdout, including `STAGE_CHECK`, `STAGE_ALLOCATION`, `STAGE_PHASE` and `STAGE_HOT` lines. |
| `checkpoint5-route-<case>-test.stderr`, `-service.stderr` | Retained process-level output. |
| `SHA256SUMS` | Hashes of everything in this directory. |

## Raw evidence location

```
benchmark-results/fs-bench-pro/issue273/checkpoint5/
  oracle-1/                      shared independent oracle build
  prepared-control/              control arm: binaries, sealed masters, oracle manifests, images
  prepared-candidate/            candidate arm: same, plus its own harness seal
  prepared-candidate-failed-0*/  retained setup failures (no measured command ran)
  retained-1..5/                 retained 4,097-record base preparations; retained-5 is the one used
  candidate-preflight-*/ control-preflight-*/   labelled harness pre-flights, not results
  campaign-1, campaign-2/        retained harness-defect campaign attempts (no measured command ran)
  campaign-3/                    the campaign of record: 12 selections x control, candidate
  report-1/                      derived tables, ratios and the space-scope correction
```

Reproduce the summary with:

```sh
python3 core/benchmark/fs-bench-pro/checkpoint5_273.py report \
  --root benchmark-results/fs-bench-pro/issue273/checkpoint5/campaign-3 \
  --route core/target/issue273/checkpoint5-route-1/supervisor-result.json \
  --output benchmark-results/fs-bench-pro/issue273/checkpoint5/report-1
```

## Read this before quoting a number

- No row is admission eligible; every numeric ratio is `INELIGIBLE`, and the
  Commit phase and complete command carry a declared container-cache
  limitation.
- `private_backing_allocated_bytes` and `metadata_allocated_bytes_subscope` are
  nested; the per-attempt field `charged_backing_bytes` summed them and is
  superseded.
- Historical #271 and `issue261`/`issue265` wall times are diagnostic context
  only and are never a denominator for these rows.
