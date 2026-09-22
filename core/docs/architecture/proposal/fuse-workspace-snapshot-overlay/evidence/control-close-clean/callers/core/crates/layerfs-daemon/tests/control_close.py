#!/usr/bin/env python3
"""Actual authenticated CloseClean through the existing daemon lifecycle harness."""
from pathlib import Path
import control_unmount as driver

driver.MODE = 'functional-authenticated-close-clean'
driver.ENTRY_SOURCE = Path(__file__)
driver.CONTROL_PEERS = [(1,'good',7),(2,'old_lifecycle',3),(3,'close',4),(4,'none',0),(5,'expiry',7)]
driver.CASES = {
    'close_success': ['CloseClean-frees-clean-unmounted-target-and-retains-closed-Status-identity'],
    'close_mounted': ['CloseClean-refuses-mounted-held-target-without-stopping-presentation'],
    'close_authority': ['CloseClean-grant-target-incarnation-and-service-authority-are-independent'],
    'close_cleanup_failure': ['failed-leaf-removal-retains-Workspace-until-explicit-checked-cleanup'],
    'close_loss': ['lost-CloseClean-terminal-is-Unknown-without-replay-and-later-signal-is-clean'],
}
driver.NOT_RUN = ['remote Attach/Mount and writable daemon startup/edit/Commit controls',
                  'dirty/staged CloseClean over the currently read-only daemon profile',
                  'deterministic signal overlap inside actual CloseClean I/O',
                  'namespace/npm/R6', 'hard RSS/cgroup qualification']

if __name__ == '__main__': driver.main()
