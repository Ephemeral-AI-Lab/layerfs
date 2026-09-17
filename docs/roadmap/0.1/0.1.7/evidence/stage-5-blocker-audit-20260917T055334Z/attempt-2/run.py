"""Build exact core library and run a small diagnostic under the shared lock.

Not a benchmark; asserts the observed defects, not product acceptance.
Run once into a fresh evidence directory. Existing output is never overwritten.
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
        result = subprocess.run(command, cwd=ROOT, stdout=out, stderr=err, timeout=60)
    (HERE / f"{name}.command.json").write_text(json.dumps({"argv": command, "exit": result.returncode}, indent=2))
    result.check_returncode()


lock_path = Path(os.environ.get("TMPDIR", "/tmp")) / "layerfs-infra-measurement.lock"
with lock_path.open("a") as lock:
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    before = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    run(["cargo", "+1.85.1", "build", "--manifest-path", "core/Cargo.toml", "--locked", "-p", "layerfs-content", "--lib", "--message-format=json"], "build")
    artifacts = [json.loads(line) for line in (HERE / "build.stdout").read_text().splitlines() if line.startswith("{")]
    library = next(Path(file) for row in artifacts if row.get("reason") == "compiler-artifact"
                   and row["target"]["name"] == "layerfs_content" for file in row["filenames"] if file.endswith(".rlib"))
    binary_dir = Path(tempfile.mkdtemp(prefix="layerfs-stage5-blocker-probe-"))
    binary = binary_dir / "probe"
    run(["rustup", "run", "1.85.1", "rustc", "--edition=2021", str(HERE / "probe.rs"),
         "--extern", f"layerfs_content={library}", "-L", f"dependency={library.parent / 'deps'}", "-o", str(binary)], "compile")
    run([str(binary), str(binary_dir / "output")], "probe")
    after = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    assert before == after, "source commit changed"
    dirty_product = subprocess.check_output(["git", "status", "--porcelain", "--", "core/crates", "core/Cargo.toml", "core/Cargo.lock"], cwd=ROOT, text=True)
    assert not dirty_product, dirty_product
    identity = {"source_commit": before, "scope": "public-API correctness diagnostic; no performance claim",
                "library": str(library), "library_sha256": hashlib.sha256(library.read_bytes()).hexdigest(),
                "binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}
    (HERE / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    manifest = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(HERE.iterdir()) if p.is_file()}
    (HERE / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print((HERE / "probe.stdout").read_text())
