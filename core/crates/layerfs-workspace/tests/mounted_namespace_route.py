#!/usr/bin/env python3
"""Two mounted kernel namespace/attribute selections through the native driver."""
from pathlib import Path
import namespace_route as driver

driver.CASES = {
    'kernel': 'actual-kernel-mknod-link-unlink-rmdir-rename-setattr-errno-identity-nlink-listings',
    'durability': 'mounted-tree-after-explicit-Commit-fresh-read-names-inodes-contents-nlink-metadata',
}
driver.TESTS = {case: f'mounted_namespace_{case}' for case in driver.CASES}
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_SOURCE = Path(__file__).with_name('mounted_namespace.rs')
driver.MODE = 'functional-mounted-workspace-namespace'
driver.NOT_RUN = [
    'symlink operations (the mounted_symlink route covers them)',
    'rename exchange/whiteout flags',
    'cross-Workspace operations',
    'committed-directory move refusal (bounded profile: Unsupported, no identity-keyed '
    'service query to re-anchor a committed directory\'s children after a move)',
    'prepared npm workload', 'R6', 'hard RSS/cgroup memory bound', 'crash/restart recovery',
]

# The shared argparse/report/marker machinery still reads the driver module that
# namespace_route itself specialises; keep both module views aligned.
base = driver.driver
base.CASES = driver.CASES
base.ENTRY_SOURCE = driver.ENTRY_SOURCE
base.TEST_SOURCE = driver.TEST_SOURCE
base.TEST_PREFIX = 'mounted_namespace_'
base.TEST_MARKER = 'MOUNTED_NAMESPACE_CHECK'
base.MODE = driver.MODE
base.NOT_RUN = driver.NOT_RUN

if __name__ == '__main__':
    base.main()
