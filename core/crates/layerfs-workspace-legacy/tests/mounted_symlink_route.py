#!/usr/bin/env python3
"""Four mounted SYMLINK selections through the unchanged CPU2 native driver."""
from pathlib import Path
import mkdir_route as driver

driver.CASES = {
    'kernel': 'actual-kernel-symlinks-and-native4096-explicit-kernel-readlink-boundary',
    'visibility_successor': 'SDK-empty-kernel-readlink-stable-directory-and-G-D1-symlink-Commits',
    'permit': 'single-use-projected-symlink-held-reply-exclusion-no-notifier-and-ref-cleanup',
    'notification_failure': 'entry-notifier-failure-retains-symlink-target-drops-ref-and-remount-repairs',
}
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_SOURCE = Path(__file__).with_name('mounted_symlink.rs')
driver.TEST_PREFIX = 'mounted_symlink_'
driver.TEST_MARKER = 'MOUNTED_SYMLINK_CHECK'
driver.MODE = 'functional-mounted-workspace-symlink'
driver.NOT_RUN = ['successful kernel creation with empty or 4096-byte target (Linux preflight refusal tested)',
                  'unprivileged DAC enforcement', 'kernel reply-send failure injection',
                  'unlink/rename', 'native create capacity failure remains open',
                  'prepared npm workload', 'R6/RSS/performance', 'crash/restart recovery',
                  'in-place failed-coherence repair']

if __name__ == '__main__':
    driver.main()
