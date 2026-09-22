#!/usr/bin/env python3
"""Mounted CREATE through the unchanged CPU2/native fixture and cleanup driver."""
from pathlib import Path
import argparse
import mkdir_route as driver

driver.CASES = {
    'kernel': 'actual-created-fd-empty-restrictive-mode-umask-access-append-resize-and-Commit',
    'existing': 'kernel-existing-excl-wrongkind-readonly-flags-and-projected-existing-create',
    'visibility': 'SDK-create-negative-lookup-parent-attrs-and-stable-kernel-directory-handle',
    'permit': 'single-use-create-permit-held-reply-exclusion-no-notifier-and-exact-custody',
    'notification_failure': 'entry-notifier-failure-retains-name-handle-drops-lookup-and-remount-repairs',
    'successor': 'created-kernel-fd-survives-captured-G-D1-and-two-explicit-Commits',
}
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_SOURCE = Path(__file__).with_name('mounted_create.rs')
driver.TEST_PREFIX = 'mounted_create_'
driver.TEST_MARKER = 'MOUNTED_CREATE_CHECK'
driver.MODE = 'functional-mounted-workspace-create'
driver.NOT_RUN = ['native capacity failure remains open', 'unprivileged later-open mode enforcement',
                  'kernel CREATE reply-send failure injection', 'unlink/rename/symlink mutation',
                  'prepared npm workload', 'R6', 'hard RSS/cgroup memory bound',
                  'crash/restart recovery', 'in-place failed-coherence repair']


def bootstrap(service_dir, port, private, public, content, wide):
    selector = argparse.ArgumentParser(add_help=False)
    selector.add_argument('--case')
    extras = ()
    if selector.parse_known_args()[0].case == 'existing':
        assert not wide
        entry = driver.shared.route.manifest_entry
        extras = (entry(0, b'directory', 2, 0o755, 1700000030, 1),
                  entry(0, b'link', 3, 0o777, 1700000031, 2, target=b'data.bin'))
    return driver.shared.bootstrap(service_dir, port, private, public, content, wide,
                                   manifest_extras=extras)


driver.BOOTSTRAP = bootstrap

if __name__ == '__main__':
    driver.main()
