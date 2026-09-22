"""External decoder for public Commit/status DTOs; production Client validates the wire."""
import history_route as route

COMMIT_PHASES = {1: 'Preparing', 2: 'CommitStaged', 3: 'CompositeCommit', 4: 'Reconcile', 5: 'Complete'}
DISPOSITIONS = {1: 'KnownBeforeCommit', 2: 'Unknown', 3: 'KnownCommitLocalFailure'}
STAGE_PHASES = {1: 'Captured', 2: 'FileSave', 3: 'MetadataSave', 4: 'StageChanges',
                5: 'Staged', 6: 'Failed', 7: 'LocalBookkeeping'}
STAGE_FAILURES = {1: 'KnownBeforeStage', 2: 'Unknown', 3: 'KnownStageLocalFailure'}


def jsonable(value):
    if isinstance(value, bytes): return value.hex()
    if isinstance(value, dict): return {key: jsonable(item) for key, item in value.items()}
    if isinstance(value, (tuple, list)): return [jsonable(item) for item in value]
    return value


def optional(reader, read):
    tag = reader.u8(); assert tag in (0, 1), tag
    return read(reader) if tag else None


def enum(reader, values, optional_value=False):
    tag = reader.u8()
    if optional_value and tag == 0: return None
    return values[tag]


def outcome(reader):
    tag = reader.u8(); assert tag in (0, 1), tag
    if tag == 0: return {'kind': 'Committed', **jsonable(route.commit_record(reader))}
    return {'kind': 'UpToDate', 'head': jsonable(optional(reader, lambda r: r.take(33))),
            'root': reader.take(32).hex()}


def contextual_failure(data):
    r = route.Reader(data)
    failure = {'code': r.u8(), 'unknown': r.u8(), 'cleanup': r.u8()}
    assert r.u8() == 1
    conflict = r.u8()
    if conflict == 0: failure['conflict'] = None
    elif conflict == 1:
        failure['conflict'] = {'kind': 'BranchMoved',
            'expected_head': optional(r, lambda d: d.take(33)), 'actual_head': optional(r, lambda d: d.take(33)),
            'expected_base': r.take(33), 'actual_base': r.take(33)}
    elif conflict == 2: failure['conflict'] = {'kind': 'StackMoved', 'expected': r.take(33), 'actual': r.take(33)}
    elif conflict == 3: failure['conflict'] = {'kind': 'StageChanged', 'expected': r.u64(), 'actual': optional(r, lambda d: d.u64())}
    elif conflict == 4: failure['conflict'] = {'kind': 'BaseMismatch', 'commit_base': r.take(33), 'branch_base': r.take(33)}
    else: raise AssertionError(conflict)
    stage = r.u8()
    if stage == 0: failure['stage'] = {'kind': 'Unobserved'}
    elif stage == 1: failure['stage'] = {'kind': 'Absent', 'workspace': r.take(32)}
    elif stage in (2, 3):
        failure['stage'] = {'kind': 'Retained' if stage == 2 else 'AcknowledgedUnknown',
                            'record': route.stage_record(r)}
    else: raise AssertionError(stage)
    r.done()
    return jsonable(failure)


def receive(client, workspace=b'read', incarnation=b'\x71' * 32):
    kind, body = route.receive(client, timeout=11)
    if kind == 7:
        assert len(body) == 3
        return {'kind': 'failure', 'code': body[0], 'unknown': body[1], 'cleanup': body[2]}
    assert kind == 6
    r = route.Reader(body); assert r.u8() == 17
    result = {'workspace': r.blob().decode(), 'incarnation': r.take(32).hex()}
    assert result['workspace'] == workspace.decode() and result['incarnation'] == incarnation.hex()
    tag = r.u8(); assert tag in (0, 1), tag
    result['kind'] = 'Completed' if tag == 0 else 'Failed'
    result['generation'] = r.u64()
    if tag == 0:
        result.update(stage_token=optional(r, lambda d: d.u64()), outcome=outcome(r), revision=r.u64())
    else:
        result.update(phase=enum(r, COMMIT_PHASES), disposition=enum(r, DISPOSITIONS), cause=contextual_failure(r.blob()))
        result.update(known_stage=optional(r, route.stage_record), observed_stage=optional(r, route.stage_record),
                      known_outcome=optional(r, outcome), observed_outcome=optional(r, outcome),
                      installed_revision=optional(r, lambda d: d.u64()))
    r.done()
    return jsonable(result)


def writable_fields(r):
    result = {'generation': r.u64(), 'revision': r.u64(), 'dirty_inodes': r.u64()}
    def commit_status(d):
        return {'phase': enum(d, COMMIT_PHASES), 'known_root': optional(d, lambda v: v.take(32)),
                'known_head': optional(d, lambda v: v.take(33)), 'installed_revision': optional(d, lambda v: v.u64()),
                'failure': enum(d, DISPOSITIONS, True)}
    def submission(d):
        return {'generation': d.u64(), 'captured_revision': d.u64(), 'dirty_inodes': d.u64(),
                'phase': enum(d, STAGE_PHASES), 'inode': optional(d, lambda v: v.u64()),
                'saved_files': d.u16(), 'saved_metadata': d.u16(), 'stage_token': optional(d, lambda v: v.u64()),
                'candidate_root': optional(d, lambda v: v.take(32)), 'failure': enum(d, STAGE_FAILURES, True),
                'failure_phase': enum(d, STAGE_PHASES, True), 'commit': optional(d, commit_status)}
    result['submission'] = optional(r, submission)
    return jsonable(result)
