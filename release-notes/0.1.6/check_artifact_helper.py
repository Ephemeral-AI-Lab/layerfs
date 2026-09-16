"""Tiny fixture check for the v0.1.6 artifact helper; writes only under a temp dir.

Run with `python3 release-notes/0.1.6/check_artifact_helper.py`. It builds a
throwaway repository whose workspace version is 0.1.6, exercises the helper's
tag resolution, its six-asset construction, its overwrite refusal and its
checksum validation, and touches nothing in the real checkout.
"""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile

helper = Path(__file__).with_name('prepare_artifacts.py')
spec = importlib.util.spec_from_file_location('artifacts', helper)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
with tempfile.TemporaryDirectory(prefix='layerfs-016-artifact-check-') as directory:
    root = Path(directory)
    repo = root / 'repo'
    repo.mkdir()
    module.REPO = repo

    def git(*args):
        return subprocess.check_output(['git', '-C', str(repo), *args], stderr=subprocess.DEVNULL)

    git('init')
    for scope in module.EVIDENCE:
        path = repo / scope
        if not path.suffix:
            path = path / 'README.md'
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('fixture\n')
    for name, content in [
        ('Cargo.toml', '[workspace.package]\nversion="0.1.6"\n'),
        ('Cargo.lock', 'lock\n'),
        ('LICENSE', 'license\n'),
    ]:
        (repo / name).write_text(content)
    git('add', '.')
    git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-m', 'fixture')
    commit = module.resolve('HEAD', True)
    try:
        module.resolve('HEAD', False)
    except subprocess.CalledProcessError:
        pass
    else:
        raise AssertionError('missing release tag accepted')
    (repo / 'untracked-secret').write_text('must not archive\n')
    output = root / 'candidate'
    sys.argv = [str(helper), str(output), '--ref', commit, '--candidate']
    module.main()
    assert module.validate(output, commit, True)['validation'] == 'PASS'
    assert module.validate(output, commit, True)['assets'] == 6
    try:
        module.main()
    except FileExistsError:
        pass
    else:
        raise AssertionError('existing output overwritten')
    for name in ('LICENSE', 'SHA256SUMS'):
        (output / name).write_text('tampered\n')
        try:
            module.validate(output, commit, True)
        except ValueError:
            pass
        else:
            raise AssertionError(f'corruption accepted: {name}')
        (output / name).write_text('license\n' if name == 'LICENSE' else '')
    git('tag', 'v0.1.6')
    assert module.resolve('refs/tags/v0.1.6', False) == commit
print('artifact helper fixture check: PASS')
