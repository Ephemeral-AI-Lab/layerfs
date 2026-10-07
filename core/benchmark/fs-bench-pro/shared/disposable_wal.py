"""Explicit WAL selection and hash-bound historical expectations; no new baseline."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
REGISTRY = ROOT / 'core/benchmark/fs-bench-pro/registry/disposable-wal-matrix-v1.json'
ROWS = {row['id']: row for row in json.loads(REGISTRY.read_text())['cases']}
PROFILE = 'sqlite-wal-off-v2'
PINS_KIND = 'retained-phase4.5-receipt-root-pins-v1'


def register(case_type):
    return {name: case_type(name, row['fixture'], row['states'], row['storage_ceiling'],
                           row['command_budget_ns'], row['verification_budget_ns'],
                           'disposable', 'all-state-structure-five-anchor-bounded-content-v1',
                           row['pack_layout'], 'owner-disposable-wal-matrix-v1')
            for name, row in ROWS.items()}


def require_profile(case):
    if case.profile != 'disposable':
        raise ValueError('Durable execution disabled by owner until explicit reauthorization')


def reference(case_id, role='reference'):
    pin = ROWS[case_id][role]
    path = ROOT / pin['path']
    if hashlib.sha256(path.read_bytes()).hexdigest() != pin['sha256']:
        raise ValueError('retained receipt hash mismatch')
    row = json.loads(path.read_text())
    if (row['status'] != 'COMPLETE' or row['cache_status'] != 'PASS' or
            row['verification_status'] != 'PASS' or row['cleanup']['status'] != 'PASS' or
            row['sample_count'] != 1 or
            row['command_wall_ns'] > row['command_budget_ns'] or
            row['verification_wall_ns'] > row['verification_budget_ns']):
        raise ValueError('retained observation is not complete/cold/verified/budgeted')
    return row


def root_pins(case_id):
    """Use recorded independent producer roots, not missing old census files."""
    row = reference(case_id)
    child, proof = row['performance']['child'], row['verification']['child']
    n = ROWS[case_id]['states']
    if (n is None or row['arm'] != 'baseline' or
            row['measured_source_commit'] != '7edddbdb8e8512627aed0ed42533ef099d802384' or
            child['profile_identity'] != 'phase4.5-memory-off' or
            child['status'] != 'COMPLETE' or child['states'] != n or
            child['selected_states'] != n or len(child['roots']) != n or
            proof['status'] != 'CHECKED' or proof['states'] != n or
            proof['custody_states'] != n):
        raise ValueError('retained independent root expectation scope mismatch')
    return {'kind': PINS_KIND, 'source_commit': row['measured_source_commit'],
            'row': row['workload_row'], 'roots': child['roots'],
            'retained_receipt': ROWS[case_id]['reference'],
            'historical_identity': row['identity'],
            'scope': 'archived independent producer roots and recorded native proof; '
                     'old separate census/pin files unavailable; current proof reruns'}


def validate_pins(pins, case_id):
    if pins != root_pins(case_id):
        raise ValueError('retained root pins differ from frozen historical receipt')


def comparison(row, case_id):
    ref = reference(case_id)
    result = {'kind': 'historical-unpaired-comparison', 'admission_eligible': False,
              'reference': ROWS[case_id]['reference'],
              'reference_source': ref['measured_source_commit'],
              'reference_profile': ref['profile'],
              'reference_ns': ref['comparison_ns'],
              'reference_storage_bytes': ref['storage_bytes'],
              'speed_formula': '10*candidate_ns<=11*historical_reference_ns',
              'strict_allocation_status': 'NOT_RUN — mechanism removed'}
    if row.get('comparison_ns') is not None:
        elapsed = row['comparison_ns']
        result.update(time_delta_ns=elapsed-ref['comparison_ns'],
                      time_delta_percent=100*(elapsed-ref['comparison_ns'])/ref['comparison_ns'],
                      historical_speed_arithmetic='PASS' if 10*elapsed <= 11*ref['comparison_ns'] else 'FAIL')
    if row.get('storage_bytes') is not None:
        result['storage_delta_bytes'] = row['storage_bytes']-ref['storage_bytes']
        limit = ROWS[case_id]['storage_ceiling']
        result['allocation_ceiling'] = limit
        if limit is not None:
            result['allocation_gate'] = 'PASS' if row['storage_bytes'] <= limit else 'FAIL'
    if 'incumbent' in ROWS[case_id]:
        incumbent = reference(case_id, 'incumbent')
        result['incumbent'] = {**ROWS[case_id]['incumbent'], 'comparison_ns': incumbent['comparison_ns'],
                               'storage_bytes': incumbent['storage_bytes']}
    return result
