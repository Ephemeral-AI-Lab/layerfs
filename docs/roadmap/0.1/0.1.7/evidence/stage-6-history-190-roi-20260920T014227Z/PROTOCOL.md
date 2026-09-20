# Remaining read cost: bounded ROI investigation

> Status: Research; informative and not a product contract.

Base81f4f1fefcd5f600742fb8b0fe4c0502354db09d. Owner requested one narrow attribution step and at most one clearly justified local optimization. No permanent telemetry, cache capacity/lifetime/budget changes or policy defaults are planned. Stop with a documented no-change result if attribution is unclear or the gain needs broader cache semantics.

First run one labelled native sample profile of history-stride10 with the immutable PR195 candidate executable, whose compilation source exactly matches this base. Attach to exact childPID immediately after launch, sample all threads every5ms until childexit (requested max120s, workload/sampler complete budget120s). Retain attach gap, profiler start/end, stdout/stderr/report and commands. Shared locks and existing no-named-competitor/70%CPU-idle preflight apply; profiler is declared measurement overhead, not an independent competing workload. No build needed; no third-party edits.

Profile timing is not a speed comparison. Sample counts are estimated stack residence, not exact CPU or elapsed attribution. Restrict disjoint classification to provider/read ancestry and keep unknown samples; do not sum ancestors/descendants or translate sample share into exact savedtime. Preparation/parser/codegen stacks under a named repeated SQL helper can justify existing prepared-statement reuse; SQLite step/copy stacks alone cannot. No broader pack/value cache even if those costs dominate.

If a small source-confirmed lever is justified, freeze it before one unprofiled matched baseline/candidate stride10 pair and confirm on stride3 only after correctness/reducedwork and useful operation improvement. Same harness/source inputs except declared treatment; archived baseline reused via original provenance and equivalence hashes. Sample order baseline10/candidate10/baseline3/candidate3, performance120/240s and separateverify60s hard plus10/20s targets; no rerun for a better number, no stride1. Fresh outputs; existing corpus reused. LAYERFS_CONSTRUCTION_WORKERS=1, LAYERFS_HISTORY_PHASES=1; eight behavior switches unset. Cache uncontrolled; admissionINELIGIBLE, O3pinsINCOMPLETE. All misses/failures remain.

Source/tests/docs ownership split among subagents; root serializes resource commands. Product LOC starts85723 (reference65417/core20306). Final checks explicit core workspace tests/examples/Clippy/fmt/boundary; noCI/preflight. Each step posted to#190; receipts/ledger retained.
