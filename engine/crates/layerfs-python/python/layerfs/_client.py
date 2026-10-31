import json
import os
import re
import secrets
import time

from ._errors import LayerFSError
from ._methods import Backend, Branch, FS, Project, Sandbox, Workspace
from ._results import Results, outcome_fields, started
from ._scope import check_scope, scope_args, scope_branches
from ._util import OMIT, clean, page, page_of, safe_integer


INIT_TIMEOUT_MS = 300_000


def make_transport():
    endpoint = os.environ.get("LAYERFS_WORKER_ENDPOINT")
    if endpoint and os.name != "posix":
        from ._worker import WorkerTransport

        return WorkerTransport(endpoint)
    if endpoint and not __import__("sys").platform.startswith("linux"):
        from ._worker import WorkerTransport

        return WorkerTransport(endpoint)
    from ._native_transport import NativeTransport

    return NativeTransport()


def _plain_options(value):
    return isinstance(value, dict)


def resolve_options(given):
    if given is OMIT:
        given = {}
    if not _plain_options(given):
        raise LayerFSError("InvalidInput", "LayerFS options must be a plain object")
    for key in given:
        if key not in ("home", "trace", "deploy", "storePolicy", "storePath", "scope"):
            raise LayerFSError("InvalidInput", f'unknown LayerFS option "{key}"')

    def env(name):
        return os.environ.get(f"LAYERFS_{name}") or None

    def field(name, env_name):
        value = given.get(name, OMIT)
        if value is OMIT:
            return env(env_name)
        if not isinstance(value, str) or not value:
            raise LayerFSError("InvalidInput", f'LayerFS option "{name}" must be a non-empty string')
        return value

    deploy_given = given.get("deploy", {})
    if not _plain_options(deploy_given):
        raise LayerFSError("InvalidInput", 'LayerFS option "deploy" must be a plain object')
    for key in deploy_given:
        if key not in ("docker", "image", "volume", "daemon", "containerPrefix", "daemonHost"):
            raise LayerFSError("InvalidInput", f'unknown LayerFS deploy option "{key}"')

    def deploy_field(name, env_name):
        value = deploy_given.get(name, OMIT)
        if value is OMIT:
            return env(env_name)
        if not isinstance(value, str) or not value:
            raise LayerFSError("InvalidInput", f'LayerFS option "deploy.{name}" must be a non-empty string')
        return value

    policy = given.get("storePolicy", OMIT)
    if policy is OMIT:
        policy = env("STORE_POLICY")
    elif isinstance(policy, str):
        if not policy:
            raise LayerFSError("InvalidInput", 'LayerFS option "storePolicy" must be JSON text or a plain object')
    elif isinstance(policy, dict):
        try:
            policy = json.dumps(policy, separators=(",", ":"), ensure_ascii=False)
        except (TypeError, ValueError) as error:
            raise LayerFSError("InvalidInput", 'LayerFS option "storePolicy" must be JSON text or a plain object') from error
    else:
        raise LayerFSError("InvalidInput", 'LayerFS option "storePolicy" must be JSON text or a plain object')
    deploy = {
        "docker": deploy_field("docker", "DOCKER") or "/var/run/docker.sock",
        "image": deploy_field("image", "IMAGE"),
        "volume": deploy_field("volume", "VOLUME"),
        "daemon": deploy_field("daemon", "DAEMON"),
    }
    for name, env_name in (("containerPrefix", "CONTAINER_PREFIX"), ("daemonHost", "DAEMON_HOST")):
        value = deploy_field(name, env_name)
        if value:
            deploy[name] = value
    scope = check_scope(given.get("scope", OMIT))
    res = {
        "home": field("home", "HOME"),
        "trace": field("trace", "TRACE"),
        "deploy": deploy,
        "storePolicy": policy,
        "storePath": field("storePath", "STORE_PATH"),
    }
    if scope:
        res["scope"] = scope
    return res


def _summary(args):
    content = {"content", "bytes", "oldText", "newText"}
    settings = {"deploy", "home", "project", "storePath"}
    result = {}
    for name, value in args.items():
        if name in settings:
            continue
        if name in content:
            result[f"{name}Bytes"] = len(value.encode("utf-8")) if isinstance(value, str) else len(value or b"")
        elif name == "changes" and isinstance(value, list):
            result["changes"] = len(value)
            result["changeBytes"] = sum(
                len(change[field].encode("utf-8")) if isinstance(change.get(field), str) else len(change.get(field) or b"")
                for change in value if isinstance(change, dict) for field in content if field in change
            )
        elif isinstance(value, str):
            result[name] = value[:200] + "…" if len(value) > 200 else value
        elif isinstance(value, (int, float, bool)) or value is None:
            result[name] = value
        elif isinstance(value, list):
            result[name] = len(value)
    return result


def _write_trace(file, line):
    try:
        with open(file, "a", encoding="utf-8") as output:
            output.write(json.dumps(line, separators=(",", ":"), ensure_ascii=False) + "\n")
    except Exception:
        pass


class LayerFS:
    def __init__(self, project_name, token=None, options=OMIT):
        if token is not self._FACTORY:
            raise LayerFSError("InvalidInput", "use `await LayerFS.open(name)` or `await LayerFS.init(name, folder)`; there is no public constructor")
        if not isinstance(project_name, str) or not re.fullmatch(r"[a-z0-9](?:[a-z0-9_.-]{0,61}[a-z0-9])?", project_name):
            raise LayerFSError("InvalidInput", "a project name is required")
        self.project_name = project_name
        self.options = resolve_options(options)
        self._closed = None
        self._transport = make_transport()
        self._results = Results()
        self._comparable = {}
        self.fs = FS(self)
        self.project = Project(self)
        self.branch = Branch(self)
        self.sandbox = Sandbox(self)
        self.workspace = Workspace(self)
        self.backend = None if self.options.get("scope") else Backend(self)

    _FACTORY = object()

    @classmethod
    async def open(cls, project_name, options=OMIT):
        client = cls(project_name, cls._FACTORY, options)
        await client._opened()
        return client

    @classmethod
    async def init(cls, project_name, folder, timeout=OMIT, options=OMIT):
        if timeout is OMIT:
            timeout = INIT_TIMEOUT_MS
        client = cls(project_name, cls._FACTORY, options)
        if client.options.get("scope"):
            raise LayerFSError("InvalidInput", "LayerFS.init does not accept scope; open the existing project with a scope instead")
        client.created = await client._init(folder, timeout)
        await client._opened()
        return client

    @property
    def projectName(self):
        return self.project_name

    def _project_args(self):
        return {"project": self.project_name, "home": self.options["home"], "deploy": self.options["deploy"]}

    def _store_args(self):
        return {"storePath": self.options["storePath"]} if self.options["storePath"] else {}

    async def _opened(self):
        if not self.options["storePath"]:
            raise LayerFSError("InvalidInput", "a LayerFS client needs storePath (LAYERFS_STORE_PATH), the project's installed Store")
        self._answer(await self._transport.store_open({**self._project_args(), **self._store_args()}))
        try:
            await self._record("project.list_branches", {
                **self._project_args(), **self._store_args(), "pageSize": 1, "pagePos": 0,
            }, self._transport.call)
        except Exception:
            try:
                await self._transport.store_close({**self._project_args(), **self._store_args()})
            except Exception:
                pass
            raise
        self._closed = False

    async def _init(self, folder, timeout):
        if not safe_integer(timeout) or timeout <= 0:
            raise LayerFSError("InvalidInput", "timeout must be a positive whole number of milliseconds")
        text = self.options["storePolicy"]
        policy = OMIT
        if text is not None:
            try:
                policy = json.loads(text)
            except (TypeError, ValueError):
                policy = None
            if not isinstance(policy, dict):
                raise LayerFSError("InvalidInput", "LAYERFS_STORE_POLICY must be a JSON object { smallFileThreshold, wholeFileDepth, chunkDepth, metadataDepth }")
        try:
            return await self._record("project.init", {
                **self._project_args(), **self._store_args(), "path": folder, "name": self.project_name,
                "policy": policy, "timeoutMs": timeout,
            }, self._transport.call)
        except LayerFSError as error:
            if policy is not OMIT and error.code == "InvalidInput" and re.match(r"policy[.:]", error.raw_message):
                raise LayerFSError("InvalidInput", f"LAYERFS_STORE_POLICY {error.raw_message}", error.detail) from error
            raise

    def _live(self):
        if self._closed is not False:
            raise LayerFSError("InvalidInput", f"this LayerFS client of {self.project_name} is closed; open another with LayerFS.open")

    async def call(self, method, args):
        if isinstance(method, str) and re.match(r"^(backend|runtime)\.", method):
            raise LayerFSError("UnknownMethod", "backend and runtime methods are @layerfs/backend's and @layerfs/runtime's")
        scope = self.options.get("scope")
        scoped_args = scope_args(scope, method, args)
        if scope and method == "project.list_branches":
            return await self._scoped_branches(scope, scoped_args)
        return await self._invoke(method, scoped_args)

    async def _scoped_branches(self, scope, args):
        """Lists the complete Branch namespace, then applies the caller's page to it."""
        args = args or {}
        requested = page(args.get("pageSize", OMIT), args.get("pagePos", OMIT))
        items = []
        page_pos = 0
        while True:
            answer = await self._invoke("project.list_branches", {**args, "pageSize": 1000, "pagePos": page_pos})
            items.extend(scope_branches(scope, answer.get("items", [])))
            if not answer.get("hasMore"):
                break
            page_pos = answer["nextPagePos"]
        return page_of(items, requested["pageSize"], requested["pagePos"])

    async def _invoke(self, method, args):
        self._live()
        return await self._record(method, {**self._project_args(), **self._store_args(), **args}, self._transport.call)

    async def _invoke_backend(self, method, args):
        self._live()
        return await self._record(method, {**self._project_args(), **self._store_args(), **args}, self._transport.backend_call)

    async def _record(self, method, args, send):
        args = clean(args)
        trace = self.options["trace"]
        scope = secrets.token_hex(32) if trace else None
        start = started()
        began = time.perf_counter()
        line = None
        if trace:
            line = {"layerfs_trace": 1, "id": f"p4-{secrets.token_hex(4)}", "method": method,
                    **({"project": args["project"]} if isinstance(args.get("project"), str) else {}),
                    "args": _summary(args), "start": start, "scope": scope, "phase": "end"}
            if os.environ.get("LAYERFS_TRACE_STEP"):
                line["step"] = os.environ["LAYERFS_TRACE_STEP"][:200]
            if os.environ.get("LAYERFS_TRACE_START") == "1":
                _write_trace(trace, {**line, "phase": "start"})
        sent = {**args, "observe": scope} if scope else args
        try:
            answer = await send(method, sent)
        except Exception as error:
            ms = round((time.perf_counter() - began) * 1000, 3)
            code = error.code if isinstance(error, LayerFSError) else None
            message = error.message if isinstance(error, LayerFSError) else str(error)
            self._results.append({"method": method, "start": start, "ms": ms, "ok": False, "code": code, "message": message})
            if line:
                _write_trace(trace, {**line, "ms": ms, "ok": False, "code": code, "message": message})
            raise
        ms = round((time.perf_counter() - began) * 1000, 3)
        plain = {key: value for key, value in answer.items() if key not in ("telemetry", "client")}
        refusal = plain.get("refused")
        if refusal:
            record = {"method": method, "start": start, "ms": ms, "ok": False,
                      "code": refusal["code"], "message": refusal["message"]}
        else:
            record = {"method": method, "start": start, "ms": ms, "ok": True,
                      **outcome_fields(method, plain.get("value"))}
        self._results.append(record)
        if line:
            _write_trace(trace, {**line, **{key: value for key, value in record.items() if key != "method"}})
        return self._answer(plain)

    @staticmethod
    def _answer(answer):
        if answer.get("refused"):
            refusal = answer["refused"]
            raise LayerFSError(refusal["code"], refusal["message"], refusal.get("detail"))
        return answer.get("value")

    def results(self, after=0):
        return self._results.read(after)

    async def close(self):
        if self._closed is not False:
            return
        self._closed = True
        self._answer(await self._transport.store_close({**self._project_args(), **self._store_args()}))

    async def __aenter__(self):
        return self

    async def __aexit__(self, exc_type, exc, tb):
        await self.close()
