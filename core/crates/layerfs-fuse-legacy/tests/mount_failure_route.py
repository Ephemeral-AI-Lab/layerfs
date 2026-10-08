#!/usr/bin/env python3
"""One real-kernel mount ownership selection, with explicit later cleanup."""
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'layerfs-workspace/tests'))
import stage_route as driver

driver.CASES = {
    'deadline': ['actual-INIT-deadline-retains-mount-until-explicit-cleanup'],
    'worker': ['native-worker-spawn-failure-retains-initialized-session'],
    'session': ['native-mount-syscall-failure-retains-lease-until-checked-absence'],
    'admission': ['pre-admission-deadline-and-authority-refusals-retain-no-owner'],
    'success': ['ordinary-readonly-and-writable-mounts-retain-checked-lifecycle'],
}
driver.REQUIREMENTS = {
    'deadline': ['R-01', 'R-12'],
    'worker': ['R-01', 'R-12'],
    'session': ['R-03', 'R-12'],
    'admission': ['R-03', 'R-11'],
    'success': ['R-01', 'R-05', 'R-07', 'R-09', 'W-01'],
}
driver.TEST_SOURCE = Path(__file__).with_name('mount_failure.rs')
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_PREFIX = 'mount_failure_'
driver.TEST_MARKER = 'MOUNT_FAILURE_CHECK'
driver.MODE = 'functional-native-mount-failure'
driver.REQUIREMENT_SCOPE = 'Actual Linux native mount entry, retained partial owner and later explicit cleanup; admission is a native API subset'
driver.LIMIT_CASES = ()
driver.NOT_RUN = ['remote Mount control', 'namespace mutations and npm', 'R6',
                  'hard RSS/cgroup bound', 'preemption of native mount or unmount syscalls']

if __name__ == '__main__': driver.main()
