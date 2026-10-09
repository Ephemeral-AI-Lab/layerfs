"""Independent read-only full-byte and supported-metadata seal of an owned copy."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import stat
import time

READ_WINDOW = 64 * 1024
EXPECTED_COMMIT = '639ed015397290b3745d163aafe02ffee4aa3f84'
MODULUS = 1 << 256


def checked_roots(source, copied):
    source = Path(source).absolute().resolve(strict=True)
    copied = Path(copied).absolute().resolve(strict=True)
    owned = Path('/tmp').resolve(strict=True)
    if owned not in copied.parents or not copied.name.startswith('layerfs-r7-'):
        raise ValueError('copy must be this stage\'s owned /tmp prepared fixture')
    if source == copied or source in copied.parents or copied in source.parents:
        raise ValueError('source and copy must be disjoint')
    if not stat.S_ISDIR(source.lstat().st_mode) or not stat.S_ISDIR(copied.lstat().st_mode):
        raise ValueError('source and copy must be actual directories')
    return source, copied


def entries(root):
    """Iterative depth-first inventory; only live directory iterators are retained."""
    yield '.', root
    stack = [('', root, os.scandir(root), root.lstat())]
    try:
        while stack:
            prefix, directory, iterator, opened = stack[-1]
            try:
                item = next(iterator)
            except StopIteration:
                iterator.close()
                stack.pop()
                if stability(opened) != stability(directory.lstat()):
                    raise ValueError(f'directory changed during inventory: {directory}')
                continue
            relative = f'{prefix}/{item.name}' if prefix else item.name
            path = directory / item.name
            kind = item.stat(follow_symlinks=False).st_mode
            yield relative, path
            if stat.S_ISDIR(kind):
                # scandir failures propagate; no omitted unreadable subtree.
                stack.append((relative, path, os.scandir(path), path.lstat()))
    finally:
        for _, _, iterator, _ in stack:
            iterator.close()


def metadata(info):
    kind = ('directory' if stat.S_ISDIR(info.st_mode) else
            'file' if stat.S_ISREG(info.st_mode) else
            'symlink' if stat.S_ISLNK(info.st_mode) else 'unsupported')
    return dict(kind=kind, mode=stat.S_IMODE(info.st_mode), uid=info.st_uid,
                gid=info.st_gid, mtime_ns=info.st_mtime_ns)


def stability(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_uid, info.st_gid,
            info.st_size, info.st_mtime_ns, info.st_ctime_ns, info.st_nlink)


def outside_output(path, source, copied):
    path = Path(path).absolute()
    resolved = path.parent.resolve(strict=True) / path.name
    if any(resolved == root or root in resolved.parents for root in (source, copied)):
        raise ValueError('evidence must be outside both roots')
    protected = Path('/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-installed-56560-full-native-handoff')
    if resolved == protected or protected in resolved.parents:
        raise ValueError('protected evidence path')
    return path


def within_path(root, relative):
    path = root if relative == '.' else root / relative
    # Symlink targets are opaque supported state. Only their parents must
    # remain inside the root; never follow a leaf to compare its target.
    parent = path.parent.resolve(strict=True) if relative != '.' else root
    if parent != root and root not in parent.parents:
        raise ValueError(f'parent symlink escapes declared root: {relative}')
    return path


def bounded_text(path):
    with path.open('rb') as stream:
        value = stream.read(READ_WINDOW + 1)
    if len(value) > READ_WINDOW:
        raise ValueError('Git control record exceeds declared bounded window')
    return value.decode('utf-8').strip()


def git_head(root):
    """Read actual Git control files; no process, cwd change, or Git invocation."""
    git = root / '.git'
    info = git.lstat()
    if stat.S_ISREG(info.st_mode):
        pointer = bounded_text(git)
        if not pointer.startswith('gitdir: '):
            raise ValueError('invalid .git pointer')
        git = (root / pointer[8:]).resolve(strict=True)
    elif not stat.S_ISDIR(info.st_mode):
        raise ValueError('.git must be a directory or declared gitdir file')
    def control_path(path):
        actual = path.resolve(strict=True)
        if actual != root and root not in actual.parents:
            raise ValueError('external Git control path is outside the sealed fixture scope')
        return actual
    head = bounded_text(control_path(git / 'HEAD'))
    if not head.startswith('ref: '):
        value = head
    else:
        reference = head[5:]
        parts = Path(reference).parts
        if not reference.startswith('refs/') or '..' in parts or Path(reference).is_absolute():
            raise ValueError('invalid symbolic HEAD reference')
        common = git
        try:
            shared = bounded_text(control_path(git / 'commondir'))
        except FileNotFoundError:
            shared = None
        if shared is not None:
            common = (git / shared).resolve(strict=True)
        try:
            value = bounded_text(control_path(common / reference))
        except FileNotFoundError:
            value = None
            with control_path(common / 'packed-refs').open('rb') as stream:
                while True:
                    line = stream.readline(READ_WINDOW + 1)
                    if not line:
                        break
                    if len(line) > READ_WINDOW:
                        raise ValueError('packed reference exceeds bounded record window')
                    if line.startswith((b'#', b'^')):
                        continue
                    pair = line.decode('utf-8').strip().split(' ', 1)
                    if len(pair) == 2 and pair[1] == reference:
                        if value is not None:
                            raise ValueError('duplicate packed HEAD reference')
                        value = pair[0]
            if value is None:
                raise ValueError('symbolic HEAD absent from declared Git records')
    if len(value) not in (40, 64) or any(c not in '0123456789abcdef' for c in value):
        raise ValueError('invalid actual Git HEAD identity')
    return value


def hash_file(path, expected):
    fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or stability(before) != stability(expected):
            raise ValueError(f'opened file differs from inventory identity: {path}')
        digest = hashlib.sha256()
        total = calls = 0
        while True:
            block = os.read(fd, READ_WINDOW)
            calls += 1
            if not block:
                break
            digest.update(block)
            total += len(block)
            del block
        after = os.fstat(fd)
        named = path.lstat()
        if stability(before) != stability(after) or stability(after) != stability(named) or total != before.st_size:
            raise ValueError(f'file changed during complete byte seal: {path}')
        return dict(sha256=digest.hexdigest(), bytes=total, read_calls=calls)
    finally:
        os.close(fd)


class SetDigest:
    """Order-independent fixed-state fingerprint over uniquely named path rows."""
    def __init__(self):
        self.count = self.total = self.xor = 0

    def add(self, row):
        framed = json.dumps(row, sort_keys=True, separators=(',', ':')).encode()
        value = int.from_bytes(hashlib.sha256(b'r7-fixture-row-v1\0' + framed).digest(), 'big')
        self.count += 1
        self.total = (self.total + value) % MODULUS
        self.xor ^= value

    def seal(self):
        return hashlib.sha256(b'r7-fixture-set-v1\0' + str(self.count).encode() + b'\0' +
                              self.total.to_bytes(32, 'big') + self.xor.to_bytes(32, 'big')).hexdigest()


class Aliases:
    """Lazy indexed oracle evidence; the zero-alias fixture creates no database."""
    def __init__(self, path):
        self.path = path
        self.connection = None
        self.mismatches = self.rows = 0

    def open(self):
        if self.connection is None:
            fd = os.open(self.path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC, 0o600)
            os.close(fd)
            self.connection = sqlite3.connect(self.path, isolation_level=None, timeout=0)
            self.connection.execute('PRAGMA journal_mode=MEMORY')
            self.connection.execute('PRAGMA synchronous=OFF')
            self.connection.execute('PRAGMA locking_mode=EXCLUSIVE')
            self.connection.execute('PRAGMA mmap_size=0')
            self.connection.execute('PRAGMA cache_size=-1024')
            self.connection.execute('PRAGMA busy_timeout=0')
            self.connection.execute('''CREATE TABLE groups (
                source_device TEXT, source_inode TEXT, copy_device TEXT,
                copy_inode TEXT, members INTEGER, copy_links INTEGER,
                first_path TEXT, PRIMARY KEY(source_device, source_inode),
                UNIQUE(copy_device, copy_inode))''')

    def observe(self, relative, source, copied):
        if source.st_nlink == 1 and copied.st_nlink == 1:
            return True
        self.open()
        key = str(source.st_dev), str(source.st_ino)
        expected = str(copied.st_dev), str(copied.st_ino)
        original = self.connection.execute('SELECT copy_device,copy_inode FROM groups WHERE source_device=? AND source_inode=?', key).fetchone()
        if original is not None:
            if original != expected:
                self.mismatches += 1
                return False
            self.connection.execute('UPDATE groups SET members=members+1, first_path=min(first_path,?) WHERE source_device=? AND source_inode=?', (relative, *key))
        else:
            other = self.connection.execute('SELECT source_device,source_inode FROM groups WHERE copy_device=? AND copy_inode=?', expected).fetchone()
            if other is not None:
                self.mismatches += 1
                return False
            self.connection.execute('INSERT INTO groups VALUES(?,?,?,?,1,?,?)', (*key, *expected, copied.st_nlink, relative))
        self.rows += 1
        return True

    def finish(self):
        result = dict(created=self.connection is not None, path=str(self.path),
                      observed_alias_candidate_paths=self.rows, mismatches=self.mismatches,
                      scope='equivalence classes within the roots; no copy external hardlinks')
        if self.connection is not None:
            try:
                groups = self.connection.execute('SELECT count(*) FROM groups').fetchone()[0]
                bad_links = self.connection.execute('SELECT count(*) FROM groups WHERE members!=copy_links').fetchone()[0]
                result.update(groups=groups, incomplete_copy_link_classes=bad_links,
                              sqlite_version=sqlite3.sqlite_version)
                result['mismatches'] += bad_links
            finally:
                self.connection.close()
            info = self.path.stat()
            result.update(logical_bytes=info.st_size, allocated_bytes=info.st_blocks * 512)
        return result


def seal(source, copied, output, expected_commit, alias_index=None):
    source, copied = checked_roots(source, copied)
    output = outside_output(output, source, copied)
    alias_index = outside_output(alias_index or Path(str(output) + '.aliases.sqlite'), source, copied)
    start = time.monotonic_ns()
    counts = dict(source_entries=0, copy_entries=0, files=0, directories=0, symlinks=0,
                  regular_bytes=0, source_bytes_read=0, copy_bytes_read=0,
                  mismatched_entries=0, unexpected_copy_entries=0)
    raw_digest = hashlib.sha256()
    source_set, copy_set = SetDigest(), SetDigest()
    aliases = Aliases(alias_index)
    original_root, copied_root = source.lstat(), copied.lstat()
    source_head, copy_head = git_head(source), git_head(copied)
    with output.open('xb') as sink:
        def emit(row):
            raw = (json.dumps(row, sort_keys=True) + '\n').encode()
            sink.write(raw)
            raw_digest.update(raw)
        emit(dict(schema='r7-fixture-seal-rows-v1', source=str(source), copy=str(copied),
                  expected_commit=expected_commit, source_head=source_head, copy_head=copy_head,
                  git_invocations=0, metadata_fields=['kind', 'mode', 'uid', 'gid', 'mtime_ns'],
                  read_window_bytes=READ_WINDOW))
        for relative, path in entries(source):
            original = path.lstat()
            source_info = metadata(original)
            counts['source_entries'] += 1
            kind = source_info['kind']
            if kind == 'unsupported':
                raise ValueError(f'unsupported required fixture kind: {path}')
            counts[dict(file='files', directory='directories', symlink='symlinks')[kind]] += 1
            counterpart = within_path(copied, relative)
            try:
                actual = counterpart.lstat()
            except FileNotFoundError:
                actual = None
            differences = []
            copy_info = metadata(actual) if actual is not None else None
            if copy_info != source_info:
                differences.append('supported_metadata_or_missing_path')
            if kind == 'file':
                content = hash_file(path, original)
                source_info.update(size=original.st_size, content_sha256=content['sha256'])
                counts['regular_bytes'] += original.st_size
                counts['source_bytes_read'] += content['bytes']
                if actual is not None and stat.S_ISREG(actual.st_mode):
                    copied_content = hash_file(counterpart, actual)
                    copy_info.update(size=actual.st_size, content_sha256=copied_content['sha256'])
                    counts['copy_bytes_read'] += copied_content['bytes']
                    if source_info != copy_info:
                        differences.append('complete_regular_bytes_or_metadata')
                    if (original.st_dev, original.st_ino) == (actual.st_dev, actual.st_ino):
                        differences.append('copy_is_linked_to_source')
                    if not aliases.observe(relative, original, actual):
                        differences.append('hardlink_equivalence')
                else:
                    differences.append('missing_regular_copy')
            elif kind == 'symlink':
                source_info['target_hex'] = os.fsencode(os.readlink(path)).hex()
                if actual is not None and stat.S_ISLNK(actual.st_mode):
                    copy_info['target_hex'] = os.fsencode(os.readlink(counterpart)).hex()
                    if copy_info['target_hex'] != source_info['target_hex']:
                        differences.append('symlink_target')
                else:
                    differences.append('missing_symlink_copy')
                if stability(original) != stability(path.lstat()):
                    raise ValueError(f'symlink changed during seal: {path}')
                if actual is not None and stability(actual) != stability(counterpart.lstat()):
                    raise ValueError(f'copy symlink changed during seal: {counterpart}')
            source_set.add(dict(path=relative, **source_info))
            if copy_info is not None:
                copy_set.add(dict(path=relative, **copy_info))
            counts['mismatched_entries'] += bool(differences)
            emit(dict(path=relative, source=source_info, copy=copy_info,
                      match=not differences, differences=differences))
        for relative, path in entries(copied):
            counts['copy_entries'] += 1
            counterpart = within_path(source, relative)
            try:
                counterpart.lstat()
            except FileNotFoundError:
                counts['unexpected_copy_entries'] += 1
                emit(dict(path=relative, match=False, differences=['unexpected_copy_path'], copy=metadata(path.lstat())))
        if stability(original_root) != stability(source.lstat()) or stability(copied_root) != stability(copied.lstat()):
            raise ValueError('root metadata changed during complete seal')
        if git_head(source) != source_head or git_head(copied) != copy_head:
            raise ValueError('actual Git HEAD changed during complete seal')
        alias_result = aliases.finish()
        status = 'PASS' if (source_head == copy_head == expected_commit and
                            counts['source_entries'] == counts['copy_entries'] and
                            not counts['mismatched_entries'] and not counts['unexpected_copy_entries'] and
                            not alias_result['mismatches'] and source_set.seal() == copy_set.seal()) else 'FAILED_FIXTURE_SEAL'
        emit(dict(event='completed', status=status, counts=counts, aliases=alias_result))
    return dict(schema='r7-fixture-seal-v1', status=status, source=str(source), copy=str(copied),
                source_head=source_head, copy_head=copy_head, expected_commit=expected_commit,
                git_invocations=0, source_written=False, copy_written=False,
                full_regular_content=status == 'PASS', name_inventory_complete=True,
                names_match=counts['source_entries'] == counts['copy_entries'] and not counts['unexpected_copy_entries'],
                root_included=True, counts=counts, aliases=alias_result,
                source_content_metadata_set_sha256=source_set.seal(),
                copy_content_metadata_set_sha256=copy_set.seal(),
                inventory=dict(path=str(output), sha256=raw_digest.hexdigest(),
                               entry_rows=counts['source_entries']+counts['unexpected_copy_entries'], complete=True),
                verification_ns=time.monotonic_ns()-start, read_window_bytes=READ_WINDOW,
                resource_scope='independent fixture verification, outside all product/sample timers',
                native_snapshot=False, source_quiescence_required=True,
                actual_git_head_file_observations_per_root=2,
                digest_rows_retained=0, traversal_state='one scandir iterator per depth',
                admission_eligible=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--copy', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--expected-commit', default=EXPECTED_COMMIT)
    parser.add_argument('--alias-index', type=Path)
    args = parser.parse_args()
    try:
        result = seal(args.source, args.copy, args.output, args.expected_commit, args.alias_index)
    except (OSError, ValueError, sqlite3.Error) as error:
        print(json.dumps(dict(schema='r7-fixture-seal-v1', status='INCOMPLETE',
                              original_error=str(error), errno=getattr(error, 'errno', None),
                              inventory_path=str(args.output), source_written=False, copy_written=False,
                              git_invocations=0, admission_eligible=False), sort_keys=True))
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0 if result['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
