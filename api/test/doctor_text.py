"""Unix CLI regression. Run with python3 api/test/doctor_text.py PATH_TO_LAYERFS."""
import json
import os
from pathlib import Path
import pty
import re
import subprocess
import sys
import tempfile
import unittest


BINARY = str(Path(sys.argv.pop(1)).resolve())
ANSI = re.compile(r'\x1b\[[0-9;]+m')


class DoctorText(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        home = Path(temp.name) / 'home'
        self.env = dict(os.environ, LAYERFS_HOME=str(home))
        self.env.pop('NO_COLOR', None)
        self.cli('home', 'init')
        store = home / 'store/shop'
        project = store / 'project/api'
        project.mkdir(parents=True)
        (store / 'record.json').write_text('{broken')
        (project / 'record.json').write_text('{}')

    def cli(self, *args):
        return subprocess.check_output([BINARY, *args], env=self.env, text=True, timeout=30)

    def terminal(self, *args):
        master, slave = pty.openpty()
        with os.fdopen(master, 'rb', buffering=0) as output, os.fdopen(slave, 'wb') as tty:
            subprocess.run([BINARY, 'store', 'doctor', 'shop', *args], stdout=tty,
                           env=self.env, check=True, timeout=30)
            # The short report fits in the PTY buffer. Keep the slave open while reading.
            return output.read(8192).decode().replace('\r\n', '\n')

    def test_columns(self):
        summary, finding, blocked = self.cli('store', 'doctor', 'shop', '--text').splitlines()
        self.assertIn('1 not checked', summary)
        self.assertEqual(finding.index('store shop'), blocked.index('project shop/api'))

    def test_detail(self):
        args = ('store', 'doctor', 'shop', '--detail')
        report = json.loads(self.cli(*args))
        text = self.cli(*args, '--text')
        for key in ('observed_at', 'status', 'counts', 'tree', 'evidence'):
            self.assertIn(key + ':', text)
        self.assertIn(json.dumps(report['counts'], separators=(',', ':')), text)
        self.assertIn('evidence: null', text)
        for node in [report['tree'], *report['tree']['projects']]:
            self.assertIn('coverage: ' + node['coverage'], text)
            for row in node['checks']:
                self.assertIn(json.dumps(row, separators=(',', ':')), text)

    def test_terminal(self):
        plain = self.cli('store', 'doctor', 'shop', '--text')
        styled = self.terminal()
        for line in styled.splitlines():
            self.assertRegex(line, ANSI)
        self.assertEqual(ANSI.sub('', styled), plain)
        self.assertNotRegex(plain, ANSI)
        self.env['NO_COLOR'] = ''
        self.assertEqual(self.terminal(), plain)


if __name__ == '__main__':
    unittest.main()
