"""Build and place the sealed Linux bundle (#533 O16), one bounded step per command.

`<cache>/layerfs/bundle/<version>/linux-<arch>/` (`<cache>` is XDG_CACHE_HOME or
~/.cache) holds what every VM, ssh host and `docker` on macOS runs: the guest
`layerfs` (its worker, without the host-only machine layer), `layerfs-daemon`
(with PostgreSQL), `layerfs-sandbox-helper`, `sandbox-image.tar` (the Sandbox
image) and `runtime/`, the Sandbox runtime layer (R5). `bundle.json` names each
file's sha256, the image's config digest and the bundle hash a machine compares
before copying the bundle in again.

The runtime layer is what the daemon needs inside any image's root: `runtime/lib/`
holds the dynamic loader and every library `ldd layerfs-daemon` lists, copied out of
the build image the daemon was built in, so the ABIs match; `runtime/bin/` holds
that image's glibc `getent` and Debian's static `bash` and `busybox`
(`bash-static`, `busybox-static`, from the build image's own Debian release).
`seal` refuses when `ldd` lists a library it did not copy. `bundle.json` names it
in `sandbox_runtime` (its hash, loader and glibc version), and `licenses/` holds
the Debian copyright files of every package it came from.

The namespaces backend reads a docker-archive with uncompressed layers, named by
its config digest (#527). Docker with the containerd store saves gzip layers, so
`seal` rewrites `docker save`'s output into that shape, one layer at a time.

The binaries are built in Docker with this worktree's warm, private Linux target
from its launcher registration (tools/worktree.py, docs/general/parallel-worktrees.md).
Each step is its own command, ended at its time limit (120 s by default, 300 s at
most), so no build runs long:

    python3 tools/linux_bundle.py daemon --resources R
    python3 tools/linux_bundle.py worker --resources R
    python3 tools/linux_bundle.py helper --resources R
    python3 tools/linux_bundle.py runtime --resources R
    python3 tools/linux_bundle.py seal --resources R

`runtime` is the one step with network: it downloads the two static packages with
the build image's apt (over https, signatures checked by apt) into the
registration's state, once; it does not depend on the source.

Each build step records the source inputs it built. `seal` refuses unless all three
built the inputs the tree has now; it exports the binaries, saves the image, writes
bundle.json and swaps the finished folder into place. Cargo runs with -j 6, or -j 2
while a QUIET file exists in the main checkout.
"""

import argparse
import gzip
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
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
    # the guest's `layerfs worker`: no machine adapter runs inside a machine;
    # it reads a PostgreSQL Store's schema binding in sandbox.create (#535 G3)
    "worker": ["-p", "layerfs-cli", "--no-default-features", "--features", "postgresql", "--bin", "layerfs"],
    # the namespaces Sandbox backend runs it beside the daemon
    "helper": ["-p", "layerfs-sandbox-backend", "--bin", "layerfs-sandbox-helper"],
}
ARCHES = {"arm64": "aarch64", "aarch64": "aarch64", "amd64": "x86_64", "x86_64": "x86_64"}
# the Sandbox runtime layer (R5): the loader each architecture's glibc names as its
# binaries' interpreter, and the static programs (Debian package, path in it)
RUNTIME = "runtime"
LOADERS = {"aarch64": "ld-linux-aarch64.so.1", "x86_64": "ld-linux-x86-64.so.2"}
STATIC = {"bash": ("bash-static", "bin/bash-static"), "busybox": ("busybox-static", "bin/busybox")}
LICENSES = "licenses"
LIMIT = 300


def cache_root():
    base = os.environ.get("XDG_CACHE_HOME") or str(Path.home() / ".cache")
    return Path(base) / "layerfs" / "bundle"


def version(worktree):
    manifest = tomllib.loads((Path(worktree) / "Cargo.toml").read_text())
    return manifest["workspace"]["package"]["version"]


def protocol(worktree):
    source = Path(worktree) / "control/crates/layerfs-machine/src/worker/hello.rs"
    match = re.search(r"pub const PROTOCOL: i64 = (\d+);", source.read_text())
    if not match:
        raise Refused("the worker protocol constant could not be read")
    return int(match[1])


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


def runtime_dir(record):
    return Path(record["state"]) / "linux-bundle" / RUNTIME


FETCH = r"""set -eu
uid=$1; gid=$2; shift 2
# apt over https: the signatures are checked either way, and https passes proxies
for f in /etc/apt/sources.list.d/*.sources /etc/apt/sources.list; do
  if [ -f "$f" ]; then sed -i 's,http://deb.debian.org,https://deb.debian.org,g' "$f"; fi
done
apt-get -o Acquire::Retries=3 -qq update
mkdir -p /out/debs /out/x
cd /out/debs
apt-get -o Acquire::Retries=3 -qq download "$@"
for deb in *.deb; do
  printf '%s %s %s\n' "$(dpkg-deb -f "$deb" Package)" "$(dpkg-deb -f "$deb" Version)" \
    "$(sha256sum "$deb" | cut -d' ' -f1)"
  dpkg-deb -x "$deb" /out/x
done > /out/packages
chown -R "$uid:$gid" /out
"""


def fetch_runtime(record, timeout):
    """Downloads the runtime layer's static programs with the build image's apt
    into the registration's state: `<state>/linux-bundle/runtime/{bash,busybox}`
    and `runtime.json` naming each package's version and .deb sha256."""
    out = runtime_dir(record)
    stage = out.with_name("." + RUNTIME + ".partial-" + uuid.uuid4().hex)
    stage.mkdir(mode=0o700, parents=True)
    try:
        receipt = operation(record, "bundle-runtime")
        packages = [package for package, _ in STATIC.values()]
        command = ["sh", "-c", FETCH, "sh", str(os.getuid()), str(os.getgid()), *packages]
        fetch = docker_build(record, command, ["--mount", f"type=bind,src={stage},dst=/out"], network="bridge")
        execute(fetch, record["worktree"], timeout=timeout, log=receipt / "output.log")
        versions = {}
        for line in (stage / "packages").read_text().splitlines():
            package, version, sha = line.split()
            versions[package] = {"version": version, "deb": sha}
        meta = {"image": record["images"]["build"], "packages": versions, "files": {}, "at": timestamp()}
        for name, (package, path) in STATIC.items():
            if package not in versions:
                raise Refused(f"apt did not download {package}")
            source = stage / "x" / path
            if interpreter(source) is not False:
                raise Refused(f"{package}'s {path} is not a static executable")
            shutil.copyfile(source, stage / name)
            (stage / name).chmod(0o755)
            meta["files"][name] = digest(stage / name)
        for package, _ in STATIC.values():
            notice = stage / "x/usr/share/doc" / package / "copyright"
            if notice.is_file():
                shutil.copyfile(notice, stage / (package + ".copyright"))
        shutil.rmtree(stage / "x")
        shutil.rmtree(stage / "debs")
        (stage / "packages").unlink()
        write(stage / "runtime.json", meta)
        place(stage, out)
    except BaseException:
        shutil.rmtree(stage, ignore_errors=True)
        raise
    write(receipt / "receipt.json", {"status": "fetched", **meta})
    named = ", ".join(package + " " + value["version"] for package, value in versions.items())
    print(f"runtime: {named} ({out})")


def interpreter(path):
    """An ELF executable's `PT_INTERP` (its loader) as text, `False` for a static
    one; refuses a file that is not a 64-bit little-endian ELF."""
    with Path(path).open("rb") as elf:
        head = elf.read(64)
        if head[:4] != b"\x7fELF" or head[4] != 2 or head[5] != 1:
            raise Refused(f"{path} is not a 64-bit little-endian ELF file")
        phoff = int.from_bytes(head[32:40], "little")
        size, count = int.from_bytes(head[54:56], "little"), int.from_bytes(head[56:58], "little")
        for index in range(count):
            elf.seek(phoff + index * size)
            entry = elf.read(size)
            if int.from_bytes(entry[0:4], "little") == 3:  # PT_INTERP
                offset, length = int.from_bytes(entry[8:16], "little"), int.from_bytes(entry[32:40], "little")
                elf.seek(offset)
                return elf.read(length).rstrip(b"\0").decode()
    return False


# Runs in the build image, offline, beside the binaries' export: copies the loader and
# every library `ldd` lists for the daemon and `getent` (by the name `ldd` gives, the
# file it resolves to), `getent` itself and the copyright files of the packages that
# own them, and leaves `ldd`'s own output for `runtime_layer` to check.
COLLECT = r"""set -eu
p=$1; u=$2; g=$3
lib=/export/runtime/lib; bin=/export/runtime/bin
mkdir -p "$lib" "$bin" /export/licenses
{ ldd "/work/target/$p/layerfs-daemon"; ldd /usr/bin/getent; } > /export/.ldd
awk '$2 == "=>" && $3 ~ /^\// { print $1, $3 } $1 ~ /^\// { n = split($1, a, "/"); print a[n], $1 }' /export/.ldd |
  sort -u | while read -r name path; do
    real=$(readlink -f "$path"); cp "$real" "$lib/$name"
    { dpkg -S "$path" 2>/dev/null || dpkg -S "$real" 2>/dev/null || true; } | cut -d: -f1 >> /export/.owners
  done
cp /usr/bin/getent "$bin/getent"
dpkg -S "$(readlink -f /usr/bin/getent)" | cut -d: -f1 >> /export/.owners
for pkg in $(sort -u /export/.owners); do
  cp "/usr/share/doc/$pkg/copyright" "/export/licenses/$pkg.copyright"
done
getconf GNU_LIBC_VERSION | cut -d' ' -f2 > /export/.glibc
chown -R "$u:$g" /export/runtime /export/licenses /export/.ldd /export/.glibc /export/.owners
"""


def runtime_layer(stage, arch_name, fetched):
    """Completes and checks the runtime layer `COLLECT` started in `stage`: adds the
    static programs from `fetched` (the `runtime` step's folder) and their copyright
    files, refuses a library `ldd` listed that was not copied (or one it did not
    find), and answers bundle.json's `sandbox_runtime` and the files to list."""
    meta = fetched / "runtime.json"
    if not meta.is_file():
        raise Refused("fetch the runtime layer's static programs first: python3 tools/linux_bundle.py runtime")
    fetched_record = json.loads(meta.read_text())
    lib, bin_ = stage / RUNTIME / "lib", stage / RUNTIME / "bin"
    loader = LOADERS.get(arch_name)
    if loader is None:
        raise Refused(f"no Sandbox runtime layer for {arch_name}")
    listed = (stage / ".ldd").read_text()
    for line in listed.splitlines():
        words = line.split()
        if not words or words[0].startswith("linux-vdso") or words[0].startswith("linux-gate"):
            continue
        if "not found" in line:
            raise Refused(f"ldd found no library for {words[0]}")
        name = Path(words[0]).name if words[0].startswith("/") else words[0]
        if not (lib / name).is_file():
            raise Refused(f"ldd lists {name}, which the runtime layer did not copy")
    if not (lib / loader).is_file():
        raise Refused(f"the runtime layer has no loader {loader}")
    for name in STATIC:
        source = fetched / name
        if digest(source) != fetched_record["files"][name]:
            raise Refused(f"the fetched {name} does not match its record; fetch it again")
        shutil.copyfile(source, bin_ / name)
    for package, _ in STATIC.values():
        notice = fetched / (package + ".copyright")
        if notice.is_file():
            shutil.copyfile(notice, stage / LICENSES / (package + ".copyright"))
    for path in lib.iterdir():
        # the loader runs as a program: it starts the daemon and `getent`
        path.chmod(0o755 if path.name == loader else 0o644)
    for path in bin_.iterdir():
        path.chmod(0o755)
    for path in (stage / LICENSES).iterdir():
        path.chmod(0o644)
    glibc = (stage / ".glibc").read_text().strip()
    for scratch in (".ldd", ".glibc", ".owners"):
        (stage / scratch).unlink(missing_ok=True)
    files = {
        p.relative_to(stage).as_posix(): digest(p)
        for top in (RUNTIME, LICENSES)
        for p in sorted((stage / top).rglob("*"))
        if p.is_file()
    }
    layer = {f: h for f, h in files.items() if f.startswith(RUNTIME + "/")}
    sandbox_runtime = {
        "hash": bundle_hash(layer),
        "loader": f"lib/{loader}",
        "glibc": glibc,
        "packages": {p: v["version"] for p, v in fetched_record["packages"].items()},
    }
    return sandbox_runtime, files


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
            collect = docker_build(
                record,
                ["sh", "-c", COLLECT, "sh", profile, str(os.getuid()), str(os.getgid())],
                ["--mount", f"type=bind,src={stage},dst=/export"],
            )
            execute(collect, record["worktree"], timeout=timeout, log=receipt / "runtime.log")
            saved = stage / ".saved.tar"
            save = ["docker", "save", "-o", str(saved), image]
            execute(save, record["worktree"], timeout=timeout, log=receipt / "save.log")
        config = docker_archive(saved, stage / ARCHIVE, stage)
        saved.unlink()
        # a guest and the docker worker read it as root through a mount
        (stage / ARCHIVE).chmod(0o644)
        sandbox_runtime, layer = runtime_layer(stage, name.removeprefix("linux-"), runtime_dir(record))
        files = {f: digest(stage / f) for f in (*BINARIES, ARCHIVE)}
        files.update(layer)
        manifest = {
            "hash": bundle_hash(files),
            "version": version(record["worktree"]),
            "platform": name,
            "profile": profile,
            "image": config,
            "protocol": protocol(record["worktree"]),
            "runtime": bundle_hash({f: files[f] for f in BINARIES}),
            "sandbox_runtime": sandbox_runtime,
            "saved_from": image,
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


def docker_archive(saved, out, scratch):
    """Rewrites a `docker save` archive holding one image as a docker-archive
    whose layers are uncompressed (`<n>/layer.tar`) and whose config is
    `<config hex>.json`: the shape the namespaces backend reads. Answers the
    config digest. Each layer is decompressed to a scratch file, added, removed."""
    with tarfile.open(saved) as source:
        entries = json.load(source.extractfile("manifest.json"))
        if len(entries) != 1:
            raise Refused(f"the saved image archive holds {len(entries)} images, not one")
        entry = entries[0]
        config_name = entry["Config"]
        hexes = Path(config_name).name.removesuffix(".json")
        if len(hexes) != 64 or any(c not in "0123456789abcdef" for c in hexes):
            raise Refused(f"the saved archive names its config {config_name!r}, not by its digest")
        with tarfile.open(out, "w", format=tarfile.PAX_FORMAT) as archive:
            data = source.extractfile(config_name).read()
            if hashlib.sha256(data).hexdigest() != hexes:
                raise Refused("the saved archive's config does not hash to its name")
            info = tarfile.TarInfo(hexes + ".json")
            info.size, info.mode = len(data), 0o644
            archive.addfile(info, io.BytesIO(data))
            layers = []
            for n, name in enumerate(entry["Layers"]):
                plain = scratch / f".layer-{n}.tar"
                with source.extractfile(name) as blob, plain.open("wb") as sink:
                    head = blob.read(2)
                    blob.seek(0)
                    stream = gzip.GzipFile(fileobj=blob) if head == b"\x1f\x8b" else blob
                    shutil.copyfileobj(stream, sink, 1 << 20)
                with plain.open("rb") as check:
                    block = check.read(512)
                    # an empty layer is a tar of zero blocks
                    if block[257:262] != b"ustar" and block.strip(b"\0"):
                        raise Refused(f"the saved layer {name} is not a tar once decompressed")
                info = tarfile.TarInfo(f"{n}/layer.tar")
                info.size, info.mode = plain.stat().st_size, 0o644
                with plain.open("rb") as layer:
                    archive.addfile(info, layer)
                plain.unlink()
                layers.append(f"{n}/layer.tar")
            listing = json.dumps([{"Config": hexes + ".json", "RepoTags": None, "Layers": layers}]).encode()
            info = tarfile.TarInfo("manifest.json")
            info.size, info.mode = len(listing), 0o644
            archive.addfile(info, io.BytesIO(listing))
    return "sha256:" + hexes


def main(argv=None):
    cli = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    cli.add_argument("step", choices=[*STEPS, RUNTIME, "seal"])
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
        elif args.step == RUNTIME:
            fetch_runtime(record, args.timeout)
        else:
            build(record, args.step, args.profile, args.timeout)
    except Refused as error:
        print(f"refused: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
