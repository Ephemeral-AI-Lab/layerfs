"""Bounded per-file observations from an inode-ordered sealed input manifest."""
import hashlib
import json
import os
from pathlib import Path
import stat
import time

MANIFEST_SCHEMA = 'r7-file-manifest-v1'
STREAM_SCHEMA = 'r7-residency-stream-v1'
RECORD_BYTES = 64 * 1024
IDENTITY_FIELDS = ('device', 'inode', 'logical_bytes', 'allocated_bytes', 'mtime_ns', 'ctime_ns')


def pinned_identity(info):
    return dict(device=info.st_dev, inode=info.st_ino, logical_bytes=info.st_size,
                allocated_bytes=info.st_blocks * 512, mtime_ns=info.st_mtime_ns,
                ctime_ns=info.st_ctime_ns)


class RootedScope:
    """One root descriptor and bounded component opens; no parent symlink escape."""
    def __init__(self, root, validate_path):
        self.root = validate_path(root).resolve(strict=True)
        self.fd = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW)
        self.initial = os.fstat(self.fd)

    def relative(self, path):
        if not isinstance(path, str) or not path or '\0' in path or '..' in path.split('/'):
            raise ValueError('manifest path must be a nonempty path without parent traversal')
        candidate = Path(path)
        relative = candidate.relative_to(self.root) if candidate.is_absolute() else candidate
        if not relative.parts:
            raise ValueError('manifest file cannot be the root directory')
        return relative

    def open(self, path):
        relative = self.relative(str(path))
        parent = os.dup(self.fd)
        try:
            for name in relative.parts[:-1]:
                opened = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW,
                                 dir_fd=parent)
                os.close(parent)
                parent = opened
            leaf = relative.parts[-1]
            file_fd = os.open(leaf, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK,
                              dir_fd=parent)
            return file_fd, parent, leaf
        except BaseException:
            os.close(parent)
            raise

    def check_root(self):
        current = os.stat(self.root, follow_symlinks=False)
        held = os.fstat(self.fd)
        if (held.st_dev, held.st_ino) != (self.initial.st_dev, self.initial.st_ino) or (
                current.st_dev, current.st_ino) != (held.st_dev, held.st_ino):
            raise ValueError('declared root identity changed during inventory')

    def close(self):
        os.close(self.fd)


class ManifestFailure(RuntimeError):
    def __init__(self, cause, receipt):
        super().__init__(str(cause))
        self.cause = cause
        self.receipt = receipt
        self.errno = getattr(cause, 'errno', None)


def read_record(stream):
    raw = stream.readline(RECORD_BYTES + 1)
    if not raw:
        return None
    if len(raw) > RECORD_BYTES or not raw.endswith(b'\n'):
        raise ValueError('manifest row exceeds bounded record window or lacks newline')
    value = json.loads(raw)
    if not isinstance(value, dict):
        raise ValueError('manifest rows must be JSON objects')
    return value


def seal(stream, expected):
    initial = os.fstat(stream.fileno())
    if not stat.S_ISREG(initial.st_mode):
        raise ValueError('sealed manifest must be a regular file')
    digest = hashlib.sha256()
    for block in iter(lambda: stream.read(RECORD_BYTES), b''):
        digest.update(block)
    observed = digest.hexdigest()
    if observed != expected:
        raise ValueError('manifest SHA-256 differs from prospective seal')
    stream.seek(0)
    return initial, observed


def alias_observation(scope, path, canonical):
    fd, parent, leaf = scope.open(path)
    try:
        before = os.fstat(fd)
        after = os.stat(leaf, dir_fd=parent, follow_symlinks=False)
        if not stat.S_ISREG(before.st_mode) or pinned_identity(before) != pinned_identity(after):
            raise ValueError('alias identity changed during observation')
        if pinned_identity(before) != {key: canonical[key] for key in IDENTITY_FIELDS}:
            raise ValueError('declared alias does not match previous attested physical file')
        return dict(path=str(path), present=True, **pinned_identity(after),
                    observation_kind='declared_alias', alias_of=canonical['path'],
                    resident_pages=canonical['resident_pages'], included_in_aggregate=False,
                    eviction_hint_attempts=0, payload_bytes_read=0)
    finally:
        os.close(fd)
        os.close(parent)


def attest_manifest(manifest, expected_sha, root, output, *, inspect_file, validate_path,
                    evict=False, allow_aliases=False):
    """Hash first, then stream one input/observation at a time into fresh evidence."""
    root = validate_path(root).resolve(strict=True)
    output = validate_path(output)
    resolved_output = output.parent.resolve(strict=True) / output.name
    if resolved_output == root or root in resolved_output.parents:
        raise ValueError('raw inventory output must be outside the measured root')
    manifest = validate_path(manifest)
    scope = RootedScope(root, validate_path)
    rows = physical = aliases = pages = total_pages = hints = 0
    digest = hashlib.sha256()
    manifest_sha = None
    declared_rows = declared_physical = None
    inventory_created = False

    def receipt(complete):
        return dict(schema=STREAM_SCHEMA, files=None,
                    inventory=dict(path=str(output), sha256=digest.hexdigest() if inventory_created else None, rows=rows,
                                   created=inventory_created,
                                   complete=complete, physical_files=physical, declared_aliases=aliases),
                    input_manifest=dict(path=str(manifest), sha256=manifest_sha,
                                        expected_sha256=expected_sha,
                                        declared_files=declared_rows, declared_physical_files=declared_physical,
                                        root=str(root)),
                    resident_pages=pages, total_pages=total_pages, eviction_hint_attempts=hints,
                    method='one per-physical-file DONTNEED hint then bounded mincore' if evict
                           else 'bounded per-physical-file mincore without eviction',
                    scope='all declared regular paths; aliases explicit; content pages only',
                    observed_monotonic_ns=time.monotonic_ns(), payload_bytes_read=0,
                    observer_record_window_bytes=RECORD_BYTES, live_input_rows=1,
                    live_observation_rows=1, retained_previous_identity_rows=1)

    try:
        with manifest.open('rb') as source:
            initial, manifest_sha = seal(source, expected_sha)
            header = read_record(source)
            if not header or header.get('schema') != MANIFEST_SCHEMA or header.get('order') != 'device-inode-path':
                raise ValueError('manifest header/schema/order mismatch')
            if not isinstance(header.get('root'), str) or any(
                    type(header.get(name)) is not int for name in ('root_device', 'root_inode')):
                raise ValueError('manifest exact actual root identity fields required')
            if Path(header['root']).absolute() != root or (
                    header.get('root_device'), header.get('root_inode')) != (scope.initial.st_dev, scope.initial.st_ino):
                raise ValueError('manifest actual root identity mismatch')
            declared_rows = header.get('regular_files')
            declared_physical = header.get('physical_files')
            if type(declared_rows) is not int or type(declared_physical) is not int or not (
                    0 <= declared_physical <= declared_rows):
                raise ValueError('manifest cardinalities required')
            # One record at a time; only the previous physical identity is retained.
            previous_key = previous_inode = canonical = None
            with output.open('xb') as sink:
                inventory_created = True
                while True:
                    item = read_record(source)
                    if item is None:
                        break
                    relative = scope.relative(item.get('path'))
                    path = root / relative
                    validate_path(path)
                    if any(type(item.get(key)) is not int for key in IDENTITY_FIELDS):
                        raise ValueError('manifest exact identity pins required')
                    inode = item['device'], item['inode']
                    key = *inode, relative.as_posix()
                    if previous_key is not None and key <= previous_key:
                        raise ValueError('manifest keys must be strictly inode/path ordered')
                    if inode == previous_inode:
                        if not allow_aliases or item.get('alias_of') != str(Path(canonical['path']).relative_to(root)):
                            raise ValueError('duplicate inode requires explicit adjacent alias declaration')
                        row = alias_observation(scope, path, canonical)
                        aliases += 1
                    else:
                        if item.get('alias_of') is not None:
                            raise ValueError('alias declaration has no adjacent physical observation')
                        row = inspect_file(path, evict, open_scope=scope,
                                           expected_identity={name: item[name] for name in IDENTITY_FIELDS})
                        row.update(observation_kind='physical_file', included_in_aggregate=True)
                        canonical = row
                        physical += 1
                        pages += row['resident_pages']
                        total_pages += row['total_pages']
                        hints += row['eviction_hint_attempts']
                    if {name: row[name] for name in IDENTITY_FIELDS} != {name: item[name] for name in IDENTITY_FIELDS}:
                        raise ValueError('observed file identity differs from sealed manifest')
                    raw = (json.dumps(row, sort_keys=True) + '\n').encode()
                    sink.write(raw)
                    digest.update(raw)
                    rows += 1
                    previous_key, previous_inode = key, inode
                    del item, relative, path, row, raw
                if rows != declared_rows or physical != declared_physical:
                    raise ValueError('manifest row cardinality mismatch')
                # Verify the manifest itself remained the exact sealed file.
                after = os.fstat(source.fileno())
                if pinned_identity(initial) != pinned_identity(after):
                    raise ValueError('sealed manifest identity changed during attestation')
                scope.check_root()
        return receipt(True)
    except (OSError, ValueError, RuntimeError, AttributeError, TypeError) as error:
        raise ManifestFailure(error, receipt(False)) from error
    finally:
        scope.close()
