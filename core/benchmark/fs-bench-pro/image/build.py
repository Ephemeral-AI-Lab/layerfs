#!/usr/bin/env python3
"""Build the sealed release image one #232 edit sample runs inside.

The image carries the release Linux daemon, the benchmark-only POSIX edit tool
and the frozen replacement payloads. Every input is sealed by a digest and the
resulting image ID is recorded; the base image is pinned by digest so a rebuild
cannot silently move it.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
ROOT = BENCH.parents[2]
CORE = ROOT / "core"
sys.path.insert(0, str(BENCH))
from shared import edit_contract as contract  # noqa: E402
from families import (edit_canonical_chunk_count as canonical,  # noqa: E402
                      edit_length_changing as changing,
                      edit_length_preserving as preserving)

BASE = "alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8"
TARGET = "aarch64-unknown-linux-musl"
TAG = "layerfs-exec-fuse-edit:issue232"
CONTEXT = CORE / "target/exec-fuse-edit/image"
SHA = lambda path: hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(command, **kwargs):
    result = subprocess.run(command, capture_output=True, text=True, **kwargs)
    if result.returncode != 0:
        raise SystemExit(f"{' '.join(map(str, command))}\n{result.stdout}\n{result.stderr}")
    return result.stdout


def payload_files():
    """Writes one replacement payload per registered operation, from the recipe."""
    directory = CONTEXT / "payloads"
    directory.mkdir(parents=True, exist_ok=True)
    written = {}
    for family in (preserving, changing, canonical):
        for operation in family.OPERATIONS:
            length = operation["replacement_len"]
            if not length:
                continue
            data = contract.payload_bytes(operation["payload_seed"], length,
                                          operation["replacement_kind"])
            if hashlib.sha256(data).hexdigest() != operation["payload_sha256"]:
                raise SystemExit(f"payload recipe mismatch: {operation['key']}")
            path = directory / f"{operation['key']}.bin"
            path.write_bytes(data)
            written[path.name] = {"bytes": length, "sha256": SHA(path)}
    return written


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default=str(CORE / "target/exec-fuse-edit/image.json"))
    parser.add_argument("--debug-daemon", action="store_true")
    arguments = parser.parse_args()
    profile = ["--release"]
    profile_dir = "release"
    run(["cargo", "+1.85.1", "zigbuild", "--manifest-path", "core/Cargo.toml", "--locked",
         "--offline", "--target", TARGET, "-p", "layerfs-daemon", *profile], cwd=ROOT)
    run(["cargo", "+1.85.1", "zigbuild", "--manifest-path",
         "core/benchmark/fs-bench-pro/workload/Cargo.toml", "--locked", "--offline",
         "--target", TARGET, *profile], cwd=ROOT)
    daemon = CORE / f"target/{TARGET}/{profile_dir}/layerfs-daemon"
    tool = BENCH / f"workload/target/{TARGET}/{profile_dir}/layerfs-edit-tool"
    CONTEXT.mkdir(parents=True, exist_ok=True)
    for source, name in ((daemon, "layerfs-daemon"), (tool, "layerfs-edit-tool")):
        (CONTEXT / name).write_bytes(source.read_bytes())
        (CONTEXT / name).chmod(0o755)
    (CONTEXT / "layerfs-edit-tool").replace(CONTEXT / "layerfs-edit-tool")
    payloads = payload_files()
    dockerfile = CONTEXT / "Dockerfile"
    dockerfile.write_text(
        f"FROM {BASE}\n"
        "COPY layerfs-daemon /layerfs-daemon\n"
        "COPY layerfs-edit-tool /layerfs-bench/bin/layerfs-edit-tool\n"
        "COPY payloads /layerfs-bench/payloads\n"
        'ENTRYPOINT ["/layerfs-daemon"]\n')
    run(["docker", "build", "-q", "-t", TAG, str(CONTEXT)])
    image = run(["docker", "image", "inspect", TAG, "--format", "{{.Id}}"]).strip()
    record = {
        "schema": "core-fs-bench-pro-exec-fuse-edit-image-v1",
        "base": BASE,
        "tag": TAG,
        "image_id": image,
        "profile": profile_dir,
        "target": TARGET,
        "lockfile_sha256": SHA(CORE / "Cargo.lock"),
        "cargo_config_sha256": SHA(ROOT / ".cargo/config.toml"),
        "dockerfile_sha256": SHA(dockerfile),
        "daemon_sha256": SHA(CONTEXT / "layerfs-daemon"),
        "edit_tool_sha256": SHA(CONTEXT / "layerfs-edit-tool"),
        "registry_sha256": contract.REGISTRY_SHA256,
        "payloads": payloads,
    }
    Path(arguments.out).write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"image_id": image, "out": arguments.out}, sort_keys=True))
    raise SystemExit(0)


if __name__ == "__main__":
    main()
