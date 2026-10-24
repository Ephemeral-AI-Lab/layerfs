#!/usr/bin/env python3
"""Discover, plan and apply authorized development cleanup with recovery records."""

import argparse
from datetime import datetime, timezone
import gzip
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import sys
import threading
from concurrent.futures import ThreadPoolExecutor

from cleanup_archive import archive
from cleanup_discover import discover
from cleanup_volume_cache import prepare as prepare_volume_cache, clear as clear_volume_cache
from cleanup_docker import (
    guard as guard_container,
    identity as container_identity,
    inspect as inspect_container,
    retire,
)
from cleanup_retirement import retirement_open_files
from cleanup_state import (
    Refused,
    absolute,
    allocated,
    docker_state,
    ensure_idle,
    ensure_volume_idle,
    git,
    inventory,
    open_files,
    overlaps,
    run,
    same_volume,
    volume_identity,
    worktrees,
)

CACHE_DIRS = ("incremental", "deps", "build", ".fingerprint", "examples")
KINDS = {
    "cargo-cache",
    "archive-tree",
    "archive-file",
    "worktree",
    "docker-volume",
    "docker-container",
    "discard-tree",
    "docker-cargo-cache",
}
EVENT_LOCK = threading.Lock()


def now():
    return datetime.now(timezone.utc).isoformat()


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def event(output, **record):
    record["at_utc"] = now()
    with EVENT_LOCK:
        with (output / "actions.jsonl").open("a") as stream:
            stream.write(json.dumps(record) + "\n")
            stream.flush()
            os.fsync(stream.fileno())


def free_space(repo):
    value = shutil.disk_usage(repo)
    return {
        "at_utc": now(),
        "filesystem_path": str(repo),
        "free_bytes": value.free,
        "free_GB": value.free / 1e9,
        "free_GiB": value.free / 1024**3,
    }


def byte_target(gb):
    if not math.isfinite(gb) or gb < 0:
        raise Refused("Disk thresholds must be finite, nonnegative GB values")
    return int(gb * 1e9)


def check_space(args):
    state = free_space(Path(args.repo).resolve())
    state.update(
        min_free_bytes=byte_target(args.min_free_gb),
        reclaim_target_bytes=byte_target(args.reclaim_gb),
        units="GB = 1,000,000,000 bytes; GiB = 1,073,741,824 bytes",
    )
    state["cleanup_required"] = state["free_bytes"] < state["min_free_bytes"]
    state["action"] = (
        "Plan with --discover --authorization or an exact scope, then apply. Preserve active work and pins."
        if state["cleanup_required"]
        else "Headroom threshold satisfied"
    )
    print(json.dumps(state, indent=2))
    return 2 if state["cleanup_required"] else 0


def pins(scope):
    paths = [absolute(x) for x in scope.get("protected_paths", [])]
    return list(dict.fromkeys([*paths, *(p.resolve() for p in paths)]))


def eligible(action):
    if action.get("released") is not True:
        raise Refused("Resource needs a recorded owner release or user-authorized discovery")
    if not action.get("owner", "").strip() or not action.get("reason", "").strip():
        raise Refused("Each action needs an owner and a disposal reason")
    if action.get("kind") == "discard-tree" and action.get("disposable_generated") is not True:
        raise Refused("Discarding a tree requires explicit disposable_generated: true")
    if action.get("kind") not in KINDS:
        raise Refused(f"Unsupported action kind: {action.get('kind')}")


def validate_path(path, kind, repo, output, registered):
    if path.is_symlink() or path == path.parent:
        raise Refused(f"Unsafe selected root: {path}")
    # Never consume the pass, Git object DB, home directory, or the invoking checkout.
    common = Path(git(repo, "rev-parse", "--path-format=absolute", "--git-common-dir").decode().strip())
    for protected in [output, common]:
        if overlaps(path, protected):
            raise Refused(f"Selection overlaps control/recovery data: {path}")
    home = Path.home().resolve()
    if path == home or path in home.parents:
        raise Refused("Cannot remove the home directory or its ancestors")
    if kind == "worktree":
        cwd = Path.cwd().resolve()
        if str(path) not in registered or path == repo or path == cwd or path in cwd.parents:
            raise Refused("Only another registered, idle linked worktree can be removed")
        if not (path / ".git").is_file() or (path / ".git").is_symlink():
            raise Refused("Primary/independent repositories cannot be removed")
        if "locked" in registered[str(path)]:
            raise Refused("Worktree is locked")
    elif any(path == Path(p) or path in Path(p).parents for p in registered):
        raise Refused("Use the worktree action for a registered checkout")


def observations(repo, roots, sizes=False):
    result = []
    for record in worktrees(repo):
        path = Path(record["worktree"])
        row = dict(record)
        try:
            row["status"] = git(path, "status", "--porcelain=v1", "--untracked-files=all").decode()
            if sizes:
                row["allocated_bytes_estimate"] = allocated(path)
        except (Refused, OSError, TimeoutError) as error:
            row["inspection_error"] = str(error)
        result.append(row)
    large = []
    errors = []
    skipped = {".git", "node_modules", "deps", "incremental", ".fingerprint", "build"}
    for raw in roots:
        root = absolute(raw)
        for base, dirs, files in os.walk(root, onerror=lambda e: errors.append(str(e))):
            dirs[:] = [d for d in dirs if d not in skipped]
            for name in files:
                path = Path(base) / name
                try:
                    info = path.lstat()
                    if info.st_size >= 5 * 1024**2:
                        large.append(
                            {"path": str(path), "bytes": info.st_size, "allocated_bytes": info.st_blocks * 512}
                        )
                except OSError as error:
                    errors.append(str(error))
    large.sort(key=lambda row: row["allocated_bytes"], reverse=True)
    return {
        "worktrees": result,
        "large_files": large,
        "scan_errors": errors,
        "note": "Allocation estimates may overlap or share APFS blocks. Age/size is not release authorization.",
    }


def make_plan(args):
    repo = Path(args.repo).resolve()
    output = absolute(args.out)
    if overlaps(repo, output):
        raise Refused("Pass directory must be outside the repository")
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    scope = json.loads(Path(args.scope).read_text()) if args.scope else {"version": 1, "actions": []}
    if scope.get("version") != 1:
        raise Refused("Unsupported scope version")
    scope["protected_paths"] = list(
        dict.fromkeys([*scope.get("protected_paths", []), *getattr(args, "protect_path", [])])
    )
    scope["protected_containers"] = list(
        dict.fromkeys([*scope.get("protected_containers", []), *getattr(args, "protect_container", [])])
    )
    plan = {
        "version": 1,
        "created_utc": now(),
        "repository": str(repo),
        "pass_directory": str(output),
        "scope": scope,
        "actions": [],
        "blocked": [],
        "free_space": free_space(repo),
    }
    plan["reclaim_target_bytes"] = byte_target(args.reclaim_gb)
    registered = {r["worktree"]: r for r in worktrees(repo)}
    try:
        containers, opened = docker_state(), open_files()
        write(output / "runtime.json", {"containers": containers, "open_files": opened})
    except (Refused, OSError, TimeoutError) as error:
        containers, opened = None, None
        plan["runtime_error"] = str(error)
    cache_worktrees = [str(absolute(p)) for p in getattr(args, "cache_worktree", [])]
    if cache_worktrees and (
        not getattr(args, "discover_caches", False) or any(p not in registered for p in cache_worktrees)
    ):
        raise Refused("--cache-worktree requires --discover-caches and registered checkout paths")
    if getattr(args, "discover", False) or getattr(args, "discover_caches", False):
        if containers is None:
            raise Refused(plan["runtime_error"])
        scope, plan["discovery_skipped"] = discover(
            repo,
            scope,
            registered,
            containers,
            opened,
            args.authorization or "",
            caches_only=getattr(args, "discover_caches", False),
            retire_idle_sessions=getattr(args, "retire_idle_sessions", False),
            cache_worktrees=[str(absolute(p)) for p in getattr(args, "cache_worktree", [])],
        )
        plan["scope"] = scope
    write(output / "scope.json", scope)
    # A planned container removal may release its bind mount before a later
    # worktree action. Apply still inspects the real current consumer inventory.
    retiring = {a.get("name") for a in scope.get("actions", []) if a["kind"] == "docker-container"}
    path_containers = [c for c in containers or [] if c["id"] not in retiring]
    chosen = []
    for action in scope.get("actions", []):
        try:
            eligible(action)
            if action["kind"] == "docker-cargo-cache":
                if containers is None:
                    raise Refused(plan["runtime_error"])
                chosen.append(prepare_volume_cache(action, scope, containers, output))
                continue
            if action["kind"] == "docker-container":
                info = inspect_container(action["name"])
                guard_container(info, action, scope)
                chosen.append({**action, "name": info["Id"], "identity": container_identity(info)})
                continue
            if action["kind"] == "docker-volume":
                if containers is None:
                    raise Refused(plan["runtime_error"])
                name = action["name"]
                ensure_volume_idle(name, scope.get("protected_volumes", []), containers)
                chosen.append({**action, "identity": volume_identity(name)})
                continue
            path = absolute(action["path"])
            validate_path(path, action["kind"], repo, output, registered)
            paths = [path]
            if action["kind"] == "cargo-cache":
                # Select only generated children. Direct executables/helpers remain in place.
                target = absolute(action["target_root"])
                if (
                    not target.is_dir()
                    or target.is_symlink()
                    or path.parent != target
                    or not path.is_dir()
                    or path.name not in {"debug", "release"}
                    or not (target / ".rustc_info.json").is_file()
                ):
                    raise Refused("cargo-cache needs a direct debug/release profile under a real Cargo target_root")
                paths = [path / child for child in CACHE_DIRS if (path / child).exists()]
            if containers is None:
                raise Refused(plan["runtime_error"])
            if action["kind"] == "archive-file" and not path.is_file():
                raise Refused("archive-file needs a regular file")
            if action["kind"] in {"archive-tree", "worktree", "discard-tree"} and not path.is_dir():
                raise Refused("Archive tree must be a directory")
            action_opened = retirement_open_files(path, opened, action.get("retire_idle_sessions", False))
            ensure_idle(paths, pins(scope), path_containers, action_opened)
            # The profile lock and open executables are outside its generated children.
            if action["kind"] == "cargo-cache":
                ensure_idle([path], [], path_containers, action_opened)
                # Cargo hard-links object files between incremental and deps.
                # Check all siblings before removing any: unlinking one changes
                # another's ctime even though no external writer touched it.
                chosen.append(
                    {
                        **action,
                        "path": str(path),
                        "guard_path": str(path),
                        "cache_snapshots": {str(item): inventory(item)[1] for item in paths},
                    }
                )
                continue
            for item in paths:
                if action["kind"] == "worktree":
                    info = item.stat()
                    snapshot = {"device": info.st_dev, "inode": info.st_ino}
                else:
                    snapshot = inventory(item)[1]
                selection = {**action, "path": str(item), "snapshot": snapshot, "guard_path": str(path)}
                if action["kind"] == "worktree":
                    selection["git_head"] = git(item, "rev-parse", "HEAD").decode().strip()
                    selection["staged_sha256"] = hashlib.sha256(
                        git(item, "diff", "--cached", "--binary", "--full-index", "--no-ext-diff", "--no-textconv")
                    ).hexdigest()
                chosen.append(selection)
        except (Refused, OSError, KeyError, TimeoutError) as error:
            plan["blocked"].append({"selection": action, "reason": str(error)})
    # Refuse overlapping selections rather than changing ordering semantics.
    for i, item in enumerate(chosen):
        if "path" in item and any(
            overlaps(item["path"], other["path"]) for other in chosen[i + 1 :] if "path" in other
        ):
            raise Refused("Overlapping path selections; use one action per resource")
    names = [x["name"] for x in chosen if x["kind"] in {"docker-volume", "docker-cargo-cache"}]
    if len(names) != len(set(names)):
        raise Refused("Duplicate volume selection")
    plan["actions"] = chosen
    plan["protected_presence"] = {str(p): p.exists() for p in pins(scope)}
    write(output / "inventory.json", observations(repo, scope.get("scan_roots", []), getattr(args, "sizes", False)))
    write(output / "plan.json", plan)
    print(
        json.dumps(
            {
                "plan": str(output / "plan.json"),
                "selected": len(chosen),
                "blocked": plan["blocked"],
                "mode": "inventory only; nothing deleted",
                "next": "Review plan.json, then apply --plan " + str(output / "plan.json")
                if chosen
                else "No removals selected. Use --discover --authorization for authorized cleanup.",
            },
            indent=2,
        )
    )
    return 0


def apply_action(repo, output, plan, action):
    eligible(action)
    scope = plan["scope"]
    if action["kind"] == "docker-cargo-cache":
        return clear_volume_cache(action, scope, output, event)
    if action["kind"] == "docker-container":
        return retire(action, scope, output, plan["pass_id"], event, repo)
    containers = docker_state()
    if action["kind"] == "docker-volume":
        name = action["name"]
        ensure_volume_idle(name, scope.get("protected_volumes", []), containers)
        if not same_volume(action["identity"], volume_identity(name)):
            raise Refused("Volume identity changed since plan")
        event(output, state="removing", action=action)
        run(["docker", "volume", "rm", name])
        remaining = run(["docker", "volume", "ls", "--format", "{{.Name}}"])
        if name in remaining.stdout.decode().splitlines():
            raise Refused("Volume still exists after removal")
        return {}
    path = absolute(action["path"])
    if action["kind"] == "discard-tree" and not path.is_dir():
        raise Refused("Discard tree must be a directory")
    registered = {r["worktree"]: r for r in worktrees(repo)}
    validate_path(path, action["kind"], repo, output, registered)
    ensure_idle(
        [path],
        pins(scope),
        containers,
        retirement_open_files(path, open_files(), action.get("retire_idle_sessions", False)),
    )
    if action["kind"] == "cargo-cache":
        guard = absolute(action["guard_path"])
        if "cache_snapshots" in action:
            target = absolute(action["target_root"])
            if path != guard or guard.parent != target or guard.name not in {"debug", "release"}:
                raise Refused("Invalid Cargo target profile")
            if not (target / ".rustc_info.json").is_file():
                raise Refused("Cargo target metadata is missing")
            children = [absolute(raw) for raw in action["cache_snapshots"]]
            for child in children:
                if child.parent != path or child.name not in CACHE_DIRS:
                    raise Refused("Invalid generated cache child")
                if inventory(child)[1] != action["cache_snapshots"][str(child)]:
                    raise Refused("Cache changed since plan; preserve the whole profile")
            # One final activity inspection covers the whole profile. Its
            # generated siblings are one planned job, not a replay or refresh.
            ensure_idle(
                [guard],
                pins(scope),
                docker_state(),
                retirement_open_files(guard, open_files(), action.get("retire_idle_sessions", False)),
            )
            event(output, state="removing", action=action)
            for child in children:
                shutil.rmtree(child)
            if any(child.exists() for child in children):
                raise Refused("Generated cache child remains after removal")
            return {"removed_cache_paths": [str(child) for child in children]}
        if path.parent != guard or path.name not in CACHE_DIRS:
            raise Refused("Invalid generated cache child")
        target = absolute(action["target_root"])
        if guard.parent != target or guard.name not in {"debug", "release"}:
            raise Refused("Invalid Cargo target profile")
        ensure_idle(
            [guard],
            [],
            containers,
            retirement_open_files(guard, open_files(), action.get("retire_idle_sessions", False)),
        )
    checkout = action["kind"] == "worktree"
    if checkout and "device" in action["snapshot"]:
        info = path.stat()
        if (info.st_dev, info.st_ino) != (action["snapshot"]["device"], action["snapshot"]["inode"]):
            raise Refused("Worktree root was replaced since plan")
    elif inventory(path, checkout)[1] != action["snapshot"]:
        raise Refused("Path identity or contents changed since plan; make a fresh plan")
    if checkout:
        if git(path, "rev-parse", "HEAD").decode().strip() != action["git_head"]:
            raise Refused("Worktree HEAD changed since plan")
        staged = git(path, "diff", "--cached", "--binary", "--full-index", "--no-ext-diff", "--no-textconv")
        if hashlib.sha256(staged).hexdigest() != action["staged_sha256"]:
            raise Refused("Worktree index changed since plan")
    recovery = {}
    if action["kind"] not in {"cargo-cache", "discard-tree"}:
        if checkout:
            if git(path, "ls-files", "--unmerged"):
                raise Refused("Resolve or separately archive an unmerged index before removing a worktree")
            # Preserve staged bytes even when the working file differs from the index.
            # A worktree's raw files alone cannot recover that intermediate version.
            head = git(path, "rev-parse", "HEAD").decode().strip()
            status = git(path, "status", "--porcelain=v1", "--untracked-files=all").decode()
            patches = {}
            for label, flags in [("staged", ["--cached"]), ("unstaged", [])]:
                data = git(path, "diff", *flags, "--binary", "--full-index", "--no-ext-diff", "--no-textconv")
                target = output / (hashlib.sha256(str(path).encode()).hexdigest()[:20] + f"-{label}.patch.gz")
                with gzip.open(target, "wb") as stream:
                    stream.write(data)
                with gzip.open(target, "rb") as stream:
                    if stream.read() != data:
                        raise Refused("Worktree patch readback mismatch")
                patches[label] = {"path": str(target), "sha256": hashlib.sha256(data).hexdigest()}
        recovery = archive(repo, path, output, plan["pass_id"], checkout)
        if checkout:
            recovery["head"] = head
            recovery["head_ref"] = recovery["recovery_ref"] + "-head"
            recovery["status"] = status
            recovery["patches"] = patches
            git(repo, "update-ref", recovery["head_ref"], head)
        event(output, state="archived", action=action, recovery=recovery)
        # Archival may take time. Refresh every ownership guard before unlinking.
        ensure_idle(
            [path],
            pins(scope),
            docker_state(),
            retirement_open_files(path, open_files(), action.get("retire_idle_sessions", False)),
        )
        if inventory(path, checkout)[1] != recovery["snapshot"]:
            raise Refused("Path changed after archival; preserve it")
        if checkout:
            if git(path, "rev-parse", "HEAD").decode().strip() != head:
                raise Refused("Worktree HEAD changed during archival")
            for label, flags in [("staged", ["--cached"]), ("unstaged", [])]:
                data = git(path, "diff", *flags, "--binary", "--full-index", "--no-ext-diff", "--no-textconv")
                if hashlib.sha256(data).hexdigest() != patches[label]["sha256"]:
                    raise Refused("Worktree index or edits changed during archival")
    event(output, state="removing", action=action, recovery=recovery)
    if checkout:
        # --force permits backed-up edits/ignored files, but never overrides a locked tree.
        run(["git", "-C", str(repo), "worktree", "remove", "--force", str(path)])
    elif path.is_dir():
        shutil.rmtree(path)
    else:
        path.unlink()
    if path.exists() or path.is_symlink():
        raise Refused("Path remains after removal")
    return recovery


def apply_plan(args):
    path = absolute(args.plan)
    plan = json.loads(path.read_text())
    output = absolute(plan["pass_directory"])
    if path != output / "plan.json" or plan.get("version") != 1:
        raise Refused("Invalid plan location or version")
    repo = absolute(plan["repository"])
    jobs = getattr(args, "jobs", 1)
    if not 1 <= jobs <= 4:
        raise Refused("Cleanup jobs must be between 1 and 4")
    if jobs > 1 and any(
        a["kind"] not in {"worktree", "docker-container", "archive-tree", "archive-file"} for a in plan["actions"]
    ):
        raise Refused("Parallel cleanup supports disjoint archives, worktrees or containers only")
    # A pass can be attempted exactly once, including failed or interrupted passes.
    with (output / "apply.started").open("x") as stream:
        stream.write(now() + "\n")
    plan["pass_id"] = (
        datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        + "-"
        + hashlib.sha256(str(output).encode()).hexdigest()[:12]
    )
    before = free_space(repo)
    results = []

    def attempt(action):
        event(output, state="started", action=action)
        try:
            recovery = apply_action(repo, output, plan, action)
            result = {"state": "removed", "action": action, "recovery": recovery}
            event(output, **result)
        except (Refused, OSError, KeyError, TimeoutError) as error:
            result = {"state": "failed_or_preserved", "action": action, "error": str(error)}
            event(output, **result)
        return result

    try:
        with ThreadPoolExecutor(max_workers=jobs) as pool:
            for offset in range(0, len(plan["actions"]), jobs):
                batch = plan["actions"][offset : offset + jobs]
                completed = list(pool.map(attempt, batch))
                results.extend(completed)
                # Settle already-running independent jobs before reporting a
                # failure. Never start another batch or replay an operation.
                if any(r["state"] != "removed" for r in completed):
                    break
    finally:
        # The executor settles in-flight jobs before this block, including when
        # the controller receives Ctrl-C. The journal is authoritative if a
        # completed batch's return values were interrupted while being collected.
        touched, terminal = set(), {}
        ledger = output / "actions.jsonl"
        if ledger.exists():
            for line in ledger.read_text().splitlines():
                row = json.loads(line)
                key = json.dumps(row["action"], sort_keys=True)
                touched.add(key)
                if row["state"] in {"removed", "failed_or_preserved"}:
                    terminal[key] = row
        results = [
            terminal[json.dumps(a, sort_keys=True)]
            for a in plan["actions"]
            if json.dumps(a, sort_keys=True) in terminal
        ]
        unattempted = [a for a in plan["actions"] if json.dumps(a, sort_keys=True) not in touched]
        uncertain = [
            a
            for a in plan["actions"]
            if json.dumps(a, sort_keys=True) in touched and json.dumps(a, sort_keys=True) not in terminal
        ]
        after = free_space(repo)
        present = {raw: Path(raw).exists() for raw in plan["protected_presence"]}
        lost_pins = [raw for raw, existed in plan["protected_presence"].items() if existed and not present[raw]]
        report = {
            "finished_utc": now(),
            "before": before,
            "after": after,
            "outcome": "no_actions_selected" if not plan["actions"] else "attempted",
            "net_free_bytes_change": after["free_bytes"] - before["free_bytes"],
            "results": results,
            "protected_presence": present,
            "lost_pins": lost_pins,
            "not_attempted": unattempted,
            "uncertain_actions": uncertain,
            "interrupted": isinstance(sys.exc_info()[1], KeyboardInterrupt),
            "blocked_at_plan": plan["blocked"],
            "note": "Net change includes concurrent I/O; allocation estimates are not physical recovery. "
            "Keep recovery refs/objects and manifests. No product build, test, Store call, "
            "global prune or measurement scheduling is performed. Only explicitly selected "
            "retired daemons may be stopped; container actions retain their mounted data.",
        }
        report["reclaim_target_bytes"] = plan.get("reclaim_target_bytes", 0)
        report["reclaim_target_met"] = (
            report["reclaim_target_bytes"] == 0 or report["net_free_bytes_change"] >= report["reclaim_target_bytes"]
        )
        write(output / "report.json", report)
    print(
        json.dumps(
            {
                "report": str(output / "report.json"),
                "removed": sum(r["state"] == "removed" for r in results),
                "net_free_bytes_change": report["net_free_bytes_change"],
                "lost_pins": lost_pins,
                "reclaim_target_met": report["reclaim_target_met"],
            },
            indent=2,
        )
    )
    if lost_pins or any(r["state"] != "removed" for r in results):
        return 1
    return 0 if report["reclaim_target_met"] else 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    check = commands.add_parser("check", help="Check free space: exit 2 below threshold, otherwise 0")
    check.add_argument("--repo", default=str(Path(__file__).resolve().parents[1]))
    check.add_argument("--min-free-gb", type=float, default=100)
    check.add_argument("--reclaim-gb", type=float, default=300)
    plan = commands.add_parser("plan", help="Inventory only; no deletion or recovery-ref writes")
    plan.add_argument("--repo", default=str(Path(__file__).resolve().parents[1]))
    plan.add_argument("--scope", help="Explicit release selections and protected resources (JSON)")
    plan.add_argument(
        "--discover", action="store_true", help="Select idle linked worktrees and stopped LayerFS containers"
    )
    plan.add_argument(
        "--discover-caches",
        action="store_true",
        help="Select idle generated Cargo caches in linked worktrees, retaining binaries and helpers",
    )
    plan.add_argument(
        "--cache-worktree",
        action="append",
        default=[],
        help="Restrict --discover-caches to this registered checkout; repeat as needed; explicitly allows the primary checkout",
    )
    plan.add_argument(
        "--retire-idle-sessions",
        action="store_true",
        help="Allow retired checkouts held only by idle agent/shell cwd handles; active files/commands stay protected",
    )
    plan.add_argument("--authorization", help="User instruction authorizing discovery; no separate release file needed")
    plan.add_argument(
        "--protect-path", action="append", default=[], help="Preserve this path and overlapping resources"
    )
    plan.add_argument("--protect-container", action="append", default=[], help="Preserve exact container ID or name")
    plan.add_argument("--sizes", action="store_true", help="Also run potentially expensive du scans of all worktrees")
    plan.add_argument("--out", required=True, help="New pass directory outside the repository")
    plan.add_argument("--reclaim-gb", type=float, default=0, help="Net host-space recovery target, decimal GB")
    apply = commands.add_parser("apply", help="Attempt a saved plan once; archive selected data before removal")
    apply.add_argument("--plan", required=True)
    apply.add_argument(
        "--jobs", type=int, default=1, help="Up to 4 independent archive/worktree/container jobs; default 1"
    )
    args = parser.parse_args()
    try:
        if args.command == "check":
            return check_space(args)
        return make_plan(args) if args.command == "plan" else apply_plan(args)
    except (Refused, OSError, KeyError, ValueError, TimeoutError) as error:
        print(f"Cleanup refused: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
