"""Author fresh small code assets and a plan for one matched-cut SDK staging proof.

This performs setup authoring only. It invokes no Git, process, Docker, Store or
Init. Large native/replay trees are referenced through their existing seals,
never copied here. The lead supplies the independent Store volume and runs the
separate staging proof under its lock and declared wall stop.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import stat
import sys
import time

from prepare_inputs import owned, generated
from prepare_cut import copy_once, Evidence
from seal_fixture import metadata, stability

REPOSITORY = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPOSITORY / "core/benchmark/fs-bench-pro"))
from r7 import deployment

WINDOW = 65536
IMAGE = "sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6"
COUNTS = {
    "full": (119976, 103108, 3475776149),
    "minus": (34997, 31215, 1349267039),
    "replay": (85387, 71894, 2169235378),
}
STATIC = {"tracked-paths.json", "node-roots.json", "largest-path"}


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


class InputFile:
    """Stable one-open reader whose close never replaces an earlier cause."""
    def __init__(self, path):
        self.path = Path(path)
        self.before = self.path.lstat()
        require(stat.S_ISREG(self.before.st_mode), "input must be an actual regular file")
        descriptor = os.open(self.path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        self.stream = None
        try:
            self.stream = os.fdopen(descriptor, "rb", buffering=WINDOW)
            require(stability(self.before) == stability(os.fstat(self.stream.fileno())),
                    "input identity changed before original read")
        except BaseException as original:
            try:
                self.stream.close() if self.stream is not None else os.close(descriptor)
            except OSError as closing:
                original.independent_close_failures = [*getattr(original, "independent_close_failures", []), str(closing)]
            raise

    def __enter__(self):
        return self.stream

    def __exit__(self, kind, original, traceback):
        changed = None
        if original is None and self.before is not None:
            try:
                require(stability(self.before) == stability(os.fstat(self.stream.fileno())) == stability(self.path.lstat()),
                        "input changed during original read")
            except BaseException as error:
                changed = error
        try:
            self.stream.close()
        except OSError as closing:
            first = original if original is not None else changed
            if first is None:
                raise
            first.independent_close_failures = [*getattr(first, "independent_close_failures", []), str(closing)]
        if changed is not None:
            raise changed
        return False


def input_digest(path):
    digest = hashlib.sha256()
    with InputFile(path) as stream:
        for block in iter(lambda: stream.read(WINDOW), b""):
            digest.update(block)
    return digest.hexdigest()


def json_input(path):
    path = owned(path)
    with InputFile(path) as stream:
        raw = stream.read()
        value = json.loads(raw)
        digest = hashlib.sha256(raw).hexdigest()
    return path, value, digest


def write_record(path, value):
    with Evidence(path) as stream:
        stream.write((json.dumps(value, sort_keys=True, indent=2) + "\n").encode())


def sealed_tree(record, expected=None):
    """Validate closed namespace/metadata/identity, without rereading payloads."""
    root, inventory = owned(record["root"]), owned(record["inventory"])
    require(stat.S_ISDIR(root.lstat().st_mode), "closed tree root must be an actual directory")
    require(record.get("schema") == deployment.TREE_SCHEMA and record.get("status") == "SEALED_SETUP_ONLY",
            "closed tree seal is incomplete")
    require(input_digest(inventory) == record["sha256"], "closed tree inventory bytes changed")
    require(not inventory.is_relative_to(root), "closed inventory must be outside its source tree")
    fingerprint = deployment.SetDigest()
    rows = files = regular_bytes = symlinks = 0
    selected = {}
    names = {}
    footer = None
    original = root.lstat()
    with InputFile(inventory) as stream:
        header_raw = stream.readline(WINDOW + 1)
        require(len(header_raw) <= WINDOW, "inventory header exceeds 64 KiB setup window")
        header = json.loads(header_raw)
        require(header.get("schema") == deployment.TREE_SCHEMA and owned(header["root"]) == root,
                "closed tree header/root differs")
        while True:
            raw = stream.readline(WINDOW + 1)
            if not raw:
                break
            require(len(raw) <= WINDOW, "inventory row exceeds 64 KiB setup window")
            row = json.loads(raw)
            if row.get("event") == "completed":
                require(footer is None and not stream.read(1), "duplicate footer/trailing inventory bytes")
                footer = row
                break
            relative, value = row["path"], row["metadata"]
            require(relative not in names, "duplicate inventory path: " + str(relative))
            require(value["kind"] in {"file", "directory", "symlink"}, "unsupported closed input kind")
            require(relative != "." or value["kind"] == "directory", "closed inventory root row must be a directory")
            names[relative] = value["kind"]
            path = deployment.within(root, relative)
            info = path.lstat()
            require(dict(device=info.st_dev, inode=info.st_ino, nlink=info.st_nlink) == row["identity"],
                    "closed input physical identity changed: " + relative)
            require(metadata(info) == {key: value[key] for key in ("kind", "mode", "uid", "gid", "mtime_ns")},
                    "closed input supported metadata changed: " + relative)
            if value["kind"] == "file":
                require(info.st_size == value["size"], "closed input file length changed")
                files += 1
                regular_bytes += value["size"]
            elif value["kind"] == "symlink":
                symlinks += 1
                require(os.fsencode(os.readlink(path)).hex() == value["target_hex"], "closed input symlink target changed")
            fingerprint.add(relative, value)
            rows += 1
            if relative in STATIC or relative == "master.json":
                require(relative not in selected, "duplicate selected code/master input")
                selected[relative] = dict(path=path, metadata=value, stat=info)
    require(names.get(".") == "directory", "closed inventory root row missing")
    for name in names:
        for parent in Path(name).parents:
            require(names.get(parent.as_posix()) == "directory", "closed inventory parent row missing or non-directory")
    require(footer is not None and footer.get("status") == "SEALED_SETUP_ONLY", "closed inventory lacks completion")
    require(fingerprint.finish() == record["content_metadata_set_sha256"] == footer["content_metadata_set_sha256"],
            "closed content/metadata set differs")
    require((rows, files, regular_bytes) == (record["entries"], record["regular_files"], record["regular_bytes"]),
            "closed input cardinality/bytes differ")
    require((rows, files, regular_bytes) == (footer["entries"], footer["regular_files"], footer["regular_bytes"]),
            "closed footer cardinality/bytes differ")
    if expected is not None:
        require((rows, files, regular_bytes) == expected and symlinks == 0,
                "plan113 cut is not the exact sealed symlink-free selection")
    actual_count = 0
    for name, _ in deployment.entries(root):
        require(name in names, "actual input name outside closed inventory: " + name)
        del names[name]
        actual_count += 1
    require(not names and actual_count == rows and stability(original) == stability(root.lstat()),
            "closed input namespace/root changed")
    require(input_digest(inventory) == record["sha256"], "closed inventory changed during validation")
    return dict(root=root, inventory=inventory, selected=selected, record=record, stat=original)


def source_file(path, expected=None):
    path = Path(path)
    require(not path.is_symlink(), "code/binary source must not be a symlink")
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode), "code/binary source must be an actual regular file")
    digest = input_digest(path)
    require(stability(info) == stability(path.lstat()), "small source changed during sealing")
    require(expected is None or digest == expected, "pinned binary/source bytes differ")
    return dict(path=path, metadata=dict(size=info.st_size, content_sha256=digest), stat=info)


def tree_item(closed, target):
    record = closed["record"]
    return dict(source=str(closed["root"]), target=target, inventory=str(closed["inventory"]),
                inventory_sha256=record["sha256"], content_metadata_set_sha256=record["content_metadata_set_sha256"])


def author(args):
    start = time.monotonic_ns()
    output = owned(args.output, existing=False)
    require(not output.exists(), "exclusive staging-author output exists")
    template_path, template, template_sha = json_input(args.template_config)
    seals_path, seals, seals_sha = json_input(args.cut_seals)
    code_seals_path, code_seals, code_seals_sha = json_input(args.prepared_seals)
    require(template["identities"]["image_id"] == IMAGE and (template["uid"], template["gid"]) == (501, 20),
            "existing setup template image/user differs")
    require(args.volume.startswith("layerfs-r7-") and all(c.isalnum() or c in "-_." for c in args.volume)
            and args.volume != template["volume"], "new lead-owned independent Store volume required")
    require(args.volume_clone_identity, "lead's independent writable-clone receipt identity required")
    closed = {label: sealed_tree(seals[label], COUNTS[label]) for label in COUNTS}
    code_source = sealed_tree(code_seals["code"])
    require(STATIC <= set(code_source["selected"]), "closed static code assets missing")
    for root in [row["root"] for row in closed.values()] + [code_source["root"]]:
        require(not output.is_relative_to(root) and not root.is_relative_to(output), "new output overlaps an existing closed tree")
    master = closed["replay"]["selected"].get("master.json")
    require(master is not None and master["metadata"]["kind"] == "file", "closed replay master missing")
    require(input_digest(master["path"]) == master["metadata"]["content_sha256"], "closed replay master bytes changed")
    runtime_path = Path(args.runtime_binary or template["runtime_binary"])
    daemon_path = Path(args.daemon_binary or template["daemon_binary"])
    for path in (runtime_path, daemon_path, args.passthrough_binary):
        require(path.resolve(strict=True).is_relative_to(REPOSITORY / "core/target"), "binary must be the lead's first-party build artifact")
    runtime = source_file(runtime_path, args.runtime_sha256 or template["runtime_sha256"])
    daemon = source_file(daemon_path, args.daemon_sha256 or template["daemon_sha256"])
    manifest = owned(args.manifest or template["manifest"])
    manifest_sha = input_digest(manifest)
    require(manifest_sha == template["manifest_sha256"] == (args.manifest_sha256 or template["manifest_sha256"]),
            "existing empty installed manifest seal differs")
    sources = {name: REPOSITORY / "core/benchmark/fs-bench-pro/r7" / name for name in
               ("workload.py", "workloads.py", "oracle.py", "changes.py", "git_index_oracle.py", "git_queries.py")}
    sources.update({name: REPOSITORY / "core/benchmark/r7-cache" / name for name in
                    ("residency.py", "stream_manifest.py", "generate_manifest.py")})
    sources["r7_deployment.py"] = Path(deployment.__file__).resolve()
    sources["oracle_prepare.py"] = REPOSITORY / "core/benchmark/r7-tools/oracle_prepare.py"
    small = {name: source_file(path) for name, path in sources.items()}
    if args.git_default_policy is not None:
        require(args.git_default_policy_sha256 is not None, "qualified Git policy SHA required")
        small["git-default-policy.json"] = source_file(owned(args.git_default_policy), args.git_default_policy_sha256)
    small["r7-passthrough"] = source_file(args.passthrough_binary, args.passthrough_sha256)
    for name in STATIC:
        small[name] = code_source["selected"][name]
    output.mkdir()
    args.created_output = output
    code = output / "code"
    code.mkdir()
    copied = {}
    for name in sorted(small):
        item = small[name]
        count, digest = copy_once(item["path"], code / name, item["metadata"], item["stat"])
        generated(code / name, 501, 20, mode=0o755 if name == "r7-passthrough" else 0o644)
        copied[name] = dict(source=str(item["path"]), bytes=count, sha256=digest)
    generated(code, 501, 20, "directory", 0o755)
    code_inventory = output / "code.inventory.jsonl"
    actual_code = deployment.seal_tree(code, code_inventory)
    require(actual_code["regular_files"] == len(small), "fresh code seal cardinality differs")
    plan_path = output / "deployment-plan.json"
    plan = dict(schema=deployment.PLAN_SCHEMA, image_id=IMAGE, uid=501, gid=20,
                assets=[dict(source=str(code), target="/code", inventory=str(code_inventory),
                             inventory_sha256=actual_code["sha256"], content_metadata_set_sha256=actual_code["content_metadata_set_sha256"]),
                        tree_item(closed["replay"], "/replay")],
                native=[tree_item(closed["full"], "/native")], passthrough_mounts=[])
    write_record(plan_path, plan)
    config = dict(runtime_binary=str(runtime_path), runtime_sha256=runtime["metadata"]["content_sha256"],
                  daemon_binary=str(daemon_path), daemon_sha256=daemon["metadata"]["content_sha256"],
                  manifest=str(manifest), manifest_sha256=manifest_sha, socket=template["socket"],
                  volume=args.volume, volume_clone_identity=args.volume_clone_identity,
                  uid=501, gid=20, native_root="/native", passthrough_inside="/code/r7-passthrough",
                  passthrough_sha256=small["r7-passthrough"]["metadata"]["content_sha256"], oracle_variant="r7-git-index-scoped-v2",
                  identities=dict(image_id=IMAGE, claim="setup-only matched symlink-free cut; no performance/source qualification"),
                  container_setup=dict(manifest=str(plan_path), manifest_sha256=input_digest(plan_path),
                                       implementation_sha256=copied["r7_deployment.py"]["sha256"], wall_stop_seconds=300),
                  fixture_kind="plan113-symlink-free-cut", reduced_fixture_cut_identity=seals["full"]["content_metadata_set_sha256"],
                  store_profile="Disposable/WAL/OFF", overlay_profile="MEMORY/OFF/EXCLUSIVE",
                  durable="NOT_RUN — disabled by owner until explicit reauthorization")
    config_path = output / "staging-config.json"
    write_record(config_path, config)
    deployment.load_plan(config, dict(arm="N", case_id="E01", cache_class="A"))
    for path, digest in ((template_path, template_sha), (seals_path, seals_sha), (code_seals_path, code_seals_sha)):
        require(input_digest(path) == digest, "original input declaration changed during authoring")
    for row in closed.values():
        require(stability(row["stat"]) == stability(row["root"].lstat()), "closed large-root identity changed")
    require(input_digest(master["path"]) == master["metadata"]["content_sha256"], "replay master changed during authoring")
    receipt = dict(schema="r7-staging-input-author-v1", status="PREPARED_SETUP_ONLY", config=str(config_path),
                   config_sha256=input_digest(config_path), deployment_plan=str(plan_path),
                   deployment_plan_sha256=input_digest(plan_path), actual_code_seal=actual_code, copied_assets=copied,
                   input_declarations=dict(template=dict(path=str(template_path), sha256=template_sha),
                                           cuts=dict(path=str(seals_path), sha256=seals_sha),
                                           prepared=dict(path=str(code_seals_path), sha256=code_seals_sha)),
                   closed_large_roots={label: dict(source=str(row["root"]), inventory=str(row["inventory"]),
                                                  inventory_sha256=row["record"]["sha256"],
                                                  content_metadata_set_sha256=row["record"]["content_metadata_set_sha256"],
                                                  selected_for_transfer=label in {"full", "replay"}) for label, row in closed.items()},
                   replay_master_sha256=master["metadata"]["content_sha256"], replay_master_unchanged=True,
                   large_tree_bytes_copied=0, project_init_operations=0, Store_operations=0,
                   volume_clone_provenance="lead-supplied independent writable-clone receipt; author performs no Docker inspection",
                   source_written=False, automatic_retry=False, admission_eligible=False, performance_samples=0,
                   source_quiescence_required=True, native_snapshot=False,
                   setup_ns=time.monotonic_ns()-start, setup_ru_maxrss=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                   setup_ru_maxrss_units="bytes macOS; KiB Linux; process lifetime high-water",
                   resource_scope="small asset author/metadata validation only; never daemon or performance",
                   implementation_sha256=input_digest(__file__))
    write_record(output / "preparation-receipt.json", receipt)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--volume", required=True)
    parser.add_argument("--volume-clone-identity", required=True)
    parser.add_argument("--template-config", type=Path, default=Path("/tmp/layerfs-r7-staging-proof-config-20261009.json"))
    parser.add_argument("--cut-seals", type=Path, default=Path("/tmp/layerfs-r7-cut-root-seals-20261009.json"))
    parser.add_argument("--prepared-seals", type=Path, default=Path("/tmp/layerfs-r7-input-root-seals-20261009.json"))
    parser.add_argument("--runtime-binary", type=Path)
    parser.add_argument("--runtime-sha256")
    parser.add_argument("--daemon-binary", type=Path)
    parser.add_argument("--daemon-sha256")
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--manifest-sha256")
    parser.add_argument("--git-default-policy", type=Path)
    parser.add_argument("--git-default-policy-sha256")
    parser.add_argument("--passthrough-binary", type=Path, default=REPOSITORY / "core/target/r7-passthrough-release/release/layerfs-r7-passthrough")
    parser.add_argument("--passthrough-sha256", required=True)
    args = parser.parse_args()
    try:
        result = author(args)
    except Exception as error:
        result = dict(schema="r7-staging-input-author-v1", status="INCOMPLETE", output=str(args.output),
                      original_failure_type=type(error).__name__, original_failure=str(error),
                      independent_close_failures=getattr(error, "independent_close_failures", []),
                      source_written=False, automatic_retry=False, admission_eligible=False,
                      partial_outputs_retained=getattr(args, "created_output", None) is not None)
        if getattr(args, "created_output", None) is not None:
            try:
                write_record(args.created_output / "preparation-failure.json", result)
            except OSError as failure:
                result["independent_receipt_output_failure"] = str(failure)
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] == "PREPARED_SETUP_ONLY" else 1


if __name__ == "__main__":
    raise SystemExit(main())
