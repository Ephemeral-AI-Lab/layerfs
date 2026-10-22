"""#420 payload study: shared helpers (raw payload access, content classes, zstd proxies)."""
import mmap
import os
import pickle
import re

from compression import zstd
from compression.zstd import CompressionParameter as CP, ZstdCompressor, ZstdDict

CHUNK_WLOG = 20
WF_WLOG = 18  # small_file_threshold 128 KiB -> WHOLE_FILE_WINDOW_LOG_SMALL
LEVEL = 3


class Raw:
    """Random access to every record's raw payload through the dump files."""

    def __init__(self, d):
        self.d = d
        self.rows = pickle.load(open(os.path.join(d, "index.pkl"), "rb"))
        self.maps = {}

    def get(self, r):
        m = self.maps.get(r["src"])
        if m is None:
            fh = open(os.path.join(self.d, r["src"]), "rb")
            m = self.maps[r["src"]] = mmap.mmap(fh.fileno(), 0, access=mmap.ACCESS_READ)
        return m[r["off"]: r["off"] + r["raw"]]


def opts(wlog):
    return {CP.compression_level: LEVEL, CP.window_log: wlog, CP.checksum_flag: 1,
            CP.content_size_flag: 1, CP.dict_id_flag: 0}


def full_frame(raw, wlog):
    return ZstdCompressor(options=opts(wlog)).compress(raw, ZstdCompressor.FLUSH_FRAME)


def prefix_frame(raw, base, wlog):
    """Proxy of CompressionWorkspace::compress_prefix: level 3, raw prefix, same frame flags."""
    if len(base) < 8:
        # python's ZstdDict refuses < 8 bytes; such a prefix is worthless, so take the frame without it
        return full_frame(raw, wlog)
    return ZstdCompressor(options=opts(wlog), zstd_dict=ZstdDict(base, is_raw=True).as_prefix).compress(
        raw, ZstdCompressor.FLUSH_FRAME)


def record_width(role, frame_len, prefix):
    # compact whole-file lane: tag [+ base] + frame; chunk lane: tag + u32 raw length [+ base] + frame
    return (1 if role == 1 else 5) + (32 if prefix else 0) + frame_len


ZLIB_FLG = {0x01, 0x5e, 0x9c, 0xda}


def classify(h):
    """Content class from the leading bytes of a file (h: up to 600 bytes)."""
    if len(h) >= 2 and h[0] == 0x78 and h[1] in ZLIB_FLG and ((h[0] << 8) | h[1]) % 31 == 0:
        return "zlib"
    if h[:4] == b"PACK":
        return "git-pack"
    if h[:4] == b"\xfftOc":
        return "git-idx"
    if h[:4] == b"DIRC":
        return "git-index"
    if h[:2] == b"\x1f\x8b":
        return "gzip"
    if h[:4] in (b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08"):
        return "zip"
    if h[:8] == b"\x89PNG\r\n\x1a\n" or h[:3] == b"\xff\xd8\xff" or h[:4] == b"GIF8" or \
            (h[:4] == b"RIFF" and h[8:12] == b"WEBP") or h[:4] in (b"\x00\x00\x01\x00",):
        return "image"
    if h[:6] == b"\xfd7zXZ\x00" or h[:4] == b"\x28\xb5\x2f\xfd" or h[:3] == b"BZh" or h[:4] == b"\x04\x22\x4d\x18":
        return "xz/zstd/bz2/lz4"
    if h[:4] == b"\x7fELF" or h[:4] in (b"\xfe\xed\xfa\xce", b"\xfe\xed\xfa\xcf", b"\xce\xfa\xed\xfe",
                                        b"\xcf\xfa\xed\xfe") or h[:4] == b"\x00asm":
        return "ELF/Mach-O/wasm"
    if h[:4] == b"\xca\xfe\xba\xbe":
        return "java-class/fat-macho"
    if h[:4] in (b"wOF2", b"wOFF"):
        return "font-woff"
    if h[:16] == b"SQLite format 3\x00":
        return "sqlite"
    if len(h) > 262 and h[257:262] == b"ustar":
        return "tar"
    if h[:4] == b"%PDF":
        return "pdf"
    return "other"


GIT_HDR = re.compile(rb"(blob|tree|commit|tag) (\d+)\x00")
