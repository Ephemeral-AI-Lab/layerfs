#!/usr/bin/env python3
"""One native handle-write selection; shared Stage driver owns the actual save observer."""
from pathlib import Path
import stage_route as driver

driver.CASES = {
    'positional': ['positional-overwrite-tail-zero-gap-and-incremental-Commit'],
    'append': ['append-selects-live-EOF-and-concurrent-successes-never-overwrite'],
    'zero_rights': ['zero-write-validates-rights-identity-offset-deadline-without-publication'],
    'envelope': ['Zero-gap-and-local-input-stream-past-the-retired-8MiB-replay-ceiling'],
    'frontier': ['multi-edit-frontier-streams-past-the-retired-256-edit-ceiling'],
    'quota': ['candidate-quota-refusal-keeps-visible-file-and-borrowed-payload'],
    'metadata_failure': ['native-write-metadata-failure-retains-version-input-and-quarantine'],
    'released': ['release-before-write-publication-refuses-without-consuming-borrowed-input'],
    'deadline': ['expired-write-refresh-publishes-no-bytes-length-or-timestamp'],
    'pending': ['pending-open-handles-refuse-nonempty-and-zero-writes'],
    'successor': ['frozen-G-and-live-positional-gap-use-exact-successive-Commit-coordinates'],
    'native_save': ['positional-write-and-append-progress-during-actual-service-save'],
}
driver.REQUIREMENTS = {case: ['W-01', 'W-07', 'W-12', 'B-01', 'B-20', 'S-18'] for case in driver.CASES}
driver.REQUIREMENTS['native_save'] += ['S-03', 'S-11']
driver.TEST_SOURCE = Path(__file__).with_name('write.rs')
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_PREFIX = 'write_'
driver.TEST_MARKER = 'WRITE_CHECK'
driver.MODE = 'functional-workspace-write'
driver.REQUIREMENT_SCOPE = 'Native existing-file handle writes only; no mounted write/coherence/append-position qualification'
driver.LIMIT_CASES = ('metadata_failure',)
driver.NOT_RUN = ['mounted writable kernel profile and syscall fd-position semantics', 'namespace mutations and npm',
                  'R6', 'hard RSS/cgroup bound', 'authenticated daemon edit/Commit controls']

if __name__ == '__main__': driver.main()
