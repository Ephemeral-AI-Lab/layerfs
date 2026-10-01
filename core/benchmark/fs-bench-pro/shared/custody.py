"""Complete, versioned build and harness input custody for the existing runner."""
import hashlib
import json
import os

METHOD = "core-build-and-harness-inputs-v2"
BUILD_ENV = ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_JOBS",
             "CC", "CFLAGS", "CXX", "AR", "SDKROOT", "MACOSX_DEPLOYMENT_TARGET")


def inputs(root, core, harness):
    product = set()
    for path in (core / "crates").rglob("*"):
        if not path.is_file():
            continue
        parts = path.relative_to(core / "crates").parts
        if any(part in ("target", "tests", "benches", "__pycache__") for part in parts):
            continue
        if "src" in parts or "sql" in parts or "examples" in parts or path.name in ("Cargo.toml", "build.rs"):
            product.add(path)
    product.update((core / "Cargo.toml", core / "Cargo.lock", root / ".cargo/config.toml"))
    for name in ("rust-toolchain.toml", "rust-toolchain"):
        if (root / name).is_file():
            product.add(root / name)
    runner = {path for path in harness.rglob("*") if path.is_file()
              and "__pycache__" not in path.parts and path.suffix != ".pyc"}
    # This imported provider is outside the runner directory.
    runner.add(core / "benchmark/fs-bench-pro-storage-content/shared/residency.py")
    return sorted(product), sorted(runner)


def inventory(root, paths):
    return {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in paths}


def compilation_seal(product_seal):
    environment = {name: os.environ.get(name) for name in BUILD_ENV}
    value = {"method": METHOD, "product_seal": product_seal,
             "toolchain": "1.85.1", "profile": "release", "environment": environment}
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest(), environment
