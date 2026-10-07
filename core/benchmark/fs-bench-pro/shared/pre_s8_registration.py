"""Read-only registration for fixed diagnostic successors; no numeric promotion."""
import hashlib
import json
from pathlib import Path
import subprocess

SCHEMA = "pre-s8-accounting-registration-v1"
REGISTRY = "core/benchmark/fs-bench-pro/registry/pre-s8-accounting-v1.json"
GROWTH_SCHEMA = "pre-s8-growth-registration-v1"
GROWTH_REGISTRY = "core/benchmark/fs-bench-pro/registry/pre-s8-growth-v1.json"


def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as stream:
        while data := stream.read(65536):
            value.update(data)
    return value.hexdigest()


def admit(value, root):
    root = Path(root).resolve()
    errors = []
    try:
        selected = {SCHEMA: REGISTRY, GROWTH_SCHEMA: GROWTH_REGISTRY}.get(value.get("schema"))
        if selected is None:
            raise ValueError("unregistered schema")
        registry = json.loads((root / selected).read_text())
        if value.get("registry_sha256") != digest(root / selected):
            raise ValueError("registration/registry identity")
        case = registry["cases"][value["case"]]
        if value.get("workload") != case or value.get("mode") != "diagnostic" or value.get("numeric_acceptance") != "OWNER_DEFERRED":
            raise ValueError("fixed workload or diagnostic acceptance changed")
        if value.get("cache") != registry["cache"] or value.get("budget_ns") != registry["budget_ns"]:
            raise ValueError("cache/budget changed")
        if value.get("profile") != {"overlay": "schema16-memory-off-exclusive", "store": "sqlite-wal-off-v2" if case["prepared"] else "NOT_IN_SCOPE"}:
            raise ValueError("profile scope")
        authority = root / registry["authority"]
        if value.get("authority_sha256") != digest(authority):
            raise ValueError("owner authority identity")
        commit = value["source_commit"]
        if subprocess.check_output(["git", "-c", f"safe.directory={root}", "rev-parse", "HEAD"], cwd=root, text=True).strip() != commit:
            raise ValueError("source commit mismatch")
        for field in ("sources", "build_inputs"):
            if not isinstance(value.get(field), dict) or not value[field]:
                raise ValueError("missing " + field)
            for name, expected in value[field].items():
                path = (root / name).resolve()
                if not path.is_relative_to(root) or digest(path) != expected:
                    raise ValueError("source/build hash mismatch: " + name)
        for name in (".cargo/config.toml", "core/Cargo.toml", "core/Cargo.lock"):
            if name not in value["build_inputs"]:
                raise ValueError("required root build input missing")
        if "--locked" not in value["build_command"] or value.get("construction_workers") != "1":
            raise ValueError("locked single-construction build required")
        binary = Path(value["binary"]["path"])
        if not binary.is_absolute() or binary.name != case["binary_name"] or digest(binary) != value["binary"]["sha256"]:
            raise ValueError("original binary identity")
        if not isinstance(value.get("environment"), dict) or not value["environment"].get("platform"):
            raise ValueError("actual environment identity missing")
        if case["prepared"]:
            if set(value.get("prepared", {})) != {"prepared.sqlite", "prepared.manifest", "expected-metadata.txt"}:
                raise ValueError("closed preparation identity missing")
        elif value.get("prepared") is not None:
            raise ValueError("unselected preparation")
    except (ValueError, KeyError, OSError, TypeError, subprocess.CalledProcessError) as error:
        errors.append(str(error))
    return {"schema": "pre-s8-accounting-registration-result-v1", "status": "INCOMPLETE" if errors else "PASS",
            "registration_only": True, "numeric_acceptance": "OWNER_DEFERRED", "qualification": "NOT_EVALUATED",
            "case": value.get("case"), "errors": errors}
