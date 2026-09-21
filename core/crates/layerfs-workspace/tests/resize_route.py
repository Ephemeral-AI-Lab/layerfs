#!/usr/bin/env python3
"""Existing-file resize, zero ranges and Commit through the native Workspace route."""
from pathlib import Path
import stage_route as driver

driver.CASES = {
    'semantics': ['shrink-reextend-keeps-zero-tail-alias-handles-and-old-reply'],
    'envelope': ['zero-input-exact-eight-MiB-bound-without-payload-allocation'],
    'successor': ['frozen-zero-range-and-live-shrink-reextend-use-exact-next-base'],
    'overwritten_zero': ['live-overwrite-of-frozen-zero-saves-only-new-bytes'],
    'metadata_only': ['same-length-resize-saves-metadata-without-file-input'],
    'refusals': ['resize-kind-identity-deadline-length-and-quota-refusal-atomicity'],
    'metadata_failure': ['native-resize-metadata-failure-retains-original-visible-version'],
    'native_save': ['resize-progress-during-actual-save-preserves-frozen-and-successor-zero-tails'],
    'frontier': ['all-104-inode-shrinks-save-once-with-hardlink-sharing'],
}
driver.REQUIREMENTS = {
    'semantics': ['W-03', 'W-04', 'B-09', 'B-10'],
    'envelope': ['B-04', 'B-05', 'B-09', 'B-14', 'B-28'],
    'successor': ['S-03', 'S-18', 'H-03'], 'overwritten_zero': ['S-18', 'B-28'],
    'metadata_only': ['B-28'], 'refusals': ['W-12', 'B-01', 'B-15'],
    'metadata_failure': ['S-15', 'B-20'], 'native_save': ['S-11', 'S-18'],
    'frontier': ['B-21', 'B-28'],
}
driver.TEST_SOURCE = Path(__file__).with_name('resize.rs')
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_PREFIX = 'resize_'
driver.TEST_MARKER = 'RESIZE_CHECK'
driver.MODE = 'functional-workspace-resize'
driver.REQUIREMENT_SCOPE = 'Existing-inode local resize SDK subsets, not writable mounted callbacks or npm/R6'
driver.LIMIT_CASES = ('metadata_failure',)
driver.NOT_RUN = ['writable open/handle flags and atomic append', 'mounted writes/truncate/extend',
                  'namespace mutations and npm', 'R6', 'hard RSS/cgroup bound',
                  'explicit failed-state/DiscardStage disposition', 'full capture/prepared-mutation race schedules']

if __name__ == '__main__': driver.main()
