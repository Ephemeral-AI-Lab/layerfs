from pathlib import Path
import hashlib
import io
import shutil
import tarfile
from tempfile import TemporaryDirectory
import unittest
import tomllib

from check_fuser_integrity import (OFFICIAL_GIT, config_errors, lock_errors,
                                   manifest_errors, package_errors, patched_package_errors,
                                   PATCH_PACKAGE, PATCH_RECORD, PATCH_MANIFESTS)

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

    def test_only_exact_owner_authorized_manifest_patch_is_allowed(self):
        path = PATCH_MANIFESTS['core/Cargo.toml']
        doc = {'patch': {'crates-io': {'fuser': {'version': '=0.18.0', 'path': path}}}}
        self.assertFalse(manifest_errors(doc, patch_path=path))
        self.assertTrue(manifest_errors(doc))
        for key, value in [('path', '../elsewhere'), ('version', '0.18')]:
            changed = {'patch': {'crates-io': {'fuser': dict(doc['patch']['crates-io']['fuser'])}}}
            changed['patch']['crates-io']['fuser'][key] = value
            self.assertTrue(manifest_errors(changed, patch_path=path))
        self.assertTrue(manifest_errors({}, patch_path=path))

    def test_patch_lock_requires_owning_context_exact_version_and_no_false_checksum(self):
        item = {'name': 'fuser', 'version': '0.18.0'}
        self.assertFalse(lock_errors({'package': [item]}, patched=True))
        for key, value in [('version', '0.18.1'), ('checksum', '1' * 64)]:
            self.assertTrue(lock_errors({'package': [item | {key: value}]}, patched=True))
        registry = item | {'source': 'registry+https://github.com/rust-lang/crates.io-index', 'checksum': '1' * 64}
        self.assertTrue(lock_errors({'package': [registry]}, patched=True))

    def test_authorized_package_rejects_unrelated_edits_inventory_changes_and_redirects(self):
        repository = Path(__file__).resolve().parents[2]
        with TemporaryDirectory() as directory:
            root = Path(directory)
            shutil.copytree(repository / PATCH_RECORD, root / PATCH_RECORD)
            shutil.copytree(repository / PATCH_PACKAGE, root / PATCH_PACKAGE)
            self.assertFalse(patched_package_errors(root))
            for name in ['src/time.rs', 'src/session.rs']:
                path = root / PATCH_PACKAGE / name
                original = path.read_bytes()
                path.write_bytes(original + b'// unauthorized edit\n')
                self.assertTrue(patched_package_errors(root))
                path.write_bytes(original)
            path = root / PATCH_RECORD / 'provenance.json'
            original = path.read_bytes()
            path.write_bytes(original + b'\n')
            self.assertTrue(patched_package_errors(root))
            path.write_bytes(original)
            injected = root / PATCH_PACKAGE / 'src/extra.rs'
            injected.write_text('extra')
            self.assertTrue(patched_package_errors(root))
            injected.unlink()
            injected.symlink_to(root / PATCH_PACKAGE / 'src/time.rs')
            self.assertTrue(patched_package_errors(root))

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
