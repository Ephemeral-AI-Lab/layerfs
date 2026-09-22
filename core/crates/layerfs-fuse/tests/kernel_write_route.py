#!/usr/bin/env python3
"""One real mounted existing-file WRITE selection; origin is a declared API subset."""
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'layerfs-workspace/tests'))
import stage_route as driver

driver.CASES = {
    'positional': ['mounted-overwrite-gap-alias-and-A-B-A-incremental-Commits'],
    'append': ['mounted-concurrent-append-preserves-live-EOF-and-fd-positions'],
    'read_race': ['mounted-old-read-refuses-new-WRITE-before-ingress-and-keeps-read-progress'],
    'mappings': ['direct-profile-refuses-shared-mmap-and-refreshes-private-mapping-and-sendfile'],
    'exec': ['direct-profile-executes-ELF-replaced-through-kernel-WRITE'],
    'native_save': ['mounted-WRITE-and-read-progress-during-actual-C2-save'],
    'ingress': ['mounted-input-is-owned-before-return-without-large-base-copy-up'],
    'quota': ['mounted-quota-refusal-preserves-bytes-size-mtime-and-cleanup'],
    'backing_failure': ['mounted-backing-failure-retains-accepted-prefix-and-accounted-failed-owner'],
    'origin': ['projection-write-origin-reply-slot-append-offset-and-single-attempt-contract'],
    'completion_failure': ['projection-write-known-publication-failure-is-reported-by-alias-flush'],
}
driver.REQUIREMENTS = {case: ['W-01', 'W-07', 'W-12', 'B-01', 'S-15', 'S-18'] for case in driver.CASES}
driver.REQUIREMENTS['native_save'] += ['S-03', 'S-11']
driver.REQUIREMENTS['append'] += ['W-02']
driver.REQUIREMENTS['positional'] += ['W-04', 'W-11']
driver.REQUIREMENTS['mappings'] += ['W-13']
driver.REQUIREMENTS['ingress'] += ['B-09', 'B-10']
driver.REQUIREMENTS['quota'] += ['B-14', 'B-20']
driver.REQUIREMENTS['backing_failure'] += ['B-20']
driver.TEST_SOURCE = Path(__file__).with_name('kernel_write.rs')
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_PREFIX = 'kernel_write_'
driver.TEST_MARKER = 'KERNEL_WRITE_CHECK'
driver.MODE = 'functional-mounted-write'
driver.REQUIREMENT_SCOPE = 'Actual Linux direct-I/O existing-file WRITE; origin/completion_failure are native projection API subsets'
driver.LIMIT_CASES = ('backing_failure',)
driver.DATA_MODES = {'exec': 0o755}
driver.NOT_RUN = ['size SETATTR/truncating open outside this WRITE-only selection', 'namespace mutations and npm',
                  'R6', 'hard RSS/cgroup bound', 'authenticated daemon edit/Commit controls']

if __name__ == '__main__': driver.main()
