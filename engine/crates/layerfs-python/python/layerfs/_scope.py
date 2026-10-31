import re

from ._errors import LayerFSError
from ._util import OMIT


HISTORY_NAME = re.compile(r"^[a-z0-9](?:[a-z0-9_.-]{0,61}[a-z0-9])?$")
IDENTITY = re.compile(r"^[0-9a-f]{66}$")
HISTORY = {"branch.list_commits", "branch.graph", "branch.changed_paths", "branch.diff_commits", "branch.diff_branches"}
PATHS = {
    "fs.list": ["path"], "fs.glob": ["path"], "fs.grep": ["path"],
    "fs.stat": ["path"], "fs.read": ["path"], "fs.blame": ["path"],
    "fs.write": ["path"], "fs.edit": ["path"], "fs.mkdir": ["path"],
    "fs.rm": ["path"], "fs.cp": ["from", "to"], "fs.mv": ["from", "to"],
}
FILE_REFS = set(PATHS.keys()) | {"fs.apply", "fs.diff_files"}


def out_of_scope(kind):
    return LayerFSError("InvalidInput", f"out of scope: {kind}", None)


def _canonical(path):
    if not isinstance(path, str):
        return False
    if path == "/":
        return True
    if not path.startswith("/"):
        return False
    parts = path.split("/")[1:]
    return not any(not part or part in (".", "..") for part in parts)


def check_scope(scope):
    if scope is OMIT:
        return None
    if not isinstance(scope, dict):
        raise LayerFSError("InvalidInput", "scope must be { folder, branch? }", None)
    for key in scope:
        if key not in ("folder", "branch"):
            raise LayerFSError("InvalidInput", f'unknown scope option "{key}"', None)
    folder = scope.get("folder")
    if not _canonical(folder):
        raise LayerFSError("InvalidInput", "scope.folder must be a canonical project-root absolute path", None)
    branch = scope.get("branch")
    if branch is not None and (not isinstance(branch, str) or not HISTORY_NAME.match(branch)):
        raise LayerFSError("InvalidInput", "scope.branch must be a slash-free HistoryName namespace prefix", None)
    result = {"folder": folder}
    if branch is not None:
        result["branch"] = branch
    return result


def _inside(path, folder):
    if not _canonical(path):
        return False
    if folder == "/":
        return True
    if path == folder:
        return True
    return path.startswith(f"{folder}/")


def _branch_name(scope, name):
    if not scope.get("branch"):
        raise out_of_scope("branch")
    if not isinstance(name, str) or not HISTORY_NAME.match(name):
        raise out_of_scope("branch")
    composed = f"{scope['branch']}.{name}"
    if not HISTORY_NAME.match(composed):
        raise out_of_scope("branch")
    return composed


def _ref(scope, name):
    return name if isinstance(name, str) and IDENTITY.match(name) else _branch_name(scope, name)


def _change_paths(change):
    if isinstance(change, dict) and change.get("op") in ("cp", "mv"):
        return [change.get("from"), change.get("to")]
    return [change.get("path") if isinstance(change, dict) else None]


def _paths_checked(scope, method, given):
    if method == "fs.apply":
        changes = given.get("changes")
        if not isinstance(changes, list):
            raise out_of_scope("path")
        for change in changes:
            for path in _change_paths(change):
                if not _inside(path, scope["folder"]):
                    raise out_of_scope("path")
    elif method == "fs.diff_files":
        left = given.get("left") or {}
        right = given.get("right") or {}
        if not _inside(left.get("path"), scope["folder"]) or not _inside(right.get("path"), scope["folder"]):
            raise out_of_scope("path")
    else:
        for key in PATHS.get(method, []):
            if not _inside(given.get(key), scope["folder"]):
                raise out_of_scope("path")


def scope_args(scope, method, given):
    if not scope:
        return given
    if method in HISTORY:
        raise out_of_scope("history")
    if method == "events.subscribe":
        raise out_of_scope("events")
    given = dict(given)
    if method in FILE_REFS:
        _paths_checked(scope, method, given)
        given["branch"] = _ref(scope, given.get("branch"))
    if method == "fs.diff_files":
        for side in ("left", "right"):
            value = given.get(side) or {}
            given[side] = {**value, "commit": _ref(scope, value.get("commit")), "path": value.get("path")}
    elif method == "branch.fork":
        given["name"] = _branch_name(scope, given.get("name"))
        given["from"] = _ref(scope, given.get("from"))
    elif method == "branch.merge":
        given["branch"] = _branch_name(scope, given.get("branch"))
        given["with"] = _branch_name(scope, given.get("with"))
        given["name"] = _branch_name(scope, given.get("name"))
    elif method == "branch.publish":
        given["branch"] = _branch_name(scope, given.get("branch"))
    elif method == "project.list_branches":
        if not scope.get("branch"):
            raise out_of_scope("branch")
    elif method == "workspace.mount":
        if "branch" in given and given["branch"] != "main":
            raise out_of_scope("branch")
        given["branch"] = _branch_name(scope, "main")
    elif method == "workspace.exec":
        if "cwd" in given and given["cwd"] != scope["folder"]:
            raise out_of_scope("path")
        given["cwd"] = scope["folder"]
    return given


def scope_branches(scope, items):
    prefix = f"{scope['branch']}."
    return [
        {**item, "name": item["name"][len(prefix):]}
        for item in items
        if isinstance(item.get("name"), str) and item["name"].startswith(prefix)
    ]
