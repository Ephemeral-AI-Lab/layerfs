"""Prepare private caches, launch scoped Cargo commands and publish pinned SDK builds."""

import json
import os
from pathlib import Path
import shutil

from cleanup_state import Refused, ensure_idle, open_files, docker_state, run
from worktree_resources import (
    TOOLCHAIN,
    closed_volumes,
    copy_dependencies,
    digest,
    docker_build,
    environment,
    execute,
    inputs,
    lease,
    load,
    operation,
    timestamp,
    volume,
    write,
)


def prepare(args):
    record = load(args.resources)
    with lease(record, args.platform):
        receipt = operation(record, "prepare-" + args.platform)
        try:
            if args.platform == "macos":
                home = Path(record["macos_cargo_home"])
                home.mkdir(mode=0o700, exist_ok=True)
                if args.dependency_seed:
                    copy_dependencies(args.dependency_seed, home)
                command = ["cargo", "+" + TOOLCHAIN, "fetch", "--manifest-path", "Cargo.toml", "--locked"]
                execute(command, record["worktree"], environment(record, "macos"), log=receipt / "output.log")
            else:
                for key in ("linux_cargo_volume", "linux_target_volume"):
                    volume(record, key)
                closed_volumes([record["linux_cargo_volume"], record["linux_target_volume"]])
                if args.dependency_seed:
                    seed = Path(args.dependency_seed).resolve()
                    ensure_idle(
                        [seed / n for n in ("registry", "git") if (seed / n).exists()], [], docker_state(), open_files()
                    )
                    script = (
                        'set -eu; for d in registry git; do if [ -d "/seed/$d" ]; then '
                        'test ! -e "/cargo-home/$d"; cp -a "/seed/$d" "/cargo-home/$d"; fi; done'
                    )
                    command = docker_build(
                        record, ["sh", "-c", script], ["--mount", f"type=bind,src={seed},dst=/seed,readonly"]
                    )
                    execute(command, record["worktree"], log=receipt / "seed.log")
                command = docker_build(
                    record,
                    ["cargo", "+" + TOOLCHAIN, "fetch", "--manifest-path", "Cargo.toml", "--locked"],
                    network="default",
                )
                execute(command, record["worktree"], log=receipt / "output.log")
            write(
                receipt / "receipt.json",
                {"status": "prepared", "at": timestamp(), "resources": record, "dependency_seed": args.dependency_seed},
            )
        except BaseException as error:
            write(receipt / "receipt.json", {"status": "failed", "error": str(error), "at": timestamp()})
            raise
    print(receipt)


def cargo_arguments(arguments):
    arguments = list(arguments)
    if arguments[:1] == ["--"]:
        arguments.pop(0)
    if not arguments or arguments[0] not in {"build", "test", "check", "clippy"}:
        raise Refused("Select build, test, check or clippy and explicit packages")
    forbidden = ("--workspace", "--all", "--manifest-path", "--target-dir", "--target", "--config")
    if any(a == f or a.startswith(f + "=") for a in arguments for f in forbidden):
        raise Refused("Launcher owns manifest/target/config; select packages instead of an aggregate workspace gate")
    if any("fs-bench-pro" in a for a in arguments):
        raise Refused("fs-bench-pro is frozen")
    if not any(a in {"-p", "--package"} or a.startswith("--package=") for a in arguments):
        raise Refused("Cargo invocation needs explicit package selection")
    return [
        "cargo",
        "+" + TOOLCHAIN,
        arguments[0],
        "--manifest-path",
        "Cargo.toml",
        "--locked",
        "--offline",
        *arguments[1:],
    ]


def cargo(args):
    record = load(args.resources)
    command = cargo_arguments(args.arguments)
    is_test = command[2] == "test" and "--no-run" not in command
    if is_test and not 0 < args.timeout <= 100:
        raise Refused("Test timeout must be in (0, 100] seconds, leaving time for container cancellation")
    with lease(record, args.platform):
        receipt = operation(record, "cargo-" + args.platform)
        try:
            if args.platform == "linux":
                closed_volumes([record["linux_cargo_volume"], record["linux_target_volume"]])
            if is_test:
                compile_command = command[: command.index("--")] if "--" in command else list(command)
                compile_command.append("--no-run")
                compile_command = docker_build(record, compile_command) if args.platform == "linux" else compile_command
                execute(
                    compile_command,
                    record["worktree"],
                    environment(record, "macos") if args.platform == "macos" else None,
                    log=receipt / "compile.log",
                )
            if args.platform == "linux":
                command = docker_build(record, command)
            execute(
                command,
                record["worktree"],
                environment(record, "macos") if args.platform == "macos" else None,
                timeout=args.timeout if is_test else None,
                log=receipt / "output.log",
            )
            outcome = "passed"
        except BaseException as error:
            outcome = "failed"
            write(receipt / "receipt.json", {"status": outcome, "error": str(error), "command": command})
            raise
        write(receipt / "receipt.json", {"status": outcome, "command": command, "at": timestamp()})
    print(receipt)


def copy_sdk(root, bundle):
    """The JavaScript packages and the `layerfs` command, in the repository's
    own layout: api/package.json, api/agent (@layerfs/agent),
    api/backend, api/runtime and api/cli. A bundle holds no symlink, so
    each package resolves by name through a re-export shim under
    api/node_modules/@layerfs/<name> that imports its one real copy."""
    destination = bundle / "api"
    destination.mkdir(parents=True)
    ignore = shutil.ignore_patterns("node_modules", "__pycache__", "target")
    packages = [destination / folder for folder in ("agent", "backend", "runtime")]
    for package in packages:
        shutil.copytree(root / "api" / package.relative_to(destination), package, ignore=ignore)
    shutil.copy2(root / "api/package.json", destination / "package.json")
    for package in sorted(packages):
        shim(destination / "node_modules", package)
    if (root / "api/cli/package.json").is_file():
        cli = destination / "cli"
        cli.mkdir()
        for folder in ("src", "test"):
            if (root / "api/cli" / folder).is_dir():
                shutil.copytree(root / "api/cli" / folder, cli / folder, ignore=ignore)
        shutil.copy2(root / "api/cli/package.json", cli / "package.json")
        shim(cli / "node_modules", destination / "agent")


def shim(node_modules, package):
    """node_modules/<name>: re-exports each of `package`'s JavaScript exports."""
    manifest = json.loads((package / "package.json").read_text())
    name = manifest["name"]
    shim = node_modules / name
    shim.mkdir(parents=True)
    exports = {}
    for subpath, target in manifest.get("exports", {}).items():
        if not target.endswith(".js"):
            continue
        file = "index.js" if subpath == "." else subpath.removeprefix("./") + ".js"
        real = os.path.relpath(package / target, shim)
        (shim / file).write_text(f'export * from "{real}";\n')
        exports[subpath] = f"./{file}"
    (shim / "package.json").write_text(
        json.dumps({"name": name, "type": "module", "exports": exports}, indent=2) + "\n"
    )


def verify_bundle(path):
    path = Path(path).resolve()
    manifest = json.loads((path / "manifest.json").read_text())
    if manifest.get("status") != "sealed" or manifest.get("version") != 1:
        raise Refused("E2E needs a sealed bundle")
    if any(p.is_symlink() for p in path.rglob("*")):
        raise Refused("Artifact bundle contains a symlink")
    for relative, expected in manifest["files"].items():
        candidate = path / relative
        if candidate.is_symlink() or not candidate.resolve().is_relative_to(path):
            raise Refused("Artifact path escapes its bundle")
        if digest(candidate) != expected:
            raise Refused("Artifact hash changed: " + relative)
    actual = {str(p.relative_to(path)) for p in path.rglob("*") if p.is_file() and p != path / "manifest.json"}
    if actual != set(manifest["files"]):
        raise Refused("Bundle namespace changed")
    return manifest


def build(args):
    record = load(args.resources)
    if args.jobs < 1:
        raise Refused("jobs must be positive")
    with lease(record, args.platform):
        receipt = operation(record, "build-" + args.platform)
        build_id = receipt.name
        bundle = Path(record["state"]) / "artifacts" / build_id
        bundle.mkdir(mode=0o700)
        before = inputs(record)
        write(receipt / "inputs.json", before)
        command = [
            "cargo",
            "+" + TOOLCHAIN,
            "build",
            "--manifest-path",
            "Cargo.toml",
            "--locked",
            "--offline",
            "-p",
            "layerfs-daemon",
            "-p",
            "layerfs-node",
            "-j",
            str(args.jobs),
        ]
        if args.profile == "release":
            command.append("--release")
        try:
            if args.platform == "linux":
                closed_volumes([record["linux_cargo_volume"], record["linux_target_volume"]])
                execute(docker_build(record, command), record["worktree"], log=receipt / "output.log")
                script = (
                    'set -eu; cp "/work/target/$1/layerfs-daemon" /export/layerfs-daemon; '
                    'cp "/work/target/$1/liblayerfs_node.so" /export/layerfs.node; '
                    'chown "$2:$3" /export/layerfs-daemon /export/layerfs.node'
                )
                export = docker_build(
                    record,
                    ["sh", "-c", script, "sh", args.profile, str(os.getuid()), str(os.getgid())],
                    ["--mount", f"type=bind,src={bundle},dst=/export"],
                )
                execute(export, record["worktree"], log=receipt / "export.log")
            else:
                execute(command, record["worktree"], environment(record, "macos"), log=receipt / "output.log")
                profile = Path(record["macos_target"]) / args.profile
                shutil.copy2(profile / "layerfs-daemon", bundle / "layerfs-daemon")
                shutil.copy2(profile / "liblayerfs_node.dylib", bundle / "layerfs.node")
            copy_sdk(Path(record["worktree"]), bundle)
            if inputs(record) != before:
                raise Refused("Source changed during build; partial bundle is not eligible for E2E")
            files = {str(p.relative_to(bundle)): digest(p) for p in bundle.rglob("*") if p.is_file()}
            manifest = {
                "version": 1,
                "status": "sealed",
                "build_id": build_id,
                "at": timestamp(),
                "worktree_id": record["id"],
                "platform": args.platform,
                "profile": args.profile,
                "source": {"head": before["head"], "sha256": before["sha256"]},
                "toolchain": TOOLCHAIN,
                "images": record["images"],
                "command": command,
                "files": files,
            }
            write(bundle / "manifest.json", manifest)
            verify_bundle(bundle)
            for path in bundle.rglob("*"):
                if path.is_file():
                    path.chmod(path.stat().st_mode & ~0o222)
            write(receipt / "receipt.json", {"status": "sealed", "bundle": str(bundle), "at": timestamp()})
        except BaseException as error:
            write(receipt / "receipt.json", {"status": "failed", "bundle": str(bundle), "error": str(error)})
            raise
    print(bundle)


def seed(args):
    record, source = load(args.resources), load(args.source_resources)
    if record["id"] == source["id"]:
        raise Refused("Seed source and destination must be different worktrees")
    if record["images"] != source["images"]:
        raise Refused("Seed images differ")
    for relative in ("Cargo.lock", ".cargo/config.toml"):
        if digest(Path(record["worktree"]) / relative) != digest(Path(source["worktree"]) / relative):
            raise Refused("Seed lockfile or build configuration differs")
    with lease(record, args.platform), lease(source, args.platform):
        receipt = operation(record, "seed-" + args.platform)
        if args.platform == "linux":
            for owner in (record, source):
                volume(owner, "linux_target_volume")
            closed_volumes([record["linux_target_volume"], source["linux_target_volume"]])
            script = (
                'set -eu; test ! -e "/work/target/$1"; cp -a "/seed/$1" "/work/target/$1"; '
                'for f in .rustc_info.json CACHEDIR.TAG; do if [ -f "/seed/$f" ]; then cp -a "/seed/$f" /work/target/; fi; done'
            )
            command = docker_build(
                record,
                ["sh", "-c", script, "sh", args.profile],
                ["--mount", f"type=volume,src={source['linux_target_volume']},dst=/seed,readonly"],
            )
            execute(command, record["worktree"], log=receipt / "output.log")
        else:
            environment(record, "macos")
            original = Path(source["macos_target"])
            dest = Path(record["macos_target"])
            ensure_idle([original / args.profile], [], docker_state(), open_files())
            dest.mkdir(parents=True, exist_ok=True)
            if (dest / args.profile).exists():
                raise Refused("Destination profile exists; never overwrite a warm target")
            run(["cp", "-cR", str(original / args.profile), str(dest / args.profile)])
            for name in (".rustc_info.json", "CACHEDIR.TAG"):
                if (original / name).is_file():
                    shutil.copy2(original / name, dest / name)
        write(
            receipt / "receipt.json",
            {"status": "seeded", "source": source["id"], "profile": args.profile, "at": timestamp()},
        )
    print(receipt)
