from pathlib import Path
import hashlib
import io
import tarfile
from tempfile import TemporaryDirectory
import unittest
import tomllib

from check_fuser_integrity import (OFFICIAL_GIT, config_errors, lock_errors,
                                   manifest_errors, package_errors)

REV = "e48279fab0ddbe4a6e22aefad3cbfdf796d0b0b7"


class FuserIntegrityTests(unittest.TestCase):
    def test_official_registry_dependency_is_allowed(self):
        self.assertFalse(manifest_errors(tomllib.loads('[dependencies]\nfuser="=0.18.0"')))

    def test_direct_and_renamed_patch_are_rejected(self):
        for dependency in ('fuser={path="../fuser"}', 'alias={package="fuser",git="https://example.com/fuser"}'):
            self.assertTrue(manifest_errors(tomllib.loads('[patch.crates-io]\n' + dependency)))

    def test_replace_is_rejected(self):
        self.assertTrue(manifest_errors(tomllib.loads('[replace]\n"fuser:0.18.0"={path="../fuser"}')))

    def test_nested_target_path_override_is_rejected(self):
        self.assertTrue(manifest_errors(tomllib.loads('[target."cfg(unix)".dependencies]\nalias={package="fuser",path="../fuser"}')))

    def test_git_requires_explicit_approved_immutable_official_revision(self):
        doc = tomllib.loads(f'[dependencies]\nfuser={{git="{OFFICIAL_GIT}",rev="{REV}"}}')
        self.assertTrue(manifest_errors(doc))
        self.assertFalse(manifest_errors(doc, (REV,)))
        for git, rev in [('https://github.com/other/fuser.git', REV), (OFFICIAL_GIT, 'main')]:
            self.assertTrue(manifest_errors({'dependencies': {'fuser': {'git': git, 'rev': rev}}}, (REV,)))

    def test_branch_and_custom_registry_are_rejected(self):
        self.assertTrue(manifest_errors({'dependencies': {'fuser': {'git': OFFICIAL_GIT, 'rev': REV, 'branch': 'master'}}}, (REV,)))
        self.assertTrue(manifest_errors({'dependencies': {'fuser': {'registry': 'custom', 'version': '0.18'}}}))

    def test_local_lock_and_unapproved_git_lock_are_rejected(self):
        self.assertTrue(lock_errors({'package': [{'name': 'fuser', 'version': '0.18.0'}]}))
        source = f'git+{OFFICIAL_GIT}?rev={REV}#{REV}'
        self.assertTrue(lock_errors({'package': [{'name': 'fuser', 'source': source}]}))
        self.assertFalse(lock_errors({'package': [{'name': 'fuser', 'source': source}]}, (REV,)))

    def test_registry_lock_requires_checksum(self):
        item = {'name': 'fuser', 'source': 'registry+https://github.com/rust-lang/crates.io-index'}
        self.assertTrue(lock_errors({'package': [item]}))
        item['checksum'] = '1' * 64
        self.assertFalse(lock_errors({'package': [item]}))

    def test_cargo_directory_and_path_overrides_are_rejected(self):
        self.assertTrue(config_errors({'paths': ['../fuser']}))
        self.assertTrue(config_errors({'source': {'vendor': {'directory': '../vendor'}}}))
        self.assertFalse(config_errors({'target': {'aarch64': {'rustflags': ['--cfg', 'aes_armv8']}}}))

    def test_archive_verification_catches_modified_and_extra_source(self):
        with TemporaryDirectory() as root:
            root = Path(root)
            source = root / 'fuser-0.18.0'
            (source / 'src').mkdir(parents=True)
            path = source / 'src/time.rs'
            path.write_bytes(b'original source\n')
            archive = root / 'fuser.crate'
            with tarfile.open(archive, 'w:gz') as file:
                info = tarfile.TarInfo('fuser-0.18.0/src/time.rs')
                info.size = len(path.read_bytes())
                file.addfile(info, io.BytesIO(path.read_bytes()))
            checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
            self.assertFalse(package_errors(archive, source, checksum))
            path.write_bytes(b'modified\n')
            self.assertTrue(package_errors(archive, source, checksum))
            path.write_bytes(b'original source\n')
            (source / 'src/injected.rs').write_text('extra')
            self.assertTrue(package_errors(archive, source, checksum))
            self.assertTrue(package_errors(archive, source, '0' * 64))
