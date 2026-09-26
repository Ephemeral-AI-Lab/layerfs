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
from shared import edit_insert_v3  # noqa: E402
from shared import edit_insert_v4  # noqa: E402
from shared import edit_all_ioctl_v3  # noqa: E402
from runner import edit_workload_seal, identities  # noqa: E402
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


def payload_files(context, scenario_version, all_ioctl=False):
    """Writes one replacement payload per registered operation, from the recipe."""
    directory = context / "payloads"
    directory.mkdir(parents=True, exist_ok=True)
    written = {}
    if all_ioctl:
        operations = {operation["key"]: operation for family in
                      (preserving, changing, canonical) for operation in family.OPERATIONS}
        for row in edit_all_ioctl_v3.registry()["cases"]:
            element = row["replacement_stream"][0]
            if element["kind"] != "bytes":
                continue
            operation = operations[row["operation_key"]]
            data = contract.payload_bytes(operation["payload_seed"],
                                          row["expected_accepted_literal_bytes"],
                                          operation["replacement_kind"])
            name = Path(element["payload_path"]).name
            if hashlib.sha256(data).hexdigest() != row["replacement_sha256"]:
                raise SystemExit(f"all-ioctl payload mismatch: {row['scenario_id']}")
            path = directory / name
            if path.exists() and path.read_bytes() != data:
                raise SystemExit(f"all-ioctl payload name collision: {name}")
            path.write_bytes(data)
            written[name] = {"bytes": len(data), "sha256": SHA(path)}
        return written
    if scenario_version in (3, 4):
        rows = (edit_insert_v4 if scenario_version == 4 else edit_insert_v3).registry()
        if len({row["replacement_sha256"] for row in rows}) != 1:
            raise SystemExit("splice rows disagree on replacement identity")
        row = rows[0]
        data = contract.payload_bytes(row["payload_seed"], row["replacement_len"],
                                      row["replacement_kind"])
        path = directory / row["payload_source"].rsplit("/", 1)[-1]
        if hashlib.sha256(data).hexdigest() != row["replacement_sha256"]:
            raise SystemExit("splice payload recipe mismatch")
        path.write_bytes(data)
        return {path.name: {"bytes": len(data), "sha256": SHA(path)}}
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
    parser.add_argument("--out")
    parser.add_argument("--scenario-version", type=int, choices=[2, 3, 4], default=2)
    parser.add_argument("--complexity-diagnostic", action="store_true")
    parser.add_argument("--all-ioctl", action="store_true")
    parser.add_argument("--fuse-trace", action="store_true")
    parser.add_argument("--debug-daemon", action="store_true")
    arguments = parser.parse_args()
    if arguments.complexity_diagnostic and arguments.scenario_version != 4:
        raise SystemExit("complexity image requires the published v4 inline carrier")
    if arguments.all_ioctl and (arguments.complexity_diagnostic or arguments.scenario_version != 2):
        raise SystemExit("all-ioctl image is a distinct #232 v3 selection")
    selected = (edit_all_ioctl_v3 if arguments.all_ioctl else
                {3: edit_insert_v3, 4: edit_insert_v4}.get(arguments.scenario_version, contract))
    if selected is edit_all_ioctl_v3:
        selected.registry()
    elif selected is not contract:
        selected.validate_registry()
    elif SHA(BENCH / contract.REGISTRY_PATH) != contract.REGISTRY_SHA256:
        raise SystemExit("v2 registry identity changed")
    context = (CORE / "target/exec-fuse-all-ioctl-v3/image" if arguments.all_ioctl
               else CORE / "target/exec-fuse-complexity-v1/image" if arguments.complexity_diagnostic
               else CORE / f"target/exec-fuse-insert-v{arguments.scenario_version}/image"
               if selected is not contract else CONTEXT)
    tag = ("layerfs-exec-fuse-all-ioctl-v3:issue232" if arguments.all_ioctl
           else "layerfs-exec-fuse-complexity-v1:issue232" if arguments.complexity_diagnostic
           else f"layerfs-exec-fuse-insert-v{arguments.scenario_version}:issue241"
           if arguments.scenario_version == 4 else
           "layerfs-exec-fuse-insert:issue241" if arguments.scenario_version == 3 else TAG)
    output = Path(arguments.out or context.parent / "image.json")
    profile = ["--release"]
    profile_dir = "release"
    run(["cargo", "+1.85.1", "zigbuild", "--manifest-path", "core/Cargo.toml", "--locked",
         "--offline", "--target", TARGET, "-p", "layerfs-daemon", *profile], cwd=ROOT)
    run(["cargo", "+1.85.1", "zigbuild", "--manifest-path",
         "core/benchmark/fs-bench-pro/workload/Cargo.toml", "--locked", "--offline",
         "--target", TARGET, *profile], cwd=ROOT)
    daemon = CORE / f"target/{TARGET}/{profile_dir}/layerfs-daemon"
    tool = BENCH / f"workload/target/{TARGET}/{profile_dir}/layerfs-edit-tool"
    context.mkdir(parents=True, exist_ok=True)
    for source, name in ((daemon, "layerfs-daemon"), (tool, "layerfs-edit-tool")):
        (context / name).write_bytes(source.read_bytes())
        (context / name).chmod(0o755)
    payloads = payload_files(context, arguments.scenario_version, arguments.all_ioctl)
    dockerfile = context / "Dockerfile"
    dockerfile.write_text(
        f"FROM {BASE}\n"
        + ("ENV LAYERFS_COMPLEXITY_DIAGNOSTIC=1\n" if arguments.complexity_diagnostic else "")
        + ("ENV LAYERFS_FUSE_TRACE=1\n" if arguments.fuse_trace else "")
        + "COPY layerfs-daemon /layerfs-daemon\n"
        "COPY layerfs-edit-tool /layerfs-bench/bin/layerfs-edit-tool\n"
        "COPY payloads /layerfs-bench/payloads\n"
        'ENTRYPOINT ["/layerfs-daemon"]\n')
    run(["docker", "build", "-q", "-t", tag, str(context)])
    image = run(["docker", "image", "inspect", tag, "--format", "{{.Id}}"]).strip()
    record = {
        "schema": ("core-fs-bench-pro-all-ioctl-image-v3" if arguments.all_ioctl
                   else "core-fs-bench-pro-complexity-image-v1" if arguments.complexity_diagnostic
                   else f"core-fs-bench-pro-exec-fuse-insert-image-v{arguments.scenario_version}" if
                   selected is not contract else
                   "core-fs-bench-pro-exec-fuse-edit-image-v1"),
        "base": BASE,
        "tag": tag,
        "image_id": image,
        "profile": profile_dir,
        "target": TARGET,
        "lockfile_sha256": SHA(CORE / "Cargo.lock"),
        "cargo_config_sha256": SHA(ROOT / ".cargo/config.toml"),
        "product_seal": identities()["product_seal"],
        "workload_source_seal": edit_workload_seal(),
        "dockerfile_sha256": SHA(dockerfile),
        "daemon_sha256": SHA(context / "layerfs-daemon"),
        "edit_tool_sha256": SHA(context / "layerfs-edit-tool"),
        "registry_sha256": selected.REGISTRY_SHA256,
        "scenario_version": 3 if arguments.all_ioctl else arguments.scenario_version,
        "carrier_abi": ("LFS2/LFE2+LFB3/LFD3/LFA3" if arguments.all_ioctl else
                        selected.ABI if selected is not contract else None),
        "complexity_diagnostic": arguments.complexity_diagnostic,
        "all_ioctl": arguments.all_ioctl,
        "fuse_trace": arguments.fuse_trace,
        "payloads": payloads,
    }
    output.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"image_id": image, "out": str(output)}, sort_keys=True))
    raise SystemExit(0)


if __name__ == "__main__":
    main()
