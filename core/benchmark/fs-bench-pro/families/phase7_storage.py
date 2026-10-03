"""Owner-directed direct-engine cluster 1 selections; no SDK receipt relabelling."""
from dataclasses import dataclass
from pathlib import Path
import fcntl
import hashlib
import json
import os
import shutil
import subprocess
import time

from families import init_namespace as init
from shared import phase7 as contract

BASE = '7edddbdb8e8512627aed0ed42533ef099d802384'


@dataclass(frozen=True)
class Case:
    id: str
    fixture: str | None
    states: int | None
    storage_ceiling: int | None
    command_budget_ns: int
    verification_budget_ns: int


CASES = {case.id: case for case in (
    *(Case(f'phase7-init-{n}-direct-engines-v2', fixture.id, None, None, 15_000_000_000, 9_500_000_000)
      for n, fixture in zip((100, 1000, 10000, 100000), init.CASES.values())),
    Case('phase7-history-stride10-direct-engines-v1', None, 17, 54_278_964, 60_000_000_000, 10_000_000_000),
    Case('phase7-history-stride3-direct-engines-v1', None, 53, 70_427_034, 170_000_000_000, 20_000_000_000),
    Case('phase7-history-stride1-direct-engines-v1', None, 157, 92_342_273, 170_000_000_000, 30_000_000_000),
)}


def invoke(command, folder, label, budget_ns, environment, cwd):
    started = time.monotonic_ns()
    with (folder / f'{label}.stdout').open('xb') as out, (folder / f'{label}.stderr').open('xb') as err:
        try:
            child = subprocess.run(command, cwd=cwd, env=environment, stdout=out, stderr=err,
                                   timeout=budget_ns/1e9)
            code, timeout = child.returncode, False
        except subprocess.TimeoutExpired:
            code, timeout = None, True
    wall = time.monotonic_ns()-started
    try:
        data = json.loads((folder / f'{label}.stdout').read_text())
    except (ValueError, UnicodeError):
        data = None
    return {'wall_ns': wall, 'exit_code': code, 'timed_out': timeout, 'child': data,
            'command': command, 'budget_ns': budget_ns}


def build(common, root, package, folder):
    target = root / 'core/target'
    command = ['cargo', '+1.85.1', 'build', '--release', '--locked', '--manifest-path', 'core/Cargo.toml',
               '-p', package]
    if package == 'layerfs-sdk':
        command += ['-p', 'layerfs-server']
    names = ['benchmark_init','verify_namespace'] + (['prepare_storage'] if package == 'layerfs-project' else [])
    for name in names:
        command += ['--example',name]
    # Each arm's target and binary archive stay within that arm's worktree.
    record = invoke(command, folder, 'build', 30_000_000_000,
                    {**os.environ, 'CARGO_TARGET_DIR': str(target)}, root)
    record['status'] = 'PASS' if record['exit_code'] == 0 else 'BUILD_SLOW' if record['timed_out'] else 'FAIL'
    record['profile'] = 'release'
    record['dependency_reuse'] = 'worktree-local locked incremental target'
    if record['status'] == 'PASS':
        record['binaries'] = {}
        for name in names:
            binary = target / 'release/examples' / name
            sha = common.digest(binary)
            archive = root/'benchmark-results/fs-bench-pro/binary-archive'/sha/name
            archive.parent.mkdir(parents=True,exist_ok=True)
            if not archive.exists():
                shutil.copy2(binary,archive); archive.chmod(0o555)
            record['binaries'][name] = {'path': str(archive), 'sha256': sha}
    common.write_json(folder/'build.json', record)
    return record


def run(selection, output, arm, baseline_root, common):
    case = CASES[selection]
    if case.fixture is None:
        raise ValueError('history port driver not yet frozen; no sample authorized')
    root = common.ROOT if arm == 'candidate' else Path(baseline_root).resolve()
    if arm == 'baseline':
        if not root.is_relative_to(common.ROOT / 'target/phase7-baseline'):
            raise ValueError('baseline requires dedicated owned nested worktree')
        head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
        dirty = subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True)
        if head != BASE or dirty:
            raise ValueError('baseline product is not the clean pinned tree')
    out = common.owned(output)
    out.mkdir(parents=True)
    locks = [common.RESULTS / 'phase7.lock']
    if arm == 'baseline':
        locks.append(root / 'target/phase7.lock')
    handles = []
    try:
        for path in locks:
            path.parent.mkdir(parents=True, exist_ok=True)
            handle = path.open('a+b'); handles.append(handle)
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        identity = common.identities()
        if identity['source_dirty']:
            raise ValueError('candidate/harness source must be committed before freeze')
        identity['measured_source_commit'] = subprocess.check_output(['git','rev-parse','HEAD'], cwd=root,text=True).strip()
        identity['measured_source_tree'] = subprocess.check_output(['git','rev-parse','HEAD^{tree}'], cwd=root,text=True).strip()
        identity['root_cargo_config_sha256'] = common.digest(root / '.cargo/config.toml')
        identity['dependency_seal'] = common.digest(root / 'core/Cargo.lock')
        def scope_seal(paths):
            value = hashlib.sha256()
            for path in sorted(paths):
                value.update(str(path.relative_to(root)).encode()+b"\0")
                value.update(path.read_bytes())
            return value.hexdigest()
        crates = root/'core/crates'
        product = [p for p in crates.rglob('*.rs') if 'src' in p.relative_to(crates).parts]
        sql = list(crates.rglob('*.sql'))
        product += sql
        identity['product_seal'] = scope_seal(product)
        identity['shipped_sql_seal'] = scope_seal(sql)
        inputs = product + list(crates.rglob('Cargo.toml'))
        inputs += [p for p in crates.rglob('*.rs') if 'examples' in p.relative_to(crates).parts]
        inputs += [root/'core/Cargo.toml', root/'core/Cargo.lock', root/'.cargo/config.toml']
        identity['compilation_seal'] = scope_seal(inputs)
        record = {'schema': 'phase7-cluster1-v1', 'case': case.id, 'arm': arm, 'identity': identity,
                  'sample_count': 0, 'status': 'NOT_RUN', 'cache_contract': contract.CACHE,
                  'paired_candidate_tree': identity['source_tree'],
                  'cache_status': 'INCOMPLETE', 'verification_status': 'NOT_RUN',
                  'command_budget_ns': case.command_budget_ns, 'verification_budget_ns': case.verification_budget_ns,
                  'storage_ceiling': case.storage_ceiling, 'storage_gap': 'Init has no numeric storage ceiling',
                  'route': 'layerfs-project::init -> PostgreSQL/MinIO' if arm == 'candidate' else 'ProjectApi::init -> retained host Service/SQLite',
                  'timer': 'includes engine open/validation/connections; empty schema bootstrap in setup' if arm == 'candidate' else 'raw ProjectApi::init; Server::create in command wall',
                  'construction_workers': 4, 'environment_construction_workers': 1,
                  'competing_work': common.competing_work()}
        common.write_json(out/'receipt.json', record)
        package = 'layerfs-project' if arm == 'candidate' else 'layerfs-sdk'
        binaries = build(common, root, package, out)
        record['build'] = binaries
        if binaries['status'] != 'PASS':
            record['reason'] = 'release build failed or exceeded prospectively declared 30 s build bound'
            common.write_json(out/'receipt.json', record)
            return out
        fixture_case = init.CASES[case.fixture]
        fixture = init.prepare(fixture_case, common.RESULTS/'sdk-prepared')
        record['fixture'] = fixture
        # Both arms get identical fresh unused service volumes, then the same input contract.
        contract.services.down()
        record['services'] = contract.services.up()
        record['fresh'] = contract.empty_services()
        record['machine'] = json.loads(contract.services.docker('info','--format','{{json .}}').stdout)
        record['machine'] = {key:record['machine'].get(key) for key in ('NCPU','MemTotal','Architecture','OSType','KernelVersion','OperatingSystem')}
        if arm == 'candidate':
            setup = invoke([binaries['binaries']['prepare_storage']['path']],out,'schema-setup',30_000_000_000,
                           {**os.environ,**contract.services.load()['environment']},root)
            record['schema_setup'] = setup
            if setup['exit_code'] != 0 or setup['timed_out'] or setup['child'].get('status') != 'PASS':
                raise ValueError('empty-schema bootstrap failed')
            contract.services.postgres_sql('CHECKPOINT;')
        record['residency'] = contract.dewarm_tree(fixture['source'])
        record['cache_status'] = record['residency']['status']
        if record['cache_status'] != 'PASS':
            record['status'] = 'INELIGIBLE'
            record['reason'] = 'whole input still resident; sample not entered'
            common.write_json(out/'receipt.json', record)
            return out
        environment = {**os.environ, **contract.services.load()['environment'],
                       'LAYERFS_CONSTRUCTION_WORKERS': '1', 'LAYERFS_HISTORY_CURSOR_KEY': os.urandom(32).hex(),
                       'LAYERFS_PRIVATE_KEY': os.urandom(32).hex()}
        driver = binaries['binaries']['benchmark_init']['path']
        store, history = out/'store.sqlite', out/'history.sqlite'
        scratch = out/'scratch'; scratch.mkdir()
        command = ([driver, fixture['source'], str(scratch), os.urandom(16).hex(), os.urandom(32).hex(), case.id]
                   if arm == 'candidate' else [driver, fixture['source'], str(store), str(history), case.id])
        # Persistent exclusive claim prevents another sample of this arm/identity.
        key = hashlib.sha256(json.dumps([case.id,arm,identity['source_tree'],identity['measured_source_tree'],identity['harness_seal'],fixture['manifest_sha256']],sort_keys=True).encode()).hexdigest()
        claim = common.RESULTS/'phase7-sample-claims'/key
        claim.parent.mkdir(parents=True,exist_ok=True)
        with claim.open('x') as claimed:
            claimed.write(str(out)+'\n')
        # No input access after residency: invoke the measured child immediately.
        sample = invoke(command, out, 'driver', case.command_budget_ns, environment, root)
        record['sample_count'] = 1
        record['performance'] = sample
        child = sample['child']
        record['command_wall_ns'] = sample['wall_ns']
        if sample['exit_code'] != 0 or sample['timed_out'] or not isinstance(child,dict) or child.get('status') not in ('PASS','COMPLETE'):
            record['status'] = 'FAIL'
        else:
            record['status'] = 'COMPLETE'
            record['comparison_ns'] = child.get('operation_ns')
            stack = child['stack'] if arm == 'candidate' else child['stack_body']
            verifier = [binaries['binaries']['verify_namespace']['path'], str(store), str(history),
                        child['root'], stack, fixture['manifest'], fixture['manifest_sha256']]
            proof = invoke(verifier, out, 'verifier', case.verification_budget_ns, environment, root)
            record['verification'] = proof
            record['verification_wall_ns'] = proof['wall_ns']
            record['verification_status'] = ('PASS' if proof['exit_code'] == 0 and not proof['timed_out'] and
                common.lite_verification_pass(proof['child'],fixture_case,child,fixture) else 'FAIL')
            if arm == 'candidate':
                record['storage'] = contract.collect()
                record['storage_bytes'] = record['storage']['total_bytes']
            else:
                record['storage_bytes'] = sum(path.stat().st_blocks*512 for path in (store,history))
        record['cleanup'] = {'status':'PASS' if not list(scratch.iterdir()) else 'FAIL',
                             'scope':'child exited and operation-owned ordering scratch empty; evidence retained'}
        common.write_json(out/'receipt.json', record)
        common.write_json(out/'run.json', {'schema':'phase7-cluster1-v1','rows':[{'case':case.id,'arm':arm}]})
        common.manifest_run(out)
        return out
    finally:
        for handle in reversed(handles):
            handle.close()
