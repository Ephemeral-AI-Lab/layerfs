#!/usr/bin/env python3
"""Build and validate source-bound SDK edit benchmark assets (stdlib only)."""
import hashlib
import fcntl
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CONTRACT = "docs/roadmap/0.1/0.1.2/sdk-only-edit-benchmark-rebuild.md"
PRODUCT = tuple(f"crates/{name}/" for name in (
    "layerfs-content", "layerfs-daemon", "layerfs-layerstack-store", "layerfs-sdk",
    "layerfs-workspace", "layerfs-fuse", "layerfs-materialization", "layerfs-monitor",
))
HARNESS = "benchmark/fs-bench-pro/"
HARNESS_EXTRA = {"tools/test-fast.sh", "docs/roadmap/0.1/benchmarking.md",
                 "docs/roadmap/0.1/0.1.1/README.md",
                 "docs/roadmap/0.1/0.1.1/namespace-optimization-spec.md"}


def output(*command):
    return subprocess.check_output(command, cwd=REPO, text=True).strip()


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def write_json(path, value):
    Path(path).write_text(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n")


def seal(root):
    root = Path(root)
    lines = [f"{sha(path)}  {path.relative_to(root)}\n" for path in sorted(root.rglob("*"))
             if path.is_file() and path != root / "evidence.sha256"]
    (root / "evidence.sha256").write_text("".join(lines))


def verify_manifest(root, name="evidence.sha256", complete=True):
    root = Path(root).resolve()
    entries = {}
    for line in (root / name).read_text().splitlines():
        expected, relative = line.split(maxsplit=1)
        relative = relative.lstrip("* ").removeprefix("./")
        path = root / relative
        assert relative not in entries and root in path.resolve().parents and not path.is_symlink(), relative
        assert len(expected) == 64 and sha(path) == expected, relative
        entries[relative] = expected
    assert entries, "empty manifest"
    if complete:
        actual = {str(path.relative_to(root)) for path in root.rglob("*") if path.is_file()
                  and path != root / name}
        assert set(entries) == actual, "manifest completeness"
    return entries


def require_clean(revision=None):
    head = output("git", "rev-parse", "HEAD")
    if revision is not None:
        assert head == revision, "source revision changed"
    subprocess.run(["git", "diff-files", "--quiet"], cwd=REPO, check=True)
    subprocess.run(["git", "diff", "--cached", "--quiet"], cwd=REPO, check=True)
    assert output("git", "write-tree") == output("git", "rev-parse", "HEAD^{tree}"), "index tree"
    assert not output("git", "ls-files", "--others", "--exclude-standard", "--", "Cargo.toml",
                      "Cargo.lock", ".cargo", "crates", "tools", "benchmark/fs-bench-pro", "release-notes/0.1.2"), "untracked build/claim input"
    return head


def source_identity(revision):
    revision = output("git", "rev-parse", revision)
    records = subprocess.check_output(["git", "ls-tree", "-rz", revision], cwd=REPO).split(b"\0")
    blobs = {}
    for record in records:
        if not record:
            continue
        metadata, path = record.split(b"\t", 1)
        _, kind, oid = metadata.split()
        if kind == b"blob":
            blobs[path.decode()] = oid
    selected = sorted(path for path in blobs if path.startswith(PRODUCT + (HARNESS,))
                      or path in HARNESS_EXTRA | {"Cargo.toml", "Cargo.lock", CONTRACT,
                                                 "release-notes/0.1.2/generate_benchmark_tables.py"})
    batch = subprocess.check_output(["git", "cat-file", "--batch"], cwd=REPO,
                                    input=b"\n".join(blobs[path] for path in selected) + b"\n")
    values, cursor = {}, 0
    for path in selected:
        end = batch.index(b"\n", cursor)
        _, kind, length = batch[cursor:end].split()
        assert kind == b"blob"
        cursor = end + 1
        values[path] = batch[cursor:cursor + int(length)]
        cursor += int(length) + 1
    seals = {kind: hashlib.sha256() for kind in ("source", "product", "harness")}
    for path in selected:
        if "__pycache__" in Path(path).parts or Path(path).name == ".DS_Store":
            continue
        is_product = path.startswith(PRODUCT)
        is_harness = path.startswith(HARNESS) or path in HARNESS_EXTRA
        for kind, includes in (("source", is_product or is_harness or path in {"Cargo.toml", "Cargo.lock"}),
                               ("product", is_product), ("harness", is_harness)):
            if includes:
                seals[kind].update(path.encode() + b"\0" + values[path])
    digest = lambda path: hashlib.sha256(values[path]).hexdigest()
    return {"revision": revision, "tree": output("git", "rev-parse", f"{revision}^{{tree}}"),
            **{f"{kind}_seal": value.hexdigest() for kind, value in seals.items()},
            "cargo_lock_sha256": digest("Cargo.lock"),
            "workload_sha256": digest(HARNESS + "workload.rs"),
            "report_generator_sha256": digest(HARNESS + "generate-sdk-edit-report.py"),
            "custody_helper_sha256": digest(HARNESS + "sdk-edit-custody.py"),
            "release_generator_sha256": digest("release-notes/0.1.2/generate_benchmark_tables.py"),
            "contract_commit": output("git", "log", "-1", "--format=%H", revision, "--", CONTRACT),
            "contract_sha256": digest(CONTRACT),
            "preparation_compatibility_sha256": preparation_digest(values)}


def preparation_digest(values):
    relevant = ("crates/layerfs-content/", "crates/layerfs-layerstack-store/",
                "crates/layerfs-sdk/", "crates/layerfs-monitor/")
    selected = {path: value for path, value in values.items()
                if path.startswith(relevant) or path in {"Cargo.toml", "Cargo.lock", HARNESS + "families/sdk_edit_common.rs"}}
    main = values[HARNESS + "src/main.rs"]
    selected[HARNESS + "src/main.rs#preparation"] = main[main.index(b"fn sdk_edit_fixture_info("):main.index(b"fn sdk_edit_qualify(")]
    workload = values[HARNESS + "workload.rs"]
    selected[HARNESS + "workload.rs#sha256"] = workload[workload.index(b"pub(crate) struct Sha256"):]
    digest = hashlib.sha256()
    for path, value in sorted(selected.items()):
        digest.update(path.encode() + b"\0" + value)
    return digest.hexdigest()


def working_preparation_digest():
    paths = [REPO / "Cargo.toml", REPO / "Cargo.lock", REPO / HARNESS / "src/main.rs",
             REPO / HARNESS / "families/sdk_edit_common.rs", REPO / HARNESS / "workload.rs"]
    for name in ("layerfs-content", "layerfs-layerstack-store", "layerfs-sdk", "layerfs-monitor"):
        paths.extend(path for path in (REPO / "crates" / name).rglob("*") if path.is_file()
                     and "target" not in path.parts and "__pycache__" not in path.parts and path.name != ".DS_Store")
    return preparation_digest({str(path.relative_to(REPO)): path.read_bytes() for path in paths})


def prepared_key(expected, compatibility):
    value = {"cache_profile": "sdk-edit-prepared-store-cache-v1", "fixture": expected,
             "preparation_compatibility_sha256": compatibility, "journal_policy": "MEMORY-no-sidecars"}
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest(), value


def acquire_prepared(cache_root, binary, size, receipt_path, build_evidence=""):
    started = time.monotonic_ns()
    cache_root, receipt_path = Path(cache_root).resolve(), Path(receipt_path).resolve()
    expected = json.loads(output(binary, "sdk-edit-fixture-info", str(size)))
    compatibility = working_preparation_digest()
    key, key_data = prepared_key(expected, compatibility)
    cache_root.mkdir(parents=True, exist_ok=True)
    entry = cache_root / key
    build_ns, validation_ns, disposition, quarantine = 0, 0, "hit", None
    with (cache_root / f"{key}.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if entry.exists():
            validation_started = time.monotonic_ns()
            try:
                entries = verify_manifest(entry)
                metadata = json.loads((entry / "cache.json").read_text())
                assert metadata["key_data"] == key_data
                assert {path.name for path in (entry / "store").iterdir()} == {"store.sqlite", "branch-id"}
                assert entries["store/store.sqlite"] == metadata["store_sha256"]
                assert (entry / "store/store.sqlite").stat().st_size == metadata["store_bytes"]
                assert all(metadata["fixture"].get(field) == value for field, value in expected.items() if field != "generator_seed")
                if build_evidence:
                    assert metadata["producer"]["status"] == "pass", "unbound cache producer"
            except (AssertionError, OSError, ValueError, KeyError):
                quarantine = cache_root / f"{key}.invalid-{uuid.uuid4().hex}"
                entry.rename(quarantine)
                disposition = "rebuilt-invalid"
            validation_ns += time.monotonic_ns() - validation_started
        if not entry.exists():
            if disposition == "hit":
                disposition = "miss"
            stage = Path(tempfile.mkdtemp(prefix=f".{key}.prepare-", dir=cache_root))
            build_started = time.monotonic_ns()
            result = None
            try:
                command = [str(binary), "sdk-edit-prepare", str(stage / "store"), str(size)]
                with (stage / "fixture.json").open("wb") as stdout, (stage / "prepare.stderr.txt").open("wb") as stderr:
                    result = subprocess.run(command, cwd=REPO, stdout=stdout, stderr=stderr, timeout=30)
                build_ns = time.monotonic_ns() - build_started
                validation_started = time.monotonic_ns()
                assert result.returncode == 0, "prepared Store process failed"
                fixture = json.loads((stage / "fixture.json").read_text())
                assert all(fixture.get(field) == value for field, value in expected.items() if field != "generator_seed"), "prepared fixture identity"
                assert {path.name for path in (stage / "store").iterdir()} == {"store.sqlite"}, "prepared Store sidecars"
                with (stage / "store/store.sqlite").open("rb") as stream:
                    assert stream.read(16) == b"SQLite format 3\0", "Store file format"
                (stage / "store/branch-id").write_text(fixture["branch_id"] + "\n")
                if build_evidence:
                    producer = validate_build(build_evidence, binary, json.loads((Path(build_evidence) / "build.json").read_text())["revision"])
                    assert producer["preparation_compatibility_sha256"] == compatibility
                    shutil.copytree(build_evidence, stage / "producer-build")
                else:
                    producer = {"revision": output("git", "rev-parse", "HEAD"), "binary_sha256": sha(binary), "status": "unbound-selected"}
                metadata = {"schema": "fs-bench-pro-sdk-edit-prepared-v1", "key": key, "key_data": key_data,
                            "producer": producer, "command": command, "exit_code": result.returncode,
                            "store_bytes": (stage / "store/store.sqlite").stat().st_size,
                            "store_sha256": sha(stage / "store/store.sqlite"), "fixture": fixture}
                write_json(stage / "cache.json", metadata)
                seal(stage)
                verify_manifest(stage)
                for path in stage.rglob("*"):
                    path.chmod(0o555 if path.is_dir() else 0o444)
                stage.chmod(0o555)
                os.replace(stage, entry)
                validation_ns += time.monotonic_ns() - validation_started
            except BaseException as error:
                failure = receipt_path.parent / f"preparation-failure-{size}-{uuid.uuid4().hex}"
                failure.mkdir()
                for name in ("fixture.json", "prepare.stderr.txt"):
                    if (stage / name).is_file():
                        shutil.copy2(stage / name, failure / name)
                write_json(failure / "context.json", {"schema":"fs-bench-pro-sdk-edit-preparation-failure-v1",
                           "command":[str(binary),"sdk-edit-prepare",str(stage/"store"),str(size)],
                           "timeout":isinstance(error,subprocess.TimeoutExpired),"error":str(error),
                           "exit_code":124 if isinstance(error,subprocess.TimeoutExpired) else result.returncode if result is not None else None,
                           "elapsed_ns":time.monotonic_ns()-build_started,"cache_key":key,"status":"fail"})
                seal(failure)
                # Only this exact unpublished staging directory is disposable.
                if stage.exists():
                    for path in stage.rglob("*"):
                        path.chmod(0o755 if path.is_dir() else 0o644)
                    stage.chmod(0o755)
                    shutil.rmtree(stage)
                raise
        metadata = json.loads((entry / "cache.json").read_text())
        if (entry / "producer-build").exists():
            shutil.copytree(entry / "producer-build", receipt_path.parent / f"prepared-source-{size}")
        receipt = {**metadata, "cache_path": str(entry), "prepared_path": str(entry / "store"),
                   "cache_disposition": disposition, "cache_manifest_sha256": sha(entry / "evidence.sha256"),
                   "cache_build_ns": build_ns, "cache_validation_ns": validation_ns,
                   "cache_acquisition_ns": time.monotonic_ns() - started,
                   "quarantined_path": str(quarantine) if quarantine else None,
                   "cache_profile": "sdk-edit-prepared-store-cache-v1", "status": "pass"}
        write_json(receipt_path, receipt)
    print(str(entry / "store"))


def clone_prepared(source, target, expected_sha256):
    started = time.monotonic_ns()
    source, target = Path(source), Path(target)
    assert source.is_file() and not target.exists(), "pristine clone paths"
    method = "apfs-clone"
    result = subprocess.run(["cp", "-c", str(source), str(target)], stderr=subprocess.DEVNULL)
    if result.returncode:
        if target.exists():
            target.unlink()
        shutil.copyfile(source, target)
        method = "byte-copy"
    target.chmod(0o600)
    assert not os.path.samefile(source, target), "hard-linked prepared Store"
    cloned_sha256 = sha(target)
    assert cloned_sha256 == expected_sha256, "pristine clone digest"
    return {"clone_method": method, "clone_wall_ns": time.monotonic_ns() - started,
            "clone_store_sha256": cloned_sha256, "prepared_store_sha256": expected_sha256,
            "clone_bytes": target.stat().st_size, "hard_link": False, "status": "pass"}


def cache_self_check(binary):
    binary = str(Path(binary).resolve())
    with tempfile.TemporaryDirectory(prefix="layerfs-sdk-cache-check-") as temporary:
        root = Path(temporary)
        processes = []
        try:
            cache = root / "cache"
            for index in (1, 2):
                processes.append(subprocess.Popen([sys.executable, str(Path(__file__).resolve()), "prepare",
                    str(cache), binary, "1048576", str(root / f"receipt-{index}.json")], stdout=subprocess.DEVNULL))
            assert all(process.wait(timeout=30) == 0 for process in processes), "concurrent builders"
            left, right = [json.loads((root / f"receipt-{index}.json").read_text()) for index in (1, 2)]
            assert {left["cache_disposition"], right["cache_disposition"]} == {"hit", "miss"}
            assert left["store_sha256"] == right["store_sha256"] and left["key"] == right["key"]
            entry = Path(left["cache_path"])
            master = entry / "store/store.sqlite"
            first = root / "sample-first.sqlite"
            clone_prepared(master, first, left["store_sha256"])
            with first.open("r+b") as stream:
                stream.write(b"sample mutation")
            assert sha(master) == left["store_sha256"], "sample changed master"
            clone_prepared(master, root / "sample-next.sqlite", left["store_sha256"])
            abandoned = cache / f".{left['key']}.prepare-interrupted"
            abandoned.mkdir()
            (abandoned / "store.sqlite").write_bytes(b"incomplete")
            acquire_prepared(cache, binary, 1048576, root / "after-interruption.json")
            assert json.loads((root / "after-interruption.json").read_text())["cache_disposition"] == "hit"
            changed = dict(left["key_data"]["fixture"], mtime_seconds=1)
            assert prepared_key(changed, left["key_data"]["preparation_compatibility_sha256"])[0] != left["key"]
            assert prepared_key(left["key_data"]["fixture"], "changed-initialization")[0] != left["key"]
            master.chmod(0o644)
            with master.open("r+b") as stream:
                stream.write(b"corrupt")
            acquire_prepared(cache, binary, 1048576, root / "after-corruption.json")
            repaired = json.loads((root / "after-corruption.json").read_text())
            assert repaired["cache_disposition"] == "rebuilt-invalid" and Path(repaired["quarantined_path"]).exists()
            entry.rename(cache / f"{left['key']}.removed-for-test")
            acquire_prepared(cache, binary, 1048576, root / "after-missing.json")
            assert json.loads((root / "after-missing.json").read_text())["cache_disposition"] == "miss"
            failing = root / "failed-prepare.py"
            failing.write_text("#!/usr/bin/env python3\nimport subprocess,sys\n"
                f"if sys.argv[1]=='sdk-edit-fixture-info': raise SystemExit(subprocess.run([{binary!r},*sys.argv[1:]]).returncode)\n"
                "print('synthetic preparation failure',file=sys.stderr)\nraise SystemExit(23)\n")
            failing.chmod(0o755)
            try:
                acquire_prepared(root / "failed-cache", str(failing), 1048576, root / "failed-receipt.json")
                raise AssertionError("failed preparation was accepted")
            except AssertionError as error:
                assert str(error) == "prepared Store process failed"
            failure = next(root.glob("preparation-failure-*"))
            assert json.loads((failure / "context.json").read_text())["exit_code"] == 23
            assert "synthetic preparation failure" in (failure / "prepare.stderr.txt").read_text()
            verify_manifest(failure)
        finally:
            for process in processes:
                if process.poll() is None:
                    process.kill()
                    process.wait()
            for path in root.rglob("*"):
                path.chmod(0o755 if path.is_dir() else 0o644)
    print(json.dumps({"schema": "fs-bench-pro-sdk-edit-cache-self-check-v1", "status": "pass",
                      "checks": ["concurrent-single-publication", "cache-hit", "independent-clones",
                                 "sample-master-isolation", "next-sample-pristine", "interrupted-staging-ignored",
                                 "metadata-invalidation", "source-invalidation", "corrupt-quarantine", "missing-rebuild",
                                 "failed-preparation-exit-and-logs-retained"]}))


def validate_image(image, identity):
    labels = image["Config"]["Labels"]
    expected = {"org.opencontainers.image.revision": identity["revision"],
                "org.opencontainers.image.source-tree": identity["tree"],
                "dev.layerfs.source-dirty": "false", "dev.layerfs.source-seal": identity["source_seal"],
                "dev.layerfs.product-seal": identity["product_seal"],
                "dev.layerfs.workload-source-sha256": identity["workload_sha256"]}
    assert all(labels.get(key) == value for key, value in expected.items()), "image labels"
    assert image["Os"] == "linux" and image["Id"].startswith("sha256:"), "image identity"


def host_identity():
    docker = json.loads(output("docker", "version", "--format", "{{json .}}"))
    info = json.loads(output("docker", "info", "--format", "{{json .}}"))
    cpu = output("sysctl", "-n", "machdep.cpu.brand_string") if sys.platform == "darwin" else platform.processor()
    return {"schema": "fs-bench-pro-sdk-edit-environment-v1", "os": platform.platform(),
            "architecture": platform.machine(), "cpu": cpu,
            "docker_client_version": docker["Client"]["Version"], "docker_server_version": docker["Server"]["Version"],
            "runtime": {key: info.get(key) for key in ("OperatingSystem", "OSType", "Architecture", "NCPU", "MemTotal", "Driver", "CgroupVersion")},
            "projection": "real-fuse-authenticated-daemon", "host_store": True,
            "cache_policy": "fresh-worker-and-container-pristine-store-clone; OS cache not flushed",
            "process_clock": "CLOCK_MONOTONIC_RAW", "daemon_clock": "sampler-relative-Instant",
            "clock_max_uncertainty_ns": 1_000_000, "sample_max_gap_ns": 1_000_000,
            "clock_offset_admission_ns": 400_000, "clock_rate_allowance_ppm": 1000,
            "calibration_max_age_ns": 2_000_000_000, "sampler_settle_ns": 2_000_000,
            "phase_watchdog_ns": 2_000_000_000, "worker_watchdog_ns": 10_000_000_000,
            "cgroup_sampler_threads": 2, "swap_allowed": False}


def build_configuration():
    rustc = output("rustc", "-Vv")
    target = next(line.removeprefix("host: ") for line in rustc.splitlines() if line.startswith("host: "))
    names = {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
             "RUSTUP_TOOLCHAIN", "CARGO_HOME", "RUSTUP_HOME", "CARGO_BUILD_TARGET", "CARGO_BUILD_RUSTFLAGS"}
    environment = {key:value for key,value in os.environ.items() if key in names or key.startswith("CARGO_PROFILE_")
                   or key.startswith("CARGO_TARGET_") and key.endswith(("_RUSTFLAGS", "_LINKER", "_RUNNER"))}
    directories = [REPO, *REPO.parents]
    paths = {directory / ".cargo" / filename for directory in directories for filename in ("config", "config.toml")}
    cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))
    paths.update(cargo_home / filename for filename in ("config", "config.toml"))
    return {"rustc":rustc,"cargo":output("cargo","-V"),"target":target,
            "rustc_path":shutil.which("rustc"),"cargo_path":shutil.which("cargo"),
            "environment":environment,"cargo_config_sha256":{str(path):sha(path) for path in sorted(paths) if path.is_file()},
            "profile":"release","locked":True,"target_directory":"repository-target"}


def build(destination, image_tag):
    revision = require_clean()
    identity = source_identity(revision)
    configuration = build_configuration()
    destination = Path(destination).resolve()
    assert not destination.exists(), "build destination exists"
    evidence = destination / "evidence"
    evidence.mkdir(parents=True)
    commands = []

    def run(name, command):
        started = time.monotonic_ns()
        with (evidence / f"{name}.stdout.txt").open("wb") as stdout, (evidence / f"{name}.stderr.txt").open("wb") as stderr:
            result = subprocess.run(command, cwd=REPO, stdout=stdout, stderr=stderr)
        commands.append({"name": name, "argv": command, "exit_code": result.returncode,
                         "elapsed_ns": time.monotonic_ns() - started})
        write_json(evidence / "commands.json", commands)
        if result.returncode:
            seal(evidence)
            raise SystemExit(result.returncode)

    run("host-build", ["cargo", "build", "--locked", "--release", "--target", configuration["target"], "--target-dir", str(REPO / "target"), "-p", "fs-benchmark-pro"])
    for test in ("group_4_invalid_type_range_overflow_and_limits_are_atomic",
                 "group_5_commit_publication_is_exactly_once_and_retry_is_up_to_date"):
        run(test, ["cargo", "test", "--locked", "-p", "layerfs-workspace", "--test", "file_edit", test, "--", "--exact"])
    command = ["docker", "build", "-f", HARNESS + "Dockerfile.layerfs", "-t", image_tag]
    for key, value in {"LAYERFS_SOURCE_COMMIT": revision, "LAYERFS_SOURCE_TREE": identity["tree"],
                       "LAYERFS_SOURCE_DIRTY": "false", "LAYERFS_SOURCE_SEAL": identity["source_seal"],
                       "LAYERFS_PRODUCT_SEAL": identity["product_seal"], "WORKLOAD_SOURCE_SHA256": identity["workload_sha256"]}.items():
        command += ["--build-arg", f"{key}={value}"]
    run("image-build", command + ["."])
    image = json.loads(output("docker", "image", "inspect", image_tag))[0]
    validate_image(image, identity)
    run("image-binaries", ["docker", "run", "--rm", "--entrypoint", "sha256sum", image["Id"],
                           "/usr/local/bin/layerfs-daemon", "/usr/local/bin/layerfs-fuse", "/usr/local/bin/fs-benchmark-workload"])
    image_binaries = {path: digest for digest, path in
                      (line.split() for line in (evidence / "image-binaries.stdout.txt").read_text().splitlines())}
    assert len(image_binaries) == 3 and all(len(digest) == 64 for digest in image_binaries.values())
    require_clean(revision)
    assert build_configuration() == configuration, "build configuration changed"
    binary = destination / "fs-benchmark-pro"
    shutil.copy2(REPO / "target" / configuration["target"] / "release/fs-benchmark-pro", binary)
    write_json(evidence / "image.json", image)
    receipt = {"schema": "fs-bench-pro-sdk-edit-build-v1", **identity,
               "binary_sha256": sha(binary), "image_id": image["Id"], "image_tag": image_tag,
               "image_binaries_sha256": image_binaries,
               "rustc": output("rustc", "-Vv"), "cargo": output("cargo", "-V"),
               "build_configuration":configuration,
               "commands_sha256": sha(evidence / "commands.json"), "status": "pass"}
    write_json(evidence / "build.json", receipt)
    seal(evidence)
    verify_manifest(evidence)
    print(json.dumps(receipt, sort_keys=True))


def validate_build(evidence, binary, revision):
    evidence = Path(evidence)
    verify_manifest(evidence)
    receipt = json.loads((evidence / "build.json").read_text())
    expected = source_identity(revision)
    assert receipt["schema"] == "fs-bench-pro-sdk-edit-build-v1" and receipt["status"] == "pass"
    assert all(receipt.get(key) == value for key, value in expected.items()), "build source identity"
    assert sha(binary) == receipt["binary_sha256"], "build binary identity"
    assert sha(evidence / "commands.json") == receipt["commands_sha256"]
    commands = json.loads((evidence / "commands.json").read_text())
    assert len(commands) == 4 and all(command["exit_code"] == 0 for command in commands)
    validate_image(json.loads((evidence / "image.json").read_text()), receipt)
    return receipt


def capture(run_dir, baseline_bin, candidate_bin, baseline_revision, candidate_revision,
            baseline_build, candidate_build, mode):
    root = Path(run_dir)
    images = json.loads((root / "environment/image.json").read_text())
    assert len(images) == 2
    head = output("git", "rev-parse", "HEAD")
    data = {"schema": "fs-bench-pro-sdk-edit-source-identity-v1", "current_revision": head,
            "current_tree": output("git", "rev-parse", "HEAD^{tree}"),
            "contract_commit": output("git", "log", "-1", "--format=%H", "--", CONTRACT),
            "contract_sha256": sha(REPO / CONTRACT), "baseline_binary_sha256": sha(baseline_bin),
            "candidate_binary_sha256": sha(candidate_bin),
            "report_generator_sha256": sha(REPO / HARNESS / "generate-sdk-edit-report.py"),
            "custody_helper_sha256": sha(Path(__file__)),
            "release_generator_sha256": sha(REPO / "release-notes/0.1.2/generate_benchmark_tables.py"),
            "workload_sha256": sha(REPO / HARNESS / "workload.rs"),
            "timed_module_sha256": sha(REPO / HARNESS / "src/sdk_file_edit.rs"),
            "baseline_binary_path": str(Path(baseline_bin).resolve()),
            "candidate_binary_path": str(Path(candidate_bin).resolve()),
            "source_policy": "authentic-directional" if mode == "admission" else "selected-non-admission"}
    for arm, binary, revision, build_path, image in zip(
            ("baseline", "candidate"), (baseline_bin, candidate_bin),
            (baseline_revision, candidate_revision), (baseline_build, candidate_build), images):
        if mode == "admission":
            assert build_path, f"{arm} build evidence required"
            receipt = validate_build(build_path, binary, revision)
            validate_image(image, receipt)
            assert image["Id"] == receipt["image_id"], f"{arm} built image"
            shutil.copytree(build_path, root / f"environment/build-{arm}")
            data[arm] = receipt
            data[f"{arm}_build_manifest_sha256"] = sha(Path(build_path) / "evidence.sha256")
        else:
            data[arm] = {"revision": revision or head, "binary_sha256": sha(binary),
                         "image_id": image["Id"], "status": "unbound-selected"}
    if mode == "admission":
        require_clean(candidate_revision)
        assert baseline_revision != candidate_revision
        for key in ("harness_seal", "workload_sha256", "report_generator_sha256", "custody_helper_sha256", "release_generator_sha256", "contract_sha256", "rustc", "cargo", "build_configuration"):
            assert data["baseline"][key] == data["candidate"][key], f"paired {key}"
        assert data["contract_sha256"] == data["candidate"]["contract_sha256"]
        assert data["report_generator_sha256"] == data["candidate"]["report_generator_sha256"]
        changed = output("git", "diff", "--name-only", baseline_revision, candidate_revision).splitlines()
        assert changed and all(path.startswith(PRODUCT) for path in changed), "product-only treatment"
        data["treatment_paths"] = changed
        (root / "environment/treatment.patch").write_text(output("git", "diff", baseline_revision, candidate_revision) + "\n")
        data["treatment_sha256"] = sha(root / "environment/treatment.patch")
        data["harness_diff"] = "none"
    else:
        data["harness_diff"] = "unbound-selected"
    write_json(root / "environment/source-identity.json", data)
    write_json(root / "environment/host-runtime.json", host_identity())


def finalize(run_dir):
    root = Path(run_dir)
    start = json.loads((root / "environment/source-identity.json").read_text())
    if start["source_policy"] == "authentic-directional":
        require_clean(start["candidate"]["revision"])
        end = source_identity("HEAD")
        assert all(start["candidate"][key] == value for key, value in end.items()), "ending source identity"
        for arm in ("baseline", "candidate"):
            assert sha(start[f"{arm}_binary_path"]) == start[arm]["binary_sha256"], "binary changed during run"
        assert host_identity() == json.loads((root / "environment/host-runtime.json").read_text()), "controlled environment changed"
        write_json(root / "environment/source-identity-end.json", {**end, "status": "pass"})
    verify_manifest(root, "environment/pre-run.sha256", complete=False)


DOCUMENTATION_FILES = {"docs/roadmap/0.1/0.1.2/README.md", "benchmark-results/fs-bench-pro/optimization-history.md"}
DOCUMENTATION_FILES.update("release-notes/0.1.2/"+name for name in (
    "README.md","benchmark-results.md","github-release.md","verification.md","release-contract.md",
    "artifacts.md","limitations.md","sdk-edit-evidence.json"))
EVIDENCE_PREFIXES = tuple("benchmark-results/fs-bench-pro/"+name+"/" for name in (
    "edit-length-preserving","edit-length-changing","edit-canonical-chunk-count",
    "sdk-edit-terminal","sdk-edit-repository-gates","sdk-edit-builds"))


def documentation_bridge(measured_revision, documentation_revision, evidence_only=False):
    subprocess.run(["git","merge-base","--is-ancestor",measured_revision,documentation_revision],cwd=REPO,check=True)
    measured, documentation = source_identity(measured_revision), source_identity(documentation_revision)
    for key in ("source_seal","product_seal","harness_seal","cargo_lock_sha256","contract_sha256",
                "workload_sha256","report_generator_sha256","custody_helper_sha256","release_generator_sha256",
                "preparation_compatibility_sha256"):
        assert measured[key] == documentation[key], f"documentation bridge changed {key}"
    changed = output("git","diff","--name-only",measured_revision,documentation_revision).splitlines()
    allowed_files = {"release-notes/0.1.2/sdk-edit-evidence.json"} if evidence_only else DOCUMENTATION_FILES
    assert all(path in allowed_files or path.startswith(EVIDENCE_PREFIXES) for path in changed), "documentation bridge scope"
    return {"measured_revision":measured_revision,"documentation_revision":documentation_revision,
            "changed_paths":changed,"evidence_only":evidence_only,"status":"pass"}


def repository_gates(destination, measured_revision=None):
    revision = require_clean()
    identity = source_identity(revision)
    measured_revision = measured_revision or revision
    bridge = documentation_bridge(measured_revision, revision)
    root = Path(destination).resolve()
    assert not root.exists(), "repository-gate directory exists"
    root.mkdir(parents=True)
    commands = []
    requested = [
        ["cargo","fmt","--all","--","--check"],
        ["cargo","test","--workspace","--all-targets","--all-features","--locked"],
        ["cargo","clippy","--workspace","--all-targets","--all-features","--locked","--","-D","warnings"],
        ["git","diff","--check"],
    ]
    passed = False
    try:
        for index, command in enumerate(requested, 1):
            started = time.monotonic_ns()
            with (root / f"{index}.stdout.txt").open("wb") as stdout, (root / f"{index}.stderr.txt").open("wb") as stderr:
                result = subprocess.run(command, cwd=REPO, stdout=stdout, stderr=stderr)
            commands.append({"argv":command,"exit_code":result.returncode,"elapsed_ns":time.monotonic_ns()-started})
            if result.returncode:
                raise SystemExit(result.returncode)
        require_clean(revision)
        assert source_identity(revision) == identity
        passed = True
    finally:
        write_json(root / "commands.json", commands)
        write_json(root / "run-status.json", {"schema":"fs-bench-pro-sdk-edit-repository-gates-v1",
                   "source":identity,"measured_source":source_identity(measured_revision),
                   "documentation_bridge":bridge,"status":"pass" if passed else "fail"})
        seal(root)
    verify_manifest(root)
    print(f"PASS repository gates {root}")


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "build":
        build(sys.argv[2], sys.argv[3])
    elif len(sys.argv) == 3 and sys.argv[1] == "identity":
        print(json.dumps(source_identity(sys.argv[2]), sort_keys=True))
    elif len(sys.argv) == 10 and sys.argv[1] == "capture":
        capture(*sys.argv[2:])
    elif len(sys.argv) == 3 and sys.argv[1] == "finalize":
        finalize(sys.argv[2])
    elif len(sys.argv) in (6, 7) and sys.argv[1] == "prepare":
        acquire_prepared(*sys.argv[2:])
    elif len(sys.argv) == 5 and sys.argv[1] == "clone":
        print(json.dumps(clone_prepared(*sys.argv[2:]), sort_keys=True, separators=(",", ":")))
    elif len(sys.argv) == 3 and sys.argv[1] == "cache-self-check":
        cache_self_check(sys.argv[2])
    elif len(sys.argv) in (3,4) and sys.argv[1] == "repository-gates":
        repository_gates(*sys.argv[2:])
    else:
        raise SystemExit("usage: sdk-edit-custody.py build OUTPUT_DIR IMAGE | identity REVISION")
