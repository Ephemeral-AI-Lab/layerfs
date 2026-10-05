#!/usr/bin/env python3
"""Mounted SDK coherence and separately scoped completion-binding proofs."""
from pathlib import Path
import stage_route as driver

driver.CASES = {
    'visibility': ['same-mounted-FDs-and-aliases-see-SDK-bytes-size-mtime-and-repeated-Commit'],
    'read_race': ['actual-old-kernel-read-excludes-publication-and-fresh-read-observes-acknowledged-write'],
    'exec': ['mounted-ELF-and-replacement-script-execute-after-checked-SDK-invalidation'],
    'native_save': ['mounted-read-sees-live-SDK-write-while-actual-C2-save-retains-G'],
    'completion': ['pending-notification-allows-new-view-and-Commit-but-excludes-next-publication'],
    'deadline': ['post-publication-deadline-preserves-receipt-and-retained-completed-failure'],
    'notify_failure': ['real-notifier-send-failure-preserves-published-write-and-allows-checked-detach'],
    'truncate_failure': ['real-notifier-failure-exposes-READY-truncating-handle-and-preserves-empty-file'],
}
driver.REQUIREMENTS = {case: ['W-01', 'W-12', 'B-01', 'S-15', 'S-18'] for case in driver.CASES}
driver.REQUIREMENTS['native_save'] += ['S-03', 'S-11']
driver.TEST_SOURCE = Path(__file__).with_name('coherence.rs')
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_PREFIX = 'coherence_'
driver.TEST_MARKER = 'COHERENCE_CHECK'
driver.MODE = 'functional-mounted-sdk-coherence'
driver.REQUIREMENT_SCOPE = 'Actual RO kernel projection with SDK mutations; completion/deadline are native binding API subsets without kernel timing claims'
driver.LIMIT_CASES = ()
driver.DATA_MODES = {'exec': 0o755}
driver.NOT_RUN = ['kernel WRITE/SETATTR and append fd-position semantics', 'namespace mutations and npm',
                  'R6', 'hard RSS/cgroup bound', 'authenticated daemon edit/Commit controls']

if __name__ == '__main__': driver.main()
