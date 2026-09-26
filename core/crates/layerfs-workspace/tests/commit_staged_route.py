#!/usr/bin/env python3
"""Registered native CommitStaged cases using the existing Workspace route driver."""
from pathlib import Path
import stage_route as driver

driver.CASES = {
    'remote_admission': ['remote-admission-refusal-leaves-stage-usable-without-replay'],
    'repeated': ['same-Workspace-A-B-A-commits-preserve-handles-and-incremental-input', 'known-commit-clean-close'],
    'successor': ['late-D1-reconciliation-keeps-exact-G-coordinates-and-old-reply'],
    'mounted_successor': ['mounted-three-generation-convergence-keeps-old-heads-and-ordered-writes'],
    'selectors': ['foreign-expired-dropped-and-consumed-selectors-never-replay'],
    'denied': ['actual-commit-denial-retains-stage-and-local-state'],
    'lost_result': ['native-lost-commit-result-stays-unknown-despite-later-observation'],
    'reconcile_failure': ['known-C5-success-retries-local-reconciliation-without-replay'],
    'consumed_stage': ['consumed-token-and-identical-root-do-not-prove-own-success'],
    'head_moved': ['Branch-move-preserves-exact-losing-stage'],
    'cycles': ['repeated-commit-frontiers-and-eligible-root-cleanup-stay-bounded'],
    'headroom': ['pre-reserved-reconciliation-progress-with-ordinary-quota-occupied'],
    'frontier': ['streamed-104-inode-reconciliation-and-next-generation-save'],
}
driver.REQUIREMENTS = {
    'remote_admission': ['S-12', 'B-26'],
    'repeated': ['S-14', 'S-18', 'H-03', 'B-28'],
    'successor': ['S-04', 'S-05', 'S-16', 'S-18', 'H-03'],
    'mounted_successor': ['S-04', 'S-05', 'S-16', 'S-18', 'H-03'],
    'selectors': ['S-12', 'H-06'],
    'denied': ['S-15', 'H-07'],
    'lost_result': ['H-07', 'H-09', 'H-10'],
    'reconcile_failure': ['H-03', 'B-20', 'B-26'],
    'consumed_stage': ['H-06', 'H-07'],
    'head_moved': ['H-05', 'H-07'],
    'cycles': ['S-14', 'B-21', 'B-23', 'B-29'],
    'headroom': ['B-26'],
    'frontier': ['S-18', 'B-21', 'B-26', 'B-28'],
}
driver.TEST_SOURCE = Path(__file__).with_name('commit_staged.rs')
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_PREFIX = 'commit_'
driver.TEST_MARKER = 'COMMIT_CHECK'
driver.MODE = 'functional-workspace-commit-staged'
driver.REQUIREMENT_SCOPE = 'CommitStaged and repeated local Stage subsets; no full mounted Pair 1 completion'
driver.LIMIT_CASES = ('reconcile_failure',)
driver.DENIED_COMMIT_CASE = 'denied'
driver.PROXY_CASE = 'lost_result'
driver.NOT_RUN = ['actual UpToDate: current public mutations always assign mtime and clean Stage is refused',
                  'composite Commit and explicit failed-state disposition',
                  'namespace mutations and npm', 'R6', 'hard RSS/cgroup bound',
                  'full read/directory in-flight and capture/prepared-mutation schedules']

if __name__ == '__main__': driver.main()
