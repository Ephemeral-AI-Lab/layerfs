#!/usr/bin/env python3
"""Derive R8 registration v4 from v3 at the R8c source identity.

Same twelve selections as v3 with unchanged criteria, stops and oracles; every
hash is recomputed from the file that will be executed. What changes is the
source (three fixes), the binaries, the prepared volumes, the confinement
controller (namespace creation is now judged) and the gate dispositions, which
follow owner decisions C-1 to C-17. Refuses to overwrite an existing v4.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[7]
REGISTRY = "core/benchmark/fs-bench-pro/registry/"
CHECKS = "core/docs/issues/307/checks/r8c-close-20261010"
OLD = "core/docs/issues/307/checks/r8b-requalification-20261010"
TOOLS = "core/benchmark/r8-tools/"
SOURCE = "307618496aadd3900d412b9d5ea9eab4267293da"
SHARED_VOLUME = "layerfs-r8c-shared-proof-20261010-307618496"
OUTPUTS = {
    "R8b-full-fixture-mounted": "30-full-mounted-proof", "Q1-wide-host": "31-host-wide-proof",
    "Q1-wide-linux": "32-linux-wide-proof", "Q1-host-handoff": "33-host-linux-handoff-proof",
    "Q1-shared-processes-linux": "34-linux-shared-process-proof", "R8b-confinement": "35-confinement",
    "R8b-holder-cwd": "36-holder-cwd", "R8b-holder-descriptor": "37-holder-descriptor",
    "R8b-holder-mapping": "38-holder-mapping", "R8b-holder-private": "39-holder-private",
    "R8b-streams": "40-streams", "R8b-lifecycle": "41-lifecycle"}
GATES = {
    "G01": ("MET", "C-4: the examined runtime deploys serial_low_water 256; the mechanism is proved by "
            "mounted_low_water, namespace and observed_application in the final suites."),
    "G02": ("MET AS RESCOPED", "C-5: debt admission is the mount:debt refusal while maintenance is stopped, "
            "proved for a quarantined engine (mount_debt). No headroom number; Status unchanged. A stop on an "
            "otherwise usable engine is a withdrawn scope."),
    "G03": ("MET", "C-2: largest READ and WRITE frames are both 131072 by the request service's own accounting "
            "(fp2_negotiation_flags, receipt c2-attempt2); flags, options and fusectl values as before. "
            "Product observation, not an independent trace."),
    "G04": ("MET AS RESCOPED", "C-3: WRITE header identity and a GETATTR held across a WRITE are withdrawn "
            "scopes. Flagged-WRITE counts, handle and RELEASE order, the dirty-page Commit and the concurrent "
            "append and read proof remain and pass."),
    "G05": ("UNMET", "Decided by the registered runtime proofs, the private-namespace holder among them, now "
            "that the Sandbox denies user namespaces (C-1). C-6 rules the stream end; C-7 withdraws the scopes "
            "the runtime protocol cannot construct."),
    "G06": ("MET AS RESCOPED", "C-8 withdraws the stages with no producer; C-10 rules reproduction R5-6a "
            "outside the Disposable profile's guarantees. The staged matrix passes in the final suites."),
    "G07": ("UNMET", "Decided by the one registered full-byte invocation at this identity."),
    "G08": ("MET AS RESCOPED", "C-13: the assertions existing tests make on H-1 to H-19, mapped in "
            "22-count-hypotheses-map.md and passing in the final suites. Six rows are asserted at a real "
            "mount, ten in part; H-13 and H-14 have none. Nothing beyond the assertions is claimed."),
    "G09": ("UNMET", "C-14: decided by the lifecycle selection's phase-boundary report of each observable "
            "domain. No numerical bound exists; unobserved domains are named."),
}


def sha(path):
    digest = hashlib.sha256()
    with open(ROOT / path, "rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def git(*arguments):
    return subprocess.run(["git", "-C", str(ROOT), *arguments], check=True,
                          capture_output=True, text=True).stdout.strip()


def retool(entry):
    path = Path(entry["path"]).relative_to(ROOT)
    return dict(path=entry["path"], sha256=sha(path))


def main():
    target = ROOT / REGISTRY / "r8-integrated-qualification-v4.json"
    if target.exists():
        raise SystemExit("refusing to overwrite " + str(target))
    if git("rev-parse", "HEAD") != SOURCE or git("status", "--porcelain", "--untracked-files=no"):
        raise SystemExit("tracked tree differs from the registered source commit")
    v3 = REGISTRY + "r8-integrated-qualification-v3.json"
    new = json.loads((ROOT / v3).read_text())
    new.update(schema="r8-integrated-qualification-registration-v4", status="PROSPECTIVE",
               run="R8c close under the owner instruction of 2026-10-10; R8b stays CLOSED - NOT QUALIFIED",
               owner_instruction=dict(text="fix all of them with simplicity and i want to close r8 and r9 fast",
                                      decisions=CHECKS + "/00-owner-instruction-and-decisions.md",
                                      decisions_sha256=sha(CHECKS + "/00-owner-instruction-and-decisions.md")))
    for name, entry in new["wide_preparation"]["files"].items():
        if sha(Path(new["wide_preparation"]["path"]) / name) != entry["sha256"]:
            raise SystemExit("wide preparation differs: " + name)

    identities = new["identities"]
    identities.update(source_commit=SOURCE, source_tree=git("rev-parse", SOURCE + "^{tree}"),
                      product_tree=git("rev-parse", SOURCE + ":core/crates"),
                      harness_tree=git("rev-parse", SOURCE + ":core/benchmark"),
                      fuser_tree=git("rev-parse", SOURCE + ":core/vendor/fuser-0.18.0"))
    dependency = {path: sha(path) for path in identities["dependency"]}
    if dependency != identities["dependency"]:
        raise SystemExit("a dependency lock or provenance record changed")
    identities["compilation"].update(
        proof_build_receipts=[CHECKS + "/11-final-host-suite.txt", CHECKS + "/15-final-linux-suite.txt"],
        release_build_receipts=[CHECKS + "/17-runtime-release-build/output.txt",
                                CHECKS + "/18-daemon-release-build/output.txt"],
        linux_source_check=CHECKS + "/14-linux-source-check.txt",
        linux_source_hashes=dict(path=CHECKS + "/14-linux-source.sha256",
                                 sha256=sha(CHECKS + "/14-linux-source.sha256")))

    inputs = CHECKS + "/19-full-proof-prepare/inputs.json"
    lifecycle_inputs = CHECKS + "/20-lifecycle-prepare/inputs.json"
    volume = json.loads((ROOT / inputs).read_text())["volume"]
    lifecycle_volume = json.loads((ROOT / lifecycle_inputs).read_text())["volume"]
    full = new["full_fixture_proof"]
    full["binaries_and_tools"] = {name: retool(entry) for name, entry in full["binaries_and_tools"].items()}
    full["shared_helpers"] = {name: retool(entry) for name, entry in full["shared_helpers"].items()}
    full.update(inputs=str(ROOT / inputs), inputs_sha256=sha(inputs), volume=volume,
                output=CHECKS + "/" + OUTPUTS[full["selection_id"]],
                preparation="Fresh independent streamed byte copy 19 of the sealed Store at 0600/root. "
                            "No earlier volume, container or sample is reused.",
                successor_of="R8b-full-fixture-mounted at registration v3 (PASS 033, kept)")

    for name, entry in new["proof_binaries"].items():
        path = Path(entry["host_path"]).relative_to(ROOT)
        entry.update(sha256=sha(path), build_receipt=CHECKS + (
            "/11-final-host-suite.txt" if name.startswith("host:") else "/15-final-linux-suite.txt"))
    for proof in new["precondition_proofs"]:
        proof.update(output=CHECKS + "/" + OUTPUTS[proof["id"]], successor_of=proof["id"] + " (v3)")
        if proof["id"] == "Q1-shared-processes-linux":
            proof["volume"] = SHARED_VOLUME
    for proof in new["runtime_proofs"]:
        lifecycle = proof["id"] == "R8b-lifecycle"
        used = lifecycle_inputs if lifecycle else inputs
        proof.update(controller=retool(proof["controller"]), helpers=[retool(entry) for entry in proof["helpers"]],
                     inputs=used, inputs_sha256=sha(used), volume=lifecycle_volume if lifecycle else volume,
                     output=CHECKS + "/" + OUTPUTS[proof["id"]])
        if proof["id"] == "R8b-confinement":
            proof.update(judged=proof["judged"] + "; the command cannot create a user and mount namespace",
                         recorded_not_judged="sibling visibility and the control-listener connect, declared as "
                                             "they are by C-11; name metadata")
        if proof["id"] == "R8b-streams":
            proof["recorded_not_judged"] = ("whether a descendant's late line arrives; by C-6 the stream ends "
                                            "at command exit, so its absence is the declared behaviour")
        if lifecycle:
            proof["resource_report"] = ("C-14: daemon resident set and threads, cgroup memory, Store and Overlay "
                                        "backing bytes at six phase boundaries; sampling limit stated; no bound")

    gates = []
    for gate in new["required_function_count_resource_order"]:
        status, reason = GATES[gate["id"]]
        gates.append(dict(id=gate["id"], requirement=gate["requirement"], scope=gate["scope"],
                          source=gate["source"], status_at_v3=gate["status"], status=status, reason=reason))
    new["required_function_count_resource_order"] = gates
    new["timing_attempts_authorized"] = 0
    new["timing_zero_attempt_reason"] = (
        "C-15: no acceptance threshold exists (OWNER-2), the P arm is unauthorized (OWNER-3) and A2 supplies no "
        "arm (OWNER-1), so no timing selection could be judged. All 282 stay NOT_RUN with zero attempts; R8 "
        "closes on function, counts and resources.")
    new["covering_receipts"] = dict(
        host=CHECKS + "/11-final-host-suite.txt", linux=CHECKS + "/15-final-linux-suite.txt",
        host_clippy=CHECKS + "/12-final-host-clippy.txt", linux_clippy=CHECKS + "/16-final-linux-clippy.txt",
        static=CHECKS + "/10-final-host-static.txt", tooling=CHECKS + "/13-final-tooling-tests.txt",
        count_map=CHECKS + "/22-count-hypotheses-map.md",
        note="Run once at this source identity before this registration was written; cited, not re-invoked.")
    new["source_change"] = (
        "Product since v3: user-namespace denial in the Sandbox container (C-1), largest READ and WRITE in the "
        "request accounting (C-2); core/crates tree 2e0d94c8 -> " + identities["product_tree"][:8]
        + ". Runtime harness deploys serial_low_water 256 (C-4). Confinement controller judges namespace "
          "creation. One external test added for the request maxima.")
    new["previous_registration"] = dict(
        path=v3, sha256=sha(v3), commit="b22fa088f",
        failure_receipt=OLD + "/042-holder-private/result.json",
        original_outcome="R8b closed NOT QUALIFIED in 3a410b588: 11 PASS, FAIL 042; not relabelled.",
        earlier=new["previous_registration"])
    new["prior_outcomes"] = dict(path=OLD + "/045-outcomes.json", sha256=sha(OLD + "/045-outcomes.json"))
    new["pending_owner"] = ["OWNER-1 to OWNER-7 stay PENDING OWNER; none blocks a functional row.",
                            "OWNER-8 to OWNER-21 are answered by decisions C-1 to C-17 under the owner "
                            "instruction; each records its alternative."]
    new["retained_resources"] = dict(
        untouched=["layerfs-r8-full-proof-20261010-cd9345cf061e", "layerfs-r8-full-proof-20261010-8d7d083aaa8d",
                   "layerfs-r8-full-proof-20261010-2cd69f68db34", "layerfs-r8-full-proof-20261010-2aca347ad746"],
        owned_this_run=[volume, lifecycle_volume, SHARED_VOLUME])
    target.write_text(json.dumps(new, indent=1, sort_keys=True) + "\n")
    print(target.relative_to(ROOT), sha(target.relative_to(ROOT)), volume, lifecycle_volume)


if __name__ == "__main__":
    sys.exit(main())
