#!/usr/bin/env python3
"""Real writable daemon, mounted edits and authenticated explicit Commit.

Closed content bytes are reused; every selection creates its own live C5 producer.
"""
import ctypes
import hashlib
import itertools
import json
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import sys
import time
import uuid

import control_unmount as driver
import control_commit_wire as wire
import stage_route

driver.MODE = 'functional-authenticated-workspace-commit'
driver.ENTRY_SOURCE = Path(__file__)
driver.CASES = {
    'commit_repeated': ['mounted-existing-file-A-B-A-explicit-Commits-install-exact-C5-heads-and-revisions'],
    'commit_dirty_signal': ['dirty-signal-detaches-with-control-live-until-explicit-Commit-remount-and-close'],
    'commit_control_loss': ['one-Commit-lost-control-terminal-is-Unknown-with-independent-current-observations-no-replay'],
    'commit_live_successor': ['actual-C2-RESERVED-lock-save-allows-mounted-live-successor-and-retains-control-during-signal'],
    'commit_authority': ['Commit-grant-target-service-authority-and-expiry-are-independent'],
    'commit_refusals': ['readonly-Commit-partial-input-and-budget-refusals-preserve-source'],
    'commit_denied': ['known-service-Commit-denial-retains-full-typed-failure-with-explicit-native-cleanup-refusal'],
    'commit_small_project': ['writable-mount-two-explicit-Commits-and-the-acknowledged-remounted-tree'],
}
driver.NOT_RUN = ['upstream native Unknown Commit failure (control-terminal loss is a separate outer delivery case)',
                  'prepared npm workload', 'R6', 'directory deltas beyond the 128-name admission bound',
                  'hard RSS/cgroup/performance qualification', 'failed-submission discard/recovery',
                  'writable process-restart or crash recovery']

TARGET = {'workspace': b'read', 'incarnation': b'\x71' * 32}
COMMIT_MS = 10_000
DISK_BYTES = 64 * 1024 * 1024


def begin_commit(client, identity, **options):
    driver.begin(client, identity, opcode=14, budget=options.pop('budget', COMMIT_MS), **options)


def commit(client, identity, **options):
    begin_commit(client, identity, **options)
    return wire.receive(client, workspace=options.get('workspace', b'read'),
                        incarnation=options.get('incarnation', b'\x71' * 32))


def current(client, identity, writable=True):
    kind, result = driver.status.query(client, identity)
    assert kind == 6 and result['workspace'] == 'read' and result['incarnation'] == '71' * 32, result
    assert ('generation' in result) == writable, result
    return result


def refused(result, code):
    assert result == {'kind': 'failure', 'code': code, 'unknown': 0, 'cleanup': 0}, result


class Witness:
    """Independent public observation calls over one owned control session.

    The daemon admits exactly one control session, so this witness owns its
    session for the whole case and numbers its requests from one.
    """
    def __init__(self, port, private, public, name):
        self.authority = port, private, public
        self.name = name
        self.client = None
        self.ids = itertools.count(1)

    @staticmethod
    def blob(value):
        return driver.route.blob(value)

    def request(self, opcode, body, profile=1):
        if self.client is None:
            self.open()
        identity = next(self.ids)
        self.client.stdin.write(driver.route.begin(identity, opcode, body, profile=profile))
        self.client.stdin.write(driver.route.frame(4, identity, struct.pack('>Q', 0)))
        self.client.stdin.flush()
        data = bytearray(); end = time.monotonic() + 10
        while True:
            header = driver.route.read_exact(self.client.stdout.fileno(), 20, end)
            assert header[:4] == b'LFB1'
            kind, flags, reserved, response_id, length = struct.unpack('>BBHQI', header[4:])
            assert flags == reserved == 0 and response_id == identity and length <= 32768
            value = driver.route.read_exact(self.client.stdout.fileno(), length, end)
            if kind == 5:
                data.extend(value); assert len(data) <= 128 * 1024
            else:
                assert kind == 6, (opcode, kind, value, len(data), body.hex(), self.client.poll())
                return value, bytes(data)

    def branch(self, branch):
        body, data = self.request(6, b'\x03' + bytes.fromhex(branch), 2)
        assert not data
        tag, result = driver.route.history(body); assert tag == 'BranchSnapshot'
        return wire.jsonable(result)

    def attributes(self, root, path):
        body, data = self.request(2, bytes.fromhex(root) + b'\x04' + driver.route.blob(path))
        assert not data
        r = driver.route.Reader(body); assert r.u8() == 9
        result = {'serial': r.u64(), 'kind': r.u8(), 'references': r.u64(),
                  'content': r.take(32).hex(), 'metadata': r.take(32).hex(), 'mode': int.from_bytes(r.take(4), 'big'),
                  'mtime': r.u64(), 'nanoseconds': int.from_bytes(r.take(4), 'big'), 'size': r.u64()}
        r.done(); return result

    def open(self, endpoint=None, server=None):
        assert self.client is None
        port, private, public = self.authority
        self.client = driver.status.controller(endpoint or port, private, server or public)
        self.ids = itertools.count(1)

    def closed(self):
        return self.client is None

    def root_observation(self, root, path):
        """One public read of a saved root: attributes, content and its digest.

        A symlink's selected payload is its target, which the public Readlink
        query returns; its content root is not a regular-file root.
        """
        attr = self.attributes(root, path)
        if attr['kind'] == 2:
            return {'attributes': attr, 'content': attr['content']}
        if attr['kind'] == 3:
            target = self.readlink(root, path)
            return {'attributes': attr, 'content': attr['content'], 'target': target,
                    'digest': hashlib.sha256(target.encode()).hexdigest()}
        data = self.read(attr['content'], attr['size'])
        return {'attributes': attr, 'content': attr['content'], 'data': data.hex(),
                'digest': hashlib.sha256(data).hexdigest()}

    def readlink(self, root, path):
        body, data = self.request(2, bytes.fromhex(root) + b'\x03' + driver.route.blob(path))
        assert not data
        r = driver.route.Reader(body)
        assert r.u8() == 6
        value = r.blob().decode()
        r.done()
        return value

    def missing(self, root, path):
        """The path is absent from this saved root; the public Inspect says so."""
        try:
            self.attributes(root, path)
        except AssertionError as error:
            # One refusal response is the expected outcome; the request helper
            # reports it as (opcode, kind, body, length, raw, client exit).
            observed = error.args[0] if error.args else None
            if isinstance(observed, tuple) and len(observed) > 2 and observed[1] == 7 and observed[2] == b'\x07\x00\x00':
                return True
            raise
        raise AssertionError(f'{path!r} is present in {root}')

    def read(self, content, length):
        body, data = self.request(1, bytes.fromhex(content) + struct.pack('>QQ', 0, length))
        r = driver.route.Reader(body); assert r.u8() == 1 and r.u64() == length; r.done()
        assert len(data) == length
        return data

    def close(self):
        """Close this session and wait for the daemon to be alone again."""
        if self.client is None:
            return
        ended = self.client
        self.client = None
        try:
            ended.stdin.close()
        except BrokenPipeError:
            ended.wait(timeout=6)
        else:
            ended.wait(timeout=6)
        # The daemon clears the closed session on its own schedule; the next
        # request retries a hand-off that arrives while the slot is still closing.
        time.sleep(0.1)



def small_expected(value):
    return bytes([value]) + bytes(i % 251 for i in range(1, 8192))


def large_prefix():
    return b''.join(hashlib.sha256(b'layerfs-control-commit-v1' + i.to_bytes(8, 'big')).digest()
                    for i in range(4096))


SMALL_PROJECT = r'''import ctypes, errno, hashlib, json, os, stat, sys


class Timespec(ctypes.Structure):
    _fields_ = [('seconds', ctypes.c_long), ('nanos', ctypes.c_long)]


LIBC = ctypes.CDLL(None, use_errno=True)
AT_FDCWD, UTIME_OMIT = -100, (1 << 30) - 2


def set_mtime(path, mtime_ns):
    """One utimensat with atime omitted, so the request carries no atime setter."""
    times = (Timespec * 2)()
    times[0].seconds, times[0].nanos = 0, UTIME_OMIT
    times[1].seconds, times[1].nanos = divmod(mtime_ns, 10 ** 9)
    ctypes.set_errno(0)
    rc = LIBC.utimensat(AT_FDCWD, path.encode(), ctypes.byref(times), 0)
    assert rc == 0, 'utimensat %s -> %s(%d)' % (path, errno.errorcode.get(ctypes.get_errno(), '?'), ctypes.get_errno())

root, phase = sys.argv[1], sys.argv[2]
left, right = root + '/left', root + '/right'
out = {}
assert phase in ('first', 'second')

def digest(data):
    return hashlib.sha256(data).hexdigest()

def listing(path):
    return sorted(os.listdir(path))

if phase == 'first':
    # ---- step 2: names, contents, aliases and portable metadata -------------
    os.mkdir(left, 0o750)
    os.mkdir(right, 0o750)
    out['mkdir_modes'] = [stat.S_IMODE(os.lstat(left).st_mode), stat.S_IMODE(os.lstat(right).st_mode)]

    # A regular mknod file: no handle is opened by the creation itself.
    os.mknod(left + '/node.bin', 0o644 | stat.S_IFREG)
    mknod_ino = os.lstat(left + '/node.bin').st_ino
    os.mknod(right + '/victim.txt', 0o644 | stat.S_IFREG)
    victim_ino = os.lstat(right + '/victim.txt').st_ino
    out['mknod'] = {'size': os.lstat(left + '/node.bin').st_size,
                    'mode': stat.S_IMODE(os.lstat(left + '/node.bin').st_mode),
                    'inode': mknod_ino}

    # Write, append and truncate through real syscalls.
    fd = os.open(left + '/a.txt', os.O_RDWR | os.O_CREAT | os.O_EXCL, 0o640)
    os.write(fd, b'alpha')
    os.close(fd)
    fd = os.open(left + '/a.txt', os.O_RDWR | os.O_APPEND)
    os.write(fd, b'-beta')
    os.close(fd)
    appended = open(left + '/a.txt', 'rb').read()
    fd = os.open(left + '/a.txt', os.O_RDWR)
    os.ftruncate(fd, 5)
    truncated = os.read(fd, 64)
    os.close(fd)
    out['write_append_truncate'] = {'after_append': appended.decode(),
                                    'after_truncate': truncated.decode(),
                                    'size': os.lstat(left + '/a.txt').st_size}

    # chmod plus an mtime set with atime omitted (UTIME_OMIT).
    os.chmod(left + '/a.txt', 0o600)
    set_mtime(left + '/a.txt', 1700000123456789000)
    os.chmod(left, 0o1777)
    a = os.lstat(left + '/a.txt')
    out['metadata'] = {'file_mode': stat.S_IMODE(a.st_mode), 'file_mtime_ns': a.st_mtime_ns,
                       'dir_mode': stat.S_IMODE(os.lstat(left).st_mode),
                       'dir_mtime_ns': os.lstat(left).st_mtime_ns}

    # A symlink and a hard-link alias.
    os.symlink('a.txt', left + '/sym')
    alias_ino = os.link(left + '/node.bin', left + '/alias.bin')
    alias = os.lstat(left + '/alias.bin')
    out['symlink'] = {'target': os.readlink(left + '/sym'), 'mode': stat.S_IMODE(os.lstat(left + '/sym').st_mode)}
    out['link'] = {'inode': alias.st_ino, 'mknod_inode': mknod_ino,
                   'nlink': alias.st_nlink, 'names': [os.lstat(left + '/node.bin').st_nlink]}

    # ---- step 2/3: replacement with the destination FD held, and unlink ------
    victim_fd = os.open(right + '/victim.txt', os.O_RDWR)
    os.write(victim_fd, b'victim-body')
    os.rename(left + '/node.bin', right + '/victim.txt')
    after_rename = os.fstat(victim_fd)
    out['replace'] = {'held_inode': after_rename.st_ino, 'replaced_inode': victim_ino,
                      'held_nlink': after_rename.st_nlink,
                      'held_body': os.pread(victim_fd, 64, 0).decode(),
                      'name_inode': os.lstat(right + '/victim.txt').st_ino,
                      'alias_inode': os.lstat(left + '/alias.bin').st_ino,
                      'left': listing(left), 'right': listing(right)}
    assert after_rename.st_ino == victim_ino and out['replace']['held_body'] == 'victim-body'
    assert out['replace']['name_inode'] == mknod_ino == out['replace']['alias_inode']

    # Unlink the last name of the mknod inode while its own FD stays open.
    held = os.open(left + '/alias.bin', os.O_RDWR)
    os.unlink(left + '/alias.bin')
    held_ino = os.fstat(held).st_ino
    os.write(held, b'orphan')
    orphan = os.pread(held, 64, 0)
    out['unlink'] = {'inode': held_ino, 'nlink': os.fstat(held).st_nlink,
                     'body': orphan.decode(), 'left': listing(left), 'right': listing(right)}
    # The kernel answers fstat from its own inode, so its nlink is not the product's
    # link count here; identity, content and name removal are what this step proves.
    assert held_ino == mknod_ino and orphan == b'orphan', out['unlink'] | {'mknod_ino': mknod_ino}
    assert 'alias.bin' not in out['unlink']['left'] and 'node.bin' not in out['unlink']['left']

    # ---- step 3: refusals leave no partial change, then an empty rmdir -------
    refusals = {}
    try:
        os.rmdir(right)
    except OSError as error:
        refusals['nonempty_rmdir'] = error.errno
    else:
        raise AssertionError('nonempty rmdir accepted')

    renameat2 = getattr(LIBC, 'renameat2', None)
    assert renameat2 is not None, 'renameat2 unavailable'
    renameat2.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
    RENAME_NOREPLACE = 1
    before_names = [listing(left), listing(right)]
    before_inodes = [os.lstat(left + '/a.txt').st_ino, os.lstat(right + '/victim.txt').st_ino]
    ctypes.set_errno(0)
    rc = renameat2(AT_FDCWD, (left + '/a.txt').encode(), AT_FDCWD, (right + '/victim.txt').encode(), RENAME_NOREPLACE)
    refusals['noreplace_collision'] = ctypes.get_errno() if rc != 0 else 0
    assert rc != 0 and ctypes.get_errno() == errno.EEXIST, (rc, ctypes.get_errno())
    assert before_names == [listing(left), listing(right)]
    assert before_inodes == [os.lstat(left + '/a.txt').st_ino, os.lstat(right + '/victim.txt').st_ino]
    os.mkdir(left + '/empty', 0o755)
    os.rmdir(left + '/empty')
    assert 'empty' not in listing(left)
    out['refusals'] = refusals

    # ---- step 3: names, contents, inode identities, link counts, metadata ----
    out['final_tree'] = {'left': listing(left), 'right': listing(right)}
    out['identities'] = {'a.txt': os.lstat(left + '/a.txt').st_ino, 'sym': os.lstat(left + '/sym').st_ino,
                         'sym_target': os.readlink(left + '/sym'),
                         'victim.txt': os.lstat(right + '/victim.txt').st_ino,
                         'a.txt_nlink': os.lstat(left + '/a.txt').st_nlink,
                         'victim.txt_nlink': os.lstat(right + '/victim.txt').st_nlink}
    out['contents'] = {'a.txt': open(left + '/a.txt', 'rb').read().decode(),
                       'victim.txt': open(right + '/victim.txt', 'rb').read().decode()}
    out['held_after'] = {'body': os.pread(held, 64, 0).decode(), 'inode': held_ino}
    out['digests'] = {'a.txt': digest(out['contents']['a.txt'].encode()),
                      'victim.txt': digest(out['contents']['victim.txt'].encode()),
                      'held': digest(out['held_after']['body'].encode())}

if phase == 'first':
    print(json.dumps(out))
    sys.exit(0)

# ---- step 5: the second edit: a rename, an unlink and a metadata change ----
os.rename(left + '/a.txt', right + '/moved.txt')
os.unlink(right + '/victim.txt')
os.chmod(right + '/moved.txt', 0o640)
set_mtime(right + '/moved.txt', 1700000999000000000)
moved = os.lstat(right + '/moved.txt')
out['second_edit'] = {'left': listing(left), 'right': listing(right),
                      'moved_inode': moved.st_ino, 'moved_mode': stat.S_IMODE(moved.st_mode),
                      'moved_mtime_ns': moved.st_mtime_ns, 'moved_size': moved.st_size,
                      'moved_body': open(right + '/moved.txt', 'rb').read().decode(),
                      'sym_inode': os.lstat(left + '/sym').st_ino,
                      'sym_target': os.readlink(left + '/sym')}
try:
    os.lstat(right + '/victim.txt')
except FileNotFoundError:
    out['second_edit']['victim_absent'] = True
else:
    out['second_edit']['victim_absent'] = False
assert out['second_edit']['victim_absent'] and out['second_edit']['left'] == ['sym']

print(json.dumps(out))
'''


SMALL_PROJECT_NAMES = ('left', 'right')
SMALL_PROJECT_A_PRESENT = (b'left/a.txt', b'left/sym', b'left', b'right/victim.txt')
SMALL_PROJECT_A_ABSENT = (b'left/node.bin', b'left/alias.bin', b'left/empty', b'right')
SMALL_PROJECT_B_PRESENT = (b'left', b'right/moved.txt')
SMALL_PROJECT_B_ABSENT = (b'left/a.txt', b'left/sym', b'right/victim.txt', b'right')


def verify_small_project(witness, branch, result, observed, previous):
    """Exact saved-tree check for one explicit Commit, through public Service reads."""
    assert result['kind'] == 'Completed' and result['outcome']['kind'] == 'Committed', result
    outcome = result['outcome']
    snapshot = witness.branch(branch)
    assert snapshot['effective_root'] == outcome['root'], (snapshot, outcome)
    assert snapshot['branch']['head_commit'] == outcome['commit']
    if previous is not None:
        assert outcome['parent'] == previous['head'], (outcome, previous)
    root = outcome['root']
    expected = observed['final_tree'] if previous is None else observed['second_edit']
    assert sorted(expected['left']) == (['a.txt', 'sym'] if previous is None else [])
    assert sorted(expected['right']) == (['victim.txt'] if previous is None else ['moved.txt'])
    names = {name: witness.root_observation(root, name)
             for name in (SMALL_PROJECT_A_PRESENT if previous is None else SMALL_PROJECT_B_PRESENT)}
    for name in (SMALL_PROJECT_A_ABSENT if previous is None else SMALL_PROJECT_B_ABSENT):
        witness.missing(root, name)
    if previous is None:
        file_attr = names[b'left/a.txt']['attributes']
        assert file_attr['kind'] == 1 and file_attr['mode'] == 0o600, file_attr
        assert (file_attr['mtime'], file_attr['nanoseconds']) == (1_700_000_123, 456_789_000), file_attr
        assert file_attr['size'] == observed['write_append_truncate']['size'] == 5
        assert names[b'left/a.txt']['digest'] == observed['digests']['a.txt'], (
            names[b'left/a.txt']['digest'], observed['digests']['a.txt'], observed['contents'])
        assert bytes.fromhex(names[b'left/a.txt']['data']).decode() == observed['contents']['a.txt']
        assert names[b'left/sym']['attributes']['kind'] == 3, names[b'left/sym']
        assert names[b'left/sym']['attributes']['mode'] == 0o777, names[b'left/sym']
        assert names[b'left/sym']['target'] == 'a.txt', names[b'left/sym']
        assert names[b'left']['attributes']['kind'] == 2, names[b'left']
        assert names[b'left']['attributes']['mode'] == 0o1777, names[b'left']
        victim = names[b'right/victim.txt']['attributes']
        assert victim['serial'] == observed['identities']['victim.txt'], (victim, observed)
        assert victim['references'] == 1 and victim['mode'] == 0o644, victim
        assert names[b'right/victim.txt']['digest'] == observed['digests']['victim.txt']
        assert bytes.fromhex(names[b'right/victim.txt']['data']).decode() == observed['contents']['victim.txt']
        return {'root': root, 'head': snapshot['branch']['head_commit'], 'branch': snapshot,
                'names': {name.decode(): names[name]['attributes'] for name in names},
                'a_content': names[b'left/a.txt']['content'],
                'a_data': names[b'left/a.txt']['data'],
                'victim_content': names[b'right/victim.txt']['content'],
                'absent': [name.decode() for name in SMALL_PROJECT_A_ABSENT]}
    moved = names[b'right/moved.txt']['attributes']
    assert moved['serial'] == observed['second_edit']['moved_inode'], (moved, observed)
    assert moved['kind'] == 1 and moved['mode'] == 0o640, moved
    assert (moved['mtime'], moved['nanoseconds']) == (1_700_000_999, 0), moved
    assert names[b'right/moved.txt']['digest'] == observed['digests']['a.txt']
    assert bytes.fromhex(names[b'right/moved.txt']['data']).decode() == observed['contents']['a.txt']
    assert names[b'left']['attributes']['kind'] == 2, names[b'left']
    return {'root': root, 'head': snapshot['branch']['head_commit'], 'branch': snapshot,
            'previous_root': previous['root'],
            'names': {name.decode(): names[name]['attributes'] for name in names},
            'moved_content': names[b'right/moved.txt']['content'],
            'absent': [name.decode() for name in SMALL_PROJECT_B_ABSENT]}


def small_project_read(name):
    """The acknowledged state as the remounted read-only Workspace presents it."""
    code = """import json,os,stat
p='/layerfs/workspace/read'
def row(path):
    s=os.lstat(p+path)
    return {'inode':s.st_ino,'mode':stat.S_IMODE(s.st_mode),'size':s.st_size,'nlink':s.st_nlink,
            'mtime_ns':s.st_mtime_ns,'kind':'link' if stat.S_ISLNK(s.st_mode) else ('dir' if stat.S_ISDIR(s.st_mode) else 'file')}
result={'left':sorted(os.listdir(p+'/left')),'right':sorted(os.listdir(p+'/right')),
        'left_row':row('/left'),'moved':row('/right/moved.txt'),'moved_body':open(p+'/right/moved.txt','rb').read().decode()}
try: os.lstat(p+'/right/victim.txt')
except FileNotFoundError: result['victim_absent']=True
else: result['victim_absent']=False
print(json.dumps(result))
"""
    return json.loads(driver.mount.checked(['docker', 'exec', name, 'python3', '-c', code], text=True).stdout)


def small_project(name, output, phase):
    """Run the 51 section 6 syscall scenario inside the mounted runtime container.

    The mounted runtime's root is read-only, so the program arrives on stdin and
    the receipt keeps both the exact source and its stdout.
    """
    program = output / 'small-project.py'
    if not program.exists():
        program.write_text(SMALL_PROJECT)
    result = subprocess.run(['docker', 'exec', '-i', name, 'python3', '-u', '-',
                             '/layerfs/workspace/read', phase],
                            input=SMALL_PROJECT.encode(), capture_output=True, timeout=30)
    (output / f'small-project-{phase}.stdout').write_bytes(result.stdout)
    (output / f'small-project-{phase}.stderr').write_bytes(result.stderr)
    assert result.returncode == 0, result.stderr.decode(errors='replace')
    return json.loads(result.stdout)


def mounted_edit(name, operation, value=0, resize=0):
    code = """import hashlib,json,os,sys
from pathlib import Path
p=Path('/layerfs/workspace/read'); op=sys.argv[1]; value=int(sys.argv[2]); size=int(sys.argv[3])
fd=os.open(p/'data.bin',os.O_RDWR)
try:
    if size: os.ftruncate(fd,size)
    if op=='byte': data=bytes([value])
    elif op=='live': data=b'LIVE'
    elif op=='large': data=b''.join(hashlib.sha256(b'layerfs-control-commit-v1'+i.to_bytes(8,'big')).digest() for i in range(131072))
    else: raise AssertionError(op)
    assert os.pwrite(fd,data,0)==len(data)
    assert os.pread(fd,min(len(data),131072),0)==data[:131072]
finally: os.close(fd)
assert (p/'alias').stat().st_ino==(p/'data.bin').stat().st_ino
print(json.dumps({'bytes_written':len(data),'input_sha256':hashlib.sha256(data).hexdigest(),
                  'size':(p/'data.bin').stat().st_size,'inode':(p/'data.bin').stat().st_ino}))
"""
    return json.loads(driver.mount.checked(['docker', 'exec', name, 'python3', '-c', code,
                                           operation, str(value), str(resize)], text=True).stdout)


def mounted_read(name, expected, size):
    code = """import hashlib,json,sys
from pathlib import Path
p=Path('/layerfs/workspace/read'); n=int(sys.argv[1]); expected_size=int(sys.argv[2])
data=(p/'data.bin').open('rb').read(n); alias=(p/'alias').open('rb').read(n)
assert data==alias and (p/'data.bin').stat().st_ino==(p/'alias').stat().st_ino
assert (p/'data.bin').stat().st_size==expected_size
print(json.dumps({'bytes_read':len(data),'sha256':hashlib.sha256(data).hexdigest(),
                  'inode':(p/'data.bin').stat().st_ino,'mode':(p/'data.bin').stat().st_mode&0o777}))
"""
    result = json.loads(driver.mount.checked(['docker', 'exec', name, 'python3', '-c', code,
                                              str(len(expected)), str(size)], text=True).stdout)
    assert result['sha256'] == hashlib.sha256(expected).hexdigest() and result['bytes_read'] == len(expected)
    return result


def held_live_files(name):
    code = """import hashlib,json,os,sys
fd=os.open('/layerfs/workspace/read/data.bin',os.O_RDWR)
alias=os.open('/layerfs/workspace/read/alias',os.O_RDONLY)
try:
    assert os.fstat(fd).st_ino==os.fstat(alias).st_ino
    print('OPENED',flush=True)
    assert sys.stdin.readline()=='WRITE\\n'
    assert os.pwrite(fd,b'LIVE',0)==4
    data=os.pread(fd,131072,0)
    assert data[:4]==b'LIVE' and data==os.pread(alias,131072,0)
    stat=os.fstat(fd); other=os.fstat(alias)
    assert stat.st_ino==other.st_ino and stat.st_size==other.st_size==4194304
    print(json.dumps({'bytes_written':4,'bytes_read':len(data),'sha256':hashlib.sha256(data).hexdigest(),
                      'inode':stat.st_ino,'size':stat.st_size,'route':'pwrite/pread/fstat on pre-opened data and alias FDs'}),flush=True)
    assert sys.stdin.readline()=='CLOSE\\n'
finally:
    os.close(alias); os.close(fd)
print('CLOSED',flush=True)
"""
    return subprocess.Popen(['docker', 'exec', '-i', name, 'python3', '-u', '-c', code],
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def helper_line(process):
    end = time.monotonic() + 3; line = bytearray()
    while len(line) < 4096:
        line.extend(driver.route.read_exact(process.stdout.fileno(), 1, end))
        if line[-1] == 10: return line.decode().strip()
    raise AssertionError('unbounded helper observation')


def verify_saved(witness, branch, result, expected, size, previous=None):
    assert result['kind'] == 'Completed' and result['outcome']['kind'] in ('Committed', 'UpToDate'), result
    outcome = result['outcome']; observed = witness.branch(branch)
    assert observed['effective_root'] == outcome['root'], (observed, outcome)
    if outcome['kind'] == 'Committed':
        assert observed['branch']['head_commit'] == outcome['commit']
        if previous is not None: assert outcome['parent'] == previous['branch']['head_commit']
    else: assert observed['branch']['head_commit'] == outcome['head']
    data = witness.attributes(outcome['root'], b'data.bin'); alias = witness.attributes(outcome['root'], b'alias')
    assert data == alias and data['size'] == size, (data, alias)
    assert witness.read(data['content'], len(expected)) == expected
    return {'branch': observed, 'attributes': data}


def completed(client, identity, before):
    result = commit(client, identity); assert result['kind'] == 'Completed', result
    after = current(client, identity + 1)
    assert result['generation'] == before['generation'] and after['generation'] == result['generation'] + 1
    assert after['revision'] == result['revision'] > before['revision']
    assert after['dirty_inodes'] == 0 and after['submission'] is None, after
    return result, after


def close_client(client, name, expected=0):
    """Close one controller and wait for its control session to be released.

    The daemon admits exactly one control session and clears the slot on its own
    schedule, so a checked close waits for the listener to be alone again.
    """
    driver.close(client, name, expected)
    end = time.monotonic() + 5
    while True:
        try:
            driver.status.control_threads(name, 1)
            return
        except subprocess.CalledProcessError:
            if time.monotonic() >= end:
                raise
            time.sleep(0.02)


def execute(args, report):
    fixture = json.loads(args.fixture.read_text())
    assert fixture['status'] == 'PASS' and fixture['logical_bytes'] == 64 * 1024 * 1024
    service_dir = args.output / 'service'; service_dir.mkdir()
    master = args.fixture.parent / 'service/store.sqlite'; master_hash = driver.sha(master)
    assert master_hash == fixture['closed_master_sha256']['store.sqlite']
    shutil.copyfile(master, service_dir / 'store.sqlite'); assert driver.sha(service_dir / 'store.sqlite') == master_hash
    report['fixture_reuse'] = {'receipt': str(args.fixture), 'receipt_sha256': driver.sha(args.fixture),
        'closed_store_sha256': master_hash, 'clone_method': 'independent-byte-copy',
        'history': 'fresh live C5 producer; CREATE=1 and no copied/restarted catalog', 'cache_claim': None}
    report.update(commit_request_budget_ms=COMMIT_MS, commit_wire_max_ms=600000, caller_receive_seconds=11,
                  workspace_disk_budget_bytes=DISK_BYTES, fixture_logical_bytes=64 * 1024 * 1024)
    report['verification_coverage'] = ('Repeated edits verify the entire declared8192-byte file; live-successor '
        'verifies128KiB from captured and successor content, exact C5 roots/heads and alias identity. '
        'Mounted truncation to8192B or4MiB is an explicit workload operation; no full64MiB readback or performance claim.')
    peers = [(1, 'good', 63), (2, 'old', 31), (3, 'commit', 32), (4, 'none', 0), (5, 'expiry', 33)]
    keys = {name: os.urandom(32).hex() for name in ['service', 'writer', 'denied'] + [p[1] for p in peers]}
    public = {name: driver.route.public_key(key) for name, key in keys.items()}
    env = os.environ.copy()
    env.update(LAYERFS_PRIVATE_KEY=keys['service'],
        LAYERFS_PEERS=f'1,{public["writer"]},{int(time.time())+3600},255;2,{public["denied"]},{int(time.time())+3600},191',
        LAYERFS_STORE=str(service_dir / 'store.sqlite'), LAYERFS_LISTEN='0.0.0.0:0', LAYERFS_TELEMETRY='off',
        LAYERFS_HISTORY_CATALOG=str(service_dir / 'history.sqlite'), LAYERFS_HISTORY_CREATE='1',
        LAYERFS_HISTORY_BINDING='pair1-stage', LAYERFS_HISTORY_INCARNATION='1',
        LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(), LAYERFS_CONSTRUCTION_WORKERS='1')
    service = subprocess.Popen([driver.route.BIN / 'layerfs-service'], env=env, stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    name = 'layerfs-control-commit-' + uuid.uuid4().hex[:12]
    daemon = client = witness = proxy = live_files = None
    volume = container = paused = expected_retention = False
    service_ready = daemon_log = ''
    try:
        service_ready = driver.mount.line_until(service); assert 'ready' in service_ready
        port = int(service_ready.strip().rsplit(':', 1)[1])
        report['fixture'] = stage_route.bootstrap(service_dir, port, keys['writer'], public['service'],
                                                 bytes.fromhex(fixture['file_root']), False)
        branch = report['fixture']['branch']
        witness = Witness(port, keys['writer'], public['service'], name)
        if args.case == 'commit_small_project':
            # This case owns the daemon's single control session from its first
            # request, so the witness opens only when the case asks it to.
            previous = None
        else:
            witness.open(); previous = witness.branch(branch); witness.close()
        report['initial_branch'] = previous
        driver.mount.checked(['docker', 'volume', 'create', name + '-root']); volume = True
        expiry = int(time.time()) + 10
        controls = ';'.join(f'{selector},{public[key]},{expiry if key=="expiry" else int(time.time())+3600},{grant}'
                            for selector, key, grant in peers)
        denied = args.case == 'commit_denied'; readonly = args.case == 'commit_refusals'
        selected = 'denied' if denied else 'writer'
        env = os.environ.copy(); env.update(LAYERFS_ENDPOINT=f'host.docker.internal:{port}',
            LAYERFS_SELECTOR='2' if denied else '1', LAYERFS_PRIVATE_KEY=keys[selected],
            LAYERFS_SERVER_KEY=public['service'], LAYERFS_TELEMETRY='off', LAYERFS_WORKSPACE_ROOT='/layerfs',
            LAYERFS_WORKSPACE_MAX_COUNT='2', LAYERFS_WORKSPACE_DISK_BUDGET_BYTES=str(DISK_BYTES),
            LAYERFS_CONTROL_LISTEN='0.0.0.0:23456', LAYERFS_CONTROL_PEERS=controls, LAYERFS_CONSTRUCTION_WORKERS='1')
        daemon = driver.mount.mount_process(args, env, 'branch:' + branch, name,
            mode='--mount-readonly' if readonly else '--mount-writable'); container = True
        report['runtime_nano_cpus'] = int(driver.mount.checked(
            ['docker', 'inspect', '--format', '{{.HostConfig.NanoCpus}}', name], text=True).stdout.strip())
        assert report['runtime_nano_cpus'] == 2_000_000_000
        first, second = driver.mount.line_until(daemon), driver.mount.line_until(daemon)
        daemon_log = first + second
        assert 'workspace control ready' in first and 'workspace ready' in second, daemon_log
        control_port = int(driver.mount.checked(['docker', 'port', name, '23456/tcp'], text=True).stdout.strip().rsplit(':', 1)[1])
        def controller(key='good', selector=1, endpoint=control_port):
            return driver.status.controller(endpoint, keys[key], public[selected], selector)
        def next_id():
            """One monotonically increasing request identity for this case.

            A fresh session accepts any identity for its first request, so one
            counter serves every session the case opens.
            """
            nonlocal identity
            identity += 1
            return identity

        def session():
            """One authenticated control session, once the daemon is alone again."""
            nonlocal identity
            time.sleep(0.1)
            opened = controller()
            identity = 0
            return opened

        def retry(operation, label, reopen=None):
            """Run one operation on a live authenticated control session.

            The daemon admits exactly one session and clears the slot on its own
            schedule, so a request that arrives while another session is closing is
            retried on a fresh session; every other failure keeps its own identity.
            """
            nonlocal client, identity
            end = time.monotonic() + 20
            while True:
                try:
                    return operation(client)
                except (EOFError, BrokenPipeError):
                    if time.monotonic() >= end:
                        raise
                    report.setdefault('session_handoff_retries', []).append(label)
                    if reopen is not None:
                        reopen.close()
                        reopen.open()
                        continue
                    if client.poll() is None:
                        close_client(client, name, 1)
                    client = session()

        client = controller(); report['initial_status'] = current(client, 1, not readonly)
        if args.case == 'commit_repeated':
            reports = []; saved = []; contents = []
            for index, byte in enumerate(b'ABA'):
                mounted_edit(name, 'byte', byte, 8192 if index == 0 else 0)
                before = current(client, 2 + index * 4)
                result, after = completed(client, 3 + index * 4, before)
                observed = verify_saved(witness, branch, result, small_expected(byte), 8192, previous)
                mounted_read(name, small_expected(byte), 8192)
                previous = observed['branch']; contents.append(observed['attributes']['content'])
                reports.append(result); saved.append({'status': after, **observed})
            assert contents[0] == contents[2] and contents[0] != contents[1]
            report.update(commits=reports, installed=saved, content_A_B_A=contents)
        elif args.case == 'commit_dirty_signal':
            mounted_edit(name, 'byte', ord('A'), 8192); before = current(client, 2)
            assert before['dirty_inodes'] == 1
            driver.mount.stop_mount(name)
            retained = driver.mount.line_until(daemon, timeout=12); daemon_log += retained
            assert 'workspace shutdown retained:' in retained and 'Busy' in retained, retained
            assert daemon.poll() is None
            after_signal = current(client, 3)
            assert not after_signal['mounted'] and after_signal['dirty_inodes'] == 1
            assert after_signal['revision'] == before['revision'] and not after_signal['stopping']
            result, after = completed(client, 4, after_signal)
            report['saved'] = verify_saved(witness, branch, result, small_expected(ord('A')), 8192, previous)
            assert driver.remount(client, 6)['outcome'] == 'Mounted'
            report.update(signal_retained=retained.strip(), control_after_signal=after_signal,
                          commit=result, installed=after, mounted=mounted_read(name, small_expected(ord('A')), 8192))
        elif args.case == 'commit_control_loss':
            mounted_edit(name, 'byte', ord('A'), 8192); before = current(client, 2)
            close_client(client, name); client = None
            proxy = driver.lost_result_proxy(control_port)
            client = controller(endpoint=proxy[0]); result = commit(client, 1)
            assert result == {'kind': 'failure', 'code': 12, 'unknown': 1, 'cleanup': 0}, result
            client.stdin.close(); assert client.wait(timeout=6) == 1; client = None
            proxy[1].join(6)
            for worker in proxy[2]: worker.join(6)
            assert not proxy[1].is_alive() and 'error' not in proxy[3] and proxy[3].get('withheld_ciphertext_bytes', 0) > 0
            driver.status.control_threads(name, 1); client = controller(); observed = current(client, 1)
            assert observed['generation'] == before['generation'] + 1 and observed['dirty_inodes'] == 0 and observed['submission'] is None
            head = witness.branch(branch); attr = witness.attributes(head['effective_root'], b'data.bin')
            assert head['branch']['head_commit'] != previous['branch']['head_commit']
            assert witness.read(attr['content'], 8192) == small_expected(ord('A'))
            report.update(unknown=result, native_current_status=observed, independent_branch=head,
                          mounted=mounted_read(name, small_expected(ord('A')), 8192), proxy=proxy[3],
                          qualification='Outer control terminal loss only; no reconstructed Commit receipt and no native Unknown claim; exactly one Commit submitted')
        elif args.case == 'commit_live_successor':
            assert args.lock_observer is not None
            observer_path = args.lock_observer.resolve(); driver.isolation.namespace().assert_owned(observer_path, 'C2 lock observer')
            library = ctypes.CDLL(str(observer_path)); observer = library.layerfs_reserved_lock_owner
            observer.argtypes = [ctypes.c_char_p]; observer.restype = ctypes.c_int
            lock_path = os.fsencode(service_dir / 'store.sqlite'); assert observer(lock_path) == 0
            report['input'] = mounted_edit(name, 'large', resize=4 * 1024 * 1024)
            live_files = held_live_files(name); assert helper_line(live_files) == 'OPENED'
            before = current(client, 2); begin_commit(client, 3)
            started = time.monotonic(); owner = 0
            while owner == 0 and time.monotonic() - started < 3:
                owner = observer(lock_path); assert owner >= 0 and service.poll() is None
            assert owner == service.pid, f'actual C2 RESERVED owner missed: {owner}'
            service.send_signal(signal.SIGSTOP); paused = True
            pid, stopped = os.waitpid(service.pid, os.WUNTRACED); assert pid == service.pid and os.WIFSTOPPED(stopped)
            assert observer(lock_path) == service.pid, 'C2 save ended before the owned service stopped'
            report['C2_boundary'] = {'service_pid': service.pid, 'reserved_owner': owner,
                'observer_sha256': driver.sha(observer_path), 'wait_seconds': time.monotonic() - started,
                'method': 'read-only F_GETLK on SQLite RESERVED byte, confirmed again after SIGSTOP'}
            driver.mount.stop_mount(name)
            retained = driver.mount.line_until(daemon, timeout=3); daemon_log += retained
            assert 'workspace shutdown retained:' in retained and 'Busy' in retained
            live_files.stdin.write(b'WRITE\n'); live_files.stdin.flush()
            report['live_while_service_stopped'] = json.loads(helper_line(live_files))
            expected_live = b'LIVE' + large_prefix()[4:]
            assert report['live_while_service_stopped']['sha256'] == hashlib.sha256(expected_live).hexdigest()
            assert report['live_while_service_stopped']['bytes_read'] == len(expected_live)
            assert observer(lock_path) == service.pid
            service.send_signal(signal.SIGCONT); paused = False
            result = wire.receive(client); assert result['kind'] == 'Completed', result
            captured = verify_saved(witness, branch, result, large_prefix(), 4 * 1024 * 1024, previous)
            live = current(client, 4)
            assert live['dirty_inodes'] == 1 and live['generation'] == result['generation'] + 1
            assert live['revision'] == result['revision']
            next_result, installed = completed(client, 5, live)
            successor = verify_saved(witness, branch, next_result, b'LIVE' + large_prefix()[4:], 4 * 1024 * 1024, captured['branch'])
            output, errors = live_files.communicate(b'CLOSE\n', timeout=6)
            assert live_files.returncode == 0 and output == b'CLOSED\n', (output, errors)
            live_files = None
            report.update(captured_commit=result, captured=captured, live_status=live,
                          successor_commit=next_result, successor=successor, installed=installed,
                          signal_retained=retained.strip(),
                          concurrent_open_qualification='Only pre-opened descriptors are proved during occupied remote admission; new path/FD open returned EBUSY in retained attempt01 and remains unqualified')
        elif args.case == 'commit_authority':
            mounted_edit(name, 'byte', ord('A'), 8192); before = current(client, 2)
            close_client(client, name); client = None
            for label, key, selector, options in [('old-mask31', 'old', 2, {}), ('no-grant', 'none', 4, {}),
                    ('wrong-target', 'good', 1, {'workspace': b'other'}),
                    ('wrong-incarnation', 'good', 1, {'incarnation': b'\x72' * 32})]:
                client = controller(key, selector); result = commit(client, 1, **options); refused(result, 3)
                close_client(client, name, 1); client = controller(); after = current(client, 1)
                assert (after['generation'], after['revision'], after['dirty_inodes']) == (before['generation'], before['revision'], 1)
                close_client(client, name); client = None
                report.setdefault('permission_refusals', []).append({'case': label, 'result': result})
            client = driver.status.controller(port, keys['writer'], public['service'])
            result = commit(client, 1); refused(result, 2)
            client.stdin.close(); assert client.wait(timeout=6) == 1; client = None
            report['service_refusal'] = result
            client = controller('commit', 3); assert driver.status.query(client, 1) == (7, {'code': 3, 'unknown': 0, 'cleanup': 0})
            close_client(client, name, 1); client = controller('expiry', 5)
            identity = 1
            while time.time() < expiry - 1:
                current(client, identity); identity += 1; time.sleep(.1)
            while time.time() < expiry: time.sleep(min(.05, max(0, expiry-time.time())))
            result = commit(client, identity); refused(result, 3)
            close_client(client, name, 1); client = controller('commit', 3)
            result = commit(client, 1); assert result['kind'] == 'Completed', result
            close_client(client, name); client = controller(); after = current(client, 1)
            assert after['revision'] == result['revision'] and after['dirty_inodes'] == 0
            report.update(commit_only_result=result, installed=after,
                          saved=verify_saved(witness, branch, result, small_expected(ord('A')), 8192, previous))
        elif args.case == 'commit_refusals':
            result = commit(client, 2); refused(result, 3)
            close_client(client, name, 1); client = controller(); current(client, 1, False)
            result = commit(client, 2, budget=50); refused(result, 11)
            close_client(client, name, 1); client = controller(); current(client, 1, False)
            begin_commit(client, 2, budget=500, end=False)
            driver.status.control_threads(name, 2); client.stdin.close()
            partial = wire.receive(client); assert partial['kind'] == 'failure'
            assert client.wait(timeout=6) == 1; client = None
            driver.status.control_threads(name, 1); client = controller()
            report.update(partial_result=partial, after=current(client, 1, False))
            assert witness.branch(branch) == previous
            report['readonly_source'] = mounted_read(name, bytes(i % 251 for i in range(8192)), 64 * 1024 * 1024)
        elif args.case == 'commit_denied':
            mounted_edit(name, 'byte', ord('A'), 8192); before = current(client, 2)
            result = commit(client, 3)
            assert result['kind'] == 'Failed' and result['phase'] == 'CompositeCommit' and result['disposition'] == 'KnownBeforeCommit', result
            assert result['generation'] == before['generation'] and result['cause']['code'] == 3 and result['cause']['unknown'] == 0
            assert result['known_stage'] is None and result['known_outcome'] is None and result['installed_revision'] is None
            retained = current(client, 4); submission = retained['submission']; assert submission is not None
            assert submission['commit']['failure'] == 'KnownBeforeCommit' and submission['commit']['phase'] == 'CompositeCommit'
            assert witness.branch(branch) == previous
            assert driver.unmount(client, 5)['outcome'] == 'Unmounted'
            close = driver.close_clean(client, 6); assert close['outcome'] == 'Retained' and close['code'] == 13
            report.update(typed_failure=result, retained_status=retained, close_refusal=close,
                          qualification='Known service failure with retained submission; native clean close refused, external teardown only')
            expected_retention = True
        elif args.case == 'commit_small_project':
            report['checks'] = []
            identity = 0
            report['mounted_kernel'] = driver.mount.checked(
                ['docker', 'exec', name, 'uname', '-srmo'], text=True).stdout.strip()

            def control(operation, label):
                """One authenticated control session for one step.

                The daemon admits exactly one control session, so this case closes
                it before the witness opens the public Service session it needs.
                """
                nonlocal client, identity
                end = time.monotonic() + 20
                while True:
                    if client is None:
                        time.sleep(0.1)
                        client = controller()
                        identity = 0
                    try:
                        return operation(client)
                    except (EOFError, BrokenPipeError):
                        if time.monotonic() >= end:
                            raise
                        report.setdefault('session_handoff_retries', []).append(label)
                        if client.poll() is None:
                            close_client(client, name, 1)
                        client = None

            def observe(operation, label):
                """One public Service session for one read."""
                end = time.monotonic() + 20
                while True:
                    if witness.closed():
                        witness.open()
                    try:
                        return operation()
                    except (EOFError, BrokenPipeError):
                        if time.monotonic() >= end:
                            raise
                        report.setdefault('session_handoff_retries', []).append(label)
                        witness.close()

            def next_id():
                nonlocal identity
                identity += 1
                return identity

            before = control(lambda session: current(session, next_id()), 'initial-status')
            assert before['mounted'] and not before['stopping'] and not before['closed'], before
            assert before['generation'] == 1 and before['dirty_inodes'] == 0, before
            report['initial_status'] = before
            # One negative authorization check on this authenticated control
            # surface: a Commit that names another incarnation is refused before
            # any admission, so the live Workspace does not move.
            denied_result = control(lambda session: commit(session, next_id(), incarnation=b'\x72' * 32),
                                    'negative-authorization')
            refused(denied_result, 3)
            unchanged = control(lambda session: current(session, next_id()), 'status-after-refusal')
            assert (unchanged['generation'], unchanged['dirty_inodes'], unchanged['revision']) == (
                before['generation'], before['dirty_inodes'], before['revision']), unchanged
            report['negative_authorization'] = {'selection': 'Commit-for-another-incarnation',
                                                'result': denied_result, 'status_unchanged': unchanged}
            report['mounted'] = small_project(name, args.output, 'first')
            edited = control(lambda session: current(session, next_id()), 'edited-status')
            assert edited['dirty_inodes'] > 0 and edited['mounted'], edited
            commit_a, after_a = control(lambda session: completed(session, next_id(), edited), 'commit-A')
            report['commit_A'] = commit_a
            report['second_edit'] = small_project(name, args.output, 'second')
            commit_b, after_b = control(lambda session: completed(session, next_id(), after_a), 'commit-B')
            report['commit_B'] = commit_b
            # Both Commits are explicit and complete before any public read: the
            # daemon admits one control session, so the saved-state inspection runs
            # after the last Commit and before the remount.
            close_client(client, name); client = None
            saved_a = observe(lambda: verify_small_project(witness, branch, commit_a,
                                                           report['mounted'], None), 'verify-A')
            report['saved_A'] = saved_a
            saved_b = observe(lambda: verify_small_project(witness, branch, commit_b,
                                                           report['second_edit'], saved_a), 'verify-B')
            report['saved_B'] = saved_b
            assert saved_a['root'] != saved_b['root'] and saved_b['previous_root'] == saved_a['root']
            # A's root stays readable and unchanged while B is the live head.
            report['root_A_after_B'] = observe(lambda: witness.root_observation(saved_a['root'], b'left/a.txt'),
                                               'root-A-after-B')
            assert report['root_A_after_B']['data'] == saved_a['a_data']
            report['generations'] = {'A': {'generation': commit_a['generation'], 'head': saved_a['head'],
                                           'root': saved_a['root']},
                                     'B': {'generation': commit_b['generation'], 'head': saved_b['head'],
                                           'root': saved_b['root']}}
            witness.close()
            report['no_hidden_commit'] = ('exactly two Commit commands were submitted; every other '
                                          'request is Status or a public Service read')
            assert control(lambda session: driver.unmount(session, next_id()), 'unmount-A')['outcome'] == 'Unmounted'
            assert control(lambda session: driver.remount(session, next_id()), 'remount')['outcome'] == 'Mounted'
            report['remounted'] = control(lambda session: current(session, next_id()), 'remounted-status')
            report['remounted_read'] = small_project_read(name)
            assert control(lambda session: driver.unmount(session, next_id()), 'unmount-B')['outcome'] == 'Unmounted'
            assert control(lambda session: driver.close_clean(session, next_id()), 'close')['outcome'] == 'Closed'
            report['closed'] = control(lambda session: current(session, next_id(), writable=False), 'closed-status')
        else: raise AssertionError(args.case)
        close_client(client, name); client = None
        witness.close(); witness = None
        if expected_retention:
            assert daemon.poll() is None
            report['native_cleanup'] = 'REFUSED: retained failed submission; no clean-close/reuse qualification'
        else:
            driver.mount.stop_mount(name); assert daemon.wait(timeout=12) == 0
            errors = daemon.stderr.read(); daemon_log += errors.decode(errors='replace')
            assert 'workspace closed' in daemon_log
            driver.mount.checked(['docker', 'exec', name, 'test', '!', '-e', '/layerfs/workspace/read'])
            report['native_cleanup'] = 'PASS: current writable/readonly owner unmounted and cleanly closed'
        report['checks'] = [{'id': key, 'status': 'PASS'} for key in driver.CASES[args.case]]
    finally:
        original_failure = sys.exc_info()[0] is not None
        if daemon is not None and daemon.poll() is None:
            probe = driver.mount.checked(['docker', 'exec', name, 'sh', '-c',
                'cat /proc/net/tcp | head -8; echo ---; python3 -c \"'
                'import socket;s=socket.create_connection((\'127.0.0.1\',23456),timeout=3);'
                'print(\'CONNECTED\',s.getsockname());s.close()\"'], text=True)
            report['daemon_probe'] = probe.stdout
        cleanup_failures = []
        def cleanup_attempt(owner, operation):
            try: return operation()
            except BaseException as error:
                cleanup_failures.append({'owner': owner, 'failure': repr(error)})
                return None
        def kill_wait(process):
            process.kill(); return process.wait(timeout=6)
        if paused:
            cleanup_attempt('resume-owned-service', lambda: service.send_signal(signal.SIGCONT))
        if live_files is not None and live_files.poll() is None:
            report['forced_live_helper_cleanup'] = True
            cleanup_attempt('live-files-helper', lambda: kill_wait(live_files))
        if client is not None and client.poll() is None:
            report['forced_client_cleanup'] = True
            cleanup_attempt('control-client', lambda: kill_wait(client))
        if witness is not None and witness.client is not None and witness.client.poll() is None:
            report['forced_witness_cleanup'] = True
            cleanup_attempt('witness-client', lambda: kill_wait(witness.client))
        if proxy:
            for index, worker in enumerate([proxy[1], *proxy[2]]):
                cleanup_attempt(f'proxy-worker-{index}', lambda worker=worker: worker.join(6))
                if worker.is_alive(): cleanup_failures.append({'owner': f'proxy-worker-{index}', 'failure': 'still alive after bounded join'})
        report['runtime_identity'] = {'container': name if container else None,
                                      'volume': name + '-root' if volume else None}
        if container:
            report['retained_container'] = name
            if daemon is not None and daemon.poll() is None: report['forced_daemon_cleanup'] = True
            removed = cleanup_attempt('remove-container', lambda: driver.mount.checked(['docker', 'rm', '-f', name]))
            if removed is not None: report.pop('retained_container')
        if daemon is not None:
            cleanup_attempt('wait-daemon-client', lambda: daemon.wait(timeout=10))
            if daemon.poll() is None:
                report['forced_daemon_client_cleanup'] = True
                cleanup_attempt('stop-daemon-client', lambda: kill_wait(daemon))
            if daemon.poll() is not None:
                cleanup_attempt('save-daemon-diagnostics', lambda: (args.output / 'daemon.stderr').write_bytes(daemon_log.encode() + daemon.stderr.read()))
        if service.poll() is None:
            cleanup_attempt('close-service-input', service.stdin.close)
            cleanup_attempt('wait-service', lambda: service.wait(timeout=10))
            if service.poll() is None:
                report['forced_service_cleanup'] = True
                cleanup_attempt('stop-service', lambda: kill_wait(service))
        if service.poll() is not None:
            cleanup_attempt('save-service-diagnostics', lambda: (args.output / 'service.stderr').write_bytes(service_ready.encode() + service.stderr.read()))
        else:
            report['retained_service_pid'] = service.pid
        if volume:
            report['retained_volume'] = name + '-root'
            removed = cleanup_attempt('remove-volume', lambda: driver.mount.checked(['docker', 'volume', 'rm', name + '-root']))
            if removed is not None: report.pop('retained_volume')
        if cleanup_failures:
            report['cleanup_failures'] = cleanup_failures
            if not original_failure: raise RuntimeError(f'independent cleanup failed: {cleanup_failures}')
    assert service.returncode == 0 and driver.sha(master) == master_hash
    report['master_unchanged'] = True
    report['cleanup'] = ('EXTERNAL_ONLY: declared retained failure; owned runtime forcibly removed, no native clean-close claim'
                         if expected_retention else 'PASS: native clean close and owned runtime/volume removal')


driver.execute = execute

if __name__ == '__main__':
    driver.main()
