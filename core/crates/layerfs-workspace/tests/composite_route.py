#!/usr/bin/env python3
"""Registered composite Workspace Commit cases through the existing native route."""
from pathlib import Path
import stage_route as driver

driver.CASES = {
    'remote_admission': ['remote-pre-admission-refuses-before-clean-capture'],
    'clean': ['actual-clean-UpToDate-before-and-after-dirty-Commit'],
    'clean_successor': ['clean-capture-preserves-late-D1-and-next-composite-input'],
    'repeated': ['one-composite-command-per-incremental-alias-Commit'],
    'successor': ['composite-capture-reconciles-exact-G-with-live-successor'],
    'native_save': ['composite-progress-during-actual-service-save'],
    'unknown_save': ['composite-prerequisite-loss-retains-G-D1-and-source-outcome'],
    'lost_result': ['composite-terminal-loss-does-not-adopt-later-Branch-observation'],
    'reconcile_failure': ['composite-known-success-survives-local-reconcile-failure'],
    'denied': ['composite-denial-preserves-file-acknowledgements-and-live-state'],
    'head_moved': ['clean-composite-stale-head-is-not-local-UpToDate'],
    'refusals': ['clean-quota-readonly-deadline-and-staged-refusals'],
    'metadata_only': ['composite-metadata-only-save-preserves-content-root'],
    'frontier': ['composite-104-inode-frontier-and-next-generation'],
}
driver.REQUIREMENTS = {
    'remote_admission': ['S-12', 'B-26'],
    'clean': ['H-03', 'H-04'], 'clean_successor': ['S-04', 'S-18', 'H-03'],
    'repeated': ['S-14', 'S-18', 'B-28'], 'successor': ['S-03', 'S-18', 'H-03'],
    'native_save': ['S-03', 'S-11'], 'unknown_save': ['S-15', 'H-07', 'H-09'],
    'lost_result': ['H-07', 'H-09', 'H-10'], 'reconcile_failure': ['H-03', 'B-20', 'B-26'],
    'denied': ['S-15', 'H-07'], 'head_moved': ['H-04', 'H-07'],
    'refusals': ['S-12', 'B-01', 'B-26'], 'metadata_only': ['B-28'],
    'frontier': ['S-18', 'B-21', 'B-26', 'B-28'],
}
driver.TEST_SOURCE = Path(__file__).with_name('composite.rs')
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_PREFIX = 'composite_'
driver.TEST_MARKER = 'COMPOSITE_CHECK'
driver.MODE = 'functional-workspace-composite-commit'
driver.REQUIREMENT_SCOPE = 'Composite Commit SDK subsets; no writable mount, namespace, npm or R6 completion'
driver.LIMIT_CASES = ('reconcile_failure',)
driver.DENIED_COMMIT_CASE = 'denied'
driver.PROXY_CASE = 'lost_result'
driver.NOT_RUN = ['explicit failed-state and DiscardStage disposition', 'mounted writes',
                  'namespace mutations and npm', 'R6', 'hard RSS/cgroup bound',
                  'full read/directory in-flight and capture/prepared-mutation schedules']

if __name__ == '__main__': driver.main()
