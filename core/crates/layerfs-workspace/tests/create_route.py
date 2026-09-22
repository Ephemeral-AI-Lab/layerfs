#!/usr/bin/env python3
"""Native atomic create/open proof; existing CPU2/ownership/runtime harness."""
from pathlib import Path
import argparse
import mkdir_route as driver

driver.CASES = {
    'semantics': 'atomic-create-handle-forget-relookup-zero-save-and-existing-next-Commit',
    'flags_permissions': 'creation-fd-rights-mode0400-mode000-and-existing-search-only-parent',
    'successor': 'captured-fresh-file-D1-and-D1-born-file-reconcile-both-saved-roots',
    'capacity': 'exact-fresh-file-wire-frontier-and-eight-MiB-envelope-refuse-atomically',
    'refusals': 'native-create-refusals-preserve-namespace-handles-and-reservations',
    'reserve_denied': 'create-denied-Reserve-no-handle-name-or-replay',
    'reserve_unknown': 'create-unknown-Reserve-consumed-once-no-handle-name-or-replay',
}
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_SOURCE = Path(__file__).with_name('create.rs')
driver.TEST_PREFIX = 'create_'
driver.TEST_MARKER = 'CREATE_CHECK'
driver.MODE = 'functional-native-workspace-create'
driver.NOT_RUN = ['mounted/kernel CREATE', 'native handle-based resize API', 'kernel fd resize on newly created files',
                  'unlink/rename/symlink mutation', 'prepared npm workload', 'R6',
                  'hard RSS/cgroup memory bound', 'crash/restart recovery']


def bootstrap(service_dir, port, private, public, content, wide):
    selector = argparse.ArgumentParser(add_help=False)
    selector.add_argument('--case')
    case = selector.parse_known_args()[0].case
    extras = ()
    if case in ('flags_permissions', 'refusals'):
        assert not wide
        entry = driver.shared.route.manifest_entry
        extras = (entry(0, b'search-only', 2, 0o500, 1700000030, 1),
                  entry(3, b'existing', 1, 0o600, 1700000031, 2, content),
                  entry(0, b'link', 3, 0o777, 1700000032, 3, target=b'data.bin'))
    return driver.shared.bootstrap(service_dir, port, private, public, content, wide, manifest_extras=extras)


driver.BOOTSTRAP = bootstrap

if __name__ == '__main__':
    driver.main()
