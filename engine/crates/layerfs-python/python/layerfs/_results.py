from collections import deque
from datetime import datetime, timezone
import threading
from ._errors import LayerFSError
from ._util import MAX_SAFE


PUBLISHING = {
    "fs.write", "fs.edit", "fs.apply", "fs.mkdir", "fs.rm", "fs.cp", "fs.mv",
    "workspace.commit", "branch.merge", "branch.publish",
}


def started():
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def outcome_fields(method, value):
    if not isinstance(value, dict):
        return {}
    result = {}
    if "kind" in value:
        result["kind"] = value["kind"]
    record = value.get("commit") if isinstance(value.get("commit"), dict) else None
    commit = None
    if record and "id" in record:
        commit = record["id"]
    elif isinstance(value.get("commit"), str):
        commit = value["commit"]
    elif method in PUBLISHING:
        source = value.get("source")
        if isinstance(source, dict):
            commit = source.get("commit")
        if commit is None:
            commit = value.get("head")
    if commit is not None:
        result["commit"] = commit
    if method in PUBLISHING and isinstance(value.get("publishedLayer"), dict) and value["publishedLayer"].get("id"):
        result["layer"] = value["publishedLayer"]["id"]
    if record and "logical" in record:
        result["sizes"] = {
            key: record[key]
            for key in ("logical", "newCanonical", "newStored", "newMetadata")
            if key in record
        }
    return result


class Results:
    def __init__(self):
        self._records = deque(maxlen=1024)
        self._next = 1
        self._evicted = 0
        self._lock = threading.Lock()

    def append(self, record):
        with self._lock:
            record["seq"] = self._next
            self._next += 1
            if len(self._records) == 1024:
                self._evicted += 1
            self._records.append(record)

    def read(self, after=0):
        if after is None:
            after = 0
        if isinstance(after, bool) or not isinstance(after, int) or after < 0 or after > MAX_SAFE:
            raise LayerFSError("InvalidInput", "after must be a nonnegative safe integer")
        with self._lock:
            items = [dict(record) for record in self._records if record["seq"] > after]
            oldest = self._records[0]["seq"] if self._records else self._next
            result = {
                "items": items,
                "next": items[-1]["seq"] if items else after,
                "evicted": self._evicted,
            }
            if after + 1 < oldest:
                result["stale"] = True
            return result
