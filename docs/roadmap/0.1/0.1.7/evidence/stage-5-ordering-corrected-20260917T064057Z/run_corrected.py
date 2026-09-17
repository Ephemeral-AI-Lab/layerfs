"""Corrected replay: the same public-API client with the required assertions.

Runs against the committed fixed source under the shared lock. The corrected
client passes only when ordering bytes are owned and reported, a refused cleanup
fails the operation after exactly one attempt with nothing left owned and no
published root, and real ordering work reaches the returned counters.
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


def run(command, name):
    with (HERE / f"{name}.stdout").open("x") as out, (HERE / f"{name}.stderr").open("x") as err:
        result = subprocess.run(command, cwd=ROOT, stdout=out, stderr=err, timeout=120)
    (HERE / f"{name}.command.json").write_text(
        json.dumps({"argv": command, "exit": result.returncode}, indent=2)
    )
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
    binary_dir = Path(tempfile.mkdtemp(prefix="layerfs-stage5-corrected-replay-"))
    binary = binary_dir / "probe_corrected"
    run(["rustup", "run", "1.85.1", "rustc", "--edition=2021",
         str(HERE / "probe_corrected.rs"), "--extern", f"layerfs_content={library}",
         "-L", f"dependency={library.parent / 'deps'}", "-o", str(binary)], "compile_corrected")
    run([str(binary), str(binary_dir / "output")], "probe_corrected")
    after = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    assert head == after, "source commit changed"
    identity = {
        "source_commit": head,
        "scope": "corrected replay of the retained defect probe; required behaviour",
        "probe_source": "copy of the audit client with its three defect assertions replaced by the required ones",
        "probe_sha256": hashlib.sha256((HERE / "probe_corrected.rs").read_bytes()).hexdigest(),
        "library": str(library),
        "library_sha256": hashlib.sha256(library.read_bytes()).hexdigest(),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    }
    (HERE / "identity_corrected.json").write_text(json.dumps(identity, indent=2) + "\n")
    print((HERE / "probe_corrected.stdout").read_text())
