#!/usr/bin/env python3
"""Append the R8c outcome of every selection registered in v4.

Reads the R8b outcomes (045), registration v4, the final suite summaries and
each registered invocation's receipt; writes 50-outcomes.json and
51-outcomes.md. Nothing in 045 or 061 is rewritten. A row that R8b left open
is closed here only by a named owner decision (C-1 to C-20), and the kind of
that decision travels with the row: a fix, a ruling, or a withdrawn scope.
Invocation statuses are read from receipts, never typed here.
"""
import collections
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[7]
CHECKS = "core/docs/issues/307/checks/r8c-close-20261010"
OLD = "core/docs/issues/307/checks/r8b-requalification-20261010"
REGISTRATION = "core/benchmark/fs-bench-pro/registry/r8-integrated-qualification-v4.json"
SUITES = [CHECKS + "/11-final-host-suite.txt", CHECKS + "/15-final-linux-suite.txt"]
# id: (decisions, kind, what closes it, scope withdrawn or declared)
ROWS = {
    "FP-1": (["C-14"], "withdrawn scope", "Lifecycle 41 and full proof 30 at the final identity.",
             "Session-buffer observation: no product observation exists. Resident memory is reported at phase "
             "boundaries against no bound."),
    "FP-2": (["C-2", "C-9"], "fix", "Largest READ and WRITE frames are 131072, the negotiated limit, by the "
             "request service's own accounting; one loop asserted against the corrected specification.",
             "The maxima are the product's observation, not an independent trace."),
    "FP-5-Runtime": (["C-1", "C-11"], "fix", "Confinement 35: 84 protected routes refused and the command "
                     "cannot create a user and mount namespace (EPERM).",
                     "Declared as they are: sibling visibility, the unauthenticated listener connect."),
    "FP-6-Runtime": (["C-7", "C-18", "C-14"], "withdrawn scope", "Streams 40: exact delivery up to 64 MiB each.",
                     "A stalled or failing sink, stressed ordering, a resident bound."),
    "FP-7-Runtime": (["C-6", "C-18"], "ruling", "Streams 40: own bytes and status beside a descendant; complete "
                     "bytes and status 7. The stream ends at command exit by C-6.",
                     "A descendant holding a file; explicit disposal."),
    "FP-15": (["C-3"], "withdrawn scope", "Flagged-WRITE counts, handle and RELEASE order, dirty-page Commit.",
              "WRITE header identity."),
    "FP-16": (["C-3"], "withdrawn scope", "Size never shrinks under concurrent append and read.",
              "A GETATTR held across a WRITE by an external fixture."),
    "FP-21": (["C-8"], "withdrawn scope", "Close retained; Busy beside a held OPEN or READDIR; FORGET units "
              "before revocation.", "Join, Owner, Lane and Registry failure stages."),
    "FP-22-FS": (["C-1", "C-11"], "fix", "Holders 36 to 39: every arrangement gives the reversible Busy and the "
                 "sibling reaches Unmounted and Gone. The private-namespace holder's unshare is refused, so no "
                 "copy of a mount exists.", "Force is unavailable in the Sandbox topology, declared by C-11."),
    "FP-23-FS": (["C-8"], "withdrawn scope", "Active-producer refusal and staged abort and detach outcomes.",
                 "The abort-error and independent-effect scopes it inherits from FP-33."),
    "FP-25-Routes": (["C-7"], "withdrawn scope", "Lost Mount and Attach acknowledgements located without replay.",
                     "Attach failure after a successful mount call; a lost runtime result at the actual topology."),
    "FP-26": (["C-5"], "ruling", "Mount refused at mount:debt while maintenance is stopped (mount_debt).",
              "A stop on an otherwise usable engine; a headroom number; Status."),
    "FP-27": (["C-8", "C-9"], "withdrawn scope", "A failing READ fails alone; a Capacity refusal is retained.",
              "Release after a failed demand, concurrent demand, foreign reader, invariant and poisoned-lock "
              "classes. A Capacity refusal fences the mount, declared by C-9."),
    "FP-30-Runtime": (["C-7"], "withdrawn scope", "Streams 40: window-edge sizes, the last megabyte before a "
                      "nonzero exit, a descendant-held pipe.", "A fast exit with a delayed consumer."),
    "FP-31": (["C-8", "C-14"], "withdrawn scope", "Underflow and failure custody on a real kernel FORGET.",
              "Foreign incarnation; a resident-memory measurement."),
    "FP-33": (["C-8"], "withdrawn scope", "Clean and held-reference forced detach outcomes with no replay.",
              "Short, failed and unknown abort writes, refusal after admission, an independent trace."),
    "R2-STEP-2": (["C-2", "C-19"], "withdrawn scope", "Lifecycle 41; FP-2 complete.",
                  "Startup and terminal failure paths beyond those staged."),
    "R2-STEP-4": (["C-7"], "withdrawn scope", "Attach, Locate and acknowledgement routes.", "FP-25-Routes' scopes."),
    "R2-STEP-5": (["C-8", "C-1"], "withdrawn scope", "Reversible Busy at the actual topology (36 to 39).",
                  "FP-21's stages with no producer."),
    "R2-STEP-6": (["C-1"], "fix", "Confinement 35 with namespace creation refused.", ""),
    "R3-STEP-6": (["C-3"], "withdrawn scope", "Shared-mapping store flag, clipping, size and dirty-page Commit.",
                  "WRITE header identity."),
    "R3-STEP-7": (["C-3"], "withdrawn scope", "Lifecycle 41; FP-15 and FP-16 as rescoped.", "As FP-15 and FP-16."),
    "R4-6": (["C-20"], "ruling", "Change-sized producer, owner and validation counts.",
             "Sibling-page and Store work grows with base size: a recorded property of the format."),
    "R5-6": (["C-10"], "ruling", "Mounted missing dependency, unknown History, install failure after publication.",
             "Reproduction R5-6a needs outside deletion of Store rows; outside the profile's guarantees."),
    "R5-7": (["C-8"], "withdrawn scope", "A release refused after a settled failure keeps the owner named.",
             "A release error after a settled failure; a release refused before admission."),
    "P-1": (["C-4"], "fix", "The runtime deploys serial_low_water 256; lifecycle 41 and every runtime proof ran "
            "with it; the mechanism is proved by its own tests.",
            "The non-contention early-failure arm is reached by no test."),
    "R6-7": (["C-5"], "ruling", "As FP-26.", "As FP-26."),
    "R6-8": (["C-7", "C-6"], "withdrawn scope", "Streams 40 exact.", "Backpressure against a stalled sink."),
}
PASSED_AT_R8B = {"R2-STEP-3", "R2-STEP-7", "R4-2", "R4-3", "R4-9", "R5-5", "R5-9", "R5-10", "FULL-FIXTURE-ORACLE"}
INVOCATIONS = [
    ("R8b-full-fixture-mounted", "30-full-mounted-proof/proof.json", "30-full-mounted-proof.stderr"),
    ("Q1-wide-host", "31-host-wide-proof/result.json", None),
    ("Q1-wide-linux", "32-linux-wide-proof/result.json", None),
    ("Q1-host-handoff", "33-host-linux-handoff-proof/result.json", None),
    ("Q1-shared-processes-linux", "34-linux-shared-process-proof/result.json", None),
    ("R8b-confinement", "35-confinement/result.json", "35-confinement.stderr"),
    ("R8b-holder-cwd", "36-holder-cwd/result.json", "36-holder-cwd.stderr"),
    ("R8b-holder-descriptor", "37-holder-descriptor/result.json", "37-holder-descriptor.stderr"),
    ("R8b-holder-mapping", "38-holder-mapping/result.json", "38-holder-mapping.stderr"),
    ("R8b-holder-private", "39-holder-private/result.json", "39-holder-private.stderr"),
    ("R8b-streams", "40-streams/result.json", "40-streams.stderr"),
    ("R8b-lifecycle", "41-lifecycle/result.json", "41-lifecycle.stderr"),
]
PREREQ = {"PREREQ-COMPLETE-INSTALLED-HOST": "Q1-wide-host", "PREREQ-COMPLETE-INSTALLED-LINUX": "Q1-wide-linux",
          "PREREQ-HOST-HANDOFF": "Q1-host-handoff", "PREREQ-SHARED-PROCESSES-LINUX": "Q1-shared-processes-linux"}
GATE_FINAL = {"G05": ("MET AS RESCOPED", "All seven runtime selections PASS at v4 (35 to 41), the "
                      "private-namespace holder among them after fix C-1. C-6 rules the stream end; C-7 and C-18 "
                      "withdraw what the runtime protocol cannot construct."),
              "G07": ("MET", "Full-byte proof 30 PASS under the unchanged 100 s and 85 s stops."),
              "G09": ("MET AS RESCOPED", "C-14: lifecycle 41 reports daemon resident set and threads, cgroup "
                      "memory and Store and Overlay backing bytes at six phase boundaries. Not continuous "
                      "peaks; no bound exists. Unobserved: immutable-cache charge, demand transients, owned "
                      "request bytes, pager and journal statistics, receive buffers.")}


def sha(path):
    return hashlib.sha256((ROOT / path).read_bytes()).hexdigest()


def suite_binaries():
    found = {}
    for path in SUITES:
        side = "host" if "host" in path else "linux"
        for line in (ROOT / path).read_text().splitlines():
            match = re.match(r"(\S+)-[0-9a-f]{16} exit=(\d+) ", line)
            if match:
                found.setdefault(match[1], {})[side] = int(match[2])
    return found


def invocations():
    rows = {}
    for identifier, result, stderr in INVOCATIONS:
        record = json.loads((ROOT / CHECKS / result).read_text())
        text = (ROOT / CHECKS / stderr).read_text() if stderr else (
            ROOT / CHECKS / result).with_name("stderr.txt").read_text()
        wall = re.search(r"R4_WALL seconds<=(\d+) limit=(\d+)s", text)
        rows[identifier] = dict(id=identifier, status=record["status"], attempts=1,
                                receipt=CHECKS + "/" + result, receipt_sha256=sha(CHECKS + "/" + result),
                                wall_seconds_at_most=int(wall[1]), limit_seconds=int(wall[2]),
                                wall_note="command custody bound, not a measurement")
    return rows


def main():
    old = json.loads((ROOT / OLD / "045-outcomes.json").read_text())
    registration = json.loads((ROOT / REGISTRATION).read_text())
    binaries, runs = suite_binaries(), invocations()
    if any(run["status"] != "PASS" for run in runs.values()):
        raise SystemExit("a registered invocation did not pass; this generator records the all-pass case only")
    functional = []
    for row in old["functional_rows"]:
        new = dict(id=row["id"], stage=row["stage"], status_r8b=row["status"])
        covering = {name: binaries.get(name) for name in row["covering_test_binaries"]}
        # A name with no suite line is a support file, not a test binary. The
        # three input-gated binaries fail in a plain suite by design and are
        # the registered precondition proofs.
        bad = [name for name, sides in covering.items() if sides and any(sides.values()) and name not in (
            "complete_installed_roots", "host_handoff", "shared_processes")]
        if bad:
            raise SystemExit("covering binary failing: %s %s" % (row["id"], bad))
        if row["id"] in ROWS:
            decisions, kind, closes, scope = ROWS[row["id"]]
            new.update(status="PASS", basis=kind, decisions=decisions, established=closes,
                       withdrawn_or_declared=scope)
        elif row["status"] == "WITHDRAWN":
            new.update(status="WITHDRAWN", basis="withdrawn prospectively by R0, unchanged")
        elif row["id"] in PREREQ:
            new.update(status=runs[PREREQ[row["id"]]]["status"], basis="registered precondition proof at v4",
                       established=runs[PREREQ[row["id"]]]["receipt"])
        elif row["status"] == "PASS":
            new.update(status="PASS", basis="as scoped", established=(
                "Passed at R8b; successor proof 30 PASS at v4." if row["id"] == "FULL-FIXTURE-ORACLE" else
                "Closed at R8b by added tests or proofs; same assertions pass at v4." if row["id"] in PASSED_AT_R8B
                else "Same assertions pass in the final suites at the v4 source."))
        else:
            raise SystemExit("no R8c disposition for " + row["id"])
        new["covering_test_binaries"] = covering
        functional.append(new)
    if set(ROWS) - {row["id"] for row in functional}:
        raise SystemExit("disposition names an unknown row")
    gates = []
    for gate in registration["required_function_count_resource_order"]:
        status, reason = GATE_FINAL.get(gate["id"], (gate["status"], gate["reason"]))
        gates.append(dict(id=gate["id"], requirement=gate["requirement"], status=status, reason=reason))
    timing = [dict(case_id=row["case_id"], arm=row["arm"], cache_class=row["cache_class"], status="NOT_RUN",
                   attempted_operation_count=0, new_receipt=None, reason_061=row["reason_061"],
                   reason="C-15: zero attempts authorized; no timing selection could be judged.")
              for row in old["timing_rows"]]
    owner = []
    for item in old["owner_questions"]:
        answered = int(item["id"].split("-")[1]) >= 8
        owner.append(dict(item, status="ANSWERED UNDER OWNER INSTRUCTION (C-1 to C-20)" if answered
                          else "PENDING OWNER"))
    basis = collections.Counter(row.get("basis", "") for row in functional if row["status"] == "PASS")
    counts = collections.Counter(row["status"] for row in functional)
    commit = subprocess.run(["git", "-C", str(ROOT), "log", "-1", "--format=%H", "--", REGISTRATION],
                            check=True, capture_output=True, text=True).stdout.strip()
    result = dict(
        schema="r8c-integrated-examination-outcomes-v1",
        status="CLOSED - QUALIFIED ON FUNCTION, COUNTS AND RESOURCES AT THE OWNER-RESCOPED SCOPE; TIMING NOT_RUN",
        meaning="Every selection registered in v4 has an outcome. Rows closed by a ruling or a withdrawn scope "
                "say so; nothing withdrawn is reported as proved. Not a timing, cold-cache or release claim.",
        owner_instruction=registration["owner_instruction"],
        registration=dict(path=REGISTRATION, sha256=sha(REGISTRATION), commit=commit),
        frozen_source=registration["identities"]["source_commit"],
        frozen_product=registration["identities"]["product_tree"],
        previous_outcomes=dict(path=OLD + "/045-outcomes.json", sha256=sha(OLD + "/045-outcomes.json"),
                               status=old["status"], functional_status_counts=old["functional_status_counts"]),
        Durable=old["Durable"], registered_invocations=list(runs.values()),
        registered_invocation_status_counts={"PASS": len(runs)}, gates=gates,
        functional_count=len(functional), functional_status_counts=dict(sorted(counts.items())),
        functional_pass_basis_counts=dict(sorted(basis.items())), functional_rows=functional,
        timing_count=len(timing), timing_samples=0, timing_status_counts={"NOT_RUN": len(timing)},
        timing_zero_attempt_reason=registration["timing_zero_attempt_reason"], timing_rows=timing,
        owner_questions=owner)
    (ROOT / CHECKS / "50-outcomes.json").write_text(json.dumps(result, indent=1, sort_keys=True) + "\n")

    lines = ["# R8c outcomes of every registered selection", "",
             "> **Status:** Dated planning checkpoint; not release evidence or a product contract.", "",
             "Generated by [make_outcomes_r8c.py](tools/make_outcomes_r8c.py) from registration v4, the",
             "[R8b outcomes](../r8b-requalification-20261010/045-outcomes.json) and each receipt. Machine form:",
             "[50-outcomes.json](50-outcomes.json). Earlier outcomes are unchanged.", "",
             "## Registered invocations (one attempt each)", "",
             "| Selection | Outcome | Custody bound | Receipt |", "| --- | --- | --- | --- |"]
    for run in runs.values():
        lines.append("| `%s` | **%s** | at most %d s of %d s | [%s](%s) |" % (
            run["id"], run["status"], run["wall_seconds_at_most"], run["limit_seconds"],
            run["receipt"].split("/")[-2], run["receipt"].split(CHECKS + "/")[1]))
    lines += ["", "The custody bound is the command supervisor's whole-second ceiling, not a measurement.", "",
              "## Gates", "", "| Gate | Requirement | Status | Reason |", "| --- | --- | --- | --- |"]
    for gate in gates:
        lines.append("| %s | %s | **%s** | %s |" % (gate["id"], gate["requirement"], gate["status"], gate["reason"]))
    lines += ["", "## Functional rows", "",
              "R8b: " + ", ".join("%s %d" % item for item in sorted(old["functional_status_counts"].items()))
              + ". R8c: " + ", ".join("%s %d" % item for item in sorted(counts.items())) + ".",
              "Of the PASS rows: " + ", ".join("%d %s" % (count, name) for name, count in sorted(basis.items()))
              + ".", "",
              "Rows closed in R8c, with the kind of decision that closed each:", "",
              "| Row | R8b | Closed by | Kind | Established | Withdrawn or declared, not proved |",
              "| --- | --- | --- | --- | --- | --- |"]
    for row in functional:
        if row["id"] in ROWS:
            lines.append("| %s | %s | %s | %s | %s | %s |" % (
                row["id"], row["status_r8b"], ", ".join(row["decisions"]), row["basis"], row["established"],
                row["withdrawn_or_declared"] or "nothing"))
    lines += ["", "The decisions and their alternatives are in",
              "[00-owner-instruction-and-decisions.md](00-owner-instruction-and-decisions.md).", "",
              "## Timing rows", "",
              "All %d timing selections: **NOT_RUN**, zero attempts, zero samples. %s" % (
                  len(timing), registration["timing_zero_attempt_reason"]), "",
              "## Owner questions", "",
              "OWNER-1 to OWNER-7 stay `PENDING OWNER`. OWNER-8 to OWNER-21 are answered by decisions taken",
              "under the owner instruction; each decision records the alternative the owner may prefer."]
    (ROOT / CHECKS / "51-outcomes.md").write_text("\n".join(lines) + "\n")
    print(dict(counts), dict(basis), len(timing))


if __name__ == "__main__":
    main()
