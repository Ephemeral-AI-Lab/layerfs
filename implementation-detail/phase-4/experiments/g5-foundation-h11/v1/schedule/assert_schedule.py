#!/usr/bin/env python3
import csv
import pathlib


schedule = pathlib.Path(__file__).with_name("SCHEDULE-v1.tsv")
with schedule.open(newline="", encoding="utf-8") as handle:
    rows = list(csv.DictReader(handle, delimiter="\t"))
observed = [(int(row["history_revisions"]), int(row["sample"])) for row in rows]
expected = [(1, 1), (10, 1), (100, 1), (1000, 1), (1000, 2), (100, 2), (10, 2), (1, 2)]
assert observed == expected
assert [int(row["ordinal"]) for row in rows] == list(range(1, 9))
assert len(set(observed)) == 8
print('{"schema":"phase4-g5-h11-schedule-check-v1","status":"PASS","rows":8}')
