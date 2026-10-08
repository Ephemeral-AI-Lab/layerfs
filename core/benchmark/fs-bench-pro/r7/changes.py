"""Declared E19 related-root preparation on an owned copy or actual mount.

Never invokes Git in the source checkout. This is setup, not the timed command;
mounted changes require a known Commit before a fresh-mount E19 measurement.
"""
import argparse
import hashlib
import json
from pathlib import Path

PROTECTED_FIXTURE = Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-harness")
CHANGE = b"// experiment 305 changed between mounts\n"


def prepare(root, tracked):
    root = Path(root).resolve()
    if root == PROTECTED_FIXTURE or root.is_relative_to(PROTECTED_FIXTURE):
        raise ValueError("fixture checkout is protected; use an owned copy")
    regular = []
    for relative in sorted(tracked):
        path = Path(relative)
        if path.is_absolute() or ".." in path.parts:
            raise ValueError("tracked path escapes owned root")
        candidate = root / path
        if not candidate.resolve().is_relative_to(root):
            raise ValueError("tracked path escapes owned root through symlink")
        if candidate.is_file() and not candidate.is_symlink():
            regular.append(relative)
    if len(regular) != 14090:
        raise ValueError("E19 full fixture tracked regular count differs from declared 14090")
    selected = regular[::50]
    if len(selected) != 282:
        raise ValueError("E19 changed file count differs from declared 282")
    # Check every in-place input before the first mutation: no fallback to a
    # shortened/reduced E19 treatment after finding an empty source.
    if any((root / relative).stat().st_size == 0 for relative in selected[1::2]):
        raise ValueError("E19 in-place selected source is empty")
    for index, relative in enumerate(selected):
        path = root / relative
        if index % 2 == 0:
            with path.open("ab") as stream:
                stream.write(CHANGE)
        else:
            with path.open("r+b") as stream:
                first = stream.read(1)
                stream.seek(0)
                stream.write(bytes([first[0] ^ 1]))
    return {"schema": "r7-e19-related-root-preparation-v1", "mode": "setup", "admission_eligible": False,
            "tracked_regular_files": len(regular), "changed_files": len(selected), "appended": 141,
            "altered_in_place": 141, "changed_paths_sha256": hashlib.sha256("\n".join(selected).encode()).hexdigest(),
            "construction_workers": 1, "requires_known_commit_if_mounted": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--tracked", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(prepare(args.root, json.loads(args.tracked.read_text())), sort_keys=True))


if __name__ == "__main__":
    main()
