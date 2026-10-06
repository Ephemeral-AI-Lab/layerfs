"""Independent fixture inputs for E1; never a measured product implementation."""
import hashlib

DOMAIN = b"LayerFS E1 fixture v1\x00"


def payload_chunks(seed, identity, length):
    """Deterministic distinct bytes, streamed in fixed 32-byte generator units.

Fixture preparation pays generator/hashing work outside the operation. A real
dense source must then be written/closed and the selected cache reset attested;
this generator cannot substitute logical size for physical acquisition.
"""
    if any(type(value) is not int or value < 0 or value >= 1 << 64 for value in (seed, identity, length)):
        raise ValueError("fixture seed/identity/length outside unsigned-64 scope")
    prefix = DOMAIN + seed.to_bytes(8, "little") + identity.to_bytes(8, "little")
    offset = 0
    while offset < length:
        unit = hashlib.sha256(prefix + (offset // 32).to_bytes(8, "little")).digest()
        take = min(len(unit), length - offset)
        yield unit[:take]
        offset += take


def write_trace(base_bytes):
    """Freeze every E04/E05 public write's offset, length and input-byte identity."""
    if base_bytes not in (16_777_216, 1_073_741_824):
        raise ValueError("unregistered dense write cohort")
    rows = []
    for index in range(1000):
        digest = hashlib.sha256()
        for chunk in payload_chunks(1, index + 1, 4096):
            digest.update(chunk)
        rows.append({"index": index, "offset": 4096 * ((104729 * index) % (base_bytes // 4096)),
                     "length": 4096, "replacement_sha256": digest.hexdigest()})
    return rows
