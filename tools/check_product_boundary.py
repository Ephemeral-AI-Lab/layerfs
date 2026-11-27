"""Small source-text guard for the product layer guides; semantic review is still required."""

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
# The one test-only cargo feature product source may name (layerfs-project#177,
# #179 E, owner 2026-11-15): the worker's fault switches, in the worker servers'
# own files and nowhere else: the guest worker's in layerfs-api (`worker/`) and
# the host worker's in layerfs-store (`admin/host_worker/`, #828). No shipped
# build turns it on: `worker_fault_violations` refuses a default, or a feature
# or dependency that turns it on, except layerfs-api's own feature forwarding
# it to layerfs-store's.
WORKER_FAULT = "worker-fault"
WORKER_FAULT_CFG = re.compile(r'feature\s*=\s*"worker-fault"')
WORKER_FAULT_FILES = {
    "layerfs-api": ("src/worker/",),
    "layerfs-store": ("src/admin/host_worker/",),
}
WORKER_FAULT_FORWARD = ("layerfs-api", "layerfs-store/worker-fault")
UNSAFE = re.compile(r"\bunsafe\b")

# Crates whose `unsafe` surface is bounded. layerfs-storage keeps one audited
# FFI module (encoding/codec.rs); Persistence has the owner-authorized macOS
# seal file-control module; layerfs-os reads macOS process facts through
# proc_pidinfo in process/darwin.rs (moved with LocalProcess from the Sandbox
# backend, layerfs-project#112, so the home and the drivers share it) and
# marks a spawned child's descriptors close-on-exec in process/inherit.rs
# (#833 review); the
# Sandbox installs the namespaces command filter (two prctl calls, D-8, after
# emptying the command's signal mask) and forks the helper and its command
# into their cgroups (one clone3, #113) in backend/namespaces/seccomp.rs.
# Their siblings stay unsafe-free. The comment in a lint name (`unsafe_code`) is not
# the bare word, so attr lines never trip the scan once comments are stripped.
UNSAFE_AUDITED_MODULE = {
    "layerfs-storage": ("src/encoding/codec.rs",),
    "layerfs-persistence": ("src/backend/sqlite/lifecycle/file_control.rs",),
    "layerfs-os": ("src/process/darwin.rs", "src/process/inherit.rs"),
    "layerfs-sandbox": ("src/backend/namespaces/seccomp.rs",),
}
UNSAFE_FREE_CRATES = (
    "layerfs-content",
    "layerfs-telemetry",
    "layerfs-project",
    "layerfs-overlay",
    "layerfs-vfs",
    "layerfs-store-service",
    "layerfs-protocol",
    "layerfs-fuse",
    "layerfs-nfs",
    "layerfs-daemon",
    "layerfs-api",
    "layerfs-cli",
    "layerfs-home",
    "layerfs-machine",
    "layerfs-store",
    "layerfs-workspace",
    "layerfs-inspect",
)
UNSAFE_ROOT_ATTR = {
    "layerfs-storage": "#![deny(unsafe_code)]",
    "layerfs-content": "#![forbid(unsafe_code)]",
    "layerfs-telemetry": "#![forbid(unsafe_code)]",
    "layerfs-persistence": "#![deny(unsafe_code)]",
    "layerfs-project": "#![forbid(unsafe_code)]",
    "layerfs-overlay": "#![forbid(unsafe_code)]",
    "layerfs-vfs": "#![forbid(unsafe_code)]",
    "layerfs-store-service": "#![forbid(unsafe_code)]",
    "layerfs-protocol": "#![forbid(unsafe_code)]",
    "layerfs-os": "#![deny(unsafe_code)]",
    "layerfs-fuse": "#![forbid(unsafe_code)]",
    "layerfs-nfs": "#![forbid(unsafe_code)]",
    "layerfs-daemon": "#![forbid(unsafe_code)]",
    "layerfs-api": "#![forbid(unsafe_code)]",
    "layerfs-cli": "#![forbid(unsafe_code)]",
    "layerfs-home": "#![forbid(unsafe_code)]",
    "layerfs-machine": "#![forbid(unsafe_code)]",
    "layerfs-store": "#![forbid(unsafe_code)]",
    "layerfs-sandbox": "#![deny(unsafe_code)]",
    "layerfs-workspace": "#![forbid(unsafe_code)]",
    "layerfs-inspect": "#![forbid(unsafe_code)]",
}

# First-party production edges; third-party approvals and the locked graph are
# checked separately. The combined SQLite adapter is the active provider.
ALLOWED_DEPENDENCIES = {
    "layerfs-node": {"layerfs-api", "layerfs-protocol"},
    "layerfs-telemetry": set(),
    "layerfs-content": {"layerfs-telemetry"},
    "layerfs-storage": {"layerfs-content", "layerfs-telemetry"},
    "layerfs-history": {"layerfs-content"},
    "layerfs-persistence": {"layerfs-storage", "layerfs-history", "layerfs-content", "layerfs-telemetry"},
    "layerfs-project": {
        "layerfs-content",
        "layerfs-storage",
        "layerfs-history",
        "layerfs-telemetry",
        "layerfs-persistence",
        "layerfs-protocol",
        "layerfs-os",
    },
    "layerfs-overlay": set(),
    # Workspace filesystem semantics with the route-neutral kernel ports FUSE
    # and NFS serve over (#828: was layerfs-workspace and layerfs-mount-ports).
    "layerfs-vfs": {"layerfs-content", "layerfs-overlay", "layerfs-telemetry"},
    "layerfs-fuse": {"layerfs-content", "layerfs-overlay", "layerfs-vfs"},
    # The NFS export reuses the kernel ports and operations; never fuse or daemon.
    "layerfs-nfs": {"layerfs-content", "layerfs-overlay", "layerfs-vfs"},
    # The Store engine (#352 F-020): the SDK's Store calls in-process and the
    # Store/overlay ports daemon Workspace Commits use; it opens the Store.
    "layerfs-store-service": {
        "layerfs-overlay",
        "layerfs-vfs",
        "layerfs-protocol",
        "layerfs-os",
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
        "layerfs-vfs",
        "layerfs-store-service",
        "layerfs-fuse",
        "layerfs-nfs",
        "layerfs-protocol",
        "layerfs-os",
        "layerfs-content",
        "layerfs-storage",
        "layerfs-history",
        "layerfs-persistence",
        "layerfs-telemetry",
    },
    # How LayerFS processes talk (#828: the wire half of layerfs-bridge).
    "layerfs-protocol": {"layerfs-content", "layerfs-history"},
    # What this kernel reports (#828: the process half of layerfs-bridge).
    "layerfs-os": set(),
    # The SDK call surface (#536): the call table and its call machinery
    # over the control crates (#101), and the guest worker's server, whose
    # Store operations take the caller's place view.
    "layerfs-api": {
        "layerfs-home",
        "layerfs-machine",
        "layerfs-store",
        "layerfs-sandbox",
        "layerfs-workspace",
        "layerfs-inspect",
        "layerfs-protocol",
        # the guest worker's server (#828): its Store operations' place view
        "layerfs-store-service",
    },
    # The home and its records (#103): knows no entity and no driver (#112);
    # layerfs-os's process names this process (pid, kernel start time, boot)
    # for an op's owner.
    "layerfs-home": {"layerfs-protocol", "layerfs-os"},
    # Machine records, lifecycle, the worker client and the platform adapters
    # (#104; the adapters only with the `machine` feature). What a machine
    # holds comes from the caller (`MachineUsers`).
    "layerfs-machine": {"layerfs-protocol", "layerfs-os", "layerfs-home"},
    # Store records and calls (#105): its backends, backups and their bucket
    # drivers, projects, placement and leases, and the host worker's Store
    # side. Who uses a Store comes from the caller (`StoreUsers`).
    "layerfs-store": {
        "layerfs-content",
        "layerfs-storage",
        "layerfs-history",
        "layerfs-persistence",
        "layerfs-telemetry",
        "layerfs-protocol",
        "layerfs-os",
        "layerfs-project",
        "layerfs-store-service",
        "layerfs-home",
        "layerfs-machine",
    },
    # Sandboxes and their backends (#106), and what uses a machine or a Store
    # (`users`, implementing the machine and Store crates' users tables).
    "layerfs-sandbox": {
        "layerfs-protocol",
        "layerfs-os",
        "layerfs-history",
        "layerfs-persistence",
        "layerfs-project",
        "layerfs-home",
        "layerfs-machine",
        "layerfs-store",
    },
    # A Sandbox's Workspaces: the workspace calls and their executing side.
    "layerfs-workspace": {
        "layerfs-protocol",
        "layerfs-history",
        "layerfs-home",
        "layerfs-machine",
        "layerfs-store",
        "layerfs-sandbox",
    },
    # The doctors and the reads that span the control entities (#828).
    "layerfs-inspect": {
        "layerfs-protocol",
        "layerfs-os",
        "layerfs-history",
        "layerfs-project",
        "layerfs-store-service",
        "layerfs-home",
        "layerfs-machine",
        "layerfs-store",
        "layerfs-sandbox",
    },
    # The 2.0 front ends (#533): thin layers over the library's call table.
    "layerfs-cli": {
        "layerfs-api",
        "layerfs-protocol",
        "layerfs-os",
        "layerfs-home",
        "layerfs-store",
        "layerfs-sandbox",
    },
    "layerfs-python": {"layerfs-api", "layerfs-protocol"},
}
DOMAIN_CRATES = {"layerfs-content", "layerfs-storage", "layerfs-history"}
LAYER_ORDER = ("core", "engine", "control", "api")
CORE, ENGINE, CONTROL, API = LAYER_ORDER
LAYER = {
    "layerfs-content": CORE,
    "layerfs-history": CORE,
    "layerfs-storage": CORE,
    "layerfs-telemetry": CORE,
    "layerfs-protocol": ENGINE,
    "layerfs-os": ENGINE,
    "layerfs-daemon": ENGINE,
    "layerfs-fuse": ENGINE,
    "layerfs-nfs": ENGINE,
    "layerfs-overlay": ENGINE,
    "layerfs-persistence": ENGINE,
    "layerfs-project": ENGINE,
    "layerfs-store-service": ENGINE,
    "layerfs-vfs": ENGINE,
    # The control layer (#535 connect, renamed control by #101 D-6), one
    # crate per entity (#828): home < machine < store < sandbox < workspace
    # < inspect.
    "layerfs-home": CONTROL,
    "layerfs-machine": CONTROL,
    "layerfs-store": CONTROL,
    "layerfs-sandbox": CONTROL,
    "layerfs-workspace": CONTROL,
    "layerfs-inspect": CONTROL,
    # The public surface under api/crates: the call surface and the 2.0 front
    # ends (#533), the `layerfs` binary, the Node binding and the PyO3 module.
    "layerfs-api": API,
    "layerfs-cli": API,
    "layerfs-node": API,
    "layerfs-python": API,
}


def layer_violations(name, dependency, kind):
    if dependency.startswith("layerfs-") and dependency not in LAYER:
        return [(1, f"crate missing from layer map: {dependency}")]
    if dependency in LAYER and LAYER_ORDER.index(LAYER[dependency]) > LAYER_ORDER.index(LAYER[name]):
        return [(1, f"upward {kind} edge {name} ({LAYER[name]}) -> {dependency} ({LAYER[dependency]})")]
    return []


CRATE_ROOTS = ("core/crates", "engine/crates", "control/crates", "api/crates", "api")


def crate_directories(root):
    """Every crate root, including unknown crates so the layer map fails closed."""
    return sorted(
        path
        for relative in CRATE_ROOTS
        for path in (root / relative).glob("*")
        if path.is_dir() and (relative != "api" or (path / "Cargo.toml").is_file())
    )


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
        for _, reason in layer_violations(name, dependency, "test #[path]"):
            found.append((source.count("\n", 0, match.start()) + 1, reason))
    return found


ENGINES_AND_CLUSTER2 = {
    "layerfs-s3",
    "layerfs-metadata",
    "layerfs-persistence",
    "layerfs-overlay",
    "layerfs-vfs",
    "layerfs-store-service",
    "layerfs-fuse",
    "layerfs-nfs",
    "layerfs-daemon",
    "layerfs-protocol",
    "layerfs-os",
    "layerfs-home",
    "layerfs-machine",
    "layerfs-store",
    "layerfs-sandbox",
    "layerfs-workspace",
    "layerfs-inspect",
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
            # Project composes Store setup through Persistence, never a database driver.
            if name in DOMAIN_CRATES | {"layerfs-project"} and dependency in {
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


# fuser comes from the owner-selected fork at one full revision (#686). Changing
# that revision needs owner approval (docs/policy/code-review.md); this check
# only refuses the ways a build could silently take other fuser source.
FUSER_GIT = "https://github.com/Ephemeral-AI-Lab/fuser.git"
FUSER_SOURCE = f"fuser must come from {FUSER_GIT} at a full 40-hex `rev`"


def is_fuser(alias, specification):
    """By dependency alias, `package` rename, or a `[replace]` key `fuser:<version>`."""
    name = specification.get("package", alias) if isinstance(specification, dict) else alias
    return name.split(":", 1)[0] == "fuser"


def fuser_specifications(manifest):
    kinds = ("dependencies", "build-dependencies", "dev-dependencies")
    tables = [manifest, *manifest.get("target", {}).values()]
    return [
        spec
        for table in tables
        for kind in kinds
        for alias, spec in table.get(kind, {}).items()
        if is_fuser(alias, spec)
    ]


def fuser_source_violations(manifests, lock, config):
    """`manifests` maps a relative path to its parsed Cargo.toml; `config` is the parsed .cargo/config.toml or {}."""
    found, revisions = [], set()
    for name, document in sorted({**manifests, ".cargo/config.toml": config}.items()):
        overrides = [document.get("replace", {}), *document.get("patch", {}).values()]
        if any(is_fuser(alias, spec) for table in overrides for alias, spec in table.items()):
            found.append((name, "fuser [patch]/[replace] override"))
        for spec in fuser_specifications(document):
            pinned = (
                isinstance(spec, dict) and spec.get("git") == FUSER_GIT and not {"branch", "tag", "path"} & set(spec)
            )
            if not pinned or not re.fullmatch(r"[0-9a-f]{40}", spec.get("rev", "")):
                found.append((name, FUSER_SOURCE))
            else:
                revisions.add(spec["rev"])
    sources = config.get("source", {}).items()
    if config.get("paths") or any(FUSER_GIT.removesuffix(".git") in f"{key} {value}" for key, value in sources):
        found.append((".cargo/config.toml", "Cargo path or source override can replace fuser"))
    if len(revisions) > 1:
        found.append(("Cargo.toml", f"fuser manifests disagree on `rev`: {sorted(revisions)}"))
    allowed = {f"git+{FUSER_GIT}?rev={rev}#{rev}" for rev in revisions}
    for package in lock.get("package", []):
        if package.get("name") == "fuser" and package.get("source") not in allowed:
            found.append(("Cargo.lock", f"locked fuser source {package.get('source')!r} is not the manifest's pin"))
    return found


# #535 D11 (owner, 2026-11-03): machine lifecycle is manual. No worker or VM
# idle stop, Sandbox daemon idle close or on-demand machine start returns;
# `machine.start` and `machine.stop` are the only lifecycle entry points.
# The Store connection reaper (store/open.rs) is not lifecycle and stays.
LIFECYCLE = "retired by #535 D11: machine lifecycle is manual (no idle stop, idle close or on-demand start)"
LIFECYCLE_NAMES = re.compile(
    r"\b(?:serve_until_idle|stop_if_idle|watch_idle|idle_exit_s|IDLE_EXIT_S|guest_stop|"
    r"power_off|start_after|OnDemand)\b|LAYERFS_WORKER_POWEROFF|LAYERFS_DAEMON_IDLE_EXIT"
)


def lifecycle_violations(path, source):
    """Refuse an idle.rs or an idle-stop/on-demand-start name in product source."""
    if path.name == "idle.rs":
        return [(1, LIFECYCLE)]
    return [(source.count("\n", 0, match.start()) + 1, LIFECYCLE) for match in LIFECYCLE_NAMES.finditer(source)]


def component_violations(path, source):
    """Reject forbidden crate references in domain code and first-party imports."""
    name = crate_name(path)
    if name in RETIRED_PACKAGES:
        return [(1, f"{RETIRED_PACKAGE}: {name}")]
    retired = {
        "layerfs-api": ("/src/client/", "/src/runtime/"),
        "layerfs-daemon": ("/src/upstream/",),
        "layerfs-protocol": ("/src/codec/", "/src/contract/", "/src/native/framing.rs"),
        "layerfs-api-core": ("/src/",),
    }
    if any(part in path.as_posix() for part in retired.get(name, ())):
        return [(1, "retired host-mediated transport path; use direct Store or native control")]
    # #536: management moved into layerfs-control by entity, and the 1.1
    # host/operator paths were deleted. Owner D15 (2026-11-04): the call
    # surface is lib.rs plus one folder per call family, one file per call.
    # layerfs-project#107: the call machinery (calls/) moved back from
    # layerfs-control to layerfs-api, which alone speaks the SDK. `home` is a
    # call family again (home.init, home.get), so it is no retired folder,
    # and #828 put the guest worker's server in layerfs-api `worker/`.
    # layerfs-control is layerfs-inspect since #828.
    moved = {
        "layerfs-api": tuple(f"/src/{part}/" for part in ("host", "family", "fs", "base", "operator")),
        "layerfs-inspect": ("/src/host/", "/src/operator/", "/src/calls/"),
        "layerfs-store-service": ("/src/remote/",),
    }
    if any(part in path.as_posix() for part in moved.get(name, ())):
        return [(1, "retired by #536: management lives in the control crates by entity")]
    found = lifecycle_violations(path, source)
    if name not in ALLOWED_DEPENDENCIES:
        return found
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
    """The product crate a path belongs to, when it belongs to one."""
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
    if UNSAFE.search(code) and (crate in UNSAFE_FREE_CRATES or str(relative) not in UNSAFE_AUDITED_MODULE[crate]):
        found.append((1, "unsafe outside the audited module boundary; see core/AGENTS.md"))
    if relative == Path("src/lib.rs") and UNSAFE_ROOT_ATTR[crate] not in source:
        found.append((1, f"crate root must declare {UNSAFE_ROOT_ATTR[crate]}"))
    return found


def worker_fault_cfg(path, body):
    """True for a cfg that names only the worker-fault feature, in a worker server's files."""
    parts = path.parts
    owned = WORKER_FAULT_FILES.get(crate_name(path))
    if owned is None or "src" not in parts:
        return False
    relative = "/".join(parts[parts.index("src") :])
    if not any(relative.startswith(prefix) for prefix in owned):
        return False
    rest, named = WORKER_FAULT_CFG.subn("", body)
    return named > 0 and not TEST_CFG.search(rest)


def worker_fault_violations(source):
    """No default feature and no dependency edge turns the worker-fault feature on."""
    manifest = tomllib.loads(source)
    name = manifest.get("package", {}).get("name")
    found = []
    features = manifest.get("features", {})
    if name not in WORKER_FAULT_FILES and WORKER_FAULT in features:
        owners = " and ".join(sorted(WORKER_FAULT_FILES))
        found.append((1, f"{WORKER_FAULT} is the feature of {owners} alone: {name}"))
    for feature, enabled in features.items():
        forwarded = (name, *enabled) == WORKER_FAULT_FORWARD and feature == WORKER_FAULT
        if not forwarded and feature != WORKER_FAULT and any(WORKER_FAULT in item for item in enabled):
            found.append((1, f"feature {feature} of {name} turns {WORKER_FAULT} on; it is for tests only"))
        if feature == WORKER_FAULT and enabled and not forwarded:
            found.append((1, f"feature {feature} of {name} turns on {enabled}; only {WORKER_FAULT_FORWARD[1]}"))
    kinds = ("dependencies", "build-dependencies", "dev-dependencies")
    sections = [manifest.get(key, {}) for key in kinds]
    for target in manifest.get("target", {}).values():
        sections.extend(target.get(key, {}) for key in kinds)
    for section in sections:
        for alias, specification in section.items():
            wanted = specification.get("features", []) if isinstance(specification, dict) else []
            if any(WORKER_FAULT in item for item in wanted):
                found.append((1, f"dependency {alias} of {name} turns {WORKER_FAULT} on; it is for tests only"))
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
            if re.match(r"cfg\b", body) and worker_fault_cfg(path, body):
                continue
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
    if crate_name(path) not in {"layerfs-overlay", "layerfs-vfs", "layerfs-daemon"}:
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


def production_files(root, include_sdk=True):
    """Known product inputs; tests/examples/tools are deliberately outside this scope."""
    files = set()
    for package in crate_directories(root):
        if not include_sdk and package == root / "api/crates/layerfs-node":
            continue
        if not package.is_dir():
            continue
        files.update(path for path in (package / "src").rglob("*") if path.is_file() and path.suffix in (".rs", ".sql"))
        files.update(path for path in (package / "sql").rglob("*.sql") if path.is_file())
    if not include_sdk:
        return sorted(files)
    api = root / "api"
    for directory in (
        api / "agent" / "src",
        api / "cli" / "src",
        api / "backend" / "src",
        api / "runtime" / "src",
    ):
        files.update(directory.rglob("*.js"))
    return sorted(files)


def main():
    # --skip-sdk leaves the Node binding and api/'s JS packages out of the scan; CI uses it (owner, 2026-10-23).
    root = Path(__file__).resolve().parents[1]
    files = production_files(root, include_sdk="--skip-sdk" not in sys.argv[1:])
    failures = 0
    for path in files:
        for line, reason in violations(path, path.read_text()):
            print(f"{path.relative_to(root)}:{line}: {reason}")
            failures += 1
    for name in RETIRED_PACKAGES:
        if any((root / relative / name).exists() for relative in CRATE_ROOTS):
            print(f"crates/{name}: {RETIRED_PACKAGE}")
            failures += 1
    manifests = {root / "Cargo.toml" for root in crate_directories(root) if (root / "Cargo.toml").is_file()}
    packages = {manifest.parent: tomllib.loads(manifest.read_text())["package"]["name"] for manifest in manifests}
    for manifest in sorted(manifests):
        for line, reason in dependency_violations(manifest.read_text()) + worker_fault_violations(manifest.read_text()):
            print(f"{manifest.relative_to(root)}:{line}: {reason}")
            failures += 1
    parsed = {
        str(path.relative_to(root)): tomllib.loads(path.read_text()) for path in manifests | {root / "Cargo.toml"}
    }
    config = root / ".cargo/config.toml"
    config = tomllib.loads(config.read_text()) if config.is_file() else {}
    for name, reason in fuser_source_violations(parsed, tomllib.loads((root / "Cargo.lock").read_text()), config):
        print(f"{name}:1: {reason}")
        failures += 1
    for package in packages:
        for path in sorted((package / "tests").rglob("*.rs")):
            for line, reason in test_path_violations(path, path.read_text(), packages):
                print(f"{path.relative_to(root)}:{line}: {reason}")
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
