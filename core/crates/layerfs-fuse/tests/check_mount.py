#!/usr/bin/env python3
"""Independent syscall oracle; launched inside the mounted proof container."""
import ctypes, errno, fcntl, hashlib, json, os, platform, stat, subprocess
from pathlib import Path
p = Path('/layerfs/workspace/read')
expected = bytes(range(251)) * 2301
assert stat.S_ISDIR(p.stat().st_mode)
assert not Path('/layerfs/private-backing').exists()
# Confirmed negative lookup must not poison later independent operations.
try: (p / 'absent').stat()
except FileNotFoundError: pass
else: raise AssertionError('missing path visible')
s = (p / 'data.bin').stat()
# FUSE receives kernel flags, including the forced large-file bit which glibc
# defines as zero for ordinary 64-bit userspace callers.
kernel_largefile = {'aarch64': 0x20000, 'x86_64': 0x8000}[platform.machine()]
largefile_fd = os.open(p / 'data.bin', os.O_RDONLY | kernel_largefile)
try:
    assert fcntl.fcntl(largefile_fd, fcntl.F_GETFL) & kernel_largefile
    assert os.pread(largefile_fd, 19, 0) == expected[:19]
finally: os.close(largefile_fd)
assert s.st_ino == (p/'alias').stat().st_ino and s.st_nlink == 2
assert (p/'alias').read_bytes() == expected
assert (p/'root-readable').read_bytes() == expected
assert s.st_size == len(expected) and stat.S_IMODE(s.st_mode) == 0o644
assert s.st_mtime_ns == -876543211, s.st_mtime_ns
assert s.st_atime_ns == s.st_ctime_ns == s.st_mtime_ns
fd = os.open(p / 'data.bin', os.O_RDONLY)
assert os.pread(fd, 19, 0) == expected[:19]
assert os.pread(fd, 100, len(expected) - 7) == expected[-7:]
assert os.pread(fd, 100, len(expected)) == b''
assert os.lseek(fd, 0, os.SEEK_END) == len(expected)
other = os.dup(fd)
independent = os.open(p / 'data.bin', os.O_RDONLY)
os.close(fd)
assert os.pread(other, 73, 271) == expected[271:344]
os.close(other)
assert os.pread(independent, 17, 19) == expected[19:36]
os.close(independent)
assert (p / 'data.bin').read_bytes() == expected
# The namespace is imported from a real host directory, and the importer refuses
# symlinks, so this fixture declares none. Symlink creation on a live Workspace
# is the execution-side route's separate proof.
assert sorted(os.listdir(p / 'empty')) == []
names = {f'entry-{i:03d}-' + 'x' * 180 for i in range(100)}
assert set(os.listdir(p)) == names | {'data.bin','unread.bin','tool','empty','alias','root-readable'}
for entry in os.scandir(p):
    assert entry.inode() > 0
    if entry.name in names: assert entry.is_file()
# Kernel cookies must support seeking to a previously delivered position.
libc = ctypes.CDLL(None, use_errno=True)
libc.opendir.argtypes = [ctypes.c_char_p]; libc.opendir.restype = ctypes.c_void_p
libc.readdir.argtypes = [ctypes.c_void_p]; libc.readdir.restype = ctypes.c_void_p
libc.telldir.argtypes = [ctypes.c_void_p]; libc.telldir.restype = ctypes.c_long
libc.seekdir.argtypes = [ctypes.c_void_p, ctypes.c_long]
libc.closedir.argtypes = [ctypes.c_void_p]
d = libc.opendir(os.fsencode(p)); assert d
for _ in range(7): assert libc.readdir(d)
cookie = libc.telldir(d); assert cookie > 0
next_entry = libc.readdir(d); assert next_entry
# Linux dirent64: ino64, off64, reclen16, type8, name[].
next_name = ctypes.string_at(next_entry + 19)
for _ in range(5): assert libc.readdir(d)
libc.seekdir(d, cookie)
assert ctypes.string_at(libc.readdir(d) + 19) == next_name
assert libc.closedir(d) == 0
assert subprocess.run([str(p / 'tool')], timeout=10).returncode == 0
# A root caller still needs an executable bit for the kernel exec-open route.
try: subprocess.run([str(p / 'data.bin')], timeout=10, check=True)
except OSError as error: assert error.errno == errno.EACCES, error
else: raise AssertionError('non-executable inode executed')
for flags in (os.O_WRONLY, os.O_RDWR, os.O_RDONLY | os.O_TRUNC):
    try: os.close(os.open(p / 'data.bin', flags))
    except OSError as e: assert e.errno in (errno.EROFS, errno.EOPNOTSUPP)
    else: raise AssertionError('mutation flags accepted')
for flags in (os.O_RDONLY | os.O_SYNC, os.O_RDONLY | os.O_DSYNC):
    try: os.close(os.open(p / 'data.bin', flags))
    except OSError as e: assert e.errno == errno.EOPNOTSUPP, e
    else: raise AssertionError('synchronous guarantee accepted')
fd = os.open(p / 'data.bin', os.O_RDONLY)
try:
    try: os.fsync(fd)
    except OSError as e: assert e.errno == errno.EOPNOTSUPP, e
    else: raise AssertionError('fsync guarantee accepted')
finally: os.close(fd)
for action in (lambda: os.unlink(p / 'data.bin'), lambda: os.mkdir(p / 'new'),
               lambda: os.rename(p, p.parent / 'renamed')):
    try: action()
    except OSError as e: assert e.errno in (errno.EROFS, errno.EBUSY, errno.EACCES, errno.EPERM)
    else: raise AssertionError('managed/mutation operation accepted')

# An ordinary non-owner cannot rename or remove the managed Workspace child.
ordinary = """
import errno, os
p = '/layerfs/workspace/read'
for action in (lambda: os.rename(p, p + '-ordinary-move'), lambda: os.rmdir(p)):
    try: action()
    except OSError as error:
        assert error.errno in (errno.EACCES, errno.EPERM, errno.EBUSY, errno.EROFS), error
    else: raise AssertionError('ordinary user changed managed mount identity')
"""
subprocess.run(['setpriv', '--reuid=65534', '--regid=65534', '--clear-groups',
                'python3', '-c', ordinary], check=True, timeout=5)

vfs = os.statvfs(p)
assert vfs.f_blocks == vfs.f_bfree == vfs.f_bavail == 0 and vfs.f_namemax == 255
print(json.dumps({'status':'PASS','bytes':len(expected),'names':len(names)+8,
                  'kernel_largefile_flag':kernel_largefile,
                  'directory_seek':True,'ordinary_user_root_refusal':True,'executable_bytes':(p/'tool').stat().st_size}))
