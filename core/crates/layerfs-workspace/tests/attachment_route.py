#!/usr/bin/env python3
"""Failed native Attach ownership using the closed read-only Branch/source fixture."""
from pathlib import Path
import attachment_driver as driver

driver.CASES = {
    'retained': ['retained-failure-exact-identity-expiry-and-no-replay',
                 'explicit-cleanup-preserves-unowned-mount-and-permits-new-attach'],
    'substitution': ['cleanup-refuses-replaced-backing-identity-and-retains-original-cause',
                     'restored-owned-identity-allows-explicit-cleanup'],
    'concurrent_cleanup': ['inflight-cleanup-retains-observable-owner-with-immediate-busy-admission',
                           'one-admitted-cleanup-retires-owner-after-real-kernel-removal'],
    'cleanup_deadline': ['inflight-cleanup-retains-observable-owner-with-immediate-busy-admission',
                         'deadline-after-removal-retains-original-cause-and-released-resource-progress',
                         'one-admitted-cleanup-retires-owner-after-real-kernel-removal'],
    'mount_substitution': ['owned-mount-leaf-substitution-refused-with-progress-and-original-deadline-cause',
                           'restored-owned-mount-leaf-cleans-and-releases-exact-attachment'],
    'branch_capacity': ['adapter-spare-capacity-fails-before-acquisition-without-twelve-attempt-arena-leak',
                         'later-local-edit-attach-and-clean-close-after-twelve-capacity-failures'],
}
driver.REQUIREMENTS = {case: ['R-03', 'B-20'] for case in driver.CASES}
driver.TEST_SOURCE = Path(__file__).with_name('attachment.rs')
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_PREFIX = 'attachment_'
driver.TEST_MARKER = 'ATTACHMENT_CHECK'
driver.MODE = 'functional-native-attachment-ownership'
driver.REQUIREMENT_SCOPE = ('native failed-Attach ownership prerequisite; no authenticated remote Attach claim; '
                            'branch_capacity is an external adapter allocation contract subset')
driver.NOT_RUN = ['authenticated remote Attach', 'writable daemon startup', 'mounted writes',
                  'npm payload replay', 'R6', 'hard RSS/cgroup bound',
                  'oversized branch metadata over native wire: branch_capacity changes external adapter spare capacity only',
                  'hostile concurrent managed-root rename between identity check and unlink']
driver.OBSERVATION_MARKERS = ('ATTACHMENT_NATIVE_FAILURE ', 'ATTACHMENT_RESOURCE ')

if __name__ == '__main__':
    driver.main()
