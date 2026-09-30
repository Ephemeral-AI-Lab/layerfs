#!/usr/bin/env python3
"""One bounded native cause probe; no speed/shape/pressure admission."""
import ctypes
import json
import platform

library = "/usr/lib/libsqlite3.dylib"
db = ctypes.CDLL(library)
db.sqlite3_config.argtypes = [ctypes.c_int]
db.sqlite3_config.restype = ctypes.c_int
db.sqlite3_initialize.argtypes = []
db.sqlite3_initialize.restype = ctypes.c_int
db.sqlite3_hard_heap_limit64.argtypes = [ctypes.c_int64]
db.sqlite3_hard_heap_limit64.restype = ctypes.c_int64
db.sqlite3_status64.argtypes = [ctypes.c_int, ctypes.POINTER(ctypes.c_int64),
                              ctypes.POINTER(ctypes.c_int64), ctypes.c_int]
db.sqlite3_status64.restype = ctypes.c_int
db.sqlite3_malloc64.argtypes = [ctypes.c_uint64]
db.sqlite3_malloc64.restype = ctypes.c_void_p
db.sqlite3_msize.argtypes = [ctypes.c_void_p]
db.sqlite3_msize.restype = ctypes.c_uint64
db.sqlite3_free.argtypes = [ctypes.c_void_p]
db.sqlite3_free.restype = None
for name in ("sqlite3_libversion", "sqlite3_sourceid"):
    function = getattr(db, name)
    function.argtypes = []
    function.restype = ctypes.c_char_p


def status(selector):
    current, highwater = ctypes.c_int64(), ctypes.c_int64()
    result = db.sqlite3_status64(selector, ctypes.byref(current), ctypes.byref(highwater), 0)
    return {"return": result, "current": current.value,
            "lifetime_highwater": highwater.value}


def counters():
    return {"bytes": status(0), "allocations": status(9)}


result = {"purpose": "labelled native cause diagnostic, not an eligible-arm rerun",
          "library": library, "machine": platform.machine(), "requested_limit": 33554432,
          "probe_request": 8192, "free_attempts": 0, "reset": False, "retry": False}
result["config_memstatus"] = db.sqlite3_config(9, ctypes.c_int(1))
result["initialize"] = db.sqlite3_initialize()
assert result["config_memstatus"] == result["initialize"] == 0
result["previous_limit"] = db.sqlite3_hard_heap_limit64(-1)
result["setter_previous_limit"] = db.sqlite3_hard_heap_limit64(33554432)
result["readback_limit"] = db.sqlite3_hard_heap_limit64(-1)
result["provider_version"] = db.sqlite3_libversion().decode("utf-8")
result["provider_source_id"] = db.sqlite3_sourceid().decode("utf-8")
address = ctypes.cast(db.sqlite3_hard_heap_limit64, ctypes.c_void_p).value
result["hard_limit_code_first64_hex"] = ctypes.string_at(address, 64).hex()
result["before"] = counters()
pointer = db.sqlite3_malloc64(8192)
result["issued"] = pointer is not None
if pointer:
    try:
        result["actual_msize"] = db.sqlite3_msize(pointer)
        ctypes.memset(pointer, 0, 8192)
        result["during"] = counters()
    finally:
        result["free_attempts"] += 1
        db.sqlite3_free(pointer)
result["after"] = counters()
print(json.dumps(result, indent=2))
