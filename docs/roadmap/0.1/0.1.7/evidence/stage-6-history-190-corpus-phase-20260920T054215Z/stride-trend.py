#!/usr/bin/env python3
"""Is the per-state cost driven by content, or by chain depth?

Stride1's last decile costs 24x its first. That is either the workload (bigger
commits later in the history) or the chain (a per-state term that grows with how
much is already stored). The two have opposite product meanings, so this separates
them: per-state elapsed against changed bytes and against the state's ordinal, with
the decile-normalised cost — nanoseconds per changed byte — as the discriminator.
A flat ns/byte across deciles means content-proportional. A rising ns/byte means a
depth term.

Also checks the two Stores this round built against the retained campaign's
candidate2 Stores byte for byte, because the space gate is on *allocated* bytes and
allocation can move under identical content.
"""
import hashlib
import json
import sys
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent
RETAINED = CAMPAIGN.parent / "stage-6-history-190-read-20260920T042617Z"
ROWS = ("history-stride10", "history-stride3", "history-stride1")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def per_state(case: str):
    raw = CAMPAIGN / "runs" / f"diagnostic-{case}" / "raw"
    tree = json.loads((raw / "timing.json").read_text())
    elapsed = {int(child["name"].rsplit(".", 1)[-1]): int(child["elapsed_ns"])
               for child in tree["children"]}
    changed = {}
    for line in (raw / "trace.jsonl").read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        record = json.loads(line)
        key = str(record.get("key", ""))
        if record.get("kind") == "counter" and key.endswith(".changed_bytes") and key.startswith("history.state."):
            changed[int(key.split(".")[2])] = int(record["value"])
    ordinals = sorted(elapsed)
    return ([elapsed[n] for n in ordinals], [changed.get(n, 0) for n in ordinals])


def pearson(xs, ys):
    n = len(xs)
    mx, my = sum(xs) / n, sum(ys) / n
    cov = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
    vx = sum((x - mx) ** 2 for x in xs) ** 0.5
    vy = sum((y - my) ** 2 for y in ys) ** 0.5
    return cov / (vx * vy) if vx and vy else float("nan")


def main() -> int:
    lines = []
    for case in ROWS:
        elapsed, changed = per_state(case)
        n = len(elapsed)
        ordinal = list(range(1, n + 1))
        decile = max(1, n // 10)
        lines.append(f"=== {case} ({n} states)")
        lines.append(f"  correlation of per-state elapsed with changed bytes : {pearson(elapsed, changed):+.3f}")
        lines.append(f"  correlation of per-state elapsed with ordinal       : {pearson(elapsed, ordinal):+.3f}")
        lines.append(f"  {'decile':>7} {'mean ms':>9} {'mean changed B':>15} {'ns per changed B':>17}")
        for index in range(10):
            lo, hi = index * decile, min(n, (index + 1) * decile)
            if lo >= hi:
                break
            e = sum(elapsed[lo:hi]) / (hi - lo)
            c = sum(changed[lo:hi]) / (hi - lo)
            lines.append(f"  {index + 1:>7} {e / 1e6:>9.1f} {c:>15,.0f} {(e / c if c else float('nan')):>17.1f}")
        lines.append("")
    lines.append("=== Store bytes: this round's runs against the retained campaign's candidate2 Stores")
    for case in ROWS:
        mine = CAMPAIGN / "runs" / f"diagnostic-{case}" / "raw" / "sample.sqlite"
        theirs = RETAINED / "runs" / f"candidate2-{case}" / "raw" / "sample.sqlite"
        if not theirs.exists():
            lines.append(f"  {case:18s} no retained candidate2 Store on disk to compare")
            continue
        a, b = sha256(mine), sha256(theirs)
        lines.append(f"  {case:18s} identical={a == b}  mine {a[:16]}…  retained {b[:16]}…")
    print("\n".join(lines))
    (CAMPAIGN / "stride-trend.txt").write_text("\n".join(lines) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
