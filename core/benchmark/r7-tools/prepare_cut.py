"""Once-only author of the three plan113 symlink-free comparison/setup cuts.

Uses closed108 inventories; source paths are stage-owned temporary copies only.
No source mutation, normalization, process, Git, Store, fsync, retry or removal.
The lead runs setup under its lock/wall stop and independently seals outputs.
"""
import argparse
import hashlib
import json
from pathlib import Path
import os
import resource
import stat
import time

from prepare_inputs import owned, relative_path, apply_metadata, sha, write_json
from seal_fixture import entries, metadata, stability, SetDigest

WINDOW = 65536
DEFAULTS = {
    "full": ("/tmp/layerfs-r7-full-fixture-20261009", "/tmp/layerfs-r7-full-inventory-20261009.jsonl",
             "d058d8c61220950b989772681cee4446575893076318d128a25884849271ba52"),
    "minus": ("/tmp/layerfs-r7-prepared-inputs-20261009/full-minus-dependencies",
              "/tmp/layerfs-r7-full-minus-dependencies-inventory-20261009.jsonl",
              "0234bb4267b4d4b70a998b86ecb8454eec78cf812aa23adad9891c1014a512ec"),
    "replay": ("/tmp/layerfs-r7-prepared-inputs-20261009/replay", "/tmp/layerfs-r7-replay-inventory-20261009.jsonl",
               "a3fd1cd659e3d69a5f746ab06e7b0ab95c0fd8b1f928c3ddf689b3716b9e01dd"),
}
EXPECTED = {
    "full": dict(input_entries=130046, entries=119976, regular_files=103108, directories=16868,
                 omitted_symlinks=10070, regular_bytes=3475776149),
    "minus": dict(input_entries=35025, entries=34997, regular_files=31215, directories=3782,
                  omitted_symlinks=28, regular_bytes=1349267039),
    "replay": dict(input_entries=95429, entries=85387, regular_files=71894, directories=13493,
                   omitted_symlinks=10042, regular_bytes=2169235378),
}
NAMES = {"full": "full", "minus": "full-minus-dependencies", "replay": "replay"}


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def identity(info):
    return dict(device=info.st_dev, inode=info.st_ino, nlink=info.st_nlink)


class Evidence:
    """Unbuffered one-attempt evidence; closing never replaces an earlier cause."""
    def __init__(self, path):
        self.stream = path.open("xb", buffering=0)

    def __enter__(self):
        return self

    def write(self, raw):
        require(len(raw) <= WINDOW, "omitted evidence record exceeds bounded window")
        delivered = self.stream.write(raw)
        require(delivered == len(raw),
                f"short original omitted-evidence write: delivered={delivered}, requested={len(raw)}; no tail resend")

    def __exit__(self, kind, original, traceback):
        try:
            self.stream.close()
        except OSError as closing:
            if original is None:
                raise
            previous = getattr(original, "independent_close_failures", [])
            original.independent_close_failures = [*previous, str(closing)]
        return False


def copy_once(source, target, expected, pinned):
    """One source window and one unbuffered destination; never resend a short write."""
    descriptor = os.open(source, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    output, original, closing = None, None, []
    value, total = hashlib.sha256(), 0
    try:
        require(stat.S_ISREG(pinned.st_mode) and stability(pinned) == stability(os.fstat(descriptor)),
                "original regular identity changed before copy")
        output = target.open("xb", buffering=0)
        while True:
            block = os.read(descriptor, WINDOW)
            if not block:
                break
            value.update(block)
            total += len(block)
            delivered = output.write(block)
            require(delivered == len(block),
                    f"short original regular write: delivered={delivered}, requested={len(block)}; no tail resend")
            del block
        require(total == expected["size"] and value.hexdigest() == expected["content_sha256"],
                "original regular payload differs from closed inventory")
        require(stability(pinned) == stability(os.fstat(descriptor)) == stability(source.lstat()),
                "original source changed during copy")
    except BaseException as error:
        original = error
    finally:
        if output is not None:
            try:
                output.close()
            except OSError as error:
                closing.append(error)
        try:
            os.close(descriptor)
        except OSError as error:
            closing.append(error)
    if original is not None:
        original.independent_close_failures = [str(error) for error in closing]
        raise original
    if closing:
        closing[0].independent_close_failures = [str(error) for error in closing[1:]]
        raise closing[0]
    return total, value.hexdigest()


def validate_source(label, root, inventory, expected_sha):
    require(sha(inventory) == expected_sha, "closed inventory hash differs: " + label)
    rows, completed = {}, None
    fingerprint = SetDigest()
    counts = dict(input_entries=0, entries=0, regular_files=0, directories=0,
                  omitted_symlinks=0, regular_bytes=0)
    with inventory.open("rb") as stream:
        header_raw = stream.readline(WINDOW + 1)
        require(len(header_raw) <= WINDOW, "inventory header exceeds bounded window")
        header = json.loads(header_raw)
        require(header.get("schema") == "r7-deployment-tree-v1" and owned(header["root"]) == root,
                "closed inventory root/schema differs")
        while True:
            raw = stream.readline(WINDOW + 1)
            if not raw:
                break
            require(len(raw) <= WINDOW, "inventory record exceeds bounded window")
            row = json.loads(raw)
            if row.get("event") == "completed":
                require(completed is None and not stream.read(1), "duplicate footer/trailing inventory bytes")
                completed = row
                break
            name = row["path"]
            relative_path(name)
            require(name not in rows, "duplicate source name in inventory")
            value = row["metadata"]
            require(value["kind"] in {"file", "directory", "symlink"}, "unsupported source kind")
            rows[name] = dict(metadata=value, identity=row["identity"], row_sha256=hashlib.sha256(raw).hexdigest())
            fingerprint.add(dict(path=name, **value))
            counts["input_entries"] += 1
            if value["kind"] == "symlink":
                counts["omitted_symlinks"] += 1
            else:
                counts["entries"] += 1
                counts["regular_files" if value["kind"] == "file" else "directories"] += 1
                if value["kind"] == "file":
                    counts["regular_bytes"] += value["size"]
    require(completed is not None and completed.get("status") == "SEALED_SETUP_ONLY", "closed inventory completion missing")
    require(fingerprint.seal() == completed["content_metadata_set_sha256"], "closed inventory set seal differs")
    require(counts == EXPECTED[label], "prospectively selected cut cardinality/bytes differs")
    require(completed["entries"] == counts["input_entries"] and
            completed["regular_files"] == counts["regular_files"] and
            completed["regular_bytes"] == counts["regular_bytes"], "closed footer counts differ")
    require(rows.get(".", {}).get("metadata", {}).get("kind") == "directory", "source root missing")
    original_root = root.lstat()
    for name, row in rows.items():
        for parent in relative_path(name).parents:
            require(rows.get(parent.as_posix(), {}).get("metadata", {}).get("kind") == "directory",
                    "source inventory has a non-directory parent")
        path = root if name == "." else root / name
        info = path.lstat()
        value = row["metadata"]
        require(identity(info) == row["identity"], "source physical identity differs: " + name)
        require(metadata(info) == {key: value[key] for key in ("kind", "mode", "uid", "gid", "mtime_ns")},
                "source supported metadata differs: " + name)
        if value["kind"] == "file":
            require(info.st_size == value["size"], "source regular length differs")
        elif value["kind"] == "symlink":
            require(os.fsencode(os.readlink(path)).hex() == value["target_hex"], "source opaque symlink target differs")
    count = 0
    for name, _ in entries(root):
        require(name in rows, "unexpected source name outside closed inventory")
        count += 1
    require(count == len(rows) and stability(original_root) == stability(root.lstat()), "source namespace/root changed")
    return dict(root=root, inventory=inventory, sha256=expected_sha, rows=rows, counts=counts,
                input_set_sha256=completed["content_metadata_set_sha256"], root_stat=original_root)


def author(label, source, output):
    root = output / NAMES[label]
    root.mkdir()
    omitted_path = output / (NAMES[label] + ".omitted-symlinks.jsonl")
    omitted_digest, cut_fingerprint = hashlib.sha256(), SetDigest()
    directories, aliases = [], {}
    written = read = alias_count = 0
    with Evidence(omitted_path) as omitted:
        for name, row in source["rows"].items():
            value = row["metadata"]
            src = source["root"] if name == "." else source["root"] / name
            info = src.lstat()
            require(identity(info) == row["identity"], "original source identity changed after validation")
            require(metadata(info) == {key: value[key] for key in ("kind", "mode", "uid", "gid", "mtime_ns")},
                    "original source metadata changed after validation")
            if value["kind"] == "symlink":
                evidence = dict(path=name, metadata=value, identity=row["identity"],
                                original_inventory_row_sha256=row["row_sha256"],
                                original_inventory_sha256=source["sha256"])
                raw = (json.dumps(evidence, sort_keys=True) + "\n").encode()
                omitted.write(raw)
                omitted_digest.update(raw)
                continue
            target = root if name == "." else root / name
            cut_fingerprint.add(dict(path=name, **value))
            if value["kind"] == "directory":
                if name != ".":
                    target.mkdir()
                directories.append((target, value))
                continue
            key = (info.st_dev, info.st_ino)
            if info.st_nlink > 1 and key in aliases:
                original, digest, size, original_stat = aliases[key]
                require(digest == value["content_sha256"] and size == value["size"], "regular alias inventory payload differs")
                require(stability(info) == original_stat, "regular alias source changed after first copy")
                os.link(original, target)
                alias_count += 1
            else:
                size, digest = copy_once(src, target, value, info)
                read += size
                written += size
                require(stability(info) == stability(src.lstat()), "original source changed around the copy attempt")
                if info.st_nlink > 1:
                    aliases[key] = (target, digest, size, stability(info))
            apply_metadata(target, value)
            require((target.lstat().st_dev, target.lstat().st_ino) != (info.st_dev, info.st_ino),
                    "new cut file unexpectedly aliases source")
    for path, value in reversed(directories):
        apply_metadata(path, value)
    require(stability(source["root_stat"]) == stability(source["root"].lstat()), "original source root changed during authoring")
    require(sha(source["inventory"]) == source["sha256"], "closed original inventory changed during authoring")
    if label == "replay":
        master = source["rows"].get("master.json")
        require(master is not None and master["metadata"]["kind"] == "file", "closed replay master missing")
        require(sha(root / "master.json") == master["metadata"]["content_sha256"], "replay master bytes changed")
    return dict(source=str(source["root"]), root=str(root), source_inventory=str(source["inventory"]),
                source_inventory_sha256=source["sha256"], source_set_sha256=source["input_set_sha256"],
                expected_cut_set_sha256=cut_fingerprint.seal(), counts=source["counts"],
                omitted=dict(inventory=str(omitted_path), sha256=omitted_digest.hexdigest(),
                             rows=source["counts"]["omitted_symlinks"],
                             fields="exact original inventory row hash, physical identity, metadata and opaque target bytes"),
                regular_payload_bytes_read=read, regular_payload_bytes_written=written,
                destination_only_regular_aliases=alias_count, setup_directory_restore_rows=len(directories),
                setup_alias_groups=len(aliases), replay_master_unchanged=label == "replay")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for label, (root, inventory, digest) in DEFAULTS.items():
        parser.add_argument("--" + label + "-root", type=Path, default=Path(root))
        parser.add_argument("--" + label + "-inventory", type=Path, default=Path(inventory))
        parser.add_argument("--" + label + "-inventory-sha256", default=digest)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    start = time.monotonic_ns()
    created = None
    result = dict(schema="r7-representable-cut-preparation-v1", status="INCOMPLETE",
                  plan_identity="522238bd5", admission_eligible=False, source_written=False,
                  claim="distinct symlink-free comparison cuts and replay-payload cut; never original full fixture",
                  store_execution="NONE", git_invocations=0, fsync_calls=0, construction_workers=1,
                  durable="NOT_RUN — disabled by owner until explicit reauthorization")
    try:
        output = owned(args.output, existing=False)
        require(not output.exists(), "exclusive cut output already exists")
        sources = {}
        for label in DEFAULTS:
            root = owned(getattr(args, label + "_root"))
            result["phase"] = dict(kind="closed_source_validation", cut=label, source=str(root))
            require(not root.is_relative_to(output) and not output.is_relative_to(root), "output overlaps source")
            sources[label] = validate_source(label, root, owned(getattr(args, label + "_inventory")),
                                             getattr(args, label + "_inventory_sha256"))
        roots = [source["root"] for source in sources.values()]
        require(all(left != right and not left.is_relative_to(right) and not right.is_relative_to(left)
                    for index, left in enumerate(roots) for right in roots[index + 1:]), "original input roots overlap")
        result["inputs"] = {label: dict(root=str(source["root"]), inventory=str(source["inventory"]),
                                     inventory_sha256=source["sha256"], counts=source["counts"])
                            for label, source in sources.items()}
        output.mkdir()
        created = output
        result["cuts"], result["author_attempts"] = {}, []
        for label, source in sources.items():
            result["phase"] = dict(kind="original_cut_authoring", cut=label, source=str(source["root"]))
            result["author_attempts"].append(dict(cut=label, root=str(output / NAMES[label]), attempts=1))
            result["cuts"][label] = author(label, source, output)
        result.update(status="PREPARED_SETUP_ONLY", phase=dict(kind="completed"), read_window_bytes=WINDOW,
                      setup_inventory_rows_retained=sum(len(source["rows"]) for source in sources.values()),
                      source_quiescence_required=True, native_snapshot=False,
                      destination_verification="lead independently seals each output before staging/measurement",
                      reduced_E19="NOT_RUN — original tracked/E19 claims not transferred",
                      full_fixture_rows="retained separately; no replacement or relabelling")
    except Exception as error:
        result.update(original_failure_type=type(error).__name__, original_failure=str(error),
                      independent_close_failures=getattr(error, "independent_close_failures", []),
                      retained_partial_outputs=created is not None, retry=False, removal=False)
    result.update(setup_ns=time.monotonic_ns()-start,
                  setup_process_ru_maxrss=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                  setup_ru_maxrss_units="bytes macOS; KiB Linux; process lifetime high-water",
                  resource_scope="untimed cut setup only; input-sized metadata maps/copies never product/measurement")
    if created is not None:
        try:
            write_json(created / "cut-preparation-receipt.json", result)
        except OSError as error:
            result.update(status="INCOMPLETE", independent_receipt_output_failure=str(error))
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] == "PREPARED_SETUP_ONLY" else 1


if __name__ == "__main__":
    raise SystemExit(main())
