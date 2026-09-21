#!/usr/bin/env python3
"""Portable file-open rights and atomic truncation through native Workspace APIs."""
from pathlib import Path
import stage_route as driver

driver.CASES = {
    'permissions': ['nonroot-owner-DAC-applies-to-open-access-and-truncation'],
    'modes': ['portable-open-rights-legacy-readonly-and-unsupported-options'],
    'capacity': ['full-handle-table-never-truncates-and-successful-open-does'],
    'last_slot': ['last-handle-slot-admits-one-truncating-open-without-id-reuse'],
    'metadata_failure': ['failed-truncating-open-releases-pending-handle-not-backing-custody'],
    'forget': ['pending-truncating-open-pins-node-across-forget-and-Commit'],
    'deadline': ['expired-truncate-preparation-publishes-neither-handle-nor-length'],
    'successor': ['truncating-open-during-save-keeps-G-and-commits-D1-empty-file'],
}
driver.REQUIREMENTS = {
    'permissions': ['W-01', 'W-02', 'W-12'],
    'modes': ['W-01', 'W-11', 'W-12'], 'capacity': ['W-02', 'W-12', 'B-01'],
    'last_slot': ['W-02', 'B-01'], 'metadata_failure': ['W-02', 'B-20'],
    'forget': ['W-02', 'S-15'], 'deadline': ['W-02', 'W-12'],
    'successor': ['S-03', 'S-18', 'W-02'],
}
driver.TEST_SOURCE = Path(__file__).with_name('open.rs')
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_PREFIX = 'open_'
driver.TEST_MARKER = 'OPEN_CHECK'
driver.MODE = 'functional-workspace-open'
driver.REQUIREMENT_SCOPE = 'Portable open/atomic truncation SDK subsets; no mounted write or append-positioning claim'
driver.LIMIT_CASES = ('metadata_failure',)
driver.DATA_MODES = {'permissions': 0o444}
driver.CALLER_USERS = {'permissions': '1001:1001'}
driver.NOT_RUN = ['handle writes and atomic append positioning', 'writable kernel coherence/mapping profile',
                  'mounted writes/truncate/extend', 'namespace mutations and npm', 'R6',
                  'hard RSS/cgroup bound', 'explicit failed-state disposition',
                  'metadata-prepared-open vs capture race']

if __name__ == '__main__': driver.main()
