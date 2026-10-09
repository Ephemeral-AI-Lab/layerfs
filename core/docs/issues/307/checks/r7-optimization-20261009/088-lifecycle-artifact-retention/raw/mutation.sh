set -euo pipefail
umask 027
python3 - <<'PY'
import os
from pathlib import Path
root = Path('r7-survival')
if os.path.lexists(root):
    raise RuntimeError('proof scope already exists; original input retained')
root.mkdir(mode=0o750)
for index in range(24):
    path = root / ('f%02d.bin' % index)
    with path.open('xb') as stream:
        stream.write(('r7-survival:%02d\n' % index).encode() + bytes(range(256)) * 4)
    path.chmod(0o640)
os.link(root / 'f00.bin', root / 'hardlink.bin')
os.symlink('f00.bin', root / 'symlink')
print('R7 mutation complete: 24 files, one hardlink, one symlink')
PY
