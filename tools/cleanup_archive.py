"""Deduplicated, raw Git-object recovery for selected development artifacts."""

import gzip
import hashlib
import json
import os
from pathlib import Path
import stat

from cleanup_state import Refused, git, inventory
from cleanup_attrs import attributes


HASH_BATCH_BYTES = 256 * 1024 * 1024


def file_hashes(repo, files, write=False):
    """Bound Git work by both file count and bytes; large files stand alone."""
    result, batch, batch_bytes = [], [], 0

    def flush():
        flags = ["-w"] if write else []
        data = "".join(str(path) + "\n" for path in batch).encode()
        result.extend(
            git(repo, "hash-object", "--no-filters", *flags, "--stdin-paths", data=data).decode().splitlines()
        )

    for path in files:
        size = path.stat().st_size
        if batch and (len(batch) >= 4096 or batch_bytes + size > HASH_BATCH_BYTES):
            flush()
            batch, batch_bytes = [], 0
        batch.append(path)
        batch_bytes += size
    if batch:
        flush()
    return result


def archive(repo, path, output, pass_id, checkout=False):
    """Keep every file, symlink and empty-directory record; never use the real index."""
    rows, snapshot = inventory(path, checkout)
    file_rows = [r for r in rows if not stat.S_ISDIR(r["mode"])]
    head = None
    if checkout:
        # Unchanged tracked source is already recoverable from the pinned HEAD.
        # Archive raw edits, untracked files and ignored helpers, rather than
        # re-encoding tens of thousands of immutable tracked files per checkout.
        head = git(path, "rev-parse", "HEAD").decode().strip()
        git(repo, "cat-file", "-e", head + "^{tree}")
        selected = set(
            os.fsdecode(value).rstrip("/")
            for value in (
                git(path, "ls-files", "--others", "-z")
                + git(path, "diff", "HEAD", "--name-only", "-z", "--no-ext-diff", "--no-textconv")
            ).split(b"\0")
            if value
        )
        kept = []
        for row in file_rows:
            relative = Path(row["path"])
            if (
                ".git" in relative.parts
                or row["path"] in selected
                or any(str(parent) in selected for parent in relative.parents)
            ):
                kept.append(row)
            else:
                row["from_head"] = True
        file_rows = kept
    regular = [r for r in file_rows if r["target"] is None]

    # For a single file, wrap it under its original leaf name in the recovery tree.
    def name(row):
        raw = path.name if row["path"] == "." else row["path"]
        # Git's index rejects .git path components. Store nested repository
        # metadata under an encoded name and retain the original mapping.
        parts = []
        for part in Path(raw).parts:
            if part.lower() == ".git":
                part = ".cleanup-nested-git-" + hashlib.sha256(raw.encode()).hexdigest()[:20]
            parts.append(part)
        result = str(Path(*parts))
        if result != raw:
            row["archive_path"] = result
        return result

    def source(row):
        return path if row["path"] == "." else path / row["path"]

    hashes = file_hashes(repo, [source(r) for r in regular], write=True)
    if len(hashes) != len(regular):
        raise Refused("Archive blob count mismatch")
    for row, oid in zip(regular, hashes):
        row["oid"] = oid
    for row in file_rows:
        if row["target"] is not None:
            row["oid"] = git(repo, "hash-object", "-w", "--stdin", data=os.fsencode(row["target"])).decode().strip()
    sizes = {r["oid"]: r["size"] for r in file_rows}
    checks = (
        git(repo, "cat-file", "--batch-check", data="".join(oid + "\n" for oid in sizes).encode()).decode().splitlines()
    )
    if len(checks) != len(sizes):
        raise Refused("Archive object count mismatch")
    for line in checks:
        oid, kind, size = line.split()
        if kind != "blob" or int(size) != sizes[oid]:
            raise Refused("Archive blob type/size mismatch")
    key = hashlib.sha256(str(path).encode()).hexdigest()[:20]
    index = output / (key + ".index")
    env = dict(os.environ, GIT_INDEX_FILE=str(index))
    try:
        git(repo, "read-tree", "--empty", env=env)
        entries = []
        for row in file_rows:
            mode = "120000" if row["target"] is not None else ("100755" if row["mode"] & stat.S_IXUSR else "100644")
            entries.append(f"{mode} {row['oid']}\t{name(row)}\0")
        git(repo, "update-index", "-z", "--index-info", data="".join(entries).encode(), env=env)
        tree = git(repo, "write-tree", env=env).decode().strip()
    finally:
        if index.exists():
            index.unlink()
    expected = {name(r): r["oid"] for r in file_rows}
    if len(expected) != len(file_rows):
        raise Refused("Recovery path encoding collision")
    actual = {}
    for entry in git(repo, "ls-tree", "-r", "-z", tree).split(b"\0"):
        if entry:
            meta, raw_name = entry.split(b"\t", 1)
            actual[os.fsdecode(raw_name)] = meta.split()[2].decode()
    if expected != actual:
        raise Refused("Incomplete archive tree")
    # Re-read every regular file after archiving. Hashing is raw, without filters.
    again = file_hashes(repo, [source(r) for r in regular])
    if again != hashes or inventory(path, checkout)[1] != snapshot:
        raise Refused("Source changed during archival; original preserved")
    reference = f"refs/cleanup/{pass_id}/{key}"
    git(repo, "update-ref", reference, tree)
    if git(repo, "rev-parse", reference).decode().strip() != tree:
        raise Refused("Recovery reference did not resolve")
    # Preserve filesystem attributes in the verified manifest rather than
    # refusing harmless provenance/quarantine attributes on every checkout.
    for row in rows:
        row["xattrs_base64"] = attributes(source(row))
    if inventory(path, checkout)[1] != snapshot:
        raise Refused("Source metadata changed during attribute capture")
    manifest = {
        "version": 2 if checkout else 1,
        "original_path": str(path),
        "repository": str(repo),
        "recovery_ref": reference,
        "tree": tree,
        "rows": rows,
        "tracked_head": head,
        "snapshot": snapshot,
        "limits": "Raw bytes, symlinks, modes, ownership IDs and times recorded. "
        "Git archive does not restore empty dirs, metadata or ACLs automatically. "
        "Restore xattrs_base64 with base64 decoding and os.setxattr. "
        "For worktrees, restore tracked_head and staged/unstaged patches first, "
        "then overlay raw archived edits/untracked/ignored files. Rows marked "
        "from_head are backed by the pinned commit, not duplicated in this tree.",
    }
    raw = json.dumps(manifest, sort_keys=True).encode()
    dest = output / (key + ".json.gz")
    with gzip.open(dest, "wb", compresslevel=1) as stream:
        stream.write(raw)
    with gzip.open(dest, "rb") as stream:
        if stream.read() != raw:
            raise Refused("Archive manifest readback mismatch")
    return {
        "manifest": str(dest),
        "manifest_sha256": hashlib.sha256(raw).hexdigest(),
        "recovery_ref": reference,
        "tree": tree,
        "snapshot": snapshot,
        "tracked_head": head,
    }
