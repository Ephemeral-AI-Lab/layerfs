"""Replay the retained defect probe against the fixed source under the shared lock.

The probe is an unmodified copy of the audit's attempt-2 client. That client
asserts the *defects* it reproduced; on fixed source those assertions must fail.
This runner therefore expects a nonzero exit and records it as the evidence that
the investigated behaviour changed. Not a benchmark and not product acceptance.
"""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = next(parent for parent in HERE.parents if (parent / "core/Cargo.toml").exists())


def run(command, name, expect_success=True):
    with (HERE / f"{name}.stdout").open("x") as out, (HERE / f"{name}.stderr").open("x") as err:
        result = subprocess.run(command, cwd=ROOT, stdout=out, stderr=err, timeout=120)
    (HERE / f"{name}.command.json").write_text(
        json.dumps({"argv": command, "exit": result.returncode}, indent=2)
    )
    if expect_success:
        result.check_returncode()
    return result


lock_path = Path(os.environ.get("TMPDIR", "/tmp")) / "layerfs-infra-measurement.lock"
with lock_path.open("a") as lock:
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(
        ["git", "status", "--porcelain", "--", "core/crates", "core/Cargo.toml", "core/Cargo.lock"],
        cwd=ROOT,
        text=True,
    )
    assert not dirty, dirty
    run(["cargo", "+1.85.1", "build", "--manifest-path", "core/Cargo.toml", "--locked",
         "-p", "layerfs-content", "--lib", "--message-format=json"], "build")
    artifacts = [
        json.loads(line)
        for line in (HERE / "build.stdout").read_text().splitlines()
        if line.startswith("{")
    ]
    library = next(
        Path(file)
        for row in artifacts
        if row.get("reason") == "compiler-artifact"
        and row["target"]["name"] == "layerfs_content"
        for file in row["filenames"]
        if file.endswith(".rlib")
    )
    binary_dir = Path(tempfile.mkdtemp(prefix="layerfs-stage5-ordering-replay-"))
    binary = binary_dir / "probe"
    run(["rustup", "run", "1.85.1", "rustc", "--edition=2021", str(HERE / "probe.rs"),
         "--extern", f"layerfs_content={library}", "-L", f"dependency={library.parent / 'deps'}",
         "-o", str(binary)], "compile")
    probe = run([str(binary), str(binary_dir / "output")], "probe", expect_success=False)
    after = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    assert head == after, "source commit changed"
    identity = {
        "source_commit": head,
        "scope": "replay of the retained defect probe; its defect assertions are expected to fail",
        "probe_exit": probe.returncode,
        "probe_source": "unmodified copy of stage-5-blocker-audit-20260917T055334Z/attempt-2/probe.rs",
        "probe_sha256": hashlib.sha256((HERE / "probe.rs").read_bytes()).hexdigest(),
        "library": str(library),
        "library_sha256": hashlib.sha256(library.read_bytes()).hexdigest(),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    }
    (HERE / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    (HERE / "probe.stdout").read_text()
    print("probe exit", probe.returncode)
    print((HERE / "probe.stdout").read_text())
