#!/usr/bin/env python3
"""Run focused native-service projected RangeEdit tests; no speed claim."""
from pathlib import Path
import stage_route as driver

driver.CASES = {
    'semantics': ['semantics'],
    'notification_failure': ['notification_failure'],
}
driver.REQUIREMENTS = {case: ['S-15'] for case in driver.CASES}
driver.TEST_SOURCE = Path(__file__).with_name('projected_range.rs')
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_PREFIX = 'projected_range_'
driver.TEST_MARKER = 'PROJECTED_RANGE_CHECK'
driver.MODE = 'functional-projected-range-workspace'
driver.REQUIREMENT_SCOPE = 'Workspace projected handle and receipt, without Linux ioctl'
driver.LIMIT_CASES = ()
driver.DATA_MODES = {}
driver.NOT_RUN = ['Linux mounted ioctl ABI', 'public SDK Exec and Commit', 'latency or cache admission']

if __name__ == '__main__':
    driver.main()
