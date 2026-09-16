from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from check_product_boundary import production_files, violations


class ProductBoundaryTests(unittest.TestCase):
    def test_product_attributes(self):
        for source in (
            "#[test]\nfn case() {}",
            "#[tokio::test]\nasync fn case() {}",
            "#[cfg(any(unix,\n test))]\nmod checks;",
            '#[cfg(feature = "test-instrumentation")]\nfn inject() {}',
            '#[cfg_attr(feature = "checks", test)]\nfn case() {}',
            'fn f() { if cfg!(any(all(unix), test)) {} }',
            'fn f() { if cfg!(any(\n all(unix),\n test\n)) {} }',
        ):
            with self.subTest(source=source):
                self.assertTrue(violations(Path("scope.rs"), source))
        self.assertFalse(violations(Path("scope.rs"), '#[cfg(unix)]\nfn f() { assert!(true); }'))

    def test_entry_files(self):
        for name in ("lib.rs", "mod.rs"):
            path = Path(name)
            self.assertFalse(violations(path, "// documentation\n" * 200))
            self.assertTrue(violations(path, "// documentation\n" * 201))
            self.assertFalse(violations(path, 'mod scope;\npub use scope::Timing;\npub fn run() { scope::run() }'))
            for source in ('pub struct State;', 'impl State {}', 'fn f() { let value = 1; }', 'fn f() { if true {} }'):
                self.assertTrue(violations(path, source))
        self.assertFalse(violations(Path("recording.rs"), "pub struct State;\nimpl State {}"))

    def test_executable_rustdoc(self):
        self.assertTrue(violations(Path("report.rs"), '/// ```rust\n/// let x = 1;\n/// ```'))

    def test_production_file_size_boundary(self):
        for name in ("save.rs", "schema.sql"):
            with self.subTest(name=name):
                source = "\n" * 998 + "final line without newline"
                self.assertFalse(violations(Path(name), source))
                self.assertEqual(violations(Path(name), source + "\nextra")[0][0], 1000)

    def test_shipped_sql_and_external_tests_scope(self):
        with TemporaryDirectory() as directory:
            core = Path(directory)
            names = ("src/lib.rs", "src/sql/query.sql", "sql/schema.sql", "tests/case.rs",
                     "tests/fixture.sql", "examples/demo.rs", "benches/save.rs")
            for name in names:
                path = core / "crates" / "store" / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("")
            self.assertEqual([str(p.relative_to(core / "crates" / "store"))
                              for p in production_files(core)],
                             ["sql/schema.sql", "src/lib.rs", "src/sql/query.sql"])


if __name__ == "__main__":
    unittest.main()
