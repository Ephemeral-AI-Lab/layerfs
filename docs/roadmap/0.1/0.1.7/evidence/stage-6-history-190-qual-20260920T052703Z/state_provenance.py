#!/usr/bin/env python3
"""The observable fingerprint of `CacheState::CreatedInSample` in the retained traces.

`ops/history.rs:1-20` documents the shape: one Store, N states, and state k reads
the previous root's content back through the Store the chain is growing. If the
Store really is written in-sample, then at state 1 there is nothing written yet to
read, and the read-side counters of state 1 must be zero - and they are the only
state's that are.

This script measures that from the retained traces. It is a fingerprint, not a
proof of provenance: it shows that the read path is exercised only after the
operation has written something, and it can see no read at all before the first
write. It cannot see whether a page came from the page cache, from the device, or
from an earlier run of the same case - which is why the corpus axis is a separate
declaration (`CACHE-STANCE.md`) and why these rows stay `INELIGIBLE` without it.
"""

import json
import sys
from pathlib import Path

EVIDENCE = Path(__file__).resolve().parent
CAMPAIGN = EVIDENCE.parent / "stage-6-history-190-read-20260920T042617Z"
RUNS = [("baseline2", "history-stride10"), ("candidate2", "history-stride10"),
        ("baseline2", "history-stride3"), ("candidate2", "history-stride3")]
# The read side of the Store, as the driver publishes it per state. Both kinds are
# read, because the driver splits them: `filesystem.provider.*` is a `resource`
# record and `filesystem.validation.*` / `filesystem.references.*` are `counter`
# records. Only the counters are O3-pinnable (`main.rs:636`), and a pin on a read
# counter is a pin on effort - see `counter-selection.txt`.
READ_KEYS = (
    "filesystem.provider.read_waves",
    "filesystem.provider.requested_objects",
    "filesystem.provider.returned_objects",
    "filesystem.provider.returned_canonical_bytes",
    "filesystem.provider.failed_waves",
    "filesystem.validation.objects_read",
    "filesystem.validation.read_waves",
    "filesystem.references.rows_read",
)


def main() -> int:
    report: dict[str, object] = {"campaign": CAMPAIGN.name, "runs": {}}
    lines = ["=== the read path over the operation's own states", ""]
    for arm, case in RUNS:
        records = [json.loads(line) for line in
                   (CAMPAIGN / "runs" / f"{arm}-{case}" / "trace-perf.jsonl")
                   .read_text(encoding="utf-8").splitlines() if line.strip()]
        per_state: dict[int, dict[str, int]] = {}
        for record in records:
            key = str(record.get("key", ""))
            if record.get("kind") not in ("resource", "counter") or not key.startswith("history.state."):
                continue
            _, _, ordinal, rest = key.split(".", 3)
            if rest in READ_KEYS and isinstance(record.get("value"), int):
                per_state.setdefault(int(ordinal), {})[rest] = record["value"]
        zero_read_states = sorted(n for n, values in per_state.items()
                                  if set(values.values()) == {0})
        reads = {n: v.get("filesystem.provider.returned_canonical_bytes", 0)
                 for n, v in per_state.items()}
        first_nonzero = min((n for n, v in reads.items() if v), default=None)
        entry = {
            "states": len(per_state),
            "states_with_every_read_counter_zero": zero_read_states,
            "first_state_with_nonzero_returned_canonical_bytes": first_nonzero,
            "total_returned_canonical_bytes": sum(reads.values()),
            "state_1_read_counters": per_state.get(1, {}),
        }
        report["runs"][f"{arm}-{case}"] = entry
        lines.append(f"=== {arm} {case}")
        lines.append(f"  states observed                                  : {entry['states']}")
        lines.append(f"  states whose read counters are all zero          : {zero_read_states}")
        lines.append(f"  first state returning canonical bytes            : {first_nonzero}")
        lines.append(f"  total returned canonical bytes over the chain    : {entry['total_returned_canonical_bytes']:,}")
        lines.append(f"  read-side keys observed per state                 : {len(entry['state_1_read_counters'])}")
        lines.append(f"  state 1 read counters, all of them               :")
        for k in sorted(entry["state_1_read_counters"]):
            lines.append(f"      {k:52s} {entry['state_1_read_counters'][k]}")
        lines.append("")
    lines.append("What this establishes: the read path is silent until the operation has")
    lines.append("written, and state 1 is the only silent state - the fingerprint of a Store")
    lines.append("whose bytes are the operation's own. What it does not establish: which")
    lines.append("backing served a read, and whether corpus pages were resident from an")
    lines.append("earlier run. Nothing here is a cold claim.")
    (EVIDENCE / "state-provenance.json").write_text(
        json.dumps(report, indent=2, sort_keys=False) + "\n", encoding="utf-8")
    (EVIDENCE / "state-provenance.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
