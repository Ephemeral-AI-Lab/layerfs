#!/usr/bin/env python3
"""Mounted kernel/SDK mkdir through the unchanged native fixture/runtime harness."""
from pathlib import Path
import mkdir_route as driver

driver.CASES = {
    'kernel': 'actual-kernel-nested-mkdir-mode-umask-EEXIST-and-explicit-Commit',
    'visibility': 'mounted-SDK-kernel-visibility-negative-lookup-and-stable-old-directory-handle',
    'permit': 'projection-reply-excludes-mkdir-and-permit-is-used-once-without-notification',
    'notification_failure': 'entry-notification-failure-retains-name-no-ref-leak-and-checked-remount-repairs',
}
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_SOURCE = Path(__file__).with_name('mounted_mkdir.rs')
driver.TEST_PREFIX = 'mounted_mkdir_'
driver.TEST_MARKER = 'MOUNTED_MKDIR_CHECK'
driver.MODE = 'functional-mounted-workspace-mkdir'
driver.NOT_RUN = ['create/unlink/rename/symlink', 'prepared npm workload', 'R6',
                  'hard RSS/cgroup memory bound', 'crash/restart recovery',
                  'in-place failed-coherence repair']

if __name__ == '__main__':
    driver.main()
