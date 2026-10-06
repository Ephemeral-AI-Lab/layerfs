"""Small source-text guard for core/AGENTS.md; semantic review is still required."""

from pathlib import Path
import re
import sys
import tomllib


ATTR = re.compile(r"#\s*!?\s*\[([^\]]*)\]", re.DOTALL)
TEST_ATTR = re.compile(r"^(?:\w+\s*::\s*)*(?:test|bench)\b")
TEST_CFG = re.compile(r'\btest\b|"[^"\n]*(?:test|bench|mock|fixture|fault)[^"\n]*"')
CFG_MACRO = re.compile(r"\bcfg\s*!\s*\(([^;]*)\)", re.DOTALL)
IMPL = re.compile(r"\b(?:struct|enum|trait|impl|union|static|const|let|if|else|match|loop|while|for)\b|\bmacro_rules\s*!")
DOC_CODE = re.compile(r"^\s*(?:///|//!)\s*```", re.MULTILINE)
UNSAFE = re.compile(r"\bunsafe\b")

# Crates whose `unsafe` surface is bounded. layerfs-storage keeps one audited
# FFI module (encoding/codec.rs); its siblings must stay unsafe-free. The
# comment in a lint name (`unsafe_code`) is not the bare word, so attr lines
# never trip the scan once comments are stripped.
UNSAFE_AUDITED_MODULE = {
    "layerfs-storage": "src/encoding/codec.rs",
}
UNSAFE_FREE_CRATES = ("layerfs-content", "layerfs-telemetry", "layerfs-persistence", "layerfs-project", "layerfs-overlay", "layerfs-workspace", "layerfs-daemon", "layerfs-sdk", "layerfs-bridge")
UNSAFE_ROOT_ATTR = {
    "layerfs-storage": "#![deny(unsafe_code)]",
    "layerfs-content": "#![forbid(unsafe_code)]",
    "layerfs-telemetry": "#![forbid(unsafe_code)]",
    "layerfs-persistence": "#![forbid(unsafe_code)]",
    "layerfs-project": "#![forbid(unsafe_code)]",
    "layerfs-overlay": "#![forbid(unsafe_code)]",
    "layerfs-workspace": "#![forbid(unsafe_code)]",
    "layerfs-daemon": "#![forbid(unsafe_code)]",
    "layerfs-sdk": "#![forbid(unsafe_code)]",
    "layerfs-bridge": "#![forbid(unsafe_code)]",
}


# First-party production edges; third-party approvals and the locked graph are
# checked separately. The combined SQLite adapter is the active provider.
ALLOWED_DEPENDENCIES = {
    "layerfs-telemetry": set(),
    "layerfs-content": {"layerfs-telemetry"},
    "layerfs-storage": {"layerfs-content", "layerfs-telemetry"},
    "layerfs-history": {"layerfs-content"},
    "layerfs-persistence": {"layerfs-storage", "layerfs-history", "layerfs-content"},
    "layerfs-project": {"layerfs-content", "layerfs-storage", "layerfs-history", "layerfs-telemetry"},
    "layerfs-overlay": set(),
    "layerfs-workspace": {"layerfs-content", "layerfs-overlay", "layerfs-telemetry"},
    # Upstream composes public authenticated consumers and owning types. The
    # Persistence edge carries only host-provisioned completion-profile facts;
    # no global provider/SQL implementation moves into Daemon.
    "layerfs-daemon": {"layerfs-overlay", "layerfs-workspace", "layerfs-sdk", "layerfs-bridge", "layerfs-content", "layerfs-storage", "layerfs-history", "layerfs-persistence"},
    "layerfs-sdk": {"layerfs-content", "layerfs-storage", "layerfs-history", "layerfs-persistence", "layerfs-telemetry", "layerfs-bridge", "layerfs-workspace"},
    "layerfs-bridge": set(),
}
DOMAIN_CRATES = {"layerfs-content", "layerfs-storage", "layerfs-history", "layerfs-project"}
ENGINES_AND_CLUSTER2 = {
    "layerfs-s3", "layerfs-metadata", "layerfs-persistence", "layerfs-overlay", "layerfs-workspace",
    "layerfs-fuse", "layerfs-daemon", "layerfs-bridge", "layerfs-sandbox",
    "layerfs-server", "layerfs-sdk", "layerfs-api-core",
}


def dependency_violations(source):
    """Check production/build edges, including target tables and aliases."""
    manifest = tomllib.loads(source)
    name = manifest.get("package", {}).get("name")
    if name not in ALLOWED_DEPENDENCIES:
        return []
    sections = [manifest.get(key, {}) for key in ("dependencies", "build-dependencies")]
    for target in manifest.get("target", {}).values():
        sections.extend(target.get(key, {}) for key in ("dependencies", "build-dependencies"))
    found = []
    for section in sections:
        for alias, specification in section.items():
            dependency = specification.get("package", alias) if isinstance(specification, dict) else alias
            if name in DOMAIN_CRATES and dependency in {"rusqlite", "postgres", "tokio-postgres", "native-tls", "reqwest"}:
                guidance = ""
                if name == "layerfs-project" and dependency == "rusqlite":
                    guidance = (
                        "; keep concrete SQLite in layerfs-persistence and expose "
                        "an owning backend-neutral port; SQLite-backed Project "
                        "acquisition is allowed through that boundary"
                    )
                found.append((1,f"engine dependency in domain {name} -> {dependency}{guidance}"))
            if dependency.startswith("layerfs-") and dependency not in ALLOWED_DEPENDENCIES[name]:
                found.append((1, f"forbidden production dependency {name} -> {dependency}"))
    return found


def component_violations(path, source):
    """Reject forbidden crate references in domain code and first-party imports."""
    name = crate_name(path)
    if name not in ALLOWED_DEPENDENCIES:
        return []
    found = []
    if name in DOMAIN_CRATES:
        for dependency in ENGINES_AND_CLUSTER2:
            spelling = dependency.replace("-", "[-_]")
            for match in re.finditer(r"\b" + spelling + r"\b", source):
                found.append((source.count("\n", 0, match.start()) + 1,
                              f"domain source names forbidden component {dependency}"))
    for match in re.finditer(r"\b(layerfs_[a-z0-9_]+)\s*::", source):
        dependency = match[1].replace("_", "-")
        if dependency != name and dependency not in ALLOWED_DEPENDENCIES[name]:
            found.append((source.count("\n", 0, match.start()) + 1,
                          f"forbidden first-party source import {name} -> {dependency}"))
    return found


def crate_name(path):
    """The core workspace crate a path belongs to, when it belongs to one."""
    parts = path.parts
    for index, part in enumerate(parts):
        if part == "crates" and index + 1 < len(parts):
            if parts[index + 1] == "layerfs-api" and index + 2 < len(parts):
                return {"sdk": "layerfs-sdk", "core": "layerfs-api-core"}.get(parts[index + 2], "layerfs-api")
            return parts[index + 1]
    return None


def unsafe_violations(path, source):
    """Enforce each crate's documented unsafe boundary."""
    found = []
    crate = crate_name(path)
    if crate is None or crate not in UNSAFE_ROOT_ATTR or path.suffix != ".rs":
        return found
    relative = Path(*path.parts[path.parts.index("src"):])
    code = "\n".join(line.split("//", 1)[0] for line in source.splitlines())
    if UNSAFE.search(code):
        if crate in UNSAFE_FREE_CRATES or str(relative) != UNSAFE_AUDITED_MODULE[crate]:
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
    if path.suffix != ".rs":
        return found
    for match in ATTR.finditer(source):
        body = match[1].strip()
        if TEST_ATTR.match(body) or (
            re.match(r"cfg(?:_attr)?\b", body) and TEST_CFG.search(body)
        ):
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
            found.append((source.count("\n", 0, match.start()) + 1, "engine access under encoding/ or pack/; use source seam"))
    found.extend(unsafe_violations(path, source))
    found.extend(component_violations(path, source))
    return found


def production_files(core):
    """Known product inputs; tests/examples/tools are deliberately outside this scope."""
    files = set()
    for package in (core / "crates").glob("*"):
        if not package.is_dir():
            continue
        packages = (package / "core", package / "sdk") if package.name == "layerfs-api" else (package,)
        for member in packages:
            files.update(path for path in (member / "src").rglob("*")
                         if path.is_file() and path.suffix in (".rs", ".sql"))
            files.update(path for path in (member / "sql").rglob("*.sql") if path.is_file())
    return sorted(files)


def main():
    core = Path(__file__).resolve().parents[1]
    files = production_files(core)
    failures = 0
    for path in files:
        for line, reason in violations(path, path.read_text()):
            print(f"{path.relative_to(core)}:{line}: {reason}")
            failures += 1
    manifests = set((core / "crates").glob("*/Cargo.toml"))
    manifests.update((core / "crates" / "layerfs-api").glob("*/Cargo.toml"))
    for manifest in sorted(manifests):
        for line, reason in dependency_violations(manifest.read_text()):
            print(f"{manifest.relative_to(core)}:{line}: {reason}")
            failures += 1
    if failures:
        print(f"FAIL: {failures} product-source boundary violations")
        return 1
    print(f"PASS: scanned {len(files)} production Rust/SQL files; semantic review still required")
    if not files:
        print("No product source exists yet; this is only a policy setup check.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
