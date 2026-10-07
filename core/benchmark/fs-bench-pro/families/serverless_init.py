"""One owner-directed WAL Init decision; reuse the existing operation harness."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time

from families import init_namespace, phase7_sqlite
from shared import cold_native, sqlite_contract, disposable_wal

CONTROL = ('core/docs/issues/307/checks/incumbent-restoration-20261007/raw/'
           'phase7-sqlite-disposable-init-1000-incumbent-restored-v1/candidate/receipt.json')
CONTROL_SHA = 'eedfd8a8a45e8a59fdae59317cee05471f99e2e118b78ffd344ceb007c21e1a8'
CONTROL_SOURCE = '2fced797d14f9d4f6b72c9ad00574976a4dfd5f0'
NONINPUTS = tuple('core/docs/issues/307/' + name for name in (
    'HANDOFF-PRE-S8-SERVERLESS-20261007.md',
    'HANDOFF-S7-S9-RESUME-20261006.md', 'S7-S9-SPEED-TEST-PLAN.md'))


def control(root):
    path = root / CONTROL
    if hashlib.sha256(path.read_bytes()).hexdigest() != CONTROL_SHA:
        raise ValueError('retained memory control custody changed')
    row = json.loads(path.read_text())
    if (row['measured_source_commit'] != CONTROL_SOURCE or
            row['comparison_ns'] != 155_291_459 or row['storage_bytes'] != 20_590_592 or
            row['effective_profile']['journal_mode'] != 'memory' or
            any(row[key] != value for key, value in (
                ('status', 'COMPLETE'), ('verification_status', 'PASS'), ('cache_status', 'PASS'))) or
            row['cleanup']['status'] != 'PASS'):
        raise ValueError('retained control scope or proof mismatch')
    return row


def check_scope(identity):
    """Keep actual Git dirtiness; accept only the owner's named non-input docs."""
    for line in identity['dirty_paths']:
        if not line.startswith('?? ') or line[3:] not in NONINPUTS:
            raise ValueError('freeze tracked product/harness and all other inputs')


def decision(row, reference):
    required = row['status'] == 'COMPLETE' and all(
        row.get(key) == 'PASS' for key in ('cache_status', 'verification_status', 'cleanup_status'))
    if not required:
        return {'status': 'INCOMPLETE', 'import_route': None}
    elapsed, retained = row['comparison_ns'], reference['comparison_ns']
    if row['command_wall_ns'] > row['command_budget_ns'] or row['verification_wall_ns'] > row['verification_budget_ns']:
        return {'status': 'FAIL', 'import_route': None, 'reason': 'frozen command/proof limit'}
    return {
        'status': 'OWNER_ACCEPTED_BASELINE',
        'import_route': 'wal-throughout',
        'historical_conditional_route': 'memory-import-wal-seal-required' if elapsed > retained else 'wal-throughout',
        'design_authority': 'owner WAL-throughout supersession 2026-10-07',
        'speed_gate': 'PASS' if 10 * elapsed <= 11 * retained else 'FAIL',
        'speed_operands_ns': [elapsed, retained], 'speed_formula': '10*candidate<=11*control',
        'time_delta_ns': elapsed - retained,
        'time_delta_percent': 100 * (elapsed - retained) / retained,
        'storage_delta_bytes': row['storage_bytes'] - reference['storage_bytes'],
        'storage_delta_percent': 100 * (row['storage_bytes'] - reference['storage_bytes']) / reference['storage_bytes'],
        'strict_allocation_status': 'NOT_RUN — mechanism removed',
    }


def run(case, output, arm, common):
    matrix = case.id in disposable_wal.ROWS
    if arm != 'candidate' or (case.id != phase7_sqlite.SERVERLESS_WAL_CASE and not matrix):
        raise ValueError('one registered candidate; retained control is never rerun')
    disposable_wal.require_profile(case)
    if platform.system() != 'Darwin':
        raise ValueError('host Init decision requires macOS/system SQLite')
    out = common.owned(output)
    out.mkdir(parents=True, exist_ok=False)
    row = {
        'schema': 'pre-s8-serverless-init-v1', 'family_id': 'pre-s8-init-journal',
        'scenario_id': case.id, 'scenario_version': 1, 'case': case.id, 'arm': arm,
        'source_arm': arm, 'public_api_call_count': None, 'sdk_edit_member_count': None,
        'attempted_operation_count': 0, 'completed_operation_count': 0,
        'failure_class': None, 'projection': 'complete registered fixture',
        'field_availability': {'sdk_edit_member_count': 'N/A — Project Init',
            'phase_peak_bytes': 'UNAVAILABLE — no phase observer',
            'daemon_request_count': 'N/A — native host component operation',
            'fuse_request_count': 'N/A — native host component operation',
            'forbidden_route_count': 'source-sealed vehicle; no independent runtime counter'},
        'mode': 'selected-development-decision', 'admission_eligible': False,
        'status': 'NOT_RUN', 'sample_count': 0, 'verification_status': 'NOT_RUN',
        'cache_status': 'INCOMPLETE', 'cleanup_status': 'NOT_RUN',
        'resource_status': 'UNAVAILABLE — no phase peak/resource gate',
        'command_budget_ns': case.command_budget_ns,
        'verification_budget_ns': case.verification_budget_ns,
        'cache_contract': sqlite_contract.CACHE, 'profile': 'sqlite-wal-off-v2',
        'requested_profile': 'disposable', 'setup': 'fresh Store; reuse closed prepared source',
        'operation_contract_id': 'pre-s8-init-create-import-seal-v1',
        'operation_surface': 'public core Project Init and Persistence lifecycle',
        'operation_entrypoint': 'Handles::create -> layerfs_project::init -> Handles::seal',
        'orchestration_executor': 'existing fs-bench-pro runner.py',
        'mutation_executor': 'macOS release benchmark_init process',
        'implementation_route_status': 'sealed public-call vehicle; no SDK/daemon/FUSE scope',
        'timing_boundary_id': 'fresh-create-through-seal-close', 'clock_id': 'Rust Instant',
        'start_event': 'before Handles::create', 'end_event': 'after seal and checked close',
        'seed_or_repetition': 1, 'expected_performance_rows': 1, 'expected_verifier_rows': 1,
        'construction_workers': 4, 'environment_workers': 1,
        'treatment': 'WAL/OFF, allocation mechanism retired, consuming seal',
        'image_identity': {'status': 'N/A', 'reason': 'native macOS Init'},
        'strict_allocation_status': 'NOT_RUN — mechanism removed',
        'durable_execution': 'NOT_RUN — disabled by owner until explicit reauthorization',
    }
    if matrix:
        row.update(schema='owner-disposable-wal-init-v1', family_id='disposable-wal-matrix',
                   mode='owner-requested-current-source-observation',
                   required_case_ids=list(disposable_wal.ROWS),
                   operation_contract_id='disposable-wal-matrix-init-v1')
    lock = None
    try:
        lock_path = common.RESULTS / 'phase7-sqlite.lock'
        lock_path.parent.mkdir(parents=True, exist_ok=True)
        lock = lock_path.open('a+b')
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        identity = common.identities()
        check_scope(identity)
        row['identity'] = identity
        row['contract_commit'] = identity['source_commit']
        row['harness_identity'] = identity['harness_seal']
        row['measurement_scope_clean'] = True
        row['declared_noninputs'] = {p: common.digest(common.ROOT / p) for p in NONINPUTS if (common.ROOT / p).exists()}
        row['measured_source_commit'] = identity['source_commit']
        row['dependency_seal'] = identity['cargo_lock_sha256']
        row['report_template_sha256'] = common.digest(common.ROOT / 'benchmark_agent_report.md')
        row['report_generator_sha256'] = common.digest(Path(__file__))
        row['competing_work'] = common.competing_work()
        row['host'] = {'platform': platform.platform(), 'architecture': platform.machine()}
        product = list((common.ROOT / 'core/crates').glob('*/src/**/*.rs'))
        product += list((common.ROOT / 'core/crates').glob('*/sql/**/*.sql'))
        product += list((common.ROOT / 'core/crates/layerfs-api').glob('*/src/**/*.rs'))
        row['product_seal'] = common.seal(product)
        row['product_identity'] = row['product_seal']
        compilation = product + list((common.ROOT / 'core/crates').glob('**/Cargo.toml'))
        compilation += list((common.ROOT / 'core/crates').glob('*/examples/**/*.rs'))
        compilation += [common.ROOT / p for p in ('core/Cargo.toml', 'core/Cargo.lock', '.cargo/config.toml')]
        row['compilation_seal'] = common.seal(compilation)
        reference = disposable_wal.reference(case.id) if matrix else control(common.ROOT)
        control_pin = disposable_wal.ROWS[case.id]['reference'] if matrix else {'path': CONTROL, 'sha256': CONTROL_SHA}
        row['control'] = {**control_pin, 'source': reference['measured_source_commit'],
                          'original_case': reference['case'], 'comparison_ns': reference['comparison_ns'],
                          'storage_bytes': reference['storage_bytes'], 'original_status': reference['status']}
        row['build'] = phase7_sqlite.build(common.ROOT, 'candidate', out, common)
        if row['build']['status'] != 'PASS':
            row['status'] = row['build']['status']
            return out
        if common.seal(compilation) != row['compilation_seal']:
            raise ValueError('compilation inputs changed during build')
        row['cold_helper'] = cold_native.build(common.ROOT, out, phase7_sqlite.invoke)
        fixture_case = init_namespace.CASES[case.fixture]
        fixture = init_namespace.prepare(fixture_case, common.RESULTS / 'sdk-prepared')
        row['fixture'] = fixture
        row['workload_identity'] = {'case': fixture['case'], 'seed': fixture['seed']}
        row['fixture_identity'] = fixture['manifest_sha256']
        row['oracle_identity'] = {'manifest': fixture['manifest_sha256'],
            'binary_sha256': row['build']['binaries']['verify_namespace']['sha256']}
        if fixture['manifest_sha256'] != reference['fixture']['manifest_sha256']:
            raise ValueError('new/control fixture mismatch')
        scratch = out / 'scratch'
        scratch.mkdir()
        env = {**os.environ, 'LAYERFS_CONSTRUCTION_WORKERS': '1',
               'LAYERFS_HISTORY_CURSOR_KEY': '28' * 32, 'TMPDIR': str(scratch)}
        db = out / 'store.sqlite'
        binaries = row['build']['binaries']
        claim = common.RESULTS / 'phase7-sqlite-sample-claims' / hashlib.sha256(
            json.dumps([case.id, identity['source_tree'], row['compilation_seal'], fixture['manifest_sha256']]).encode()).hexdigest()
        claim.parent.mkdir(parents=True, exist_ok=True)
        with claim.open('x') as stream:
            stream.write(str(out) + '\n')
        started = time.monotonic_ns()
        row['residency'] = cold_native.attest(fixture['source'], row['cold_helper'], out,
                                            case.command_budget_ns, phase7_sqlite.invoke, common.ROOT)
        row['cache_status'] = row['residency']['status']
        if row['cache_status'] != 'PASS':
            row['status'] = 'INELIGIBLE'
            return out
        remaining = case.command_budget_ns - (time.monotonic_ns() - started)
        if remaining <= 0:
            row['status'] = 'FAIL'
            row['failure_class'] = 'cold attestation exhausted complete command'
            return out
        command = [binaries['benchmark_init']['path'], fixture['source'], str(db), case.id, 'disposable', '30']
        sample = phase7_sqlite.invoke(command, out, 'driver', remaining, env, common.ROOT)
        row.update(performance=sample, sample_count=1, attempted_operation_count=None, completed_operation_count=None)
        row['storage'] = sqlite_contract.allocations([db])
        row['storage_bytes'] = row['storage']['total_bytes']
        row['cleanup_status'] = 'PASS' if not list(scratch.iterdir()) and not any(Path(str(db) + s).exists() for s in ('-wal', '-shm', '-journal')) else 'FAIL'
        row['command_wall_ns'] = time.monotonic_ns() - started
        child = sample['child']
        if sample['exit_code'] != 0 or sample['timed_out'] or not isinstance(child, dict) or child.get('status') != 'COMPLETE':
            row['status'] = 'FAIL'
            return out
        if child.get('public_api_call_count') != 1:
            raise ValueError('expected exactly one public Project Init call')
        row.update(status='COMPLETE', attempted_operation_count=1, completed_operation_count=1,
                   public_api_call_count=child['public_api_call_count'], comparison_ns=child['operation_ns'])
        stage_sum = sum(child[key] for key in ('bootstrap_ns', 'init_ns', 'reclamation_ns', 'close_ns'))
        if stage_sum > child['operation_ns']:
            raise ValueError('lifecycle spans overlap or exceed the operation')
        row['unattributed_lifecycle_ns'] = child['operation_ns'] - stage_sum
        effective = [json.loads(line[len('EFFECTIVE_PROFILE '):]) for line in (out / 'driver.stderr').read_text().splitlines() if line.startswith('EFFECTIVE_PROFILE ')]
        if len(effective) != 1 or effective[0] != {
                'identity': 'sqlite-wal-off-v2', 'journal_mode': 'wal', 'synchronous': 0,
                'foreign_keys': 1, 'fullfsync': 0, 'checkpoint_fullfsync': 1, 'page_size': 4096,
                'cache_size': -2048, 'mmap_size': 0, 'temp_store': 2, 'wal_checkpoint_performed': True}:
            raise ValueError('effective WAL profile/one-file seal mismatch')
        row['effective_profile'] = effective[0]
        proof_command = [binaries['verify_namespace']['path'], str(db), str(db), child['root'],
                         child['stack'], fixture['manifest'], fixture['manifest_sha256'], 'disposable']
        proof = phase7_sqlite.invoke(proof_command, out, 'verifier', case.verification_budget_ns, env, common.ROOT)
        row.update(verification=proof, verification_wall_ns=proof['wall_ns'])
        row['verification_status'] = 'PASS' if proof['exit_code'] == 0 and not proof['timed_out'] and common.lite_verification_pass(proof['child'], fixture_case, child, fixture) and child['root'] == reference['performance']['child']['root'] else 'FAIL'
        row['decision'] = disposable_wal.comparison(row, case.id) if matrix else decision(row, reference)
        return out
    except Exception as error:
        row.update(status='INCOMPLETE', failure_class=type(error).__name__, reason=str(error))
        raise
    finally:
        row.setdefault('decision', {'status': 'INCOMPLETE', 'import_route': None})
        row['performance_status'] = row['status']
        row['row_status'] = row['status']
        row['custody_status'] = 'SCOPED_INPUTS_SEALED' if row.get('measurement_scope_clean') else 'INCOMPLETE'
        row['report_generator_identity'] = row.get('report_generator_sha256')
        row['claim_eligibility_status'] = 'selected development; no release claim'
        row['pairing_and_order_status'] = 'one candidate; retained control; unpaired diagnostic'
        row['cleanup_scope'] = 'original child exited; empty scratch and sidecar-free Store retained'
        row['observed_performance_rows'] = row['sample_count']
        row['observed_verifier_rows'] = int('verification' in row)
        common.write_json(out / 'receipt.json', row)
        common.manifest_run(out)
        if lock is not None:
            lock.close()
