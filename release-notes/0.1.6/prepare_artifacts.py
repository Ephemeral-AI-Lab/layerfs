#!/usr/bin/env python3
"""Prepare/verify six source-only v0.1.6 assets (Python 3.11+).

Only committed files are archived. The evidence bundle is a selection of tracked
reports and registries, not the untracked local raw receipts under
benchmark-results/ referenced by absolute path in those reports. It preserves
original source identities and does not represent a benchmark rerun. No command
creates a tag, release, binary, package or runtime image.
"""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile
import tomllib
import zipfile


REPO = Path(__file__).resolve().parents[2]
TAG = "refs/tags/v0.1.6"
EVIDENCE = (
    "docs/roadmap/0.1/0.1.6",
    "release-notes/0.1.5",
    "benchmark/fs-bench-pro/families",
    "docs/general/benchmark_rules.md",
    "docs/versioned/0.1.6",
    "docs/releases/v0.1.6",
    "release-notes/0.1.6",
)


def git(*args):
    return subprocess.check_output(["git", "-C", str(REPO), *args])


def resolve(ref, candidate):
    commit = git("rev-parse", "--verify", "--end-of-options", ref + "^{commit}").decode().strip()
    if not candidate:
        tagged = git("rev-parse", "--verify", TAG + "^{commit}").decode().strip()
        if commit != tagged:
            raise ValueError("release archives must resolve to the actual v0.1.6 tag")
    manifest = tomllib.loads(git("show", commit + ":Cargo.toml").decode())
    if manifest["workspace"]["package"]["version"] != "0.1.6":
        raise ValueError("selected commit does not have workspace version 0.1.6")
    return commit


def tracked(commit, paths=()):
    return set(git("ls-tree", "-r", "--name-only", "-z", commit, "--", *paths).decode().split("\0")) - {""}


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def asset_names(commit, candidate):
    stem = "layerfs-0.1.6" + ("-candidate-" + commit if candidate else "")
    return stem, [stem + ".tar.gz", stem + ".zip", stem + "-benchmark-data.tar.gz", "Cargo.lock", "LICENSE"]


def validate(output, commit, candidate):
    """Validate exact asset/checksum set and tracked archive members; never extract."""
    stem, names = asset_names(commit, candidate)
    if {path.name for path in output.iterdir()} != set(names + ["SHA256SUMS"]):
        raise ValueError("output must contain exactly the six specified assets")
    expected_sums = "".join(f"{digest(output / name)}  {name}\n" for name in names)
    if (output / "SHA256SUMS").read_text() != expected_sums:
        raise ValueError("SHA256SUMS mismatch")
    source = {stem + "/" + name for name in tracked(commit)}
    evidence = {stem + "/" + name for name in tracked(commit, EVIDENCE)}
    for filename, expected in ((names[0], source), (names[2], evidence)):
        with tarfile.open(output / filename, "r:gz") as archive:
            members = [member.name for member in archive if not member.isdir()]
        if len(members) != len(expected) or set(members) != expected:
            raise ValueError(f"tracked member mismatch: {filename}")
    with zipfile.ZipFile(output / names[1]) as archive:
        members = [member.filename for member in archive.infolist() if not member.is_dir()]
        if len(members) != len(source) or set(members) != source or archive.testzip():
            raise ValueError("ZIP members or CRC mismatch")
    for name in ("Cargo.lock", "LICENSE"):
        if (output / name).read_bytes() != git("show", commit + ":" + name):
            raise ValueError(f"standalone asset differs from commit: {name}")
    return {
        "commit": commit,
        "candidate": candidate,
        "assets": 6,
        "source_members": len(source),
        "evidence_members": len(evidence),
        "validation": "PASS",
        "raw_evidence_scope": "selected tracked reports/registries; the untracked local raw receipts under benchmark-results/ remain external",
        "checksums": {name: digest(output / name) for name in names},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="fresh output directory (existing directory only with --verify)")
    parser.add_argument("--ref", default=TAG, help="git ref; default actual v0.1.6 tag")
    parser.add_argument("--candidate", action="store_true", help="allow an untagged commit; label source archives as candidates")
    parser.add_argument("--verify", action="store_true", help="only validate an existing six-asset directory")
    args = parser.parse_args()
    commit = resolve(args.ref, args.candidate)
    if not args.verify:
        stem, names = asset_names(commit, args.candidate)
        for path in EVIDENCE:
            if not tracked(commit, (path,)):
                raise ValueError(f"required evidence scope missing at commit: {path}")
        args.output.mkdir(parents=True, exist_ok=False)
        for filename, fmt, paths in ((names[0], "tar.gz", ()), (names[1], "zip", ()),
                                     (names[2], "tar.gz", EVIDENCE)):
            subprocess.run(["git", "-C", str(REPO), "archive", "--format=" + fmt,
                            "--prefix=" + stem + "/", "--output=" + str((args.output / filename).resolve()),
                            commit, "--", *paths], check=True)
        for name in ("Cargo.lock", "LICENSE"):
            (args.output / name).write_bytes(git("show", commit + ":" + name))
        (args.output / "SHA256SUMS").write_text("".join(f"{digest(args.output / name)}  {name}\n" for name in names))
    print(json.dumps(validate(args.output, commit, args.candidate), indent=2))


if __name__ == "__main__":
    main()
