"""Read no-follow extended attributes on Linux and macOS using the stdlib."""

import base64
import ctypes
import errno
import os
import sys
from functools import lru_cache

from cleanup_state import Refused


@lru_cache(maxsize=1)
def darwin_library():
    library = ctypes.CDLL(None, use_errno=True)
    library.listxattr.argtypes = [ctypes.c_char_p, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int]
    library.listxattr.restype = ctypes.c_ssize_t
    library.getxattr.argtypes = [
        ctypes.c_char_p,
        ctypes.c_char_p,
        ctypes.c_void_p,
        ctypes.c_size_t,
        ctypes.c_uint32,
        ctypes.c_int,
    ]
    library.getxattr.restype = ctypes.c_ssize_t
    return library


def attributes(path):
    if hasattr(os, "listxattr"):
        return {
            key: base64.b64encode(os.getxattr(path, key, follow_symlinks=False)).decode()
            for key in os.listxattr(path, follow_symlinks=False)
        }
    if sys.platform != "darwin":
        raise Refused("Extended attribute inspection is unsupported on this platform")
    library = darwin_library()
    encoded = os.fsencode(path)

    def checked(value):
        if value < 0:
            code = ctypes.get_errno()
            raise OSError(code, os.strerror(code), str(path))
        return value

    try:
        count = checked(library.listxattr(encoded, None, 0, 1))
    except OSError as error:
        if error.errno in {errno.ENOTSUP, errno.EOPNOTSUPP}:
            return {}
        raise
    names = ctypes.create_string_buffer(count)
    count = checked(library.listxattr(encoded, names, count, 1))
    result = {}
    for name in names.raw[:count].split(b"\0"):
        if not name:
            continue
        size = checked(library.getxattr(encoded, name, None, 0, 0, 1))
        data = ctypes.create_string_buffer(size)
        actual = checked(library.getxattr(encoded, name, data, size, 0, 1))
        result[os.fsdecode(name)] = base64.b64encode(data.raw[:actual]).decode()
    return result
