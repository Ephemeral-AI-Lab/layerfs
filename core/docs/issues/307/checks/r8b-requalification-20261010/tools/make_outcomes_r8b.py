#!/usr/bin/env python3
"""Append the R8b outcome of every selection registered in v3.

Reads the closed examination's outcomes (061), registration v3, the final
suite summaries and the result of each registered invocation, and writes
045-outcomes.json and 046-outcomes.md. The 061 row is carried unchanged under
`status_061`; nothing in 061 is rewritten. A row's R8b status is the lead's
judgement recorded in ROWS below, against the row's own retained gaps; every
invocation status is read from its receipt, never typed here.
"""
import collections
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[7]
CHECKS = "core/docs/issues/307/checks/r8b-requalification-20261010"
OLD = "core/docs/issues/307/checks/r8-qualification-20261010"
REGISTRATION = "core/benchmark/fs-bench-pro/registry/r8-integrated-qualification-v3.json"
TESTS = "core/crates/layerfs-daemon/tests/"
SUITES = [CHECKS + "/021-final-host-suite.txt", CHECKS + "/025-final-linux-suite.txt"]

# id: (status, what R8b added, what remains open)
ROWS = {
    "FP-1": ("PARTIAL", "Complete SDK/Sandbox/release-daemon lifecycle PASS at the final identity (044) and a "
             "terminal lifecycle after the full proof (033).",
             "No independent session-buffer observation; resident memory is sampled at phase boundaries only and "
             "judged against no bound."),
    "FP-2": ("PARTIAL", "Forbidden negotiation set compared by name with the Ready receipt, mount options and the "
             "connection's own fusectl values (fp2_negotiation_flags).",
             "Observed maximum READ and WRITE request sizes need a system-call observation of the test's own "
             "threads: PENDING OWNER. Specification 8.1 says two loops, the product runs one; selected INIT "
             "flags are computed, not read back."),
    "FP-5-Runtime": ("PARTIAL", "Actual topology at the final identity (038): 84 protected routes refused, no "
                     "inherited descriptor, declared identity and capability sets, both Workspaces answer after.",
                     "Sibling Workspace is listable from a command in another; an unprivileged command may create "
                     "a user and mount namespace; the control listener accepts an unauthenticated connect. No "
                     "declaration exists to judge them: PENDING OWNER."),
    "FP-6-Runtime": ("PARTIAL", "Exact delivery of both streams at every declared size up to 64 MiB each through "
                     "the real Engine at the final identity (043).",
                     "A stalled or failing sink and stressed ordering are not constructible through the runtime "
                     "protocol; no resident bound is observed."),
    "FP-7-Runtime": ("PARTIAL", "Descendant-held stdout: the command's own bytes and status are exact; a command "
                     "exiting 7 delivers complete bytes and its actual status (043).",
                     "A descendant's line written after the command exits is not delivered; which stream-EOF "
                     "sentence governs is PENDING OWNER. File-held and disposal cases are not run."),
    "FP-15": ("PARTIAL", "A still-dirty shared-mapping store is outside the Commit taken before msync and inside "
              "the next (r5_9_dirty_mapping), under reading L-6.",
              "WRITE header identity is not observed (same observation question as FP-2)."),
    "FP-16": ("PARTIAL", "Nothing added.", "A GETATTR held after attribute sampling across WRITE has no "
              "real-resource holding fixture; NOT_RUN."),
    "FP-21": ("PARTIAL", "Close retained after Revoke; Unmount beside an OPEN or READDIR held in its Store read "
              "answers Busy with no effect; FORGET units parked at detach run before revocation "
              "(fp21_detach_stages).",
              "Join, Owner, Lane and Registry stages have no producer at a real mount without a product hook."),
    "FP-22-FS": ("FAIL", "Actual topology at the final identity: cwd, descriptor and mapping holders give the "
                 "reversible Busy and the sibling reaches Unmounted and Gone (039, 040, 041).",
                 "A holder in a private user and mount namespace kept the sibling's normal Unmount from "
                 "completing inside its stop (042 FAIL; diagnosis 042-holder-private-diagnosis.md). Force is "
                 "refused in the Sandbox topology. Topology decision PENDING OWNER; not repaired."),
    "FP-23-FS": ("PARTIAL", "Nothing added.", "Depends on FP-33's unrun abort-error and independent-effect scopes."),
    "FP-25-Routes": ("PARTIAL", "Nothing added; no selection was registered for it.",
                     "Attach failure after a successful mount syscall is not staged; lost runtime-result "
                     "ownership at the actual topology is not run."),
    "FP-26": ("PARTIAL", "Mount is refused Capacity at mount:debt while maintenance is stopped, with no entry, "
              "read or owner job (product fix ea812d3f8, mount_debt).",
              "Staged for a quarantined engine only; maintenance stopped on a usable engine is not staged. "
              "Debt headroom value and maintenance failure in Status: PENDING OWNER."),
    "FP-27": ("PARTIAL", "A READ whose own demand meets the quarantined reader fails alone with EIO; a read "
              "refused Capacity on its own full lane is retained and Unmount stops at Requests "
              "(fp27_reader_health).",
              "Release after a failed demand, concurrent demand, foreign reader, invariant and poisoned-lock "
              "classifications are not staged. A Capacity refusal fences the whole mount (reported)."),
    "FP-30-Runtime": ("PARTIAL", "First execution (was NOT_RUN): window-edge sizes, the last buffered megabyte "
                      "before a nonzero exit, and a descendant-held pipe (043).",
                      "A fast exit with a delayed consumer is not constructible through the runtime protocol."),
    "FP-31": ("PARTIAL", "Underflow and failure custody on a real kernel FORGET, retained with the kernel's input "
              "and the engine's original cause (fp31_forget_underflow).",
              "Foreign incarnation has no kernel input; no resident-memory measurement."),
    "FP-33": ("PARTIAL", "Nothing added.", "Short, failed and unknown abort writes, refusal after admission and "
              "an independent system-call trace are not staged."),
    "R2-STEP-2": ("PARTIAL", "Lifecycle PASS at the final identity (044); FP-2 flag half.",
                  "FP-2's request maxima and the terminal and startup failure paths."),
    "R2-STEP-3": ("PASS", "Its one retained gap is closed: the full SDK/Sandbox/release-daemon lifecycle PASS at "
                  "the final identity (044), and a changed Commit with a fresh-mount oracle through the real "
                  "daemon executable (r5_1_actual_daemon_commit).", ""),
    "R2-STEP-4": ("PARTIAL", "Nothing added.", "FP-25-Routes scopes."),
    "R2-STEP-5": ("PARTIAL", "FP-21 additions; reversible Busy at the actual topology (039 to 041).",
                  "FP-21's unstaged stages; the private-namespace outcome recorded under FP-22-FS."),
    "R2-STEP-6": ("PARTIAL", "Actual unprivileged deployment protection executed at the final identity (038 PASS "
                  "for every protected route).",
                  "Whether the topology must deny namespace creation, shown by 042 to defeat normal Unmount, is "
                  "PENDING OWNER; kept PARTIAL rather than passed beside that finding."),
    "R2-STEP-7": ("PASS", "Its retained gap named three separate selections; each PASS at the final identity: "
                  "lifecycle (044), huge prepared root on host and Linux (034, 035), full fixture (033).", ""),
    "R3-STEP-6": ("PARTIAL", "Commit while a mapped page is still dirty (r5_9_dirty_mapping).",
                  "WRITE header identity is not observed."),
    "R3-STEP-7": ("PARTIAL", "Lifecycle at the final identity (044).", "FP-15 and FP-16 remain PARTIAL."),
    "R4-2": ("PASS", "Its one retained gap is closed: a second parent and a self-contained cycle written into "
             "the real engine are refused through the product constructor as definite failures "
             "(r4_2_topology_rows). The run gives a definite failure with custody ended, which the test asserts.",
             ""),
    "R4-3": ("PASS", "Its one retained gap, state-table row six, is closed at mounted scope: a Commit runs while "
             "a real mapped page is dirty (r5_9_dirty_mapping), under reading L-6.", ""),
    "R4-6": ("PARTIAL", "Nothing added.", "Sibling-page and Store work grow with base size; a recorded limit. "
             "Flattening needs canonical summaries, which are forbidden proposals."),
    "R4-9": ("PASS", "Its retained gap is closed at the scope the row names: at driver scope an engine refusal of "
             "each of the 13 provider call kinds is definite and an opaque failure of each is uncertain with "
             "custody kept (r4_9_driver_causes).",
             "Not a gap of this row: the same causes at product-constructor scope are not constructible "
             "without a hook."),
    "R5-5": ("PASS", "Both retained gaps are closed: access time, O_DIRECT reads with no kept descriptor and "
             "shared mappings held across two installs, and a pipe-stepped writer captured inside an iteration "
             "at eight positions (r5_5_install_survival).",
             "Scope: the eight declared positions, not every instruction boundary."),
    "R5-6": ("PARTIAL", "Mounted missing dependency from real Store damage, unknown History with nothing "
             "published and install failure after a known publication, by a real file-size limit "
             "(r5_6_failure_matrix).",
             "R5-6a, kept as an ignored reproduction that fails against the product: with an untouched file's "
             "content-root location row deleted externally, a Commit that rewrites the inode leaf succeeds and "
             "publishes a root naming an object the Store cannot locate. Reported, not judged: PENDING OWNER."),
    "R5-7": ("PARTIAL", "A release refused after a settled failure keeps the owner named (r5_6_failure_matrix).",
             "A release error after a settled failure and a release refused before admission have no producer."),
    "R5-9": ("PASS", "Its one retained gap is closed: a Commit runs while the kernel page is still dirty; the "
             "store is outside it and inside the next after msync; a fresh mount is exact (r5_9_dirty_mapping).",
             "Passed under reading L-6 (a dirty page is outside the frontier, per #303 and architecture 79); "
             "the R5-9 handoff sentence read alone could require a flush. Listed for owner review."),
    "R5-10": ("PASS", "All three retained gaps are answered: producers are counted (one per Commit over four "
              "mounted Commits), the writer session's own Disposable/WAL/OFF profile is read back, and it is "
              "recorded as fact that no product source reads LAYERFS_CONSTRUCTION_WORKERS: one producer is "
              "structural (r5_10_producer_profile).", ""),
    "P-1": ("PARTIAL", "The early low-water reservation is implemented with an explicit configuration value "
            "(product fix c127b6d55) and proved with nonzero values at range, record, mounted and "
            "real-executable scope.",
            "No deployed value: the examined runtime and every harness set 0, which makes no early attempt. "
            "The number, the bound below the window (L-3) and the terminal consequence of a non-contention "
            "early failure (L-4) are PENDING OWNER."),
    "R6-7": ("PARTIAL", "Same addition as FP-26.", "Same open scope as FP-26."),
    "R6-8": ("PARTIAL", "Runtime stream delivery exact at the final identity (043).",
             "Backpressure against a stalled sink is not constructible; descendant late bytes PENDING OWNER."),
    "FULL-FIXTURE-ORACLE": ("PASS", "Successor R8b-full-fixture-mounted PASS (033): all 130,046 names and kinds, "
                            "103,108 regular payloads and 3,475,776,149 bytes, metadata, symlinks and alias "
                            "classes; 0 differences; terminal Unmounted, Gone observed, stop acknowledged; "
                            "comparator 57.0 s under the unchanged 85 s stop.",
                            "The original FAIL 053 stands at its own identity. The comparator observes with 8 "
                            "walkers (L-1), listed for owner review. Functional, natural cache; no cold claim."),
}
INVOCATIONS = [
    ("R8b-full-fixture-mounted", "033-full-mounted-proof/proof.json", "033-full-mounted-proof.stderr"),
    ("Q1-wide-host", "034-host-wide-proof/result.json", None),
    ("Q1-wide-linux", "035-linux-wide-proof/result.json", None),
    ("Q1-host-handoff", "036-host-linux-handoff-proof/result.json", None),
    ("Q1-shared-processes-linux", "037-linux-shared-process-proof/result.json", None),
    ("R8b-confinement", "038-confinement/result.json", "038-confinement.stderr"),
    ("R8b-holder-cwd", "039-holder-cwd/result.json", "039-holder-cwd.stderr"),
    ("R8b-holder-descriptor", "040-holder-descriptor/result.json", "040-holder-descriptor.stderr"),
    ("R8b-holder-mapping", "041-holder-mapping/result.json", "041-holder-mapping.stderr"),
    ("R8b-holder-private", "042-holder-private/result.json", "042-holder-private.stderr"),
    ("R8b-streams", "043-streams/result.json", "043-streams.stderr"),
    ("R8b-lifecycle", "044-lifecycle/result.json", "044-lifecycle.stderr"),
]
PREREQ = {"PREREQ-COMPLETE-INSTALLED-HOST": "Q1-wide-host", "PREREQ-COMPLETE-INSTALLED-LINUX": "Q1-wide-linux",
          "PREREQ-HOST-HANDOFF": "Q1-host-handoff", "PREREQ-SHARED-PROCESSES-LINUX": "Q1-shared-processes-linux"}
GATES = {
    "G01": "PENDING OWNER", "G02": "UNMET", "G03": "UNMET", "G04": "UNMET", "G05": "UNMET", "G06": "UNMET",
    "G07": "MET", "G08": "UNMET", "G09": "UNMET"}
GATE_NOTES = {
    "G05": "Registered runtime proofs: confinement, three holders, streams and lifecycle PASS; "
           "R8b-holder-private FAIL (042). Backpressure and lost-result scopes not run.",
    "G07": "R8b-full-fixture-mounted PASS (033) under the unchanged 100 s and 85 s stops.",
}
NEW_OWNER = [
    ("OWNER-8", "May a Sandbox command create a user and mount namespace, and what must a normal Unmount answer "
     "when one holds a copy of the mount?",
     "Decide the topology first (shared parent mount, denying namespace creation in Sandbox setup, or binding "
     "the abort control); 042 shows one such command also holds every sibling Workspace of the daemon."),
    ("OWNER-9", "Which stream-EOF sentence governs bytes a descendant writes after the command has exited?",
     "Today they are dropped silently at command exit; either declare that or require delivery until EOF."),
    ("OWNER-10", "Deployed serial low-water value, the bound below the 1,024 window, and whether a "
     "non-contention early failure should end only that create (L-2, L-3, L-4).",
     "Name a number; until then 0 keeps the previous behaviour and P-1 stays PARTIAL."),
    ("OWNER-11", "Debt headroom value at Mount, and maintenance failure in Status (L-5).",
     "Both are additive; nothing built blocks them. Reading debt at Mount adds an owner job."),
    ("OWNER-12", "Is a seccomp or equivalent system-call fixture for the test's own threads, outside product "
     "source, acceptable for FP-2, FP-15, FP-16, FP-21 Join, FP-23-FS and FP-33?",
     "The alternative is product telemetry that looks like a test hook."),
    ("OWNER-13", "Rows whose remaining stages have no producer or no kernel input at a real mount (FP-21, FP-27, "
     "FP-31, FP-33, R5-7): scope each to what a mount can present, or accept component scope?", "Rescope; the "
     "receipts recorded here can be reused."),
    ("OWNER-14", "Specification 8.1 says two loops where the product runs one; selected INIT flags are computed, "
     "not read back; a Capacity refusal fences the whole mount.", "Correct the specification or the product."),
    ("OWNER-15", "R5-6a: after external deletion of an untouched content root's location row, a Commit publishes "
     "a root naming an unlocatable object.", "Decide whether the Save's dependency check must be offered "
     "content and metadata roots of trusted leaf pages; needs external Store damage to occur."),
    ("OWNER-16", "Recovery catalogue: 1,131 weak-signal rows, 7 pin values (59 rows) that resolve to no local "
     "object, and a recovery commit that is local only.", "Review the classes; push or tag the recovery "
     "commit before any retirement."),
    ("OWNER-17", "Does a generator pinned by blob and recovery commit satisfy 'ignored-only generation' for the "
     "content fixture seal?", "The generator drives the reference's own tree API and cannot be ported without "
     "making the oracle self-referential."),
    ("OWNER-18", "Content/storage harness port: seal placement outside the timer, counters and timer nodes "
     "reported unavailable, lint allowances at frozen include sites, a pre-existing sync_all in the "
     "acquisition copy.", "Review before any registered row of that harness is executed."),
    ("OWNER-19", "Sibling Workspace visibility from a command, the unauthenticated control-listener connect, and "
     "Force being unavailable in the Sandbox topology.", "Declare each or change the topology."),
    ("OWNER-20", "Lead decisions L-1 (8 comparator walkers), L-6 (a dirty mapped page is outside the Commit) and "
     "L-7 (rows needing a ruling stay PARTIAL).", "Confirm or reverse; each reversal is recorded in the plan."),
    ("OWNER-21", "Counts and resources (G08, G09): per-statement correlation in a mounted run, pager and journal "
     "statistics, continuous request maxima and phase peaks have no product observation, and no bound exists.",
     "Product telemetry is not a defect fix; authorize it or rescope the gates."),
]


def sha(path):
    return hashlib.sha256((ROOT / path).read_bytes()).hexdigest()


def suite_binaries():
    found = {}
    for path in SUITES:
        side = "host" if "host" in path else "linux"
        for line in (ROOT / path).read_text().splitlines():
            match = re.match(r"(\S+)-[0-9a-f]{16} exit=(\d+) .*?(\d+) passed; (\d+) failed; (\d+) ignored", line)
            if match:
                found.setdefault(match[1], {})[side] = dict(
                    exit=int(match[2]), passed=int(match[3]), failed=int(match[4]), ignored=int(match[5]))
    return found


def invocations():
    rows = {}
    for identifier, result, stderr in INVOCATIONS:
        record = json.loads((ROOT / CHECKS / result).read_text())
        row = dict(id=identifier, status=record["status"], receipt=CHECKS + "/" + result,
                   receipt_sha256=sha(CHECKS + "/" + result), attempts=1)
        text = (ROOT / CHECKS / stderr).read_text() if stderr else (
            ROOT / CHECKS / result).with_name("stderr.txt").read_text()
        wall = re.search(r"R4_WALL seconds<=(\d+) limit=(\d+)s", text)
        row.update(wall_seconds_at_most=int(wall[1]), limit_seconds=int(wall[2]),
                   wall_note="command custody bound, not a measurement")
        if record.get("original_failure"):
            row["original_failure"] = record["original_failure"]
        rows[identifier] = row
    return rows


def main():
    old = json.loads((ROOT / OLD / "061-outcomes.json").read_text())
    registration = json.loads((ROOT / REGISTRATION).read_text())
    binaries, runs = suite_binaries(), invocations()
    functional = []
    for row in old["functional_rows"]:
        new = dict(id=row["id"], stage=row["stage"], status_061=row["status"], gaps_061=row["known_gaps"])
        tests = [Path(path).stem for path in row["source_paths"] if "/tests/" in path and path.endswith(".rs")]
        covering = {name: binaries.get(name) for name in tests}
        bad = [name for name, sides in covering.items()
               if not sides or any(side["exit"] for side in sides.values())]
        if row["id"] in ROWS:
            status, added, remains = ROWS[row["id"]]
            new.update(status=status, added_in_r8b=added, remains_open=remains,
                       basis="R8b evidence at the final identity against the row's retained gaps")
        elif row["id"] in PREREQ:
            run = runs[PREREQ[row["id"]]]
            new.update(status=run["status"], added_in_r8b="Invoked once at registration v3: " + run["receipt"],
                       remains_open="", basis="registered precondition proof")
            bad = []
        elif row["status"] == "WITHDRAWN":
            new.update(status="WITHDRAWN", added_in_r8b="", remains_open="", basis=row["reason"])
        elif row["status"] == "PASS":
            new.update(status="PASS", added_in_r8b="", remains_open="",
                       basis="same assertions passed in the final suites at the registered source identity")
        else:
            raise SystemExit("no R8b disposition for " + row["id"])
        if bad and new["status"] == "PASS":
            raise SystemExit("PASS row lacks a passing covering binary: %s %s" % (row["id"], bad))
        new["covering_test_binaries"] = covering
        functional.append(new)
    if set(ROWS) - {row["id"] for row in functional}:
        raise SystemExit("disposition names an unknown row")
    reason = registration["timing_zero_attempt_reason"]
    timing = [dict(case_id=row["case_id"], arm=row["arm"], cache_class=row["cache_class"], status="NOT_RUN",
                   attempted_operation_count=0, new_receipt=None, reason_061=row["reason"],
                   reason="Zero attempts authorized by registration v3: gates " + ", ".join(
                       gate for gate, status in GATES.items() if status != "MET") + " are not met.")
              for row in old["timing_rows"]]
    gates = []
    for gate in registration["required_function_count_resource_order"]:
        gates.append(dict(id=gate["id"], requirement=gate["requirement"], status_at_registration=gate["status"],
                          status=GATES[gate["id"]], reason=GATE_NOTES.get(gate["id"], gate["reason"])))
    owner = old["owner_questions"] + [dict(id=identifier, question=question, recommendation=recommendation,
                                           status="PENDING OWNER") for identifier, question, recommendation in NEW_OWNER]
    head = subprocess.run(["git", "-C", str(ROOT), "log", "-1", "--format=%H", "--", REGISTRATION],
                          check=True, capture_output=True, text=True).stdout.strip()
    counts = collections.Counter(row["status"] for row in functional)
    result = dict(
        schema="r8b-integrated-examination-outcomes-v1", status="CLOSED - NOT QUALIFIED",
        meaning="Every selection registered in v3 has an outcome; a closed examination is not product acceptance.",
        registration=dict(path=REGISTRATION, sha256=sha(REGISTRATION), commit=head),
        frozen_source=registration["identities"]["source_commit"],
        frozen_product=registration["identities"]["product_tree"],
        previous_outcomes=dict(path=OLD + "/061-outcomes.json", sha256=sha(OLD + "/061-outcomes.json"),
                               status=old["status"], functional_status_counts=old["functional_status_counts"]),
        Durable=old["Durable"], registered_invocations=list(runs.values()),
        registered_invocation_status_counts=dict(collections.Counter(run["status"] for run in runs.values())),
        gates=gates, timing_zero_attempt_reason=reason, functional_count=len(functional),
        functional_status_counts=dict(sorted(counts.items())), functional_rows=functional,
        timing_count=len(timing), timing_samples=0, timing_status_counts={"NOT_RUN": len(timing)},
        timing_rows=timing, owner_questions=owner)
    (ROOT / CHECKS / "045-outcomes.json").write_text(json.dumps(result, indent=1, sort_keys=True) + "\n")

    lines = ["# R8b outcomes of every registered selection", "",
             "> **Status:** Dated planning checkpoint; not release evidence or a product contract.", "",
             "Generated by [make_outcomes_r8b.py](tools/make_outcomes_r8b.py) from registration v3, the closed",
             "examination's [061 outcomes](../r8-qualification-20261010/061-outcomes.json) and each receipt.",
             "Machine form: [045-outcomes.json](045-outcomes.json). The 061 outcomes are unchanged.", "",
             "## Registered invocations (one attempt each)", "",
             "| Selection | Outcome | Custody bound | Receipt |", "| --- | --- | --- | --- |"]
    for run in runs.values():
        name = run["receipt"].split("/")[-2]
        lines.append("| `%s` | **%s**%s | at most %d s of %d s | [%s](%s) |" % (
            run["id"], run["status"], " (" + run["original_failure"] + ")" if run.get("original_failure") else "",
            run["wall_seconds_at_most"], run["limit_seconds"], name, run["receipt"].split(CHECKS + "/")[1]))
    lines += ["", "The custody bound is the command supervisor's whole-second ceiling, not a measurement.", "",
              "## Gates", "", "| Gate | Requirement | Status | Reason |", "| --- | --- | --- | --- |"]
    for gate in gates:
        lines.append("| %s | %s | **%s** | %s |" % (gate["id"], gate["requirement"], gate["status"], gate["reason"]))
    lines += ["", "## Functional rows", "",
              "061: " + ", ".join("%s %d" % item for item in sorted(old["functional_status_counts"].items()))
              + ". R8b: " + ", ".join("%s %d" % item for item in sorted(counts.items())) + ".", "",
              "Rows whose status or evidence changed:", "",
              "| Row | 061 | R8b | Added | Still open |", "| --- | --- | --- | --- | --- |"]
    for row in functional:
        if row["id"] in ROWS:
            lines.append("| %s | %s | **%s** | %s | %s |" % (
                row["id"], row["status_061"], row["status"], row["added_in_r8b"], row["remains_open"] or "nothing"))
    carried = [row["id"] for row in functional if row["id"] not in ROWS and row["status"] == "PASS"]
    withdrawn = [row["id"] for row in functional if row["status"] == "WITHDRAWN"]
    lines += ["", "PASS in 061 and PASS again, by the same assertions in the final suites at the registered",
              "source identity or by a registered precondition proof (%d rows): %s." % (len(carried), ", ".join(carried)),
              "", "WITHDRAWN, unchanged (%d rows): %s." % (len(withdrawn), ", ".join(withdrawn)), "",
              "## Timing rows", "",
              "All %d timing selections: **NOT_RUN**, zero attempts, zero samples. %s" % (len(timing), reason),
              "Each row keeps its 061 reason in the machine form.", "", "## Owner questions", "",
              "| Id | Question | Recommendation |", "| --- | --- | --- |"]
    for item in owner:
        lines.append("| %s | %s | %s |" % (item["id"], item["question"], item["recommendation"]))
    (ROOT / CHECKS / "046-outcomes.md").write_text("\n".join(lines) + "\n")
    print(dict(counts), dict(collections.Counter(run["status"] for run in runs.values())), len(timing), len(owner))


if __name__ == "__main__":
    main()
