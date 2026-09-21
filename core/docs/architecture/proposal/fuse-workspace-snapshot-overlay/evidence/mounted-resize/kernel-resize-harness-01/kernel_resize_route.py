#!/usr/bin/env python3
"""One actual mounted size SETATTR selection; origin is a native API subset."""
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'layerfs-workspace/tests'))
import stage_route as driver

driver.CASES = {
    'semantics': ['mounted-path-and-fd-shrink-reextend-preserve-aliases-and-incremental-Commits'],
    'open_trunc': ['existing-file-O_TRUNC-and-shell-redirection-publish-empty-at-open'],
    'same_length': ['mounted-same-length-resize-saves-mtime-without-content-upload'],
    'refused': ['unsupported-metadata-open-flags-and-readonly-ftruncate-preserve-visible-state'],
    'failed_open': ['failed-truncating-open-preserves-original-version-and-drains-projection-handle'],
    'metadata_failure': ['mounted-size-metadata-failure-preserves-version-and-retains-accounted-owner'],
    'envelope': ['mounted-zero-extension-exact-eight-MiB-bound-without-payload-allocation'],
    'native_save': ['mounted-resize-and-zero-read-progress-during-actual-C2-save'],
    'origin': ['projection-size-origin-skips-notifier-and-keeps-identity-deadline-and-reply-contract'],
}
driver.REQUIREMENTS = {
    'semantics': ['W-04', 'W-07', 'W-11', 'S-18'],
    'open_trunc': ['W-03', 'W-07', 'W-11', 'R-07'],
    'same_length': ['W-11', 'B-28'],
    'refused': ['W-03', 'W-12', 'B-01'],
    'failed_open': ['W-03', 'R-07', 'B-01', 'B-14'],
    'metadata_failure': ['W-04', 'B-20'],
    'envelope': ['W-04', 'B-09', 'B-14', 'B-28'],
    'native_save': ['W-04', 'W-11', 'S-03', 'S-11', 'S-18'],
    'origin': ['W-04', 'B-01', 'S-15'],
}
driver.TEST_SOURCE = Path(__file__).with_name('kernel_resize.rs')
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_PREFIX = 'kernel_resize_'
driver.TEST_MARKER = 'KERNEL_RESIZE_CHECK'
driver.MODE = 'functional-mounted-resize'
driver.REQUIREMENT_SCOPE = 'Actual Linux size-only SETATTR and existing-file truncating OPEN; origin is a native projection API subset'
driver.LIMIT_CASES = ('metadata_failure',)
driver.NOT_RUN = ['namespace mutations and npm', 'R6', 'hard RSS/cgroup bound',
                  'authenticated daemon edit/Commit controls',
                  'universal mmap coherence and mixed-origin cached-i_size/RWF semantics']

if __name__ == '__main__': driver.main()
