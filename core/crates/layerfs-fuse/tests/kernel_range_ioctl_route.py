#!/usr/bin/env python3
"""One actual Linux mounted projected range insert and Commit selection."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'layerfs-workspace/tests'))
import stage_route as driver

driver.CASES = {'insert_commit': ['mounted-projected-insert-Commit']}
driver.REQUIREMENTS = {'insert_commit': ['W-04', 'W-07', 'W-11', 'S-18']}
driver.TEST_SOURCE = Path(__file__).with_name('kernel_range_ioctl.rs')
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_PREFIX = 'kernel_range_'
driver.TEST_MARKER = 'KERNEL_RANGE_CHECK'
driver.MODE = 'functional-mounted-range-ioctl'
driver.REQUIREMENT_SCOPE = 'Actual Linux STATE/EDIT through projected Workspace and explicit Commit'
driver.NOT_RUN = ['position sweep, independent old-Commit oracle, public SDK Exec, performance/cache admission']

if __name__ == '__main__':
    driver.main()
