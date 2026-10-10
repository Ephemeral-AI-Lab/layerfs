set -euo pipefail
python3 - <<'PY'
import hashlib
import json
import os
from pathlib import Path
import stat
root = Path('r7-survival')
uid, gid = 501, 20
expected = {'f%02d.bin' % index for index in range(24)} | {'hardlink.bin', 'symlink'}
assert set(os.listdir(root)) == expected, 'exact scoped names differ'
info = root.lstat()
assert stat.S_ISDIR(info.st_mode) and stat.S_IMODE(info.st_mode) == 0o750
assert (info.st_uid, info.st_gid) == (uid, gid), 'directory owner differs'
digest = hashlib.sha256()
identities = {}
for index in range(24):
    name = 'f%02d.bin' % index
    path = root / name
    info = path.lstat()
    assert stat.S_ISREG(info.st_mode) and stat.S_IMODE(info.st_mode) == 0o640
    assert (info.st_uid, info.st_gid) == (uid, gid), 'file owner differs'
    expected_bytes = ('r7-survival:%02d\n' % index).encode() + bytes(range(256)) * 4
    with path.open('rb') as stream:
        actual = stream.read(65536)
        assert not stream.read(1), 'unexpected excess payload'
    assert actual == expected_bytes and info.st_size == len(expected_bytes), name
    assert info.st_nlink == (2 if index == 0 else 1), 'link count differs'
    identities[name] = (info.st_dev, info.st_ino)
    digest.update(name.encode() + b'\0' + hashlib.sha256(actual).digest())
alias = (root / 'hardlink.bin').lstat()
assert stat.S_ISREG(alias.st_mode) and stat.S_IMODE(alias.st_mode) == 0o640
assert (alias.st_uid, alias.st_gid) == (uid, gid) and alias.st_nlink == 2
assert (alias.st_dev, alias.st_ino) == identities['f00.bin'], 'hardlink identity differs'
assert len(set(identities.values())) == 24, 'independent files alias'
link = (root / 'symlink').lstat()
assert stat.S_ISLNK(link.st_mode) and stat.S_IMODE(link.st_mode) == 0o777
assert (link.st_uid, link.st_gid) == (uid, gid) and link.st_nlink == 1
assert os.readlink(root / 'symlink') == 'f00.bin', 'symlink target differs'
with (root / 'hardlink.bin').open('rb') as stream:
    assert stream.read(65536) == b'r7-survival:00\n' + bytes(range(256)) * 4
    assert not stream.read(1)
print(json.dumps({'schema': 'r7-lifecycle-scoped-oracle-v1', 'status': 'PASS',
                  'scope': 'r7-survival', 'regular_files': 24, 'names': 26,
                  'hardlink_aliases': 1, 'symlinks': 1, 'uid': uid, 'gid': gid,
                  'payload_set_sha256': digest.hexdigest()}, sort_keys=True))
PY
