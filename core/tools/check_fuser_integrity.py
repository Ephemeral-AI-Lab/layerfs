#!/usr/bin/env python3
"""Check fuser dependency provenance without modifying packages or running builds.

This is a focused dependency check, not an aggregate pre-push/CI wrapper.
Registry packages are checked against their locked archive checksum and every
archived file. Git sources must use the official repository and an explicitly
approved full revision. No Cargo patch/replace/path override is accepted.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tarfile
import tomllib

OFFICIAL_GIT = "https://github.com/cberner/fuser.git"
APPROVED_REVISIONS: tuple[str, ...] = ()


def is_fuser(name, value):
    return name.split(":", 1)[0] == "fuser" or (
        isinstance(value, dict) and value.get("package") == "fuser"
    )


def manifest_errors(document, approved=APPROVED_REVISIONS):
    errors = []
    for section in ("patch", "replace"):
        entries = document.get(section, {})
        groups = entries.values() if section == "patch" else (entries,)
        for group in groups:
            if isinstance(group, dict):
                for name, value in group.items():
                    if is_fuser(name, value):
                        errors.append(f"{section} override of fuser is forbidden")

    def visit(table):
        if not isinstance(table, dict):
            return
        for section, values in table.items():
            if section in ("dependencies", "dev-dependencies", "build-dependencies"):
                for name, value in values.items():
                    if not is_fuser(name, value) or isinstance(value, str):
                        continue
                    if not isinstance(value, dict):
                        errors.append("malformed fuser dependency")
                        continue
                    if any(key in value for key in ("path", "registry", "registry-index")):
                        errors.append("local/custom-registry fuser source is forbidden")
                    if "git" in value:
                        repository = value["git"].removesuffix(".git")
                        if repository != OFFICIAL_GIT.removesuffix(".git"):
                            errors.append("fuser Git source must be the official repository")
                        revision = value.get("rev", "")
                        if not re.fullmatch(r"[0-9a-f]{40}", revision):
                            errors.append("fuser Git source needs a full immutable revision")
                        elif revision not in approved:
                            errors.append("fuser Git revision has no owner approval")
                        if "branch" in value or "tag" in value:
                            errors.append("fuser Git source cannot follow a branch/tag")
            elif isinstance(values, dict) and section not in ("patch", "replace"):
                visit(values)

    visit(document)
    return errors


def lock_errors(document, approved=APPROVED_REVISIONS):
    errors = []
    for package in document.get("package", []):
        if package.get("name") != "fuser":
            continue
        source = package.get("source", "")
        if source == "registry+https://github.com/rust-lang/crates.io-index":
            if not re.fullmatch(r"[0-9a-f]{64}", package.get("checksum", "")):
                errors.append("registry fuser requires its locked checksum")
        elif source.startswith("git+"):
            valid = any(
                source in (
                    f"git+{OFFICIAL_GIT}?rev={revision}#{revision}",
                    f"git+{OFFICIAL_GIT.removesuffix('.git')}?rev={revision}#{revision}",
                )
                for revision in approved
            )
            if not valid:
                errors.append("locked fuser Git source/revision is not approved")
        else:
            errors.append("locked fuser has a local/replacement source")
    return errors


def config_errors(document):
    errors = []
    if document.get("paths"):
        errors.append("Cargo path overrides can replace fuser and are forbidden")
    if any(isinstance(source, dict) and "directory" in source
           for source in document.get("source", {}).values()):
        errors.append("Cargo directory source replacement can vendor fuser")
    return errors


def package_errors(archive: Path, source: Path, checksum: str):
    errors = []
    if hashlib.sha256(archive.read_bytes()).hexdigest() != checksum:
        return ["fuser archive differs from the locked registry checksum"]
    prefix = f"{source.name}/"
    archived = set()
    with tarfile.open(archive) as package:
        for entry in package.getmembers():
            if entry.isdir():
                continue
            if not entry.isfile() or not entry.name.startswith(prefix):
                errors.append(f"unexpected archive entry: {entry.name}")
                continue
            relative = Path(entry.name[len(prefix):])
            if relative.is_absolute() or ".." in relative.parts:
                errors.append("invalid archive path")
                continue
            archived.add(relative.as_posix())
            path = source / relative
            if path.is_symlink() or not path.is_file():
                errors.append(f"missing/redirected package file: {relative}")
                continue
            stream = package.extractfile(entry)
            assert stream is not None
            if hashlib.sha256(path.read_bytes()).digest() != hashlib.sha256(stream.read()).digest():
                errors.append(f"modified package file: {relative}")
    generated = {".cargo-ok", ".cargo-checksum.json"}
    for path in source.rglob("*"):
        if path.is_file() and path.relative_to(source).as_posix() not in archived | generated:
            errors.append(f"extra package file: {path.relative_to(source)}")
    return errors


def repository_errors(root: Path):
    paths = subprocess.check_output(
        ["git", "-C", str(root), "ls-files", "-z", "--cached", "--others",
         "--exclude-standard", "--", "Cargo.toml", "Cargo.lock", "**/Cargo.toml",
         "**/Cargo.lock", ".cargo/config.toml", ".cargo/config"]
    ).decode().split("\0")
    errors = []
    checked = 0
    for name in sorted(set(paths) - {""}):
        path = root / name
        if not path.is_file() or "target" in path.parts:
            continue
        checked += 1
        try:
            document = tomllib.loads(path.read_text())
            findings = (manifest_errors(document) if path.name == "Cargo.toml"
                        else lock_errors(document) if path.name == "Cargo.lock"
                        else config_errors(document))
            errors.extend(f"{name}: {error}" for error in findings)
        except (OSError, ValueError) as error:
            errors.append(f"{name}: cannot verify: {error}")
    return checked, errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--package-directory", type=Path)
    parser.add_argument("--locked-checksum")
    args = parser.parse_args()
    supplied = [args.archive, args.package_directory, args.locked_checksum]
    if any(supplied) and not all(supplied):
        parser.error("archive, package-directory and locked-checksum must be supplied together")
    checked, errors = repository_errors(args.root)
    if all(supplied):
        errors.extend(package_errors(*supplied))
    print(json.dumps({"checked_manifests_locks_configs": checked, "errors": errors,
                      "package_verified": bool(all(supplied)) and not errors}, indent=2))
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
