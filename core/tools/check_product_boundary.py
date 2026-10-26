"""Small source-text guard for core/AGENTS.md; semantic review is still required."""

import re
import sys
import tomllib
from pathlib import Path

ATTR = re.compile(r"#\s*!?\s*\[([^\]]*)\]", re.DOTALL)
TEST_ATTR = re.compile(r"^(?:\w+\s*::\s*)*(?:test|bench)\b")
TEST_CFG = re.compile(r'\btest\b|"[^"\n]*(?:test|bench|mock|fixture|fault)[^"\n]*"')
CFG_MACRO = re.compile(r"\bcfg\s*!\s*\(([^;]*)\)", re.DOTALL)
IMPL = re.compile(
    r"\b(?:struct|enum|trait|impl|union|static|const|let|if|else|match|loop|while|for)\b|\bmacro_rules\s*!"
)
DOC_CODE = re.compile(r"^\s*(?:///|//!)\s*```", re.MULTILINE)
UNSAFE = re.compile(r"\bunsafe\b")

# Crates whose `unsafe` surface is bounded. layerfs-storage keeps one audited
# FFI module (encoding/codec.rs); Persistence has the owner-authorized macOS
# seal file-control module. Their siblings stay unsafe-free. The
# comment in a lint name (`unsafe_code`) is not the bare word, so attr lines
# never trip the scan once comments are stripped.
UNSAFE_AUDITED_MODULE = {
    "layerfs-storage": "src/encoding/codec.rs",
    "layerfs-persistence": "src/backend/sqlite/file_control.rs",
}
UNSAFE_FREE_CRATES = (
    "layerfs-content",
    "layerfs-telemetry",
    "layerfs-project",
    "layerfs-overlay",
    "layerfs-workspace",
    "layerfs-store-service",
    "layerfs-fuse",
    "layerfs-daemon",
    "layerfs-api",
    "layerfs-bridge",
    "layerfs-sandbox",
)
UNSAFE_ROOT_ATTR = {
    "layerfs-storage": "#![deny(unsafe_code)]",
    "layerfs-content": "#![forbid(unsafe_code)]",
    "layerfs-telemetry": "#![forbid(unsafe_code)]",
    "layerfs-persistence": "#![deny(unsafe_code)]",
    "layerfs-project": "#![forbid(unsafe_code)]",
    "layerfs-overlay": "#![forbid(unsafe_code)]",
    "layerfs-workspace": "#![forbid(unsafe_code)]",
    "layerfs-store-service": "#![forbid(unsafe_code)]",
    "layerfs-fuse": "#![forbid(unsafe_code)]",
    "layerfs-daemon": "#![forbid(unsafe_code)]",
    "layerfs-api": "#![forbid(unsafe_code)]",
    "layerfs-bridge": "#![forbid(unsafe_code)]",
    "layerfs-sandbox": "#![forbid(unsafe_code)]",
}


# First-party production edges; third-party approvals and the locked graph are
# checked separately. The combined SQLite adapter is the active provider.
ALLOWED_DEPENDENCIES = {
    "layerfs-node": {
        "layerfs-api",
        "layerfs-bridge",
        "layerfs-history",
        "layerfs-persistence",
        "layerfs-sandbox",
        "layerfs-storage",
        "layerfs-store-service",
        "layerfs-telemetry",
    },
    "layerfs-telemetry": set(),
    "layerfs-content": {"layerfs-telemetry"},
    "layerfs-storage": {"layerfs-content", "layerfs-telemetry"},
    "layerfs-history": {"layerfs-content"},
    "layerfs-persistence": {"layerfs-storage", "layerfs-history", "layerfs-content", "layerfs-telemetry"},
    "layerfs-project": {"layerfs-content", "layerfs-storage", "layerfs-history", "layerfs-telemetry"},
    "layerfs-overlay": set(),
    "layerfs-workspace": {"layerfs-content", "layerfs-overlay", "layerfs-telemetry"},
    "layerfs-fuse": {"layerfs-content", "layerfs-overlay", "layerfs-workspace"},
    # The Store engine (#352 F-020): the SDK's Store calls in-process and the
    # Store/overlay ports daemon Workspace Commits use; it opens the Store.
    "layerfs-store-service": {
        "layerfs-overlay",
        "layerfs-workspace",
        "layerfs-bridge",
        "layerfs-content",
        "layerfs-storage",
        "layerfs-history",
        "layerfs-persistence",
        "layerfs-telemetry",
    },
    # Concrete Store open/install belongs to application composition. The
    # provider-independent daemon store/ subtree has a narrower source boundary.
    "layerfs-daemon": {
        "layerfs-overlay",
        "layerfs-workspace",
        "layerfs-store-service",
        "layerfs-fuse",
        "layerfs-bridge",
        "layerfs-content",
        "layerfs-storage",
        "layerfs-history",
        "layerfs-persistence",
        "layerfs-telemetry",
    },
    "layerfs-api": {
        "layerfs-content",
        "layerfs-storage",
        "layerfs-history",
        "layerfs-persistence",
        "layerfs-telemetry",
        "layerfs-bridge",
        "layerfs-project",
        "layerfs-sandbox",
        "layerfs-store-service",
    },
    "layerfs-bridge": {"layerfs-content", "layerfs-history"},
    "layerfs-sandbox": set(),
}
DOMAIN_CRATES = {"layerfs-content", "layerfs-storage", "layerfs-history", "layerfs-project"}
LAYER_ORDER = ("core", "engine", "api")
CORE, ENGINE, API = LAYER_ORDER
LAYER = {
    "layerfs-content": CORE,
    "layerfs-history": CORE,
    "layerfs-storage": CORE,
    "layerfs-telemetry": CORE,
    "layerfs-bridge": ENGINE,
    "layerfs-daemon": ENGINE,
    "layerfs-fuse": ENGINE,
    "layerfs-overlay": ENGINE,
    "layerfs-persistence": ENGINE,
    "layerfs-project": ENGINE,
    "layerfs-sandbox": ENGINE,
    "layerfs-store-service": ENGINE,
    "layerfs-workspace": ENGINE,
    "layerfs-api": API,
    "layerfs-node": API,
}
# Temporary upward test link; migration #463 step 2b removes both exceptions.
UPWARD_TEST_EDGE = ("layerfs-daemon", "layerfs-api")
UPWARD_TEST_PATHS = {
    (f"tests/{test}.rs", f"tests/support/{helper}.rs")
    for test in ("merge_report", "concurrent_commits", "lane_merge")
    for helper in ("installed_store", "sdk")
}


def layer_violations(name, dependency, kind):
    if dependency.startswith("layerfs-") and dependency not in LAYER:
        return [(1, f"crate missing from layer map: {dependency}")]
    if dependency in LAYER and LAYER_ORDER.index(LAYER[dependency]) > LAYER_ORDER.index(LAYER[name]):
        if kind == "dev-dependencies" and (name, dependency) == UPWARD_TEST_EDGE:
            return []
        return [(1, f"upward {kind} edge {name} ({LAYER[name]}) -> {dependency} ({LAYER[dependency]})")]
    return []


def crate_directories(core):
    """All Rust crate roots; keep layout knowledge in one helper."""
    return sorted(path for path in (core / "crates").glob("*") if path.is_dir()) + [core.parent / "api/binding"]


def test_path_violations(path, source, packages):
    """Resolve test includes before checking their owning crate's layer."""
    packages = {root.resolve(): name for root, name in packages.items()}
    path = path.resolve()
    owner = next((root for root in packages if path.is_relative_to(root)), None)
    if owner is None or packages[owner] not in LAYER:
        return []
    found = []
    for match in re.finditer(r'#\s*\[\s*path\s*=\s*"([^"]+)"\s*\]', source):
        target = (path.parent / match[1]).resolve()
        destination = next((root for root in packages if target.is_relative_to(root)), None)
        if destination is None:
            continue
        name, dependency = packages[owner], packages[destination]
        exception = (path.relative_to(owner).as_posix(), target.relative_to(destination).as_posix())
        if (name, dependency) == UPWARD_TEST_EDGE and exception in UPWARD_TEST_PATHS:
            continue
        for _, reason in layer_violations(name, dependency, "test #[path]"):
            found.append((source.count("\n", 0, match.start()) + 1, reason))
    return found


ENGINES_AND_CLUSTER2 = {
    "layerfs-s3",
    "layerfs-metadata",
    "layerfs-persistence",
    "layerfs-overlay",
    "layerfs-workspace",
    "layerfs-store-service",
    "layerfs-fuse",
    "layerfs-daemon",
    "layerfs-bridge",
    "layerfs-sandbox",
    "layerfs-server",
    "layerfs-sdk",
    "layerfs-api",
    "layerfs-api-core",
}


# Package directories under core/crates that R7-retire removed. Their source
# stays in Git history; a directory or manifest that returns fails the scan.
RETIRED_PACKAGES = (
    "layerfs-server",
    "layerfs-fuse-legacy",
    "layerfs-sandbox-legacy",
    "layerfs-sdk-legacy",
    "layerfs-daemon-legacy",
    "layerfs-bridge-legacy",
    "layerfs-workspace-legacy",
)
RETIRED_PACKAGE = "retired package; its directory was removed and is not restored"


def dependency_violations(source):
    """Check production/build and test edges, including target tables and aliases."""
    manifest = tomllib.loads(source)
    name = manifest.get("package", {}).get("name")
    if name in RETIRED_PACKAGES:
        return [(1, f"{RETIRED_PACKAGE}: {name}")]
    if name not in LAYER:
        return [(1, f"crate missing from layer map: {name}")]
    kinds = ("dependencies", "build-dependencies", "dev-dependencies")
    sections = [(key, manifest.get(key, {})) for key in kinds]
    for target in manifest.get("target", {}).values():
        sections.extend((key, target.get(key, {})) for key in kinds)
    found = []
    for kind, section in sections:
        for alias, specification in section.items():
            dependency = specification.get("package", alias) if isinstance(specification, dict) else alias
            found.extend(layer_violations(name, dependency, kind))
            if LAYER[name] == CORE and dependency == "rusqlite":
                found.append((1, f"rusqlite dependency in core crate {name}"))
            if kind == "dev-dependencies":
                continue
            if name in DOMAIN_CRATES and dependency in {
                "rusqlite",
                "postgres",
                "tokio-postgres",
                "native-tls",
                "reqwest",
            }:
                guidance = ""
                if name == "layerfs-project" and dependency == "rusqlite":
                    guidance = (
                        "; keep concrete SQLite in layerfs-persistence and expose "
                        "an owning backend-neutral port; SQLite-backed Project "
                        "acquisition is allowed through that boundary"
                    )
                found.append((1, f"engine dependency in domain {name} -> {dependency}{guidance}"))
            if dependency.startswith("layerfs-") and dependency not in ALLOWED_DEPENDENCIES[name]:
                found.append((1, f"forbidden production dependency {name} -> {dependency}"))
    return found


def component_violations(path, source):
    """Reject forbidden crate references in domain code and first-party imports."""
    name = crate_name(path)
    if name in RETIRED_PACKAGES:
        return [(1, f"{RETIRED_PACKAGE}: {name}")]
    retired = {
        "layerfs-api": ("/src/client/", "/src/runtime/"),
        "layerfs-daemon": ("/src/upstream/",),
        "layerfs-bridge": ("/src/codec/", "/src/contract/", "/src/native/framing.rs"),
        "layerfs-api-core": ("/src/",),
    }
    if any(part in path.as_posix() for part in retired.get(name, ())):
        return [(1, "retired host-mediated transport path; use direct Store or native control")]
    if name not in ALLOWED_DEPENDENCIES:
        return []
    found = []
    if name == "layerfs-daemon" and "/src/store/" in path.as_posix():
        concrete = r"\b(?:layerfs_persistence|layerfs_api|layerfs_bridge|rusqlite|sqlite)\s*::|\b(?:Path|PathBuf)\b"
        for match in re.finditer(concrete, source):
            found.append(
                (
                    source.count("\n", 0, match.start()) + 1,
                    "concrete provider/transport/path in daemon store/; use opened Storage/History ports",
                )
            )
    if name in DOMAIN_CRATES:
        for dependency in ENGINES_AND_CLUSTER2:
            spelling = dependency.replace("-", "[-_]")
            for match in re.finditer(r"\b" + spelling + r"\b", source):
                found.append(
                    (source.count("\n", 0, match.start()) + 1, f"domain source names forbidden component {dependency}")
                )
    for match in re.finditer(r"\b(layerfs_[a-z0-9_]+)\s*::", source):
        dependency = match[1].replace("_", "-")
        if dependency != name and dependency not in ALLOWED_DEPENDENCIES[name]:
            found.append(
                (
                    source.count("\n", 0, match.start()) + 1,
                    f"forbidden first-party source import {name} -> {dependency}",
                )
            )
    return found


def crate_name(path):
    """The core workspace crate a path belongs to, when it belongs to one."""
    parts = path.parts
    for index, part in enumerate(parts):
        if part == "crates" and index + 1 < len(parts):
            if parts[index + 1] == "layerfs-api" and index + 2 < len(parts) and parts[index + 2] == "core":
                return "layerfs-api-core"
            return parts[index + 1]
    return None


def unsafe_violations(path, source):
    """Enforce each crate's documented unsafe boundary."""
    found = []
    crate = crate_name(path)
    if crate is None or crate not in UNSAFE_ROOT_ATTR or path.suffix != ".rs":
        return found
    relative = Path(*path.parts[path.parts.index("src") :])
    code = "\n".join(line.split("//", 1)[0] for line in source.splitlines())
    if UNSAFE.search(code) and (crate in UNSAFE_FREE_CRATES or str(relative) != UNSAFE_AUDITED_MODULE[crate]):
        found.append((1, "unsafe outside the audited module boundary; see core/AGENTS.md"))
    if relative == Path("src/lib.rs") and UNSAFE_ROOT_ATTR[crate] not in source:
        found.append((1, f"crate root must declare {UNSAFE_ROOT_ATTR[crate]}"))
    return found


def violations(path, source):
    """Return line/reason pairs; intentionally reject marker text in comments too."""
    found = []
    lines = source.splitlines()
    entry = path.name in ("lib.rs", "mod.rs")
    limit = 200 if entry else 999
    if len(lines) > limit:
        kind = "entry file" if entry else "production file"
        found.append((limit + 1, f"{kind} has {len(lines)} physical lines; maximum is {limit}"))
    found.extend(overlay_terminology_violations(path, source))
    if path.suffix != ".rs":
        return found
    for match in ATTR.finditer(source):
        body = match[1].strip()
        if TEST_ATTR.match(body) or (re.match(r"cfg(?:_attr)?\b", body) and TEST_CFG.search(body)):
            found.append((source.count("\n", 0, match.start()) + 1, "test-only attribute in product source"))
    for match in CFG_MACRO.finditer(source):
        if TEST_CFG.search(match[1]):
            found.append((source.count("\n", 0, match.start()) + 1, "test-only cfg! in product source"))
    if DOC_CODE.search(source):
        found.append((1, "put fenced executable documentation examples outside src/"))
    if entry:
        for number, line in enumerate(lines, 1):
            code = line.split("//", 1)[0].strip()
            if IMPL.search(code):
                found.append((number, "implementation construct in declaration/delegation entry file"))
    if crate_name(path) == "layerfs-storage" and any(part in ("encoding", "pack") for part in path.parts):
        for match in re.finditer(r"\b(?:rusqlite|sqlite)\s*::", source):
            found.append(
                (source.count("\n", 0, match.start()) + 1, "engine access under encoding/ or pack/; use source seam")
            )
    found.extend(unsafe_violations(path, source))
    found.extend(component_violations(path, source))
    return found


def overlay_terminology_violations(path, source):
    """Guard the renamed SQL/API concepts; generic temporary buffers remain valid."""
    if crate_name(path) not in {"layerfs-overlay", "layerfs-workspace", "layerfs-daemon"}:
        return []
    retired = (
        r"\b(?:Dentry|Dentries|DENTRY\w*|dentry\w*|ScratchRecord|SCRATCH_BYTES|"
        r"OverlayScratch|IndexedScratch\w*|indexed_scratch\w*|owned_scratch\w*|"
        r"scratch_(?:rows|bytes|page|contains|get|apply|keys\w*)|put_scratch|"
        r"IndexedScope|IndexedKey|IndexedChange|IndexedApply|ExpectedValue)\b|"
        r"\b(?:StatementKind|ServiceClass)::Scratch\b|"
        r"\b(?:TABLE|FROM|INTO|UPDATE|ON)\s+scratch\b|"
        r"\bpub\s+(?:struct|enum|trait)\s+Scratch\w*\b"
    )
    found = [
        (
            source.count("\n", 0, match.start()) + 1,
            "retired overlay concept name; use directory_entry or operation_record terminology",
        )
        for match in re.finditer(retired, source)
    ]
    if "scratch" in path.name or "dentry" in path.name:
        found.append((1, "retired overlay concept filename"))
    return found


def production_files(core, include_sdk=True):
    """Known product inputs; tests/examples/tools are deliberately outside this scope."""
    files = set()
    for package in crate_directories(core):
        if not include_sdk and package == core.parent / "api/binding":
            continue
        if not package.is_dir():
            continue
        files.update(path for path in (package / "src").rglob("*") if path.is_file() and path.suffix in (".rs", ".sql"))
        files.update(path for path in (package / "sql").rglob("*.sql") if path.is_file())
    if not include_sdk:
        return sorted(files)
    api = core.parent / "api"
    for directory in (
        api / "agent" / "sdk" / "src",
        api / "agent" / "cli" / "src",
        api / "backend" / "src",
        api / "runtime" / "src",
    ):
        files.update(directory.rglob("*.js"))
    return sorted(files)


def main():
    # --skip-sdk leaves api/ (binding and JS packages) out of the scan; CI uses it (owner, 2026-10-23).
    core = Path(__file__).resolve().parents[1]
    files = production_files(core, include_sdk="--skip-sdk" not in sys.argv[1:])
    failures = 0
    for path in files:
        for line, reason in violations(path, path.read_text()):
            print(f"{path.relative_to(core.parent)}:{line}: {reason}")
            failures += 1
    for name in RETIRED_PACKAGES:
        if (core / "crates" / name).exists():
            print(f"crates/{name}: {RETIRED_PACKAGE}")
            failures += 1
    manifests = {root / "Cargo.toml" for root in crate_directories(core) if (root / "Cargo.toml").is_file()}
    packages = {manifest.parent: tomllib.loads(manifest.read_text())["package"]["name"] for manifest in manifests}
    for manifest in sorted(manifests):
        for line, reason in dependency_violations(manifest.read_text()):
            print(f"{manifest.relative_to(core.parent)}:{line}: {reason}")
            failures += 1
    for package in packages:
        for path in sorted((package / "tests").rglob("*.rs")):
            for line, reason in test_path_violations(path, path.read_text(), packages):
                print(f"{path.relative_to(core.parent)}:{line}: {reason}")
                failures += 1
    if failures:
        print(f"FAIL: {failures} product-source boundary violations")
        return 1
    print(f"PASS: scanned {len(files)} production Rust/SQL/JavaScript files; semantic review still required")
    if not files:
        print("No product source exists yet; this is only a policy setup check.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
