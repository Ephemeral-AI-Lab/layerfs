"""The typing of the package (#35, F6): `py.typed` and the `.pyi` stubs that
tools/gen_stubs.py writes from the call table must not drift from it, and a
type checker must accept the samples in typed_samples/ok.py and report
exactly the lines of typed_samples/bad.py marked `# E`. Build first, as for
test_calls.py. The type-checker tests are skipped when mypy is not installed.
"""

import importlib.util
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
PACKAGE = HERE.parent / "src" / "layerfs"
sys.path.insert(0, str(HERE.parent / "src"))

import layerfs  # noqa: E402

spec = importlib.util.spec_from_file_location("gen_stubs", HERE.parent / "tools" / "gen_stubs.py")
gen_stubs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gen_stubs)

MYPY = importlib.util.find_spec("mypy") is not None


class Stubs(unittest.TestCase):
    def test_the_package_is_marked_typed(self):
        self.assertTrue((PACKAGE / "py.typed").is_file())

    def test_the_package_ships_the_repositorys_licence(self):
        root = HERE.parent.parent.parent / "LICENSE"
        if not root.is_file():
            self.skipTest("not inside the repository checkout")
        self.assertEqual((HERE.parent / "LICENSE").read_text(), root.read_text())
        project = (HERE.parent / "pyproject.toml").read_text()
        self.assertIn('license = "MIT"', project)
        self.assertIn('license-files = ["LICENSE"]', project)

    def test_the_committed_stubs_are_what_the_call_table_generates(self):
        for name, text in gen_stubs.render(layerfs.TABLE).items():
            self.assertEqual(
                (PACKAGE / name).read_text(),
                text,
                f"{name} drifted from the call table: run python api/python/tools/gen_stubs.py",
            )

    def test_every_call_of_the_table_is_in_the_sync_and_the_async_stub(self):
        sync = (PACKAGE / "_client.pyi").read_text()
        asynchronous = (PACKAGE / "aio.pyi").read_text()
        self.assertEqual(len(re.findall(r"^    def \w+\(", sync, re.M)), len(layerfs.TABLE) + 1)  # + __init__
        self.assertEqual(len(re.findall(r"^    async def \w+\(", asynchronous, re.M)), len(layerfs.TABLE))
        self.assertNotIn("async def", sync)

    def test_every_attribute_a_client_stub_declares_exists_on_the_client(self):
        import ast

        from layerfs import aio
        from layerfs._client import Client

        for stub, name, make in (("_client.pyi", "Client", Client), ("aio.pyi", "AsyncClient", aio.AsyncClient)):
            tree = ast.parse((PACKAGE / stub).read_text())
            declared = [
                item.target.id
                for node in tree.body
                if isinstance(node, ast.ClassDef) and node.name == name
                for item in node.body
                if isinstance(item, ast.AnnAssign)
            ]
            self.assertTrue(declared, stub)
            with tempfile.TemporaryDirectory() as home:
                client = make(home)
                for attribute in declared:
                    self.assertTrue(hasattr(client, attribute), f"{stub}: {name}.{attribute} is not on the client")

    def test_every_stub_names_what_the_package_exports(self):
        aio = __import__("layerfs.aio", fromlist=["x"])
        for module in (layerfs, aio):
            stub = (PACKAGE / ("__init__.pyi" if module is layerfs else "aio.pyi")).read_text()
            for name in module.__all__:
                self.assertIn(f"'{name}'", stub, f"{module.__name__}.{name}")


def mypy(path, *extra):
    with tempfile.TemporaryDirectory() as cache:
        run = subprocess.run(
            [
                sys.executable,
                "-m",
                "mypy",
                "--strict",
                "--python-version",
                "3.10",
                "--cache-dir",
                cache,
                *extra,
                str(path),
            ],
            capture_output=True,
            text=True,
            timeout=100,
            env={**os.environ, "MYPYPATH": str(HERE.parent / "src")},
            cwd=HERE.parent,
        )
    return run.returncode, run.stdout


@unittest.skipUnless(MYPY, "NOT_RUN: mypy is not installed")
class Checked(unittest.TestCase):
    def test_the_sample_that_uses_the_package_correctly_passes_strict_mypy(self):
        code, out = mypy(HERE / "typed_samples" / "ok.py")
        self.assertEqual(code, 0, out)

    def test_the_sample_that_misuses_it_fails_on_exactly_the_marked_lines(self):
        path = HERE / "typed_samples" / "bad.py"
        marked = {n for n, line in enumerate(path.read_text().splitlines(), 1) if "  # E:" in line}
        code, out = mypy(path)
        self.assertEqual(code, 1, out)
        reported = {int(m.group(1)) for m in re.finditer(r"bad\.py:(\d+): error", out)}
        self.assertEqual(reported, marked, out)


if __name__ == "__main__":
    unittest.main()
