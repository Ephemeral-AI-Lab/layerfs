"""Once-only untimed deployment into an acknowledged owned SDK container.

The same sealed file is copied as /code/r7_deployment.py for independent
container verification. No product API, image build, network fetch or retry.
"""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import resource
import secrets
import stat
import subprocess
import time

WINDOW = 65536
WALL_STOP_SECONDS = 300
TREE_SCHEMA = "r7-deployment-tree-v1"
PLAN_SCHEMA = "r7-container-deployment-v1"


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


@contextmanager
def original_file(path, mode, phase):
    """One selected evidence owner; close cannot replace an earlier cause."""
    try:
        stream = Path(path).open(mode)
    except BaseException as error:
        if not hasattr(error, "original_phase"):
            error.original_phase = phase + "_open"
        raise
    original = None
    try:
        yield stream
    except BaseException as error:
        original = error
        if not hasattr(error, "original_phase"):
            error.original_phase = phase
        raise
    finally:
        try:
            stream.close()
        except BaseException as closing:
            if original is None:
                if not hasattr(closing, "original_phase"):
                    closing.original_phase = phase + "_close"
                raise
            original.independent_close_failures = [*getattr(original, "independent_close_failures", []), str(closing)]


def digest(path):
    value = hashlib.sha256()
    with original_file(path, "rb", "deployment_digest_read") as stream:
        for block in iter(lambda: stream.read(WINDOW), b""):
            value.update(block)
    return value.hexdigest()


def write_new(path, value):
    with original_file(path, "x", "deployment_evidence_write") as stream:
        json.dump(value, stream, sort_keys=True, indent=2)
        stream.write("\n")


def owned_input(path):
    supplied = Path(path).absolute()
    require(".." not in supplied.parts, "owned deployment path has parent traversal")
    owned = Path("/tmp").resolve(strict=True)
    if supplied.parts[:2] == ("/", "tmp"):
        relative = supplied.relative_to("/tmp")
    else:
        require(owned in supplied.parents, "deployment input must be under owned /tmp")
        relative = supplied.relative_to(owned)
    actual = owned
    for part in relative.parts:
        actual /= part
        require(not actual.is_symlink(), "owned deployment input traverses a symlink")
    actual = actual.resolve(strict=True)
    require(owned in actual.parents and any(part.startswith("layerfs-r7-") for part in relative.parts),
            "deployment copy inputs must be this stage's owned /tmp paths")
    return actual


def target_root(value, role):
    require(isinstance(value,str) and value.startswith("/") and len(Path(value).parts) == 2 and value == str(Path(value)),
            "deployment roots must be canonical top-level container paths")
    allowed = value in {"/code", "/replay"} if role == "asset" else (
        value == "/native" or value.startswith("/native-")) if role == "native" else (
        value == "/p" or value.startswith("/p-"))
    require(allowed, "protected or unsupported deployment target: " + value)
    return value


def metadata(info):
    kind = "file" if stat.S_ISREG(info.st_mode) else "directory" if stat.S_ISDIR(info.st_mode) else "symlink" if stat.S_ISLNK(info.st_mode) else "unsupported"
    require(kind != "unsupported", "unsupported deployment inode kind")
    return dict(kind=kind, mode=stat.S_IMODE(info.st_mode), uid=info.st_uid, gid=info.st_gid, mtime_ns=info.st_mtime_ns)


def stable(info):
    return (info.st_dev,info.st_ino,info.st_mode,info.st_uid,info.st_gid,info.st_size,info.st_mtime_ns,info.st_ctime_ns,info.st_nlink)


def file_hash(path):
    before = path.lstat()
    descriptor = os.open(path,os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    value = hashlib.sha256()
    total = 0
    original = None
    try:
        opened = os.fstat(descriptor)
        require(stat.S_ISREG(opened.st_mode) and stable(opened) == stable(before), "regular deployment file identity changed before read")
        while True:
            block = os.read(descriptor,WINDOW)
            if not block:
                break
            total += len(block)
            value.update(block)
        require(stable(opened) == stable(os.fstat(descriptor)) == stable(path.lstat()) and total == opened.st_size,
                "regular deployment file changed during full-byte validation")
    except BaseException as error:
        original = error
        raise
    finally:
        try:
            os.close(descriptor)
        except OSError as closing:
            if original is None:
                raise
            original.independent_close_failures = [*getattr(original, "independent_close_failures", []), str(closing)]
    return value.hexdigest(), total


def entries(root):
    yield ".", root
    stack = [(root,os.scandir(root),root.lstat())]
    try:
        while stack:
            directory, iterator, before = stack[-1]
            try:
                item = next(iterator)
            except StopIteration:
                iterator.close()
                stack.pop()
                require(stable(before) == stable(directory.lstat()), "directory changed during deployment inventory")
                continue
            path = directory / item.name
            yield path.relative_to(root).as_posix(),path
            if stat.S_ISDIR(path.lstat().st_mode):
                stack.append((path,os.scandir(path),path.lstat()))
    finally:
        for _,iterator,_ in stack:
            iterator.close()


def within(root, relative):
    require(relative == "." or isinstance(relative,str) and bool(relative) and relative == Path(relative).as_posix() and not Path(relative).is_absolute() and ".." not in Path(relative).parts,
            "invalid deployment inventory relative path")
    path = root if relative == "." else root / relative
    parent = root if relative == "." else path.parent.resolve(strict=True)
    require(parent == root or root in parent.parents, "deployment inventory parent symlink escapes root")
    return path


class SetDigest:
    """Same fixed-state canonical fingerprint as the retained fixture seal."""
    def __init__(self):
        self.count = self.total = self.xor = 0

    def add(self, relative, value):
        raw = json.dumps(dict(path=relative,**value),sort_keys=True,separators=(",",":")).encode()
        number = int.from_bytes(hashlib.sha256(b"r7-fixture-row-v1\0" + raw).digest(),"big")
        self.count += 1
        self.total = (self.total+number) % (1 << 256)
        self.xor ^= number

    def finish(self):
        return hashlib.sha256(b"r7-fixture-set-v1\0" + str(self.count).encode() + b"\0" + self.total.to_bytes(32,"big") + self.xor.to_bytes(32,"big")).hexdigest()


def seal_tree(root, output, *, fixture_rows=None, fixture_rows_sha256=None, expected_set=None):
    root = owned_input(root)
    require(root.is_dir() and not root.is_symlink(), "owned deployment source must be an actual directory")
    output = Path(output)
    resolved = output.parent.resolve(strict=True) / output.name
    require(resolved != root and root not in resolved.parents, "deployment inventory must be outside source root")
    start = time.monotonic_ns()
    fingerprint = SetDigest()
    counts = dict(entries=0,regular_files=0,regular_bytes=0,source_payload_bytes_read=0)
    aliases = {}
    def projected():
        require(fixture_rows_sha256 and digest(fixture_rows) == fixture_rows_sha256, "closed fixture raw seal hash missing or changed")
        with Path(fixture_rows).open() as stream:
            header = json.loads(next(stream))
            require(header.get("schema") == "r7-fixture-seal-rows-v1" and Path(header["copy"]).resolve(strict=True) == root,
                    "retained fixture seal does not select this owned copy")
            completed = False
            for line in stream:
                row = json.loads(line)
                if row.get("event") == "completed":
                    require(row.get("status") == "PASS", "retained fixture seal did not pass")
                    completed = True
                else:
                    require(not completed and row.get("match") is True and row.get("copy"), "unqualified retained fixture row")
                    yield row["path"],within(root,row["path"]),row["copy"]
            require(completed, "retained fixture seal missing original completion")
    selected = projected() if fixture_rows else ((relative,path,None) for relative,path in entries(root))
    initial = root.lstat()
    with output.open("xb") as sink:
        def emit(row):
            sink.write((json.dumps(row,sort_keys=True) + "\n").encode())
        emit(dict(schema=TREE_SCHEMA,root=str(root),projection="retained closed copy metadata/content" if fixture_rows else "new owned input seal",
                  fixture_rows_sha256=fixture_rows_sha256,read_window_bytes=WINDOW))
        seen = set()
        for relative,path,expected in selected:
            require(relative not in seen, "duplicate deployment input path")
            seen.add(relative)
            info = path.lstat()
            value = metadata(info)
            if value["kind"] == "file":
                value["size"] = info.st_size
                value["content_sha256"] = expected["content_sha256"] if expected else file_hash(path)[0]
                counts["source_payload_bytes_read"] += 0 if expected else info.st_size
                counts["regular_files"] += 1
                counts["regular_bytes"] += info.st_size
            elif value["kind"] == "symlink":
                value["target_hex"] = os.fsencode(os.readlink(path)).hex()
            if value["kind"] != "directory" and info.st_nlink > 1:
                group = aliases.setdefault((info.st_dev,info.st_ino),[0,info.st_nlink])
                group[0] += 1
            require(expected is None or expected == value, "owned source differs from retained supported metadata: " + relative)
            fingerprint.add(relative,value)
            counts["entries"] += 1
            emit(dict(path=relative,metadata=value,identity=dict(device=info.st_dev,inode=info.st_ino,nlink=info.st_nlink)))
        require(all(members == links for members,links in aliases.values()), "owned deployment source has external hardlink aliases")
        require(stable(initial) == stable(root.lstat()), "owned source root changed during seal")
        if fixture_rows:
            require(sum(1 for _ in entries(root)) == counts["entries"], "owned copy name inventory differs from retained fixture seal")
        set_sha = fingerprint.finish()
        require(expected_set is None or expected_set == set_sha, "closed fixture metadata/content set seal differs")
        emit(dict(event="completed",status="SEALED_SETUP_ONLY",content_metadata_set_sha256=set_sha,**counts))
    return dict(schema=TREE_SCHEMA,status="SEALED_SETUP_ONLY",root=str(root),inventory=str(output),sha256=digest(output),
                content_metadata_set_sha256=set_sha,setup_ns=time.monotonic_ns()-start,
                setup_ru_maxrss=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                resource_domain="input setup only; never product/measurement",**counts)


def verify_tree(root, inventory, *, reconcile=False):
    root = Path(root)
    role = "asset" if str(root) in {"/code","/replay"} else "native"
    target_root(str(root),role)
    require(root.resolve(strict=True) == root and stat.S_ISDIR(root.lstat().st_mode), "actual deployed root must not alias another path")
    fingerprint = SetDigest()
    counts = dict(entries=0,regular_files=0,regular_bytes=0,allocated_regular_bytes=0,metadata_actions=0)
    source_groups, target_groups, seen = {}, {}, set()
    code_hashes = {}
    expected_footer = None
    with Path(inventory).open() as stream:
        header = json.loads(next(stream))
        require(header.get("schema") == TREE_SCHEMA, "deployment tree inventory schema")
        for line in stream:
            row = json.loads(line)
            if row.get("event") == "completed":
                require(expected_footer is None and row["status"] == "SEALED_SETUP_ONLY", "deployment input completion invalid")
                expected_footer = row
                continue
            require(expected_footer is None and row["path"] not in seen, "duplicate or trailing deployment inventory entry")
            relative, want = row["path"],row["metadata"]
            seen.add(relative)
            path = within(root,relative)
            actual = path.lstat()
            require(metadata(actual)["kind"] == want["kind"], "deployed kind mismatch: " + relative)
            if reconcile:
                if (actual.st_uid,actual.st_gid) != (want["uid"],want["gid"]):
                    os.chown(path,want["uid"],want["gid"],follow_symlinks=False)
                    counts["metadata_actions"] += 1
                actual = path.lstat()
                if want["kind"] != "symlink" and stat.S_IMODE(actual.st_mode) != want["mode"]:
                    os.chmod(path,want["mode"],follow_symlinks=False)
                    counts["metadata_actions"] += 1
                actual = path.lstat()
                if actual.st_mtime_ns != want["mtime_ns"]:
                    os.utime(path,ns=(actual.st_atime_ns,want["mtime_ns"]),follow_symlinks=False)
                    counts["metadata_actions"] += 1
            actual = path.lstat()
            got = metadata(actual)
            if want["kind"] == "file":
                content, size = file_hash(path)
                got.update(size=size,content_sha256=content)
                counts["regular_files"] += 1
                counts["regular_bytes"] += size
                counts["allocated_regular_bytes"] += actual.st_blocks*512
                if str(root) == "/code":
                    code_hashes[relative] = content
            elif want["kind"] == "symlink":
                got["target_hex"] = os.fsencode(os.readlink(path)).hex()
            if want["kind"] != "directory" and (row["identity"]["nlink"] > 1 or actual.st_nlink > 1):
                source_key = (row["identity"]["device"],row["identity"]["inode"])
                actual_key = (actual.st_dev,actual.st_ino)
                group = source_groups.setdefault(source_key,[actual_key,0,row["identity"]["nlink"],actual.st_nlink])
                require(group[0] == actual_key and target_groups.setdefault(actual_key,source_key) == source_key,
                        "deployed hardlink equivalence mismatch: " + relative)
                group[1] += 1
            require(got == want, "deployed full-byte/supported-metadata mismatch: " + relative + "; want=" + repr(want) + "; got=" + repr(got))
            fingerprint.add(relative,got)
            counts["entries"] += 1
    require(expected_footer is not None and "." in seen, "deployment inventory incomplete or root missing")
    require(all(members == expected_links == actual_links for _,members,expected_links,actual_links in source_groups.values()),
            "deployed hardlink classes incomplete or externally aliased")
    require(sum(1 for _ in entries(root)) == counts["entries"] == expected_footer["entries"], "deployed complete name inventory mismatch")
    require(fingerprint.finish() == expected_footer["content_metadata_set_sha256"], "deployed content/metadata set mismatch")
    info = root.lstat()
    return dict(schema="r7-deployment-verification-v1",status="PASS",root=str(root),root_device=info.st_dev,root_inode=info.st_ino,
                inventory_sha256=digest(inventory),content_metadata_set_sha256=fingerprint.finish(),helper_sha256=code_hashes,
                supported_metadata=["kind","mode","uid","gid","mtime_ns","symlink target","internal hardlink equivalence"],
                ctime_and_cross_filesystem_inode_equality="NOT_CLAIMED",setup_ru_maxrss=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                resource_domain="container input setup/verification only; never daemon/measurement",**counts)


def load_plan(config, selection):
    setup = config.get("container_setup",{})
    require(setup.get("wall_stop_seconds") == WALL_STOP_SECONDS, "prospective staging wall stop must be exactly 300 seconds")
    require(setup.get("implementation_sha256") == digest(__file__), "sealed staging implementation identity missing or changed")
    manifest = owned_input(setup["manifest"])
    require(digest(manifest) == setup.get("manifest_sha256"), "sealed container deployment manifest changed")
    plan = json.loads(manifest.read_text())
    require(plan.get("schema") == PLAN_SCHEMA and plan.get("image_id") == config["identities"]["image_id"] and
            (plan.get("uid"),plan.get("gid")) == (config["uid"],config["gid"]), "deployment plan image/user identity mismatch")
    require(all(key in plan for key in ("assets","native","passthrough_mounts")), "deployment plan must explicitly retain selected and unselected asset/native/mount lists")
    assets, natives = plan.get("assets",[]),plan.get("native",[])
    require(isinstance(assets,list) and assets and isinstance(natives,list), "sealed deployment assets/native inventory missing")
    require(selection["arm"] != "L" or not natives, "L must not materialize a native Workspace root")
    expected_native = config.get("native_peer_roots",[config["native_root"]]) if selection["case_id"].startswith("W") else [config["native_root"]]
    require(selection["arm"] == "L" or [item["target"] for item in natives] == expected_native, "actual native deployment roots differ from selected peers")
    targets = []
    bootstrap = False
    code_paths = set()
    for role,items in (("asset",assets),("native",natives)):
        for item in items:
            source = owned_input(item["source"])
            require(source.is_dir(), "deployment source must be a prepared owned directory")
            target = target_root(item["target"],role)
            require(target not in targets, "overlapping or duplicate deployment roots")
            targets.append(target)
            inventory = owned_input(item["inventory"])
            require(digest(inventory) == item["inventory_sha256"], "sealed deployment tree inventory changed")
            require(inventory != source and source not in inventory.parents, "deployment inventory must not be copied as fixture contents")
            with inventory.open() as stream:
                header = json.loads(next(stream))
                require(header.get("schema") == TREE_SCHEMA and Path(header["root"]).resolve(strict=True) == source, "deployment inventory/source mismatch")
                completed = None
                for line in stream:
                    row = json.loads(line)
                    encoded = row.get("metadata",{})
                    require(encoded.get("kind") != "symlink" or encoded.get("mode") == 0o777,
                            "UNSUPPORTED_LINUX_SYMLINK_MODE: " + target + "/" + row.get("path", "") +
                            "; sealed mode=" + str(encoded.get("mode")) + "; native Linux representation requires 0777; no normalization or copy attempted")
                    if row.get("event") == "completed":
                        require(completed is None and row.get("status") == "SEALED_SETUP_ONLY", "deployment input seal not complete")
                        completed = row
                    else:
                        require(completed is None, "deployment input seal has trailing unsealed rows")
                    if target == "/code" and row.get("metadata",{}).get("kind") == "file":
                        code_paths.add(row["path"])
                    if target == "/code" and row.get("path") == "r7_deployment.py":
                        bootstrap = row["metadata"].get("content_sha256") == setup["implementation_sha256"]
                require(completed and completed.get("content_metadata_set_sha256") == item.get("content_metadata_set_sha256"),
                        "declared complete deployment content/metadata set seal missing or changed")
    require(bootstrap, "sealed /code/r7_deployment.py bootstrap identity missing")
    required_helpers = {"r7_deployment.py","residency.py","stream_manifest.py","generate_manifest.py","workload.py","oracle.py","changes.py"}
    if config.get("oracle_variant") == "r7-git-index-scoped-v2":
        required_helpers.add("git_index_oracle.py")
    if selection["case_id"] in {"E11","E19"}:
        required_helpers.add("tracked-paths.json")
    if selection["case_id"] in {"E12","E13","E14","K02","K03"}:
        required_helpers.add("node-roots.json")
        require("/replay" in targets, "selected closed replay artifacts missing")
    if selection["case_id"] == "E15":
        required_helpers.add("largest-path")
    if selection["arm"] == "P":
        require(config["passthrough_inside"].startswith("/code/"), "P binary must be a sealed deployed /code file")
        required_helpers.add(config["passthrough_inside"][len("/code/"):])
    require(required_helpers <= code_paths, "selected static helpers/assets missing: " + ", ".join(sorted(required_helpers-code_paths)))
    mounts = plan.get("passthrough_mounts",[])
    expected_mounts = config.get("passthrough_peer_mounts",[config.get("passthrough_mount")]) if selection["case_id"].startswith("W") else [config.get("passthrough_mount")]
    require(selection["arm"] != "P" or mounts == expected_mounts, "P prepared mountpoints differ from selected peers")
    require(selection["arm"] == "P" or not mounts, "unselected P mounts must not be prepared")
    for mount in mounts:
        target_root(mount,"mount")
    require(len(set(mounts)) == len(mounts), "duplicate P mountpoint")
    return plan


def stage(config, selection, container, output):
    require(isinstance(container,str) and len(container) == 64 and all(character in "0123456789abcdef" for character in container), "exact acknowledged owned container ID required")
    output = Path(output)
    output.mkdir(exist_ok=False)
    start = time.monotonic_ns()
    deadline = time.monotonic()+WALL_STOP_SECONDS
    calls = []
    result = dict(schema="r7-container-staging-v1",status="INCOMPLETE",container=container,attempts=1,setup_wall_stop_seconds=WALL_STOP_SECONDS,
                  deployment_manifest_sha256=config["container_setup"]["manifest_sha256"],implementation_sha256=config["container_setup"]["implementation_sha256"],
                  source_identities=config["identities"],image_id=config["identities"]["image_id"],uid=config["uid"],gid=config["gid"],product_attempts=0,
                  operations=calls,native_manifests=[],auxiliary_manifests=[],verifications=[],helper_sha256={},automatic_retry=False,container_stop="NOT_ATTEMPTED")
    docker = ["docker","--host","unix://"+config["socket"]]
    def invoke(label,arguments):
        require(time.monotonic() < deadline, "declared staging wall stop expired before next distinct action")
        sequence = len(calls)+1
        argv = docker+arguments
        admission = dict(sequence=sequence,label=label,argv=argv,container=container,attempts=1,
                         engine_operation_identity="UNAVAILABLE through Docker CLI; no guessed Exec ID")
        write_new(output / f"{sequence:03}-{label}.attempt.json",admission)
        calls.append(admission)
        with original_file(output/f"{sequence:03}-{label}.stdout", "xb", label + "_stdout") as stdout, original_file(output/f"{sequence:03}-{label}.stderr", "xb", label + "_stderr") as stderr:
            before = time.monotonic_ns()
            completed = subprocess.run(argv,stdout=stdout,stderr=stderr,timeout=max(0.001,deadline-time.monotonic()))
            after = time.monotonic_ns()
            admission.update(exit_code=completed.returncode,start_ns=before,end_ns=after,duration_ns=after-before)
            require(completed.returncode == 0, "original staging operation failed: " + label)
        write_new(output/f"{sequence:03}-{label}.completion.json",admission)
        return output/f"{sequence:03}-{label}.stdout"
    evidence = "/tmp/layerfs-r7-setup-"+secrets.token_hex(16)
    original = None
    try:
        plan = load_plan(config,selection)
        roots = [item["target"] for item in plan["assets"]+plan["native"]]
        # No mounts exist yet. Exact top-level targets and fresh evidence only.
        bootstrap = "import json,os,sys; v=json.loads(sys.argv[1]); assert all(not os.path.lexists(p) for p in v['roots']+v['mounts']); os.mkdir(v['evidence'],0o700); [os.mkdir(p,0o755) for p in v['mounts']]"
        invoke("prepare",["exec","--user","0:0",container,"python3","-c",bootstrap,json.dumps(dict(roots=roots,mounts=plan["passthrough_mounts"],evidence=evidence))])
        for index,item in enumerate(plan["assets"]+plan["native"]):
            invoke("copy-"+str(index),["cp","-a",str(owned_input(item["source"])),container+":"+item["target"]])
            inside = evidence+"/expected-"+str(index)+".jsonl"
            invoke("inventory-"+str(index),["cp",str(owned_input(item["inventory"])),container+":"+inside])
            item["inside_inventory"] = inside
        actual_bootstrap = invoke("bootstrap-sha",["exec","--user","0:0",container,"sha256sum","/code/r7_deployment.py"])
        require(actual_bootstrap.read_text().split()[0] == config["container_setup"]["implementation_sha256"], "deployed bootstrap bytes differ; never execute unverified setup helper")
        for index,item in enumerate(plan["assets"]+plan["native"]):
            actual = invoke("verify-"+str(index),["exec","--user","0:0",container,"python3","-B","/code/r7_deployment.py","verify-tree",
                                                "--root",item["target"],"--inventory",item["inside_inventory"],"--reconcile-metadata"])
            verified = json.loads(actual.read_text())
            require(verified.get("status") == "PASS" and verified.get("root") == item["target"] and verified.get("inventory_sha256") == item["inventory_sha256"], "actual deployment verification incomplete")
            result["verifications"].append(verified)
            result["helper_sha256"].update(verified.get("helper_sha256",{}))
        for index,item in enumerate(plan["native"]):
            inside = evidence+"/native-"+str(index)+".files.jsonl"
            generated = invoke("native-manifest-"+str(index),["exec","--user","0:0",container,"python3","-B","/code/generate_manifest.py","--root",item["target"],"--output",inside])
            manifest = json.loads(generated.read_text())
            require(manifest.get("status") == "SEALED_SETUP_ONLY" and manifest.get("root") == item["target"] and manifest.get("manifest") == inside,
                    "actual native inode manifest generation incomplete")
            copied = output/("native-"+str(index)+".files.jsonl")
            invoke("retain-manifest-"+str(index),["cp",container+":"+inside,str(copied)])
            require(digest(copied) == manifest["sha256"], "actual native manifest transfer changed bytes")
            result["native_manifests"].append(manifest)
        for item in plan["assets"]:
            if item["target"] != "/replay":
                continue
            inside = evidence+"/replay.files.jsonl"
            generated = invoke("replay-manifest",["exec","--user","0:0",container,"python3","-B","/code/generate_manifest.py","--root","/replay","--output",inside])
            manifest = json.loads(generated.read_text())
            require(manifest.get("status") == "SEALED_SETUP_ONLY" and manifest.get("root") == "/replay" and manifest.get("manifest") == inside,
                    "actual replay inode manifest generation incomplete")
            copied = output/"replay.files.jsonl"
            invoke("retain-replay-manifest",["cp",container+":"+inside,str(copied)])
            require(digest(copied) == manifest["sha256"], "actual replay manifest transfer changed bytes")
            result["auxiliary_manifests"].append(manifest)
        result["status"] = "PASS"
    except BaseException as error:
        original = error
        result.update(status="FAILED_SETUP",original_error=str(error),original_error_type=type(error).__name__,
                      original_phase=getattr(error,"original_phase",None),
                      independent_close_failures=getattr(error,"independent_close_failures",[]),
                      failure_custody="exact container retained; lead explicit disposition only; no replay or inferred drain")
    result.update(setup_start_ns=start,setup_end_ns=time.monotonic_ns(),host_setup_process_ru_maxrss=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                  host_ru_maxrss_scope="controller lifetime high-water, platform native units; never daemon/phase peak",
                  resource_domain="untimed deployment/verification only; copy and full-byte verification warmth never prove cold eligibility")
    result["setup_ns"] = result["setup_end_ns"]-start
    try:
        write_new(output/"receipt.json",result)
    except BaseException as output_error:
        if original is None:
            original = output_error
            result.update(status="FAILED_SETUP",original_error=str(original),original_error_type=type(original).__name__,
                          original_phase=getattr(original,"original_phase",None))
        else:
            original.independent_output_failures = [*getattr(original,"independent_output_failures",[]), str(output_error)]
    if original is not None:
        original.staging_receipt = result
        original.staging_output = str(output)
        raise original
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command",required=True)
    seal = commands.add_parser("seal-tree")
    seal.add_argument("--root",type=Path,required=True)
    seal.add_argument("--output",type=Path,required=True)
    seal.add_argument("--fixture-rows",type=Path)
    seal.add_argument("--fixture-rows-sha256")
    seal.add_argument("--expected-set-sha256")
    verify = commands.add_parser("verify-tree")
    verify.add_argument("--root",type=Path,required=True)
    verify.add_argument("--inventory",type=Path,required=True)
    verify.add_argument("--reconcile-metadata",action="store_true")
    args = parser.parse_args()
    if args.command == "seal-tree":
        result = seal_tree(args.root,args.output,fixture_rows=args.fixture_rows,fixture_rows_sha256=args.fixture_rows_sha256,expected_set=args.expected_set_sha256)
    else:
        result = verify_tree(args.root,args.inventory,reconcile=args.reconcile_metadata)
    print(json.dumps(result,sort_keys=True))


if __name__ == "__main__":
    main()
