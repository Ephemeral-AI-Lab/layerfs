#!/usr/bin/env python3
"""One retained symlink backing-read failure; existing Stage teardown is external only."""
from pathlib import Path
import stage_route as driver

driver.CASES = {'backing_failure': ['typed-symlink-backing-read-preserves-Unknown-and-retained-custody']}
driver.REQUIREMENTS = {'backing_failure': []}
driver.TEST_SOURCE = Path(__file__).with_name('symlink.rs')
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_PREFIX = 'symlink_'
driver.TEST_MARKER = 'SYMLINK_CHECK'
driver.MODE = 'functional-native-symlink-backing-failure-external-only'
driver.REQUIREMENT_SCOPE = 'Typed target read failure and retained Unknown; cleanup is external teardown only, with no clean Workspace close claim'
driver.LIMIT_CASES = ()
driver.NOT_RUN = ['native clean Workspace close or failed-owner recovery', 'mounted/kernel SYMLINK',
                  'remote Service save failure', 'prepared npm workload', 'R6/RSS/performance']

if __name__ == '__main__':
    driver.main()
