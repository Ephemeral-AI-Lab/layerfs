#!/usr/bin/env python3
"""Derive R8 registration v3 from v2 at the final R8b source identity.

Reads registration v2 and the files it will pin, and writes v3. Every hash is
computed here from the file that will be executed; nothing is copied from a
report. Fields that do not change (arms, cache classes, clock, priced
boundaries, the two inventories, verdict vocabulary) are carried verbatim.
Run from the repository root; refuses to overwrite an existing v3.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[7]
REGISTRY = ROOT / "core/benchmark/fs-bench-pro/registry"
CHECKS = "core/docs/issues/307/checks/r8b-requalification-20261010"
OLD = "core/docs/issues/307/checks/r8-qualification-20261010"
TOOLS = "core/benchmark/r8-tools"
SOURCE = "77a9b52dbe1e221954f8a2afbf7e0495a82b615e"
FULL_VOLUME = "layerfs-r8-full-proof-20261010-2aca347ad746"
LIFECYCLE_VOLUME = "layerfs-r8-full-proof-20261010-f1291be40649"
SHARED_VOLUME = "layerfs-r8b-shared-proof-20261010-77a9b52db"
RUNTIME = "core/target/r8b-host-release/release/layerfs-r7-runtime"
DAEMON = "core/target/r8b-linux-release/release/layerfs-daemon"


def sha(path):
    digest = hashlib.sha256()
    with open(ROOT / path, "rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def git(*arguments):
    return subprocess.run(["git", "-C", str(ROOT), *arguments], check=True,
                          capture_output=True, text=True).stdout.strip()


def tool(path):
    return dict(path=str(ROOT / path), sha256=sha(path))


def proof_binary(side, name, receipt):
    host = ("core/target/debug/deps/" if side == "host" else "core/target/cluster2-linux/debug/deps/") + name
    executable = str(ROOT / host) if side == "host" else "/work/" + host
    return dict(build_receipt=receipt, executable=executable, host_path=str(ROOT / host), sha256=sha(host))


def runtime_proof(identifier, controller, rows, inputs, volume, stop, judged, recorded, extra=None, helpers=()):
    row = dict(id=identifier, controller=tool(TOOLS + "/" + controller),
               helpers=[tool(TOOLS + "/" + helper) for helper in helpers],
               original_rows=rows, inputs=inputs, inputs_sha256=sha(inputs), volume=volume,
               overall_stop_seconds=stop, expected_class="functional-natural", performance_samples=0,
               operation_attempts=1, judged=judged, recorded_not_judged=recorded)
    row.update(extra or {})
    return row


def gate(old, status, reason, evidence):
    return dict(id=old["id"], requirement=old["requirement"], scope=old["scope"], source=old["source"],
                status_at_v2=old["status"], status=status, reason=reason, evidence=evidence)


def main():
    target = REGISTRY / "r8-integrated-qualification-v3.json"
    if target.exists():
        raise SystemExit("refusing to overwrite " + str(target))
    if git("rev-parse", "HEAD") != SOURCE:
        raise SystemExit("HEAD differs from the registered source commit")
    if git("status", "--porcelain", "--untracked-files=no"):
        raise SystemExit("tracked files differ from the registered source commit")
    v2_path = "core/benchmark/fs-bench-pro/registry/r8-integrated-qualification-v2.json"
    old = json.loads((ROOT / v2_path).read_text())
    new = {key: old[key] for key in (
        "arm_order", "attempt_policy", "cache_classes", "clock", "construction_workers", "date",
        "docker_endpoint", "durable", "family", "functional_inventory", "global_store_profile",
        "one_sample_spread", "overlay_profile", "pending_owner_inherited", "priced_boundaries",
        "timing_inventory", "verdicts", "wide_preparation")}
    new["schema"] = "r8-integrated-qualification-registration-v3"
    new["status"] = "PROSPECTIVE"
    new["run"] = "R8b requalification; R8 examination of 2026-10-10 stays CLOSED - NOT QUALIFIED"
    for name, entry in old["wide_preparation"]["files"].items():
        if sha(Path(old["wide_preparation"]["path"]) / name) != entry["sha256"]:
            raise SystemExit("wide preparation differs: " + name)

    identities = dict(old["identities"])
    identities.update(
        source_commit=SOURCE, source_tree=git("rev-parse", SOURCE + "^{tree}"),
        product_tree=git("rev-parse", SOURCE + ":core/crates"),
        harness_tree=git("rev-parse", SOURCE + ":core/benchmark"),
        fuser_tree=git("rev-parse", SOURCE + ":core/vendor/fuser-0.18.0"),
        cache="functional: natural setup/cache warmth; no cold or timing qualification; "
              "timing: class contracts attached, zero attempts while the prerequisite gates are unmet")
    identities["compilation"] = dict(
        old["identities"]["compilation"], cargo_config_sha256=sha(".cargo/config.toml"),
        source_manifest=git("rev-parse", SOURCE + ":core/Cargo.toml"),
        proof_build_receipts=[CHECKS + "/021-final-host-suite.txt", CHECKS + "/025-final-linux-suite.txt"],
        release_build_receipts=[CHECKS + "/027-runtime-release-build/output.txt",
                                CHECKS + "/028-daemon-release-build/output.txt"],
        linux_source_check=CHECKS + "/024-linux-source-check.txt",
        linux_source_hashes=dict(path=CHECKS + "/024-linux-source.sha256",
                                 sha256=sha(CHECKS + "/024-linux-source.sha256")))
    identities["dependency"] = {path: sha(path) for path in old["identities"]["dependency"]}
    if identities["dependency"] != old["identities"]["dependency"]:
        raise SystemExit("a dependency lock or provenance record changed")
    identities["canonical_registry_sha256"] = sha("core/benchmark/fs-bench-pro/registry/r7-optimization-v1.json")
    new["identities"] = identities

    inputs = CHECKS + "/029-full-proof-prepare/inputs.json"
    lifecycle_inputs = CHECKS + "/030-lifecycle-prepare/inputs.json"
    full = dict(old["full_fixture_proof"])
    for key in ("preparation_defect_fix",):
        full.pop(key)
    full.update(
        selection_id="R8b-full-fixture-mounted", successor_of="R8-full-fixture-mounted",
        binaries_and_tools=dict(comparator=tool(TOOLS + "/full_oracle.py"),
                                controller=tool(TOOLS + "/run_full_oracle.py"),
                                daemon=tool(DAEMON), runtime=tool(RUNTIME)),
        shared_helpers=dict(lifecycle=tool("core/benchmark/r7-tools/lifecycle_proof.py"),
                            runner=tool("core/benchmark/fs-bench-pro/r7/runner.py")),
        inputs=str(ROOT / inputs), inputs_sha256=sha(inputs), volume=FULL_VOLUME,
        comparator_walkers=8,
        comparator_change="The comparator observes with 8 bounded walker threads and compares in one thread "
                          "(commit 4a920a3ee, diagnosis 006). Names, kinds, every payload byte, metadata, "
                          "symlinks, alias classes, inventory, limits and both stops are unchanged. Lead "
                          "decision L-1, listed for owner review.",
        preparation="Fresh independent streamed byte copy 029 of the sealed Store at 0600/root; receipt records "
                    "the sealed digest before startup. No earlier volume, container or sample is reused.",
        output=CHECKS + "/033-full-mounted-proof")
    new["full_fixture_proof"] = full

    receipts = dict(host=CHECKS + "/021-final-host-suite.txt", linux=CHECKS + "/025-final-linux-suite.txt")
    new["proof_binaries"] = {
        "host:complete_installed_roots": proof_binary("host", "complete_installed_roots-e76bd78a0eded4e2", receipts["host"]),
        "host:host_handoff": proof_binary("host", "host_handoff-a4c4b3d19848e0be", receipts["host"]),
        "linux:complete_installed_roots": proof_binary("linux", "complete_installed_roots-b8a4f28c1707b846", receipts["linux"]),
        "linux:host_handoff": proof_binary("linux", "host_handoff-f5838c7100750c80", receipts["linux"]),
        "linux:shared_processes": proof_binary("linux", "shared_processes-498c7928b7fbb8cc", receipts["linux"]),
    }
    proofs = json.loads(json.dumps(old["precondition_proofs"]))
    outputs = dict(zip(("Q1-wide-host", "Q1-wide-linux", "Q1-host-handoff", "Q1-shared-processes-linux"),
                       ("034-host-wide-proof", "035-linux-wide-proof", "036-host-linux-handoff-proof",
                        "037-linux-shared-process-proof")))
    for proof in proofs:
        proof["output"] = CHECKS + "/" + outputs[proof["id"]]
        proof["successor_of"] = proof["id"] + " (v2)"
        if proof["id"] == "Q1-shared-processes-linux":
            proof["volume"] = SHARED_VOLUME
    new["precondition_proofs"] = proofs
    new["precondition_reason"] = (
        "These four environment-gated tests are the only failures of the final suites 021 and 025, where their "
        "variables are absent by design. Each is invoked once here with its declared input.")

    new["runtime_proofs"] = [
        runtime_proof(
            "R8b-confinement", "run_confinement.py", ["FP-5-Runtime", "R2-STEP-6"], inputs, FULL_VOLUME, 60,
            "no protected cell allowed; no inherited descriptor; declared identity and capability sets; both "
            "Workspaces and Status answer afterwards",
            "unprivileged namespace creation, sibling visibility, unauthenticated control-listener connect, "
            "name metadata", dict(output=CHECKS + "/038-confinement"), helpers=("confinement_probe.py",)),
    ] + [
        runtime_proof(
            "R8b-holder-" + arrangement, "run_mount_holders.py", ["FP-22-FS", "R6-8", "R2-STEP-5"], inputs,
            FULL_VOLUME, 45,
            "sibling reaches Unmounted and Gone while the first is held; held Unmount is the reversible Busy at "
            "unmount:kernel with the mount present, or a complete Unmounted; retained teardown custody is FAIL",
            "the holder's own reference kind and release", dict(arrangement=arrangement,
                                                                output=CHECKS + "/" + output),
            helpers=("mount_holder.py",))
        for arrangement, output in (("cwd", "039-holder-cwd"), ("descriptor", "040-holder-descriptor"),
                                    ("mapping", "041-holder-mapping"), ("private", "042-holder-private"))
    ] + [
        runtime_proof(
            "R8b-streams", "run_streams.py", ["FP-6-Runtime", "FP-7-Runtime", "FP-30-Runtime"], inputs,
            FULL_VOLUME, 60,
            "length, SHA-256 and delivered counts of both streams at every declared size including 64 MiB each; "
            "complete bytes and actual status of a command exiting 7; own bytes and status beside a descendant",
            "whether a descendant's line written after the command's exit arrives",
            dict(output=CHECKS + "/043-streams",
                 not_constructible="stalled or failing sink, delayed consumer, connection cut between frames"),
            helpers=("stream_source.py",)),
        runtime_proof(
            "R8b-lifecycle", "run_lifecycle.py", ["FP-1", "R2-STEP-2", "R2-STEP-3", "R2-STEP-7"],
            lifecycle_inputs, LIFECYCLE_VOLUME, 60,
            "typed outcomes and scoped oracle equality across mount, external mutation, Commit, terminal "
            "unmount and fresh mount; per-Workspace threads exist exactly while mounted; kernel mount row",
            "phase-boundary daemon RSS, cgroup memory and backing-file bytes (not continuous peaks)",
            dict(output=CHECKS + "/044-lifecycle",
                 volume_note="the Commit changes the Store, so this selection has its own prepared volume")),
    ]
    new["invocation_order"] = (["R8b-full-fixture-mounted"] + [proof["id"] for proof in proofs]
                               + [proof["id"] for proof in new["runtime_proofs"]])

    by = {row["id"]: row for row in old["required_function_count_resource_order"]}
    suites = [receipts["host"], receipts["linux"]]
    new["required_function_count_resource_order"] = [
        gate(by["G01"], "PENDING OWNER",
             "The early low-water reservation is implemented (c127b6d55) and proved with nonzero values at "
             "range, record, mounted and real-executable scope. No deployed value exists: every harness and "
             "the examined runtime set 0, which makes no early attempt. The owner names the number.",
             suites + ["layerfs-daemon/tests/mounted_low_water.rs", "layerfs-workspace/tests/namespace.rs"]),
        gate(by["G02"], "UNMET",
             "Mount is refused Capacity/mount:debt while maintenance is stopped (ea812d3f8), proved for a "
             "quarantined engine. Maintenance stopped on an otherwise usable engine is not staged; a declared "
             "debt headroom and maintenance failure in Status need owner decisions. FP-26 stays PARTIAL.",
             suites + ["layerfs-daemon/tests/mount_debt.rs"]),
        gate(by["G03"], "UNMET",
             "Flag half proved: Ready receipt, twelve forbidden flags, mount options and the connection's own "
             "fusectl values agree. Separate READ/WRITE request maxima need an observation of the test's own "
             "system calls that does not exist outside product telemetry; PENDING OWNER on the fixture.",
             suites + ["layerfs-daemon/tests/fp2_negotiation_flags.rs"]),
        gate(by["G04"], "UNMET",
             "A still-dirty mapped store is proved outside the Commit before msync and inside the next. Exact "
             "flagged WRITE header identity and the held post-sampling GETATTR remain without an observation "
             "source; same fixture question as G03.",
             suites + ["layerfs-daemon/tests/r5_9_dirty_mapping.rs"]),
        gate(by["G05"], "UNMET",
             "Decided by the registered runtime proofs. Known before invocation from exploratory receipts "
             "010 to 018: a holder in a private mount namespace made normal Unmount end in retained teardown "
             "custody, which this registration judges FAIL; a stalled sink, delayed consumer and cut "
             "connection are not constructible through the runtime protocol; lost original result at the "
             "actual topology (FP-25-Routes) has no new selection.",
             [CHECKS + "/013-runtime-confinement-and-holders.md", CHECKS + "/017-runtime-streams.md"]),
        gate(by["G06"], "UNMET",
             "Mounted missing dependency, unknown History without publication, install failure after known "
             "publication, refused release after a settled failure, dirty pages and the one-producer profile "
             "are proved. Open: release refused before admission and provider causes at product-constructor "
             "scope are not constructible without a hook; one ignored reproduction (R5-6a) is reported.",
             suites + ["layerfs-daemon/tests/r5_6_failure_matrix.rs", "layerfs-daemon/tests/r5_10_producer_profile.rs"]),
        gate(by["G07"], "UNMET",
             "Decided by the one registered full-byte invocation under the unchanged 100 s and 85 s stops.",
             [CHECKS + "/006-full-oracle-timeout-diagnosis.md"]),
        gate(by["G08"], "UNMET",
             "No selection in this registration asserts the H-1 to H-19 count rows at the final identity. "
             "Per-statement correlation in a mounted run, pager and journal statistics and continuous request "
             "maxima have no product observation; adding one is not a defect fix.",
             [CHECKS + "/01-deepest-file-plan.md"]),
        gate(by["G09"], "UNMET",
             "The lifecycle selection records phase-boundary samples only and judges none against a bound; no "
             "numerical threshold exists and none is invented. Continuous phase peaks and internal cache "
             "domains have no observation.",
             [CHECKS + "/018-lifecycle-exploratory"]),
    ]
    new["timing_attempts_authorized"] = 0
    new["timing_zero_attempt_reason"] = (
        "Gates G01 to G06, G08 and G09 cannot be closed by any invocation registered here, whatever the "
        "registered proofs return; the required deterministic function, count and resource scope is therefore "
        "unmet and all 282 timing selections are NOT_RUN with zero attempts. No R7 exploratory receipt is "
        "promoted. P execution is additionally PENDING OWNER at the unchanged provenance refusal.")
    new["control_binary_identity"] = old["control_binary_identity"]
    new["covering_receipts"] = dict(
        receipts, host_clippy=CHECKS + "/022-final-host-clippy.txt", linux_clippy=CHECKS + "/026-final-linux-clippy.txt",
        static=CHECKS + "/020-final-host-static.txt", tooling=CHECKS + "/023-final-tooling-tests.txt",
        note="Run once at this source identity before this registration was written; cited, not re-invoked. "
             "Their only failures are the four precondition proofs registered above.")
    new["harness_prerequisite"] = dict(
        content_storage_harness="Ported and building at 77a9b52db (16 test binaries, 146 tests); no registered "
                                "row executed; new harness identity with no continuity to the old byte gates",
        retired_host_mediated_families=old["harness_prerequisite"]["retired_host_mediated_families"])
    new["source_change"] = (
        "Product: two demonstrated defect fixes (c127b6d55 low-water reservation, ea812d3f8 mount:debt "
        "refusal) and comment corrections (f0e1e2d84); core/crates tree 9a760772 -> " + identities["product_tree"][:8]
        + ". Harness: concurrent comparator, four new runtime controllers, content/storage harness port. "
          "External tests added for T-A to T-J.")
    new["previous_registration"] = dict(
        path=v2_path, sha256=sha(v2_path), commit="64609e47b",
        failure_receipt=OLD + "/053-full-mounted-proof-v2/proof.json",
        original_outcome="FAIL 053: comparator stopped at 85 s; not relabelled. Examination closed NOT QUALIFIED "
                         "in 496bb5643; its 061/062 outcomes stay as written.",
        earlier=old["previous_registration"])
    new["prior_outcomes"] = dict(path=OLD + "/061-outcomes.json", sha256=sha(OLD + "/061-outcomes.json"))
    new["pending_owner"] = old["pending_owner"] + [
        "Deployed serial low-water value (G01).",
        "Debt headroom value and maintenance failure in Status (G02).",
        "A system-call fixture for the test's own threads, outside product source (G03, G04).",
        "Whether Sandbox commands may create mount namespaces, and the Unmount outcome when one holds the mount (G05).",
        "Which stream-EOF sentence governs a descendant's late bytes (G05).",
    ]
    new["retained_resources"] = dict(
        untouched=["layerfs-r8-full-proof-20261010-cd9345cf061e", "layerfs-r8-full-proof-20261010-8d7d083aaa8d"],
        owned_this_run=[FULL_VOLUME, LIFECYCLE_VOLUME, SHARED_VOLUME, "layerfs-r8-full-proof-20261010-2cd69f68db34"])
    target.write_text(json.dumps(new, indent=1, sort_keys=True) + "\n")
    print(target.relative_to(ROOT), sha(target.relative_to(ROOT)), len(new), "keys")


if __name__ == "__main__":
    sys.exit(main())
