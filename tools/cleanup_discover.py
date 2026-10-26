"""Translate an authorized repository cleanup into exact, reviewable selections."""

from pathlib import Path
import os

from cleanup_state import Refused, ensure_idle, host_bind_path, overlaps
from cleanup_retirement import retirement_open_files


def repository_container(container, repo, registered):
    if container.get("labels", {}).get("layerfs.dev.worktree"):
        return True
    roots = [repo, *(Path(p) for p in registered)]
    for mount in container["mounts"]:
        if mount.get("Type") == "bind" and any(overlaps(host_bind_path(mount["Source"]), root) for root in roots):
            return True
        if mount.get("Destination") == "/layerfs-store":
            return True
    return False


def cargo_profiles(worktree):
    """Find real Cargo targets, including the repository's nested Linux targets."""
    # Keep discovering the retired target path for other worktrees awaiting rebase.
    for root in (worktree / "target", worktree / "core/target"):
        if not root.is_dir() or root.is_symlink():
            continue
        for base, dirs, files in os.walk(root):
            target = Path(base)
            dirs[:] = [
                d
                for d in dirs
                if d not in {"debug", "release", "registry", "git", "node_modules"} and not (target / d).is_symlink()
            ]
            if len(target.relative_to(root).parts) >= 3:
                dirs[:] = []
            if ".rustc_info.json" in files:
                for profile in ("debug", "release"):
                    if (target / profile).is_dir() and not (target / profile).is_symlink():
                        yield target, target / profile


def discover(
    repo,
    scope,
    registered,
    containers,
    opened,
    authorization,
    caches_only=False,
    retire_idle_sessions=False,
    cache_worktrees=None,
):
    """User authorization replaces a separate release file, never an activity guard."""
    if not authorization.strip():
        raise Refused("Discovery needs --authorization recording the user's cleanup instruction")
    scope = {**scope, "actions": list(scope.get("actions", []))}
    scope["authorization"] = authorization
    protected = [Path(p).resolve() for p in scope.get("protected_paths", [])]
    protected_containers = set(scope.get("protected_containers", []))
    skipped = []
    selected = {a.get("path") or a.get("name") for a in scope["actions"]}
    retiring = {a.get("name") for a in scope["actions"] if a["kind"] == "docker-container"}
    path_containers = [
        c
        for c in containers
        if c["running"]
        and c["id"] not in retiring
        or not repository_container(c, repo, registered)
        or c["id"] in protected_containers
        or c["name"].lstrip("/") in protected_containers
        or any(
            m.get("Type") == "bind" and any(overlaps(host_bind_path(m["Source"]), p) for p in protected)
            for m in c["mounts"]
        )
    ]
    for raw, record in registered.items():
        path = Path(raw)
        explicit_cache = caches_only and cache_worktrees and raw in cache_worktrees
        if (path == repo and not explicit_cache) or path in repo.parents or "locked" in record or raw in selected:
            continue
        if caches_only:
            if cache_worktrees and raw not in cache_worktrees:
                continue
            for target, profile in cargo_profiles(path):
                try:
                    ensure_idle([profile], protected, containers, opened)
                except Refused as error:
                    skipped.append({"kind": "cargo-cache", "path": str(profile), "reason": str(error)})
                    continue
                scope["actions"].append(
                    {
                        "kind": "cargo-cache",
                        "path": str(profile),
                        "target_root": str(target),
                        "released": True,
                        "owner": "user-authorized cleanup",
                        "reason": authorization,
                    }
                )
            continue
        try:
            ensure_idle([path], protected, path_containers, retirement_open_files(path, opened, retire_idle_sessions))
        except Refused as error:
            skipped.append({"kind": "worktree", "path": raw, "reason": str(error)})
            continue
        scope["actions"].append(
            {
                "kind": "worktree",
                "path": raw,
                "released": True,
                "retire_idle_sessions": retire_idle_sessions,
                "owner": "user-authorized cleanup",
                "reason": authorization,
            }
        )
    for container in containers:
        if caches_only:
            break
        if container["running"] or container.get("status") not in {"exited", "created"}:
            continue
        if not repository_container(container, repo, registered):
            continue
        if container["id"] in protected_containers or container["name"].lstrip("/") in protected_containers:
            continue
        if any(
            m.get("Type") == "bind" and any(overlaps(host_bind_path(m["Source"]), p) for p in protected)
            for m in container["mounts"]
        ):
            continue
        if container["id"] not in selected:
            scope["actions"].append(
                {
                    "kind": "docker-container",
                    "name": container["id"],
                    "released": True,
                    "owner": "user-authorized cleanup",
                    "reason": authorization,
                }
            )
    # Containers first: removing stopped containers releases their bind mounts.
    scope["actions"].sort(key=lambda action: action["kind"] != "docker-container")
    return scope, skipped
