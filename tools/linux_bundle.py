"""Build and place the sealed Linux bundle (#533 O16), one bounded step per command.

`<cache>/layerfs/bundle/<version>/linux-<arch>/` (`<cache>` is XDG_CACHE_HOME or
~/.cache) holds what every VM, ssh host and `docker` on macOS runs: the guest
`layerfs` (its worker, without the host-only machine layer), `layerfs-daemon`
(with PostgreSQL), `layerfs-sandbox-helper` and `sandbox-image.tar` (the Sandbox
image, saved by `docker save`). `bundle.json` names each file's sha256 and the
bundle hash a machine compares before copying the bundle in again.

The binaries are built in Docker with this worktree's warm, private Linux target
from its launcher registration (tools/worktree.py, docs/general/parallel-worktrees.md).
Each step is its own command, ended at its time limit (120 s by default, 300 s at
most), so no build runs long:

    python3 tools/linux_bundle.py daemon --resources R
    python3 tools/linux_bundle.py worker --resources R
    python3 tools/linux_bundle.py helper --resources R
    python3 tools/linux_bundle.py seal --resources R

Each build step records the source inputs it built. `seal` refuses unless all three
built the inputs the tree has now; it exports the binaries, saves the image, writes
bundle.json and swaps the finished folder into place. Cargo runs with -j 6, or -j 2
while a QUIET file exists in the main checkout.
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import tomllib
import uuid
from pathlib import Path

from cleanup_state import Refused
from worktree_resources import (
    TOOLCHAIN,
    digest,
    docker_build,
    execute,
    inputs,
    lease,
    load,
    operation,
    timestamp,
    write,
)

ARCHIVE = "sandbox-image.tar"
BINARIES = ("layerfs", "layerfs-daemon", "layerfs-sandbox-helper")
STEPS = {
    # the daemon serves sqlite and PostgreSQL Stores in a Sandbox
    "daemon": ["-p", "layerfs-daemon", "--features", "layerfs-daemon/postgresql", "--bin", "layerfs-daemon"],
    # the guest's `layerfs worker`: no machine adapter runs inside a machine
    "worker": ["-p", "layerfs-cli", "--no-default-features", "--bin", "layerfs"],
    # the namespaces Sandbox backend runs it beside the daemon
    "helper": ["-p", "layerfs-sandbox", "--bin", "layerfs-sandbox-helper"],
}
ARCHES = {"arm64": "aarch64", "aarch64": "aarch64", "amd64": "x86_64", "x86_64": "x86_64"}
LIMIT = 300


def cache_root():
    base = os.environ.get("XDG_CACHE_HOME") or str(Path.home() / ".cache")
    return Path(base) / "layerfs" / "bundle"


def version(worktree):
    manifest = tomllib.loads((Path(worktree) / "Cargo.toml").read_text())
    return manifest["workspace"]["package"]["version"]


def jobs(worktree):
    common = subprocess.run(
        ["git", "-C", str(worktree), "rev-parse", "--path-format=absolute", "--git-common-dir"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    return 2 if (Path(common).parent / "QUIET").exists() else 6


def arch(image):
    out = subprocess.run(
        ["docker", "image", "inspect", image, "--format", "{{.Architecture}}"],
        check=True,
        capture_output=True,
        text=True,
        timeout=30,
    ).stdout.strip()
    if out not in ARCHES:
        raise Refused(f"the build image's architecture {out!r} has no bundle name")
    return ARCHES[out]


def cargo_command(step, profile, job_count):
    command = ["cargo", "+" + TOOLCHAIN, "build", "--manifest-path", "Cargo.toml", "--locked", "--offline"]
    command += STEPS[step] + ["-j", str(job_count)]
    if profile == "release":
        command.append("--release")
    return command


def steps_file(record, profile):
    return Path(record["state"]) / "linux-bundle" / (profile + ".json")


def read_steps(record, profile):
    path = steps_file(record, profile)
    return json.loads(path.read_text()) if path.exists() else {}


def build(record, step, profile, timeout):
    """One package, in the worktree's Linux target; records the inputs it built."""
    before = inputs(record)
    command = cargo_command(step, profile, jobs(record["worktree"]))
    with lease(record, "linux"):
        receipt = operation(record, "bundle-" + step)
        started = time.monotonic()
        execute(docker_build(record, command), record["worktree"], timeout=timeout, log=receipt / "output.log")
        seconds = round(time.monotonic() - started, 1)
    if inputs(record) != before:
        raise Refused(f"the source changed while {step} built; build it again")
    path = steps_file(record, profile)
    path.parent.mkdir(mode=0o700, exist_ok=True)
    steps = read_steps(record, profile)
    source = {"head": before["head"], "sha256": before["sha256"]}
    steps[step] = {"source": source, "command": command, "seconds": seconds, "at": timestamp()}
    write(path, steps)
    write(receipt / "receipt.json", {"status": "built", "step": step, **steps[step]})
    print(f"{step}: built in {seconds} s ({receipt})")


def bundle_hash(files):
    return hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()


def place(stage, final):
    """Swaps the finished folder in; the bundle it replaces is removed after."""
    old = None
    if final.exists():
        old = final.with_name("." + final.name + ".old-" + uuid.uuid4().hex)
        final.rename(old)
    stage.rename(final)
    if old:
        shutil.rmtree(old)


def seal(record, profile, image, dest, timeout):
    steps = read_steps(record, profile)
    missing = [s for s in STEPS if s not in steps]
    if missing:
        raise Refused("build these steps first: " + ", ".join(missing))
    now = inputs(record)
    stale = [s for s in STEPS if steps[s]["source"]["sha256"] != now["sha256"]]
    if stale:
        raise Refused("the source changed since these steps built; build them again: " + ", ".join(stale))
    image = image or record["images"]["sandbox"]
    name = "linux-" + arch(record["images"]["build"])
    parent = Path(dest) / version(record["worktree"])
    parent.mkdir(parents=True, exist_ok=True)
    stage = parent / ("." + name + ".partial-" + uuid.uuid4().hex)
    stage.mkdir(mode=0o755)
    try:
        with lease(record, "linux"):
            receipt = operation(record, "bundle-seal")
            script = 'set -eu; p=$1; u=$2; g=$3; shift 3; for f in "$@"; do cp "/work/target/$p/$f" "/export/$f"; '
            script += 'chown "$u:$g" "/export/$f"; done'
            export = docker_build(
                record,
                ["sh", "-c", script, "sh", profile, str(os.getuid()), str(os.getgid()), *BINARIES],
                ["--mount", f"type=bind,src={stage},dst=/export"],
            )
            execute(export, record["worktree"], timeout=timeout, log=receipt / "export.log")
            save = ["docker", "save", "-o", str(stage / ARCHIVE), image]
            execute(save, record["worktree"], timeout=timeout, log=receipt / "save.log")
        # docker save writes 0600; a guest and the docker worker read it as root through a mount
        (stage / ARCHIVE).chmod(0o644)
        files = {f: digest(stage / f) for f in (*BINARIES, ARCHIVE)}
        manifest = {
            "hash": bundle_hash(files),
            "version": version(record["worktree"]),
            "platform": name,
            "profile": profile,
            "image": image,
            "source": {"head": now["head"], "sha256": now["sha256"]},
            "toolchain": TOOLCHAIN,
            "files": files,
            "at": timestamp(),
        }
        write(stage / "bundle.json", manifest)
        final = parent / name
        place(stage, final)
    except BaseException:
        shutil.rmtree(stage, ignore_errors=True)
        raise
    write(receipt / "receipt.json", {"status": "sealed", "bundle": str(final), **manifest})
    print(f"sealed {final} (hash {manifest['hash']})")
    return final


def main(argv=None):
    cli = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    cli.add_argument("step", choices=[*STEPS, "seal"])
    cli.add_argument("--resources", required=True, help="the worktree's resources.json from tools/worktree.py")
    cli.add_argument("--profile", choices=["debug", "release"], default="debug")
    cli.add_argument("--image", help="the Sandbox image to save (default: the registration's sandbox image)")
    cli.add_argument("--dest", default=None, help="the bundle root (default: <cache>/layerfs/bundle)")
    cli.add_argument("--timeout", type=int, default=120, help=f"this step's time limit in seconds (at most {LIMIT})")
    args = cli.parse_args(argv)
    try:
        if not 0 < args.timeout <= LIMIT:
            raise Refused(f"a step's time limit is in (0, {LIMIT}] seconds")
        record = load(args.resources)
        if args.step == "seal":
            seal(record, args.profile, args.image, args.dest or cache_root(), args.timeout)
        else:
            build(record, args.step, args.profile, args.timeout)
    except Refused as error:
        print(f"refused: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
