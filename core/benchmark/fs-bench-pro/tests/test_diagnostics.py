"""External observer calibration and explicit non-admission route tests."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import runner
from diagnostics import run_init as diagnostic

class CauseDiagnostics(unittest.TestCase):
    def test_sqlite_observer_records_vm_work_and_both_execution_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            folder=Path(directory);lib=folder/'observer.dylib';probe=folder/'probe'
            source=runner.HERE/'diagnostics'
            subprocess.run(['clang','-Wall','-Wextra','-Werror','-dynamiclib',str(source/'sqlite_work.c'),'-lsqlite3','-o',str(lib)],check=True,capture_output=True)
            subprocess.run(['clang','-Wall','-Wextra','-Werror',str(source/'sqlite_probe.c'),'-lsqlite3','-o',str(probe)],check=True,capture_output=True)
            output=folder/'work.json'
            subprocess.run([str(probe)],check=True,capture_output=True,env={**os.environ,'DYLD_INSERT_LIBRARIES':str(lib),'LAYERFS_SQLITE_WORK_OUTPUT':str(output)})
            value=json.loads(output.read_text());self.assertEqual(value['opens'],1);self.assertEqual(value['omitted'],0)
            self.assertEqual(sum(row['calls'] for row in value['classes']),5)
            self.assertEqual(sum(row['exec_calls'] for row in value['classes']),1)
            select=next(row for row in value['classes'] if row['phase']==1)
            self.assertEqual(select['calls'],1);self.assertEqual(select['step_calls'],3);self.assertGreater(select['vm_steps'],0)
            self.assertEqual(sum(row['step_calls'] for row in value['classes'] if row['phase']==0),0)
    def test_runner_dispatch_does_not_use_speed_arm(self):
        case='phase7-init-work-100-v1'
        with patch.object(diagnostic,'run') as call,patch.object(sys,'argv',['runner.py','run','--case',case,'--arm','baseline','--out','fresh']):
            runner.main()
        call.assert_called_once_with('100','baseline','fresh')
    def test_diagnostic_is_explicit_and_preserves_bounds(self):
        source=(runner.HERE/'diagnostics/run_init.py').read_text()
        self.assertIn("'admission_eligible':False",source)
        self.assertIn("'CAUSE_DIAGNOSTIC'",source)
        self.assertIn('15_000_000_000',source);self.assertIn('9_500_000_000',source)
        self.assertIn("with claim.open('x')",source)
