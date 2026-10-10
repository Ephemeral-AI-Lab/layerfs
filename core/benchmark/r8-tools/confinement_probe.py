#!/usr/bin/env python3
"""Actual access attempts of one ordinary unprivileged Sandbox command.

Runs inside the daemon's container as the configured command identity, with
its working directory in one mounted Workspace. Every cell is a real system
call against a protected object or route and records the actual outcome; no
expectation is read from the product. It mutates nothing it is allowed to
reach and keeps no descriptor. The caller decides the verdict: a cell whose
`protected` flag is set must not be ALLOWED; a cell marked `record` only
reports what the deployed topology does.
"""
import ctypes
import errno
import json
import os
import resource
import socket
import subprocess
import sys

LIBC = ctypes.CDLL(None, use_errno=True)
STORE = "/layerfs-store/global"
LOCAL = "/layerfs-local"
PROTECTED = [STORE, STORE + "/store.sqlite", STORE + "/store.sqlite-wal", STORE + "/store.sqlite-shm",
             LOCAL, LOCAL + "/overlay", LOCAL + "/overlay/overlay.sqlite",
             LOCAL + "/config", LOCAL + "/config/daemon.setup"]
CLONE_NEWNS, CLONE_NEWUSER = 0x00020000, 0x10000000
cells = []


def outcome(call):
    try:
        value = call()
    except OSError as error:
        return errno.errorcode.get(error.errno, str(error.errno)), None
    return "ALLOWED", value


def cell(group, target, action, call, protected=True, record=False):
    result, value = outcome(call)
    if isinstance(value, int) and action.startswith("open"):
        os.close(value)
        value = None
    row = dict(group=group, target=target, action=action, outcome=result,
               protected=protected and not record, record=record)
    if value is not None:
        row["value"] = value
    cells.append(row)
    return result


def paths(group, prefix):
    for path in PROTECTED:
        target = prefix + path
        cell(group, target, "open_read", lambda: os.open(target, os.O_RDONLY | os.O_NONBLOCK | os.O_CLOEXEC))
        cell(group, target, "open_write", lambda: os.open(target, os.O_WRONLY | os.O_NONBLOCK | os.O_CLOEXEC))
        # Metadata of a name is visible wherever its parent is searchable; it
        # is recorded, never judged as access to the protected object.
        cell(group, target, "lstat", lambda: oct(os.lstat(target).st_mode), record=True)
    for directory in (STORE, LOCAL, LOCAL + "/overlay", LOCAL + "/config"):
        cell(group, prefix + directory, "listdir", lambda: sorted(os.listdir(prefix + directory)))


def syscall(name, *arguments):
    def call():
        ctypes.set_errno(0)
        if getattr(LIBC, name)(*arguments) == -1:
            raise OSError(ctypes.get_errno(), name)
        return 0
    return call


def helper(argv):
    def call():
        try:
            done = subprocess.run(argv, capture_output=True, timeout=5)
        except FileNotFoundError:
            raise OSError(errno.ENOENT, argv[0])
        if done.returncode:
            raise OSError(errno.EPERM, done.stderr.decode(errors="replace").strip()[:160])
        return 0
    return call


def namespace(mount):
    """One child attempts a private user and mount namespace; it never mounts."""
    reader, writer = os.pipe()
    child = os.fork()
    if child == 0:
        os.close(reader)
        report = {}
        result, _ = outcome(syscall("unshare", CLONE_NEWUSER | CLONE_NEWNS))
        report["unshare_user_mount"] = result
        if result == "ALLOWED":
            for path in PROTECTED:
                report["inside:" + path] = outcome(
                    lambda: os.close(os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_CLOEXEC)))[0]
            report["inside:umount2"] = outcome(syscall("umount2", mount.encode(), 0))[0]
            report["inside:mountinfo_has_workspace"] = any(
                " " + mount + " " in line for line in open("/proc/self/mountinfo"))
        os.write(writer, json.dumps(report).encode())
        os._exit(0)
    os.close(writer)
    data = b""
    while True:
        block = os.read(reader, 65536)
        if not block:
            break
        data += block
    os.close(reader)
    os.waitpid(child, 0)
    return json.loads(data)


def descriptors():
    """Every descriptor this command holds, with what it refers to.

    Each number is asked by path, so the enumeration itself opens nothing
    and cannot report a descriptor of its own.
    """
    held = {}
    for number in range(min(resource.getrlimit(resource.RLIMIT_NOFILE)[0], 65536)):
        result, target = outcome(lambda: os.readlink("/proc/self/fd/%d" % number))
        if result == "ALLOWED":
            held[str(number)] = target
        elif result != "ENOENT":
            held[str(number)] = "UNREADABLE " + result
    return held


def main():
    sibling, port = sys.argv[1], int(sys.argv[2])
    mount = os.getcwd()
    paths("direct", "")
    paths("proc_root", "/proc/1/root")
    for name in ("cwd", "exe", "root"):
        cell("proc_link", "/proc/1/" + name, "readlink", lambda: os.readlink("/proc/1/" + name))
    for name in ("fd", "fdinfo", "map_files"):
        cell("proc_descriptors", "/proc/1/" + name, "listdir", lambda: sorted(os.listdir("/proc/1/" + name)))
    tasks = outcome(lambda: sorted(os.listdir("/proc/1/task")))[1] or []
    for task in tasks:
        cell("proc_descriptors", "/proc/1/task/%s/fd" % task, "listdir",
             lambda: sorted(os.listdir("/proc/1/task/%s/fd" % task)))
    def one_byte(path):
        # Several /proc files open for anyone and refuse at the first read.
        descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_CLOEXEC)
        try:
            return len(os.pread(descriptor, 1, 4096 if path.endswith("/mem") else 0))
        finally:
            os.close(descriptor)
    for name in ("environ", "maps", "mem", "smaps", "pagemap", "auxv", "stack", "io"):
        cell("proc_memory", "/proc/1/" + name, "open_and_read_one_byte",
             lambda: one_byte("/proc/1/" + name))
    cell("device", "/dev/fuse", "open_rdwr", lambda: os.open("/dev/fuse", os.O_RDWR | os.O_NONBLOCK | os.O_CLOEXEC))
    cell("device", "/tmp/confinement-fuse-twin", "mknod_char_10_229",
         lambda: os.mknod("/tmp/confinement-fuse-twin", 0o600 | 0o020000, os.makedev(10, 229)))

    class Vector(ctypes.Structure):
        _fields_ = [("base", ctypes.c_void_p), ("length", ctypes.c_size_t)]
    landing = ctypes.create_string_buffer(1)
    local = Vector(ctypes.addressof(landing), 1)
    # The access decision precedes any use of the remote address, so the
    # address is arbitrary; the controller requires exactly EPERM here.
    remote = Vector(0x10000, 1)
    cell("daemon_process", "pid 1", "process_vm_readv",
         syscall("process_vm_readv", 1, ctypes.byref(local), 1, ctypes.byref(remote), 1, 0))
    cell("abort_control", "/sys/fs/fuse/connections", "listdir",
         lambda: sorted(os.listdir("/sys/fs/fuse/connections")), record=True)
    for name in outcome(lambda: os.listdir("/sys/fs/fuse/connections"))[1] or []:
        cell("abort_control", "/sys/fs/fuse/connections/%s/abort" % name, "open_write",
             lambda: os.open("/sys/fs/fuse/connections/%s/abort" % name, os.O_WRONLY | os.O_CLOEXEC))
    cell("detach", mount, "umount2", syscall("umount2", mount.encode(), 0))
    cell("detach", mount, "umount2_detach", syscall("umount2", mount.encode(), 2))
    for argv in (["umount", mount], ["umount", "-l", mount], ["fusermount", "-u", mount],
                 ["fusermount3", "-u", mount]):
        cell("detach", mount, " ".join(argv[:-1]), helper(argv))
    cell("daemon_process", "pid 1", "kill_0", lambda: os.kill(1, 0))
    cell("daemon_process", "pid 1", "ptrace_attach", syscall("ptrace", 16, 1, 0, 0))
    cell("mount", mount, "mount_tmpfs_over", syscall("mount", b"none", mount.encode(), b"tmpfs", 0, None))
    cell("mount", sibling, "umount2_sibling", syscall("umount2", sibling.encode(), 0))

    def connect():
        with socket.create_connection(("127.0.0.1", port), timeout=2):
            return "connected and closed without authentication"
    cell("control_listener", "127.0.0.1:%d" % port, "tcp_connect", connect, record=True)
    cell("sibling", sibling, "listdir", lambda: len(os.listdir(sibling)), record=True)
    status = {}
    for line in open("/proc/self/status"):
        key, _, value = line.partition(":")
        if key in {"Uid", "Gid", "Groups", "CapInh", "CapPrm", "CapEff", "CapBnd", "CapAmb",
                   "NoNewPrivs", "Seccomp"}:
            status[key] = value.strip()
    mountinfo = [line.rstrip("\n") for line in open("/proc/self/mountinfo")
                 if any(" " + point + " " in line for point in ("/", "/layerfs-store", mount, sibling))]
    print(json.dumps(dict(schema="r8b-confinement-probe-v1", cwd=mount, sibling=sibling,
                          identity=dict(uid=os.getuid(), gid=os.getgid(), groups=os.getgroups(), status=status),
                          own_descriptors=descriptors(),
                          namespace=namespace(mount), mountinfo=mountinfo, cells=cells), sort_keys=True))


if __name__ == "__main__":
    main()
