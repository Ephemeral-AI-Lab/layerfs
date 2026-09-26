#!/usr/bin/env python3
"""Native symlink publication through the unchanged CPU2 fixture/runtime driver."""
from pathlib import Path
import argparse
import mkdir_route as driver

driver.CASES = {
    'semantics': 'native-opaque-empty-self-symlinks-forget-listing-and-explicit-Commit',
    'successor': 'captured-local-readlink-and-D1-file-plus-symlink-reconcile',
    'capacity': 'exact-long-name-v3-frontier-full4096-target-and-mixed-file-Commit',
    'refusals': 'native-symlink-target-name-access-kind-readonly-deadline-and-unbound-projection-refusals',
    'reserve_denied': 'symlink-denied-Reserve-no-name-handle-or-replay',
    'reserve_unknown': 'symlink-unknown-Reserve-consumed-once-no-name-or-replay',
}
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_SOURCE = Path(__file__).with_name('symlink.rs')
driver.TEST_PREFIX = 'symlink_'
driver.TEST_MARKER = 'SYMLINK_CHECK'
driver.MODE = 'functional-native-workspace-symlink'
driver.NOT_RUN = ['mounted/kernel SYMLINK', 'native create capacity failure remains open',
                  'unlink/rename', 'prepared npm workload', 'R6',
                  'hard RSS/cgroup memory bound', 'crash/restart recovery']


def bootstrap(service_dir, port, private, public, wide):
    selector = argparse.ArgumentParser(add_help=False)
    selector.add_argument('--case')
    directories = ()
    if selector.parse_known_args()[0].case == 'refusals':
        assert not wide
        directories = (('no-write', 0o500), ('no-search', 0o600))
    return driver.shared.bootstrap(service_dir, port, private, public, wide,
                                   directories=directories)


driver.BOOTSTRAP = bootstrap

if __name__ == '__main__':
    driver.main()
