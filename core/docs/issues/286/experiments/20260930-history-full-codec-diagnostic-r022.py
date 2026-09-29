import ctypes as C
import json, os, sqlite3, struct, sys, time
from collections import Counter
from pathlib import Path

source = Path(sys.argv[1])
lib = C.CDLL('/opt/homebrew/lib/libzstd.dylib')
size = C.c_size_t
lib.ZSTD_getFrameContentSize.argtypes = [C.c_void_p, size]
lib.ZSTD_getFrameContentSize.restype = C.c_ulonglong
lib.ZSTD_decompress.argtypes = [C.c_void_p, size, C.c_void_p, size]
lib.ZSTD_decompress.restype = size
lib.ZSTD_createCCtx.restype = C.c_void_p
lib.ZSTD_freeCCtx.argtypes = [C.c_void_p]
lib.ZSTD_CCtx_reset.argtypes = [C.c_void_p, C.c_int]
lib.ZSTD_CCtx_reset.restype = size
lib.ZSTD_CCtx_setParameter.argtypes = [C.c_void_p, C.c_int, C.c_int]
lib.ZSTD_CCtx_setParameter.restype = size
lib.ZSTD_compressBound.argtypes = [size]
lib.ZSTD_compressBound.restype = size
lib.ZSTD_compress2.argtypes = [C.c_void_p, C.c_void_p, size, C.c_void_p, size]
lib.ZSTD_compress2.restype = size
lib.ZSTD_isError.argtypes = [size]
lib.ZSTD_isError.restype = C.c_uint

def checked(value):
    if lib.ZSTD_isError(value):
        raise ValueError('zstd error')
    return value

ctx = lib.ZSTD_createCCtx()
def compress(raw, level):
    checked(lib.ZSTD_CCtx_reset(ctx, 3))
    for parameter, value in ((100, level), (101, 18), (200, 1), (201, 1), (202, 0), (400, 0)):
        checked(lib.ZSTD_CCtx_setParameter(ctx, parameter, value))
    src = C.create_string_buffer(raw)
    dst = C.create_string_buffer(lib.ZSTD_compressBound(len(raw)))
    started = time.process_time_ns()
    length = checked(lib.ZSTD_compress2(ctx, dst, len(dst), src, len(raw)))
    cpu = time.process_time_ns() - started
    return dst.raw[:length], cpu

counts=Counter(); widths=Counter(); cpu=Counter(); calibration=Counter()
connection=sqlite3.connect(f'file:{source.resolve()}?mode=ro',uri=True)
for (pack,) in connection.execute('SELECT data FROM object_packs'):
    if struct.unpack_from('<I',pack,8)[0] != 18:
        continue
    group_count=struct.unpack_from('<I',pack,12)[0]
    starts=[struct.unpack_from('<I',pack,24+4*i)[0] for i in range(group_count)]
    for i,start in enumerate(starts):
        end=starts[i+1] if i+1<group_count else len(pack)
        body=memoryview(pack)[start:end]
        count=struct.unpack_from('<I',body,0)[0]
        ends=struct.unpack_from('<'+str(count)+'I',body,4)
        area=4+4*count
        before=0
        for finish in ends:
            record=bytes(body[area+before:area+finish]); before=finish
            if record[0] != 0:
                continue
            frame=record[1:]
            frame_buf=C.create_string_buffer(frame)
            raw_size=lib.ZSTD_getFrameContentSize(frame_buf,len(frame))
            if raw_size > 1_048_576: raise ValueError('bad frame size')
            raw_buf=C.create_string_buffer(raw_size)
            written=checked(lib.ZSTD_decompress(raw_buf,raw_size,frame_buf,len(frame)))
            if written != raw_size: raise ValueError('decompression width')
            raw=raw_buf.raw[:written]
            old,cpu9=compress(raw,9)
            new,cpu12=compress(raw,12)
            counts['full_frames']+=1
            calibration['level9_exact']+=old==frame
            widths['raw']+=len(raw)
            widths['level9']+=len(frame)
            widths['level12']+=min(len(new),len(raw))
            cpu['level9_ns']+=cpu9
            cpu['level12_ns']+=cpu12
connection.close();lib.ZSTD_freeCCtx(ctx)
result={'schema':'issue286-r021-full-only-codec-diagnostic-v1','kind':'offline closed-Store count-driven diagnostic; not candidate sample or speed arm','source':str(source),'counts':dict(counts),'calibration':dict(calibration),'widths':dict(widths),'cpu_process_ns_cache_uncontrolled':dict(cpu),'projected_full_only_saving_bytes':widths['level9']-widths['level12'],'projected_added_codec_cpu_ns':cpu['level12_ns']-cpu['level9_ns']}
print(json.dumps(result,indent=2))
