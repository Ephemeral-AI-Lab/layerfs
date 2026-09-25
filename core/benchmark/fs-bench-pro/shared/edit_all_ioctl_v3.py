"""Immutable 56-case #232 mounted all-ioctl selection."""

import hashlib
import json
from pathlib import Path

BENCH = Path(__file__).resolve().parent.parent
REGISTRY_PATH = "registry/workspace-exec-edit-v3.json"
REGISTRY_SHA256 = "469bf23363083980cf2e424394eba994ed7a6009116d215b3d96c379e66b975c"
SCENARIO_VERSION = 3


def registry():
    raw = (BENCH / REGISTRY_PATH).read_bytes()
    if hashlib.sha256(raw).hexdigest() != REGISTRY_SHA256:
        raise ValueError("all-ioctl registry digest changed")
    document = json.loads(raw)
    rows = document["cases"]
    if len(rows) != 56 or len({row["scenario_id"] for row in rows}) != 56:
        raise ValueError("all-ioctl registry is incomplete")
    if document["operation_contract_id"] != "workspace-exec-mounted-range-replace-v3":
        raise ValueError("all-ioctl contract changed")
    return document
