from ._errors import LayerFSError
from ._util import (
    OMIT, body, checked_message, commit_options, deployment, diff_options,
    page, page_of, sandbox_arg, workspace_args, write_options,
)


class FS:
    def __init__(self, client):
        self.client = client

    async def read(self, ref, path, start_line=1, end_line=OMIT, options=OMIT):
        if options is OMIT:
            options = {}
        if not isinstance(options, dict):
            raise LayerFSError("InvalidInput", "options must be { binary?, offset? }")
        for key in options:
            if key not in ("binary", "offset"):
                raise LayerFSError("InvalidInput", f'unknown fs.read option "{key}"')
        binary = options.get("binary", False)
        offset = options.get("offset")
        if not isinstance(binary, bool):
            raise LayerFSError("InvalidInput", "binary must be a boolean")
        if offset is not None and not isinstance(offset, str):
            raise LayerFSError("InvalidInput", "offset must be a string or null")
        args = {"branch": ref, "path": path, "startLine": start_line, "endLine": end_line, "binary": binary}
        if isinstance(offset, str):
            args["offset"] = offset
        return await self.client.call("fs.read", args)

    async def list(self, ref, path="/", page_size=50, page_pos=0):
        return await self.client.call("fs.list", {"branch": ref, "path": path, **page(page_size, page_pos)})

    async def stat(self, ref, path):
        return await self.client.call("fs.stat", {"branch": ref, "path": path})

    async def grep(self, ref, pattern, path="/", page_size=50, page_pos=0):
        return await self.client.call("fs.grep", {"branch": ref, "pattern": pattern, "path": path, **page(page_size, page_pos)})

    async def glob(self, ref, pattern, path="/", page_size=50, page_pos=0):
        return await self.client.call("fs.glob", {"branch": ref, "pattern": pattern, "path": path, **page(page_size, page_pos)})

    async def diff_files(self, left_ref, left_path, right_ref, right_path):
        return await self.client.call("fs.diff_files", {
            "branch": left_ref,
            "left": {"commit": left_ref, "path": left_path},
            "right": {"commit": right_ref, "path": right_path},
        })

    async def blame(self, ref, path, from_line=1):
        return await self.client.call("fs.blame", {"branch": ref, "path": path, "fromLine": from_line})

    async def write(self, branch, path, content, options=OMIT):
        data = {"content": content} if isinstance(content, str) else {"bytes": body(content)}
        return await self.client.call("fs.write", {"branch": branch, "path": path, **data, **write_options(options)})

    async def edit(self, branch, path, old_text, new_text, message=OMIT):
        return await self.client.call("fs.edit", {
            "branch": branch, "path": path, "oldText": old_text, "newText": new_text,
            "message": checked_message(message),
        })

    async def apply(self, branch, changes, message=OMIT):
        if isinstance(changes, list):
            changes = [self._change(change) for change in changes]
        return await self.client.call("fs.apply", {"branch": branch, "changes": changes, "message": checked_message(message)})

    @staticmethod
    def _change(change):
        if not isinstance(change, dict) or change.get("op") != "write":
            return change
        result = dict(change)
        if "content" in result and not isinstance(result["content"], str) and "bytes" not in result:
            result["bytes"] = body(result.pop("content"))
        if "bytes" in result:
            result["bytes"] = body(result["bytes"])
        return result

    async def mkdir(self, branch, path, options=OMIT):
        return await self.client.call("fs.mkdir", {"branch": branch, "path": path, "recursive": True, **write_options(options)})

    async def rm(self, branch, path, message=OMIT):
        return await self.client.call("fs.rm", {"branch": branch, "path": path, "recursive": True, "message": checked_message(message)})

    async def cp(self, branch, from_path, to_path, message=OMIT):
        return await self.client.call("fs.cp", {"branch": branch, "from": from_path, "to": to_path, "recursive": True, "message": checked_message(message)})

    async def mv(self, branch, from_path, to_path, message=OMIT):
        return await self.client.call("fs.mv", {"branch": branch, "from": from_path, "to": to_path, "message": checked_message(message)})


class Project:
    def __init__(self, client):
        self.client = client

    async def list_branches(self, page_size=50, page_pos=0):
        return await self.client.call("project.list_branches", page(page_size, page_pos))

    async def list_sandboxes(self, page_size=50, page_pos=0):
        page(page_size, page_pos)
        value = await self.client._invoke("project.list_sandboxes", {})
        items = [{key: item.get(key) for key in ("sandbox", "name", "image", "workspaces")} for item in value["items"]]
        return page_of(items, page_size, page_pos)


class Branch:
    def __init__(self, client):
        self.client = client

    async def list_commits(self, branch, page_size=50, page_pos=0):
        return await self.client.call("branch.list_commits", {"branch": branch, **page(page_size, page_pos)})

    async def graph(self, branch, page_size=50, page_pos=0):
        return await self.client.call("branch.graph", {"branch": branch, **page(page_size, page_pos)})

    async def changed_paths(self, commit, page_size=50, page_pos=0, options=OMIT):
        return await self.client.call("branch.changed_paths", {"commit": commit, **page(page_size, page_pos), **diff_options(options)})

    async def diff_commits(self, from_commit, to_commit, page_size=50, page_pos=0, options=OMIT):
        return await self.client.call("branch.diff_commits", {"from": from_commit, "to": to_commit, **page(page_size, page_pos), **diff_options(options)})

    async def diff_branches(self, from_branch, to_branch, page_size=50, page_pos=0, options=OMIT):
        return await self.client.call("branch.diff_branches", {"from": from_branch, "to": to_branch, **page(page_size, page_pos), **diff_options(options)})

    async def fork(self, name, from_ref="main"):
        return await self.client.call("branch.fork", {"name": name, "from": from_ref})

    async def merge(self, a, b, name, message=OMIT, units=OMIT):
        return await self.client.call("branch.merge", {
            "branch": a, "with": b, "name": name, "message": checked_message(message), "units": units,
        })

    async def publish(self, branch, commit=OMIT):
        return await self.client.call("branch.publish", {"branch": branch, "commit": commit})


class Sandbox:
    def __init__(self, client):
        self.client = client

    async def create(self, options=OMIT):
        if options is OMIT:
            options = {}
        if not isinstance(options, dict) or ("image" in options and not isinstance(options["image"], str)):
            raise LayerFSError("InvalidInput", "options are { image? }, an image digest")
        selected = dict(self.client.options["deploy"])
        if options.get("image"):
            selected["image"] = options["image"]
        value = await self.client._invoke("sandbox.create", {"deploy": deployment(selected)})
        return {"sandbox": value["sandbox"], "image": value["image"]}

    async def list_workspaces(self, sandbox, page_size=50, page_pos=0):
        page(page_size, page_pos)
        value = await self.client._invoke("sandbox.list_workspaces", {"sandbox": sandbox_arg(sandbox)})
        activity = {"idle": "Idle", "committing": "Busy", "attaching": "Busy", "closing": "Closing", "uncertain": "Uncertain", "failed": "Failed"}
        items = []
        for item in value.get("workspaces", []):
            if not item.get("mounted") or not item.get("directory"):
                continue
            dirty = item.get("dirty")
            items.append({
                "sandbox": sandbox,
                "path": item["directory"],
                "branch": item.get("branch"),
                "base": item.get("base"),
                "activity": activity.get(item.get("activity"), item.get("activity")),
                "changed": dirty["inodes"] + dirty["entries"] > 0 if dirty else None,
                "workspace": item.get("workspace"),
                "state": item.get("activity"),
                "dirty": dirty,
                "headMovedSinceMount": item.get("headMovedSinceMount"),
                "pendingMerge": bool(item.get("pendingMerge")),
            })
        return {**page_of(items, page_size, page_pos), "version": value.get("version")}

    async def close(self, sandbox):
        return await self.client._invoke("sandbox.close", {"sandbox": sandbox_arg(sandbox)})


class Workspace:
    def __init__(self, client):
        self.client = client

    async def mount(self, sandbox, branch):
        import re

        if not isinstance(sandbox, str) or not re.fullmatch(r"[1-9][0-9]{0,9}", sandbox) or not isinstance(branch, str):
            raise LayerFSError("InvalidInput", 'workspace.mount(sandbox, branch) takes a Sandbox number such as "1" (from sandbox.create() or project.list_sandboxes()) and a Branch name, and no options')
        value = await self.client._invoke("workspace.mount", {"sandbox": sandbox, "branch": branch})
        return {"sandbox": value["sandbox"], "path": value["path"], "image": value["image"]}

    async def exec(self, workspace, command, message=OMIT):
        value = await self.client._invoke("workspace.exec", {
            **workspace_args(workspace), "command": command, "message": checked_message(message),
        })
        if "output" not in value:
            return value
        output = value.pop("output")
        try:
            for name in ("stdout", "stderr"):
                if value.get(name) is not None:
                    continue
                chunks = []
                offset = 0
                while True:
                    answer = await self.client._transport.exec_output({"output": output, "stream": name, "offset": offset})
                    page_value = self.client._answer(answer)
                    chunk = page_value["data"]
                    chunks.append(chunk)
                    offset += len(chunk)
                    if page_value["end"]:
                        break
                    if not chunk:
                        raise LayerFSError("Failed", f"{name} page at {offset} was empty")
                value[f"{name}Bytes"] = offset
                value[name] = b"".join(chunks).decode("utf-8", errors="replace")
        finally:
            await self.client._transport.call("workspace.exec_release", {"output": output})
        return value

    async def commit(self, workspace, message=OMIT, force=False, options=OMIT):
        force = False if force is None or force is OMIT else bool(force)
        return await self.client._invoke("workspace.commit", {
            **workspace_args(workspace), "message": checked_message(message), "force": force,
            **commit_options(options),
        })

    async def unmount(self, workspace, discard=False, force=False):
        return await self.client._invoke("workspace.unmount", {**workspace_args(workspace), "discard": discard, "force": force})


class StoreProject:
    def __init__(self, client):
        self.client = client

    async def get_summary(self):
        return await self.client._invoke_backend("backend.store.project.get_summary", {})

    async def get_history(self, since=OMIT):
        return await self.client._invoke_backend("backend.store.project.get_history", {"since": since})

    async def get_storage(self, options=OMIT):
        if options is OMIT:
            options = {}
        if not isinstance(options, dict):
            raise LayerFSError("InvalidInput", "options must be an object")
        for key in options:
            if key != "commits":
                raise LayerFSError("InvalidInput", f"unknown option {key}")
        commits = options.get("commits", OMIT)
        if commits is not OMIT and not isinstance(commits, bool):
            raise LayerFSError("InvalidInput", "commits must be a boolean")
        value = await self.client._invoke_backend("backend.store.project.get_storage", {"commits": commits})
        for key in ("dedupe", "compression"):
            if value.get(key) is not None:
                value[key] = float(value[key])
        return value

    async def get_comparable(self, ref):
        import re

        if not isinstance(ref, str) or not re.fullmatch(r"[0-9a-f]{64,68}", ref):
            raise LayerFSError("InvalidInput", "ref must be a saved Commit or Layer id")
        known = self.client._comparable
        if ref not in known:
            history = await self.get_history()
            if len(known) > 100000:
                known.clear()
            by_root = {}
            roots = {}
            for commit in history.get("commits", []):
                known[commit["id"]] = {"commit": commit["id"], "root": commit["root"], "exact": True}
                by_root[commit["root"]] = commit["id"]
                roots[commit["id"]] = commit["root"]
            for layer in history.get("layers", []):
                same = by_root.get(layer["root"])
                if same:
                    known[layer["id"]] = {"commit": same, "root": layer["root"], "exact": True}
                elif layer.get("sourceCommit") and layer["sourceCommit"] not in roots:
                    known[layer["id"]] = {"commit": layer["sourceCommit"], "root": layer["root"], "exact": False}
                else:
                    known[layer["id"]] = None
        found = known.get(ref)
        if not found:
            raise LayerFSError("NotComparable", "LayerFS compares Commits, and this Layer has no Commit with the same files (Init's Layer, or a publish that merged)", {"ref": ref})
        return dict(found)

    async def list_overwritten(self, layer, page_size=50, page_pos=0):
        return await self.client._invoke_backend("backend.store.project.list_overwritten", {"layer": layer, **page(page_size, page_pos)})


class StoreBranch:
    def __init__(self, client):
        self.client = client

    async def list_commit_storage(self, branch, page_size=50, page_pos=0):
        return await self.client._invoke_backend("backend.store.branch.list_commit_storage", {"branch": branch, **page(page_size, page_pos)})


class StoreFS:
    def __init__(self, client):
        self.client = client

    async def get_storage(self, ref, path):
        return await self.client._invoke_backend("backend.store.fs.get_storage", {"branch": ref, "path": path})

    async def list_revisions(self, ref, path, page_size=50, page_pos=0):
        return await self.client._invoke_backend("backend.store.fs.list_revisions", {"branch": ref, "path": path, **page(page_size, page_pos)})


class DaemonWorkspace:
    def __init__(self, client):
        self.client = client

    async def status(self, workspace):
        return await self.client._invoke_backend("backend.daemon.workspace.status", workspace_args(workspace))

    async def list_changes(self, workspace, page_size=50, page_pos=0):
        return await self.client._invoke_backend("backend.daemon.workspace.list_changes", {**workspace_args(workspace), **page(page_size, page_pos)})

    async def get_storage(self, workspace):
        return await self.client._invoke_backend("backend.daemon.workspace.get_storage", workspace_args(workspace))


class DaemonProcess:
    def __init__(self, client):
        self.client = client

    async def start(self, workspace, command):
        return await self.client._invoke_backend("backend.daemon.process.start", {**workspace_args(workspace), "command": command, "background": True})

    async def list(self, page_size=50, page_pos=0):
        page(page_size, page_pos)
        value = await self.client._invoke_backend("backend.daemon.process.list", {})
        return page_of(value.get("items", []), page_size, page_pos)

    async def cancel(self, process):
        if not isinstance(process, str) or not process:
            raise LayerFSError("InvalidInput", "a process is its id from process.start or process.list")
        return await self.client._invoke_backend("backend.daemon.process.cancel", {"process": process})


class DaemonSandbox:
    def __init__(self, client):
        self.client = client

    async def get_log(self, sandbox, options=OMIT):
        if options is OMIT:
            options = {}
        lines = options.get("lines", 200) if isinstance(options, dict) else None
        from ._util import safe_integer

        if not safe_integer(lines) or not 1 <= lines <= 2000 or not isinstance(options, dict) or "since" in options:
            raise LayerFSError("InvalidInput", "options are { lines }: 1..2000 (a `since` time is not supported)")
        return await self.client._invoke_backend("backend.daemon.sandbox.get_log", {"sandbox": sandbox_arg(sandbox), "lines": lines})


class _Namespace:
    def __init__(self, **groups):
        self.__dict__.update(groups)


class Backend:
    def __init__(self, client):
        self.store = _Namespace(project=StoreProject(client), branch=StoreBranch(client), fs=StoreFS(client))
        self.daemon = _Namespace(workspace=DaemonWorkspace(client), process=DaemonProcess(client), sandbox=DaemonSandbox(client))
