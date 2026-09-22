#!/usr/bin/env python3
"""Two fresh-stream prerequisites through the unchanged CPU2 native driver."""
from pathlib import Path
import mkdir_route as driver

driver.CASES = {
    'mounted_dsh': 'pinned-DSH-mounted-upload-full-construction-then-small-canonical-edit',
    'captured_replay': 'captured-fresh-G-tail-survives-exact-replay-limit-before-and-after-rebase',
}
driver.ENTRY_SOURCE = Path(__file__)
driver.TEST_SOURCE = Path(__file__).with_name('fresh_stream.rs')
driver.TEST_PREFIX = 'fresh_stream_'
driver.TEST_MARKER = 'FRESH_STREAM_CHECK'
driver.MODE = 'functional-workspace-fresh-stream-prerequisite'
driver.NOT_RUN = ['original native-create capacity failure remains open; no rerun or reclassification',
                  'full prepared DSH tree upload or npm installation', '32-bit target',
                  'performance/cold-cache/RSS claims', '4GiB or 1024-piece boundary workload',
                  'crash/restart recovery']

if __name__ == '__main__':
    driver.main()
