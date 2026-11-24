#!/usr/bin/env python3
"""Check the `#print axioms` receipts for the GC-admission Lean proof.

Usage: check_lean_axioms.py [receipt.txt]   (stdin when no path is given)

The only accepted axioms are the Lean-standard `propext`, `Classical.choice`
and `Quot.sound`.  `sorryAx` and any other name fail the check.  The receipt
must name every declaration in `REQUIRED` (the main theorem, the §9.4
corollaries of the shipped scope, the Inv7/EntriesSound/base-case lemmas and
the force-remove end) and every named declaration's closure must be within
`ALLOWED`.
A receipt may also say a declaration "does not depend on any axioms".

The script is deliberately small: it reads the `depends on axioms: [...]`
lines that `lake env lean GcAdmission/Axioms.lean` prints.  The workflow greps
for `sorry`/`admit`/`axiom`/`constant`/`native_decide` separately.
"""

from __future__ import annotations

import re
import sys

ALLOWED = {"propext", "Classical.choice", "Quot.sound"}
REQUIRED = {
    "GcAdmission.step_preserves",
    "GcAdmission.no_reclaim_live",
    "GcAdmission.compact_barrier",
    "GcAdmission.reconcile_only_dead",
    "GcAdmission.done_is_operator",
    "GcAdmission.place_fallback_conservative",
    "GcAdmission.conservative_no_death",
    "GcAdmission.unordered_never_dead",
    "GcAdmission.restore_is_uncertain",
    "GcAdmission.ordered_rule",
    "GcAdmission.ordered_rule_sequential",
    "GcAdmission.compareMachine_uncertain_cases",
    "GcAdmission.unordered_place_never_dead",
    "GcAdmission.recorded_newer_is_uncertain",
    "GcAdmission.done_survives_compact",
    "GcAdmission.preview_is_done",
    "GcAdmission.Inv7_holds",
    "GcAdmission.Inv7_preserves",
    "GcAdmission.Inv3_preserves",
    "GcAdmission.Inv4_preserves",
    "GcAdmission.EntriesSound_preserves",
    "GcAdmission.empty_inv",
    "GcAdmission.force_remove_ends_mounts",
}

LINE = re.compile(
    r"'(?P<name>[A-Za-z0-9_.]+)' "
    r"(?:depends on axioms:\s*\[(?P<axioms>[^\]]*)\]|does not depend on any axioms)"
)


def main(argv: list[str]) -> int:
    if len(argv) > 1:
        text = open(argv[1], encoding="utf-8").read()
    else:
        text = sys.stdin.read()

    if "sorryAx" in text:
        print("check_lean_axioms: sorryAx found", file=sys.stderr)
        return 1

    seen: dict[str, set[str]] = {}
    for match in LINE.finditer(text):
        names = {name.strip() for name in (match.group("axioms") or "").split(",") if name.strip()}
        seen[match.group("name")] = names

    if not seen:
        print("check_lean_axioms: no axiom receipt line found", file=sys.stderr)
        return 1

    missing = sorted(REQUIRED - seen.keys())
    if missing:
        print(f"check_lean_axioms: missing receipts: {missing}", file=sys.stderr)
        return 1

    for name in sorted(seen):
        bad = sorted(ax for ax in seen[name] if ax not in ALLOWED)
        if bad:
            print(f"check_lean_axioms: {name} uses disallowed axioms: {bad}", file=sys.stderr)
            return 1

    all_axioms = sorted({ax for axs in seen.values() for ax in axs})
    print(f"check_lean_axioms: OK ({len(seen)} receipts; {', '.join(all_axioms) or 'no axioms'})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
