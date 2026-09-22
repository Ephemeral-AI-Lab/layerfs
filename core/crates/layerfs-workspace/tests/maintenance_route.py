#!/usr/bin/env python3
"""One functional healthy-owner maintenance selection through native Workspace APIs."""
from pathlib import Path
import stage_route as driver

driver.CASES = {
    'payload_churn': ['4097-owned-inputs-reuse-bounded-records-without-explicit-reclaim'],
    'mutation_churn': ['96-mutations-and-40-clean-Commits-stay-bounded-without-explicit-reclaim'],
    'cross_workspace': ['consumer-wide-maintenance-releases-dead-peers-and-preserves-reader-pins'],
    'frozen': ['64-successor-mutations-preserve-frozen-G-and-reader-through-Commit'],
    'payload_failure': ['failed-and-partial-payload-owners-stay-accounted-until-explicit-cleanup'],
    'metadata_failure': ['cleanup-failure-before-DFS-is-retained-across-input-and-Commit'],
}
driver.REQUIREMENTS = {case: ['B-01', 'B-20', 'B-26', 'S-15'] for case in driver.CASES}
driver.TEST_SOURCE = Path(__file__).with_name('maintenance.rs')
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_PREFIX = 'maintenance_'
driver.TEST_MARKER = 'MAINTENANCE_CHECK'
driver.MODE = 'functional-workspace-maintenance'
driver.REQUIREMENT_SCOPE = 'Synchronous healthy-owner cleanup SDK subsets; no mounted write or performance claim'
driver.LIMIT_CASES = ()
driver.NOT_RUN = ['handle writes and mounted writable kernel profile', 'namespace mutations and npm',
                  'R6', 'hard RSS/cgroup bound', 'failed-state recovery or automatic retry']

if __name__ == '__main__': driver.main()
